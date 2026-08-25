//! Bounded ECIES-X25519-AEAD-Ratchet destination session manager.
//!
//! Plan 126 replaces the Plan 121 i2pr-internal ECIES dialect with
//! the normative I2P ECIES-X25519-AEAD-Ratchet contract. The manager
//! owns one paired ECIES session per remote static key per local
//! destination:
//!
//! - the outbound tag set (local -> remote),
//! - the inbound tag window (remote -> local, bounded look-ahead,
//!   remove-on-hit replay rejection),
//! - the pending New Session Reply window while an outbound
//!   handshake is in flight, and
//! - the provisional responder state while an inbound bound New
//!   Session awaits its reply.
//!
//! The manager is the single producer of bound New Session /
//! New Session Reply / Existing Session messages. Unbound New
//! Sessions are structurally rejected by the primitive.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, VecDeque};

use i2pr_crypto::{
    BoundNewSessionMessage, BoundNewSessionSender, EciesError, EciesTagSet, EciesTagSetEntry,
    ExistingSessionMessage, NewSessionReplyMessage, NewSessionResponder, REPRESENTATIVE_LENGTH,
    SESSION_TAG_LENGTH, X25519_KEY_LENGTH, open_bound_new_session, open_existing_session,
    open_new_session_reply, seal_bound_new_session, seal_existing_session, seal_new_session_reply,
};
use i2pr_proto::{EciesPayloadBlock, EciesPayloadSequence, GarlicCloveBlock, GarlicDelivery};

use crate::identity::DestinationId;

/// Hard ceiling on the number of paired sessions per local
/// destination.
pub const MAX_PEERS_PER_LOCAL_DESTINATION: usize = 64;
/// Hard ceiling on the number of pending New Session handshakes per
/// local destination.
pub const MAX_PENDING_NEW_SESSIONS: usize = 64;
/// Hard ceiling on the number of retained session-tag look-ahead
/// entries per inbound session window.
pub const MAX_TAG_LOOK_AHEAD: usize = 32;
/// Hard ceiling on the maximum retained session lifetime in seconds.
pub const MAX_SESSION_IDLE_SECONDS: u32 = 1800;
/// Default retained-session lifetime in seconds.
pub const DEFAULT_SESSION_IDLE_SECONDS: u32 = 600;

/// Configuration for [`EciesSessionManager`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EciesSessionConfig {
    /// Maximum paired sessions tracked per local destination.
    max_peers: u16,
    /// Maximum pending outbound New Session handshakes per local
    /// destination.
    max_pending_handshakes: u16,
    /// Maximum inbound tag look-ahead entries per session window.
    max_tag_look_ahead: u16,
    /// Idle/lifetime session cap in seconds.
    idle_seconds: u32,
}

impl EciesSessionConfig {
    /// Builds a configuration after applying every ceiling.
    pub const fn try_new(
        max_peers: u16,
        max_pending_handshakes: u16,
        max_tag_look_ahead: u16,
        idle_seconds: u32,
    ) -> Result<Self, EciesSessionConfigError> {
        if max_peers == 0 {
            return Err(EciesSessionConfigError::ZeroPeers);
        }
        if (max_peers as usize) > MAX_PEERS_PER_LOCAL_DESTINATION {
            return Err(EciesSessionConfigError::PeersExceedsMaximum {
                actual: max_peers,
                maximum: MAX_PEERS_PER_LOCAL_DESTINATION as u16,
            });
        }
        if max_pending_handshakes == 0 {
            return Err(EciesSessionConfigError::ZeroPendingHandshakes);
        }
        if (max_pending_handshakes as usize) > MAX_PENDING_NEW_SESSIONS {
            return Err(EciesSessionConfigError::PendingExceedsMaximum {
                actual: max_pending_handshakes,
                maximum: MAX_PENDING_NEW_SESSIONS as u16,
            });
        }
        if max_tag_look_ahead == 0 {
            return Err(EciesSessionConfigError::ZeroTagLookAhead);
        }
        if (max_tag_look_ahead as usize) > MAX_TAG_LOOK_AHEAD {
            return Err(EciesSessionConfigError::TagLookAheadExceedsMaximum {
                actual: max_tag_look_ahead,
                maximum: MAX_TAG_LOOK_AHEAD as u16,
            });
        }
        if idle_seconds == 0 {
            return Err(EciesSessionConfigError::ZeroIdleSeconds);
        }
        if idle_seconds > MAX_SESSION_IDLE_SECONDS {
            return Err(EciesSessionConfigError::IdleExceedsMaximum {
                actual: idle_seconds,
                maximum: MAX_SESSION_IDLE_SECONDS,
            });
        }
        Ok(Self {
            max_peers,
            max_pending_handshakes,
            max_tag_look_ahead,
            idle_seconds,
        })
    }

    /// Returns a balanced experimental default.
    pub fn balanced() -> Self {
        Self::try_new(8, 8, 8, DEFAULT_SESSION_IDLE_SECONDS)
            .expect("balanced ECIES session config is within every ceiling")
    }

    /// Returns the maximum pending-handshake ceiling.
    pub const fn max_pending_handshakes(&self) -> usize {
        self.max_pending_handshakes as usize
    }

    /// Returns the inbound tag look-ahead ceiling.
    pub const fn max_tag_look_ahead(&self) -> usize {
        self.max_tag_look_ahead as usize
    }

    /// Returns the idle lifetime ceiling.
    pub const fn idle_seconds(&self) -> u32 {
        self.idle_seconds
    }
}

impl Default for EciesSessionConfig {
    fn default() -> Self {
        Self::balanced()
    }
}

/// Typed configuration validation failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum EciesSessionConfigError {
    /// The peer ceiling was zero.
    #[error("ECIES peers per local destination must be nonzero")]
    ZeroPeers,
    /// The peer ceiling exceeded the local bound.
    #[error("ECIES peers {actual} exceeds maximum {maximum}")]
    PeersExceedsMaximum {
        /// Actual value.
        actual: u16,
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The pending-handshake ceiling was zero.
    #[error("ECIES pending handshakes must be nonzero")]
    ZeroPendingHandshakes,
    /// The pending-handshake ceiling exceeded the local bound.
    #[error("ECIES pending handshakes {actual} exceeds maximum {maximum}")]
    PendingExceedsMaximum {
        /// Actual value.
        actual: u16,
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The tag-look-ahead ceiling was zero.
    #[error("ECIES tag look-ahead must be nonzero")]
    ZeroTagLookAhead,
    /// The tag-look-ahead ceiling exceeded the local bound.
    #[error("ECIES tag look-ahead {actual} exceeds maximum {maximum}")]
    TagLookAheadExceedsMaximum {
        /// Actual value.
        actual: u16,
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The idle-seconds lifetime was zero.
    #[error("ECIES session idle seconds must be nonzero")]
    ZeroIdleSeconds,
    /// The idle-seconds lifetime exceeded the local bound.
    #[error("ECIES session idle seconds {actual} exceeds maximum {maximum}")]
    IdleExceedsMaximum {
        /// Actual value.
        actual: u32,
        /// Accepted ceiling.
        maximum: u32,
    },
}

/// The canonical 64-hex SHA-256 router-hash key type for session
/// ownership: the remote endpoint's X25519 static public key bytes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct RemoteStaticKey([u8; X25519_KEY_LENGTH]);

impl RemoteStaticKey {
    fn from_public(public: &[u8; X25519_KEY_LENGTH]) -> Self {
        Self(*public)
    }

    fn public(&self) -> &[u8; X25519_KEY_LENGTH] {
        &self.0
    }
}

/// Bounded inbound tag window over one tag set: a fixed look-ahead
/// of pre-derived tags with remove-on-hit replay rejection.
#[derive(Debug)]
struct InboundTagWindow {
    tag_set: EciesTagSet,
    entries: BTreeMap<[u8; SESSION_TAG_LENGTH], u32>,
    capacity: usize,
}

impl InboundTagWindow {
    fn new(tag_set: EciesTagSet, capacity: usize) -> Result<Self, EciesError> {
        let mut window = Self {
            tag_set,
            entries: BTreeMap::new(),
            capacity,
        };
        window.refill()?;
        Ok(window)
    }

    fn refill(&mut self) -> Result<(), EciesError> {
        while self.entries.len() < self.capacity {
            let EciesTagSetEntry { index, tag } = self.tag_set.next_entry()?;
            self.entries.insert(tag, index);
        }
        Ok(())
    }

    fn contains(&self, tag: &[u8; SESSION_TAG_LENGTH]) -> bool {
        self.entries.contains_key(tag)
    }

    fn consume(&mut self, tag: &[u8; SESSION_TAG_LENGTH]) -> Option<u32> {
        let index = self.entries.remove(tag)?;
        if self.refill().is_err() {
            // A exhausted tag set leaves the window short but never
            // panics; subsequent traffic fails authentication typed.
        }
        Some(index)
    }
}

/// One paired ECIES session keyed by the remote static public key.
#[derive(Debug)]
struct PairedSession {
    /// Outbound tag set (local destination -> remote).
    outbound: EciesTagSet,
    /// Inbound tag window (remote -> local destination).
    inbound: InboundTagWindow,
    last_used_seconds: u32,
}

/// An outbound bound New Session awaiting its reply.
#[derive(Debug)]
struct PendingInitiated {
    sender: BoundNewSessionSender,
    created_seconds: u32,
}

/// An accepted inbound bound New Session awaiting reply sealing.
#[derive(Debug)]
struct ProvisionalResponder {
    responder: NewSessionResponder,
    last_used_seconds: u32,
}

/// The local-destination scoped session manager.
#[derive(Debug)]
pub struct EciesSessionManager {
    config: EciesSessionConfig,
    sessions: BTreeMap<RemoteStaticKey, PairedSession>,
    /// Pending outbound handshakes; tombstoned slots are reusable.
    pending_initiated: Vec<Option<PendingInitiated>>,
    /// Reply-window tags -> pending slot index, pre-derived at
    /// handshake creation so inbound replies classify in O(log n).
    pending_reply_tags: BTreeMap<[u8; SESSION_TAG_LENGTH], usize>,
    provisional_responders: BTreeMap<RemoteStaticKey, ProvisionalResponder>,
    seen_new_session_ephemerals: VecDeque<[u8; REPRESENTATIVE_LENGTH]>,
}

impl EciesSessionManager {
    /// Constructs an empty session manager.
    pub fn new(config: EciesSessionConfig) -> Self {
        Self {
            config,
            sessions: BTreeMap::new(),
            pending_initiated: Vec::new(),
            pending_reply_tags: BTreeMap::new(),
            provisional_responders: BTreeMap::new(),
            seen_new_session_ephemerals: VecDeque::new(),
        }
    }

    /// Returns the manager configuration.
    pub const fn config(&self) -> EciesSessionConfig {
        self.config
    }

    /// Returns the number of established paired sessions.
    pub fn established_sessions(&self) -> usize {
        self.sessions.len()
    }

    /// Returns the number of pending outbound New Session
    /// handshakes.
    pub fn pending_handshake_count(&self) -> usize {
        self.pending_initiated.iter().flatten().count()
    }

    /// Returns the number of provisional inbound responders.
    pub fn provisional_responder_count(&self) -> usize {
        self.provisional_responders.len()
    }

    /// Encrypts a Garlic Clove payload destined for the remote
    /// endpoint identified by `remote_hash` whose X25519 static
    /// public key is `remote_static_public`. Reuses the paired
    /// outbound session when one exists; otherwise allocates a
    /// fresh bound New Session handshake rooted at the local
    /// destination's own static key.
    #[allow(clippy::too_many_arguments)]
    pub fn encrypt_to_remote<R: rand_core::TryCryptoRng + ?Sized>(
        &mut self,
        local_id: DestinationId,
        local_static_secret: &[u8; X25519_KEY_LENGTH],
        remote_hash: &[u8; 32],
        remote_static_public: &[u8; X25519_KEY_LENGTH],
        payload: &[u8],
        now_seconds: u32,
        rng: &mut R,
    ) -> Result<EciesOutboundMessage, EciesSessionError> {
        let _ = local_id;
        let _ = remote_hash;
        let remote_key = RemoteStaticKey::from_public(remote_static_public);
        if let Some(session) = self.sessions.get_mut(&remote_key) {
            if now_seconds.saturating_sub(session.last_used_seconds) <= self.config.idle_seconds {
                session.last_used_seconds = now_seconds;
                let message = seal_existing_session(&mut session.outbound, payload)?;
                return Ok(EciesOutboundMessage::Existing(message));
            }
            // The paired session expired; drop it and re-initiate.
            self.sessions.remove(&remote_key);
        }

        if self.pending_handshake_count() >= self.config.max_pending_handshakes as usize {
            return Err(EciesSessionError::PendingHandshakeCapacity {
                maximum: self.config.max_pending_handshakes,
            });
        }
        if self.sessions.len().saturating_add(1) > self.config.max_peers as usize {
            return Err(EciesSessionError::PeerCapacity {
                maximum: self.config.max_peers,
            });
        }

        let ephemeral_keypair = i2pr_crypto::EciesEphemeralKeypair::generate(rng)?;
        let (message, sender) = seal_bound_new_session(
            local_static_secret,
            &ephemeral_keypair,
            remote_static_public,
            payload,
        )?;
        self.install_pending(sender, now_seconds)?;
        Ok(EciesOutboundMessage::NewSession { message })
    }

    /// Pre-derives the reply-window tags for a freshly created
    /// outbound handshake and installs it in the first free slot.
    fn install_pending(
        &mut self,
        sender: BoundNewSessionSender,
        created_seconds: u32,
    ) -> Result<(), EciesSessionError> {
        let slot = match self.pending_initiated.iter().position(Option::is_none) {
            Some(free) => free,
            None => {
                self.pending_initiated.push(None);
                self.pending_initiated.len() - 1
            }
        };
        // `reply_tag_set()` returns an already-ratcheted window;
        // calling `begin_tag_ratchet` again would re-initialize it.
        let mut window = sender.reply_tag_set()?;
        for _ in 0..self.config.max_tag_look_ahead {
            let entry = window.next_entry()?;
            if self.pending_reply_tags.insert(entry.tag, slot).is_some() {
                return Err(EciesSessionError::Protocol(
                    "reply-window tag collision across pending handshakes",
                ));
            }
        }
        self.pending_initiated[slot] = Some(PendingInitiated {
            sender,
            created_seconds,
        });
        Ok(())
    }

    /// Classifies one inbound encrypted envelope by structure and
    /// tag membership without consuming any state. The caller feeds
    /// the classified variant to the matching `accept_*` method,
    /// which performs the stateful decryption.
    pub fn classify(&self, envelope_bytes: &[u8]) -> ClassifiedInbound {
        let len = envelope_bytes.len();
        if len < 8 + 16 {
            return ClassifiedInbound::Unknown(ClassifiedUnknown::TooShort {
                actual: len,
                minimum: 8 + 16,
            });
        }
        let mut tag = [0_u8; SESSION_TAG_LENGTH];
        tag.copy_from_slice(&envelope_bytes[..SESSION_TAG_LENGTH]);
        if len >= 72 && self.pending_reply_tags.contains_key(&tag) {
            return ClassifiedInbound::NewSessionReply;
        }
        if self
            .sessions
            .values()
            .any(|session| session.inbound.contains(&tag))
        {
            return ClassifiedInbound::ExistingSession;
        }
        if len >= 96 + 16 {
            return ClassifiedInbound::CandidateNewSession;
        }
        ClassifiedInbound::Unknown(ClassifiedUnknown::UnmatchedTag)
    }

    /// Decrypts an inbound bound New Session message, installs the
    /// provisional responder state, and returns the decrypted
    /// Garlic Clove payload together with the sender's static
    /// public key (the session identity).
    pub fn accept_new_session(
        &mut self,
        local_id: DestinationId,
        local_static_secret: &[u8; X25519_KEY_LENGTH],
        local_static_public: &[u8; X25519_KEY_LENGTH],
        message: &BoundNewSessionMessage,
        now_seconds: u32,
    ) -> Result<AcceptedNewSession, EciesSessionError> {
        let _ = local_id;
        if self
            .seen_new_session_ephemerals
            .iter()
            .any(|seen| *seen == *message.representative.as_bytes())
        {
            return Err(EciesSessionError::DuplicateNewSession);
        }
        let opened = open_bound_new_session(local_static_secret, local_static_public, message)?;
        self.remember_new_session_ephemeral(message.representative.as_bytes());
        let alice_static_public = opened.responder.alice_static_public;
        if !self
            .provisional_responders
            .contains_key(&RemoteStaticKey::from_public(&alice_static_public))
            && self.provisional_responders.len() >= self.config.max_pending_handshakes as usize
        {
            return Err(EciesSessionError::PendingHandshakeCapacity {
                maximum: self.config.max_pending_handshakes,
            });
        }
        self.provisional_responders.insert(
            RemoteStaticKey::from_public(&alice_static_public),
            ProvisionalResponder {
                responder: opened.responder,
                last_used_seconds: now_seconds,
            },
        );
        Ok(AcceptedNewSession {
            payload: opened.payload,
            alice_static_public,
        })
    }

    /// Seals the New Session Reply for the provisional responder
    /// installed under `alice_static_public`, promotes the paired
    /// session, and returns the wire message to transmit.
    pub fn seal_new_session_reply_for<R: rand_core::TryCryptoRng + ?Sized>(
        &mut self,
        local_id: DestinationId,
        local_static_secret: &[u8; X25519_KEY_LENGTH],
        alice_static_public: &[u8; X25519_KEY_LENGTH],
        payload: &[u8],
        now_seconds: u32,
        rng: &mut R,
    ) -> Result<NewSessionReplyOutbound, EciesSessionError> {
        let _ = local_id;
        let remote_key = RemoteStaticKey::from_public(alice_static_public);
        if self.sessions.contains_key(&remote_key)
            && self.sessions.len() >= self.config.max_peers as usize
        {
            return Err(EciesSessionError::PeerCapacity {
                maximum: self.config.max_peers,
            });
        }
        let provisional = self
            .provisional_responders
            .remove(&remote_key)
            .ok_or(EciesSessionError::NoProvisionalResponder)?;
        let sealed =
            seal_new_session_reply(&provisional.responder, local_static_secret, payload, rng)?;
        let inbound = InboundTagWindow::new(
            sealed.inbound_tag_set,
            self.config.max_tag_look_ahead as usize,
        )?;
        self.sessions.insert(
            remote_key,
            PairedSession {
                outbound: sealed.outbound_tag_set,
                inbound,
                last_used_seconds: now_seconds,
            },
        );
        Ok(NewSessionReplyOutbound {
            message: sealed.message,
        })
    }

    /// Accepts a New Session Reply for one of this manager's
    /// pending outbound handshakes. The reply tag selects the
    /// pending handshake; no caller-supplied remote identity is
    /// consulted. On success the paired session is installed under
    /// the remote static public key Alice bound into the handshake.
    pub fn accept_new_session_reply(
        &mut self,
        local_id: DestinationId,
        local_static_secret: &[u8; X25519_KEY_LENGTH],
        reply: &NewSessionReplyMessage,
        now_seconds: u32,
    ) -> Result<AcceptedNewSessionReply, EciesSessionError> {
        let _ = local_id;
        let slot = *self
            .pending_reply_tags
            .get(&reply.tag)
            .ok_or(EciesSessionError::NoPendingHandshake)?;
        let pending = self.pending_initiated[slot]
            .take()
            .ok_or(EciesSessionError::NoPendingHandshake)?;
        self.pending_reply_tags.retain(|_, index| *index != slot);
        let remote_static_public = *pending.sender.bob_static_public();
        let opened = open_new_session_reply(&pending.sender, local_static_secret, reply)?;
        if self.sessions.len() >= self.config.max_peers as usize
            && !self
                .sessions
                .contains_key(&RemoteStaticKey::from_public(&remote_static_public))
        {
            return Err(EciesSessionError::PeerCapacity {
                maximum: self.config.max_peers,
            });
        }
        let inbound = InboundTagWindow::new(
            opened.inbound_tag_set,
            self.config.max_tag_look_ahead as usize,
        )?;
        self.sessions.insert(
            RemoteStaticKey::from_public(&remote_static_public),
            PairedSession {
                outbound: opened.outbound_tag_set,
                inbound,
                last_used_seconds: now_seconds,
            },
        );
        Ok(AcceptedNewSessionReply {
            payload: opened.payload,
            remote_static_public,
        })
    }

    /// Decrypts an inbound Existing Session message against the
    /// paired session selected by its tag.
    pub fn accept_existing_session(
        &mut self,
        message: &ExistingSessionMessage,
    ) -> Result<AcceptedExistingSession, EciesSessionError> {
        for (remote_key, session) in self.sessions.iter_mut() {
            if let Some(index) = session.inbound.consume(&message.tag) {
                let payload = open_existing_session(&mut session.inbound.tag_set, index, message)?;
                return Ok(AcceptedExistingSession {
                    payload,
                    remote_static_public: *remote_key.public(),
                });
            }
        }
        Err(EciesSessionError::UnknownSessionTag)
    }

    /// Advances the deterministic clock: expires stale sessions,
    /// pending handshakes, and provisional responders.
    pub fn advance_time(&mut self, now_seconds: u32) -> EciesAdvanceReport {
        let idle = self.config.idle_seconds;
        let mut expired = 0_usize;
        self.sessions.retain(|_, session| {
            let alive = now_seconds.saturating_sub(session.last_used_seconds) <= idle;
            if !alive {
                expired = expired.saturating_add(1);
            }
            alive
        });
        let before_pending = self.pending_initiated.iter().flatten().count();
        for slot in self.pending_initiated.iter_mut() {
            let expired_pending = match slot {
                Some(pending) => now_seconds.saturating_sub(pending.created_seconds) > idle,
                None => false,
            };
            if expired_pending {
                *slot = None;
            }
        }
        let live: Vec<bool> = self.pending_initiated.iter().map(Option::is_some).collect();
        self.pending_reply_tags.retain(|_, index| live[*index]);
        let dropped_pending = before_pending - self.pending_initiated.iter().flatten().count();
        let before_provisional = self.provisional_responders.len();
        self.provisional_responders.retain(|_, provisional| {
            now_seconds.saturating_sub(provisional.last_used_seconds) <= idle
        });
        let dropped_provisional = before_provisional - self.provisional_responders.len();
        EciesAdvanceReport {
            expired_sessions: expired,
            dropped_replay_entries: dropped_pending + dropped_provisional,
            pending_handshakes: self.pending_initiated.iter().flatten().count() as u16,
        }
    }

    fn remember_new_session_ephemeral(&mut self, representative: &[u8; REPRESENTATIVE_LENGTH]) {
        if self.seen_new_session_ephemerals.len() >= self.config.max_pending_handshakes as usize {
            self.seen_new_session_ephemerals.pop_front();
        }
        self.seen_new_session_ephemerals.push_back(*representative);
    }
}

/// Encrypted ECIES outbound message produced by the manager.
#[derive(Debug)]
pub enum EciesOutboundMessage {
    /// A bound New Session handshake message. The manager retains
    /// the pending reply window internally.
    NewSession {
        /// The wire-encoded bound New Session message.
        message: BoundNewSessionMessage,
    },
    /// An Existing Session message riding the paired outbound tag
    /// set.
    Existing(ExistingSessionMessage),
}

/// Decrypted inbound bound New Session result.
#[derive(Debug)]
pub struct AcceptedNewSession {
    /// The decrypted Garlic Clove payload.
    pub payload: Vec<u8>,
    /// The sender's X25519 static public key (session identity).
    pub alice_static_public: [u8; X25519_KEY_LENGTH],
}

/// Wire-bound New Session Reply ready for transmission.
#[derive(Debug)]
pub struct NewSessionReplyOutbound {
    /// The wire-encoded New Session Reply message.
    pub message: NewSessionReplyMessage,
}

/// Decrypted inbound New Session Reply result.
#[derive(Debug)]
pub struct AcceptedNewSessionReply {
    /// The decrypted Garlic Clove payload.
    pub payload: Vec<u8>,
    /// The remote static public key the session is paired under.
    pub remote_static_public: [u8; X25519_KEY_LENGTH],
}

/// Decrypted inbound Existing Session result.
#[derive(Debug)]
pub struct AcceptedExistingSession {
    /// The decrypted Garlic Clove payload.
    pub payload: Vec<u8>,
    /// The remote static public key the session is paired under.
    pub remote_static_public: [u8; X25519_KEY_LENGTH],
}

/// Classification outcome for [`EciesSessionManager::classify`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassifiedInbound {
    /// The leading tag matches a pending outbound handshake's
    /// reply window.
    NewSessionReply,
    /// The leading tag matches an established session's inbound
    /// window.
    ExistingSession,
    /// No tag matched; the length is consistent with a bound New
    /// Session message.
    CandidateNewSession,
    /// The envelope matches no known classification.
    Unknown(ClassifiedUnknown),
}

/// Typed unknown-classification reasons.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassifiedUnknown {
    /// The envelope is shorter than the smallest valid ECIES
    /// message.
    TooShort {
        /// Observed byte count.
        actual: usize,
        /// Smallest acceptable byte count.
        minimum: usize,
    },
    /// The leading 8 bytes match no window and the envelope is too
    /// short to be a New Session.
    UnmatchedTag,
}

/// Aggregated report from [`EciesSessionManager::advance_time`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EciesAdvanceReport {
    /// Number of expired paired sessions dropped by the advance.
    pub expired_sessions: usize,
    /// Number of stale pending handshakes plus provisional
    /// responders dropped by the advance.
    pub dropped_replay_entries: usize,
    /// Pending outbound handshakes still held.
    pub pending_handshakes: u16,
}

/// Typed session-manager failures.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum EciesSessionError {
    /// The wrapped ECIES primitive returned a typed error.
    #[error("ECIES session primitive failed: {0}")]
    Ecies(#[from] EciesError),
    /// The pending-handshake capacity was exhausted.
    #[error("ECIES pending-handshake capacity {maximum} exhausted")]
    PendingHandshakeCapacity {
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The paired-session peer capacity was exhausted.
    #[error("ECIES paired-session capacity {maximum} exhausted")]
    PeerCapacity {
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The caller addressed a session that does not exist.
    #[error("ECIES session not found for remote destination")]
    NoSession,
    /// No pending outbound handshake matched the reply tag.
    #[error("ECIES New Session Reply matched no pending handshake")]
    NoPendingHandshake,
    /// No provisional responder exists for the supplied remote
    /// static key.
    #[error("ECIES no provisional responder for remote static key")]
    NoProvisionalResponder,
    /// The inbound tag matched no established session window.
    #[error("ECIES Existing Session tag matched no session window")]
    UnknownSessionTag,
    /// A replayed bound New Session (duplicate ephemeral) was
    /// rejected.
    #[error("ECIES duplicate bound New Session rejected")]
    DuplicateNewSession,
    /// The session manager detected a protocol-level violation.
    #[error("ECIES session manager protocol violation: {0}")]
    Protocol(&'static str),
}

/// Decode a payload from the ECIES decoded bytes into a typed
/// Garlic Clove block.
pub fn decode_decrypted_payload(plaintext: &[u8]) -> Result<GarlicCloveBlock, EciesPayloadError> {
    let sequence = EciesPayloadSequence::decode(plaintext, plaintext.len(), false)
        .map_err(EciesPayloadError::Codec)?;
    for block in sequence.blocks() {
        if let EciesPayloadBlock::GarlicClove(clove) = block {
            return Ok(clove.clone());
        }
    }
    Err(EciesPayloadError::NoClove)
}

/// Encode a Garlic Clove into the I2P ECIES payload block sequence.
pub fn encode_garlic_clove_payload(clove: &GarlicCloveBlock) -> Result<Vec<u8>, EciesPayloadError> {
    let mut sequence = EciesPayloadSequence::empty();
    // Plan 121 §4 mandates a DateTime first block in a New
    // Session payload. Existing Session messages may omit the
    // DateTime because they ride an installed session. The
    // caller decides which case applies; the ECIES layer
    // itself does not insert a DateTime here.
    sequence
        .push(EciesPayloadBlock::GarlicClove(clove.clone()))
        .map_err(EciesPayloadError::Codec)?;
    sequence
        .encode_to_vec(crate::message::MAX_DESTINATION_PAYLOAD_BYTES, false)
        .map_err(EciesPayloadError::Codec)
}

/// Encode a New Session Garlic payload that contains a DateTime
/// block followed by the supplied Garlic Clove.
pub fn encode_new_session_payload(
    now_seconds: u32,
    clove: &GarlicCloveBlock,
) -> Result<Vec<u8>, EciesPayloadError> {
    let mut sequence = EciesPayloadSequence::empty();
    sequence
        .push(EciesPayloadBlock::DateTime(now_seconds))
        .map_err(EciesPayloadError::Codec)?;
    sequence
        .push(EciesPayloadBlock::GarlicClove(clove.clone()))
        .map_err(EciesPayloadError::Codec)?;
    sequence
        .encode_to_vec(crate::message::MAX_DESTINATION_PAYLOAD_BYTES, true)
        .map_err(EciesPayloadError::Codec)
}

/// Build the local Garlic Clove block for a destination-local
/// payload destined for `now_seconds` and `message_id`. The
/// short-form I2NP message bytes are passed through unchanged.
pub fn local_clove(now_seconds: u32, message_id: u32, i2np_body: Vec<u8>) -> GarlicCloveBlock {
    let _ = (now_seconds, message_id);
    GarlicCloveBlock {
        delivery: GarlicDelivery::Local,
        message: i2np_body,
    }
}

/// Typed payload encoding/decoding failures.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EciesPayloadError {
    /// The I2P ECIES payload block codec rejected the input.
    #[error("ECIES payload codec error: {0}")]
    Codec(i2pr_proto::CodecError),
    /// No Garlic Clove block was present in the decoded payload.
    #[error("ECIES decrypted payload did not contain a Garlic Clove")]
    NoClove,
}
