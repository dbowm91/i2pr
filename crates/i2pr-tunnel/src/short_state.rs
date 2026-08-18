//! Short-build registrar and state-machine exports.
//!
//! Plan 116 §3.5 owns the success-gated registrar that converts a
//! terminal `ShortBuildStateMachine` `Established` outcome into a
//! successful `ExploratoryPool` registration by extracting the
//! real build-derived `EstablishedMaterial`. The registrar is the
//! only path through which completed builds enter the pool; it
//! refuses to report successful insertion semantics for any
//! outcome that lacks build-derived secret keys.
//!
//! The registrar and its supporting state module deliberately
//! remain runtime-neutral: no sockets, no Tokio runtime, no
//! filesystem persistence.

#![forbid(unsafe_code)]

use thiserror::Error;

use crate::established::EstablishedMaterial;
use crate::pool::{ExploratoryPool, PoolError, RegisterError, RegisterOutcome};

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
    /// The state machine reported success but the build-derived
    /// material could not be extracted.
    #[error("registrar could not extract established material: {0}")]
    Construction(String),
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
    /// The legacy `ShortBuildOutcome` surface was given an
    /// `Established` outcome without material; the registrar
    /// refuses to fabricate a successful registration.
    #[error("established material is required for a successful registration")]
    EstablishedMaterialRequired,
}

/// Short-build registrar that owns the ExploratoryPool integration.
///
/// The registrar is the single interface the build state machine
/// uses to admit tunnels into the pool. Pool admission happens
/// only after the state machine reports `Established` and after
/// the registrar has extracted the real build-derived
/// `EstablishedMaterial`. The legacy `admit(&ShortBuildOutcome,
/// ...)` surface fails closed when given a successful
/// `Established` outcome that is not paired with material.
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
    /// `ShortBuildStateMachine::take_established_material` call.
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

    /// Canonical success-only registrar path. The helper takes the
    /// build-derived `EstablishedMaterial` from the state machine
    /// in one expression and hands it to the pool. The state
    /// machine must already be in the `Established` phase.
    pub fn admit_established_machine(
        &mut self,
        machine: &mut super::short::ShortBuildStateMachine,
        now_seconds: u64,
    ) -> Result<RegisterOutcome, ShortRegistrarError> {
        let material =
            machine
                .take_established_material(now_seconds)
                .map_err(|error| match error {
                    super::short::ShortBuildConstructionError::NotEstablished => {
                        ShortRegistrarError::NotEstablished
                    }
                    super::short::ShortBuildConstructionError::EstablishedMaterialAlreadyTaken => {
                        ShortRegistrarError::AlreadyConsumed
                    }
                    super::short::ShortBuildConstructionError::EstablishedPathStateInvalid {
                        reason,
                    } => ShortRegistrarError::Construction(reason.to_string()),
                    other => ShortRegistrarError::Construction(other.to_string()),
                })?;
        self.admit_material(material, now_seconds)
    }

    /// Legacy registrar surface retained only for source-level
    /// compatibility. The registrar refuses to fabricate a
    /// successful registration from the legacy outcome alone:
    /// the call returns [`ShortRegistrarError::EstablishedMaterialRequired`]
    /// for an `Established` outcome (the registrar cannot rebuild
    /// the layer keys the outcome discards) and
    /// [`ShortRegistrarError::NotEstablished`] for every other
    /// outcome. New code must call
    /// [`Self::admit_material`] or
    /// [`Self::admit_established_machine`] instead.
    pub fn admit(
        &mut self,
        outcome: &super::short::ShortBuildOutcome,
        _slot: super::short::BuildAttemptId,
        now_seconds: u64,
    ) -> Result<RegisterOutcome, ShortRegistrarError> {
        self.pool.advance_time(now_seconds);
        let _ = self.pool.consecutive_failures();
        match outcome {
            super::short::ShortBuildOutcome::Established { .. } => {
                Err(ShortRegistrarError::EstablishedMaterialRequired)
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
    use crate::short::{BuildAttemptId, ShortBuildOutcome};

    #[test]
    fn registrar_rejects_non_success_outcome() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let mut registrar = ShortBuildRegistrar::new(&mut pool);
        let result = registrar.admit(&ShortBuildOutcome::TimedOut, BuildAttemptId::new(1), 0);
        assert!(matches!(result, Err(ShortRegistrarError::NotEstablished)));
    }

    #[test]
    fn registrar_rejects_legacy_established_outcome_without_material() {
        // Plan 116 §3.6: the legacy `admit` surface must fail
        // closed with `EstablishedMaterialRequired` when the
        // caller hands in an `Established` outcome without real
        // material. The pool length must remain zero.
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let mut registrar = ShortBuildRegistrar::new(&mut pool);
        let outcome = ShortBuildOutcome::Established {
            slot: crate::TunnelId::new(1).expect("id"),
            per_hop_replies: Vec::new(),
        };
        let result = registrar.admit(&outcome, BuildAttemptId::new(2), 0);
        assert!(matches!(
            result,
            Err(ShortRegistrarError::EstablishedMaterialRequired)
        ));
        assert_eq!(pool.inbound_len(), 0);
        assert_eq!(pool.outbound_len(), 0);
    }
}
