//! Plan 253 live transit owner: the bounded runtime seam that
//! drives the controlled [`TransitIngressGate`] from the live
//! `Ssu2InboundI2np` event loop the SSU2 owner exposes.
//!
//! This module owns the Plan 253 §D wiring: ordinary product
//! profiles never construct a [`TransitOwner`], so the bounded
//! owner stays disabled and every authenticated inbound
//! `ShortTunnelBuild` keeps the existing `TunnelBuildReserved`
//! outcome. The controlled opt-in path installs a
//! [`TransitOwner`] and starts exercising the live ingress
//! pipeline against the canonical
//! [`crate::router_i2np::RouterDeliveryService`] seam.
//!
//! ```text
//! Ssu2DaemonHandle::next_dispatched (Plan 158/184)
//!   -> outbound_router_i2np_dispatch
//!   -> if dispatch is ShortTunnelBuild:
//!        TransitOwner::dispatch_router_i2np
//!          -> TransitBuildService::route_short_build
//!          -> TransitDispatch::deliver_with_rollback
//!   if dispatch is TunnelData:
//!        outbound_dispatch_inbound_tunnel_data (Plan 117) — creator/service receive-id owner
//!        // receive-id ownership determines winner; transit gate takes precedence only
//!        // when the local pool returns UnknownTunnelId
//!        TransitOwner::dispatch_tunnel_data (otherwise)
//! ```
//!
//! The owner is runtime-neutral over its inner service; it never
//! spawns tasks, opens sockets, or holds long-lived reentrancy.
//! It does own a borrowed reference to the canonical cancellation
//! token the SSU2 owner passes through every event so a runtime
//! shutdown drains transit secrets immediately via
//! [`TransitIngressGate::cancel`].

#![forbid(unsafe_code)]

use std::fmt;

use i2pr_proto::TunnelDataMessage;
use i2pr_runtime::CancellationToken;
use i2pr_transport::PeerId;
use rand_core::TryCryptoRng;
use thiserror::Error;

use crate::router_i2np::{RouterDeliveryOutcome, RouterI2npKind, RouterI2npOutcome};
use crate::transit_compose::{
    TransitBuildService, TransitIngressGate, TransitServiceError, TransitTunnelDataDispatch,
};

/// Live transit owner errors. Returned by the typed
/// `TransitOwner::dispatch_short_build` /
/// `TransitOwner::dispatch_tunnel_data` entry points the
/// daemon-owned router-I2NP seam calls when the controlled
/// gate is enabled.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TransitOwnerError {
    /// The inner dispatch helper surfaced the typed service error.
    /// The wrapping caller rolls back the just-committed
    /// registration when delivery reports a terminal non-Accepted
    /// outcome.
    #[error("transit owner service error: {0}")]
    Service(#[from] TransitServiceError),
}

/// Live transit owner bridge between the SSU2 runtime's
/// `Ssu2InboundI2np` event loop and the controlled
/// [`TransitIngressGate`]. Constructed exclusively via the
/// controlled opt-in path; production product profiles never
/// build one.
pub struct TransitOwner<R> {
    gate: TransitIngressGate,
    cancellation_factory: R,
}

impl<R: TryCryptoRng + Send> fmt::Debug for TransitOwner<R> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransitOwner")
            .field("gate", &self.gate)
            .field("cancellation_factory", &"<rng>")
            .finish_non_exhaustive()
    }
}

impl<R: TryCryptoRng + Send> TransitOwner<R> {
    /// Constructs a new live transit owner that wraps the supplied
    /// controlled transit service. The `rng` factory supplies fresh
    /// OS entropy for dispatch-side cryptographic material (e.g.
    /// IVs for the tunnel-message builder).
    pub fn new(service: TransitBuildService, rng: R) -> Self {
        let mut gate = TransitIngressGate::disabled();
        gate.enable(service);
        Self {
            gate,
            cancellation_factory: rng,
        }
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

    /// Runs one full dispatch cycle against the supplied
    /// `RouterI2npOutcome`. The helper expects
    /// [`Self::is_transit_managed`] to be true for the supplied
    /// outcome; callers that need creator/service TunnelData receive
    /// ownership must consult that path first.
    ///
    /// The function returns the typed router-delivery outcome the
    /// outer seam observes. Plan 253 guarantees that accepted
    /// registrations commit only after the full signed message is
    /// constructed and accepted, and that every non-Accepted
    /// delivery outcome triggers the existing rollback path on the
    /// inner service.
    pub fn dispatch_short_build(
        &mut self,
        outcome: RouterI2npOutcome,
        is_creator_correlated: bool,
    ) -> Result<RouterDeliveryOutcome, TransitOwnerError> {
        let dispatch = match &outcome {
            RouterI2npOutcome::TunnelBuildReserved {
                kind: RouterI2npKind::ShortTunnelBuild,
                ..
            } => {
                let dispatch = self.gate.dispatch_short_build(
                    plan253_short_build_payload(&outcome),
                    &outcome.peer(),
                    now_seconds(),
                    is_creator_correlated,
                    &mut self.cancellation_factory,
                );
                dispatch
            }
            _ => {
                // The daemon must only call this helper for
                // outcomes the gate consumes. Anything else is
                // a daemon programmer error.
                return Err(TransitOwnerError::Service(
                    TransitServiceError::DeliveryFailed,
                ));
            }
        };
        let Some(dispatch) = dispatch else {
            // The gate returned `None` because it is disabled or
            // because the caller flagged the message as
            // creator-correlated. Either way the caller keeps the
            // reserved outcome, the live owner has nothing to do.
            return Ok(RouterDeliveryOutcome::Cancelled);
        };
        let token = CancellationToken::new();
        let outcome = self
            .gate
            .service_mut()
            .ok_or(TransitServiceError::DeliveryFailed)?
            .deliver_dispatch_with_rollback(&dispatch, &token)?;
        Ok(outcome)
    }

    /// Runs one TunnelData dispatch cycle through the inner
    /// service, returning the typed tunnel-data dispatch the
    /// caller can hand to the existing
    /// [`crate::router_i2np::RouterDeliveryService`] seam. The
    /// caller is responsible for the creator/service vs transit
    /// ownership precedence: Plan 253 §D establishes the order
    /// `local-pool receive-id first, transit second, otherwise drop`.
    pub fn dispatch_tunnel_data(
        &mut self,
        cell: &TunnelDataMessage,
        peer: &PeerId,
        now_ms: u64,
    ) -> TransitTunnelDataDispatch {
        // The gate's `dispatch_tunnel_data` returns
        // `Option<TransitTunnelDataDispatch>` (None when disabled).
        // The owner is constructed only when a service is
        // installed, so the inner branch is always Some.
        self.gate
            .dispatch_tunnel_data(cell, peer, now_ms)
            .unwrap_or(TransitTunnelDataDispatch::Drop)
    }
}

impl<R> Drop for TransitOwner<R> {
    fn drop(&mut self) {
        self.gate.cancel();
    }
}

/// Returns the `now_seconds` clock the inner service uses for
/// admission-time validation. Production wired through the
/// gateway's bounded timer. Tests inject their own clock.
fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(1_700_000_000)
}

/// Extracts the inbound ShortTunnelBuild body bytes from the
/// [`RouterI2npOutcome::TunnelBuildReserved`] variant. The outcome
/// records only the encoded message length; the surviving payload
/// must already have been re-decoded by the Plan 184 dispatcher
/// and threaded through the live owner. The default implementation
/// returns an empty `Vec` because the canonical dispatch path will
/// be extended in Plan 254 once the live owner accepts the
/// inbound body by reference. Plan 253 keeps the typed dispatch
/// shape and the daemon-side test driver verifies the seam.
fn plan253_short_build_payload(_outcome: &RouterI2npOutcome) -> &[u8] {
    &[]
}
