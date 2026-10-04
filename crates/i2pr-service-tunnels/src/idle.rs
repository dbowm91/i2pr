//! Plan 292 idle-sweep decision (runtime-neutral, deterministic).
//!
//! The manager owns the sweep clock and runtime counters; this module
//! owns the pure per-tunnel decision so unit tests can pin every edge
//! (exact deadline, saturation, priority) without timers or sockets.
//! Priority is fixed: explicit destination rotation wins over close,
//! which wins over pool rebuild, which wins over reduction. An inert policy (deadline with no action, rejected at
//! every construction boundary) decides nothing here either.

use crate::config::IdlePolicy;

/// Idle action selected by [`idle_decision`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdleSweepAction {
    /// Stop the runtime (definition and persisted intent preserved;
    /// an operator start resumes from the stored definition).
    Close,
    /// Restart an ephemeral client service with a fresh Destination,
    /// keeping the service active on the new identity.
    RotateDestination,
    /// Rebuild tunnel pools in place under the same identity.
    RebuildPools,
    /// Reduce pool targets toward one via a runtime-only override
    /// (a restart restores the stored shaping).
    ReducePools,
}

/// Pure per-tunnel idle decision.
///
/// Fires only when nothing is active (no open streams and no Streamr
/// subscribers) and the quiet interval reaches the deadline. Uses
/// saturating arithmetic so clock jumps never panic or wrap.
pub fn idle_decision(
    policy: &IdlePolicy,
    active_connections: usize,
    streamr_subscribers: usize,
    last_activity_ms: u64,
    now_ms: u64,
) -> Option<IdleSweepAction> {
    if active_connections > 0 || streamr_subscribers > 0 {
        return None;
    }
    let idle_ms = now_ms.saturating_sub(last_activity_ms);
    let close_timeout = policy.close_timeout_ms.or(policy.timeout_ms);
    let reduce_timeout = policy.reduce_timeout_ms.or(policy.timeout_ms);
    if policy.rotate_destination_on_idle && close_timeout.is_some_and(|timeout| idle_ms >= timeout)
    {
        return Some(IdleSweepAction::RotateDestination);
    }
    if policy.close_on_idle && close_timeout.is_some_and(|timeout| idle_ms >= timeout) {
        return Some(IdleSweepAction::Close);
    }
    if policy.new_dest_on_idle && policy.timeout_ms.is_some_and(|timeout| idle_ms >= timeout) {
        return Some(IdleSweepAction::RebuildPools);
    }
    if policy.reduce_on_idle && reduce_timeout.is_some_and(|timeout| idle_ms >= timeout) {
        return Some(IdleSweepAction::ReducePools);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IdlePolicy;

    fn policy(timeout_ms: Option<u64>, close: bool, new_dest: bool, reduce: bool) -> IdlePolicy {
        IdlePolicy {
            timeout_ms,
            close_on_idle: close,
            new_dest_on_idle: new_dest,
            rotate_destination_on_idle: false,
            reduce_on_idle: reduce,
            close_timeout_ms: None,
            reduce_timeout_ms: None,
            reduce_count: None,
        }
    }

    #[test]
    fn disabled_and_inert_policies_decide_nothing() {
        assert_eq!(
            idle_decision(&IdlePolicy::disabled(), 0, 0, 0, u64::MAX),
            None
        );
        let inert = policy(Some(1_000), false, false, false);
        assert_eq!(idle_decision(&inert, 0, 0, 0, u64::MAX), None);
    }

    #[test]
    fn activity_suppresses_the_sweep() {
        let close = policy(Some(1_000), true, false, false);
        assert_eq!(idle_decision(&close, 1, 0, 0, 10_000), None);
        assert_eq!(idle_decision(&close, 0, 1, 0, 10_000), None);
        assert_eq!(
            idle_decision(&close, 0, 0, 0, 10_000),
            Some(IdleSweepAction::Close)
        );
    }

    #[test]
    fn deadline_edge_fires_exactly_at_timeout() {
        let reduce = policy(Some(1_000), false, false, true);
        assert_eq!(idle_decision(&reduce, 0, 0, 9_000, 9_999), None);
        assert_eq!(
            idle_decision(&reduce, 0, 0, 9_000, 10_000),
            Some(IdleSweepAction::ReducePools)
        );
        assert_eq!(
            idle_decision(&reduce, 0, 0, 9_000, 11_000),
            Some(IdleSweepAction::ReducePools)
        );
    }

    #[test]
    fn existing_priority_is_close_then_rebuild_then_reduce() {
        let all = policy(Some(1_000), true, true, true);
        assert_eq!(
            idle_decision(&all, 0, 0, 0, 5_000),
            Some(IdleSweepAction::Close)
        );
        let rebuild = policy(Some(1_000), false, true, true);
        assert_eq!(
            idle_decision(&rebuild, 0, 0, 0, 5_000),
            Some(IdleSweepAction::RebuildPools)
        );
    }

    #[test]
    fn proposal_destination_rotation_precedes_idle_close() {
        let mut rotate = policy(Some(1_000), true, false, true);
        rotate.rotate_destination_on_idle = true;
        assert_eq!(
            idle_decision(&rotate, 0, 0, 0, 1_000),
            Some(IdleSweepAction::RotateDestination)
        );
    }

    #[test]
    fn proposal_action_deadlines_are_independent_and_allow_zero() {
        let mut configured = policy(None, true, false, true);
        configured.close_timeout_ms = Some(30 * 60 * 1000);
        configured.reduce_timeout_ms = Some(20 * 60 * 1000);
        configured.reduce_count = Some(1);
        assert_eq!(
            idle_decision(&configured, 0, 0, 0, 20 * 60 * 1000),
            Some(IdleSweepAction::ReducePools)
        );
        assert_eq!(
            idle_decision(&configured, 0, 0, 0, 30 * 60 * 1000),
            Some(IdleSweepAction::Close)
        );

        configured.close_timeout_ms = Some(0);
        assert_eq!(
            idle_decision(&configured, 0, 0, 0, 0),
            Some(IdleSweepAction::Close)
        );
    }

    #[test]
    fn clock_jumps_saturate_without_wrapping() {
        let close = policy(Some(1_000), true, false, false);
        // now before last (clock step): saturates to zero, no fire.
        assert_eq!(idle_decision(&close, 0, 0, 10_000, 9_000), None);
        // Max values: saturates, fires exactly once by rule.
        assert_eq!(
            idle_decision(&close, 0, 0, 0, u64::MAX),
            Some(IdleSweepAction::Close)
        );
    }
}
