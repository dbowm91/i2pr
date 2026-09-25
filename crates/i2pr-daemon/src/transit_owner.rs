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
    dispatch_router_i2np_with_transit_bodies,
};
use crate::transit_compose::{
    TransitBuildService, TransitIngressGate, TransitServiceError, TransitTunnelDataDispatch,
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
    /// Transit forwarded one cell; the value is the bounded delivery
    /// outcome observed on the router seam.
    Forwarded(RouterDeliveryOutcome),
    /// Transit completed an OBEP semantic delivery.
    DeliveredObep(ObepDeliveryOutcome),
    /// Unknown receive id, wrong peer, replay, expiry, or malformed
    /// cell. Neither owner mutated state beyond the data-plane
    /// duplicate/peer-lock path.
    Dropped,
}

/// Typed outcome of one OBEP semantic delivery.
#[derive(Debug, Eq, PartialEq)]
pub enum ObepDeliveryOutcome {
    /// LOCAL action reached the installed bounded local consumer.
    LocalOk,
    /// ROUTER action produced one bounded router delivery.
    Router(RouterDeliveryOutcome),
    /// TUNNEL action preserved target gateway/tunnel and produced
    /// one bounded gateway delivery.
    Tunnel(RouterDeliveryOutcome),
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
    /// delivery outcome observed on the router seam.
    Dispatched(RouterDeliveryOutcome),
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
    Delivered {
        /// Cells accepted by the router seam.
        delivered: usize,
        /// Cells that failed delivery (bounded, explicit).
        failures: usize,
    },
    /// Unknown gateway tunnel id, wrong peer, non-IBGW role, or
    /// expiry. Fail closed.
    Dropped,
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
        let (outcome, bodies) = dispatch_router_i2np_with_transit_bodies(inbound, now_ms)
            .map_err(|_| TransitLiveError::LocalDeliveryFailed)?;
        match outcome {
            RouterI2npOutcome::TunnelBuildReserved {
                kind,
                message_id,
                peer,
                ..
            } => {
                match kind {
                    RouterI2npKind::ShortTunnelBuild => {
                        if self.creator_builds.contains(&(peer.hash(), message_id)) {
                            return Ok(LiveInboundOutcome::Build(LiveBuildOutcome::CreatorBypass));
                        }
                        if !self.is_enabled() {
                            return Ok(LiveInboundOutcome::Build(
                                LiveBuildOutcome::DisabledReserved,
                            ));
                        }
                        let Some(body) = bodies.short_build_body.as_deref() else {
                            return Ok(LiveInboundOutcome::Build(
                                LiveBuildOutcome::DisabledReserved,
                            ));
                        };
                        let (dispatch_opt, delivery) = {
                            let (owner, rng, cancellation) =
                                (&mut self.owner, &mut self.rng, &self.cancellation);
                            let gate = &mut owner.gate;
                            let dispatch =
                                gate.dispatch_short_build(body, &peer, now_seconds, false, rng);
                            let Some(dispatch) = dispatch else {
                                return Ok(LiveInboundOutcome::Build(
                                    LiveBuildOutcome::CreatorBypass,
                                ));
                            };
                            let service = gate
                                .service_mut()
                                .ok_or(TransitLiveError::LocalDeliveryFailed)?;
                            let delivery = match service
                                .deliver_dispatch_with_rollback(&dispatch, cancellation)
                            {
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
                            };
                            (dispatch, delivery)
                        };
                        let _ = dispatch_opt;
                        Ok(LiveInboundOutcome::Build(LiveBuildOutcome::Dispatched(
                            delivery,
                        )))
                    }
                    _ => Ok(LiveInboundOutcome::ReplyIgnored),
                }
            }
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
                Ok(LiveInboundOutcome::Ignored)
            }
            RouterI2npOutcome::RouterControl { .. } => Ok(LiveInboundOutcome::Ignored),
        }
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
                cell: next_cell,
                ..
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
                    delivery,
                )))
            }
            TransitTunnelDataDispatch::Deliver(action) => {
                let outcome = self.deliver_obep_action(&action, message_id)?;
                Ok(LiveInboundOutcome::Data(
                    TransitDataDisposition::DeliveredObep(outcome),
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
            return Ok(LiveInboundOutcome::Gateway(LiveGatewayOutcome::Dropped));
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
            return Ok(LiveInboundOutcome::Gateway(LiveGatewayOutcome::Dropped));
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
        Ok(LiveInboundOutcome::Gateway(LiveGatewayOutcome::Delivered {
            delivered,
            failures,
        }))
    }

    /// Delivers one OBEP semantic action through the existing daemon
    /// seams: LOCAL via the installed bounded sink, ROUTER via
    /// bounded direct router delivery, TUNNEL via canonical
    /// TunnelGateway delivery.
    pub fn deliver_obep_action(
        &mut self,
        action: &i2pr_tunnel::RouterDeliveryAction,
        message_id: u32,
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
                let delivery = service
                    .deliver_obep_router(action, &self.cancellation)
                    .map_err(TransitLiveError::Service)?;
                Ok(ObepDeliveryOutcome::Router(delivery))
            }
            RouterDeliveryKind::TunnelGateway => {
                let Some(service) = self.owner.gate.service_mut() else {
                    return Err(TransitLiveError::LocalDeliveryFailed);
                };
                let delivery = service
                    .deliver_obep_tunnel(action, message_id, &self.cancellation)
                    .map_err(TransitLiveError::Service)?;
                Ok(ObepDeliveryOutcome::Tunnel(delivery))
            }
        }
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
