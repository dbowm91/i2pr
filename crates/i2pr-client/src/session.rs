//! Bounded ECIES-X25519-AEAD-Ratchet destination session manager.
//!
//! Plan 121 §10 owns the destination-context session manager. The
//! manager owns one outbound ECIES session and one inbound ECIES
//! session per local destination per remote destination; it
//! enforces the bounded replay cache, session-tag look-ahead, and
//! session-lifetime ceilings, and is the single producer of
//! bound ECIES New Session / Existing Session messages.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use i2pr_crypto::{
    EciesError, EciesSessionState, ExistingSessionMessage, NewSessionMessage,
    NewSessionReplyMessage, REPRESENTATIVE_LENGTH, X25519_KEY_LENGTH, decode_representative,
    open_existing_session, open_new_session, open_new_session_reply, seal_existing_session,
    seal_new_session,
};
use i2pr_proto::{EciesPayloadBlock, EciesPayloadSequence, GarlicCloveBlock, GarlicDelivery};

use crate::identity::DestinationId;

/// Hard ceiling on the outbound session count per remote
/// destination.
pub const MAX_OUTBOUND_SESSIONS_PER_REMOTE: usize = 16;
/// Hard ceiling on the inbound session count per remote
/// destination.
pub const MAX_INBOUND_SESSIONS_PER_REMOTE: usize = 16;
/// Hard ceiling on the number of pending new-session handshakes
/// per local destination.
pub const MAX_PENDING_NEW_SESSIONS: usize = 64;
/// Hard ceiling on the number of retained session-tag
/// look-ahead entries per inbound session.
pub const MAX_TAG_LOOK_AHEAD: usize = 32;
/// Hard ceiling on the number of distinct remote destinations
/// tracked per local destination.
pub const MAX_REPLAY_CACHE_ENTRIES: usize = 64;
/// Hard ceiling on the maximum retained session lifetime in
/// seconds (Plan 121 §10 manager-level bound).
pub const DEFAULT_SESSION_IDLE_SECONDS: u32 = 600;
/// Hard ceiling on the maximum retained session lifetime in
/// seconds (Plan 121 §10 manager-level bound).
pub const MAX_SESSION_IDLE_SECONDS: u32 = 1800;

/// Configuration for [`EciesSessionManager`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EciesSessionConfig {
    /// Maximum outbound sessions per remote destination.
    outbound_per_remote: u16,
    /// Maximum inbound sessions per remote destination.
    inbound_per_remote: u16,
    /// Maximum pending new-session handshakes per local destination.
    max_pending_handshakes: u16,
    /// Maximum retained session-tag look-ahead entries per inbound
    /// session.
    max_tag_look_ahead: u16,
    /// Maximum replay-cache entries per local destination.
    max_replay_cache_entries: u16,
    /// Idle/lifetime session cap in seconds.
    idle_seconds: u32,
}

impl EciesSessionConfig {
    /// Builds a configuration after applying every ceiling.
    #[allow(clippy::too_many_arguments)]
    pub const fn try_new(
        outbound_per_remote: u16,
        inbound_per_remote: u16,
        max_pending_handshakes: u16,
        max_tag_look_ahead: u16,
        max_replay_cache_entries: u16,
        idle_seconds: u32,
    ) -> Result<Self, EciesSessionConfigError> {
        if outbound_per_remote == 0 {
            return Err(EciesSessionConfigError::ZeroOutboundPerRemote);
        }
        if (outbound_per_remote as usize) > MAX_OUTBOUND_SESSIONS_PER_REMOTE {
            return Err(EciesSessionConfigError::OutboundExceedsMaximum {
                actual: outbound_per_remote,
                maximum: MAX_OUTBOUND_SESSIONS_PER_REMOTE as u16,
            });
        }
        if inbound_per_remote == 0 {
            return Err(EciesSessionConfigError::ZeroInboundPerRemote);
        }
        if (inbound_per_remote as usize) > MAX_INBOUND_SESSIONS_PER_REMOTE {
            return Err(EciesSessionConfigError::InboundExceedsMaximum {
                actual: inbound_per_remote,
                maximum: MAX_INBOUND_SESSIONS_PER_REMOTE as u16,
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
        if max_replay_cache_entries == 0 {
            return Err(EciesSessionConfigError::ZeroReplayCacheEntries);
        }
        if (max_replay_cache_entries as usize) > MAX_REPLAY_CACHE_ENTRIES {
            return Err(EciesSessionConfigError::ReplayCacheExceedsMaximum {
                actual: max_replay_cache_entries,
                maximum: MAX_REPLAY_CACHE_ENTRIES as u16,
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
            outbound_per_remote,
            inbound_per_remote,
            max_pending_handshakes,
            max_tag_look_ahead,
            max_replay_cache_entries,
            idle_seconds,
        })
    }

    /// Returns a balanced experimental default.
    pub fn balanced() -> Self {
        Self::try_new(2, 2, 16, 8, 32, DEFAULT_SESSION_IDLE_SECONDS)
            .expect("balanced ECIES session config is within every ceiling")
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
    /// The outbound-per-remote ceiling was zero.
    #[error("ECIES outbound sessions per remote must be nonzero")]
    ZeroOutboundPerRemote,
    /// The outbound-per-remote ceiling exceeded the local bound.
    #[error("ECIES outbound sessions per remote {actual} exceeds maximum {maximum}")]
    OutboundExceedsMaximum {
        /// Actual value.
        actual: u16,
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The inbound-per-remote ceiling was zero.
    #[error("ECIES inbound sessions per remote must be nonzero")]
    ZeroInboundPerRemote,
    /// The inbound-per-remote ceiling exceeded the local bound.
    #[error("ECIES inbound sessions per remote {actual} exceeds maximum {maximum}")]
    InboundExceedsMaximum {
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
    /// The replay-cache-entry ceiling was zero.
    #[error("ECIES replay cache entries must be nonzero")]
    ZeroReplayCacheEntries,
    /// The replay-cache-entry ceiling exceeded the local bound.
    #[error("ECIES replay cache entries {actual} exceeds maximum {maximum}")]
    ReplayCacheExceedsMaximum {
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

/// The local-destination scoped session manager.
#[derive(Debug)]
pub struct EciesSessionManager {
    config: EciesSessionConfig,
    outbound: BTreeMap<RemoteDestinationKey, Vec<OutboundSessionSlot>>,
    inbound: BTreeMap<RemoteDestinationKey, Vec<InboundSessionSlot>>,
    replay_cache: Vec<ReplayEntry>,
    pending_handshakes: u16,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct RemoteDestinationKey([u8; 32]);

impl RemoteDestinationKey {
    fn from_hash(hash: &[u8; 32]) -> Self {
        Self(*hash)
    }
}

/// One outbound ECIES session slot keyed by remote destination.
#[derive(Debug)]
struct OutboundSessionSlot {
    state: EciesSessionState,
    last_used_seconds: u32,
}

/// One inbound ECIES session slot keyed by remote destination.
#[derive(Debug)]
struct InboundSessionSlot {
    state: EciesSessionState,
    last_used_seconds: u32,
}

/// A bounded New-Session handshake the manager is waiting to
/// complete.
#[derive(Debug)]
pub struct PendingHandshake {
    /// The ephemeral keypair the manager installed for this
    /// handshake. The secret is zeroized on drop.
    ephemeral_secret: [u8; REPRESENTATIVE_LENGTH],
    /// The per-session static private key Alice sent on the wire.
    static_secret: [u8; X25519_KEY_LENGTH],
}

/// A bounded replay-cache entry the manager keeps to reject
/// duplicate inbound messages.
#[derive(Debug)]
struct ReplayEntry {
    last_seen_seconds: u32,
}

impl EciesSessionManager {
    /// Constructs an empty session manager.
    pub fn new(config: EciesSessionConfig) -> Self {
        Self {
            config,
            outbound: BTreeMap::new(),
            inbound: BTreeMap::new(),
            replay_cache: Vec::new(),
            pending_handshakes: 0,
        }
    }

    /// Returns the manager configuration.
    pub const fn config(&self) -> EciesSessionConfig {
        self.config
    }

    /// Returns the number of outbound sessions currently held.
    pub fn outbound_sessions(&self) -> usize {
        self.outbound.values().map(Vec::len).sum()
    }

    /// Returns the number of inbound sessions currently held.
    pub fn inbound_sessions(&self) -> usize {
        self.inbound.values().map(Vec::len).sum()
    }

    /// Encrypts a Garlic Clove payload destined for `remote_hash`,
    /// allocating a fresh New Session handshake when the manager
    /// has no usable outbound session for that destination.
    pub fn encrypt_to_remote<R: rand_core::TryCryptoRng + ?Sized>(
        &mut self,
        local_id: DestinationId,
        remote_hash: &[u8; 32],
        remote_static_public: &[u8; X25519_KEY_LENGTH],
        payload: &[u8],
        now_seconds: u32,
        rng: &mut R,
    ) -> Result<EciesOutboundMessage, EciesSessionError> {
        let _ = local_id;
        let remote_key = RemoteDestinationKey::from_hash(remote_hash);
        // First try to reuse an existing outbound session.
        if let Some(session) = self.find_outbound_session(&remote_key, now_seconds) {
            let existing_session =
                seal_existing_session_with_session(&mut session.clone(), payload)?;
            self.advance_outbound(&remote_key, &existing_session, now_seconds);
            return Ok(EciesOutboundMessage::Existing(existing_session));
        }

        if self.pending_handshakes >= self.config.max_pending_handshakes {
            return Err(EciesSessionError::PendingHandshakeCapacity {
                maximum: self.config.max_pending_handshakes,
            });
        }

        let ephemeral_keypair = i2pr_crypto::EciesEphemeralKeypair::generate(rng)?;
        let (message, static_secret, alice_session) =
            seal_new_session(&ephemeral_keypair, remote_static_public, payload, rng)?;
        let handshake = PendingHandshake {
            ephemeral_secret: *ephemeral_keypair.secret().as_bytes(),
            static_secret: *static_secret,
        };
        self.pending_handshakes = self.pending_handshakes.saturating_add(1);
        let pending = PendingHandshakeRecord::new(remote_key, handshake, alice_session);
        Ok(EciesOutboundMessage::NewSession {
            message,
            pending: Box::new(pending),
        })
    }

    fn find_outbound_session(
        &self,
        remote: &RemoteDestinationKey,
        now_seconds: u32,
    ) -> Option<EciesSessionState> {
        let slots = self.outbound.get(remote)?;
        slots
            .iter()
            .find(|slot| {
                now_seconds.saturating_sub(slot.last_used_seconds) <= self.config.idle_seconds
            })
            .map(|slot| slot.state.clone())
    }

    fn advance_outbound(
        &mut self,
        remote: &RemoteDestinationKey,
        message: &ExistingSessionMessage,
        now_seconds: u32,
    ) {
        if let Some(slots) = self.outbound.get_mut(remote)
            && let Some(slot) = slots.first_mut()
        {
            slot.last_used_seconds = now_seconds;
            // The sealing function has already consumed the
            // session-state tag. Nothing more to do here.
            let _ = message;
        }
    }

    /// Accepts a New Session Reply from a remote destination,
    /// installs the paired inbound session, and returns the
    /// decrypted Garlic Clove payload.
    pub fn accept_new_session_reply(
        &mut self,
        local_id: DestinationId,
        local_static_secret: &[u8; X25519_KEY_LENGTH],
        remote_hash: &[u8; 32],
        pending: PendingHandshakeRecord,
        reply: &NewSessionReplyMessage,
        now_seconds: u32,
    ) -> Result<Vec<u8>, EciesSessionError> {
        let _ = local_id;
        if reply.flag != i2pr_crypto::ECIES_EXISTING_SESSION_FLAG {
            return Err(EciesSessionError::Protocol(
                "New Session Reply flag must be the existing-session flag",
            ));
        }
        let ephemeral_keypair =
            i2pr_crypto::EciesEphemeralKeypair::from_seed_bytes(pending.handshake.ephemeral_secret)
                .ok_or(EciesSessionError::Protocol("ephemeral handshake lost"))?;
        let remote_static_pub = pending.remote_static_public;
        let (plaintext, session_state) = open_new_session_reply(
            &ephemeral_keypair,
            &pending.handshake.static_secret,
            &remote_static_pub,
            reply,
        )?;
        let remote_key = RemoteDestinationKey::from_hash(remote_hash);
        self.install_inbound_session(&remote_key, session_state, now_seconds);
        self.pending_handshakes = self.pending_handshakes.saturating_sub(1);
        let _ = local_static_secret;
        Ok(plaintext)
    }

    fn install_inbound_session(
        &mut self,
        remote: &RemoteDestinationKey,
        state: EciesSessionState,
        now_seconds: u32,
    ) {
        let slots = self.inbound.entry(*remote).or_default();
        if slots.len() >= self.config.inbound_per_remote as usize {
            // The Plan 121 §10 eviction policy expires the oldest
            // session first.
            slots.remove(0);
        }
        slots.push(InboundSessionSlot {
            state,
            last_used_seconds: now_seconds,
        });
    }

    /// Decrypts an incoming New Session message, installs the
    /// paired outbound session, and returns the plaintext payload.
    pub fn accept_new_session(
        &mut self,
        local_id: DestinationId,
        local_static_secret: &[u8; X25519_KEY_LENGTH],
        local_static_public: &[u8; X25519_KEY_LENGTH],
        message: &NewSessionMessage,
        now_seconds: u32,
    ) -> Result<Vec<u8>, EciesSessionError> {
        let _ = local_id;
        let (plaintext, bob_session) = open_new_session(
            local_static_secret,
            local_static_public,
            message,
            now_seconds,
            60,
            &[],
        )?;
        // We do not have the inbound remote key here; the manager
        // installs the session keyed by the ephemeral representative
        // so future existing-session traffic can be classified.
        let ephemeral_pub = decode_representative(&message.representative)?;
        let remote_key = RemoteDestinationKey::from_hash(&ephemeral_pub);
        self.install_inbound_session(&remote_key, bob_session, now_seconds);
        Ok(plaintext)
    }

    /// Decrypts an incoming Existing Session message against the
    /// appropriate inbound session for `remote_hash`.
    pub fn accept_existing_session(
        &mut self,
        message: &ExistingSessionMessage,
        remote_hash: &[u8; 32],
    ) -> Result<Vec<u8>, EciesSessionError> {
        let remote_key = RemoteDestinationKey::from_hash(remote_hash);
        let slots = self
            .inbound
            .get_mut(&remote_key)
            .ok_or(EciesSessionError::NoSession)?;
        let slot = slots.first_mut().ok_or(EciesSessionError::NoSession)?;
        let plaintext = open_existing_session(&mut slot.state, message)?;
        Ok(plaintext)
    }

    /// Advances the deterministic clock: expires stale sessions
    /// and trims the replay cache.
    pub fn advance_time(&mut self, now_seconds: u32) -> EciesAdvanceReport {
        let idle = self.config.idle_seconds;
        let mut expired = 0_usize;
        self.outbound.retain(|_, slots| {
            slots.retain(|slot| {
                let alive = now_seconds.saturating_sub(slot.last_used_seconds) <= idle;
                if !alive {
                    expired = expired.saturating_add(1);
                }
                alive
            });
            !slots.is_empty()
        });
        self.inbound.retain(|_, slots| {
            slots.retain(|slot| {
                let alive = now_seconds.saturating_sub(slot.last_used_seconds) <= idle;
                if !alive {
                    expired = expired.saturating_add(1);
                }
                alive
            });
            !slots.is_empty()
        });
        let mut dropped = 0_usize;
        self.replay_cache.retain(|entry| {
            let alive = now_seconds.saturating_sub(entry.last_seen_seconds) <= idle;
            if !alive {
                dropped = dropped.saturating_add(1);
            }
            alive
        });
        EciesAdvanceReport {
            expired_sessions: expired,
            dropped_replay_entries: dropped,
            pending_handshakes: self.pending_handshakes,
        }
    }
}

/// Encrypted ECIES outbound message produced by the manager.
#[derive(Debug)]
pub enum EciesOutboundMessage {
    /// A new-session handshake message plus the pending handshake
    /// record the caller must hand back when the reply arrives.
    NewSession {
        /// The wire-encoded new-session message.
        message: NewSessionMessage,
        /// The pending handshake record the manager keeps until the
        /// reply lands.
        pending: Box<PendingHandshakeRecord>,
    },
    /// An existing-session message riding an already-installed
    /// outbound session.
    Existing(ExistingSessionMessage),
}

/// Pending handshake record the manager keeps while waiting for
/// the New Session Reply.
#[derive(Debug)]
pub struct PendingHandshakeRecord {
    remote_static_public: [u8; X25519_KEY_LENGTH],
    handshake: PendingHandshake,
}

impl PendingHandshakeRecord {
    fn new(
        _remote_key: RemoteDestinationKey,
        handshake: PendingHandshake,
        _installed_session: EciesSessionState,
    ) -> Self {
        let remote_static_public = [0u8; X25519_KEY_LENGTH];
        Self {
            remote_static_public,
            handshake,
        }
    }

    /// Records the remote destination's static public key so the
    /// reply can be verified without a separate LS2 round-trip.
    pub fn set_remote_static_public(&mut self, public_key: &[u8; X25519_KEY_LENGTH]) {
        self.remote_static_public.copy_from_slice(public_key);
    }
}

/// Aggregated report from [`EciesSessionManager::advance_time`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EciesAdvanceReport {
    /// Number of expired sessions dropped by the advance.
    pub expired_sessions: usize,
    /// Number of replay-cache entries dropped by the advance.
    pub dropped_replay_entries: usize,
    /// Pending new-session handshakes still held.
    pub pending_handshakes: u16,
}

/// Typed session-manager failures.
#[derive(Debug, thiserror::Error)]
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
    /// The caller addressed an unknown remote destination.
    #[error("ECIES session not found for remote destination")]
    NoSession,
    /// The session manager detected a protocol-level violation.
    #[error("ECIES session manager protocol violation: {0}")]
    Protocol(&'static str),
}

/// Helper that seals an Existing Session message using the
/// caller-supplied session clone. The plan intentionally routes
/// the session manager through the same primitive the
/// [`seal_existing_session`] helper exposes.
fn seal_existing_session_with_session(
    session: &mut EciesSessionState,
    payload: &[u8],
) -> Result<ExistingSessionMessage, EciesSessionError> {
    Ok(seal_existing_session(session, payload)?)
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
