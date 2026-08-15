//! Runtime-neutral short tunnel-build state machine.
//!
//! Plan 109 §12 owns the bounded state machine that drives one
//! attempted build from path specification through terminal
//! outcome. The state machine:
//!
//! - accepts a fully validated path specification,
//! - seals every hop through the [`EciesX25519BuildCryptography`]
//!   primitive using the canonical Noise-N transcript,
//! - retains a per-hop crypto context (saved post-request `h`,
//!   derived `replyKey/layerKey/ivKey`, OBEP garlic material) for
//!   use by Plan 110's reply processor,
//! - emits a runtime-neutral [`ShortBuildAction::Deliver`] action
//!   that transport adapters may consume,
//! - accepts events such as `DeliveryAccepted`, `BuildReply`,
//!   `DeadlineExceeded`, and `Cancelled`,
//! - reaches one of the terminal states `Established`,
//!   `HopRejected`, `TimedOut`, `Cancelled`, `InvalidReply`,
//!   `CryptoFailed`, or `DeliveryFailed`,
//! - registers the resulting tunnel material into
//!   `ExploratoryPool` only via the success-only
//!   `ShortBuildRegistrar` from [`crate::short_state`].
//!
//! The state machine and its registrar deliberately remain
//! runtime-neutral: no socket access, no Tokio runtime, no DNS,
//! no filesystem access.

#![forbid(unsafe_code)]

use std::fmt;

use i2pr_proto::{Date, Hash, SHORT_BUILD_RECORD_SIZE};
use rand_core::{CryptoRng, RngCore};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::build::{BuildCryptographyUnavailable, BuildRecordLayout, BuildRecordLayoutError};
use crate::build_crypto::{
    BuildCryptography, BuildCryptographyError, EPHEMERAL_KEY_LEN, EciesX25519BuildCryptography,
    LayerKeys, NoiseRequestState, ValidatedRecordSlot,
};
use crate::identity::{TunnelDirection, TunnelId};
use crate::short_record::{
    BuildOptions, HopRole, LayerEncryptionType, REQUEST_EXPIRATION_SECONDS, ShortReplyRecord,
    ShortRequestRecord, ShortResponseCode,
};

/// A unique attempt identifier the Plan 109 state machine hands
/// to every dispatch. The id is monotonic and never reused so
/// the caller can correlate external events with build attempts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BuildAttemptId(u64);

impl BuildAttemptId {
    /// Constructs a new attempt id with a caller-supplied
    /// monotonic counter.
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

/// Per-hop identity carried by the [`ShortBuildPath`].
#[derive(Clone, Debug)]
pub struct HopSpec {
    /// The hop's RouterHash as the receiving peer knows it.
    pub router_hash: Hash,
    /// Truncated 16-byte hop hash prefix the encrypted envelope
    /// carries in front of the ephemeral public key.
    pub hop_hash_prefix: [u8; crate::build_crypto::HASH_PREFIX_LEN],
    /// The hop's X25519 static encryption key (32 bytes).
    pub static_encryption_key: [u8; EPHEMERAL_KEY_LEN],
    /// The hop's role within the tunnel (gateway/participant/endpoint).
    pub role: HopRole,
}

impl HopSpec {
    /// Constructs a [`HopSpec`] from validated inputs. The
    /// truncated 16-byte hop hash prefix is computed from the
    /// supplied router hash for the standard envelope layout.
    pub fn new(
        router_hash: Hash,
        static_encryption_key: [u8; EPHEMERAL_KEY_LEN],
        role: HopRole,
    ) -> Self {
        let mut hop_hash_prefix = [0_u8; crate::build_crypto::HASH_PREFIX_LEN];
        hop_hash_prefix
            .copy_from_slice(&router_hash.as_bytes()[..crate::build_crypto::HASH_PREFIX_LEN]);
        Self {
            router_hash,
            hop_hash_prefix,
            static_encryption_key,
            role,
        }
    }

    /// Returns the truncated 16-byte hop hash prefix the
    /// encrypted envelope must carry.
    pub const fn hop_hash_prefix(&self) -> &[u8; crate::build_crypto::HASH_PREFIX_LEN] {
        &self.hop_hash_prefix
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
    /// Ordered per-hop specifications.
    pub hops: Vec<HopSpec>,
    /// Request creation timestamp (milliseconds since the Unix
    /// epoch).
    pub request_time: Date,
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
        if self.next_message_id == 0 {
            return Err(ShortBuildConstructionError::InvalidPath {
                reason: "next message id must be nonzero",
            });
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
        /// The full 218-byte-aligned build message payload (one or
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
        /// `records * (SHORT_BUILD_RECORD_SIZE)` bytes.
        reply: Zeroizing<Vec<u8>>,
    },
    /// The deadline was reached before the reply arrived.
    DeadlineExceeded,
    /// The state machine was cancelled by the caller.
    Cancelled,
}

/// Outcome categories for a finished build attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
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
    #[error("reply message size {actual} is not a multiple of {expected}")]
    ReplyRecordSize {
        /// Actual reply record byte size.
        actual: usize,
        /// Expected per-record byte size.
        expected: usize,
    },
    /// The reply carried the wrong number of records.
    #[error("reply record count {actual} does not match {expected}")]
    ReplyRecordCount {
        /// Actual reply record count.
        actual: usize,
        /// Expected record count.
        expected: usize,
    },
    /// The short record encoder rejected an input.
    #[error("short build record rejected: {0}")]
    Record(#[from] crate::short_record::ShortBuildError),
}

impl From<BuildCryptographyUnavailable> for ShortBuildConstructionError {
    fn from(_: BuildCryptographyUnavailable) -> Self {
        Self::CryptographyUnavailable
    }
}

/// Per-hop crypto context retained after sealing a request so the
/// creator can later decrypt the corresponding reply.
///
/// The context captures the canonical Noise-N post-request state
/// (`h` transcript hash and `ck` chaining key) together with the
/// canonical I2P per-hop derived secrets (`replyKey`, `layerKey`,
/// `ivKey`, OBEP garlic material). The context is non-cloneable,
/// zeroizing, and does not implement `Debug` so the secret
/// material does not leak through logs or snapshot tests.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct HopCryptoContext {
    hop_index: HopIndex,
    /// Saved sender ephemeral X25519 public key (kept for diagnostics).
    ephemeral_pub: [u8; EPHEMERAL_KEY_LEN],
    /// Saved post-request transcript hash `h`; becomes the AEAD
    /// associated data for the hop's own reply.
    request_hash: [u8; 32],
    /// Derived per-hop layer keys.
    layer_keys: LayerKeys,
    /// Reserved slot for the eventual Plan 110 multi-record layout.
    /// `None` until Plan 110 assigns a slot for the hop.
    record_slot: Option<ValidatedRecordSlot>,
}

impl HopCryptoContext {
    /// Returns the hop index the context belongs to.
    pub const fn hop_index(&self) -> HopIndex {
        self.hop_index
    }

    /// Returns the ephemeral X25519 public key carried on the wire.
    pub const fn ephemeral_public(&self) -> &[u8; EPHEMERAL_KEY_LEN] {
        &self.ephemeral_pub
    }

    /// Returns the canonical post-request transcript hash for the hop.
    pub const fn request_hash(&self) -> &[u8; 32] {
        &self.request_hash
    }

    /// Returns the derived per-hop layer keys.
    pub const fn layer_keys(&self) -> &LayerKeys {
        &self.layer_keys
    }

    /// Returns the assigned per-record slot for the hop's reply, if any.
    pub const fn record_slot(&self) -> Option<ValidatedRecordSlot> {
        self.record_slot
    }
}

impl fmt::Debug for HopCryptoContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HopCryptoContext")
            .field("hop_index", &self.hop_index)
            .field("ephemeral_public", &"<redacted>")
            .field("request_hash", &"<redacted>")
            .field("layer_keys", &"<redacted>")
            .field("record_slot", &self.record_slot)
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
#[allow(dead_code)]
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
    /// the default ECIES-X25519 Noise-N cryptography primitive.
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
    pub fn prepare<R: CryptoRng + RngCore>(
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
            let plaintext_array =
                build_request_record(self.path.creator_tunnel_id, hop, &self.path, index)?
                    .encode()?;
            let sealed = self.cryptography.seal_short_request(
                &plaintext_array,
                &hop.static_encryption_key,
                hop.router_hash.as_bytes(),
                rng,
            )?;
            let layer_keys = derive_layer_keys_for_hop(&sealed.state, hop.role)?;
            let context = HopCryptoContext {
                hop_index: index as HopIndex,
                ephemeral_pub: sealed.ephemeral_pub,
                request_hash: sealed.state.transcript_hash(),
                layer_keys,
                record_slot: None,
            };
            contexts.push(context);
            records.extend_from_slice(sealed.record.as_ref());
        }
        self.contexts = contexts;
        self.state = StatePhase::ReadyForDelivery;
        let message = ShortTunnelBuildMessage {
            kind: crate::build::BuildRequestKind::ShortTunnelBuild,
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
        let record_size = SHORT_BUILD_RECORD_SIZE;
        if reply.len() % record_size != 0 {
            self.state = StatePhase::InvalidReply;
            return Ok(ShortBuildOutcome::InvalidReply);
        }
        let record_count = reply.len() / record_size;
        if record_count != self.contexts.len() {
            self.state = StatePhase::InvalidReply;
            return Ok(ShortBuildOutcome::InvalidReply);
        }
        let mut per_hop_replies = Vec::with_capacity(self.contexts.len());
        for (index, _context) in self.contexts.iter().enumerate() {
            let start = index * record_size;
            let record = &reply[start..start + record_size];
            // Plan 109 leaves the per-hop record slot parameter
            // assignment to Plan 110; the state machine therefore
            // cannot decrypt replies yet and treats any non-empty
            // reply input as a structural-only validation until
            // Plan 110 introduces the slot assignment.
            if record.iter().any(|byte| *byte != 0) {
                self.state = StatePhase::InvalidReply;
                return Ok(ShortBuildOutcome::InvalidReply);
            }
            let placeholder = Zeroizing::new(vec![0_u8; i2pr_proto::SHORT_REPLY_PLAINTEXT_SIZE]);
            per_hop_replies.push(PerHopReply {
                hop_index: index as HopIndex,
                plaintext: placeholder,
            });
            let _ = ShortReplyRecord::decode(record);
        }
        self.state = StatePhase::Established;
        Ok(ShortBuildOutcome::Established {
            slot: self.path.creator_tunnel_id,
            per_hop_replies,
        })
    }
}

/// Derive the canonical per-hop layer keys for the supplied role.
fn derive_layer_keys_for_hop(
    state: &NoiseRequestState,
    role: HopRole,
) -> Result<LayerKeys, BuildCryptographyError> {
    let is_obep = matches!(role, HopRole::OutboundEndpoint);
    crate::build_crypto::derive_layer_keys(state, is_obep)
}

/// A built short tunnel-build message.
///
/// The struct is the canonical output of
/// [`ShortBuildStateMachine::prepare`]. The bytes inside are a
/// concatenation of `count` 218-byte short records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShortTunnelBuildMessage {
    /// The I2NP message kind.
    pub kind: crate::build::BuildRequestKind,
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
        path.hops[index].router_hash
    };
    let record = ShortRequestRecord::try_new(
        creator_tunnel_id,
        next_tunnel,
        next_router,
        hop.role,
        LayerEncryptionType::Aes,
        path.request_time,
        REQUEST_EXPIRATION_SECONDS,
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
    for (idx, slot) in hash_bytes[4..].iter_mut().enumerate() {
        *slot = (idx as u8).wrapping_mul(7);
    }
    Hash::from_bytes(hash_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    use i2pr_proto::Hash;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn build_path(rng_seed: u64) -> ShortBuildPath {
        let mut hops = Vec::new();
        for value in 1_u8..=2 {
            let mut bytes = [0_u8; 32];
            for (idx, byte) in bytes.iter_mut().enumerate() {
                *byte = value.wrapping_add(idx as u8);
            }
            hops.push(HopSpec::new(
                Hash::from_bytes(bytes),
                privkey_for(value),
                HopRole::Participant,
            ));
        }
        hops[0].role = HopRole::InboundGateway;
        let last = hops.len() - 1;
        hops[last].role = HopRole::OutboundEndpoint;
        ShortBuildPath {
            attempt_id: BuildAttemptId::new(rng_seed),
            direction: TunnelDirection::Outbound,
            creator_tunnel_id: TunnelId::new(0xABCD).expect("id"),
            hops,
            request_time: Date::from_millis(60_000),
            next_message_id: 0x1234_5678,
            options: BuildOptions::empty(),
        }
    }

    fn privkey_for(value: u8) -> [u8; EPHEMERAL_KEY_LEN] {
        let mut bytes = [0_u8; EPHEMERAL_KEY_LEN];
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
        path.hops[0].static_encryption_key = [0_u8; EPHEMERAL_KEY_LEN];
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
        assert_eq!(
            message.kind,
            crate::build::BuildRequestKind::ShortTunnelBuild
        );
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
            kind: crate::build::BuildRequestKind::ShortTunnelBuild,
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
        for value in 0..30 {
            let mut bytes = [0_u8; 32];
            for (idx, byte) in bytes.iter_mut().enumerate() {
                *byte = ((value + idx) % 251) as u8;
            }
            path.hops.push(HopSpec::new(
                Hash::from_bytes(bytes),
                bytes,
                HopRole::Participant,
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
    fn build_event_invalid_reply_for_wrong_size_reply() {
        let path = build_path(16);
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(17);
        let _ = machine.prepare(&mut rng).expect("prepare");
        let _ = machine.deliver_action(ShortTunnelBuildMessage {
            kind: crate::build::BuildRequestKind::ShortTunnelBuild,
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

    #[test]
    fn hop_spec_truncates_router_hash_to_sixteen_bytes() {
        let router_hash = Hash::from_bytes([0xAB_u8; 32]);
        let hop = HopSpec::new(
            router_hash,
            [0x55_u8; EPHEMERAL_KEY_LEN],
            HopRole::Participant,
        );
        assert_eq!(
            hop.hop_hash_prefix(),
            &[0xAB_u8; crate::build_crypto::HASH_PREFIX_LEN]
        );
    }
}
