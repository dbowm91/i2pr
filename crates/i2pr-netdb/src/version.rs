//! Plan 491 — the router's own peer-eligibility policy.
//!
//! The comparator and the specification's obligation table live in
//! `i2pr-proto` (`i2pr_proto::common::version`). This module binds them to
//! *this* router: it reads the level the controlled profile declares, derives
//! the two minimum-version floors from the specification's table, and exposes
//! the typed eligibility decisions the daemon enforces.
//!
//! The floors are **not** operator configuration. There is no configuration
//! key that lowers them, and the derived constants cannot be edited without
//! editing the declared level or the specification's table — both of which
//! are pinned by tests.

use std::collections::BTreeMap;

use i2pr_proto::{
    ApiVersion, PublishedVersion, SPECIFICATION_API_LEVEL, VersionEligibility, VersionFloors,
    floors_in_force,
};

use crate::local::CONTROLLED_ROUTER_VERSION;
use crate::router_info::ValidatedRouterInfo;

/// The property a router publishes its API level in.
pub const ROUTER_VERSION_PROPERTY: &str = "router.version";

/// The API level the controlled profile declares, parsed.
///
/// This is the same value as [`CONTROLLED_ROUTER_VERSION`]; it is parsed once
/// here so the floor derivation never has to re-parse a string.
pub const CONTROLLED_API_VERSION: ApiVersion = match ApiVersion::parse(CONTROLLED_ROUTER_VERSION) {
    Some(version) => version,
    None => panic!("the controlled router version must be a comparable API version"),
};

/// The network release the pinned specification snapshot describes.
///
/// The dossier in `specs/protocols/02-i2np.md` is accurate for I2NP API
/// `0.9.69`. An obligation row whose "as of" clause names a later release is
/// not yet in force, and one that names an earlier release is.
pub const SPECIFICATION_RELEASE: ApiVersion = match ApiVersion::parse(SPECIFICATION_API_LEVEL) {
    Some(version) => version,
    None => panic!("the specification API level must be a comparable API version"),
};

/// The floors in force for the level this router declares.
///
/// Derived from the specification's table; see
/// [`i2pr_proto::floors_in_force`].
pub const CONTROLLED_VERSION_FLOORS: VersionFloors =
    floors_in_force(CONTROLLED_API_VERSION, SPECIFICATION_RELEASE);

/// The minimum published API version a peer must declare to be used as a
/// tunnel-build peer.
///
/// The specification's API-version table attaches "minimum peers will build
/// tunnels through" to the `0.9.62` row. The floor is derived from that row
/// and from the level this router declares; it is not a hand-maintained copy.
pub const MIN_PEER_VERSION_FOR_TUNNEL_BUILD: ApiVersion =
    match CONTROLLED_VERSION_FLOORS.min_peer_for_tunnel_build {
        Some(floor) => floor,
        None => panic!("the specification's table imposes a tunnel-build floor"),
    };

/// The minimum published API version a floodfill must declare to be sent a
/// DSM.
///
/// The specification's API-version table attaches "minimum floodfill peers
/// will send DSM to" to the `0.9.62` row. The floor is derived from that row
/// and from the level this router declares; it is not a hand-maintained copy.
pub const MIN_FLOODFILL_VERSION_FOR_DSM: ApiVersion =
    match CONTROLLED_VERSION_FLOORS.min_floodfill_for_dsm {
        Some(floor) => floor,
        None => panic!("the specification's table imposes a DSM floodfill floor"),
    };

/// The published API version of a validated peer record.
///
/// A record that publishes no version yields [`PublishedVersion::Unknown`]; a
/// record that publishes a malformed value yields
/// [`PublishedVersion::Uncomparable`]. Neither is ever treated as "newest".
pub fn published_version_of(record: &ValidatedRouterInfo) -> PublishedVersion {
    PublishedVersion::parse_option(record.router_info().options().get(ROUTER_VERSION_PROPERTY))
}

/// The eligibility decision for using a peer as a tunnel-build peer.
pub fn tunnel_peer_eligibility(published: PublishedVersion) -> VersionEligibility {
    VersionEligibility::evaluate(
        CONTROLLED_VERSION_FLOORS.min_peer_for_tunnel_build,
        published,
    )
}

/// The eligibility decision for sending a DSM to a floodfill.
pub fn dsm_target_eligibility(published: PublishedVersion) -> VersionEligibility {
    VersionEligibility::evaluate(CONTROLLED_VERSION_FLOORS.min_floodfill_for_dsm, published)
}

/// The counted reasons peers were refused for a gated purpose.
///
/// The counters are the receipt's whole content: a refusal is never a silent
/// skip. No peer hash, address, key, or payload is retained here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VersionRefusalCounts {
    /// Peers that published no version at all.
    pub unknown_version: u32,
    /// Peers that published a version that cannot be compared.
    pub uncomparable_version: u32,
    /// Peers whose version is below the floor.
    pub below_floor: u32,
}

impl VersionRefusalCounts {
    /// Records one decision.
    pub fn record(&mut self, eligibility: VersionEligibility) {
        match eligibility {
            VersionEligibility::Eligible => {}
            VersionEligibility::UnknownVersion => {
                self.unknown_version = self.unknown_version.saturating_add(1);
            }
            VersionEligibility::UncomparableVersion => {
                self.uncomparable_version = self.uncomparable_version.saturating_add(1);
            }
            VersionEligibility::BelowFloor { .. } => {
                self.below_floor = self.below_floor.saturating_add(1);
            }
        }
    }

    /// The total number of refusals.
    pub const fn total(&self) -> u32 {
        self.unknown_version
            .saturating_add(self.uncomparable_version)
            .saturating_add(self.below_floor)
    }

    /// The counts keyed by their stable reason labels.
    pub fn counts_by_reason(&self) -> BTreeMap<&'static str, u32> {
        let mut counts = BTreeMap::new();
        counts.insert("unknown-version", self.unknown_version);
        counts.insert("uncomparable-version", self.uncomparable_version);
        counts.insert("below-floor", self.below_floor);
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_declared_level_parses_to_the_controlled_version() {
        assert_eq!(CONTROLLED_ROUTER_VERSION, "0.9.62");
        assert_eq!(CONTROLLED_API_VERSION, ApiVersion::new(0, 9, 62));
    }

    #[test]
    fn the_floors_are_derived_from_the_specification_table() {
        // The specification's 0.9.62 row carries both obligations, and the
        // snapshot describes the 0.9.69 release, so both are in force.
        assert_eq!(MIN_PEER_VERSION_FOR_TUNNEL_BUILD, ApiVersion::new(0, 9, 62));
        assert_eq!(MIN_FLOODFILL_VERSION_FOR_DSM, ApiVersion::new(0, 9, 62));
    }

    #[test]
    fn the_floors_are_not_a_hand_maintained_copy() {
        // Re-deriving from the table with the declared level reproduces the
        // constants exactly.
        let derived = floors_in_force(CONTROLLED_API_VERSION, SPECIFICATION_RELEASE);
        assert_eq!(
            derived.min_peer_for_tunnel_build,
            Some(MIN_PEER_VERSION_FOR_TUNNEL_BUILD)
        );
        assert_eq!(
            derived.min_floodfill_for_dsm,
            Some(MIN_FLOODFILL_VERSION_FOR_DSM)
        );
    }

    #[test]
    fn a_below_floor_peer_is_refused_for_both_purposes() {
        let below = PublishedVersion::Known(ApiVersion::new(0, 9, 61));
        assert_eq!(
            tunnel_peer_eligibility(below),
            VersionEligibility::BelowFloor {
                floor: MIN_PEER_VERSION_FOR_TUNNEL_BUILD
            }
        );
        assert_eq!(
            dsm_target_eligibility(below),
            VersionEligibility::BelowFloor {
                floor: MIN_FLOODFILL_VERSION_FOR_DSM
            }
        );
    }

    #[test]
    fn an_unknown_version_is_refused_for_both_purposes() {
        assert_eq!(
            tunnel_peer_eligibility(PublishedVersion::Unknown),
            VersionEligibility::UnknownVersion
        );
        assert_eq!(
            dsm_target_eligibility(PublishedVersion::Unknown),
            VersionEligibility::UnknownVersion
        );
    }

    #[test]
    fn an_uncomparable_version_is_refused_for_both_purposes() {
        assert_eq!(
            tunnel_peer_eligibility(PublishedVersion::Uncomparable),
            VersionEligibility::UncomparableVersion
        );
        assert_eq!(
            dsm_target_eligibility(PublishedVersion::Uncomparable),
            VersionEligibility::UncomparableVersion
        );
    }

    #[test]
    fn a_peer_at_the_floor_is_eligible_for_both_purposes() {
        let at = PublishedVersion::Known(ApiVersion::new(0, 9, 62));
        assert!(tunnel_peer_eligibility(at).is_eligible());
        assert!(dsm_target_eligibility(at).is_eligible());
    }

    #[test]
    fn refusal_counts_are_counted_by_reason() {
        let mut counts = VersionRefusalCounts::default();
        counts.record(VersionEligibility::UnknownVersion);
        counts.record(VersionEligibility::UncomparableVersion);
        counts.record(VersionEligibility::UncomparableVersion);
        counts.record(VersionEligibility::BelowFloor {
            floor: MIN_PEER_VERSION_FOR_TUNNEL_BUILD,
        });
        counts.record(VersionEligibility::Eligible);
        assert_eq!(counts.unknown_version, 1);
        assert_eq!(counts.uncomparable_version, 2);
        assert_eq!(counts.below_floor, 1);
        assert_eq!(counts.total(), 4);
        let by_reason = counts.counts_by_reason();
        assert_eq!(by_reason.get("unknown-version"), Some(&1));
        assert_eq!(by_reason.get("uncomparable-version"), Some(&2));
        assert_eq!(by_reason.get("below-floor"), Some(&1));
    }
}
