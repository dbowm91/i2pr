//! Short-build registrar and state-machine exports.
//!
//! Plan 108 §3.6 + §8 owns the success-gated registrar that
//! converts a terminal `ShortBuildOutcome::Established` into a
//! successful `ExploratoryPool` registration. The registrar is
//! the only path through which completed builds enter the pool.
//!
//! The registrar and its supporting state module deliberately
//! remain runtime-neutral: no sockets, no Tokio runtime, no
//! filesystem persistence.

#![forbid(unsafe_code)]

use thiserror::Error;

use crate::established::EstablishedMaterial;
use crate::pool::{ExploratoryPool, PoolError, RegisterError, RegisterOutcome, TunnelSlot};

/// Per-hop reply outcome the registrar consumes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HopResponse {
    /// Index of the responding hop in canonical path order.
    pub hop_index: u8,
    /// True when the hop accepted.
    pub accepted: bool,
}

/// Errors that the registrar may return.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum ShortRegistrarError {
    /// The state machine outcome was not `Established`.
    #[error("registrar received non-success outcome")]
    NotEstablished,
    /// The pool rejected the registration.
    #[error("registrar failed: {0}")]
    Registration(RegisterError),
    /// The pool reached an inconsistent state.
    #[error("pool state error: {0}")]
    Pool(PoolError),
    /// The registrar was supplied an `EstablishedTunnel` whose
    /// internal extraction flag was already consumed (double-take).
    #[error("established material has already been consumed")]
    AlreadyConsumed,
}

/// Short-build registrar that owns the ExploratoryPool integration.
///
/// The registrar is the single interface the build state machine
/// uses to admit tunnels into the pool. Pool admission happens
/// only after the state machine reports `Established`.
#[derive(Debug)]
pub struct ShortBuildRegistrar<'a> {
    pool: &'a mut ExploratoryPool,
}

impl<'a> ShortBuildRegistrar<'a> {
    /// Constructs a registrar that borrows the supplied pool.
    pub const fn new(pool: &'a mut ExploratoryPool) -> Self {
        Self { pool }
    }

    /// Inserts the supplied `EstablishedMaterial` into the pool.
    /// The registrar hands the assigned `TunnelSlot` back to the
    /// caller. The caller must come from a successful
    /// `ShortBuildStateMachine::handle_event` outcome.
    pub fn admit_material(
        &mut self,
        established: EstablishedMaterial,
        now_seconds: u64,
    ) -> Result<RegisterOutcome, ShortRegistrarError> {
        self.pool.advance_time(now_seconds);
        let _ = self.pool.consecutive_failures();
        match established.direction() {
            crate::identity::TunnelDirection::Inbound => self
                .pool
                .register_inbound_with_material(established, now_seconds)
                .map_err(ShortRegistrarError::Registration),
            crate::identity::TunnelDirection::Outbound => self
                .pool
                .register_outbound_with_material(established, now_seconds)
                .map_err(ShortRegistrarError::Registration),
        }
    }

    /// Returns the assigned `TunnelSlot` from the supplied
    /// outcome. Used by the registrar API surface that still
    /// accepts the legacy `ShortBuildOutcome` argument.
    pub fn admit(
        &mut self,
        outcome: &super::short::ShortBuildOutcome,
        _slot: super::short::BuildAttemptId,
        now_seconds: u64,
    ) -> Result<RegisterOutcome, ShortRegistrarError> {
        self.pool.advance_time(now_seconds);
        let _ = self.pool.consecutive_failures();
        match outcome {
            super::short::ShortBuildOutcome::Established { slot, .. } => {
                // The legacy `Establish` outcome carries only the
                // creator tunnel id; the registrar cannot fabricate
                // a real `EstablishedMaterial` from it, so the
                // legacy surface returns the slot the caller
                // already recorded without mutating the pool.
                Ok(RegisterOutcome::Duplicate {
                    slot: TunnelSlot::from_raw(slot.get()),
                })
            }
            _ => Err(ShortRegistrarError::NotEstablished),
        }
    }
}

/// State machine bookkeeping value exposed to the registrar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShortBuildState {
    /// Build has not yet entered the state machine.
    Pending,
    /// State machine is currently in flight.
    Active,
    /// State machine reached a terminal success.
    Succeeded,
    /// State machine reached a terminal failure.
    Failed,
}

/// Convenience type alias to keep the public API responsive when
/// calling code mixes `PoolFullError` and the registrar's own
/// taxonomy.
pub type RegistrarFullError = crate::pool::PoolFullError;

/// Re-export a clean `TunnelDirection` alias to keep call
/// sites stable as the pool API evolves.
pub use crate::identity::TunnelDirection as ShortBuildDirectionError;

/// Stub for the state machine type that's only re-exported here so
/// library users can refer to `crate::ShortBuildStateMachine` from
/// `short_state`. The real implementation lives in `crate::short`.
pub use crate::short::ShortBuildStateMachine;

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::ExploratoryPoolConfig;
    use crate::pool::ExploratoryPool;

    #[test]
    fn registrar_rejects_non_success_outcome() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let mut registrar = ShortBuildRegistrar::new(&mut pool);
        let result = registrar.admit(
            &super::super::short::ShortBuildOutcome::TimedOut,
            crate::short::BuildAttemptId::new(1),
            0,
        );
        assert!(matches!(result, Err(ShortRegistrarError::NotEstablished)));
    }
}
