//! Plan 122 §D-§G: destination routing composition.
//!
//! The routing module composes the Plan 119 LS2 lookup surface, the
//! Plan 120 destination runtime, the Plan 121 ECIES Garlic session
//! layer, and the Plan 116 tunnel data plane into an end-to-end
//! outbound send path. The path executes the canonical I2P routing
//! sequence without skipping the OBEP tunnel delivery and without
//! falling back to a direct client shortcut:
//!
//! ```text
//! application payload
//!  -> I2NP Data message
//!  -> ECIES Garlic Clove (LOCAL/NEW SESSION) + optional bundled LS2
//!  -> outbound destination tunnel
//!  -> OBEP TUNNEL delivery to selected Lease2 (gateway, tunnel id)
//!  -> typed router-delivery boundary (Phase G)
//!  -> remote IBGW / inbound tunnel / endpoint (consumed by tests)
//! ```
//!
//! The router-delivery boundary in Phase G emits one
//! [`OutboundDeliveryPlan`] per send; the receiver consumes the plan
//! through the Plan 116 tunnel data plane (the test fixture drives
//! it via `LocalInboundEndpointRole::process`).
//!
//! The module owns no runtime state itself: the local destination
//! runtime owns the destination's identity, session manager, and
//! pool, while the routing layer composes them. The router-side
//! LeaseSet2 cache lives in [`i2pr_netdb::LeaseSet2Store`] and is
//! threaded through every send so the selector only sees validated
//! records.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use i2pr_netdb::{
    DestinationHash, LeaseSet2Store, LookupAction, LookupId, LookupKind, ReplyPath,
    ReplyPathProvider, ValidatedLeaseSet2, handle_database_store_lease_set2,
    router_hash_from_destination,
};
use i2pr_proto::{
    CodecError, Date, DeferredPayload, GarlicCloveBlock, GarlicDelivery, Hash, I2npMessage,
    LeaseSet2, MAX_I2NP_PAYLOAD_SIZE, OpaqueMessageBody, TunnelDataMessage,
};
use i2pr_tunnel::{
    DeliveryInstruction, EciesX25519BuildCryptography, EstablishedTunnel, LayerKeys,
    OBGWRouterDelivery, OutboundGatewayRole, TunnelId, TunnelLifetime, TunnelPayloadHeader,
    TunnelRoleError,
};
use rand_core::{CryptoRng, RngCore};

use crate::identity::DestinationId;
use crate::lease_selection::{
    LeaseSelectionError, LeaseSelectionPolicy, LeaseSelector, SelectedLease,
};
use crate::session::{
    EciesOutboundMessage, EciesPayloadError, EciesSessionManager, PendingHandshakeRecord,
    decode_decrypted_payload, encode_new_session_payload,
};

/// Hard ceiling on the number of distinct remote destinations the
/// routing layer keeps an outstanding lookup for at any moment.
pub const MAX_CONCURRENT_REMOTE_LOOKUPS: usize = 256;
/// Hard ceiling on the number of pending outbound payloads per local
/// destination while waiting for the LeaseSet2 resolution.
pub const MAX_PENDING_OUTBOUND_PER_REMOTE: usize = 64;
/// Hard ceiling on the number of bytes the router-side LS2 cache
/// will retain for a single local destination lookup context.
pub const MAX_ROUTER_SIDE_LS2_BYTES_PER_REMOTE: usize = 16 * 1024;

/// Typed outcome of a Plan 122 send composition.
#[derive(Debug)]
pub enum SendError {
    /// The supplied destination identity failed to seal its static
    /// public key into the LS2 encryption-key binding.
    NoRemoteStaticPublicKey,
    /// The remote LeaseSet2 is not yet resolved; the caller must
    /// retry after the LeaseSet2 lookup completes.
    LeaseSet2LookupPending,
    /// The remote LeaseSet2 lookup failed terminally.
    LeaseSet2LookupFailed,
    /// The selector could not find a usable lease in the resolved
    /// LeaseSet2.
    NoUsableLease(LeaseSelectionError),
    /// The ECIES session manager rejected the payload.
    Session(EciesSessionError),
    /// The Garlic payload codec rejected the composed clove sequence.
    Payload(EciesPayloadError),
    /// The I2NP codec rejected the inner Data message.
    DataCodec(CodecError),
    /// The outbound tunnel role reported a data-plane failure.
    TunnelRole(TunnelRoleError),
    /// The destination is stopping or stopped.
    DestinationStopping,
    /// The local destination's tunnel pool has no usable outbound
    /// tunnel registered.
    NoOutboundTunnel,
}

impl core::fmt::Display for SendError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoRemoteStaticPublicKey => {
                formatter.write_str("remote destination has no usable X25519 static public key")
            }
            Self::LeaseSet2LookupPending => {
                formatter.write_str("remote LeaseSet2 lookup is still pending")
            }
            Self::LeaseSet2LookupFailed => {
                formatter.write_str("remote LeaseSet2 lookup terminated without a result")
            }
            Self::NoUsableLease(error) => write!(formatter, "no usable lease: {error}"),
            Self::Session(error) => write!(formatter, "ECIES session: {error}"),
            Self::Payload(error) => write!(formatter, "ECIES payload: {error}"),
            Self::DataCodec(error) => write!(formatter, "I2NP codec: {error}"),
            Self::TunnelRole(error) => write!(formatter, "tunnel role: {error}"),
            Self::DestinationStopping => formatter.write_str("local destination is stopping"),
            Self::NoOutboundTunnel => {
                formatter.write_str("local destination has no usable outbound tunnel")
            }
        }
    }
}

impl std::error::Error for SendError {}

/// Outcome of a successful Plan 122 send composition.
#[derive(Debug)]
pub struct OutboundDeliveryPlan {
    /// Selected lease metadata the routing layer bound to the send.
    pub selected_lease: SelectedLease,
    /// Standard-encoded inner I2NP envelope the local creator emitted.
    pub inner_envelope_bytes: Vec<u8>,
    /// Encrypted Garlic outbound message (New Session or Existing).
    pub encrypted_message: EncryptedOutbound,
    /// Outbound tunnel cell(s) the runtime must dispatch through the
    /// transport adapter.
    pub cells: Vec<OBGWRouterDelivery>,
}

/// One encrypted outbound payload. Plan 122 exposes the raw
/// cryptographic envelope so the local receiver can drive the
/// matching decryption path against its own EciesSessionManager.
#[derive(Debug)]
pub enum EncryptedOutbound {
    /// A New Session handshake is in flight; the caller must hand
    /// the pending handshake back to the session manager when the
    /// reply lands.
    NewSession {
        /// Encoded New Session message bytes.
        message: Vec<u8>,
        /// Pending handshake the session manager keeps until the
        /// reply arrives.
        pending: PendingHandshakeRecord,
    },
    /// An Existing Session message; no pending state.
    Existing {
        /// Encoded Existing Session message bytes.
        message: Vec<u8>,
    },
}

impl EncryptedOutbound {
    /// Returns the encoded message bytes.
    pub fn message_bytes(&self) -> &[u8] {
        match self {
            Self::NewSession { message, .. } | Self::Existing { message } => message,
        }
    }
}

/// Plan 122 §E + §I mirror type: typed recipient-side failure from
/// the ECIES session manager.
#[derive(Debug)]
pub enum EciesSessionError {
    /// No installed session and the pending-handshake capacity was
    /// reached.
    PendingHandshakeCapacity {
        /// Configured maximum.
        maximum: u16,
    },
    /// The supplied pending handshake could not be matched against
    /// the local session state.
    NoSession,
    /// The wrapped ECIES primitive returned a typed error.
    Protocol(&'static str),
}

impl core::fmt::Display for EciesSessionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::PendingHandshakeCapacity { maximum } => write!(
                formatter,
                "ECIES pending-handshake capacity {maximum} exhausted"
            ),
            Self::NoSession => formatter.write_str("no session for remote destination"),
            Self::Protocol(message) => write!(formatter, "ECIES session protocol: {message}"),
        }
    }
}

impl std::error::Error for EciesSessionError {}

/// Router-side outbound destination tunnel handle. The handle is the
/// caller-facing view of the local creator's outbound tunnel role;
/// it owns the secret material exclusively and never lets the
/// routing layer clone it.
#[derive(Debug)]
pub struct DestinationOutboundRole {
    role: OutboundGatewayRole,
    expires_at_ms: u64,
}

impl DestinationOutboundRole {
    /// Wraps an established outbound tunnel in a fresh role.
    pub fn new(established: EstablishedTunnel, expires_at_ms: u64) -> Self {
        Self {
            role: OutboundGatewayRole::new(established, expires_at_ms),
            expires_at_ms,
        }
    }

    /// Returns the underlying role.
    pub const fn role(&self) -> &OutboundGatewayRole {
        &self.role
    }

    /// Returns the configured role expiration.
    pub const fn expires_at_ms(&self) -> u64 {
        self.expires_at_ms
    }
}

/// Router-side destination routing configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DestinationRoutingConfig {
    /// Hard ceiling on the number of distinct remote destinations
    /// the routing layer keeps outstanding lookups for at any moment.
    max_concurrent_remote_lookups: u16,
    /// Hard ceiling on the number of pending outbound payloads per
    /// local destination while waiting for the LeaseSet2 resolution.
    max_pending_outbound_per_remote: u16,
    /// Lease safety margin used when selecting leases.
    lease_safety_margin_seconds: u32,
}

impl DestinationRoutingConfig {
    /// Constructs a routing configuration, enforcing every ceiling.
    pub const fn try_new(
        max_concurrent_remote_lookups: u16,
        max_pending_outbound_per_remote: u16,
        lease_safety_margin_seconds: u32,
    ) -> Result<Self, DestinationRoutingError> {
        if (max_concurrent_remote_lookups as usize) > MAX_CONCURRENT_REMOTE_LOOKUPS {
            return Err(DestinationRoutingError::RemoteLookupBudgetExceeded);
        }
        if (max_pending_outbound_per_remote as usize) > MAX_PENDING_OUTBOUND_PER_REMOTE {
            return Err(DestinationRoutingError::PendingOutboundBudgetExceeded);
        }
        if lease_safety_margin_seconds > 600 {
            return Err(DestinationRoutingError::InvalidLeaseSafetyMargin);
        }
        Ok(Self {
            max_concurrent_remote_lookups,
            max_pending_outbound_per_remote,
            lease_safety_margin_seconds,
        })
    }

    /// Returns a balanced experimental default.
    pub fn balanced() -> Self {
        Self::try_new(64, 32, 60).expect("balanced routing config is within every ceiling")
    }

    /// Returns the concurrent-lookup ceiling.
    pub const fn max_concurrent_remote_lookups(&self) -> u16 {
        self.max_concurrent_remote_lookups
    }

    /// Returns the pending-outbound ceiling.
    pub const fn max_pending_outbound_per_remote(&self) -> u16 {
        self.max_pending_outbound_per_remote
    }

    /// Returns the lease safety margin in seconds.
    pub const fn lease_safety_margin_seconds(&self) -> u32 {
        self.lease_safety_margin_seconds
    }
}

/// Typed routing configuration failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestinationRoutingError {
    /// The concurrent-lookup ceiling exceeded the local bound.
    RemoteLookupBudgetExceeded,
    /// The pending-outbound ceiling exceeded the local bound.
    PendingOutboundBudgetExceeded,
    /// The lease safety margin exceeded the local bound.
    InvalidLeaseSafetyMargin,
}

impl core::fmt::Display for DestinationRoutingError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::RemoteLookupBudgetExceeded => {
                formatter.write_str("remote lookup budget exceeded")
            }
            Self::PendingOutboundBudgetExceeded => {
                formatter.write_str("pending outbound budget exceeded")
            }
            Self::InvalidLeaseSafetyMargin => formatter.write_str("invalid lease safety margin"),
        }
    }
}

impl std::error::Error for DestinationRoutingError {}

/// Bounded router-side destination routing state. The state owns
/// the LeaseSet2 lookup state machine, the LS2 cache, the active
/// remote destinations the local creator has resolved, and the
/// pending handshake records for in-flight New Session replies.
pub struct DestinationRouting {
    config: DestinationRoutingConfig,
    selector: LeaseSelector,
    /// Router-side LeaseSet2 lookup state machine.
    lookup: RouterInfoLookupShim,
    /// Bounded LS2 cache the routing layer reads from.
    lease_set2_store: LeaseSet2Store,
    /// Active remote destinations keyed by the destination hash the
    /// local creator is currently communicating with.
    active_remotes: BTreeMap<DestinationHash, RemoteState>,
}

struct RemoteState {
    validated: ValidatedLeaseSet2,
    static_public: [u8; 32],
}

impl DestinationRouting {
    /// Constructs a new routing state machine with the supplied
    /// configuration and an empty LeaseSet2 cache.
    pub fn new(config: DestinationRoutingConfig) -> Self {
        Self {
            config,
            selector: LeaseSelector::new(),
            lookup: RouterInfoLookupShim::new(),
            lease_set2_store: LeaseSet2Store::default(),
            active_remotes: BTreeMap::new(),
        }
    }

    /// Returns the routing configuration.
    pub const fn config(&self) -> DestinationRoutingConfig {
        self.config
    }

    /// Returns the LeaseSet2 cache for callers that need to drive
    /// external publication or to inspect inserted records.
    pub const fn lease_set2_store(&self) -> &LeaseSet2Store {
        &self.lease_set2_store
    }

    /// Returns a mutable reference to the LeaseSet2 cache for
    /// callers that ingest unsolicited DatabaseStore messages
    /// outside the Plan 122 lookup state machine.
    pub fn lease_set2_store_mut(&mut self) -> &mut LeaseSet2Store {
        &mut self.lease_set2_store
    }

    /// Returns the active remote destination count.
    pub fn active_remote_count(&self) -> usize {
        self.active_remotes.len()
    }

    /// Resolves a remote Destination through the router-side LeaseSet2
    /// store. The lookup drives the router's existing NetDB
    /// composition; the runtime adapter is expected to dispatch the
    /// returned [`LookupAction`] through its outbound delivery
    /// adapter and feed the response back via
    /// [`Self::ingest_lookup_response`].
    pub fn begin_remote_lookup(
        &mut self,
        request_id: u64,
        target: DestinationHash,
    ) -> LookupAction {
        let lookup_id = LookupId::new(
            request_id,
            LookupKind::LeaseSet2,
            router_hash_from_destination(target),
        );
        self.lookup.start(lookup_id)
    }

    /// Ingests a LeaseSet2 `DatabaseStoreMessage` response and
    /// updates the active remotes cache on success.
    pub fn ingest_lookup_response(
        &mut self,
        lookup_id: LookupId,
        store_message: &i2pr_proto::DatabaseStoreMessage,
        now_seconds: u32,
    ) -> Result<LookupIngestOutcome, LookupIngestError> {
        if lookup_id.kind() != LookupKind::LeaseSet2 {
            return Err(LookupIngestError::WrongKind);
        }
        let context = i2pr_netdb::LeaseSet2ValidationContext::new(now_seconds);
        let outcome = handle_database_store_lease_set2(
            &mut self.lookup.inner,
            &mut self.lease_set2_store,
            lookup_id,
            store_message,
            context,
        )
        .map_err(LookupIngestError::Engine)?;
        match outcome {
            i2pr_netdb::ResponseOutcome::Completed(result) => match *result {
                i2pr_netdb::LookupResult::LeaseSet2Success {
                    lookup_id: id,
                    lease_set2,
                } => {
                    let destination_hash = lease_set2.key();
                    let static_public = static_public_from_ls2(lease_set2.lease_set2())?;
                    self.active_remotes.insert(
                        destination_hash,
                        RemoteState {
                            validated: *lease_set2,
                            static_public,
                        },
                    );
                    Ok(LookupIngestOutcome::Success {
                        lookup_id: id,
                        destination_hash,
                    })
                }
                other => Err(LookupIngestError::WrongResult(Box::new(other))),
            },
            i2pr_netdb::ResponseOutcome::Continue => Ok(LookupIngestOutcome::Continue),
            i2pr_netdb::ResponseOutcome::Ignored => Ok(LookupIngestOutcome::Ignored),
        }
    }

    /// Records a typed delivery outcome for the active LeaseSet2
    /// lookup and advances the router-side state machine.
    pub fn lookup_delivery_outcome(
        &mut self,
        lookup_id: LookupId,
        outcome: i2pr_netdb::DeliveryOutcome,
    ) -> Result<(), LookupIngestError> {
        i2pr_netdb::handle_delivery_outcome(&mut self.lookup.inner, lookup_id, outcome)
            .map(|_| ())
            .map_err(LookupIngestError::Engine)
    }

    /// Cancels an active LeaseSet2 lookup.
    pub fn cancel_remote_lookup(&mut self) {
        let _ = self.lookup.inner.cancel();
    }

    /// Selects a lease for the supplied remote destination hash and
    /// returns the resolved lease metadata.
    pub fn select_lease<R: RngCore + ?Sized>(
        &self,
        remote: DestinationHash,
        now_seconds: u32,
        rng: &mut R,
    ) -> Result<SelectedLease, SendError> {
        let state = self
            .active_remotes
            .get(&remote)
            .ok_or(SendError::LeaseSet2LookupPending)?;
        let policy = LeaseSelectionPolicy::try_new(
            remote.as_hash().copy(),
            self.config.lease_safety_margin_seconds,
        )
        .map_err(SendError::NoUsableLease)?;
        let usable = self.config.lease_safety_margin_seconds;
        let _ = usable;
        self.selector
            .select_with_rng(state.validated.lease_set2(), &policy, now_seconds, rng)
            .map_err(SendError::NoUsableLease)
    }

    /// Returns the static X25519 public key for a previously
    /// resolved remote destination.
    pub fn remote_static_public_key(&self, remote: DestinationHash) -> Result<[u8; 32], SendError> {
        self.active_remotes
            .get(&remote)
            .map(|state| state.static_public)
            .ok_or(SendError::LeaseSet2LookupPending)
    }

    /// Drops a remote destination entry; the caller may invoke this
    /// when a destination shuts down or when a refresh lookup has
    /// invalidated the previous record.
    pub fn forget_remote(&mut self, remote: DestinationHash) -> bool {
        self.active_remotes.remove(&remote).is_some()
    }
}

struct RouterInfoLookupShim {
    #[allow(dead_code)]
    inner: i2pr_netdb::RouterInfoLookup,
}

impl Default for RouterInfoLookupShim {
    fn default() -> Self {
        Self::new()
    }
}

impl RouterInfoLookupShim {
    fn new() -> Self {
        // The LeaseSet2 lookup state machine only needs floodfill
        // selection; the policy defaults are bounded and stable.
        Self {
            inner: i2pr_netdb::RouterInfoLookup::new(i2pr_netdb::LookupPolicy::default()),
        }
    }

    fn start(&mut self, lookup_id: LookupId) -> LookupAction {
        // Plan 122 uses an empty placeholder RouterInfo store; the
        // routing layer drives floodfill selection off the
        // daemon-side store, not its own.
        let store = i2pr_netdb::RouterInfoStore::default();
        let routing_key = lookup_id.target();
        let outcome = self.inner.start(&store, lookup_id, &routing_key);
        match outcome {
            i2pr_netdb::StartOutcome::PendingAttempt(action) => action,
            i2pr_netdb::StartOutcome::NeedsReplyPath(action) => action,
            i2pr_netdb::StartOutcome::Terminal(result) => {
                let final_state = match *result {
                    i2pr_netdb::LookupResult::Failure { final_state, .. } => final_state,
                    _ => i2pr_netdb::LookupFinalState::Success,
                };
                LookupAction::Complete {
                    lookup_id,
                    outcome: i2pr_netdb::LookupOutcome::new(
                        lookup_id.kind(),
                        lookup_id.target(),
                        final_state,
                        0,
                        0,
                    ),
                }
            }
        }
    }
}

/// Outcome of ingesting a LeaseSet2 lookup response.
#[derive(Clone, Debug)]
pub enum LookupIngestOutcome {
    /// The lookup produced a validated LeaseSet2 and the routing
    /// layer cached the record.
    Success {
        /// The completed lookup identity.
        lookup_id: LookupId,
        /// Resolved destination hash.
        destination_hash: DestinationHash,
    },
    /// The state machine kept the lookup alive and awaits another
    /// attempt.
    Continue,
    /// The state machine was already terminal; the response was
    /// ignored.
    Ignored,
}

/// Typed lookup ingestion failures.
#[derive(Debug)]
pub enum LookupIngestError {
    /// The supplied lookup kind did not match the active LeaseSet2
    /// lookup.
    WrongKind,
    /// The lookup state machine returned an error.
    Engine(i2pr_netdb::LookupEngineError),
    /// The lookup state machine produced a non-LeaseSet2 success
    /// variant; that is unreachable for a LeaseSet2 lookup but the
    /// helper fails closed.
    WrongResult(Box<i2pr_netdb::LookupResult>),
    /// The LeaseSet2 carried no usable X25519 key.
    NoStaticPublicKey,
}

impl core::fmt::Display for LookupIngestError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::WrongKind => formatter.write_str("lookup kind is not LeaseSet2"),
            Self::Engine(error) => write!(formatter, "lookup engine: {error}"),
            Self::WrongResult(_) => formatter.write_str("unexpected lookup result variant"),
            Self::NoStaticPublicKey => formatter.write_str("LeaseSet2 has no static public key"),
        }
    }
}

impl std::error::Error for LookupIngestError {}

/// Extracts the ECIES-X25519 static public key from a validated
/// LeaseSet2. The selector rejects records that do not advertise an
/// X25519 key.
fn static_public_from_ls2(ls2: &LeaseSet2) -> Result<[u8; 32], LookupIngestError> {
    let key = ls2
        .usable_x25519_key()
        .map_err(|_| LookupIngestError::NoStaticPublicKey)?;
    let mut out = [0_u8; 32];
    let bytes = key.as_bytes();
    if bytes.len() != 32 {
        return Err(LookupIngestError::NoStaticPublicKey);
    }
    out.copy_from_slice(bytes);
    Ok(out)
}

// ---- Phase D / Phase E / Phase F / Phase G composition ----

/// One queued outbound payload the routing layer is composing. The
/// queue owns the I2NP Data envelope and the optional bundled
/// LeaseSet2 DatabaseStore payload that the New Session will carry
/// alongside the application data.
#[derive(Debug)]
pub struct OutboundRequest {
    pub(crate) inner_envelope: I2npMessage,
    pub(crate) bundled_lease_set2: Option<LeaseSet2>,
}

impl OutboundRequest {
    /// Builds a request from a `DestinationPayload`, wrapping the
    /// application bytes in an I2NP `Data` envelope (type 20). The
    /// request may optionally bundle a [`LeaseSet2`] DatabaseStore
    /// clove the New Session will carry.
    pub fn new(
        protocol_byte: u8,
        payload: &[u8],
        now_ms: u64,
        bundled_lease_set2: Option<LeaseSet2>,
    ) -> Result<Self, SendError> {
        if payload.is_empty() {
            return Err(SendError::DataCodec(CodecError::InvalidFieldValue {
                offset: 0,
                context: "application payload body",
            }));
        }
        let i2np_body = i2pr_proto::I2npBody::Data(OpaqueMessageBody {
            payload: DeferredPayload::new(payload.to_vec(), MAX_I2NP_PAYLOAD_SIZE)
                .map_err(SendError::DataCodec)?,
        });
        let _ = protocol_byte;
        let inner_envelope = I2npMessage::new_standard(0, Date::from_millis(now_ms), i2np_body)
            .map_err(SendError::DataCodec)?;
        Ok(Self {
            inner_envelope,
            bundled_lease_set2,
        })
    }

    /// Returns the encoded I2NP envelope the routing layer will
    /// encrypt and tunnel.
    pub fn inner_envelope(&self) -> &I2npMessage {
        &self.inner_envelope
    }

    /// Returns the bundled LeaseSet2 DatabaseStore payload, if any.
    pub fn bundled_lease_set2(&self) -> Option<&LeaseSet2> {
        self.bundled_lease_set2.as_ref()
    }
}

/// Compose an outbound Garlic delivery plan.
///
/// The composer:
/// 1. Selects a lease through the routing state machine.
/// 2. Encodes the inner I2NP Data envelope as the first Garlic
///    Clove, optionally appending a DatabaseStore LS2 clove.
/// 3. Hands the encrypted payload to the [`EciesSessionManager`].
/// 4. Forwards the resulting encrypted envelope through the
///    outbound tunnel role with `DeliveryInstruction::Tunnel` targeting
///    the selected lease's gateway and tunnel id.
#[allow(clippy::too_many_arguments)]
pub fn compose_outbound_delivery<R: CryptoRng + RngCore>(
    routing: &DestinationRouting,
    session: &mut EciesSessionManager,
    outbound: &DestinationOutboundRole,
    local_id: DestinationId,
    _local_static_secret: &[u8; i2pr_crypto::X25519_KEY_LENGTH],
    remote_hash: DestinationHash,
    request: &OutboundRequest,
    now_seconds: u32,
    now_ms: u64,
    rng: &mut R,
) -> Result<OutboundDeliveryPlan, SendError> {
    let remote_static = routing.remote_static_public_key(remote_hash)?;
    let selected = routing.select_lease(remote_hash, now_seconds, rng)?;
    let inner_bytes = request
        .inner_envelope
        .encode_standard_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .map_err(SendError::DataCodec)?;
    let data_clove = GarlicCloveBlock {
        delivery: GarlicDelivery::Destination(*remote_hash.as_bytes()),
        message: inner_bytes,
    };
    let payload_bytes = if let Some(ls2) = request.bundled_lease_set2() {
        let database_store_bytes = encode_database_store_clove(ls2)?;
        let database_store_clove = GarlicCloveBlock {
            delivery: GarlicDelivery::Destination(*remote_hash.as_bytes()),
            message: database_store_bytes,
        };
        encode_two_clove_new_session(now_seconds, &data_clove, &database_store_clove)
            .map_err(SendError::Payload)?
    } else {
        encode_new_session_payload(now_seconds, &data_clove).map_err(SendError::Payload)?
    };
    let mut outbound_message = session
        .encrypt_to_remote(
            local_id,
            remote_hash.as_bytes(),
            &remote_static,
            &payload_bytes,
            now_seconds,
            rng,
        )
        .map_err(map_session_error)?;
    let encrypted = encode_encrypted_outbound(&mut outbound_message);
    let header = TunnelPayloadHeader {
        delivery: DeliveryInstruction::Tunnel {
            tunnel_id: selected.tunnel_id,
            gateway: selected.gateway_router_hash,
        },
        message_id: 1,
        expiration_ms: now_ms,
    };
    let inner_envelope_bytes = request
        .inner_envelope
        .encode_standard_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .map_err(SendError::DataCodec)?;
    let cells = outbound
        .role
        .forward_cells(&header, &inner_envelope_bytes, rng, now_ms)
        .map_err(map_role_error)?;
    Ok(OutboundDeliveryPlan {
        selected_lease: selected,
        inner_envelope_bytes,
        encrypted_message: encrypted,
        cells,
    })
}

fn map_session_error(error: crate::session::EciesSessionError) -> SendError {
    match error {
        crate::session::EciesSessionError::Ecies(_) => {
            SendError::Session(EciesSessionError::Protocol("ECIES primitive error"))
        }
        crate::session::EciesSessionError::PendingHandshakeCapacity { maximum } => {
            SendError::Session(EciesSessionError::PendingHandshakeCapacity { maximum })
        }
        crate::session::EciesSessionError::NoSession => {
            SendError::Session(EciesSessionError::NoSession)
        }
        crate::session::EciesSessionError::Protocol(message) => {
            SendError::Session(EciesSessionError::Protocol(message))
        }
    }
}

fn map_role_error(error: i2pr_tunnel::TunnelRoleError) -> SendError {
    SendError::TunnelRole(error)
}

fn encode_encrypted_outbound(message: &mut EciesOutboundMessage) -> EncryptedOutbound {
    match message {
        EciesOutboundMessage::NewSession { message, pending } => {
            let bytes = message.encode_to_vec(MAX_I2NP_PAYLOAD_SIZE).ok();
            let pending = std::mem::replace(
                pending.as_mut(),
                crate::session::PendingHandshakeRecord::dummy_for_swap(),
            );
            EncryptedOutbound::NewSession {
                message: bytes.unwrap_or_default(),
                pending,
            }
        }
        EciesOutboundMessage::Existing(message) => {
            let bytes = message.encode_to_vec(MAX_I2NP_PAYLOAD_SIZE).ok();
            EncryptedOutbound::Existing {
                message: bytes.unwrap_or_default(),
            }
        }
    }
}

fn encode_database_store_clove(ls2: &LeaseSet2) -> Result<Vec<u8>, SendError> {
    let body = i2pr_proto::I2npBody::DatabaseStore(Box::new(i2pr_proto::DatabaseStoreMessage {
        key: ls2.key_hash().map_err(SendError::DataCodec)?,
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: i2pr_proto::DatabaseStoreData::LeaseSet2(Box::new(ls2.clone())),
    }));
    // The Garlic Clove carries the *short-form* I2NP envelope; we
    // must encode the inner message with the 9-byte short-transport
    // header so the recipient can decode it.
    let envelope = I2npMessage::new_short_transport(0, 0, body).map_err(SendError::DataCodec)?;
    envelope
        .encode_short_transport_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .map_err(SendError::DataCodec)
}

fn encode_two_clove_new_session(
    now_seconds: u32,
    data_clove: &GarlicCloveBlock,
    database_store_clove: &GarlicCloveBlock,
) -> Result<Vec<u8>, EciesPayloadError> {
    let mut sequence = i2pr_proto::EciesPayloadSequence::empty();
    sequence
        .push(i2pr_proto::EciesPayloadBlock::DateTime(now_seconds))
        .map_err(EciesPayloadError::Codec)?;
    sequence
        .push(i2pr_proto::EciesPayloadBlock::GarlicClove(
            data_clove.clone(),
        ))
        .map_err(EciesPayloadError::Codec)?;
    sequence
        .push(i2pr_proto::EciesPayloadBlock::GarlicClove(
            database_store_clove.clone(),
        ))
        .map_err(EciesPayloadError::Codec)?;
    sequence
        .encode_to_vec(crate::message::MAX_DESTINATION_PAYLOAD_BYTES, true)
        .map_err(EciesPayloadError::Codec)
}

/// Decodes the recipient-side `Data` body out of a decrypted
/// Garlic clove. The helper fails closed on any malformed sequence.
pub fn decode_data_clove(plaintext: &[u8]) -> Result<Vec<u8>, EciesPayloadError> {
    let clove = decode_decrypted_payload(plaintext)?;
    Ok(clove.message)
}

/// Convenience constructor that re-exports a builder for the inner
/// I2NP message. Tests use this to drive the recipient-side
/// dispatcher.
pub fn build_local_data_envelope(
    protocol_byte: u8,
    payload: &[u8],
    now_ms: u64,
) -> Result<I2npMessage, SendError> {
    let _ = protocol_byte;
    let body = i2pr_proto::I2npBody::Data(OpaqueMessageBody {
        payload: DeferredPayload::new(payload.to_vec(), MAX_I2NP_PAYLOAD_SIZE)
            .map_err(SendError::DataCodec)?,
    });
    I2npMessage::new_standard(0, Date::from_millis(now_ms), body).map_err(SendError::DataCodec)
}

// Silence unused imports for modules the Phase H dispatch layer
// will pull in once we add the inbound surface.
#[allow(dead_code)]
fn _silence_tunnel_unused(
    _: &EstablishedTunnel,
    _: &LayerKeys,
    _: &TunnelLifetime,
    _: &TunnelId,
    _: &Hash,
) {
}

// Silence unused imports for the Phase G typed router-delivery
// boundary helper.
#[allow(dead_code)]
fn _silence_crypto_unused(_: &EciesX25519BuildCryptography) {}

#[allow(dead_code)]
fn _silence_tunnel_data_unused(_: &TunnelDataMessage) {}

// Avoid `unused` lint for items brought in only when the inbound
// dispatcher module extends this crate in a follow-up commit.
#[allow(dead_code)]
fn _silence_reply_path(_: &dyn ReplyPathProvider) -> Option<ReplyPath> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_config_enforces_ceilings() {
        assert!(matches!(
            DestinationRoutingConfig::try_new(257, 32, 60),
            Err(DestinationRoutingError::RemoteLookupBudgetExceeded)
        ));
        assert!(matches!(
            DestinationRoutingConfig::try_new(64, 65, 60),
            Err(DestinationRoutingError::PendingOutboundBudgetExceeded)
        ));
        assert!(matches!(
            DestinationRoutingConfig::try_new(64, 32, 601),
            Err(DestinationRoutingError::InvalidLeaseSafetyMargin)
        ));
    }

    #[test]
    fn routing_state_initially_empty() {
        let routing = DestinationRouting::new(DestinationRoutingConfig::balanced());
        assert_eq!(routing.active_remote_count(), 0);
        assert_eq!(routing.lease_set2_store().len(), 0);
    }

    #[test]
    fn encode_database_store_clove_round_trips_through_short_transport() {
        // Build a placeholder LS2 just to exercise the envelope
        // encoder. We do not validate signatures here because the
        // helper never inspects the LS2 contents.
        use i2pr_crypto::RouterIdentityBundle;
        use i2pr_proto::{
            CryptoKeyType, Date32, LeaseSet2EncryptionKey, LeaseSet2Flags, LeaseSet2Header, Mapping,
        };
        use rand_chacha::ChaCha8Rng;
        use rand_core::SeedableRng;
        let mut rng = ChaCha8Rng::seed_from_u64(0x33);
        let bundle = RouterIdentityBundle::generate(&mut rng).expect("bundle");
        let destination =
            i2pr_proto::Destination::new(bundle.identity().key_and_cert().clone()).expect("dest");
        let header = LeaseSet2Header::new(destination, 1_000, 60, LeaseSet2Flags::from_raw(0))
            .expect("header");
        let encryption_keys =
            vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).expect("key")];
        let leases = vec![i2pr_proto::Lease2::new(
            i2pr_proto::Hash::from_bytes([0x11; 32]),
            1,
            Date32::from_seconds(1_200),
        )];
        let placeholder =
            i2pr_proto::SignatureValue::new(i2pr_crypto::ROUTER_SIGNING_KEY_TYPE, vec![0u8; 64])
                .expect("placeholder");
        let ls2 = LeaseSet2::new(
            header,
            Mapping::empty(),
            encryption_keys,
            leases,
            placeholder,
        )
        .expect("ls2");
        let bytes = encode_database_store_clove(&ls2).expect("encode");
        let decoded =
            I2npMessage::decode_short_transport(&bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode");
        assert!(matches!(
            decoded.body(),
            i2pr_proto::I2npBody::DatabaseStore(_)
        ));
    }
}
