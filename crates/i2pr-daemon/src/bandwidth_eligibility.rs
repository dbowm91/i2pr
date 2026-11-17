//! Plan 454's private gate from enforced shared limits to a RouterInfo tier.
//!
//! The runtime permit is bound to one healthy governor. This daemon-owned
//! wrapper derives the tier from the permit's configured bottleneck and
//! operator share, then revalidates the permit at the point where the
//! independently gated role/address builder consumes it. No normal startup
//! path calls these helpers, so this does not alter advertised capabilities.

use i2pr_netdb::BandwidthClass;
use i2pr_runtime::{BandwidthEnforcementPermit, BandwidthGovernor};

pub struct CapacityEligibility {
    permit: BandwidthEnforcementPermit,
    class: BandwidthClass,
}

impl CapacityEligibility {
    /// Derives an opaque eligibility value only from an enabled, healthy
    /// governor and a valid nonzero share. `None` models disabled enforcement.
    pub(crate) fn issue(governor: Option<&BandwidthGovernor>, share_percent: u8) -> Option<Self> {
        let governor = governor?;
        let permit = governor.capacity_permit(share_percent)?;
        let (inbound, outbound, share) = permit.configured_limits();
        let class = BandwidthClass::from_configured_rates(inbound, outbound, share).ok()??;
        Some(Self { permit, class })
    }

    /// Returns the tier only while the exact issuing shaper remains healthy.
    pub(crate) fn current_class(
        &self,
        governor: Option<&BandwidthGovernor>,
    ) -> Option<BandwidthClass> {
        let governor = governor?;
        self.permit.is_current_for(governor).then_some(self.class)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_runtime::BandwidthGovernorConfig;

    fn make_governor(inbound: u64, outbound: u64) -> BandwidthGovernor {
        BandwidthGovernor::new(BandwidthGovernorConfig {
            inbound_bytes_per_second: inbound,
            outbound_bytes_per_second: outbound,
            burst_bytes: 16 * 1024,
            control_reserve_percent: 10,
            max_pending_requests: 16,
            max_pending_per_peer: 2,
        })
        .expect("valid enabled shaper")
    }

    #[test]
    fn disabled_shaper_cannot_issue_capacity_eligibility() {
        assert!(CapacityEligibility::issue(None, 75).is_none());
    }

    #[test]
    fn capacity_tier_underclaims_the_enforced_bottleneck() {
        // 256 KiB/s × 50% is exactly 128 KiB/s, so Java's tier is N.
        // It must not round up to O (which starts at 129 KiB/s).
        let governor = make_governor(512 * 1024, 256 * 1024);
        let eligibility =
            CapacityEligibility::issue(Some(&governor), 50).expect("healthy bounded share");
        assert_eq!(
            eligibility.current_class(Some(&governor)),
            Some(BandwidthClass::N)
        );
    }

    #[test]
    fn a_permit_from_an_old_or_different_shaper_is_stale() {
        let old = make_governor(512 * 1024, 256 * 1024);
        let eligibility = CapacityEligibility::issue(Some(&old), 50).expect("eligible");
        let replacement = make_governor(1024 * 1024, 1024 * 1024);
        assert_eq!(eligibility.current_class(Some(&replacement)), None);
    }

    #[test]
    fn stopped_shaper_withdraws_existing_capacity_eligibility() {
        let governor = make_governor(512 * 1024, 256 * 1024);
        let eligibility = CapacityEligibility::issue(Some(&governor), 50).expect("eligible");
        drop(governor);
        assert_eq!(eligibility.current_class(None), None);
        assert!(!eligibility.permit.is_healthy());
    }

    #[test]
    fn class_requires_valid_share_and_active_nonzero_rates() {
        let governor = make_governor(1024, 1024);
        assert!(CapacityEligibility::issue(Some(&governor), 0).is_none());
        assert!(CapacityEligibility::issue(Some(&governor), 101).is_none());
        assert_eq!(
            BandwidthClass::from_configured_rates(1024, 1024, 101),
            Err(i2pr_netdb::BandwidthClassError::InvalidShare)
        );
    }
}
