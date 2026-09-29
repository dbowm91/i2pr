//! Plan 254 live transit ingress owner: the bounded runtime seam that
//! drives the controlled [`TransitIngressGate`] from the real
//! authenticated SSU2/router-I2NP inbound path.
//!
//! Ordinary product profiles never enable the gate, so every
//! authenticated inbound `ShortTunnelBuild` keeps the existing
//! `TunnelBuildReserved` outcome. The controlled opt-in path installs
//! a [`TransitLiveOwner`] and exercises the live ingress pipeline
//! against the canonical
//! [`crate::router_i2np::RouterDeliveryService`] seam.
//!
//! ```text
//! Ssu2InboundI2np (real authenticated bytes)
//!   -> dispatch_router_i2np_with_transit_bodies (one canonical decode)
//!   -> creator correlation check
//!   -> if controlled transit disabled: preserve TunnelBuildReserved
//!   -> if enabled and non-creator: TransitOwner::dispatch_short_build(real body)
//!   -> deliver accepted/rejected result through existing bounded RouterDeliveryService
//! ```
//!
//! TunnelData ownership order is creator/service first (typed probe),
//! transit second, unknown fail-closed. The first successful owner
//! consumes the cell exactly once. OBEP `Deliver` actions route
//! through the existing delivery seams (LOCAL via the injected
//! bounded local sink, ROUTER/TUNNEL via `RouterDeliveryService`).
//! IBGW `TunnelGateway` ingress routes creator/service first, then
//! the runtime-neutral `process_tunnel_gateway`, delivering every
//! emitted cell through the bounded router-delivery seam.
//!
//! The owner never decodes I2NP envelopes itself; the single
//! canonical decode in `router_i2np` supplies every body. The owner
//! uses the outer owner's real cancellation token, never a fresh
//! token per dispatch.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::fmt;

use i2pr_proto::TunnelDataMessage;
use i2pr_runtime::CancellationToken;
use i2pr_transport::PeerId;
use rand_core::{CryptoRng, RngCore, TryCryptoRng};
use thiserror::Error;

use crate::router_i2np::{
    RouterDeliveryOutcome, RouterDeliveryService, RouterI2npKind, RouterI2npOutcome,
    dispatch_router_i2np_with_transit_bodies, extract_inner_short_build,
};
use crate::transit_compose::{
    SelfReplyOtbrmArgs, TransitBuildService, TransitDispatch, TransitDispatchRole,
    TransitIngressGate, TransitLiveStateSnapshot, TransitServiceError, TransitTunnelDataDispatch,
};

/// Live transit owner errors.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TransitOwnerError {
    /// The inner dispatch helper surfaced the typed service error.
    #[error("transit owner service error: {0}")]
    Service(#[from] TransitServiceError),
}

/// Live ingress errors surfaced by [`TransitLiveOwner`].
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TransitLiveError {
    /// The inner service surfaced a typed failure.
    #[error("transit live service error: {0}")]
    Service(#[from] TransitServiceError),
    /// An OBEP LOCAL action arrived with no bounded local consumer
    /// installed. The action is dropped explicitly, never silently.
    #[error("transit OBEP LOCAL action has no installed local consumer")]
    NoLocalConsumer,
    /// The installed bounded local consumer rejected the action.
    #[error("transit OBEP local consumer failed")]
    LocalDeliveryFailed,
}

/// Thin controlled gate wrapper. The gate is enabled only through the
/// controlled opt-in path; production product profiles never build
/// one.
pub struct TransitOwner {
    gate: TransitIngressGate,
}

impl fmt::Debug for TransitOwner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransitOwner")
            .field("gate", &self.gate)
            .finish_non_exhaustive()
    }
}

impl TransitOwner {
    /// Constructs a new live transit owner wrapping the supplied
    /// controlled transit service.
    pub fn new(service: TransitBuildService) -> Self {
        let mut gate = TransitIngressGate::disabled();
        gate.enable(service);
        Self { gate }
    }

    /// Returns whether the live transit owner is currently enabled.
    pub const fn is_enabled(&self) -> bool {
        self.gate.is_enabled()
    }

    /// Returns whether the supplied `RouterI2npOutcome` carries a
    /// build body the controlled transit gate consumes.
    pub const fn is_transit_managed(outcome: &RouterI2npOutcome) -> bool {
        matches!(
            outcome,
            RouterI2npOutcome::TunnelBuildReserved {
                kind: RouterI2npKind::ShortTunnelBuild,
                ..
            }
        )
    }

    /// Runs one full short-build dispatch cycle against the exact
    /// count-prefixed STBM body from the single canonical decode.
    ///
    /// `body` must be the `short_build_body` from
    /// [`dispatch_router_i2np_with_transit_bodies`], not a second
    /// decode and never an empty shim. `cancellation` is the outer
    /// owner's real token. Returns the typed router-delivery outcome
    /// the outer seam observes; every non-Accepted outcome rolls the
    /// just-committed registration back through the inner service.
    pub fn dispatch_short_build<R: TryCryptoRng>(
        &mut self,
        body: &[u8],
        peer: &PeerId,
        now_seconds: u64,
        is_creator_correlated: bool,
        cancellation: &CancellationToken,
        rng: &mut R,
    ) -> Result<RouterDeliveryOutcome, TransitOwnerError> {
        let dispatch =
            self.gate
                .dispatch_short_build(body, peer, now_seconds, is_creator_correlated, rng);
        let Some(dispatch) = dispatch else {
            return Ok(RouterDeliveryOutcome::Cancelled);
        };
        let outcome = self
            .gate
            .service_mut()
            .ok_or(TransitServiceError::DeliveryFailed)?
            .deliver_dispatch_with_rollback(&dispatch, cancellation)?;
        Ok(outcome)
    }

    /// Runs one TunnelData dispatch cycle through the inner service.
    pub fn dispatch_tunnel_data(
        &mut self,
        cell: &TunnelDataMessage,
        peer: &PeerId,
        now_ms: u64,
    ) -> TransitTunnelDataDispatch {
        self.gate
            .dispatch_tunnel_data(cell, peer, now_ms)
            .unwrap_or(TransitTunnelDataDispatch::Drop)
    }

    /// Borrows the inner service mutably when enabled.
    pub fn service_mut(&mut self) -> Option<&mut TransitBuildService> {
        self.gate.service_mut()
    }

    /// Drains transit state synchronously.
    pub fn cancel(&mut self) {
        self.gate.cancel();
    }
}

impl Drop for TransitOwner {
    fn drop(&mut self) {
        self.gate.cancel();
    }
}

/// Typed disposition of one live TunnelData cell.
#[derive(Debug, Eq, PartialEq)]
pub enum TransitDataDisposition {
    /// Creator/service probe owned the receive id; transit was never
    /// consulted.
    CreatorOwned,
    /// Transit was disabled; the caller keeps its existing path.
    Disabled,
    /// Transit forwarded one cell; the value carries the bounded
    /// routing facts the forward used plus the terminal delivery
    /// outcome observed on the router seam.
    Forwarded(TransitDataForwardEvidence),
    /// Transit completed an OBEP semantic delivery. The length is
    /// the reassembled message size in bytes and the inner type
    /// names the reassembled I2NP message: together they are the
    /// only semantic tags binding a delivery to its datagram
    /// (512- vs 4096-payload garlics reassemble to distinct
    /// sizes), so lane evidence can separate genuine datagram
    /// deliveries from LeaseSet publishes (DatabaseStore) and
    /// tunnel-test completions sharing the tunnel.
    DeliveredObep {
        /// Terminal delivery outcome.
        outcome: ObepDeliveryOutcome,
        /// Reassembled message length in bytes.
        message_len: usize,
        /// Inner I2NP message type of the reassembled bytes
        /// (`None` when empty, which never delivers).
        inner_type: Option<i2pr_proto::MessageType>,
        /// Next-hop router the delivered action addressed (`None`
        /// for LOCAL actions, which stay inside the daemon).
        target_router: Option<i2pr_proto::Hash>,
        /// Target tunnel id the delivered TUNNEL action addressed
        /// (`None` for LOCAL/ROUTER actions).
        target_tunnel: Option<u32>,
    },
    /// Unknown receive id, wrong peer, replay, expiry, or malformed
    /// cell. Neither owner mutated state beyond the data-plane
    /// duplicate/peer-lock path.
    Dropped,
}

/// Non-secret typed evidence for one forwarded transit data cell.
///
/// Plan 256 data-plane rows bind to these routing facts: the
/// receive tunnel id must resolve the accepted registration and
/// the output must address the expected next router/tunnel. No
/// cell bytes cross this boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitDataForwardEvidence {
    /// Receive tunnel id the registration recorded.
    pub receive_tunnel: u32,
    /// Next-hop router hash the registration recorded.
    pub next_router: i2pr_proto::Hash,
    /// Next-hop receive tunnel id the registration recorded.
    pub next_tunnel: u32,
    /// Terminal delivery outcome observed on the router seam.
    pub outcome: RouterDeliveryOutcome,
}

/// Typed outcome of one OBEP semantic delivery.
#[derive(Debug, Eq, PartialEq)]
pub enum ObepDeliveryOutcome {
    /// LOCAL action reached the installed bounded local consumer.
    LocalOk,
    /// ROUTER action produced one bounded router delivery.
    Router(RouterDeliveryOutcome),
    /// TUNNEL action preserved target gateway/tunnel and produced
    /// one bounded gateway delivery (remote path: target != local).
    Tunnel(RouterDeliveryOutcome),
    /// Self-target TUNNEL action entered the local IBGW
    /// registration through the source-neutral seam (Plan 262 work
    /// package D2). No synthetic `PeerId` was created, the local
    /// router was never inserted into the remote peer index, and
    /// the message never serialized through an artificial SSU2
    /// path. `delivered`/`failures` count per-cell router-seam
    /// outcomes; `nested_len` is the reconstructed standard I2NP
    /// byte count.
    LocalIbgwDelivered {
        /// IBGW receive id the self-loop addressed.
        receive_id: u32,
        /// Committed next-hop router every emitted cell addressed.
        next_router: i2pr_proto::Hash,
        /// Committed next-hop receive tunnel every emitted cell addressed.
        next_tunnel: u32,
        /// Cells accepted by the router seam.
        delivered: usize,
        /// Cells that failed delivery (bounded, explicit, no retry).
        failures: usize,
        /// Reconstructed nested standard I2NP length.
        nested_len: usize,
    },
    /// Self-target TUNNEL action failed closed without local
    /// ingress (unknown/zero/non-IBGW/expired/cancelled/malformed
    /// id or empty forward set). The `reason` is a
    /// secret-free reason class; `receive_id` is `Some` when the
    /// action carried a nonzero tunnel id.
    LocalIbgwDropped {
        /// Addressed tunnel id when nonzero.
        receive_id: Option<u32>,
        /// Secret-free reason class (`unknown-id`, `zero-id`,
        /// `non-ibgw-or-expired`, `cancelled`, `malformed-nested`,
        /// `empty-forward`, `delivery-overflow`).
        reason: &'static str,
    },
}

/// Typed outcome of one live short-build ingress.
#[derive(Debug, Eq, PartialEq)]
pub enum LiveBuildOutcome {
    /// Transit disabled; the caller preserves `TunnelBuildReserved`.
    DisabledReserved,
    /// Creator correlation precedes transit; the existing
    /// coordinator behavior is preserved.
    CreatorBypass,
    /// Transit dispatch completed; the value is the terminal
    /// delivery outcome plus the non-secret typed build evidence
    /// observed on the router seam.
    Dispatched(TransitBuildEvidence),
}

/// Non-secret typed evidence for one dispatched short-build
/// ingress through the controlled live owner.
///
/// Every `Some` field is already published on the wire by the
/// ECIES short-build exchange (role, receive tunnel id,
/// next/reply router, message id) or is a local delivery
/// disposition. No keys, reply material, or payload bytes cross
/// this boundary. Plan 256 binds each external role row to the
/// `role` kind recorded here; a generic `Dispatched` whose role
/// is `None` (fatal decode) or names the wrong role can never
/// satisfy a role-specific row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitBuildEvidence {
    /// Decoded hop-role kind from the local request record.
    /// `None` only when the payload never decoded (fatal
    /// input); no role row may consume a `None` observation.
    pub role: Option<i2pr_tunnel::TransitHopRoleKind>,
    /// Authenticated receive tunnel id this hop committed.
    /// `u32::MAX` when a code-30 rejection carried no slot;
    /// `0` when the payload never decoded (fatal input; tunnel
    /// id 0 is never a real registration).
    pub receive_tunnel: u32,
    /// Authenticated next/reply router hash when the dispatch
    /// carried one (forward continuations and OBEP
    /// terminations); `None` only when the route had no
    /// onward router.
    pub next_router: Option<i2pr_proto::Hash>,
    /// Authenticated next/reply message id (`0` when the route
    /// carried none).
    pub next_message_id: u32,
    /// True when admission refused and the transformed payload
    /// carries the normal code-30 policy rejection.
    pub rejected: bool,
    /// Terminal delivery outcome observed on the router seam.
    pub delivery: RouterDeliveryOutcome,
    /// Non-secret typed bandwidth disposition copied from the
    /// runtime-neutral transaction (Plan 257 work package E).
    /// Fatal inputs carry the empty (absent) summary.
    pub bandwidth: i2pr_tunnel::TransitBandwidthSummary,
}

impl TransitBuildEvidence {
    /// Returns the canonical short role label for sanitized evidence.
    pub const fn role_label(&self) -> &'static str {
        match self.role {
            Some(kind) => kind.label(),
            None => "fatal",
        }
    }

    /// Builds non-secret typed evidence from one completed
    /// dispatch plus its terminal delivery outcome. The helper
    /// copies only wire-published routing facts; transformed
    /// payloads, keys, and reply material stay inside the
    /// dispatch owner.
    pub fn from_dispatch(dispatch: &TransitDispatch, delivery: RouterDeliveryOutcome) -> Self {
        match dispatch {
            TransitDispatch::ForwardStbm {
                receive_tunnel,
                next_router,
                next_message_id,
                role_kind,
                bandwidth,
                ..
            } => Self {
                role: Some(*role_kind),
                receive_tunnel: receive_tunnel.get(),
                next_router: Some(*next_router),
                next_message_id: *next_message_id,
                rejected: false,
                delivery,
                bandwidth: *bandwidth,
            },
            TransitDispatch::EmitOtbrm {
                receive_tunnel,
                reply_router,
                reply_message_id,
                role_kind,
                bandwidth,
                ..
            } => Self {
                role: Some(*role_kind),
                receive_tunnel: receive_tunnel.get(),
                next_router: Some(*reply_router),
                next_message_id: *reply_message_id,
                rejected: false,
                delivery,
                bandwidth: *bandwidth,
            },
            TransitDispatch::Rejected {
                receive_tunnel,
                role,
                payload: _,
                role_kind,
                reason: _,
                bandwidth,
            } => {
                let (next_router, next_message_id) = match role {
                    TransitDispatchRole::ContinueStbm {
                        next_router,
                        next_message_id,
                    } => (Some(*next_router), *next_message_id),
                    TransitDispatchRole::TerminateOtbrm {
                        reply_router,
                        reply_message_id,
                        ..
                    } => (Some(*reply_router), *reply_message_id),
                };
                Self {
                    role: Some(*role_kind),
                    receive_tunnel: receive_tunnel.map_or(u32::MAX, |id| id.get()),
                    next_router,
                    next_message_id,
                    rejected: true,
                    delivery,
                    bandwidth: *bandwidth,
                }
            }
            TransitDispatch::Fatal => Self {
                role: None,
                receive_tunnel: 0,
                next_router: None,
                next_message_id: 0,
                rejected: false,
                delivery,
                bandwidth: i2pr_tunnel::TransitBandwidthSummary::new(
                    i2pr_tunnel::TransitBandwidthRequest::default(),
                    None,
                    false,
                ),
            },
        }
    }
}

/// Typed outcome of one live TunnelGateway ingress.
#[derive(Debug, Eq, PartialEq)]
pub enum LiveGatewayOutcome {
    /// Creator/service probe owned the gateway tunnel id.
    CreatorOwned,
    /// Transit disabled.
    Disabled,
    /// Transit IBGW delivered every emitted cell. `delivered` counts
    /// cells accepted by the router seam; `failures` counts bounded
    /// per-cell delivery failures (explicit, no retry).
    /// `receive_tunnel` is the gateway tunnel id the ingress
    /// addressed, so lane evidence can bind each delivery to the
    /// accepted registration (Plan 256 section 8).
    Delivered {
        /// Cells accepted by the router seam.
        delivered: usize,
        /// Cells that failed delivery (bounded, explicit).
        failures: usize,
        /// Gateway tunnel id the ingress addressed.
        receive_tunnel: u32,
        /// Encoded nested standard I2NP message length the
        /// ingress carried (Plan 258 failure telemetry: the
        /// already-decoded byte count, no new decode, no payload
        /// retention — the lane derives the single-cell versus
        /// multi-cell-capable size class from this fact).
        nested_len: usize,
        /// Next-hop router hash every emitted cell addressed (all
        /// cells of one ingress share the registration's committed
        /// tuple). Plan 260 binds the creator-owned inbound
        /// receipt tuple from this wire-published fact.
        next_router: i2pr_proto::Hash,
        /// Next-hop receive tunnel id every emitted cell addressed
        /// (the registration's committed next tunnel — the
        /// creator-local inbound tunnel id on the creator-owned
        /// path). Plan 260 binds the receipt tuple from this
        /// wire-published fact.
        next_tunnel: u32,
    },
    /// Unknown gateway tunnel id, wrong peer, non-IBGW role, or
    /// expiry. Fail closed. `tunnel_id` is the gateway tunnel id
    /// the ingress ADDRESSED (accepted or not) and `nested_len`
    /// the encoded nested length it carried (Plan 258 work
    /// package A drop-side telemetry: distinguishes relays to
    /// stale ids from drops at the accepted registration, and
    /// datagram-sized batches from maintenance trickle — the
    /// WP B attempt-1 evidence cannot tell them apart).
    Dropped { tunnel_id: u32, nested_len: usize },
}

/// Typed outcome of one real inbound message through the production
/// live owner.
#[derive(Debug, Eq, PartialEq)]
pub enum LiveInboundOutcome {
    /// Short-build ingress disposition.
    Build(LiveBuildOutcome),
    /// Outbound-build-reply arrivals never enter transit.
    ReplyIgnored,
    /// TunnelData ingress disposition.
    Data(TransitDataDisposition),
    /// TunnelGateway ingress disposition.
    Gateway(LiveGatewayOutcome),
    /// Any other body kind; transit takes no action.
    Ignored,
}

/// Bounded LOCAL consumer for OBEP semantic delivery. Installed by
/// the controlled harness; the production qualification installs the
/// real local-delivery seam through the same narrow setter.
pub type TransitLocalSink =
    Box<dyn FnMut(&i2pr_tunnel::RouterDeliveryAction) -> Result<(), TransitLiveError> + Send>;

/// The actual live authenticated SSU2/router-I2NP inbound owner for
/// controlled transit (Plan 254 work packages C–G).
///
/// Default construction is transit-disabled; controlled
/// tests/qualification enable transit through [`Self::enable`].
/// No public RouterInfo advertisement, no second socket or event
/// loop, no public config surface. Creator/service ownership probes
/// are typed `BTreeSet`s installed by the controlled harness; the
/// production daemon installs its real coordinator/registry facts
/// through the same narrow setters.
pub struct TransitLiveOwner<R> {
    owner: TransitOwner,
    rng: R,
    delivery: RouterDeliveryService,
    cancellation: CancellationToken,
    creator_builds: BTreeSet<(i2pr_proto::Hash, u32)>,
    creator_data: BTreeSet<u32>,
    creator_gateways: BTreeSet<u32>,
    local_sink: Option<TransitLocalSink>,
}

impl<R: TryCryptoRng + Send> fmt::Debug for TransitLiveOwner<R> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransitLiveOwner")
            .field("owner", &self.owner)
            .field("creator_builds", &self.creator_builds.len())
            .field("creator_data", &self.creator_data.len())
            .field("creator_gateways", &self.creator_gateways.len())
            .field("has_local_sink", &self.local_sink.is_some())
            .finish_non_exhaustive()
    }
}

impl<R> TransitLiveOwner<R>
where
    R: TryCryptoRng + RngCore + CryptoRng + Send,
{
    /// Constructs a transit-disabled live owner. Ordinary product
    /// construction uses this and never calls [`Self::enable`].
    pub fn new_disabled(
        delivery: RouterDeliveryService,
        cancellation: CancellationToken,
        rng: R,
    ) -> Self {
        Self {
            owner: TransitOwner {
                gate: TransitIngressGate::disabled(),
            },
            rng,
            delivery,
            cancellation,
            creator_builds: BTreeSet::new(),
            creator_data: BTreeSet::new(),
            creator_gateways: BTreeSet::new(),
            local_sink: None,
        }
    }

    /// Installs the controlled transit service. Explicit opt-in for
    /// tests/qualification only.
    pub fn enable(&mut self, service: TransitBuildService) {
        let mut gate = TransitIngressGate::disabled();
        gate.enable(service);
        self.owner.gate = gate;
    }

    /// Returns whether controlled transit is enabled.
    pub const fn is_enabled(&self) -> bool {
        self.owner.gate.is_enabled()
    }

    /// Records a creator-correlated `(peer, message_id)` build the
    /// live owner must bypass.
    pub fn install_creator_build(&mut self, peer: PeerId, message_id: u32) {
        self.creator_builds.insert((peer.hash(), message_id));
    }

    /// Records a creator/service-owned TunnelData receive id that
    /// must never reach transit.
    pub fn install_creator_data(&mut self, tunnel_id: u32) {
        self.creator_data.insert(tunnel_id);
    }

    /// Records a creator/service-owned TunnelGateway tunnel id that
    /// must bypass transit.
    pub fn install_creator_gateway(&mut self, tunnel_id: u32) {
        self.creator_gateways.insert(tunnel_id);
    }

    /// Installs the bounded LOCAL consumer for OBEP semantic
    /// delivery. Without a sink, LOCAL actions return
    /// [`TransitLiveError::NoLocalConsumer`] explicitly.
    pub fn install_local_sink<F>(&mut self, sink: F)
    where
        F: FnMut(&i2pr_tunnel::RouterDeliveryAction) -> Result<(), TransitLiveError>
            + Send
            + 'static,
    {
        self.local_sink = Some(Box::new(sink));
    }

    /// Returns the number of active transit registrations, or zero
    /// while disabled.
    pub fn active_count(&mut self) -> usize {
        self.owner
            .gate
            .service_mut()
            .map_or(0, |service| service.active_count())
    }

    /// Returns the bounded non-secret state snapshot (Plan 257 work
    /// package C). Read-only and side-effect free; zero while
    /// disabled. The qualification driver binds exact registration
    /// cardinality, cancellation drain, restart baseline,
    /// session-close, and pending-baseline rows to this snapshot.
    pub fn live_state_snapshot(&mut self) -> TransitLiveStateSnapshot {
        self.owner
            .gate
            .service_mut()
            .map_or(TransitLiveStateSnapshot::zero(), |service| {
                service.live_state_snapshot()
            })
    }

    /// Returns whether the transit peer index holds the supplied
    /// router (session-close B-retained proof). False while disabled.
    pub fn has_peer(&mut self, router: &i2pr_proto::Hash) -> bool {
        self.owner
            .gate
            .service_mut()
            .is_some_and(|service| service.has_peer(router))
    }

    /// Handles one real authenticated inbound message through the
    /// production owner path: single canonical decode, creator
    /// correlation check, controlled transit dispatch, bounded
    /// delivery with rollback. `now_seconds` is wall-clock seconds
    /// for admission; `now_ms` is wall-clock milliseconds for the
    /// data plane.
    pub fn handle_inbound(
        &mut self,
        inbound: &i2pr_runtime::Ssu2InboundI2np,
        now_ms: u64,
        now_seconds: u64,
    ) -> Result<LiveInboundOutcome, TransitLiveError> {
        if self.cancellation.is_cancelled() {
            // Cancelled owners drain and refuse further delivery;
            // short-build delivery reports Cancelled with no new
            // registration, data/gateway paths drop closed.
            self.owner.cancel();
        }
        // Undecodable input (malformed, expired, far-future,
        // oversize) never enters transit: the canonical decoder
        // already rejected it at the codec boundary (Plan 254
        // §B.11-12), so the owner drops it as Ignored with no
        // state change. Returning a hard error here would abort
        // the caller's inbound loop on routine stale input
        // (delayed retransmits, clock skew), which Plan 256
        // localized against exact-pinned i2pd traffic.
        let (outcome, bodies) = match dispatch_router_i2np_with_transit_bodies(inbound, now_ms) {
            Ok(value) => value,
            Err(_) => return Ok(LiveInboundOutcome::Ignored),
        };
        match outcome {
            RouterI2npOutcome::TunnelBuildReserved {
                kind,
                message_id,
                peer,
                ..
            } => match kind {
                RouterI2npKind::ShortTunnelBuild => {
                    let Some(body) = bodies.short_build_body.as_deref() else {
                        return Ok(LiveInboundOutcome::Build(
                            LiveBuildOutcome::DisabledReserved,
                        ));
                    };
                    self.dispatch_build_body(body, &peer, message_id, now_seconds)
                }
                _ => Ok(LiveInboundOutcome::ReplyIgnored),
            },
            RouterI2npOutcome::TunnelData {
                tunnel_id,
                message_id,
                peer,
                ..
            } => Ok(self.handle_tunnel_data_inner(
                tunnel_id,
                message_id,
                peer,
                bodies.tunnel_data_cell.as_ref(),
                now_ms,
            )?),
            RouterI2npOutcome::Unsupported { .. } => {
                if let Some(parts) = bodies.tunnel_gateway {
                    return self.handle_gateway_inner(parts, inbound.peer, now_ms);
                }
                if let Some(opaque) = bodies.garlic_opaque.as_deref() {
                    return self.dispatch_router_garlic(opaque, &inbound.peer, now_seconds);
                }
                Ok(LiveInboundOutcome::Ignored)
            }
            RouterI2npOutcome::RouterControl { .. } => Ok(LiveInboundOutcome::Ignored),
        }
    }

    /// Routes one inbound router garlic that may wrap a
    /// creator-routed short-build request (see
    /// [`TransitBuildService::route_router_garlic`]). Unwrappable
    /// input keeps the existing `Ignored` outcome; a recovered
    /// build flows through the shared deliver-with-rollback tail
    /// so evidence semantics match the direct path exactly.
    fn dispatch_router_garlic(
        &mut self,
        opaque: &[u8],
        peer: &PeerId,
        now_seconds: u64,
    ) -> Result<LiveInboundOutcome, TransitLiveError> {
        if !self.is_enabled() {
            return Ok(LiveInboundOutcome::Build(
                LiveBuildOutcome::DisabledReserved,
            ));
        }
        let dispatch_opt = {
            let (owner, rng) = (&mut self.owner, &mut self.rng);
            let gate = &mut owner.gate;
            let Some(service) = gate.service_mut() else {
                return Ok(LiveInboundOutcome::Build(
                    LiveBuildOutcome::DisabledReserved,
                ));
            };
            service.route_router_garlic(opaque, peer, now_seconds, rng)
        };
        let Some((dispatch, inner_msgid)) = dispatch_opt else {
            return Ok(LiveInboundOutcome::Ignored);
        };
        // Creator correlation uses the inner build message id the
        // service recovered; a match means our own coordinator
        // originated this build and transit must not claim it.
        if self.creator_builds.contains(&(peer.hash(), inner_msgid)) {
            return Ok(LiveInboundOutcome::Build(LiveBuildOutcome::CreatorBypass));
        }
        self.deliver_dispatched(dispatch, peer, now_seconds.saturating_mul(1_000))
    }

    /// Delivers one routed build dispatch with rollback and typed
    /// evidence. Shared tail for the direct, garlic-wrapped, and
    /// OBEP-redispatched build paths. `peer` is the authenticated
    /// transport sender of the build and `now_ms` the wall-clock
    /// millisecond stamp; both feed only the local-IBGW self-reply
    /// branch, which originates the reply here on behalf of the
    /// reply tunnel's path.
    fn deliver_dispatched(
        &mut self,
        dispatch: TransitDispatch,
        peer: &PeerId,
        now_ms: u64,
    ) -> Result<LiveInboundOutcome, TransitLiveError> {
        if self.is_self_reply(&dispatch) {
            return self.deliver_self_reply(dispatch, peer, now_ms);
        }
        let delivery = {
            let (owner, cancellation) = (&mut self.owner, &self.cancellation);
            let gate = &mut owner.gate;
            let service = gate
                .service_mut()
                .ok_or(TransitLiveError::LocalDeliveryFailed)?;
            match service.deliver_dispatch_with_rollback(&dispatch, cancellation) {
                Ok(outcome) => outcome,
                // A missing peer mapping is the same
                // terminal `NoActiveSession` the
                // router seam reports for a closed
                // session; the service already rolled
                // the registration back, so the live
                // owner propagates the terminal
                // result instead of a service error.
                Err(TransitServiceError::NoActiveSession(_)) => {
                    RouterDeliveryOutcome::NoActiveSession
                }
                Err(other) => {
                    return Err(TransitLiveError::Service(other));
                }
            }
        };
        let evidence = TransitBuildEvidence::from_dispatch(&dispatch, delivery);
        Ok(LiveInboundOutcome::Build(LiveBuildOutcome::Dispatched(
            evidence,
        )))
    }

    /// Returns true when the dispatch terminates an OBEP build
    /// (accept or code-30 rejection) whose decoded reply router is
    /// this router itself. The reference never sends such a reply
    /// on a session (`libi2pd/TransitTunnel.cpp`: `IBGW is local`);
    /// it injects the reply into its own gateway tunnel.
    fn is_self_reply(&mut self, dispatch: &TransitDispatch) -> bool {
        let Some(local) = self
            .owner
            .gate
            .service_mut()
            .map(|service| *service.hop_identity())
        else {
            return false;
        };
        match dispatch {
            TransitDispatch::EmitOtbrm { reply_router, .. } => *reply_router == local,
            TransitDispatch::Rejected {
                role: TransitDispatchRole::TerminateOtbrm { reply_router, .. },
                ..
            } => *reply_router == local,
            _ => false,
        }
    }

    /// Delivers one self-addressed OBEP reply through the local
    /// IBGW branch with the same rollback and typed-evidence
    /// semantics as the session path. The payload clone is bounded
    /// (one count-prefixed record set, at most eight records);
    /// borrowing the dispatch while the service mutably routes
    /// would alias the owner through the gate.
    fn deliver_self_reply(
        &mut self,
        dispatch: TransitDispatch,
        peer: &PeerId,
        now_ms: u64,
    ) -> Result<LiveInboundOutcome, TransitLiveError> {
        let (receive_tunnel, reply_tunnel, reply_message_id, payload) = match &dispatch {
            TransitDispatch::EmitOtbrm {
                receive_tunnel,
                reply_tunnel,
                reply_message_id,
                payload,
                ..
            } => (
                Some(*receive_tunnel),
                *reply_tunnel,
                *reply_message_id,
                payload.clone(),
            ),
            TransitDispatch::Rejected {
                receive_tunnel,
                role:
                    TransitDispatchRole::TerminateOtbrm {
                        reply_tunnel,
                        reply_message_id,
                        ..
                    },
                payload,
                ..
            } => (
                *receive_tunnel,
                *reply_tunnel,
                *reply_message_id,
                payload.clone(),
            ),
            _ => return Err(TransitLiveError::LocalDeliveryFailed),
        };
        let args = SelfReplyOtbrmArgs::new(
            receive_tunnel,
            reply_tunnel,
            reply_message_id,
            &payload,
            peer,
            now_ms,
        );
        let delivery = {
            let (owner, rng, cancellation) = (&mut self.owner, &mut self.rng, &self.cancellation);
            let service = owner
                .gate
                .service_mut()
                .ok_or(TransitLiveError::LocalDeliveryFailed)?;
            service.deliver_self_reply_otbrm(args, rng, cancellation)?
        };
        let evidence = TransitBuildEvidence::from_dispatch(&dispatch, delivery);
        Ok(LiveInboundOutcome::Build(LiveBuildOutcome::Dispatched(
            evidence,
        )))
    }

    /// Routes one count-prefixed short-build body through the
    /// controlled transit dispatch with rollback and typed
    /// evidence. Shared by the direct short-build ingress arm and
    /// the OBEP redispatch below so both paths observe identical
    /// creator-correlation, admission, rollback, and evidence
    /// semantics.
    fn dispatch_build_body(
        &mut self,
        body: &[u8],
        peer: &PeerId,
        message_id: u32,
        now_seconds: u64,
    ) -> Result<LiveInboundOutcome, TransitLiveError> {
        if self.creator_builds.contains(&(peer.hash(), message_id)) {
            return Ok(LiveInboundOutcome::Build(LiveBuildOutcome::CreatorBypass));
        }
        if !self.is_enabled() {
            return Ok(LiveInboundOutcome::Build(
                LiveBuildOutcome::DisabledReserved,
            ));
        }
        let dispatch = {
            let (owner, rng) = (&mut self.owner, &mut self.rng);
            let gate = &mut owner.gate;
            let dispatch = gate.dispatch_short_build(body, peer, now_seconds, false, rng);
            let Some(dispatch) = dispatch else {
                return Ok(LiveInboundOutcome::Build(LiveBuildOutcome::CreatorBypass));
            };
            dispatch
        };
        self.deliver_dispatched(dispatch, peer, now_seconds.saturating_mul(1_000))
    }

    fn handle_tunnel_data_inner(
        &mut self,
        tunnel_id: u32,
        message_id: u32,
        peer: PeerId,
        cell: Option<&TunnelDataMessage>,
        now_ms: u64,
    ) -> Result<LiveInboundOutcome, TransitLiveError> {
        // Ownership order: creator/service first, transit second,
        // unknown fail closed. The probe is consulted before any
        // transit state mutation.
        if self.creator_data.contains(&tunnel_id) {
            return Ok(LiveInboundOutcome::Data(
                TransitDataDisposition::CreatorOwned,
            ));
        }
        if !self.is_enabled() {
            return Ok(LiveInboundOutcome::Data(TransitDataDisposition::Disabled));
        }
        let Some(cell) = cell else {
            return Ok(LiveInboundOutcome::Data(TransitDataDisposition::Dropped));
        };
        let dispatch = {
            let gate = &mut self.owner.gate;
            match gate.dispatch_tunnel_data(cell, &peer, now_ms) {
                Some(value) => value,
                None => {
                    return Ok(LiveInboundOutcome::Data(TransitDataDisposition::Disabled));
                }
            }
        };
        match dispatch {
            TransitTunnelDataDispatch::Forward {
                next_router,
                next_tunnel,
                cell: next_cell,
            } => {
                if self.cancellation.is_cancelled() {
                    return Ok(LiveInboundOutcome::Data(TransitDataDisposition::Dropped));
                }
                let service = self
                    .owner
                    .gate
                    .service_mut()
                    .ok_or(TransitLiveError::LocalDeliveryFailed)?;
                let delivery = match service.deliver_tunnel_data_forward(
                    &next_router,
                    &next_cell,
                    message_id,
                    &self.cancellation,
                ) {
                    Ok(outcome) => outcome,
                    Err(TransitServiceError::NoActiveSession(_)) => {
                        RouterDeliveryOutcome::NoActiveSession
                    }
                    Err(other) => return Err(TransitLiveError::Service(other)),
                };
                // The cell is already consumed, so no rollback applies
                // to data forwards; the terminal outcome is observed.
                Ok(LiveInboundOutcome::Data(TransitDataDisposition::Forwarded(
                    TransitDataForwardEvidence {
                        receive_tunnel: tunnel_id,
                        next_router,
                        next_tunnel: next_tunnel.get(),
                        outcome: delivery,
                    },
                )))
            }
            TransitTunnelDataDispatch::Deliver(action) => {
                // A creator whose request-send path used one of its
                // own outbound tunnels delivers the build request
                // encapsulated in tunnel data instead of directly.
                // The inner message is still a first-hop build
                // request from the same previous peer, so an inner
                // ShortTunnelBuild re-enters the build dispatch;
                // every other payload delivers as before. A
                // same-tunnel retransmit is already suppressed by
                // the data-plane duplicate window before delivery,
                // so re-entry cannot double-install a registration.
                if let Some((body, inner_msgid)) = extract_inner_short_build(&action.message) {
                    return self.dispatch_build_body(&body, &peer, inner_msgid, now_ms / 1_000);
                }
                let message_len = action.message.len();
                let inner_type = action
                    .message
                    .first()
                    .map(|code| i2pr_proto::MessageType::from_code(*code));
                // The routing facts the delivered action used ride
                // with the disposition so lane evidence can bind each
                // OBEP delivery to its next router/tunnel (Plan 256
                // section 8 observation fields); LOCAL actions address
                // no next hop.
                use i2pr_tunnel::RouterDeliveryKind;
                let (target_router, target_tunnel) = match action.kind {
                    RouterDeliveryKind::Local => (None, None),
                    RouterDeliveryKind::Router => (Some(action.target_router), None),
                    RouterDeliveryKind::TunnelGateway => (
                        Some(action.target_router),
                        action.tunnel_id.map(|id| id.get()),
                    ),
                };
                let outcome = self.deliver_obep_action(&action, message_id, now_ms)?;
                Ok(LiveInboundOutcome::Data(
                    TransitDataDisposition::DeliveredObep {
                        outcome,
                        message_len,
                        inner_type,
                        target_router,
                        target_tunnel,
                    },
                ))
            }
            TransitTunnelDataDispatch::Drop => {
                Ok(LiveInboundOutcome::Data(TransitDataDisposition::Dropped))
            }
        }
    }

    fn handle_gateway_inner(
        &mut self,
        parts: crate::router_i2np::TransitGatewayParts,
        peer: PeerId,
        now_ms: u64,
    ) -> Result<LiveInboundOutcome, TransitLiveError> {
        if self.creator_gateways.contains(&parts.tunnel_id) {
            return Ok(LiveInboundOutcome::Gateway(
                LiveGatewayOutcome::CreatorOwned,
            ));
        }
        if !self.is_enabled() {
            return Ok(LiveInboundOutcome::Gateway(LiveGatewayOutcome::Disabled));
        }
        if self.cancellation.is_cancelled() {
            return Ok(LiveInboundOutcome::Gateway(LiveGatewayOutcome::Dropped {
                tunnel_id: parts.tunnel_id,
                nested_len: parts.nested.len(),
            }));
        }
        let forwards = {
            let (owner, rng) = (&mut self.owner, &mut self.rng);
            let Some(service) = owner.gate.service_mut() else {
                return Ok(LiveInboundOutcome::Gateway(LiveGatewayOutcome::Disabled));
            };
            service
                .route_tunnel_gateway(parts.tunnel_id, &parts.nested, &peer, now_ms, rng)
                .unwrap_or_default()
        };
        let Some(forwards) = forwards else {
            return Ok(LiveInboundOutcome::Gateway(LiveGatewayOutcome::Dropped {
                tunnel_id: parts.tunnel_id,
                nested_len: parts.nested.len(),
            }));
        };
        // Deliver every emitted cell through the existing bounded
        // router seam. Partial multi-cell failure is explicit and
        // bounded; no retry. Cancellation stops subsequent cells.
        let mut delivered = 0_usize;
        let mut failures = 0_usize;
        let mut message_id = parts.tunnel_id;
        for forward in &forwards {
            if self.cancellation.is_cancelled() {
                break;
            }
            message_id = message_id.wrapping_add(1);
            let service = self
                .owner
                .gate
                .service_mut()
                .ok_or(TransitLiveError::LocalDeliveryFailed)?;
            match service.deliver_tunnel_data_forward(
                &forward.next_router,
                &forward.cell,
                message_id,
                &self.cancellation,
            ) {
                Ok(RouterDeliveryOutcome::Accepted) => delivered += 1,
                Ok(_) | Err(_) => failures += 1,
            }
        }
        // Every emitted cell shares the registration's committed
        // next tuple; the first cell's facts name the whole
        // ingress (Plan 260 receipt-tuple binding). An empty
        // forward vector cannot name a tuple, so it fails closed
        // as a drop rather than fabricating routing facts.
        let Some(first) = forwards.first() else {
            return Ok(LiveInboundOutcome::Gateway(LiveGatewayOutcome::Dropped {
                tunnel_id: parts.tunnel_id,
                nested_len: parts.nested.len(),
            }));
        };
        Ok(LiveInboundOutcome::Gateway(LiveGatewayOutcome::Delivered {
            delivered,
            failures,
            receive_tunnel: parts.tunnel_id,
            nested_len: parts.nested.len(),
            next_router: first.next_router,
            next_tunnel: first.next_tunnel.get(),
        }))
    }

    /// Delivers one OBEP semantic action through the existing daemon
    /// seams: LOCAL via the installed bounded sink, ROUTER via
    /// bounded direct router delivery, TUNNEL via canonical
    /// TunnelGateway delivery with a Plan 262 self-loop arm.
    ///
    /// Plan 262 work package D2/D3: when a decoded OBEP TUNNEL
    /// action targets the local router itself
    /// (`action.target_router == local_router_hash`), the daemon
    /// takes `action.tunnel_id` and the already reconstructed
    /// standard I2NP bytes and calls the same source-neutral
    /// `route_ibgw_gateway` IBGW operation directly. It never
    /// creates a synthetic `PeerId`, never inserts the local
    /// router into the remote peer index, and never serializes
    /// the message through an artificial SSU2 path. When
    /// `target_router != local`, behavior remains bit-identical
    /// (authenticated remote peer resolution + bounded
    /// router-delivery seam).
    pub fn deliver_obep_action(
        &mut self,
        action: &i2pr_tunnel::RouterDeliveryAction,
        message_id: u32,
        now_ms: u64,
    ) -> Result<ObepDeliveryOutcome, TransitLiveError> {
        use i2pr_tunnel::RouterDeliveryKind;
        match action.kind {
            RouterDeliveryKind::Local => {
                let Some(sink) = self.local_sink.as_mut() else {
                    return Err(TransitLiveError::NoLocalConsumer);
                };
                sink(action).map_err(|_| TransitLiveError::LocalDeliveryFailed)?;
                Ok(ObepDeliveryOutcome::LocalOk)
            }
            RouterDeliveryKind::Router => {
                let Some(service) = self.owner.gate.service_mut() else {
                    return Err(TransitLiveError::LocalDeliveryFailed);
                };
                // A missing peer mapping is the same terminal
                // `NoActiveSession` the router seam reports for a
                // closed session (a lifecycle event, not an
                // internal failure), so the OBEP action surfaces it
                // as an observable delivery outcome instead of
                // aborting the caller's inbound loop.
                let delivery = match service.deliver_obep_router(action, &self.cancellation) {
                    Ok(outcome) => outcome,
                    Err(TransitServiceError::NoActiveSession(_)) => {
                        RouterDeliveryOutcome::NoActiveSession
                    }
                    Err(other) => return Err(TransitLiveError::Service(other)),
                };
                Ok(ObepDeliveryOutcome::Router(delivery))
            }
            RouterDeliveryKind::TunnelGateway => {
                // Self-loop check first: compare the decoded OBEP
                // target router with the service's own RouterIdentity
                // hash. This is trusted router-internal delivery
                // after OBEP decryption and local-router comparison;
                // it must not synthesize or spoof an authenticated
                // network peer.
                let local = {
                    let Some(service) = self.owner.gate.service_mut() else {
                        return Err(TransitLiveError::LocalDeliveryFailed);
                    };
                    *service.hop_identity()
                };
                if action.target_router == local {
                    return self.deliver_obep_tunnel_to_self(action, now_ms);
                }
                let Some(service) = self.owner.gate.service_mut() else {
                    return Err(TransitLiveError::LocalDeliveryFailed);
                };
                let delivery =
                    match service.deliver_obep_tunnel(action, message_id, &self.cancellation) {
                        Ok(outcome) => outcome,
                        Err(TransitServiceError::NoActiveSession(_)) => {
                            RouterDeliveryOutcome::NoActiveSession
                        }
                        Err(other) => return Err(TransitLiveError::Service(other)),
                    };
                Ok(ObepDeliveryOutcome::Tunnel(delivery))
            }
        }
    }

    /// Local self-loop for a decoded OBEP TUNNEL action whose
    /// target router is this router itself (Plan 262 WP D2).
    ///
    /// Takes the already reconstructed standard I2NP bytes and the
    /// addressed tunnel id and enters the same source-neutral
    /// `route_ibgw_gateway` seam the network `TunnelGateway` path
    /// uses. Unknown/zero/non-IBGW/expired/cancelled/malformed ids
    /// and empty forward sets fail closed as
    /// `LocalIbgwDropped` with a secret-free reason class. No
    /// synthetic peer, no peer-index mutation, no artificial SSU2
    /// serialization.
    fn deliver_obep_tunnel_to_self(
        &mut self,
        action: &i2pr_tunnel::RouterDeliveryAction,
        now_ms: u64,
    ) -> Result<ObepDeliveryOutcome, TransitLiveError> {
        if self.cancellation.is_cancelled() {
            return Ok(ObepDeliveryOutcome::LocalIbgwDropped {
                receive_id: action.tunnel_id.map(|id| id.get()),
                reason: "cancelled",
            });
        }
        let Some(tunnel_id) = action.tunnel_id.map(|id| id.get()) else {
            return Ok(ObepDeliveryOutcome::LocalIbgwDropped {
                receive_id: None,
                reason: "zero-id",
            });
        };
        if tunnel_id == 0 {
            return Ok(ObepDeliveryOutcome::LocalIbgwDropped {
                receive_id: None,
                reason: "zero-id",
            });
        }
        let nested_len = action.message.len();
        if nested_len == 0 {
            return Ok(ObepDeliveryOutcome::LocalIbgwDropped {
                receive_id: Some(tunnel_id),
                reason: "malformed-nested",
            });
        }
        // Source-neutral local ingress: the same seam the network
        // path uses, with no peer argument at all.
        let forwards = {
            let (owner, rng) = (&mut self.owner, &mut self.rng);
            let Some(service) = owner.gate.service_mut() else {
                return Err(TransitLiveError::LocalDeliveryFailed);
            };
            service
                .route_ibgw_gateway(tunnel_id, &action.message, now_ms, rng)
                .unwrap_or_default()
        };
        let Some(forwards) = forwards else {
            return Ok(ObepDeliveryOutcome::LocalIbgwDropped {
                receive_id: Some(tunnel_id),
                reason: "unknown-id-or-expired",
            });
        };
        if forwards.is_empty() {
            return Ok(ObepDeliveryOutcome::LocalIbgwDropped {
                receive_id: Some(tunnel_id),
                reason: "empty-forward",
            });
        }
        // Every emitted cell shares the registration's committed
        // next tuple; the first cell's facts name the whole
        // ingress for receipt-tuple binding.
        let (next_router, next_tunnel) = {
            let Some(first) = forwards.first() else {
                return Ok(ObepDeliveryOutcome::LocalIbgwDropped {
                    receive_id: Some(tunnel_id),
                    reason: "empty-forward",
                });
            };
            (first.next_router, first.next_tunnel.get())
        };
        let mut delivered = 0_usize;
        let mut failures = 0_usize;
        let mut out_message_id = tunnel_id;
        for forward in &forwards {
            if self.cancellation.is_cancelled() {
                break;
            }
            out_message_id = out_message_id.wrapping_add(1);
            let service = self
                .owner
                .gate
                .service_mut()
                .ok_or(TransitLiveError::LocalDeliveryFailed)?;
            match service.deliver_tunnel_data_forward(
                &forward.next_router,
                &forward.cell,
                out_message_id,
                &self.cancellation,
            ) {
                Ok(RouterDeliveryOutcome::Accepted) => delivered += 1,
                Ok(_) | Err(_) => failures += 1,
            }
        }
        // Ingress itself succeeded (the IBGW registration emitted
        // cells); per-cell forward failures are counted explicitly
        // in `failures` and the external receipt row requires zero
        // failures. Unit tests with no live sessions observe
        // `delivered == 0` with `failures > 0` but still prove
        // local ingress and cell emission.
        Ok(ObepDeliveryOutcome::LocalIbgwDelivered {
            receive_id: tunnel_id,
            next_router,
            next_tunnel,
            delivered,
            failures,
            nested_len,
        })
    }

    /// Installs the bounded next/reply-router -> session-peer mapping
    /// the live owner learned from an authenticated active session.
    /// Controlled tests install the mapping explicitly; the
    /// production qualification installs it from real session state.
    pub fn install_peer(
        &mut self,
        router: i2pr_proto::Hash,
        peer: PeerId,
    ) -> Result<(), TransitServiceError> {
        let Some(service) = self.owner.gate.service_mut() else {
            return Err(TransitServiceError::DeliveryFailed);
        };
        service
            .install_peer(router, peer)
            .map_err(|_| TransitServiceError::DeliveryFailed)
    }

    /// Sweeps expired registrations at the supplied logical time
    /// (seconds since the Unix epoch) and returns the removal
    /// count. The qualification lane advances the injected transit
    /// clock beyond creation + 600 s and sweeps here after proving
    /// post-lifetime data drops; the production daemon owns when
    /// sweeps run.
    pub fn expire(&mut self, now_seconds: u64) -> usize {
        self.owner.gate.expire(now_seconds)
    }

    /// Removes any transit peer mapping for the closed session peer.
    /// Established only from authenticated active session state.
    pub fn note_session_closed(&mut self, peer: &PeerId) -> bool {
        let Some(service) = self.owner.gate.service_mut() else {
            return false;
        };
        service.forget_peer_by_session(peer)
    }

    /// Marks the owner cancelled and drains transit synchronously.
    /// Idempotent; drop remains a fail-safe.
    pub fn cancel(&mut self) {
        self.cancellation
            .cancel(i2pr_core::CancellationReason::OperatorRequest);
        self.owner.cancel();
    }

    /// Returns the outer authoritative cancellation token.
    pub const fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    /// Returns the narrow outbound delivery capability for tests.
    pub const fn delivery(&self) -> &RouterDeliveryService {
        &self.delivery
    }
}

impl<R> Drop for TransitLiveOwner<R> {
    fn drop(&mut self) {
        self.owner.cancel();
    }
}

/// Production inbound-owner reference for the static guard.
///
/// The ordinary daemon SSU2 pump calls this helper for every
/// authenticated inbound message while controlled transit stays
/// disabled by default: the helper preserves the existing reserved
/// outcome and performs no transit dispatch. Controlled
/// tests/qualification construct [`TransitLiveOwner`] directly and
/// traverse the same canonical decode through
/// [`TransitLiveOwner::handle_inbound`].
pub fn controlled_transit_disabled_probe(outcome: &RouterI2npOutcome) -> bool {
    TransitOwner::is_transit_managed(outcome)
}
