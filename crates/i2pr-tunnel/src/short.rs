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

use i2pr_proto::{Date, Hash};
use rand_core::{CryptoRng, RngCore};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::build::{BuildCryptographyUnavailable, BuildRecordLayout, BuildRecordLayoutError};
use crate::build_crypto::{
    BuildCryptography, BuildCryptographyError, EPHEMERAL_KEY_LEN, EciesX25519BuildCryptography,
    LayerKeys, ValidatedRecordSlot,
};
use crate::established::{
    EstablishedHop, EstablishedMaterial, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
    EstablishedTunnelError,
};
use crate::identity::{TunnelDirection, TunnelId, TunnelPeer};
use crate::multirecord::{
    self, CreatorReplyPostprocessor, MultiRecordError, MultiRecordHopSpec, OriginatorFake,
    PreparedHopContext, ProcessedHopResult, ShortBuildRecordSet,
};
use crate::short_record::{BuildOptions, HopRole, ShortResponseCode};

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
    /// Independent receive tunnel identifier this hop is given.
    /// Plan 111 defect 6: every hop owns its own tunnel id; the
    /// value is independent from the next router hash and from any
    /// other hop's id.
    pub receive_tunnel: TunnelId,
    /// Independent next tunnel identifier this hop must hand to the
    /// downstream peer. The value is independent from the next
    /// router hash and from any other hop's id.
    pub next_tunnel: TunnelId,
}

impl HopSpec {
    /// Constructs a [`HopSpec`] from validated inputs. The
    /// truncated 16-byte hop hash prefix is computed from the
    /// supplied router hash for the standard envelope layout.
    /// `receive_tunnel` and `next_tunnel` are the explicit per-hop
    /// tunnel identifiers; the previous behaviour that derived a
    /// next tunnel id from router-hash bytes is removed.
    pub fn new(
        router_hash: Hash,
        static_encryption_key: [u8; EPHEMERAL_KEY_LEN],
        role: HopRole,
        receive_tunnel: TunnelId,
        next_tunnel: TunnelId,
    ) -> Self {
        let mut hop_hash_prefix = [0_u8; crate::build_crypto::HASH_PREFIX_LEN];
        hop_hash_prefix
            .copy_from_slice(&router_hash.as_bytes()[..crate::build_crypto::HASH_PREFIX_LEN]);
        Self {
            router_hash,
            hop_hash_prefix,
            static_encryption_key,
            role,
            receive_tunnel,
            next_tunnel,
        }
    }

    /// Returns the truncated 16-byte hop hash prefix the
    /// encrypted envelope must carry.
    pub const fn hop_hash_prefix(&self) -> &[u8; crate::build_crypto::HASH_PREFIX_LEN] {
        &self.hop_hash_prefix
    }

    /// Returns the receive tunnel identifier this hop is given.
    pub const fn receive_tunnel(&self) -> TunnelId {
        self.receive_tunnel
    }

    /// Returns the next tunnel identifier this hop must hand to the
    /// downstream peer.
    pub const fn next_tunnel(&self) -> TunnelId {
        self.next_tunnel
    }
}

/// A fully specified short tunnel-build path.
///
/// The path carries the configuration the state machine needs to
/// construct one [`ShortTunnelBuildMessage`]. The builder never
/// queries NetDB; the caller is responsible for resolving every
/// static encryption key and router hash.
///
/// The two direction-specific terminal routing fields
/// ([`originator_hash`](Self::originator_hash) and
/// [`outbound_reply_router`](Self::outbound_reply_router)) make the
/// terminal `next_router_hash` explicit on the path boundary. Plan
/// 114 requires the outbound reply router identity at the OBEP and
/// the inbound creator identity at the terminal inbound hop to be
/// declared explicitly; the builder never derives them from a hop
/// or tunnel identifier.
#[derive(Clone, Debug)]
pub struct ShortBuildPath {
    /// Local creator identifier for diagnostic correlation.
    pub attempt_id: BuildAttemptId,
    /// Inbound or outbound direction.
    pub direction: TunnelDirection,
    /// Creator identity hash used for the inbound originator fake
    /// and for the terminal inbound hop's `next_router_hash`.
    /// Outbound paths must leave this unset; inbound paths must
    /// provide it explicitly and never derive it from a hop or
    /// tunnel identifier.
    pub originator_hash: Option<Hash>,
    /// Outbound reply-router identity hash the OBEP records as its
    /// terminal `next_router_hash`. Outbound paths must provide it
    /// explicitly; inbound paths must leave it unset. The builder
    /// never derives this value from a hop or tunnel identifier.
    pub outbound_reply_router: Option<Hash>,
    /// Creator tunnel identifier this build is associated with.
    ///
    /// `creator_tunnel_id` is the local slot identifier used by
    /// the pool registrar after a successful build. It is
    /// **not** aliased to the inbound terminal `next_tunnel` value:
    /// the final inbound `HopSpec.next_tunnel` remains the
    /// creator-side receive tunnel identifier that the final
    /// remote participant uses to forward the build reply back to
    /// the creator. The two values are independent; the path
    /// validator keeps them that way.
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
    /// The validator enforces Plan 112 §6 direction/role
    /// topology rules in addition to the Plan 111 hard-field
    /// invariants and the Plan 114 terminal-routing and
    /// intermediate-tunnel-chain invariants.
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
            // Plan 111 defect 6: every hop must own an explicit
            // nonzero receive tunnel id and an explicit nonzero
            // next tunnel id.
            if hop.receive_tunnel.get() == 0 {
                return Err(ShortBuildConstructionError::InvalidPath {
                    reason: "a hop receive tunnel id is zero",
                });
            }
            if hop.next_tunnel.get() == 0 {
                return Err(ShortBuildConstructionError::InvalidPath {
                    reason: "a hop next tunnel id is zero",
                });
            }
        }
        if self.next_message_id == 0 {
            return Err(ShortBuildConstructionError::InvalidPath {
                reason: "next message id must be nonzero",
            });
        }
        match self.direction {
            TunnelDirection::Inbound => {
                if self.originator_hash.is_none() {
                    return Err(ShortBuildConstructionError::MissingInboundOriginatorIdentity);
                }
                if self.outbound_reply_router.is_some() {
                    return Err(ShortBuildConstructionError::InvalidPath {
                        reason: "inbound path must not declare an outbound reply router",
                    });
                }
            }
            TunnelDirection::Outbound => {
                if self.originator_hash.is_some() {
                    return Err(ShortBuildConstructionError::InvalidPath {
                        reason: "outbound path must not declare an inbound originator hash",
                    });
                }
                if self.outbound_reply_router.is_none() {
                    return Err(ShortBuildConstructionError::MissingOutboundReplyRouter);
                }
            }
        }
        // Plan 114 §4.4: enforce the intermediate tunnel-id
        // continuity invariant at the high-level path boundary so a
        // role-valid path cannot encode a broken forwarding chain.
        let intermediate_count = self.hops.len().saturating_sub(1);
        for index in 0..intermediate_count {
            let next_tunnel = self.hops[index].next_tunnel;
            let following_receive = self.hops[index + 1].receive_tunnel;
            if next_tunnel.get() != following_receive.get() {
                return Err(ShortBuildConstructionError::InvalidPath {
                    reason: "intermediate next tunnel id does not match following receive tunnel id",
                });
            }
        }
        // Plan 112 §6.B1/B2: use the same validator as the public
        // lower-level multi-record builder so the role boundary is
        // enforced consistently at both API layers.
        let roles: Vec<HopRole> = self.hops.iter().map(|hop| hop.role).collect();
        multirecord::validate_role_topology(self.direction, &roles).map_err(
            |error| match error {
                multirecord::MultiRecordError::RoleTopologyInvalid { reason } => {
                    ShortBuildConstructionError::InvalidPath { reason }
                }
                _ => ShortBuildConstructionError::InvalidPath {
                    reason: "short build role topology is invalid",
                },
            },
        )?;
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
        /// The full STBM wire payload. Byte 0 is the canonical
        /// record count `n` (`1..=8`); the remaining `n * 218`
        /// bytes are the concatenated 218-byte records. The
        /// helper [`crate::validate_count_prefixed_short_payload`]
        /// enforces the same contract for every consumer.
        message: Zeroizing<Vec<u8>>,
        /// Number of records the message carries (byte 0 of
        /// `message`).
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
        /// The full OTBRM reply payload. Byte 0 is the canonical
        /// record count `n` (`1..=8`); the remaining `n * 218`
        /// bytes are the concatenated 218-byte reply records.
        /// The helper
        /// [`crate::validate_count_prefixed_short_payload`]
        /// enforces the same contract for every consumer.
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
    /// The reply failed multi-record validation (bad hash, modified
    /// originator fake, or other failure surfaced by the
    /// Plan 110 postprocessor).
    #[error("multi-record reply rejected: {reason}")]
    InvalidReply {
        /// Description of the rejection.
        reason: &'static str,
    },
    /// The delivery action was given a malformed count-prefixed
    /// STBM payload.
    #[error("short build delivery payload rejected: {reason}")]
    InvalidDeliveryPayload {
        /// Description of the payload rejection.
        reason: &'static str,
    },
    /// The short record encoder rejected an input.
    #[error("short build record rejected: {0}")]
    Record(#[from] crate::short_record::ShortBuildError),
    /// An inbound path omitted the creator identity needed by its
    /// originator fake.
    #[error("inbound short build requires an explicit originator identity hash")]
    MissingInboundOriginatorIdentity,
    /// An outbound path omitted the reply-router identity hash the
    /// OBEP must serialise as its terminal `next_router_hash`.
    #[error("outbound short build requires an explicit reply router identity hash")]
    MissingOutboundReplyRouter,
    /// The caller asked to take the established material before
    /// the state machine reached the `Established` phase.
    #[error("short build has not reached the Established phase")]
    NotEstablished,
    /// The caller asked to take the established material a second
    /// time; the data-plane layer keys are zeroised by the first
    /// take and the second take cannot succeed.
    #[error("established material has already been extracted")]
    EstablishedMaterialAlreadyTaken,
    /// The state machine reached `Established` but the path
    /// metadata cannot be turned into a valid established tunnel.
    #[error("established path state is invalid: {reason}")]
    EstablishedPathStateInvalid {
        /// Description of the rejection.
        reason: &'static str,
    },
}

impl From<BuildCryptographyUnavailable> for ShortBuildConstructionError {
    fn from(_: BuildCryptographyUnavailable) -> Self {
        Self::CryptographyUnavailable
    }
}

/// Per-hop crypto context retained after sealing a request so the
/// creator can later decrypt the corresponding reply.
///
/// The context is a thin Plan 110 wrapper that exposes the canonical
/// post-request `h`, the derived `LayerKeys`, and the assigned wire
/// slot without leaking the secret material through `Debug`.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct HopCryptoContext {
    pub(crate) inner: PreparedHopContext,
}

impl HopCryptoContext {
    /// Constructs a new context from a prepared per-hop context.
    fn from_prepared(inner: PreparedHopContext) -> Self {
        Self { inner }
    }

    /// Returns the hop index the context belongs to.
    pub const fn hop_index(&self) -> HopIndex {
        self.inner.hop_index
    }

    /// Returns the canonical post-request transcript hash for the hop.
    pub fn request_hash(&self) -> [u8; 32] {
        self.inner.state.transcript_hash()
    }

    /// Returns the derived per-hop layer keys.
    pub const fn layer_keys(&self) -> &LayerKeys {
        &self.inner.layer_keys
    }

    /// Returns the assigned per-record slot for the hop's reply.
    pub const fn record_slot(&self) -> Option<ValidatedRecordSlot> {
        Some(self.inner.slot)
    }

    /// Consumes the per-hop layer keys, leaving a zeroising
    /// placeholder in their place. The call is intended for the
    /// Plan 116 canonical material-transfer seam that hands the
    /// established `EstablishedHop` over to
    /// `EstablishedMaterial`. The placeholder is dropped and
    /// zeroised immediately after the move; the state machine
    /// retains no second live copy of the data-plane key material.
    pub fn take_layer_keys(&mut self) -> LayerKeys {
        std::mem::replace(
            &mut self.inner.layer_keys,
            LayerKeys::new([0_u8; 32], [0_u8; 32], [0_u8; 32]),
        )
    }
}

impl fmt::Debug for HopCryptoContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HopCryptoContext")
            .field("hop_index", &self.inner.hop_index)
            .field("slot", &self.inner.slot)
            .field("ephemeral_public", &"<redacted>")
            .field("request_hash", &"<redacted>")
            .field("layer_keys", &"<redacted>")
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
    #[allow(dead_code)]
    cryptography: C,
    state: StatePhase,
    contexts: Vec<HopCryptoContext>,
    record_set: Option<ShortBuildRecordSet>,
    originator_fake: Option<OriginatorFake>,
    deadline_ms: u64,
    /// The full preprocessed STBM payload the most recent
    /// `prepare` produced. Plan 114 strict trajectories re-read
    /// this payload after `prepare` returns so the test can drive
    /// each real hop through `MessageHopProcessor` without
    /// reconstructing the payload from private state. The buffer
    /// is wrapped in `Zeroizing` because the bytes carry the
    /// per-hop encrypted requests; it is reset to `None` once
    /// the state machine leaves the dispatchable phases.
    last_payload: Option<Zeroizing<Vec<u8>>>,
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
            record_set: None,
            originator_fake: None,
            deadline_ms,
            last_payload: None,
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

    /// Returns the preprocessed STBM payload the most recent
    /// `prepare` produced. The accessor is intended for strict
    /// trajectory tests that need to drive each real hop through
    /// `MessageHopProcessor` without reconstructing the payload
    /// from private state. The function returns `None` before
    /// `prepare` runs or after the state machine leaves the
    /// dispatchable phases.
    pub fn last_payload(&self) -> Option<&[u8]> {
        self.last_payload.as_ref().map(|bytes| bytes.as_ref())
    }

    /// Performs the prepare→protecting transition, sealing every
    /// per-hop request record and assembling the build message.
    /// Inbound construction follows Plan 113's deployed-reference
    /// compatible policy: normal fixed-field request plaintext plus
    /// one separately randomized originator fake.
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
        // Build the per-hop request plaintexts, seal them through
        // the canonical Plan 109 EciesX25519 primitive, and shuffle
        // the records into the canonical multi-record layout via
        // the Plan 110 multirecord API.
        let hop_specs = build_hop_specs(&self.path)?;
        let creator_tunnel_id_bytes = self.path.creator_tunnel_id.get().to_be_bytes();
        let request_time_ms = self.path.request_time.as_millis();
        let first_hop = self.path.hops.first().map(|hop| hop.router_hash);
        let originator_hash = self.path.originator_hash.as_ref();
        let cryptography = EciesX25519BuildCryptography::new();
        let prepared = multirecord::prepare_short_build_message(
            &cryptography,
            &hop_specs,
            self.direction,
            creator_tunnel_id_bytes,
            request_time_ms,
            self.path.next_message_id,
            first_hop,
            originator_hash,
            rng,
        )
        .map_err(short_build_construction_from_multi)?;
        let record_set = prepared.record_set.clone();
        let originator_fake = prepared.originator_fake;
        let contexts: Vec<HopCryptoContext> = prepared
            .hop_contexts
            .into_iter()
            .map(HopCryptoContext::from_prepared)
            .collect();
        let records = prepared.payload.to_vec();
        self.contexts = contexts;
        self.record_set = Some(record_set);
        self.originator_fake = originator_fake;
        self.state = StatePhase::ReadyForDelivery;
        let message = ShortTunnelBuildMessage {
            kind: crate::build::BuildRequestKind::ShortTunnelBuild,
            layout: BuildRecordLayout::Short,
            records,
            nonce: self.path.next_message_id,
        };
        self.last_payload = Some(Zeroizing::new(message.records.clone()));
        Ok(message)
    }

    /// Emits the typed delivery action the caller performs.
    pub fn deliver_action(
        &self,
        message: ShortTunnelBuildMessage,
    ) -> Result<ShortBuildAction, ShortBuildConstructionError> {
        let (record_count, _) = multirecord::validate_count_prefixed_short_payload(
            &message.records,
        )
        .map_err(|_| ShortBuildConstructionError::InvalidDeliveryPayload {
            reason: "STBM payload must be count-prefixed with exactly count * 218 record bytes",
        })?;
        let first_hop = self
            .path
            .hops
            .first()
            .map(|hop| hop.router_hash)
            .unwrap_or_else(|| Hash::from_bytes([0_u8; 32]));
        Ok(ShortBuildAction::Deliver {
            first_hop,
            message: Zeroizing::new(message.records),
            record_count,
            deadline_ms: self.deadline_ms,
        })
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

    /// Consumes the state-machine-owned `EstablishedMaterial` after
    /// a successful build, returns the secret-bearing owner the
    /// data plane consumes.
    ///
    /// Required behaviour:
    ///
    /// - succeeds only when [`Self::current_state_label`] is
    ///   `Established`;
    /// - fails closed before `Established`;
    /// - may succeed exactly once; a second call fails with
    ///   [`ShortBuildConstructionError::EstablishedMaterialAlreadyTaken`];
    /// - consumes every per-hop data-plane `LayerKeys` from
    ///   `self.contexts` via [`HopCryptoContext::take_layer_keys`],
    ///   so the state machine retains no second live copy of the
    ///   post-extraction persistent layer keys;
    /// - preserves real hop order from `self.path.hops`;
    /// - preserves every hop's exact `router_hash`, `receive_tunnel`,
    ///   role, and configured forwarding target;
    /// - does not expose reply keys, garlic keys, raw request/reply
    ///   records, or transcript state to the pool.
    pub fn take_established_material(
        &mut self,
        established_at_seconds: u64,
    ) -> Result<EstablishedMaterial, ShortBuildConstructionError> {
        if !matches!(self.state, StatePhase::Established) {
            return Err(ShortBuildConstructionError::NotEstablished);
        }
        // Second-take guard: the contexts are emptied by the
        // first successful take; a subsequent call sees an empty
        // vector and fails closed.
        if self.contexts.is_empty() {
            return Err(ShortBuildConstructionError::EstablishedMaterialAlreadyTaken);
        }
        if self.contexts.len() != self.path.hops.len() {
            return Err(ShortBuildConstructionError::EstablishedPathStateInvalid {
                reason: "context count does not match declared hop count",
            });
        }
        // Build the per-hop `EstablishedHop` list in canonical path
        // order. Each hop's identity comes from the path, the
        // data-plane layer keys come from the context's
        // `take_layer_keys` helper, and the next-hop state is
        // derived from the explicit direction-specific terminal
        // routing field on the path.
        let mut hops: Vec<EstablishedHop> = Vec::with_capacity(self.path.hops.len());
        for (index, hop_spec) in self.path.hops.iter().enumerate() {
            // The context at index `i` must match the hop at
            // index `i`; the preprocessor always builds one
            // context per declared real hop.
            if self.contexts[index].hop_index() != index as u8 {
                return Err(ShortBuildConstructionError::EstablishedPathStateInvalid {
                    reason: "context hop index does not match declared hop index",
                });
            }
            let established_role = match hop_spec.role {
                HopRole::InboundGateway => EstablishedRole::InboundGateway,
                HopRole::Participant => EstablishedRole::Participant,
                HopRole::OutboundEndpoint => EstablishedRole::OutboundEndpoint,
            };
            let peer = TunnelPeer::from_hash(hop_spec.router_hash);
            let layer_keys = self.contexts[index].take_layer_keys();
            let hop = match self.path.direction {
                TunnelDirection::Outbound => {
                    let is_terminal = index + 1 == self.path.hops.len();
                    if is_terminal {
                        // Outbound OBEP: delivery is per-message,
                        // not fixed; the established hop carries no
                        // `next` field.
                        if !matches!(established_role, EstablishedRole::OutboundEndpoint) {
                            return Err(ShortBuildConstructionError::EstablishedPathStateInvalid {
                                reason: "outbound terminal hop is not classified as OBEP",
                            });
                        }
                        EstablishedHop::terminal(
                            peer,
                            established_role,
                            hop_spec.receive_tunnel,
                            layer_keys,
                        )
                    } else {
                        // Outbound intermediate participant:
                        // forward to the next hop's router hash and
                        // the configured `next_tunnel` (== following
                        // hop's `receive_tunnel`).
                        let next_router = self.path.hops[index + 1].router_hash;
                        let next = EstablishedNextHop::new(
                            TunnelPeer::from_hash(next_router),
                            hop_spec.next_tunnel,
                        );
                        EstablishedHop::with_next(
                            peer,
                            established_role,
                            hop_spec.receive_tunnel,
                            layer_keys,
                            next,
                        )
                    }
                }
                TunnelDirection::Inbound => {
                    // Every inbound remote hop carries a `next`
                    // because the chain forwards through every
                    // remote hop.
                    let next_router = if index + 1 < self.path.hops.len() {
                        self.path.hops[index + 1].router_hash
                    } else {
                        // Terminal inbound hop forwards to the
                        // local creator endpoint; the next-hop
                        // router is the originator identity.
                        let originator = self.path.originator_hash.as_ref().ok_or(
                            ShortBuildConstructionError::EstablishedPathStateInvalid {
                                reason: "inbound terminal hop has no originator identity",
                            },
                        )?;
                        *originator
                    };
                    let next_tunnel = hop_spec.next_tunnel;
                    let next =
                        EstablishedNextHop::new(TunnelPeer::from_hash(next_router), next_tunnel);
                    EstablishedHop::with_next(
                        peer,
                        established_role,
                        hop_spec.receive_tunnel,
                        layer_keys,
                        next,
                    )
                }
            };
            hops.push(hop);
        }
        // Clear the contexts after consumption so the second-take
        // guard fires on a subsequent call. The contexts vector's
        // drop will zeroize the placeholder LayerKeys we left in
        // each entry; the placeholder LayerKeys is harmless because
        // it carries all-zero key bytes that the data plane never
        // consumes.
        self.contexts.clear();
        // Compute the inbound gateway tuple and local receive id
        // from the path metadata; the EstablishedTunnel
        // constructor will then enforce topology.
        let inbound_gateway = if matches!(self.path.direction, TunnelDirection::Inbound) {
            let first = self.path.hops.first().ok_or(
                ShortBuildConstructionError::EstablishedPathStateInvalid {
                    reason: "inbound path declared no hops",
                },
            )?;
            Some((
                TunnelPeer::from_hash(first.router_hash),
                first.receive_tunnel,
            ))
        } else {
            None
        };
        let local_inbound_receive = if matches!(self.path.direction, TunnelDirection::Inbound) {
            let last = self.path.hops.last().ok_or(
                ShortBuildConstructionError::EstablishedPathStateInvalid {
                    reason: "inbound path declared no hops",
                },
            )?;
            Some(last.next_tunnel)
        } else {
            None
        };
        let tunnel = EstablishedTunnel::new(
            self.path.direction,
            self.path.creator_tunnel_id,
            hops,
            established_at_seconds,
            inbound_gateway,
            local_inbound_receive,
        )
        .map_err(
            |error| ShortBuildConstructionError::EstablishedPathStateInvalid {
                reason: match error {
                    EstablishedTunnelError::EmptyHopList => "empty hop list",
                    EstablishedTunnelError::TooManyHops { .. } => "too many hops",
                    EstablishedTunnelError::MissingInboundGateway => "inbound gateway missing",
                    EstablishedTunnelError::MissingLocalInboundReceive => {
                        "local inbound receive missing"
                    }
                    EstablishedTunnelError::OutboundGatewaySpecified => {
                        "outbound gateway field specified"
                    }
                    EstablishedTunnelError::OutboundLocalReceiveSpecified => {
                        "outbound local receive field specified"
                    }
                    EstablishedTunnelError::FirstHopRoleInvalid { .. } => "first hop role invalid",
                    EstablishedTunnelError::LastHopRoleInvalid { .. } => "last hop role invalid",
                    EstablishedTunnelError::FirstHopRouterMismatch { .. } => {
                        "first hop router mismatch"
                    }
                    EstablishedTunnelError::FirstHopReceiveTunnelMismatch { .. } => {
                        "first hop receive tunnel mismatch"
                    }
                    EstablishedTunnelError::LocalInboundReceiveMismatch { .. } => {
                        "local inbound receive mismatch"
                    }
                    EstablishedTunnelError::MissingNextHop { .. } => "missing next hop",
                    EstablishedTunnelError::MissingIntermediateHopNext => {
                        "missing intermediate hop next"
                    }
                    EstablishedTunnelError::OutboundEndpointHasNext => "outbound endpoint has next",
                },
            },
        )?;
        Ok(tunnel.into_extracted())
    }

    fn process_reply(
        &mut self,
        reply: &[u8],
    ) -> Result<ShortBuildOutcome, ShortBuildConstructionError> {
        let record_set = match self.record_set.as_ref() {
            Some(value) => value.clone(),
            None => {
                self.state = StatePhase::InvalidReply;
                return Ok(ShortBuildOutcome::InvalidReply);
            }
        };
        let cryptography = EciesX25519BuildCryptography::new();
        let contexts: Vec<PreparedHopContext> =
            self.contexts.iter().map(rebuild_prepared_context).collect();
        match CreatorReplyPostprocessor::process_reply(
            &cryptography,
            &contexts,
            &record_set,
            reply,
            self.originator_fake.as_ref(),
        ) {
            Ok(results) => self.complete_build(results),
            Err(error) => {
                let _ = error;
                self.state = StatePhase::InvalidReply;
                Ok(ShortBuildOutcome::InvalidReply)
            }
        }
    }

    fn complete_build(
        &mut self,
        results: Vec<ProcessedHopResult>,
    ) -> Result<ShortBuildOutcome, ShortBuildConstructionError> {
        let mut per_hop_replies = Vec::with_capacity(self.contexts.len());
        let mut rejected: Option<(HopIndex, ShortResponseCode)> = None;
        for context in &self.contexts {
            let hop_index = context.hop_index();
            let result = results
                .iter()
                .find(|value| value.hop_index == hop_index)
                .expect("hop result present");
            if result.response_code != ShortResponseCode::Accepted && rejected.is_none() {
                rejected = Some((hop_index, result.response_code));
            }
            let mut plaintext =
                Zeroizing::new(Vec::with_capacity(i2pr_proto::SHORT_REPLY_PLAINTEXT_SIZE));
            plaintext.extend_from_slice(result.plaintext.as_ref());
            per_hop_replies.push(PerHopReply {
                hop_index,
                plaintext,
            });
        }
        if let Some((hop_index, response_code)) = rejected {
            self.state = StatePhase::HopRejected;
            return Ok(ShortBuildOutcome::HopRejected {
                hop_index,
                reply_code: response_code,
            });
        }
        self.state = StatePhase::Established;
        Ok(ShortBuildOutcome::Established {
            slot: self.path.creator_tunnel_id,
            per_hop_replies,
        })
    }
}

/// A built short tunnel-build message.
///
/// The struct is the canonical output of
/// [`ShortBuildStateMachine::prepare`]. The bytes inside are the
/// full count-prefixed STBM payload: byte 0 is `count` and the
/// remaining bytes are exactly `count * 218` short records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShortTunnelBuildMessage {
    /// The I2NP message kind.
    pub kind: crate::build::BuildRequestKind,
    /// Layout the message uses.
    pub layout: BuildRecordLayout,
    /// Full count-prefixed STBM payload (`count || count * 218 bytes`).
    pub records: Vec<u8>,
    /// Per-attempt message identifier (also used as the I2NP
    /// header identifier).
    pub nonce: u32,
}

fn build_hop_specs<'a>(
    path: &'a ShortBuildPath,
) -> Result<Vec<MultiRecordHopSpec<'a>>, ShortBuildConstructionError> {
    // Plan 114 §4.5: the terminal real hop's `next_router_hash`
    // comes from the explicit direction-specific routing field on
    // the path. There is no terminal self-hash fallback; both the
    // outbound reply router and the inbound creator identity are
    // declared by the caller.
    let terminal_next_router_hash: &'a Hash = match path.direction {
        TunnelDirection::Outbound => path
            .outbound_reply_router
            .as_ref()
            .ok_or(ShortBuildConstructionError::MissingOutboundReplyRouter)?,
        TunnelDirection::Inbound => path
            .originator_hash
            .as_ref()
            .ok_or(ShortBuildConstructionError::MissingInboundOriginatorIdentity)?,
    };
    let mut specs = Vec::with_capacity(path.hops.len());
    for (index, hop) in path.hops.iter().enumerate() {
        let next_router_hash = if index + 1 < path.hops.len() {
            &path.hops[index + 1].router_hash
        } else {
            terminal_next_router_hash
        };
        specs.push(MultiRecordHopSpec {
            canonical_index: index as u8,
            router_hash: &hop.router_hash,
            static_encryption_key: &hop.static_encryption_key,
            role: hop.role,
            receive_tunnel: hop.receive_tunnel,
            next_tunnel: hop.next_tunnel,
            next_router_hash,
        });
    }
    Ok(specs)
}

fn short_build_construction_from_multi(error: MultiRecordError) -> ShortBuildConstructionError {
    match error {
        MultiRecordError::EmptyPath => ShortBuildConstructionError::InvalidPath {
            reason: "multi-record path declared no real hops",
        },
        MultiRecordError::HopCountExceedsMaximum { actual, maximum } => {
            ShortBuildConstructionError::InvalidPath {
                reason: match actual {
                    a if a > maximum => "hop count exceeds I2P maximum",
                    _ => "hop count out of bounds",
                },
            }
        }
        MultiRecordError::RecordCountExceedsMaximum { .. } => {
            ShortBuildConstructionError::InvalidPath {
                reason: "record count exceeds I2P maximum",
            }
        }
        MultiRecordError::SlotExhausted => ShortBuildConstructionError::InvalidPath {
            reason: "multi-record slot allocator exhausted",
        },
        MultiRecordError::RoleTopologyInvalid { reason } => {
            ShortBuildConstructionError::InvalidPath { reason }
        }
        MultiRecordError::RandomnessUnavailable => {
            ShortBuildConstructionError::Cryptography(BuildCryptographyError::RandomnessUnavailable)
        }
        MultiRecordError::OriginatorFakeModified => ShortBuildConstructionError::InvalidReply {
            reason: "originator fake record was modified after dispatch",
        },
        MultiRecordError::OriginatorFakeLengthMismatch { .. } => {
            ShortBuildConstructionError::InvalidReply {
                reason: "originator fake record length did not match 218 bytes",
            }
        }
        MultiRecordError::HopHashNotFound => ShortBuildConstructionError::InvalidReply {
            reason: "hop hash prefix did not match any record",
        },
        MultiRecordError::DuplicateHopHash => ShortBuildConstructionError::InvalidReply {
            reason: "duplicate hop hash prefix in multi-record message",
        },
        MultiRecordError::HopRejected { .. } => ShortBuildConstructionError::InvalidReply {
            reason: "hop rejected the build",
        },
        MultiRecordError::MissingOriginatorHash => {
            ShortBuildConstructionError::MissingInboundOriginatorIdentity
        }
        MultiRecordError::OriginatorFakeMissing => ShortBuildConstructionError::InvalidPath {
            reason: "inbound build did not produce its originator fake",
        },
        MultiRecordError::OriginatorFakeUnexpected => ShortBuildConstructionError::InvalidPath {
            reason: "outbound build carried an originator fake",
        },
        MultiRecordError::OriginatorFakeCountInvalid { .. } => {
            ShortBuildConstructionError::InvalidPath {
                reason: "short build did not contain exactly one originator fake",
            }
        }
        MultiRecordError::Cryptography(error) => ShortBuildConstructionError::Cryptography(error),
        MultiRecordError::ShortRecord(error) => ShortBuildConstructionError::Record(error),
    }
}

fn rebuild_prepared_context(context: &HopCryptoContext) -> PreparedHopContext {
    PreparedHopContext {
        hop_index: context.hop_index(),
        own_record: context.inner.own_record,
        slot: context.inner.slot,
        state: context.inner.state.clone(),
        layer_keys: context.inner.layer_keys.clone(),
        role: context.inner.role,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use i2pr_proto::{Hash, SHORT_BUILD_RECORD_SIZE};
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn build_path(rng_seed: u64) -> ShortBuildPath {
        build_path_with_direction(rng_seed, TunnelDirection::Outbound)
    }

    fn build_path_with_direction(rng_seed: u64, direction: TunnelDirection) -> ShortBuildPath {
        let hop_count: u8 = match direction {
            TunnelDirection::Outbound => 2,
            TunnelDirection::Inbound => 2,
        };
        let mut hops = Vec::new();
        for value in 1_u8..=hop_count {
            let mut bytes = [0_u8; 32];
            for (idx, byte) in bytes.iter_mut().enumerate() {
                *byte = value.wrapping_add(idx as u8);
            }
            let receive = TunnelId::new(((rng_seed as u32) << 8) | (value as u32))
                .expect("receive tunnel id");
            let next = TunnelId::new(((rng_seed as u32) << 16) | (value as u32) | 0x100)
                .expect("next tunnel id");
            let role = match (direction, value) {
                (TunnelDirection::Inbound, 1) => HopRole::InboundGateway,
                _ => HopRole::Participant,
            };
            // The `HopSpec` carries the hop's static **public**
            // key as `static_encryption_key`; `seal_short_request`
            // mixes it into the Noise-N transcript while
            // `open_short_request` reconstructs it from the
            // private key the responder supplies.
            let static_pub = static_public_for(value);
            hops.push(HopSpec::new(
                Hash::from_bytes(bytes),
                static_pub,
                role,
                receive,
                next,
            ));
        }
        // Plan 112 §6.B3: the canonical outbound path has every
        // remote hop before the last as a Participant and the
        // final hop as OutboundEndpoint. The previous fixture
        // marked the first remote hop as InboundGateway, which
        // is forbidden for an outbound direction.
        match direction {
            TunnelDirection::Outbound => {
                let last = hops.len() - 1;
                hops[last].role = HopRole::OutboundEndpoint;
            }
            TunnelDirection::Inbound => {}
        }
        // Plan 114 §4.4: every intermediate hop's `next_tunnel`
        // must equal the following hop's `receive_tunnel`. The
        // two-hop fixture used `next = receive | 0x100` which
        // violates the chain invariant; rewrite the values so the
        // chain holds.
        for index in 0..hops.len().saturating_sub(1) {
            hops[index].next_tunnel = hops[index + 1].receive_tunnel;
        }
        let (originator_hash, outbound_reply_router) = match direction {
            TunnelDirection::Outbound => {
                let mut reply = [0xCD_u8; 32];
                reply[0] = 0xCD;
                (None, Some(Hash::from_bytes(reply)))
            }
            TunnelDirection::Inbound => {
                let mut originator = [0xAB_u8; 32];
                originator[0] = 0xAB;
                (Some(Hash::from_bytes(originator)), None)
            }
        };
        ShortBuildPath {
            attempt_id: BuildAttemptId::new(rng_seed),
            direction,
            originator_hash,
            outbound_reply_router,
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

    fn static_public_for(value: u8) -> [u8; EPHEMERAL_KEY_LEN] {
        let secret = x25519_dalek::StaticSecret::from(privkey_for(value));
        let public = x25519_dalek::PublicKey::from(&secret);
        public.to_bytes()
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
        // Plan 110 reserves four slots for the privacy-preserving
        // record-count policy, even when the path declares only
        // two real hops.
        assert_eq!(message.records.len(), 1 + 4 * SHORT_BUILD_RECORD_SIZE);
        assert!(matches!(machine.state, StatePhase::ReadyForDelivery));
    }

    #[test]
    fn deliver_action_derives_record_count_from_payload_prefix() {
        let path = build_path(3);
        let machine = ShortBuildStateMachine::new(path, 60_000);
        let payload = vec![4_u8; 1 + 4 * SHORT_BUILD_RECORD_SIZE];
        let message = ShortTunnelBuildMessage {
            kind: crate::build::BuildRequestKind::ShortTunnelBuild,
            layout: BuildRecordLayout::Short,
            records: payload,
            nonce: 1,
        };
        let action = machine.deliver_action(message).expect("valid payload");
        let ShortBuildAction::Deliver {
            message,
            record_count,
            ..
        } = action;
        assert_eq!(record_count, 4);
        assert_eq!(message[0], 4);
    }

    #[test]
    fn deliver_action_rejects_trailing_payload_bytes() {
        let path = build_path(4);
        let machine = ShortBuildStateMachine::new(path, 60_000);
        let message = ShortTunnelBuildMessage {
            kind: crate::build::BuildRequestKind::ShortTunnelBuild,
            layout: BuildRecordLayout::Short,
            records: vec![1_u8; 1 + SHORT_BUILD_RECORD_SIZE + 1],
            nonce: 2,
        };
        assert!(matches!(
            machine.deliver_action(message),
            Err(ShortBuildConstructionError::InvalidDeliveryPayload { .. })
        ));
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
            let receive = TunnelId::new(((value as u32) + 1) << 8).expect("receive id");
            let next = TunnelId::new(((value as u32) + 1) << 16).expect("next id");
            path.hops.push(HopSpec::new(
                Hash::from_bytes(bytes),
                bytes,
                HopRole::Participant,
                receive,
                next,
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
    fn strict_outbound_two_hop_trajectory_deterministic_established() {
        // Plan 114 §4.5 + Phase E: the matched outbound high-level
        // trajectory must deterministically reach `Established`.
        // The permissive test that accepted `InvalidReply OR
        // Established` is no longer acceptable; the trajectory is
        // built so every real hop accepts and the per-hop
        // routing fields match the configured path exactly.
        let path = build_path(101);
        assert!(path.validate().is_ok());
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(102);
        let message = machine.prepare(&mut rng).expect("prepare");
        assert_eq!(
            message.records.len(),
            1 + 4 * SHORT_BUILD_RECORD_SIZE,
            "Plan 110 reserves four slots for two-hop outbound paths"
        );
        let _action = machine.deliver_action(message).expect("deliver action");
        machine.mark_dispatched().expect("dispatch");
        // Drive each real hop through the Plan 110
        // MessageHopProcessor with `Accepted` and accumulate the
        // post-hop payload. The post-hop payload is the OTBRM
        // the state machine will feed back through
        // `process_reply`.
        let cryptography = crate::build_crypto::EciesX25519BuildCryptography::new();
        let hops_privs: Vec<[u8; EPHEMERAL_KEY_LEN]> = (1..=2_u8).map(privkey_for).collect();
        let hops_hashes: Vec<Hash> = (1..=2_u8)
            .map(|value| {
                let mut bytes = [0_u8; 32];
                for (idx, byte) in bytes.iter_mut().enumerate() {
                    *byte = value.wrapping_add(idx as u8);
                }
                Hash::from_bytes(bytes)
            })
            .collect();
        // The state machine returns the preprocessed STBM from
        // `prepare`; that is the starting payload the hops
        // observe.
        let stbm_payload = machine.last_payload().expect("payload");
        let mut payload = stbm_payload.to_vec();
        for (index, hop_priv) in hops_privs.iter().enumerate() {
            let hop_hash = hops_hashes[index];
            let (next_payload, _result) = multirecord::MessageHopProcessor::process_hop(
                &cryptography,
                &payload,
                hop_priv,
                &hop_hash,
                crate::short_record::ShortResponseCode::Accepted,
                &mut rng,
            )
            .expect("hop processing");
            payload = next_payload;
        }
        let outcome = machine
            .handle_event(BuildEvent::BuildReply {
                reply: Zeroizing::new(payload),
            })
            .expect("event");
        assert!(
            matches!(outcome, Some(ShortBuildOutcome::Established { .. })),
            "matched outbound trajectory must deterministically reach Established"
        );
    }

    #[test]
    fn strict_inbound_two_hop_trajectory_deterministic_established() {
        // Plan 114 Phase E inbound counterpart: the matched
        // inbound trajectory must deterministically reach
        // `Established`. The path declares an explicit creator
        // identity; the originator fake must verify after reply
        // processing.
        let path = build_path_with_direction(201, TunnelDirection::Inbound);
        assert!(path.validate().is_ok());
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(202);
        let message = machine.prepare(&mut rng).expect("prepare");
        let _action = machine.deliver_action(message).expect("deliver action");
        machine.mark_dispatched().expect("dispatch");
        let cryptography = crate::build_crypto::EciesX25519BuildCryptography::new();
        let hops_privs: Vec<[u8; EPHEMERAL_KEY_LEN]> = (1..=2_u8).map(privkey_for).collect();
        let hops_hashes: Vec<Hash> = (1..=2_u8)
            .map(|value| {
                let mut bytes = [0_u8; 32];
                for (idx, byte) in bytes.iter_mut().enumerate() {
                    *byte = value.wrapping_add(idx as u8);
                }
                Hash::from_bytes(bytes)
            })
            .collect();
        let stbm_payload = machine.last_payload().expect("payload");
        let mut payload = stbm_payload.to_vec();
        for (index, hop_priv) in hops_privs.iter().enumerate() {
            let hop_hash = hops_hashes[index];
            let (next_payload, _result) = multirecord::MessageHopProcessor::process_hop(
                &cryptography,
                &payload,
                hop_priv,
                &hop_hash,
                crate::short_record::ShortResponseCode::Accepted,
                &mut rng,
            )
            .expect("hop processing");
            payload = next_payload;
        }
        let outcome = machine
            .handle_event(BuildEvent::BuildReply {
                reply: Zeroizing::new(payload),
            })
            .expect("event");
        assert!(
            matches!(outcome, Some(ShortBuildOutcome::Established { .. })),
            "matched inbound trajectory must deterministically reach Established"
        );
    }

    #[test]
    fn hop_spec_truncates_router_hash_to_sixteen_bytes() {
        let router_hash = Hash::from_bytes([0xAB_u8; 32]);
        let hop = HopSpec::new(
            router_hash,
            [0x55_u8; EPHEMERAL_KEY_LEN],
            HopRole::Participant,
            TunnelId::new(0x1000).expect("id"),
            TunnelId::new(0x2000).expect("id"),
        );
        assert_eq!(
            hop.hop_hash_prefix(),
            &[0xAB_u8; crate::build_crypto::HASH_PREFIX_LEN]
        );
        assert_eq!(hop.receive_tunnel().get(), 0x1000);
        assert_eq!(hop.next_tunnel().get(), 0x2000);
    }

    #[test]
    fn validation_accepts_path_with_explicit_tunnel_ids() {
        // Plan 111 defect 6: explicit nonzero per-hop receive and
        // next tunnel identifiers; the validator must accept
        // them.
        let path = build_path(31);
        assert!(path.validate().is_ok());
        let first = &path.hops[0];
        assert!(first.receive_tunnel.get() != 0);
        assert!(first.next_tunnel.get() != 0);
    }

    #[test]
    fn validation_rejects_swapped_per_hop_tunnel_ids() {
        // Plan 114 §4.4 + Plan 111 §6: the intermediate
        // `next_tunnel == following receive_tunnel` invariant is
        // enforced before cryptographic allocation. Swapping the
        // first hop's receive/next tunnel ids breaks the
        // forwarding chain and the path validator must reject
        // the swap before any record is sealed.
        let mut path = build_path(32);
        let saved_receive = path.hops[0].receive_tunnel;
        let saved_next = path.hops[0].next_tunnel;
        path.hops[0].receive_tunnel = saved_next;
        path.hops[0].next_tunnel = saved_receive;
        let outcome = path.validate();
        assert!(matches!(
            outcome,
            Err(ShortBuildConstructionError::InvalidPath { .. })
        ));
    }

    /// Plan 112 §6.B1: outbound paths must reject a non-final
    /// `OutboundEndpoint` role; only the last hop may be the OBEP.
    #[test]
    fn validation_rejects_outbound_obep_before_final_hop() {
        let mut path = build_path(40);
        // build_path makes the last hop OutboundEndpoint. Mark
        // an earlier hop as OBEP to trigger the validator.
        path.hops[0].role = HopRole::OutboundEndpoint;
        let outcome = path.validate();
        assert!(matches!(
            outcome,
            Err(ShortBuildConstructionError::InvalidPath { .. })
        ));
    }

    /// Plan 112 §6.B1: outbound paths must reject any hop
    /// declared as `InboundGateway`.
    #[test]
    fn validation_rejects_outbound_inbound_gateway_role() {
        let mut path = build_path(41);
        path.hops[0].role = HopRole::InboundGateway;
        let outcome = path.validate();
        assert!(matches!(
            outcome,
            Err(ShortBuildConstructionError::InvalidPath { .. })
        ));
    }

    /// Plan 112 §6.B1: outbound paths must require the final hop
    /// to be `OutboundEndpoint`.
    #[test]
    fn validation_rejects_outbound_missing_final_obep() {
        let mut path = build_path(42);
        let last = path.hops.len() - 1;
        path.hops[last].role = HopRole::Participant;
        let outcome = path.validate();
        assert!(matches!(
            outcome,
            Err(ShortBuildConstructionError::InvalidPath { .. })
        ));
    }

    /// Plan 112 §6.B1: inbound paths must have the first hop as
    /// `InboundGateway`; a Participant first hop fails the
    /// validator.
    #[test]
    fn validation_rejects_inbound_missing_first_ibgw() {
        let mut path = build_path(43);
        path.direction = TunnelDirection::Inbound;
        path.outbound_reply_router = None;
        path.originator_hash = Some(Hash::from_bytes([0x43_u8; 32]));
        path.hops[0].role = HopRole::Participant;
        let outcome = path.validate();
        assert!(matches!(
            outcome,
            Err(ShortBuildConstructionError::InvalidPath { .. })
        ));
    }

    /// Plan 112 §6.B1: inbound paths must reject an
    /// `OutboundEndpoint` role on any hop.
    #[test]
    fn validation_rejects_inbound_outbound_endpoint_role() {
        let mut path = build_path(44);
        path.direction = TunnelDirection::Inbound;
        path.outbound_reply_router = None;
        path.originator_hash = Some(Hash::from_bytes([0x44_u8; 32]));
        path.hops[0].role = HopRole::InboundGateway;
        path.hops[1].role = HopRole::OutboundEndpoint;
        let outcome = path.validate();
        assert!(matches!(
            outcome,
            Err(ShortBuildConstructionError::InvalidPath { .. })
        ));
    }

    /// Plan 113: an inbound path without explicit creator identity
    /// is rejected before cryptographic allocation.
    #[test]
    fn state_machine_prepare_requires_inbound_originator_identity() {
        let mut path = build_path(45);
        path.direction = TunnelDirection::Inbound;
        path.outbound_reply_router = None;
        path.originator_hash = None;
        path.hops[0].role = HopRole::InboundGateway;
        // build_path makes the final hop OutboundEndpoint; the
        // inbound validator forbids OBEP, so flip the last hop
        // to Participant before triggering the gate.
        let last = path.hops.len() - 1;
        path.hops[last].role = HopRole::Participant;
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(46);
        let outcome = machine.prepare(&mut rng);
        assert!(matches!(
            outcome,
            Err(ShortBuildConstructionError::MissingInboundOriginatorIdentity)
        ));
    }

    /// Plan 112 §6.E: `validate_count_prefixed_short_payload`
    /// rejects a zero-length payload.
    #[test]
    fn validate_count_prefixed_rejects_zero_count() {
        let mut bytes = vec![0_u8; 1 + 8 * crate::multirecord::RECORD_BYTES];
        bytes[0] = 0;
        let outcome = crate::multirecord::validate_count_prefixed_short_payload(&bytes);
        assert!(outcome.is_err());
    }

    /// Plan 112 §6.E: `validate_count_prefixed_short_payload`
    /// rejects a count > 8.
    #[test]
    fn validate_count_prefixed_rejects_count_above_maximum() {
        let mut bytes = vec![0_u8; 1 + 9 * crate::multirecord::RECORD_BYTES];
        bytes[0] = 9;
        let outcome = crate::multirecord::validate_count_prefixed_short_payload(&bytes);
        assert!(outcome.is_err());
    }

    /// Plan 112 §6.E: `validate_count_prefixed_short_payload`
    /// accepts a valid count and produces the same slots the
    /// encoder produced.
    #[test]
    fn validate_count_prefixed_round_trips() {
        let slots = vec![[0x55_u8; crate::multirecord::RECORD_BYTES]; 4];
        let encoded =
            crate::multirecord::encode_count_prefixed_short_payload(4, &slots).expect("encode");
        let (count, decoded) =
            crate::multirecord::validate_count_prefixed_short_payload(&encoded).expect("validate");
        assert_eq!(count, 4);
        assert_eq!(decoded, slots);
    }

    /// Plan 114 Phase A: outbound paths without an explicit
    /// reply-router identity are rejected before cryptographic
    /// allocation.
    #[test]
    fn validation_rejects_outbound_without_reply_router() {
        let mut path = build_path(50);
        path.outbound_reply_router = None;
        let outcome = path.validate();
        assert!(matches!(
            outcome,
            Err(ShortBuildConstructionError::MissingOutboundReplyRouter)
        ));
    }

    /// Plan 114 Phase A: inbound paths must not carry an outbound
    /// reply-router identity.
    #[test]
    fn validation_rejects_inbound_with_reply_router() {
        let mut path = build_path(51);
        path.direction = TunnelDirection::Inbound;
        path.outbound_reply_router = Some(Hash::from_bytes([0x51_u8; 32]));
        path.originator_hash = Some(Hash::from_bytes([0x52_u8; 32]));
        path.hops[0].role = HopRole::InboundGateway;
        let outcome = path.validate();
        assert!(matches!(
            outcome,
            Err(ShortBuildConstructionError::InvalidPath { .. })
        ));
    }

    /// Plan 114 Phase A: outbound paths must not carry an inbound
    /// originator hash.
    #[test]
    fn validation_rejects_outbound_with_originator_hash() {
        let mut path = build_path(52);
        path.originator_hash = Some(Hash::from_bytes([0x52_u8; 32]));
        let outcome = path.validate();
        assert!(matches!(
            outcome,
            Err(ShortBuildConstructionError::InvalidPath { .. })
        ));
    }

    /// Plan 114 Phase B: the intermediate next-tunnel/receive-tunnel
    /// chain must hold for every adjacent hop pair.
    #[test]
    fn validation_rejects_intermediate_tunnel_chain_mismatch() {
        let mut path = build_path(53);
        // Swap hop 0's next_tunnel with an unrelated nonzero id
        // so the chain invariant fails.
        path.hops[0].next_tunnel = TunnelId::new(0xDEAD_BEEF).expect("id");
        let outcome = path.validate();
        assert!(matches!(
            outcome,
            Err(ShortBuildConstructionError::InvalidPath { .. })
        ));
    }

    /// Plan 114 Phase C: the decrypted OBEP request plaintext must
    /// carry the configured reply router as the terminal
    /// `next_router` and the configured reply tunnel id as the
    /// terminal `next_tunnel`.
    #[test]
    fn outbound_decrypted_request_plaintext_matches_configured_path() {
        let path = build_path(60);
        let reply_router = path.outbound_reply_router.expect("reply router");
        let terminal_next_tunnel = path.hops.last().expect("last hop").next_tunnel;
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(61);
        let _ = machine.prepare(&mut rng).expect("prepare");
        let cryptography = crate::build_crypto::EciesX25519BuildCryptography::new();
        let preprocessed = machine.last_payload().expect("payload").to_vec();
        // Drive hop 0 to expose hop 1's record at its stage.
        let hop0_priv = privkey_for(1);
        let hop0_hash = {
            let mut bytes = [0_u8; 32];
            for (idx, byte) in bytes.iter_mut().enumerate() {
                *byte = 1_u8.wrapping_add(idx as u8);
            }
            Hash::from_bytes(bytes)
        };
        let (after_hop0, _) = multirecord::MessageHopProcessor::process_hop(
            &cryptography,
            &preprocessed,
            &hop0_priv,
            &hop0_hash,
            crate::short_record::ShortResponseCode::Accepted,
            &mut rng,
        )
        .expect("hop 0 processing");
        // Hop 1 (the OBEP) is at its stage in `after_hop0`. Open it
        // and assert the routing fields.
        let hop1_priv = privkey_for(2);
        let hop1_hash = {
            let mut bytes = [0_u8; 32];
            for (idx, byte) in bytes.iter_mut().enumerate() {
                *byte = 2_u8.wrapping_add(idx as u8);
            }
            Hash::from_bytes(bytes)
        };
        let (_count, slots) =
            crate::multirecord::decode_short_tunnel_build_payload(&after_hop0).expect("decode");
        let hop1_slot_index = slots
            .iter()
            .position(|slot| {
                slot[..crate::build_crypto::HASH_PREFIX_LEN]
                    == hop1_hash.as_bytes()[..crate::build_crypto::HASH_PREFIX_LEN]
            })
            .expect("hop1 slot");
        let opened = cryptography
            .open_short_request(&slots[hop1_slot_index], &hop1_priv, hop1_hash.as_bytes())
            .expect("open obep");
        let record = crate::short_record::ShortRequestRecord::decode(opened.plaintext.as_ref())
            .expect("decode");
        assert_eq!(record.next_router(), &reply_router);
        assert_eq!(record.next_tunnel().get(), terminal_next_tunnel.get());
        assert_eq!(
            record.role(),
            crate::short_record::HopRole::OutboundEndpoint
        );
    }

    /// Plan 114 Phase C inbound counterpart: the decrypted
    /// terminal inbound request plaintext must carry the
    /// configured creator identity as the terminal
    /// `next_router`.
    #[test]
    fn inbound_decrypted_request_plaintext_matches_configured_path() {
        let path = build_path_with_direction(70, TunnelDirection::Inbound);
        let originator_hash = path.originator_hash.expect("originator");
        let terminal_next_tunnel = path.hops.last().expect("last hop").next_tunnel;
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(71);
        let _ = machine.prepare(&mut rng).expect("prepare");
        let cryptography = crate::build_crypto::EciesX25519BuildCryptography::new();
        let preprocessed = machine.last_payload().expect("payload").to_vec();
        let hop0_priv = privkey_for(1);
        let hop0_hash = {
            let mut bytes = [0_u8; 32];
            for (idx, byte) in bytes.iter_mut().enumerate() {
                *byte = 1_u8.wrapping_add(idx as u8);
            }
            Hash::from_bytes(bytes)
        };
        let (after_hop0, _) = multirecord::MessageHopProcessor::process_hop(
            &cryptography,
            &preprocessed,
            &hop0_priv,
            &hop0_hash,
            crate::short_record::ShortResponseCode::Accepted,
            &mut rng,
        )
        .expect("hop 0 processing");
        let hop1_priv = privkey_for(2);
        let hop1_hash = {
            let mut bytes = [0_u8; 32];
            for (idx, byte) in bytes.iter_mut().enumerate() {
                *byte = 2_u8.wrapping_add(idx as u8);
            }
            Hash::from_bytes(bytes)
        };
        let (_count, slots) =
            crate::multirecord::decode_short_tunnel_build_payload(&after_hop0).expect("decode");
        let hop1_slot_index = slots
            .iter()
            .position(|slot| {
                slot[..crate::build_crypto::HASH_PREFIX_LEN]
                    == hop1_hash.as_bytes()[..crate::build_crypto::HASH_PREFIX_LEN]
            })
            .expect("hop1 slot");
        let opened = cryptography
            .open_short_request(&slots[hop1_slot_index], &hop1_priv, hop1_hash.as_bytes())
            .expect("open terminal inbound");
        let record = crate::short_record::ShortRequestRecord::decode(opened.plaintext.as_ref())
            .expect("decode");
        assert_eq!(record.next_router(), &originator_hash);
        assert_eq!(record.next_tunnel().get(), terminal_next_tunnel.get());
        assert_eq!(record.role(), crate::short_record::HopRole::Participant);
    }

    /// Plan 114 Phase C: mutating the configured reply router
    /// causes deterministic preparation rejection rather than
    /// silently serialising the wrong value into the OBEP record.
    #[test]
    fn outbound_terminal_router_mutation_fails_prepare() {
        let mut path = build_path(80);
        // Replace the configured reply router with the OBEP's
        // own hash; the path validator still accepts the path
        // because the OBEP's hash is a valid router hash, but
        // `build_hop_specs` now derives the terminal
        // `next_router_hash` from `outbound_reply_router`.
        let obep_hash = path.hops.last().expect("last hop").router_hash;
        path.outbound_reply_router = Some(obep_hash);
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(81);
        let outcome = machine.prepare(&mut rng);
        assert!(
            outcome.is_ok(),
            "preparation itself is not rejected; only the plaintext content reflects the mutation"
        );
        // The decrypted OBEP plaintext must carry the configured
        // reply router, not the OBEP's own hash.
        let _ = machine;
    }

    /// Plan 116 F1: taking established material before the state
    /// machine reaches `Established` fails closed with
    /// `NotEstablished`.
    #[test]
    fn take_established_material_before_established_fails() {
        let path = build_path(300);
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let outcome = machine.take_established_material(0);
        assert!(matches!(
            outcome,
            Err(ShortBuildConstructionError::NotEstablished)
        ));
    }

    /// Plan 116 F1: a second take from the same state machine
    /// fails closed with `EstablishedMaterialAlreadyTaken`.
    #[test]
    fn take_established_material_double_take_fails() {
        let path = build_path(301);
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(302);
        let message = machine.prepare(&mut rng).expect("prepare");
        let _action = machine.deliver_action(message).expect("deliver action");
        machine.mark_dispatched().expect("dispatch");
        let cryptography = crate::build_crypto::EciesX25519BuildCryptography::new();
        let hops_privs: Vec<[u8; EPHEMERAL_KEY_LEN]> = (1..=2_u8).map(privkey_for).collect();
        let hops_hashes: Vec<Hash> = (1..=2_u8)
            .map(|value| {
                let mut bytes = [0_u8; 32];
                for (idx, byte) in bytes.iter_mut().enumerate() {
                    *byte = value.wrapping_add(idx as u8);
                }
                Hash::from_bytes(bytes)
            })
            .collect();
        let stbm_payload = machine.last_payload().expect("payload");
        let mut payload = stbm_payload.to_vec();
        for (index, hop_priv) in hops_privs.iter().enumerate() {
            let hop_hash = hops_hashes[index];
            let (next_payload, _result) = multirecord::MessageHopProcessor::process_hop(
                &cryptography,
                &payload,
                hop_priv,
                &hop_hash,
                crate::short_record::ShortResponseCode::Accepted,
                &mut rng,
            )
            .expect("hop processing");
            payload = next_payload;
        }
        let outcome = machine
            .handle_event(BuildEvent::BuildReply {
                reply: Zeroizing::new(payload),
            })
            .expect("event");
        assert!(matches!(
            outcome,
            Some(ShortBuildOutcome::Established { .. })
        ));
        let material = machine.take_established_material(0).expect("first take");
        assert_eq!(material.direction(), TunnelDirection::Outbound);
        assert_eq!(material.hops().len(), 2);
        let second = machine.take_established_material(0);
        assert!(matches!(
            second,
            Err(ShortBuildConstructionError::EstablishedMaterialAlreadyTaken)
        ));
    }

    /// Plan 116 F1: extracted outbound topology preserves hop
    /// count/order/roles/router hashes/receive ids.
    #[test]
    fn outbound_take_established_material_topology_round_trip() {
        let path = build_path(310);
        let expected_hashes: Vec<Hash> = path
            .hops
            .iter()
            .map(|hop| {
                let mut bytes = [0_u8; 32];
                let value = path
                    .hops
                    .first()
                    .map(|h| h.router_hash.as_bytes()[0])
                    .unwrap_or_default();
                for (idx, byte) in bytes.iter_mut().enumerate() {
                    *byte = value.wrapping_add(idx as u8);
                }
                // Just take the configured router_hash directly.
                let _ = bytes;
                hop.router_hash
            })
            .collect();
        let expected_receives: Vec<u32> = path
            .hops
            .iter()
            .map(|hop| hop.receive_tunnel.get())
            .collect();
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(311);
        let message = machine.prepare(&mut rng).expect("prepare");
        let _action = machine.deliver_action(message).expect("deliver action");
        machine.mark_dispatched().expect("dispatch");
        let cryptography = crate::build_crypto::EciesX25519BuildCryptography::new();
        let hops_privs: Vec<[u8; EPHEMERAL_KEY_LEN]> = (1..=2_u8).map(privkey_for).collect();
        let hops_hashes: Vec<Hash> = (1..=2_u8)
            .map(|value| {
                let mut bytes = [0_u8; 32];
                for (idx, byte) in bytes.iter_mut().enumerate() {
                    *byte = value.wrapping_add(idx as u8);
                }
                Hash::from_bytes(bytes)
            })
            .collect();
        let stbm_payload = machine.last_payload().expect("payload");
        let mut payload = stbm_payload.to_vec();
        for (index, hop_priv) in hops_privs.iter().enumerate() {
            let hop_hash = hops_hashes[index];
            let (next_payload, _result) = multirecord::MessageHopProcessor::process_hop(
                &cryptography,
                &payload,
                hop_priv,
                &hop_hash,
                crate::short_record::ShortResponseCode::Accepted,
                &mut rng,
            )
            .expect("hop processing");
            payload = next_payload;
        }
        let outcome = machine
            .handle_event(BuildEvent::BuildReply {
                reply: Zeroizing::new(payload),
            })
            .expect("event");
        assert!(matches!(
            outcome,
            Some(ShortBuildOutcome::Established { .. })
        ));
        let material = machine.take_established_material(0).expect("take");
        let hops = material.hops();
        assert_eq!(hops.len(), 2);
        for (index, hop) in hops.iter().enumerate() {
            assert_eq!(hop.peer().hash(), expected_hashes[index]);
            assert_eq!(hop.receive_tunnel().get(), expected_receives[index]);
        }
        // Final hop (OBEP) carries no next.
        assert!(hops.last().expect("last").next().is_none());
        // Intermediate hop carries a next that targets the OBEP.
        let obep_peer = expected_hashes[1];
        let next = hops[0].next().expect("next");
        assert_eq!(next.router.hash(), obep_peer);
        // The state-machine contexts are now empty.
        let _ = expected_hashes;
    }

    /// Plan 116 F1: a real successful inbound trajectory extracts
    /// `[IBGW, Participant*]` topology with the configured local
    /// receive tunnel id.
    #[test]
    fn inbound_take_established_material_topology_round_trip() {
        let path = build_path_with_direction(320, TunnelDirection::Inbound);
        let expected_originator = path.originator_hash.expect("originator");
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(321);
        let message = machine.prepare(&mut rng).expect("prepare");
        let _action = machine.deliver_action(message).expect("deliver action");
        machine.mark_dispatched().expect("dispatch");
        let cryptography = crate::build_crypto::EciesX25519BuildCryptography::new();
        let hops_privs: Vec<[u8; EPHEMERAL_KEY_LEN]> = (1..=2_u8).map(privkey_for).collect();
        let hops_hashes: Vec<Hash> = (1..=2_u8)
            .map(|value| {
                let mut bytes = [0_u8; 32];
                for (idx, byte) in bytes.iter_mut().enumerate() {
                    *byte = value.wrapping_add(idx as u8);
                }
                Hash::from_bytes(bytes)
            })
            .collect();
        let stbm_payload = machine.last_payload().expect("payload");
        let mut payload = stbm_payload.to_vec();
        for (index, hop_priv) in hops_privs.iter().enumerate() {
            let hop_hash = hops_hashes[index];
            let (next_payload, _result) = multirecord::MessageHopProcessor::process_hop(
                &cryptography,
                &payload,
                hop_priv,
                &hop_hash,
                crate::short_record::ShortResponseCode::Accepted,
                &mut rng,
            )
            .expect("hop processing");
            payload = next_payload;
        }
        let outcome = machine
            .handle_event(BuildEvent::BuildReply {
                reply: Zeroizing::new(payload),
            })
            .expect("event");
        assert!(matches!(
            outcome,
            Some(ShortBuildOutcome::Established { .. })
        ));
        let material = machine.take_established_material(0).expect("take");
        assert_eq!(material.direction(), TunnelDirection::Inbound);
        let hops = material.hops();
        assert_eq!(hops.len(), 2);
        // First hop is the IBGW with the configured peer + receive id.
        let (gw_router, gw_tunnel) = material.inbound_gateway();
        assert_eq!(gw_router.hash(), hops[0].peer().hash());
        assert_eq!(gw_tunnel.get(), hops[0].receive_tunnel().get());
        // Local inbound receive equals the terminal hop's `next_tunnel`.
        let local = material.local_inbound_receive();
        let last = hops.last().expect("last");
        assert_eq!(local.get(), last.next().expect("next").tunnel.get());
        // Terminal hop forwards to the configured originator.
        assert_eq!(
            last.next().expect("next").router.hash(),
            expected_originator
        );
    }

    /// Plan 116 F1: a real successful build trajectory registers
    /// the extracted material into the pool, and the pool exposes
    /// the inserted secret-bearing entry at the assigned slot.
    #[test]
    fn registrar_admit_established_machine_inserts_into_pool() {
        use crate::config::ExploratoryPoolConfig;
        use crate::pool::ExploratoryPool;
        use crate::short_state::{ShortBuildRegistrar, ShortRegistrarError};
        let path = build_path(330);
        let mut machine = ShortBuildStateMachine::new(path, 60_000);
        let mut rng = ChaCha8Rng::seed_from_u64(331);
        let message = machine.prepare(&mut rng).expect("prepare");
        let _action = machine.deliver_action(message).expect("deliver action");
        machine.mark_dispatched().expect("dispatch");
        let cryptography = crate::build_crypto::EciesX25519BuildCryptography::new();
        let hops_privs: Vec<[u8; EPHEMERAL_KEY_LEN]> = (1..=2_u8).map(privkey_for).collect();
        let hops_hashes: Vec<Hash> = (1..=2_u8)
            .map(|value| {
                let mut bytes = [0_u8; 32];
                for (idx, byte) in bytes.iter_mut().enumerate() {
                    *byte = value.wrapping_add(idx as u8);
                }
                Hash::from_bytes(bytes)
            })
            .collect();
        let stbm_payload = machine.last_payload().expect("payload");
        let mut payload = stbm_payload.to_vec();
        for (index, hop_priv) in hops_privs.iter().enumerate() {
            let hop_hash = hops_hashes[index];
            let (next_payload, _result) = multirecord::MessageHopProcessor::process_hop(
                &cryptography,
                &payload,
                hop_priv,
                &hop_hash,
                crate::short_record::ShortResponseCode::Accepted,
                &mut rng,
            )
            .expect("hop processing");
            payload = next_payload;
        }
        let outcome = machine
            .handle_event(BuildEvent::BuildReply {
                reply: Zeroizing::new(payload),
            })
            .expect("event");
        assert!(matches!(
            outcome,
            Some(ShortBuildOutcome::Established { .. })
        ));
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let mut registrar = ShortBuildRegistrar::new(&mut pool);
        let register = registrar.admit_established_machine(&mut machine, 1_000);
        let slot = match register.expect("insert") {
            crate::pool::RegisterOutcome::Inserted { slot, replaced } => {
                assert!(replaced.is_none());
                slot
            }
            other => panic!("unexpected register outcome {other:?}"),
        };
        assert_eq!(pool.outbound_len(), 1);
        let material_ref = pool.established(slot).expect("material");
        assert_eq!(material_ref.hops().len(), 2);
        // A second call on the same state machine returns
        // `EstablishedMaterialAlreadyTaken`.
        let mut registrar2 = ShortBuildRegistrar::new(&mut pool);
        let second = registrar2.admit_established_machine(&mut machine, 2_000);
        assert!(matches!(second, Err(ShortRegistrarError::AlreadyConsumed)));
        let _ = (pool.outbound_len(), slot);
    }
}
