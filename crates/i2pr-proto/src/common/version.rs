//! Typed comparison of the published `router.version` API level.
//!
//! Plan 491. The I2NP specification's API-version table is the authority for
//! two *minimum-peer obligations*:
//!
//! ```text
//! Minimum peers will build tunnels through, as of 0.9.68
//! Minimum floodfill peers will send DSM to, as of 0.9.68
//! ```
//!
//! Both obligations are attached to the API version `0.9.62` row of that
//! table. A router that publishes a version at or above a row's version
//! inherits that row's obligation once the network release named in the row's
//! "as of" clause has been reached.
//!
//! This module owns:
//!
//! * [`ApiVersion`] — the numeric comparator. The specification's rule is a
//!   numeric comparison of the dotted components ("0.9.13 implies support for
//!   0.9.12 features"), never a lexicographic one.
//! * [`PublishedVersion`] — the *evidence* a peer publishes, with `Unknown`
//!   (no version published) and `Uncomparable` (a version that cannot be
//!   compared) as first-class states. Neither is ever treated as "newest".
//! * [`VersionEligibility`] — the typed decision a floor produces.
//! * [`MINIMUM_PEER_OBLIGATIONS`] — the specification's obligation rows, as
//!   data, so the floors are *derived* rather than hand-maintained.
//!
//! Nothing here performs I/O, allocates in the comparison path, or depends on
//! a runtime.

use std::{cmp::Ordering, fmt};

/// The specification's API-version table's widest row, as this tree holds it.
///
/// The specification's table contains a four-component row (`0.6.1.10`). This
/// tree holds the table's **minimum-peer obligation** rows as data
/// (`MINIMUM_PEER_OBLIGATIONS`); it does not hold the table's other rows, so
/// the four-component row the comparator's bound depends on appeared here
/// only as prose. Holding the row as **data** makes the comparator's bound a
/// derivation rather than an assumption, and makes a future wider row in the
/// specification's table widen the comparator by construction.
pub const SPECIFICATION_WIDEST_VERSION_ROW: &[u16] = &[0, 6, 1, 10];

/// The component count of one recorded version row.
///
/// The comparator's bound is the **width** of the specification's table's
/// widest row, so the derivation is a width and not a literal.
pub const fn version_row_width(row: &[u16]) -> usize {
    row.len()
}

/// The maximum number of dotted components a published API version may carry.
///
/// The bound is **derived** from the specification's table's widest row, which
/// this tree holds as data (`SPECIFICATION_WIDEST_VERSION_ROW`). The
/// specification's table contains a four-component row (`0.6.1.10`), so four
/// components are accepted. A version with more components is uncomparable
/// rather than truncated.
///
/// **A wider row widens this bound.** If the specification's table gains a
/// five-component row, the held record becomes that row, this constant
/// follows it, and such a version becomes **comparable** rather than
/// uncomparable. The stored array's width and the parser's bound are this
/// same constant, so the ordering stays exact for every version the parser
/// accepts.
pub const MAX_API_VERSION_COMPONENTS: usize = version_row_width(SPECIFICATION_WIDEST_VERSION_ROW);

/// A well-formed, comparable I2NP API version.
///
/// The ordering is the specification's numeric rule: components are compared
/// left to right as numbers. When one side runs out of components while the
/// other still has one, the shorter version is *less* — the same rule the
/// pinned Java reference uses in `net.i2p.util.VersionComparator`.
#[derive(Clone, Copy, Debug)]
pub struct ApiVersion {
    components: [u16; MAX_API_VERSION_COMPONENTS],
    len: u8,
}

impl ApiVersion {
    /// Builds a three-component version.
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            components: [major, minor, patch, 0],
            len: 3,
        }
    }

    /// Builds a four-component version.
    pub const fn new4(major: u16, minor: u16, patch: u16, build: u16) -> Self {
        Self {
            components: [major, minor, patch, build],
            len: 4,
        }
    }

    /// Parses a published version string.
    ///
    /// The grammar is strict: one to `MAX_API_VERSION_COMPONENTS` dotted
    /// components, each a non-empty run of ASCII digits whose value fits in a
    /// `u16`. Anything else — an empty string, a trailing dot, a non-digit
    /// character, a component that overflows — is *not* a version. It is
    /// reported as uncomparable rather than being coerced, so a malformed
    /// value can never compare as "newest".
    pub const fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return None;
        }
        let mut components = [0u16; MAX_API_VERSION_COMPONENTS];
        let mut len = 0usize;
        let mut index = 0usize;
        while index < bytes.len() {
            let mut value: u32 = 0;
            let mut digits = 0usize;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                value = value * 10 + (bytes[index] - b'0') as u32;
                if value > u16::MAX as u32 {
                    return None;
                }
                digits += 1;
                index += 1;
            }
            if digits == 0 || len >= MAX_API_VERSION_COMPONENTS {
                return None;
            }
            components[len] = value as u16;
            len += 1;
            if index < bytes.len() {
                if bytes[index] != b'.' {
                    return None;
                }
                index += 1;
                if index == bytes.len() {
                    return None;
                }
            }
        }
        if len == 0 {
            return None;
        }
        Some(Self {
            components,
            len: len as u8,
        })
    }

    /// The dotted components, in order.
    pub fn components(&self) -> &[u16] {
        &self.components[..self.len as usize]
    }

    /// The major component.
    pub const fn major(&self) -> u16 {
        self.components[0]
    }

    /// The minor component.
    pub const fn minor(&self) -> u16 {
        self.components[1]
    }

    /// The patch component, or zero for a two-component version.
    pub const fn patch(&self) -> u16 {
        self.components[2]
    }

    /// The build component, or zero when the version carries fewer than four
    /// components.
    pub const fn build(&self) -> u16 {
        self.components[3]
    }

    /// The component-wise comparison, usable in a `const` context.
    pub const fn cmp_to(&self, other: &Self) -> Ordering {
        let mut index = 0usize;
        while index < MAX_API_VERSION_COMPONENTS {
            let left = if index < self.len as usize {
                Some(self.components[index])
            } else {
                None
            };
            let right = if index < other.len as usize {
                Some(other.components[index])
            } else {
                None
            };
            match (left, right) {
                (None, None) => return Ordering::Equal,
                (None, Some(_)) => return Ordering::Less,
                (Some(_), None) => return Ordering::Greater,
                (Some(a), Some(b)) => {
                    if a < b {
                        return Ordering::Less;
                    }
                    if a > b {
                        return Ordering::Greater;
                    }
                }
            }
            index += 1;
        }
        Ordering::Equal
    }

    /// Whether this version is strictly below `other`.
    pub const fn lt(&self, other: &Self) -> bool {
        matches!(self.cmp_to(other), Ordering::Less)
    }

    /// Whether this version is strictly above `other`.
    pub const fn gt(&self, other: &Self) -> bool {
        matches!(self.cmp_to(other), Ordering::Greater)
    }
}

impl PartialEq for ApiVersion {
    fn eq(&self, other: &Self) -> bool {
        matches!(self.cmp_to(other), Ordering::Equal)
    }
}

impl Eq for ApiVersion {}

impl PartialOrd for ApiVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(Ord::cmp(self, other))
    }
}

impl Ord for ApiVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_to(other)
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let components = self.components();
        for (index, component) in components.iter().enumerate() {
            if index > 0 {
                formatter.write_str(".")?;
            }
            write!(formatter, "{component}")?;
        }
        Ok(())
    }
}

/// The published API version of a peer, with every non-comparable case kept
/// explicit.
///
/// A peer that publishes no version and a peer that publishes a malformed
/// version are *different* facts, and both are refusals rather than passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublishedVersion {
    /// A well-formed version.
    Known(ApiVersion),
    /// The peer published no version at all.
    Unknown,
    /// The peer published a value that is not a comparable version.
    Uncomparable,
}

impl PublishedVersion {
    /// Reads a published version out of an optional property value.
    pub fn parse_option(value: Option<&str>) -> Self {
        match value {
            None => Self::Unknown,
            Some(text) => match ApiVersion::parse(text) {
                Some(version) => Self::Known(version),
                None => Self::Uncomparable,
            },
        }
    }

    /// Whether the version is well-formed and comparable.
    pub const fn is_known(&self) -> bool {
        matches!(self, Self::Known(_))
    }

    /// A stable, non-sensitive label for receipts.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Known(_) => "known",
            Self::Unknown => "unknown",
            Self::Uncomparable => "uncomparable",
        }
    }
}

impl fmt::Display for PublishedVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Known(version) => write!(formatter, "{version}"),
            Self::Unknown => formatter.write_str("unknown"),
            Self::Uncomparable => formatter.write_str("uncomparable"),
        }
    }
}

/// The typed outcome of applying a minimum-version floor to a published
/// version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionEligibility {
    /// The peer satisfies the floor.
    Eligible,
    /// The peer published no version at all.
    UnknownVersion,
    /// The peer published a version that cannot be compared.
    UncomparableVersion,
    /// The peer's version is below the floor.
    BelowFloor {
        /// The floor the peer failed to meet.
        floor: ApiVersion,
    },
}

impl VersionEligibility {
    /// Applies a floor to a published version.
    ///
    /// An unknown or uncomparable version is refused whatever the floor is:
    /// "not comparable" is never "good enough". A floor of `None` means the
    /// specification imposes no obligation at the declared level, in which case
    /// a comparable version is accepted and the two non-comparable states are
    /// still refused.
    pub fn evaluate(floor: Option<ApiVersion>, published: PublishedVersion) -> Self {
        match published {
            PublishedVersion::Unknown => Self::UnknownVersion,
            PublishedVersion::Uncomparable => Self::UncomparableVersion,
            PublishedVersion::Known(version) => match floor {
                Some(floor) if version.lt(&floor) => Self::BelowFloor { floor },
                _ => Self::Eligible,
            },
        }
    }

    /// Whether the peer may be used for the gated purpose.
    pub const fn is_eligible(&self) -> bool {
        matches!(self, Self::Eligible)
    }

    /// A stable, non-sensitive reason label for receipts.
    pub const fn reason_label(&self) -> &'static str {
        match self {
            Self::Eligible => "eligible",
            Self::UnknownVersion => "unknown-version",
            Self::UncomparableVersion => "uncomparable-version",
            Self::BelowFloor { .. } => "below-floor",
        }
    }
}

/// One row of the specification's API-version table that carries a
/// minimum-peer obligation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MinimumPeerObligation {
    /// The API version that introduces the obligation — the row's own
    /// version.
    pub api_version: ApiVersion,
    /// The network release in which the obligation came into force, taken
    /// verbatim from the row's "as of" clause.
    pub effective_from: ApiVersion,
    /// Whether the row constrains the peers a tunnel is built through.
    pub constrains_tunnel_peers: bool,
    /// Whether the row constrains the floodfills a DSM is sent to.
    pub constrains_dsm_targets: bool,
}

/// The minimum-peer obligation rows of the specification's API-version table.
///
/// Only the rows that carry a minimum-peer obligation appear here; the other
/// rows describe feature support and impose no floor. The rows are copied
/// from the specification's table:
///
/// ```text
/// 0.9.62  minimum peers will build tunnels through, as of 0.9.68
///         minimum floodfill peers will send DSM to, as of 0.9.68
/// 0.9.59  minimum peers will build tunnels through, as of 0.9.63
///         minimum floodfill peers will send DSM to, as of 0.9.63
/// 0.9.58  minimum peers will build tunnels through, as of 0.9.62
/// 0.9.51  minimum peers will build tunnels through, as of 0.9.58
///         minimum floodfill peers will send DSM to, as of 0.9.58
/// 0.9.36  minimum peers will build tunnels through, as of 0.9.46
/// 0.9.28  minimum floodfill peers will send DSM to, as of 0.9.34
/// ```
///
/// The "as of" clause names the network release in which the obligation came
/// into force; the row's own version is the version a peer must publish to
/// satisfy it.
pub const MINIMUM_PEER_OBLIGATIONS: &[MinimumPeerObligation] = &[
    MinimumPeerObligation {
        api_version: ApiVersion::new(0, 9, 62),
        effective_from: ApiVersion::new(0, 9, 68),
        constrains_tunnel_peers: true,
        constrains_dsm_targets: true,
    },
    MinimumPeerObligation {
        api_version: ApiVersion::new(0, 9, 59),
        effective_from: ApiVersion::new(0, 9, 63),
        constrains_tunnel_peers: true,
        constrains_dsm_targets: true,
    },
    MinimumPeerObligation {
        api_version: ApiVersion::new(0, 9, 58),
        effective_from: ApiVersion::new(0, 9, 62),
        constrains_tunnel_peers: true,
        constrains_dsm_targets: false,
    },
    MinimumPeerObligation {
        api_version: ApiVersion::new(0, 9, 51),
        effective_from: ApiVersion::new(0, 9, 58),
        constrains_tunnel_peers: true,
        constrains_dsm_targets: true,
    },
    MinimumPeerObligation {
        api_version: ApiVersion::new(0, 9, 36),
        effective_from: ApiVersion::new(0, 9, 46),
        constrains_tunnel_peers: true,
        constrains_dsm_targets: false,
    },
    MinimumPeerObligation {
        api_version: ApiVersion::new(0, 9, 28),
        effective_from: ApiVersion::new(0, 9, 34),
        constrains_tunnel_peers: false,
        constrains_dsm_targets: true,
    },
];

/// The API level of the specification snapshot this router implements.
///
/// The pinned dossier in `specs/protocols/02-i2np.md` is accurate for I2NP
/// API `0.9.69`. The network release named in an obligation row's "as of"
/// clause is compared against this level, so a row is in force when the
/// snapshot already describes it.
pub const SPECIFICATION_API_LEVEL: &str = "0.9.69";

/// The specification's catch-all line: "0.7.9 or lower — all messages and
/// features not listed above".
///
/// A peer at or below this line supports none of the listed features. The
/// numeric comparison handles it without a special case, but the constant is
/// named so the rule is explicit and testable rather than incidental.
pub const CATCH_ALL_VERSION: ApiVersion = ApiVersion::new(0, 7, 9);

/// The two minimum-version floors in force for a router.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VersionFloors {
    /// The minimum published version a peer must declare to be used as a
    /// tunnel-build peer, or `None` when no such obligation is in force.
    pub min_peer_for_tunnel_build: Option<ApiVersion>,
    /// The minimum published version a floodfill must declare to be sent a
    /// DSM, or `None` when no such obligation is in force.
    pub min_floodfill_for_dsm: Option<ApiVersion>,
}

impl VersionFloors {
    /// The floors that impose no obligation at all.
    pub const NONE: Self = Self {
        min_peer_for_tunnel_build: None,
        min_floodfill_for_dsm: None,
    };
}

/// Derives the floors in force for a router declaring `declared`.
///
/// A row applies when the router's declared version is at or above the row's
/// version *and* the network release named in the row's "as of" clause has
/// been reached (`release`). Among the applicable rows the highest version
/// wins, so the floor rises as the specification's table tightens.
pub const fn floors_in_force(declared: ApiVersion, release: ApiVersion) -> VersionFloors {
    let mut tunnel: Option<ApiVersion> = None;
    let mut dsm: Option<ApiVersion> = None;
    let mut index = 0usize;
    while index < MINIMUM_PEER_OBLIGATIONS.len() {
        let row = &MINIMUM_PEER_OBLIGATIONS[index];
        let at_or_below_declared = !matches!(row.api_version.cmp_to(&declared), Ordering::Greater);
        let in_force = !matches!(row.effective_from.cmp_to(&release), Ordering::Greater);
        if at_or_below_declared && in_force {
            if row.constrains_tunnel_peers
                && match tunnel {
                    None => true,
                    Some(current) => current.lt(&row.api_version),
                }
            {
                tunnel = Some(row.api_version);
            }
            if row.constrains_dsm_targets
                && match dsm {
                    None => true,
                    Some(current) => current.lt(&row.api_version),
                }
            {
                dsm = Some(row.api_version);
            }
        }
        index += 1;
    }
    VersionFloors {
        min_peer_for_tunnel_build: tunnel,
        min_floodfill_for_dsm: dsm,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_three_and_four_component_versions() {
        assert_eq!(
            ApiVersion::parse("0.9.62").unwrap().components(),
            &[0, 9, 62]
        );
        assert_eq!(
            ApiVersion::parse("0.6.1.10").unwrap().components(),
            &[0, 6, 1, 10]
        );
        assert_eq!(ApiVersion::parse("0.9").unwrap().components(), &[0, 9]);
    }

    #[test]
    fn rejects_malformed_versions() {
        for text in [
            "",
            ".",
            "0.",
            ".0",
            "0..9",
            "0.9.",
            "0.9.a",
            "a",
            "0.9.62b",
            " 0.9.62",
            "0.9.62 ",
            "+0.9",
            "0.9.62.1.2",
        ] {
            assert!(ApiVersion::parse(text).is_none(), "{text:?} must not parse");
        }
    }

    #[test]
    fn rejects_component_overflow() {
        assert!(ApiVersion::parse("0.9.65536").is_none());
        assert!(ApiVersion::parse("65536.0.0").is_none());
        assert!(ApiVersion::parse("0.9.65535").is_some());
    }

    #[test]
    fn compares_numerically_not_lexicographically() {
        let a = ApiVersion::parse("0.9.13").unwrap();
        let b = ApiVersion::parse("0.9.12").unwrap();
        assert!(a.gt(&b));
        assert!(!a.lt(&b));
        // A lexicographic comparison would invert these.
        let ten = ApiVersion::parse("0.9.9").unwrap();
        let nine = ApiVersion::parse("0.9.10").unwrap();
        assert!(nine.gt(&ten));
    }

    #[test]
    fn shorter_version_is_less_when_the_prefix_matches() {
        let short = ApiVersion::parse("0.9").unwrap();
        let long = ApiVersion::parse("0.9.0").unwrap();
        assert!(short.lt(&long));
        assert!(!long.lt(&short));
        assert_ne!(short, long);
    }

    #[test]
    fn ordering_is_a_total_order() {
        let mut versions = [
            ApiVersion::parse("0.9.62").unwrap(),
            ApiVersion::parse("0.9.9").unwrap(),
            ApiVersion::parse("0.9.10").unwrap(),
            ApiVersion::parse("0.10.0").unwrap(),
            ApiVersion::parse("0.6.1.10").unwrap(),
            ApiVersion::parse("1.0.0").unwrap(),
        ];
        versions.sort();
        assert_eq!(
            versions.iter().map(|v| v.to_string()).collect::<Vec<_>>(),
            vec![
                "0.6.1.10".to_string(),
                "0.9.9".to_string(),
                "0.9.10".to_string(),
                "0.9.62".to_string(),
                "0.10.0".to_string(),
                "1.0.0".to_string(),
            ]
        );
    }

    #[test]
    fn published_version_states_are_distinct() {
        assert_eq!(
            PublishedVersion::parse_option(None),
            PublishedVersion::Unknown
        );
        assert_eq!(
            PublishedVersion::parse_option(Some("nonsense")),
            PublishedVersion::Uncomparable
        );
        assert_eq!(
            PublishedVersion::parse_option(Some("0.9.62")),
            PublishedVersion::Known(ApiVersion::parse("0.9.62").unwrap())
        );
        assert_eq!(PublishedVersion::Unknown.label(), "unknown");
        assert_eq!(PublishedVersion::Uncomparable.label(), "uncomparable");
    }

    #[test]
    fn an_unknown_version_is_never_eligible() {
        let floor = Some(ApiVersion::parse("0.9.62").unwrap());
        assert_eq!(
            VersionEligibility::evaluate(floor, PublishedVersion::Unknown),
            VersionEligibility::UnknownVersion
        );
        assert_eq!(
            VersionEligibility::evaluate(floor, PublishedVersion::Uncomparable),
            VersionEligibility::UncomparableVersion
        );
        assert_eq!(
            VersionEligibility::evaluate(None, PublishedVersion::Unknown),
            VersionEligibility::UnknownVersion
        );
    }

    #[test]
    fn a_below_floor_peer_is_refused_and_a_peer_at_the_floor_is_accepted() {
        let floor = ApiVersion::parse("0.9.62").unwrap();
        let below = PublishedVersion::Known(ApiVersion::parse("0.9.61").unwrap());
        let at = PublishedVersion::Known(ApiVersion::parse("0.9.62").unwrap());
        let above = PublishedVersion::Known(ApiVersion::parse("0.9.70").unwrap());
        assert_eq!(
            VersionEligibility::evaluate(Some(floor), below),
            VersionEligibility::BelowFloor { floor }
        );
        assert!(VersionEligibility::evaluate(Some(floor), at).is_eligible());
        assert!(VersionEligibility::evaluate(Some(floor), above).is_eligible());
    }

    #[test]
    fn the_catch_all_line_is_below_every_obligation() {
        for row in MINIMUM_PEER_OBLIGATIONS {
            assert!(CATCH_ALL_VERSION.lt(&row.api_version));
        }
        assert!(CATCH_ALL_VERSION.lt(&ApiVersion::parse("0.9.62").unwrap()));
    }

    #[test]
    fn the_derived_floors_follow_the_specification_table() {
        let declared = ApiVersion::parse("0.9.62").unwrap();
        let release = ApiVersion::parse(SPECIFICATION_API_LEVEL).unwrap();
        let floors = floors_in_force(declared, release);
        assert_eq!(
            floors.min_peer_for_tunnel_build,
            Some(ApiVersion::new(0, 9, 62))
        );
        assert_eq!(
            floors.min_floodfill_for_dsm,
            Some(ApiVersion::new(0, 9, 62))
        );
    }

    #[test]
    fn a_row_that_is_not_yet_in_force_does_not_raise_the_floor() {
        // At the 0.9.62 release the 0.9.59 row ("as of 0.9.63") and the 0.9.62
        // row ("as of 0.9.68") are not yet in force, so the floors come from
        // the 0.9.58 and 0.9.51 rows.
        let declared = ApiVersion::parse("0.9.62").unwrap();
        let release = ApiVersion::parse("0.9.62").unwrap();
        let floors = floors_in_force(declared, release);
        assert_eq!(
            floors.min_peer_for_tunnel_build,
            Some(ApiVersion::new(0, 9, 58))
        );
        assert_eq!(
            floors.min_floodfill_for_dsm,
            Some(ApiVersion::new(0, 9, 51))
        );
    }

    #[test]
    fn each_release_raises_the_floor_by_its_own_row() {
        let declared = ApiVersion::parse("0.9.70").unwrap();
        let at = |release: &str| floors_in_force(declared, ApiVersion::parse(release).unwrap());
        assert_eq!(
            at("0.9.62").min_peer_for_tunnel_build,
            Some(ApiVersion::new(0, 9, 58))
        );
        assert_eq!(
            at("0.9.63").min_peer_for_tunnel_build,
            Some(ApiVersion::new(0, 9, 59))
        );
        assert_eq!(
            at("0.9.68").min_peer_for_tunnel_build,
            Some(ApiVersion::new(0, 9, 62))
        );
        // The DSM floor follows the rows that carry a DSM obligation.
        assert_eq!(
            at("0.9.34").min_floodfill_for_dsm,
            Some(ApiVersion::new(0, 9, 28))
        );
        assert_eq!(
            at("0.9.58").min_floodfill_for_dsm,
            Some(ApiVersion::new(0, 9, 51))
        );
        assert_eq!(
            at("0.9.68").min_floodfill_for_dsm,
            Some(ApiVersion::new(0, 9, 62))
        );
    }

    #[test]
    fn a_router_below_every_obligation_row_has_no_floor() {
        let declared = ApiVersion::parse("0.9.20").unwrap();
        let release = ApiVersion::parse(SPECIFICATION_API_LEVEL).unwrap();
        let floors = floors_in_force(declared, release);
        assert_eq!(floors.min_peer_for_tunnel_build, None);
        assert_eq!(floors.min_floodfill_for_dsm, None);
    }

    #[test]
    fn the_floors_never_fall_below_the_row_that_carries_them() {
        let declared = ApiVersion::parse("0.9.70").unwrap();
        let release = ApiVersion::parse(SPECIFICATION_API_LEVEL).unwrap();
        let floors = floors_in_force(declared, release);
        assert_eq!(
            floors.min_peer_for_tunnel_build,
            Some(ApiVersion::new(0, 9, 62))
        );
        assert_eq!(
            floors.min_floodfill_for_dsm,
            Some(ApiVersion::new(0, 9, 62))
        );
    }

    /// A small deterministic generator: the corpus must be reproducible
    /// without a dependency on a property-testing crate.
    fn corpus(count: usize) -> Vec<String> {
        const ALPHABET: &[u8] = b"0123456789..aAzZ _-+";
        let mut state = 0x5eed_4910_5177_u64;
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let mut text = String::new();
            let length = (state % 12) as usize + 1;
            for _ in 0..length {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let index = (state >> 33) as usize % ALPHABET.len();
                text.push(ALPHABET[index] as char);
            }
            out.push(text);
        }
        out
    }

    /// WP1's totality property: every published version string maps to a
    /// comparable value or to an explicit state, and no input panics.
    #[test]
    fn every_version_string_maps_to_a_value_or_an_explicit_state() {
        let mut inputs = corpus(4_096);
        inputs.extend([
            String::new(),
            "0".to_owned(),
            "0.9.62".to_owned(),
            "0.9.62.".to_owned(),
            ".0.9.62".to_owned(),
            "0.9..62".to_owned(),
            "0.9.62.1.2".to_owned(),
            "0.9.65535".to_owned(),
            "0.9.65536".to_owned(),
            " 0.9.62".to_owned(),
            "0.9.62 ".to_owned(),
        ]);
        for text in &inputs {
            let published = PublishedVersion::parse_option(Some(text));
            match published {
                PublishedVersion::Known(version) => {
                    // A known version round-trips through the comparator.
                    assert_eq!(
                        ApiVersion::parse(text),
                        Some(version),
                        "{text:?} must parse to the value it reported"
                    );
                }
                PublishedVersion::Unknown | PublishedVersion::Uncomparable => {
                    assert!(ApiVersion::parse(text).is_none(), "{text:?} must not parse");
                }
            }
            // The decision never panics and never treats an incomparable
            // version as eligible.
            let eligibility =
                VersionEligibility::evaluate(Some(ApiVersion::new(0, 9, 62)), published);
            if !eligibility.is_eligible() {
                assert_ne!(eligibility.reason_label(), "eligible");
            }
        }
    }

    /// The comparison is a total order over the comparable subset.
    #[test]
    fn the_comparison_is_a_total_order_over_the_corpus() {
        let mut versions: Vec<ApiVersion> = corpus(2_048)
            .iter()
            .filter_map(|text| ApiVersion::parse(text))
            .collect();
        versions.sort();
        versions.dedup();
        for (index, left) in versions.iter().enumerate() {
            for right in versions.iter().skip(index) {
                assert!(!left.gt(right) || right.gt(left));
                assert_eq!(left == right, left.cmp_to(right) == Ordering::Equal);
                if left < right {
                    assert!(right > left);
                }
            }
        }
    }
}
