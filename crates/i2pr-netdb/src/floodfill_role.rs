//! Daemon-owned floodfill eligibility and advertisement authority vocabulary.

use i2pr_proto::RouterAddress;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloodfillRoleState {
    Disabled,
    Eligible,
    Activating,
    Active,
    Draining,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FloodfillEligibilitySnapshot {
    pub controlled_qualification_permit: bool,
    pub qualified_ssu2_address: bool,
    pub direct_reachability: bool,
    pub netdb_ready: bool,
    pub storage_ready: bool,
    pub maintenance_ready: bool,
    pub resource_headroom: bool,
    pub clock_sane: bool,
    pub supervision_healthy: bool,
}

impl FloodfillEligibilitySnapshot {
    pub const fn eligible(self) -> bool {
        self.controlled_qualification_permit
            && self.qualified_ssu2_address
            && self.direct_reachability
            && self.netdb_ready
            && self.storage_ready
            && self.maintenance_ready
            && self.resource_headroom
            && self.clock_sane
            && self.supervision_healthy
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloodfillRoleEffect {
    None,
    ReadyToActivate,
    WithdrawAdvertisementAndStopAdmission,
    FailedClosed,
}

#[derive(Debug)]
pub struct FloodfillRoleController {
    state: FloodfillRoleState,
    snapshot: Option<FloodfillEligibilitySnapshot>,
}

impl Default for FloodfillRoleController {
    fn default() -> Self {
        Self::new()
    }
}

impl FloodfillRoleController {
    pub const fn new() -> Self {
        Self {
            state: FloodfillRoleState::Disabled,
            snapshot: None,
        }
    }
    pub const fn state(&self) -> FloodfillRoleState {
        self.state
    }
    pub const fn accepts_new_work(&self) -> bool {
        matches!(self.state, FloodfillRoleState::Active)
    }

    pub fn update(&mut self, snapshot: FloodfillEligibilitySnapshot) -> FloodfillRoleEffect {
        self.snapshot = Some(snapshot);
        if !snapshot.eligible() {
            return match self.state {
                FloodfillRoleState::Active
                | FloodfillRoleState::Activating
                | FloodfillRoleState::Eligible => {
                    self.state = FloodfillRoleState::Draining;
                    FloodfillRoleEffect::WithdrawAdvertisementAndStopAdmission
                }
                _ => {
                    self.state = FloodfillRoleState::Disabled;
                    FloodfillRoleEffect::None
                }
            };
        }
        match self.state {
            FloodfillRoleState::Disabled | FloodfillRoleState::Failed => {
                self.state = FloodfillRoleState::Eligible;
                FloodfillRoleEffect::ReadyToActivate
            }
            _ => FloodfillRoleEffect::None,
        }
    }

    pub fn begin_activation(&mut self) -> bool {
        if self.state != FloodfillRoleState::Eligible
            || !self
                .snapshot
                .is_some_and(FloodfillEligibilitySnapshot::eligible)
        {
            return false;
        }
        self.state = FloodfillRoleState::Activating;
        true
    }

    pub fn complete_activation(&mut self) -> Option<FloodfillAdvertisementPermit> {
        if self.state != FloodfillRoleState::Activating
            || !self
                .snapshot
                .is_some_and(FloodfillEligibilitySnapshot::eligible)
        {
            return None;
        }
        self.state = FloodfillRoleState::Active;
        Some(FloodfillAdvertisementPermit { _sealed: () })
    }

    pub fn complete_drain(&mut self) -> bool {
        if self.state != FloodfillRoleState::Draining {
            return false;
        }
        self.state = FloodfillRoleState::Disabled;
        true
    }

    pub fn fail(&mut self) -> FloodfillRoleEffect {
        let had_advertisement = self.state == FloodfillRoleState::Active;
        self.state = FloodfillRoleState::Failed;
        if had_advertisement {
            FloodfillRoleEffect::WithdrawAdvertisementAndStopAdmission
        } else {
            FloodfillRoleEffect::FailedClosed
        }
    }

    pub fn advertisement_permit(&self) -> Option<FloodfillAdvertisementPermit> {
        (self.state == FloodfillRoleState::Active
            && self
                .snapshot
                .is_some_and(FloodfillEligibilitySnapshot::eligible))
        .then_some(FloodfillAdvertisementPermit { _sealed: () })
    }
}

/// Opaque proof that the role controller reached Active from a complete eligibility snapshot.
#[derive(Clone)]
pub struct FloodfillAdvertisementPermit {
    _sealed: (),
}

impl std::fmt::Debug for FloodfillAdvertisementPermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FloodfillAdvertisementPermit(..)")
    }
}

/// Validates the final RouterAddress style at the typed advertisement boundary.
pub fn is_qualified_ssu2_address(address: &RouterAddress) -> bool {
    address.transport_style() == "SSU2"
}

#[cfg(test)]
mod tests {
    use super::*;
    fn eligible() -> FloodfillEligibilitySnapshot {
        FloodfillEligibilitySnapshot {
            controlled_qualification_permit: true,
            qualified_ssu2_address: true,
            direct_reachability: true,
            netdb_ready: true,
            storage_ready: true,
            maintenance_ready: true,
            resource_headroom: true,
            clock_sane: true,
            supervision_healthy: true,
        }
    }
    #[test]
    fn config_without_controlled_eligibility_never_activates_and_health_loss_withdraws_first() {
        let mut controller = FloodfillRoleController::new();
        let mut snapshot = eligible();
        snapshot.controlled_qualification_permit = false;
        assert_eq!(controller.update(snapshot), FloodfillRoleEffect::None);
        assert!(controller.advertisement_permit().is_none());
        snapshot = eligible();
        assert_eq!(
            controller.update(snapshot),
            FloodfillRoleEffect::ReadyToActivate
        );
        assert!(controller.begin_activation());
        assert!(controller.complete_activation().is_some());
        assert!(controller.accepts_new_work());
        snapshot.resource_headroom = false;
        assert_eq!(
            controller.update(snapshot),
            FloodfillRoleEffect::WithdrawAdvertisementAndStopAdmission
        );
        assert!(!controller.accepts_new_work());
        assert!(controller.complete_drain());
        assert_eq!(controller.state(), FloodfillRoleState::Disabled);
    }
}
