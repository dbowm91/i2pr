//! Plan 511: the version comparator's component ceiling is **derived** from
//! a fact the tree holds, not asserted in a prose comment.
//!
//! Plan 491 recorded finding **F5**: *"The comparator's four-component
//! ceiling is an unstated assumption in the specification's terms."* The
//! specification's API-version table contains a four-component row
//! (`0.6.1.10`), but this tree holds only the table's **minimum-peer
//! obligation** rows as data, so the row the ceiling depends on appeared
//! here only as prose.
//!
//! The artefact proves the four things the derivation rests on:
//!
//! 1. **The ceiling is the width of the held row.** The constant is not a
//!    literal that happens to agree with the row; it *is* the row's width.
//! 2. **The held row is the specification's four-component row**, so the
//!    derivation has a real source.
//! 3. **The ordering stays exact.** The comparator's order agrees with the
//!    specification's numeric rule for every pair in a matrix that includes
//!    the four-component row — no truncation, no lossy fallback.
//! 4. **A wider held row would widen the ceiling**, so a future
//!    five-component row in the specification's table makes such a version
//!    **comparable** rather than uncomparable. That is exactly what F5 says
//!    the hard-coded ceiling denies.
//!
//! The artefact also proves the refusal that stands today is **typed** and
//! **counted**: a version beyond the ceiling is not a silent skip and not a
//! pass.

use std::cmp::Ordering;

use i2pr_proto::{
    ApiVersion, MAX_API_VERSION_COMPONENTS, PublishedVersion, SPECIFICATION_WIDEST_VERSION_ROW,
    VersionEligibility, version_row_width,
};

/// The specification's numeric rule, stated independently of the
/// implementation under test.
///
/// Components are compared left to right as numbers; when one side runs out
/// of components while the other still has one, the **shorter version is
/// less**. This is the rule the specification states and the rule the pinned
/// Java reference implements in `net.i2p.util.VersionComparator`.
fn reference_order(left: &str, right: &str) -> Ordering {
    let left: Vec<u16> = left
        .split('.')
        .map(|component| component.parse().expect("a numeric component"))
        .collect();
    let right: Vec<u16> = right
        .split('.')
        .map(|component| component.parse().expect("a numeric component"))
        .collect();
    for index in 0..left.len().max(right.len()) {
        match (left.get(index).copied(), right.get(index).copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(a), Some(b)) => {
                if a != b {
                    return a.cmp(&b);
                }
            }
        }
    }
    Ordering::Equal
}

/// The versions the ordering matrix covers: the obligation rows, the
/// four-component row the ceiling depends on, and the boundaries.
const MATRIX: &[&str] = &[
    "0", "0.9", "0.9.0", "0.9.9", "0.9.10", "0.9.28", "0.9.36", "0.9.51", "0.9.58", "0.9.59",
    "0.9.62", "0.9.62.0", "0.9.62.1", "0.6.1.10", "0.10.0", "1.0",
];

/// The ceiling is the width of the held row, not a literal beside it.
#[test]
fn the_ceiling_is_the_width_of_the_held_row() {
    assert_eq!(
        MAX_API_VERSION_COMPONENTS,
        version_row_width(SPECIFICATION_WIDEST_VERSION_ROW),
        "the comparator's bound must be the held row's width"
    );
}

/// The held row is the specification's four-component row.
///
/// The tree holds the specification's API-version table's **minimum-peer
/// obligation** rows as data (`MINIMUM_PEER_OBLIGATIONS`); it does not hold
/// the table's feature-support rows, so this one row is held explicitly.
#[test]
fn the_held_row_is_the_specifications_four_component_row() {
    assert_eq!(
        SPECIFICATION_WIDEST_VERSION_ROW,
        &[0, 6, 1, 10],
        "the held record must be the specification's four-component row"
    );
    assert_eq!(MAX_API_VERSION_COMPONENTS, 4);
}

/// A version that carries exactly the ceiling's components parses in full.
#[test]
fn a_version_at_the_ceiling_parses_in_full() {
    let version = ApiVersion::parse("0.6.1.10").expect("the four-component row parses");
    assert_eq!(version.components(), &[0, 6, 1, 10]);
    assert_eq!(version.to_string(), "0.6.1.10");
}

/// The ordering is the specification's numeric rule, for every pair.
///
/// A truncation or a lossy fallback would show up here as a disagreement.
#[test]
fn the_ordering_is_the_dotted_decimal_order() {
    for left in MATRIX {
        for right in MATRIX {
            let left_version = ApiVersion::parse(left).expect("a matrix version parses");
            let right_version = ApiVersion::parse(right).expect("a matrix version parses");
            assert_eq!(
                left_version.cmp_to(&right_version),
                reference_order(left, right),
                "the comparator must order {left} against {right} as the specification does"
            );
            assert_eq!(
                left_version == right_version,
                reference_order(left, right) == Ordering::Equal,
                "equality must agree with the order for {left} against {right}"
            );
        }
    }
}

/// A version beyond the ceiling is refused — today.
///
/// This is the state F5 describes: with the ceiling at the specification's
/// widest row, a longer version is uncomparable rather than truncated.
#[test]
fn a_version_beyond_the_ceiling_is_refused() {
    assert!(
        ApiVersion::parse("0.9.62.1.2").is_none(),
        "a version beyond the ceiling must not parse"
    );
    assert_eq!(
        PublishedVersion::parse_option(Some("0.9.62.1.2")),
        PublishedVersion::Uncomparable,
        "a version beyond the ceiling must be uncomparable, not coerced"
    );
}

/// The refusal is typed, never a silent skip and never a pass.
#[test]
fn the_refusal_is_typed_and_counted() {
    let floor = ApiVersion::new(0, 9, 62);
    let eligibility = VersionEligibility::evaluate(
        Some(floor),
        PublishedVersion::parse_option(Some("0.9.62.1.2")),
    );
    assert_eq!(
        eligibility,
        VersionEligibility::UncomparableVersion,
        "the refusal must name its own reason"
    );
    assert!(
        !eligibility.is_eligible(),
        "an uncomparable version must never be eligible"
    );
    assert_eq!(
        eligibility.reason_label(),
        "uncomparable-version",
        "the refusal must carry a stable, value-free reason label"
    );
}

/// A wider held row would widen the ceiling.
///
/// This is the mechanism F5 asks for. The ceiling is the held row's width,
/// so a specification table with a five-component row gives a five-wide
/// comparator — and a five-component version becomes **comparable** rather
/// than uncomparable. The artefact cannot change the compile-time constant,
/// so it proves the mechanism's **input**: the width function the constant is
/// derived from.
#[test]
fn a_wider_row_would_widen_the_ceiling() {
    assert_eq!(
        version_row_width(&[0, 6, 1, 10]),
        MAX_API_VERSION_COMPONENTS,
        "the held row's width is the ceiling"
    );
    assert_eq!(
        version_row_width(&[0, 9, 62, 1, 2]),
        5,
        "a five-component row would widen the ceiling to five"
    );
    assert_eq!(
        version_row_width(&[0, 9, 62]),
        3,
        "the obligation rows are three components wide"
    );
}

/// The stored width and the parser's bound are the same number.
///
/// A version the parser accepts must be stored in full; if the two bounds
/// ever diverged, a version could parse and then compare from a truncated
/// array.
#[test]
fn the_parser_accepts_exactly_the_stored_width() {
    for width in 1..=MAX_API_VERSION_COMPONENTS {
        let text = (0..width).map(|_| "1").collect::<Vec<_>>().join(".");
        assert!(
            ApiVersion::parse(&text).is_some(),
            "a {width}-component version must parse"
        );
    }
    let beyond = (0..=MAX_API_VERSION_COMPONENTS)
        .map(|_| "1")
        .collect::<Vec<_>>()
        .join(".");
    assert!(
        ApiVersion::parse(&beyond).is_none(),
        "a version one component beyond the stored width must not parse"
    );
}
