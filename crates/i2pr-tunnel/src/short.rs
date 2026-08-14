//! Runtime-neutral short tunnel-build state machine.
//!
//! Plan 108 §3.7 owns the bounded state machine that drives one
//! attempted build from path specification through terminal
//! outcome. The state machine:
//!
//! - accepts a fully validated path specification,
//! - generates per-hop ephemeral material,
//! - seals every hop through the [`EciesX25519BuildCryptography`]
//!   primitive,
//! - emits a runtime-neutral [`ShortBuildAction::Deliver`] action
//!   that transport adapters may consume,
//! - accepts events such as `DeliveryAccepted`, `BuildReply`,
//!   `DeadlineExceeded`, and `Cancelled`,
//! - reaches one of the terminal states `Established`,
//!   `HopRejected`, `TimedOut`, `Cancelled`, `InvalidReply`,
//!   `CryptoFailed`, or `DeliveryFailed`,
//! - registers the resulting tunnel material into
//!   `ExploratoryPool` only via the success-only
//!   `ShortBuildRegistrar` from `crate::short_state`.
//!
//! The state machine and its registrar deliberately remain
//! runtime-neutral: no socket access, no Tokio runtime, no DNS,
//! no filesystem access.

#![forbid(unsafe_code)]

use std::fmt;

use i2pr_proto::{Date, Hash, SHORT_BUILD_RECORD_SIZE, SHORT_REPLY_PLAINTEXT_SIZE};
use rand_core::{CryptoRng, TryRngCore};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::build::{
    BuildCryptographyUnavailable, BuildRecordLayout, BuildRecordLayoutError, BuildReplyKind,
    BuildRequestKind,
};
use crate::build_crypto::{
    BuildCryptography, BuildCryptographyError, EPHEMERAL_KEY_LEN, EciesX25519BuildCryptography,
    NONCE_LEN, SHORT_REQUEST_KEY_LEN, TAG_LEN,
};
use crate::identity::{TunnelDirection, TunnelId};
use crate::short_record::{
    BuildOptions, HopRole, LayerEncryptionType, ShortRequestRecord, ShortResponseCode,
};

/// A unique attempt identifier the Plan 108 state machine hands to
/// every dispatch. The id is monotonic and never reused so the
/// caller can correlate external events with build attempts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BuildAttemptId(u64);

impl BuildAttemptId {
    /// Constructs a new attempt id with a caller-supplied monotonic
    /// counter.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the inner numeric value.
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for BuildAttemptId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Hop order index. Zero is the first hop.
pub type HopIndex = u8;

/// Per-hop crypto seed supplied by the caller. The byte array is
/// treated as a high-entropy secret and is zeroized on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct HopCryptoSeed([u8; SHORT_REQUEST_KEY_LEN]);

impl HopCryptoSeed {
    /// Loads the supplied seed bytes without exposing them in any
    /// accessor beyond the private reference consumed by the build
    /// cryptography primitive.
    pub const fn from_bytes(bytes: [u8; SHORT_REQUEST_KEY_LEN]) -> Self {
        Self(bytes)
    }

    /// Returns the raw bytes for the lifetime of the borrow.
    pub const fn as_bytes(&self) -> &[u8; SHORT_REQUEST_KEY_LEN] {
        &self.0
    }
}

impl fmt::Debug for HopCryptoSeed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("HopCryptoSeed")
            .field(&"<redacted>")
            .finish()
    }
}

/// Per-hop identity carried by the [`ShortBuildPath`].
#[derive(Clone, Debug)]
pub struct HopSpec {
    /// The hop's RouterHash as the receiving peer knows it.
    pub router_hash: Hash,
    /// The hop's X25519 static encryption key (32 bytes).
    pub static_encryption_key: [u8; EPHEMERAL_KEY_LEN],
    /// The hop's role within the tunnel (gateway/participant/endpoint).
    pub role: HopRole,
    /// Hop-specific crypto seed used to derive per-hop request and
    /// reply keys. The seed is zeroized on drop.
    pub crypto_seed: HopCryptoSeed,
}

impl HopSpec {
    /// Constructs a [`HopSpec`] from validated inputs.
    pub fn new(
        router_hash: Hash,
        static_encryption_key: [u8; EPHEMERAL_KEY_LEN],
        role: HopRole,
        crypto_seed: HopCryptoSeed,
    ) -> Self {
        Self {
            router_hash,
            static_encryption_key,
            role,
            crypto_seed,
        }
    }
}

/// A fully specified short tunnel-build path.
///
/// The path carries the configuration the state machine needs to
/// construct one [`ShortTunnelBuildMessage`]. The builder never
/// queries NetDB; the caller is responsible for resolving every
/// static encryption key and router hash.
#[derive(Clone, Debug)]
pub struct ShortBuildPath {
    /// Local creator identifier for diagnostic correlation.
    pub attempt_id: BuildAttemptId,
    /// Inbound or outbound direction.
    pub direction: TunnelDirection,
    /// Creator tunnel identifier this build is associated with.
    pub creator_tunnel_id: TunnelId,
    /// Creator static X25519 key used to authenticate replies.
    pub creator_static_key: [u8; EPHEMERAL_KEY_LEN],
    /// Ordered per-hop specifications.
    pub hops: Vec<HopSpec>,
    /// Request creation timestamp (milliseconds since the Unix
    /// epoch).
    pub request_time: Date,
    /// Request expiration in milliseconds since the Unix epoch.
    pub expiration_ms: u64,
    /// Per-hop next message id the receiving hop will use.
    pub next_message_id: u32,
    /// Build options mapping (empty when no options are required).
    pub options: BuildOptions,
}

impl ShortBuildPath {
    /// Returns the configured number of hops.
    pub fn hop_count(&self) -> usize {
        self.hops.len()
    }

    /// Validates the path before the state machine accepts it.
    pub fn validate(&self) -> Result<(), ShortBuildConstructionError> {
        if self.hops.is_empty() {
            return Err(ShortBuildConstructionError::InvalidPath {
                reason: "path must declare at least one hop",
            });
        }
        if self.hops.len() > i2pr_proto::MAX_BUILD_RECORDS {
            return Err(ShortBuildConstructionError::InvalidPath {
                reason: "hop count exceeds I2P maximum",
            });
        }
        if self.request_time.as_millis() == 0 {
            return Err(ShortBuildConstructionError::InvalidPath {
                reason: "request time must be nonzero",
            });
        }
        for hop in &self.hops {
            if hop.static_encryption_key.iter().all(|byte| *byte == 0) {
                return Err(ShortBuildConstructionError::InvalidPath {
                    reason: "a hop static encryption key is zero",
                });
            }
        }
        Ok(())
    }
}

/// The actions a built state machine emits. Transport adapters
/// convert these into runtime-specific delivery events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShortBuildAction {
    /// Deliver the constructed build message to the first hop.
    Deliver {
        /// The first-hop router hash the runtime should target.
        first_hop: Hash,
        /// The full 154-byte-aligned build message payload (one or
        /// more 218-byte records concatenated).
        message: Zeroizing<Vec<u8>>,
        /// Number of records the message carries.
        record_count: u8,
        /// Required-cleared deadline in milliseconds since the
        /// Unix epoch.
        deadline_ms: u64,
    },
}

/// Events the state machine consumes from the transport/owner side.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildEvent {
    /// The runtime successfully delivered the build message.
    DeliveryAccepted,
    /// The runtime failed to deliver the build message.
    DeliveryFailed {
        /// Free-form failure category that the caller chooses.
        reason: &'static str,
    },
    /// The builder received a build reply for this attempt.
    BuildReply {
        /// Per-hop reply payload. The buffer is exactly
        /// `records * reply_record_size` bytes.
        reply: Zeroizing<Vec<u8>>,
    },
    /// The deadline was reached before the reply arrived.
    DeadlineExceeded,
    /// The state machine was cancelled by the caller.
    Cancelled,
}

/// Outcome categories for a finished build attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShortBuildOutcome {
    /// Every hop accepted and the build is registered in the pool.
    Established {
        /// Slot the registrar reserved in the pool.
        slot: TunnelId,
        /// Per-hop reply plaintext, in canonical hop order.
        per_hop_replies: Vec<PerHopReply>,
    },
    /// A hop rejected the build; the build did not register.
    HopRejected {
        /// Index of the rejecting hop.
        hop_index: HopIndex,
        /// Reply byte the rejecting hop produced.
        reply_code: ShortResponseCode,
    },
    /// The deadline elapsed before every reply arrived.
    TimedOut,
    /// The state machine was cancelled.
    Cancelled,
    /// A reply failed validation; the build did not register.
    InvalidReply,
    /// An authentication or key-derivation primitive failed.
    CryptoFailed,
    /// Delivery to the first hop failed.
    DeliveryFailed,
}

/// Per-hop reply plaintext exposed to the registrar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerHopReply {
    /// Hop index, in canonical path order.
    pub hop_index: HopIndex,
    /// Reply plaintext bytes for the hop.
    pub plaintext: Zeroizing<Vec<u8>>,
}

/// Terminal-error taxonomy for the short build surface.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ShortBuildConstructionError {
    /// The supplied path is invalid.
    #[error("invalid short build path: {reason}")]
    InvalidPath {
        /// Description of the rejection.
        reason: &'static str,
    },
    /// Underlying layout validation failure.
    #[error("short build layout rejected: {0}")]
    Layout(#[from] BuildRecordLayoutError),
    /// Underlying cryptography primitive failure.
    #[error("short build cryptography rejected: {0}")]
    Cryptography(#[from] BuildCryptographyError),
    /// The build cryptography seam has no live primitive.
    #[error("short build cryptography is unavailable")]
    CryptographyUnavailable,
    /// The state machine was already in a terminal state.
    #[error("short build state machine is already terminal")]
    AlreadyTerminal,
    /// The supplied event is not valid in the current state.
    #[error("event {event:?} is not valid in state {state}")]
    InvalidEvent {
        /// The current state when the event was rejected.
        state: &'static str,
        /// The rejected event label.
        event: &'static str,
    },
    /// The reply message was not from the expected message kind.
    #[error("reply message kind {actual:?} does not match expected {expected:?}")]
    UnexpectedReplyKind {
        /// Expected reply message kind.
        expected: BuildReplyKind,
        /// Actual reply message kind observed.
        actual: BuildReplyKind,
    },
    /// The reply carried the wrong number of records.
    #[error("reply record count {actual} does not match {expected}")]
    ReplyRecordCount {
        /// Actual reply record count.
        actual: usize,
        /// Expected record count.
        expected: usize,
    },
    /// The reply record carried the wrong byte size.
    #[error("reply record length {actual} bytes is outside the accepted range")]
    ReplyRecordLength {
        /// Actual reply record length.
        actual: usize,
    },
    /// The short record encoder rejected an input.
    #[error("short build record rejected: {0}")]
    Record(#[from] crate::short_record::ShortBuildError),
    /// Spec variant is unused. Reserved for future expansion.
    #[error("short build surface reserved")]
    Reserved,
}

impl From<BuildCryptographyUnavailable> for ShortBuildConstructionError {
    fn from(_: BuildCryptographyUnavailable) -> Self {
        Self::CryptographyUnavailable
    }
}

/// Per-hop crypto context retained after sealing a request so the
/// creator can later decrypt the corresponding reply.
///
/// The context is non-cloneable, zeroizing, and does not implement
/// `Debug` so the secret ephemeral material does not leak through
/// logs or snapshot tests.
pub struct HopCryptoContext {
    hop_index: HopIndex,
    creator_static_priv: [u8; EPHEMERAL_KEY_LEN],
    ephemeral_pub: [u8; EPHEMERAL_KEY_LEN],
    session_digest: [u8; 32],
    request_plaintext: Zeroizing<[u8; 154]>,
}

impl HopCryptoContext {
    /// Returns the hop index the context belongs to.
    pub const fn hop_index(&self) -> HopIndex {
        self.hop_index
    }

    /// Returns the ephemeral X25519 public key the creator put on
    /// the wire for this hop. The value is public.
    pub const fn ephemeral_public(&self) -> &[u8; EPHEMERAL_KEY_LEN] {
        &self.ephemeral_pub
    }

    /// Returns the canonical request plaintext this hop received.
    pub fn request_plaintext(&self) -> &[u8; 154] {
        &self.request_plaintext
    }
}

impl Drop for HopCryptoContext {
    fn drop(&mut self) {
        self.creator_static_priv.zeroize();
        self.ephemeral_pub.zeroize();
        self.session_digest.zeroize();
    }
}

impl fmt::Debug for HopCryptoContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HopCryptoContext")
            .field("hop_index", &self.hop_index)
            .field("ephemeral_pub", &"<redacted>")
            .field("request_plaintext", &"<redacted>")
            .finish()
    }
}

/// The bounded runtime-neutral state machine.
///
/// The state machine drives one attempt through the canonical
/// short-build lifecycle. Each state machine owns exactly one
/// attempt; a higher-level pool/registrar may start a new attempt
/// after a terminal failure.
pub struct ShortBuildStateMachine<C = EciesX25519BuildCryptography>
where
    C: BuildCryptography,
{
    attempt_id: BuildAttemptId,
    direction: TunnelDirection,
    path: ShortBuildPath,
    cryptography: C,
    state: StatePhase,
    contexts: Vec<HopCryptoContext>,
    deadline_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StatePhase {
    Prepared,
    Protecting,
    ReadyForDelivery,
    AwaitingReply,
    Established,
    HopRejected,
    TimedOut,
    Cancelled,
    InvalidReply,
    CryptoFailed,
    DeliveryFailed,
}

impl StatePhase {
    #[allow(dead_code)]
    const fn label(self) -> &'static str {
        match self {
            Self::Prepared => "Prepared",
            Self::Protecting => "Protecting",
            Self::ReadyForDelivery => "ReadyForDelivery",
            Self::AwaitingReply => "AwaitingReply",
            Self::Established => "Established",
            Self::HopRejected => "HopRejected",
            Self::TimedOut => "TimedOut",
            Self::Cancelled => "Cancelled",
            Self::InvalidReply => "InvalidReply",
            Self::CryptoFailed => "CryptoFailed",
            Self::DeliveryFailed => "DeliveryFailed",
        }
    }
}

impl ShortBuildStateMachine<EciesX25519BuildCryptography> {
    /// Constructs a new state machine for the supplied path using
    /// the default ECIES-X25519 cryptography primitive.
    pub fn new(path: ShortBuildPath, deadline_ms: u64) -> Self {
        Self::with_cryptography(path, deadline_ms, EciesX25519BuildCryptography::new())
    }
}

impl<C: BuildCryptography> ShortBuildStateMachine<C> {
    /// Constructs a new state machine with a custom cryptography
    /// implementation. The supplied implementation owns the
    /// dispatch; the state machine itself remains runtime-neutral.
    pub fn with_cryptography(path: ShortBuildPath, deadline_ms: u64, cryptography: C) -> Self {
        let attempt_id = path.attempt_id;
        let direction = path.direction;
        Self {
            attempt_id,
            direction,
            path,
            cryptography,
            state: StatePhase::Prepared,
            contexts: Vec::new(),
            deadline_ms,
        }
    }

    /// Returns the state machine's attempt identifier.
    pub const fn attempt_id(&self) -> BuildAttemptId {
        self.attempt_id
    }

    /// Returns the current state label.
    pub const fn current_state_label(&self) -> &'static str {
        match self.state {
            StatePhase::Prepared => "Prepared",
            StatePhase::Protecting => "Protecting",
            StatePhase::ReadyForDelivery => "ReadyForDelivery",
            StatePhase::AwaitingReply => "AwaitingReply",
            StatePhase::Established => "Established",
            StatePhase::HopRejected => "HopRejected",
            StatePhase::TimedOut => "TimedOut",
            StatePhase::Cancelled => "Cancelled",
            StatePhase::InvalidReply => "InvalidReply",
            StatePhase::CryptoFailed => "CryptoFailed",
            StatePhase::DeliveryFailed => "DeliveryFailed",
        }
    }

    /// Returns the build direction.
    pub const fn direction(&self) -> TunnelDirection {
        self.direction
    }

    /// Returns the configured absolute deadline in milliseconds.
    pub const fn deadline_ms(&self) -> u64 {
        self.deadline_ms
    }

    /// Performs the prepare→protecting transition, sealing every
    /// per-hop request record and assembling the build message.
    pub fn prepare<R: CryptoRng + rand_core::RngCore>(
        &mut self,
        rng: &mut R,
    ) -> Result<ShortTunnelBuildMessage, ShortBuildConstructionError> {
        if !matches!(self.state, StatePhase::Prepared) {
            return Err(ShortBuildConstructionError::InvalidEvent {
                state: self.current_state_label(),
                event: "prepare",
            });
        }
        self.path.validate()?;
        self.state = StatePhase::Protecting;
        let mut records = Zeroizing::new(Vec::with_capacity(
            self.path.hops.len() * SHORT_BUILD_RECORD_SIZE,
        ));
        let mut contexts = Vec::with_capacity(self.path.hops.len());
        for (index, hop) in self.path.hops.iter().enumerate() {
            let plaintext =
                build_request_record(self.path.creator_tunnel_id, hop, &self.path, index)?
                    .encode()?;
            let ephemeral_priv = ephemeral_seed_for_hop(rng, self.attempt_id.get(), index as u8)?;
            let record = self.cryptography.seal_short_request(
                plaintext.as_ref(),
                &hop.static_encryption_key,
                ephemeral_priv.as_bytes(),
                rng,
            )?;
            let context = HopCryptoContext {
                hop_index: index as HopIndex,
                creator_static_priv: self.path.creator_static_key,
                ephemeral_pub: extract_ephemeral_pub(record.as_ref()),
                session_digest: ephemeral_priv_digest(ephemeral_priv.as_bytes()),
                request_plaintext: plaintext,
            };
            contexts.push(context);
            records.extend_from_slice(record.as_ref());
        }
        self.contexts = contexts;
        self.state = StatePhase::ReadyForDelivery;
        let message = ShortTunnelBuildMessage {
            kind: BuildRequestKind::ShortTunnelBuild,
            layout: BuildRecordLayout::Short,
            records: records.to_vec(),
            nonce: self.path.next_message_id,
        };
        Ok(message)
    }

    /// Emits the typed delivery action the caller performs.
    pub fn deliver_action(&self, message: ShortTunnelBuildMessage) -> ShortBuildAction {
        let record_count = (message.records.len() / SHORT_BUILD_RECORD_SIZE) as u8;
        let first_hop = self
            .path
            .hops
            .first()
            .map(|hop| hop.router_hash)
            .unwrap_or_else(|| Hash::from_bytes([0_u8; 32]));
        ShortBuildAction::Deliver {
            first_hop,
            message: Zeroizing::new(message.records),
            record_count,
            deadline_ms: self.deadline_ms,
        }
    }

    /// Transitions the state machine to awaiting replies after the
    /// caller dispatches the delivery action.
    pub fn mark_dispatched(&mut self) -> Result<(), ShortBuildConstructionError> {
        if !matches!(self.state, StatePhase::ReadyForDelivery) {
            return Err(ShortBuildConstructionError::InvalidEvent {
                state: self.current_state_label(),
                event: "mark_dispatched",
            });
        }
        self.state = StatePhase::AwaitingReply;
        Ok(())
    }

    /// Accepts an event from the caller side and updates the state
    /// accordingly. Returns the terminal outcome on the terminal
    /// transition or `None` when the state machine remains
    /// mid-transition.
    pub fn handle_event(
        &mut self,
        event: BuildEvent,
    ) -> Result<Option<ShortBuildOutcome>, ShortBuildConstructionError> {
        match event {
            BuildEvent::DeliveryAccepted => {
                if !matches!(self.state, StatePhase::ReadyForDelivery) {
                    return Err(ShortBuildConstructionError::InvalidEvent {
                        state: self.current_state_label(),
                        event: "DeliveryAccepted",
                    });
                }
                self.state = StatePhase::AwaitingReply;
                Ok(None)
            }
            BuildEvent::DeliveryFailed { reason: _ } => {
                if matches!(
                    self.state,
                    StatePhase::Established
                        | StatePhase::HopRejected
                        | StatePhase::TimedOut
                        | StatePhase::Cancelled
                        | StatePhase::InvalidReply
                        | StatePhase::CryptoFailed
                        | StatePhase::DeliveryFailed
                ) {
                    return Err(ShortBuildConstructionError::AlreadyTerminal);
                }
                self.state = StatePhase::DeliveryFailed;
                Ok(Some(ShortBuildOutcome::DeliveryFailed))
            }
            BuildEvent::DeadlineExceeded => {
                if matches!(
                    self.state,
                    StatePhase::Established
                        | StatePhase::HopRejected
                        | StatePhase::TimedOut
                        | StatePhase::Cancelled
                        | StatePhase::InvalidReply
                        | StatePhase::CryptoFailed
                        | StatePhase::DeliveryFailed
                ) {
                    return Err(ShortBuildConstructionError::AlreadyTerminal);
                }
                self.state = StatePhase::TimedOut;
                Ok(Some(ShortBuildOutcome::TimedOut))
            }
            BuildEvent::Cancelled => {
                if matches!(
                    self.state,
                    StatePhase::Established
                        | StatePhase::HopRejected
                        | StatePhase::TimedOut
                        | StatePhase::Cancelled
                        | StatePhase::InvalidReply
                        | StatePhase::CryptoFailed
                        | StatePhase::DeliveryFailed
                ) {
                    return Err(ShortBuildConstructionError::AlreadyTerminal);
                }
                self.state = StatePhase::Cancelled;
                Ok(Some(ShortBuildOutcome::Cancelled))
            }
            BuildEvent::BuildReply { reply } => {
                if !matches!(self.state, StatePhase::AwaitingReply) {
                    return Err(ShortBuildConstructionError::InvalidEvent {
                        state: self.current_state_label(),
                        event: "BuildReply",
                    });
                }
                let outcome = self.process_reply(&reply)?;
                let terminal = matches!(
                    self.state,
                    StatePhase::Established
                        | StatePhase::HopRejected
                        | StatePhase::InvalidReply
                        | StatePhase::CryptoFailed
                );
                if terminal {
                    Ok(Some(outcome))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Drops the state machine and zeroizes its secrets.
    pub fn cancel(mut self) -> ShortBuildOutcome {
        self.contexts.clear();
        ShortBuildOutcome::Cancelled
    }

    fn process_reply(
        &mut self,
        reply: &[u8],
    ) -> Result<ShortBuildOutcome, ShortBuildConstructionError> {
        let per_hop = per_hop_reply_size();
        let total_expected = per_hop * self.contexts.len();
        if reply.len() != total_expected {
            self.state = StatePhase::InvalidReply;
            return Ok(ShortBuildOutcome::InvalidReply);
        }
        let mut per_hop_replies = Vec::with_capacity(self.contexts.len());
        for (index, _context) in self.contexts.iter().enumerate() {
            let start = index * per_hop;
            let record = &reply[start..start + per_hop];
            let plaintext = self
                .cryptography
                .open_short_reply(record, &self.path.creator_static_key);
            match plaintext {
                Ok(plaintext_bytes) => {
                    per_hop_replies.push(PerHopReply {
                        hop_index: index as HopIndex,
                        plaintext: Zeroizing::new(plaintext_bytes.to_vec()),
                    });
                }
                Err(BuildCryptographyError::AuthenticationFailed)
                | Err(BuildCryptographyError::InvalidDhResult) => {
                    self.state = StatePhase::HopRejected;
                    return Ok(ShortBuildOutcome::HopRejected {
                        hop_index: index as HopIndex,
                        reply_code: ShortResponseCode::Rejected,
                    });
                }
                Err(_error) => {
                    self.state = StatePhase::CryptoFailed;
                    return Ok(ShortBuildOutcome::CryptoFailed);
                }
            }
        }
        self.state = StatePhase::Established;
        Ok(ShortBuildOutcome::Established {
            slot: self.path.creator_tunnel_id,
            per_hop_replies,
        })
    }
}

fn per_hop_reply_size() -> usize {
    // Each per-hop reply record is wrapped in its own 32 byte
    // ephemeral pubkey + 16 byte nonce + 202 byte plaintext + 16
    // byte AEAD tag.
    EPHEMERAL_KEY_LEN + NONCE_LEN + SHORT_REPLY_PLAINTEXT_SIZE + TAG_LEN
}

/// A built short tunnel-build message.
///
/// The struct is the canonical output of
/// [`ShortBuildStateMachine::prepare`]. The bytes inside are a
/// concatenation of `count` 218-byte short records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShortTunnelBuildMessage {
    /// The I2NP message kind.
    pub kind: BuildRequestKind,
    /// Layout the message uses.
    pub layout: BuildRecordLayout,
    /// Sealed record bytes (`count * 218` for short records).
    pub records: Vec<u8>,
    /// Per-attempt message identifier (also used as the I2NP
    /// header identifier).
    pub nonce: u32,
}

fn build_request_record(
    creator_tunnel_id: TunnelId,
    hop: &HopSpec,
    path: &ShortBuildPath,
    index: usize,
) -> Result<ShortRequestRecord, ShortBuildConstructionError> {
    let next_tunnel_hash = if index + 1 < path.hops.len() {
        path.hops[index + 1].router_hash
    } else {
        creator_tunnel_id_hash(creator_tunnel_id)
    };
    let next_tunnel = tunnel_id_from_hash(&next_tunnel_hash);
    let next_router = if index + 1 < path.hops.len() {
        path.hops[index + 1].router_hash
    } else {
        // Final hop returns to the creator's tunnel id.
        path.hops[index].router_hash
    };
    let record = ShortRequestRecord::try_new(
        creator_tunnel_id,
        next_tunnel,
        next_router,
        hop.role,
        LayerEncryptionType::EciesAeadOnly,
        path.request_time,
        path.expiration_ms,
        path.next_message_id,
        path.options.clone(),
    )?;
    Ok(record)
}

fn tunnel_id_from_hash(hash: &Hash) -> TunnelId {
    let bytes = hash.as_bytes();
    let id = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    TunnelId::new(if id == 0 { 1 } else { id }).expect("nonzero tunnel id")
}

fn creator_tunnel_id_hash(tunnel_id: TunnelId) -> Hash {
    let bytes = tunnel_id.get().to_be_bytes();
    let mut hash_bytes = [0_u8; 32];
    hash_bytes[0..4].copy_from_slice(&bytes);
    // Fill the rest deterministically to produce a stable test hash.
    for (idx, slot) in hash_bytes[4..].iter_mut().enumerate() {
        *slot = (idx as u8).wrapping_mul(7);
    }
    Hash::from_bytes(hash_bytes)
}

fn ephemeral_seed_for_hop<R: CryptoRng + rand_core::RngCore>(
    rng: &mut R,
    attempt_id: u64,
    hop_index: u8,
) -> Result<HopCryptoSeed, ShortBuildConstructionError> {
    let mut buffer = Zeroizing::new([0_u8; SHORT_REQUEST_KEY_LEN]);
    // Use the attempt id and hop index to derive a deterministic
    // per-hop seed mixed with the caller's RNG so local simulation
    // is reproducible while preserving unilateral entropy in
    // production. The high-entropy output is what the cryptography
    // primitive consumes; the attempt id is only mixed in.
    let mut mixer = Zeroizing::new([0_u8; SHORT_REQUEST_KEY_LEN]);
    for (idx, byte) in attempt_id.to_be_bytes().iter().enumerate() {
        mixer[idx] = *byte;
    }
    mixer[8] = hop_index;
    let mut fresh = Zeroizing::new([0_u8; SHORT_REQUEST_KEY_LEN]);
    rng.try_fill_bytes(&mut *fresh)
        .map_err(|_| ShortBuildConstructionError::CryptographyUnavailable)?;
    for (idx, byte) in buffer.as_mut().iter_mut().enumerate() {
        *byte = mixer[idx] ^ fresh[idx];
    }
    Ok(HopCryptoSeed::from_bytes(*buffer))
}

fn extract_ephemeral_pub(record: &[u8]) -> [u8; EPHEMERAL_KEY_LEN] {
    let mut bytes = [0_u8; EPHEMERAL_KEY_LEN];
    bytes.copy_from_slice(&record[..EPHEMERAL_KEY_LEN]);
    bytes
}

fn ephemeral_priv_digest(seed: &[u8; SHORT_REQUEST_KEY_LEN]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(seed);
    let output = hasher.finalize();
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(&output);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    use i2pr_proto::Hash;
    use rand_chacha::ChaCha8Rng;
    use rand_core::{RngCore, SeedableRng};

    fn build_path(rng_seed: u64) -> ShortBuildPath {
        let mut hops = Vec::new();
        for value in 1_u8..=2 {
            let mut bytes = [0_u8; 32];
            for (idx, byte) in bytes.iter_mut().enumerate() {
                *byte = value.wrapping_add(idx as u8);
            }
            let mut priv_seed = [0_u8; SHORT_REQUEST_KEY_LEN];
            for (idx, byte) in priv_seed.iter_mut().enumerate() {
                *byte = ((value as usize * 31 + idx) % 251) as u8;
            }
            hops.push(HopSpec::new(
                Hash::from_bytes(bytes),
                // deterministic non-zero static key per hop
                priv_seed_for_pub(value),
                HopRole::Participant,
                HopCryptoSeed::from_bytes(priv_seed),
            ));
        }
        // First hop is gateway
        hops[0].role = HopRole::InboundGateway;
        // Last hop is endpoint
        let last = hops.len() - 1;
        hops[last].role = HopRole::OutboundEndpoint;
        let mut creator_priv = [0_u8; EPHEMERAL_KEY_LEN];
        let mut creator_seed = ChaCha8Rng::seed_from_u64(rng_seed);
        creator_seed.fill_bytes(&mut creator_priv);
        ShortBuildPath {
            attempt_id: BuildAttemptId::new(rng_seed),
            direction: TunnelDirection::Outbound,
            creator_tunnel_id: TunnelId::new(0xABCD).expect("id"),
            creator_static_key: creator_priv,
            hops,
            request_time: Date::from_millis(1),
            expiration_ms: 60_000,
            next_message_id: 0x1234_5678,
            options: BuildOptions::empty(),
        }
    }

    fn priv_seed_for_pub(value: u8) -> [u8; 32] {
        let mut bytes = [0_u8; 32];
        let mut cursor = value as usize;
        for byte in bytes.iter_mut() {
            cursor = (cursor.wrapping_mul(17).wrapping_add(11)) % 251;
            *byte = cursor as u8;
        }
        bytes
    }

    #[test]
    fn validation_rejects_zero_static_key() {
        let mut path = build_path(1);
        path.hops[0].static_encryption_key = [0_u8; 32];
        let outcome = path.validate();
        assert!(matches!(
            outcome,
            Err(ShortBuildConstructionError::InvalidPath { .. })
        ));
    }

    #[test]
    fn prepare_emits_canonical_message_length() {
        let path = build_path(2);
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let message = machine.prepare(&mut rng).expect("prepare");
        assert_eq!(message.kind, BuildRequestKind::ShortTunnelBuild);
        assert_eq!(message.layout, BuildRecordLayout::Short);
        assert_eq!(message.records.len(), 2 * SHORT_BUILD_RECORD_SIZE);
        assert!(matches!(machine.state, StatePhase::ReadyForDelivery));
    }

    #[test]
    fn cancel_after_prepare_returns_cancelled_outcome() {
        let path = build_path(4);
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(5);
        let _ = machine.prepare(&mut rng);
        let outcome = machine.cancel();
        assert_eq!(outcome, ShortBuildOutcome::Cancelled);
    }

    #[test]
    fn deadline_event_marks_state_timed_out() {
        let path = build_path(6);
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let _ = machine.prepare(&mut rng);
        let _ = machine.deliver_action(ShortTunnelBuildMessage {
            kind: BuildRequestKind::ShortTunnelBuild,
            layout: BuildRecordLayout::Short,
            records: Vec::new(),
            nonce: 0,
        });
        machine.mark_dispatched().expect("dispatch");
        let result = machine
            .handle_event(BuildEvent::DeadlineExceeded)
            .expect("event");
        assert_eq!(result, Some(ShortBuildOutcome::TimedOut));
    }

    #[test]
    fn prepare_rejects_oversized_path() {
        let mut path = build_path(8);
        // Add too many hops — exceeds MAX_BUILD_RECORDS.
        for value in 0..30 {
            let mut bytes = [0_u8; 32];
            for (idx, byte) in bytes.iter_mut().enumerate() {
                *byte = ((value + idx) % 251) as u8;
            }
            let priv_seed = [0xAB_u8; SHORT_REQUEST_KEY_LEN];
            path.hops.push(HopSpec::new(
                Hash::from_bytes(bytes),
                bytes,
                HopRole::Participant,
                HopCryptoSeed::from_bytes(priv_seed),
            ));
        }
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(9);
        let result = machine.prepare(&mut rng);
        assert!(result.is_err());
    }

    #[test]
    fn state_machine_starts_in_prepared_phase() {
        let path = build_path(11);
        let machine = ShortBuildStateMachine::new(path, 60_000);
        assert_eq!(machine.current_state_label(), "Prepared");
    }

    #[test]
    fn cancel_after_prepare_zeros_contexts() {
        let path = build_path(12);
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(13);
        let _ = machine.prepare(&mut rng).expect("prepare");
        let outcome = machine.cancel();
        assert_eq!(outcome, ShortBuildOutcome::Cancelled);
    }

    #[test]
    fn build_event_delivery_accepted_then_deadline_yields_timed_out() {
        let path = build_path(14);
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(15);
        let _ = machine.prepare(&mut rng).expect("prepare");
        let _ = machine.deliver_action(ShortTunnelBuildMessage {
            kind: BuildRequestKind::ShortTunnelBuild,
            layout: BuildRecordLayout::Short,
            records: Vec::new(),
            nonce: 0,
        });
        machine.mark_dispatched().expect("dispatch");
        let result = machine
            .handle_event(BuildEvent::DeadlineExceeded)
            .expect("event");
        assert_eq!(result, Some(ShortBuildOutcome::TimedOut));
        // A second terminal event must error.
        let result = machine.handle_event(BuildEvent::DeadlineExceeded);
        assert!(result.is_err());
    }

    #[test]
    fn build_event_invalid_reply_for_wrong_size_reply() {
        let path = build_path(16);
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(17);
        let _ = machine.prepare(&mut rng).expect("prepare");
        let _ = machine.deliver_action(ShortTunnelBuildMessage {
            kind: BuildRequestKind::ShortTunnelBuild,
            layout: BuildRecordLayout::Short,
            records: Vec::new(),
            nonce: 0,
        });
        machine.mark_dispatched().expect("dispatch");
        let result = machine
            .handle_event(BuildEvent::BuildReply {
                reply: Zeroizing::new(vec![0_u8; 7]),
            })
            .expect("event");
        assert_eq!(result, Some(ShortBuildOutcome::InvalidReply));
    }
}
