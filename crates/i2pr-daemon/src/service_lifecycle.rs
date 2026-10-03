//! Bounded normal-daemon Destination-group startup and retirement policy.
//!
//! This module owns only local lifecycle phase/timing information. It carries no
//! peer, Destination, address, or tunnel identifiers.

use std::time::Duration;

use i2pr_runtime::CancellationToken;
use tokio::sync::watch;

/// Reference-informed startup spacing between the inbound and outbound pools.
pub(crate) const OUTBOUND_POOL_START_DELAY: Duration = Duration::from_secs(1);
/// i2pr's initial service-retirement target: one normal tunnel lifetime.
pub(crate) const SERVICE_RETIREMENT_TARGET: Duration = Duration::from_secs(10 * 60);
/// Absolute cap, including any active connection tail.
pub(crate) const SERVICE_RETIREMENT_HARD_CAP: Duration = Duration::from_secs(11 * 60);

/// Router shutdown intent consumed by the one normal group owner.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum LifecycleCommand {
    #[default]
    Running,
    Graceful,
    Hard,
}

/// Coarse local lifecycle state; values contain no peer or service identity.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum LifecycleStatus {
    #[default]
    Starting,
    NoGroups,
    Active,
    Retiring {
        remaining: RemainingBucket,
    },
    Drained,
    HardStopped,
    Failed,
}

/// Coarse remaining-time bucket for local status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RemainingBucket {
    UnderOneMinute,
    OneToFiveMinutes,
    OverFiveMinutes,
}

impl RemainingBucket {
    fn from_millis(remaining_ms: u64) -> Self {
        if remaining_ms < 60_000 {
            Self::UnderOneMinute
        } else if remaining_ms < 5 * 60_000 {
            Self::OneToFiveMinutes
        } else {
            Self::OverFiveMinutes
        }
    }
}

/// Shared command/status handles between daemon shutdown and the SSU2 group
/// owner. The watch channels retain only their latest bounded state.
#[derive(Clone, Debug)]
pub(crate) struct ServiceLifecycleController {
    command: watch::Sender<LifecycleCommand>,
    status: watch::Sender<LifecycleStatus>,
}

impl ServiceLifecycleController {
    pub(crate) fn new() -> Self {
        let (command, _) = watch::channel(LifecycleCommand::Running);
        let (status, _) = watch::channel(LifecycleStatus::Starting);
        Self { command, status }
    }

    pub(crate) fn status(&self) -> LifecycleStatus {
        *self.status.borrow()
    }

    pub(crate) fn command(&self) -> LifecycleCommand {
        *self.command.borrow()
    }

    pub(crate) fn set_status(&self, status: LifecycleStatus) {
        self.status.send_replace(status);
    }

    pub(crate) fn request_graceful(&self) {
        if *self.command.borrow() == LifecycleCommand::Running {
            self.command.send_replace(LifecycleCommand::Graceful);
        }
    }

    pub(crate) fn request_hard(&self) {
        self.command.send_replace(LifecycleCommand::Hard);
        self.status.send_replace(LifecycleStatus::HardStopped);
    }

    pub(crate) async fn wait_for_terminal_status(&self) -> LifecycleStatus {
        let mut status = self.status.subscribe();
        loop {
            let current = *status.borrow_and_update();
            if matches!(
                current,
                LifecycleStatus::NoGroups
                    | LifecycleStatus::Drained
                    | LifecycleStatus::HardStopped
                    | LifecycleStatus::Failed
            ) {
                return current;
            }
            if status.changed().await.is_err() {
                return LifecycleStatus::Failed;
            }
        }
    }
}

/// Pure retirement state used by the production group product and
/// manual-time lifecycle tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RetirementState {
    Retiring {
        natural_deadline_ms: u64,
        hard_deadline_ms: u64,
    },
    Drained,
    HardCapReached,
    HardStopped,
}

impl RetirementState {
    pub(crate) fn begin(
        now_ms: u64,
        published_lease_expiry_ms: Option<u64>,
        active_connections: usize,
    ) -> Self {
        let hard_deadline_ms = now_ms.saturating_add(
            SERVICE_RETIREMENT_HARD_CAP
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        );
        let target_deadline_ms = now_ms.saturating_add(
            SERVICE_RETIREMENT_TARGET
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        );
        let natural_deadline_ms = published_lease_expiry_ms
            .unwrap_or(now_ms)
            .max(now_ms)
            .min(target_deadline_ms);
        if active_connections == 0 && natural_deadline_ms == now_ms {
            Self::Drained
        } else {
            Self::Retiring {
                natural_deadline_ms,
                hard_deadline_ms,
            }
        }
    }

    pub(crate) fn advance(self, now_ms: u64, active_connections: usize) -> Self {
        match self {
            Self::Retiring {
                natural_deadline_ms: _,
                hard_deadline_ms,
            } if now_ms >= hard_deadline_ms => Self::HardCapReached,
            Self::Retiring {
                natural_deadline_ms,
                ..
            } if now_ms >= natural_deadline_ms && active_connections == 0 => Self::Drained,
            other => other,
        }
    }

    pub(crate) fn hard_stop(self) -> Self {
        match self {
            Self::Drained | Self::HardCapReached | Self::HardStopped => self,
            Self::Retiring { .. } => Self::HardStopped,
        }
    }

    pub(crate) fn status(self, now_ms: u64) -> LifecycleStatus {
        match self {
            Self::Retiring {
                natural_deadline_ms,
                hard_deadline_ms,
            } => {
                let deadline = natural_deadline_ms.min(hard_deadline_ms);
                LifecycleStatus::Retiring {
                    remaining: RemainingBucket::from_millis(deadline.saturating_sub(now_ms)),
                }
            }
            Self::Drained | Self::HardCapReached => LifecycleStatus::Drained,
            Self::HardStopped => LifecycleStatus::HardStopped,
        }
    }
}

/// Waits one bounded inbound-first startup interval and remains promptly
/// cancellable by the owning service runtime.
pub(crate) async fn wait_for_outbound_pool_start(cancellation: &CancellationToken) -> bool {
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => false,
        _ = tokio::time::sleep(OUTBOUND_POOL_START_DELAY) => true,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use i2pr_runtime::CancellationToken;

    use super::{
        LifecycleStatus, RetirementState, SERVICE_RETIREMENT_HARD_CAP, SERVICE_RETIREMENT_TARGET,
    };

    #[tokio::test(start_paused = true)]
    async fn outbound_start_waits_full_second_and_observes_cancellation() {
        let cancellation = CancellationToken::new();
        let wait = tokio::spawn({
            let cancellation = cancellation.clone();
            async move { super::wait_for_outbound_pool_start(&cancellation).await }
        });
        tokio::time::advance(Duration::from_millis(999)).await;
        assert!(!wait.is_finished());
        tokio::time::advance(Duration::from_millis(1)).await;
        assert!(wait.await.expect("delay task completed"));

        let cancelled = CancellationToken::new();
        cancelled.cancel(i2pr_core::CancellationReason::OperatorRequest);
        assert!(!super::wait_for_outbound_pool_start(&cancelled).await);
    }

    #[test]
    fn zero_group_state_drains_without_waiting() {
        assert_eq!(
            RetirementState::begin(12_000, None, 0),
            RetirementState::Drained
        );
    }

    #[test]
    fn published_leases_and_connections_retire_with_target_and_hard_cap() {
        let state = RetirementState::begin(10_000, Some(310_000), 1);
        assert_eq!(
            state,
            RetirementState::Retiring {
                natural_deadline_ms: 310_000,
                hard_deadline_ms: 670_000,
            }
        );
        assert_eq!(
            state.advance(310_000, 1),
            RetirementState::Retiring {
                natural_deadline_ms: 310_000,
                hard_deadline_ms: 670_000,
            },
            "open streams can outlive published leases"
        );
        assert_eq!(state.advance(310_001, 0), RetirementState::Drained);
        assert_eq!(state.advance(670_000, 1), RetirementState::HardCapReached);
        assert_eq!(
            SERVICE_RETIREMENT_TARGET.as_secs(),
            600,
            "normal target is one tunnel lifetime"
        );
        assert_eq!(SERVICE_RETIREMENT_HARD_CAP.as_secs(), 660);
    }

    #[test]
    fn hard_shutdown_bypasses_graceful_deadline() {
        let state = RetirementState::begin(0, Some(600_000), 1);
        assert_eq!(state.hard_stop(), RetirementState::HardStopped);
        assert_eq!(state.hard_stop().status(0), LifecycleStatus::HardStopped);
    }
}
