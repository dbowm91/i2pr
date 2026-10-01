//! Shared, runtime-neutral floodfill admission budgets with drop-released leases.

use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FloodfillResourcePolicy {
    pub active_requests: usize,
    pub queued_effects: usize,
    pub queued_bytes: usize,
    pub crypto_validations: usize,
    pub direct_flood_attempts: usize,
    pub maintenance_records: usize,
}

impl Default for FloodfillResourcePolicy {
    fn default() -> Self {
        Self {
            active_requests: 128,
            queued_effects: 256,
            queued_bytes: 4 * 1024 * 1024,
            crypto_validations: 1024,
            direct_flood_attempts: 128,
            maintenance_records: 256,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    ActiveRequest,
    QueuedEffect,
    QueuedBytes(usize),
    CryptoValidation,
    DirectFloodAttempt,
    MaintenanceRecord,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FloodfillResourceSnapshot {
    pub active_requests: usize,
    pub queued_effects: usize,
    pub queued_bytes: usize,
    pub crypto_validations: usize,
    pub direct_flood_attempts: usize,
    pub maintenance_records: usize,
}

#[derive(Clone)]
pub struct FloodfillResourceBudget {
    policy: FloodfillResourcePolicy,
    state: Arc<Mutex<FloodfillResourceSnapshot>>,
}

impl FloodfillResourceBudget {
    pub fn new(policy: FloodfillResourcePolicy) -> Self {
        Self {
            policy,
            state: Arc::new(Mutex::new(FloodfillResourceSnapshot::default())),
        }
    }
    pub fn snapshot(&self) -> FloodfillResourceSnapshot {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn reserve(&self, kind: ResourceKind) -> Option<ResourceLease> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let (slot, ceiling, amount) = match kind {
            ResourceKind::ActiveRequest => {
                (&mut state.active_requests, self.policy.active_requests, 1)
            }
            ResourceKind::QueuedEffect => {
                (&mut state.queued_effects, self.policy.queued_effects, 1)
            }
            ResourceKind::QueuedBytes(bytes) => {
                (&mut state.queued_bytes, self.policy.queued_bytes, bytes)
            }
            ResourceKind::CryptoValidation => (
                &mut state.crypto_validations,
                self.policy.crypto_validations,
                1,
            ),
            ResourceKind::DirectFloodAttempt => (
                &mut state.direct_flood_attempts,
                self.policy.direct_flood_attempts,
                1,
            ),
            ResourceKind::MaintenanceRecord => (
                &mut state.maintenance_records,
                self.policy.maintenance_records,
                1,
            ),
        };
        let next = slot.checked_add(amount)?;
        if next > ceiling {
            return None;
        }
        *slot = next;
        Some(ResourceLease {
            state: Arc::clone(&self.state),
            kind,
            amount,
        })
    }
}

pub struct ResourceLease {
    state: Arc<Mutex<FloodfillResourceSnapshot>>,
    kind: ResourceKind,
    amount: usize,
}

impl Drop for ResourceLease {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let slot = match self.kind {
            ResourceKind::ActiveRequest => &mut state.active_requests,
            ResourceKind::QueuedEffect => &mut state.queued_effects,
            ResourceKind::QueuedBytes(_) => &mut state.queued_bytes,
            ResourceKind::CryptoValidation => &mut state.crypto_validations,
            ResourceKind::DirectFloodAttempt => &mut state.direct_flood_attempts,
            ResourceKind::MaintenanceRecord => &mut state.maintenance_records,
        };
        *slot = slot.saturating_sub(self.amount);
    }
}

impl std::fmt::Debug for ResourceLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ResourceLease(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resource_leases_enforce_ceilings_and_release_on_drop() {
        let budget = FloodfillResourceBudget::new(FloodfillResourcePolicy {
            active_requests: 1,
            queued_effects: 1,
            queued_bytes: 10,
            crypto_validations: 1,
            direct_flood_attempts: 1,
            maintenance_records: 1,
        });
        let lease = budget
            .reserve(ResourceKind::QueuedBytes(10))
            .expect("exact capacity");
        assert!(budget.reserve(ResourceKind::QueuedBytes(1)).is_none());
        assert_eq!(budget.snapshot().queued_bytes, 10);
        drop(lease);
        assert_eq!(budget.snapshot().queued_bytes, 0);
        assert!(budget.reserve(ResourceKind::ActiveRequest).is_some());
    }
}
