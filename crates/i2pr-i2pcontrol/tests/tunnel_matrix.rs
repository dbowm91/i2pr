//! Plan 292 matrix completeness and disposition-shape tests, with Plan 293
//! determinations applied.
//!
//! The frozen inventory fixes 46 options and 12 types; the mask
//! populations fix 336 applicable cells. These tests pin the disposition
//! census so no option silently disappears and no residual hides outside
//! the named limitation/corrective plans (293 determinations carried by
//! 295; correctives 296, 297).

use i2pr_i2pcontrol::tunnel::TUNNEL_TYPES;
use i2pr_i2pcontrol::tunnel_matrix::{
    APPLY_CELLS, CORRECTIVE_296_CELLS, CORRECTIVE_297_CELLS, CellDisposition, INCOMPATIBLE_CELLS,
    MATRIX, MATRIX_CELLS, NOT_APPLICABLE_CELLS, find_cell,
};
use i2pr_i2pcontrol::tunnel_options::TUNNEL_OPTIONS;

#[test]
fn matrix_covers_every_applicable_cell() {
    assert_eq!(MATRIX_CELLS, 336);
    assert_eq!(MATRIX.len(), 336);
    let mut seen = [[false; 12]; 46];
    for cell in MATRIX {
        assert!(
            TUNNEL_OPTIONS[cell.option_index].applies_to(cell.type_index),
            "cell ({}, {}) outside inventory mask",
            cell.type_index,
            cell.option_index
        );
        assert!(
            !seen[cell.option_index][cell.type_index],
            "duplicate cell ({}, {})",
            cell.type_index, cell.option_index
        );
        seen[cell.option_index][cell.type_index] = true;
    }
    for (option_index, option) in TUNNEL_OPTIONS.iter().enumerate() {
        for (type_index, _) in TUNNEL_TYPES.iter().enumerate() {
            if option.applies_to(type_index) {
                assert!(
                    seen[option_index][type_index],
                    "missing cell ({type_index}, {option_index}) for {}",
                    option.name
                );
            }
        }
    }
}

#[test]
fn disposition_census_is_exact() {
    // Plan 334 moved 12 cells: the three LeaseSet security options on the
    // four publishing kinds went from incompatible to apply, because they now
    // have the real ELS2 owners frozen by Plans 332/333.
    assert_eq!(APPLY_CELLS, 281);
    assert_eq!(NOT_APPLICABLE_CELLS, 37);
    assert_eq!(INCOMPATIBLE_CELLS, 18);
    assert_eq!(CORRECTIVE_296_CELLS, 0);
    assert_eq!(CORRECTIVE_297_CELLS, 0);
    assert_eq!(
        APPLY_CELLS
            + NOT_APPLICABLE_CELLS
            + INCOMPATIBLE_CELLS
            + CORRECTIVE_296_CELLS
            + CORRECTIVE_297_CELLS,
        MATRIX_CELLS
    );
}

#[test]
fn every_cell_carries_a_named_disposition() {
    for cell in MATRIX {
        match cell.disposition {
            CellDisposition::Apply { owner } => {
                assert!(
                    !owner.is_empty(),
                    "apply cell ({}, {}) has no owner",
                    cell.type_index,
                    cell.option_index
                );
            }
            CellDisposition::NotApplicable { reason } => {
                assert!(
                    !reason.is_empty(),
                    "not-applicable cell ({}, {}) has no reason",
                    cell.type_index,
                    cell.option_index
                );
            }
            CellDisposition::ExplicitIncompatibility { limitation } => {
                assert!(
                    !limitation.is_empty(),
                    "incompatible cell ({}, {}) has no limitation",
                    cell.type_index,
                    cell.option_index
                );
            }
            CellDisposition::CorrectivePending { plan, reason } => {
                assert!(
                    plan == 296 || plan == 297,
                    "corrective cell names plan {plan}, want 296 or 297"
                );
                assert!(!reason.is_empty());
            }
        }
    }
}

#[test]
fn every_type_and_option_appears() {
    let mut type_seen = [false; 12];
    let mut option_seen = [false; 46];
    for cell in MATRIX {
        type_seen[cell.type_index] = true;
        option_seen[cell.option_index] = true;
    }
    assert!(type_seen.iter().all(|seen| *seen));
    assert!(option_seen.iter().all(|seen| *seen));
}

#[test]
fn find_cell_respects_masks() {
    assert!(find_cell(12, 0).is_none());
    assert!(find_cell(0, 46).is_none());
    // target_destination does not apply to server (mask excludes index 1).
    assert!(find_cell(1, 4).is_none());
    // use_ssl applies to httpserver.
    assert!(find_cell(8, 6).is_some());
}

#[test]
fn spot_dispositions_match_plan_record() {
    // (server, use_ssl) -> Plan 297 TLS dial owner.
    assert!(matches!(
        find_cell(1, 6).expect("use_ssl cell").disposition,
        CellDisposition::Apply { .. }
    ));
    // (client, target_host) -> refined not-applicable (I2P destination).
    assert!(matches!(
        find_cell(0, 0).expect("target_host cell").disposition,
        CellDisposition::NotApplicable { .. }
    ));
    // (httpserver, address_helper) -> HTTP presentation policy owner.
    assert!(matches!(
        find_cell(8, 31).expect("address_helper cell").disposition,
        CellDisposition::Apply { .. }
    ));
    // (server, address_helper) -> raw TCP has no HTTP layer.
    assert!(matches!(
        find_cell(1, 31).expect("address_helper cell").disposition,
        CellDisposition::NotApplicable { .. }
    ));
    // (streamrclient, profile) -> datagram path, no streaming windows.
    assert!(matches!(
        find_cell(10, 18).expect("profile cell").disposition,
        CellDisposition::NotApplicable { .. }
    ));
    // (socks, proxy_password) -> listener auth owner.
    assert!(matches!(
        find_cell(3, 27).expect("proxy_password cell").disposition,
        CellDisposition::Apply { .. }
    ));
    // (client, sig_type) -> Plan 293 SigType incompatibility.
    assert!(matches!(
        find_cell(0, 40).expect("sig_type cell").disposition,
        CellDisposition::ExplicitIncompatibility { .. }
    ));
    // (streamrserver, encrypt_lease_set) -> Plan 334 ELS2 mode owner. The
    // Plan 293 determination that blocked every value of this key, including
    // explicit `disable`, no longer holds: the mode now selects a real record
    // shape with a named owner.
    assert!(matches!(
        find_cell(11, 41).expect("encrypt cell").disposition,
        CellDisposition::Apply { .. }
    ));
    // (server, leaseset_password) / (server, leaseset_client_auth) likewise.
    assert!(matches!(
        find_cell(1, 42).expect("lookup secret cell").disposition,
        CellDisposition::Apply { .. }
    ));
    assert!(matches!(
        find_cell(1, 44).expect("client auth cell").disposition,
        CellDisposition::Apply { .. }
    ));
    // (server, leaseset_blinding_secret) -> Plan 334 retires the duplicate
    // spelling: the ELS2 specification defines exactly one lookup secret and
    // Proposal 170 spells it once, so a second slot has no distinct owner.
    assert!(matches!(
        find_cell(1, 43).expect("duplicate secret cell").disposition,
        CellDisposition::ExplicitIncompatibility { .. }
    ));
    // (client, reply_bundling) -> Plan 296 garlic bundling owner.
    assert!(matches!(
        find_cell(0, 35).expect("reply cell").disposition,
        CellDisposition::Apply { .. }
    ));
    // (server, multihoming) -> Plan 296 target-selection owner.
    assert!(matches!(
        find_cell(1, 34).expect("multihoming cell").disposition,
        CellDisposition::Apply { .. }
    ));
    // (client, tunnel_backup_quantity) -> Plan 296 standby owner.
    assert!(matches!(
        find_cell(0, 12).expect("backup cell").disposition,
        CellDisposition::Apply { .. }
    ));
    // (client, tunnel_variance) -> Plan 296 sampler owner.
    assert!(matches!(
        find_cell(0, 13).expect("variance cell").disposition,
        CellDisposition::Apply { .. }
    ));
    // (httpclient, use_outproxy_plugin) -> Plan 293 provider semantics.
    assert!(matches!(
        find_cell(2, 45).expect("outproxy cell").disposition,
        CellDisposition::ExplicitIncompatibility { .. }
    ));
}

/// Every remaining incompatible cell carries the exact class limitation for
/// its own key, and the three LeaseSet security keys that Plan 334 promoted
/// carry no limitation at all.
#[test]
fn incompatible_cells_match_class_limitations() {
    const SIGTYPE_LIMITATION: &str =
        "dynamic destination SigType has no key-generation owner (Ed25519-only)";
    const DUPLICATE_LOOKUP_SECRET_LIMITATION: &str = "a second LeaseSet lookup secret has no distinct owner; Proposal 170 \
         spells the single ELS2 lookup secret as OptionalLookup";
    const OUTPROXY_LIMITATION: &str = "outproxy provider semantics have no I2P-routed provider";
    let mut incompatible = 0;
    for cell in MATRIX {
        match cell.disposition {
            CellDisposition::ExplicitIncompatibility { limitation } => {
                incompatible += 1;
                let expected = if cell.option_index == 40 {
                    SIGTYPE_LIMITATION
                } else if cell.option_index == 43 {
                    DUPLICATE_LOOKUP_SECRET_LIMITATION
                } else if cell.option_index == 45 {
                    OUTPROXY_LIMITATION
                } else {
                    panic!(
                        "unexpected incompatible cell ({}, {})",
                        cell.type_index, cell.option_index
                    );
                };
                assert_eq!(
                    limitation, expected,
                    "wrong limitation for cell ({}, {})",
                    cell.type_index, cell.option_index
                );
            }
            _ => {
                assert!(
                    !(cell.option_index == 40
                        || cell.option_index == 43
                        || cell.option_index == 45),
                    "deep-primitive cell ({}, {}) lost its incompatibility",
                    cell.type_index,
                    cell.option_index
                );
                // Plan 334: the three promoted keys must be apply cells on
                // every in-mask type, never silently incompatible.
                if matches!(cell.option_index, 41 | 42 | 44) {
                    assert!(
                        matches!(cell.disposition, CellDisposition::Apply { .. }),
                        "Plan 334 cell ({}, {}) is not an apply owner",
                        cell.type_index,
                        cell.option_index
                    );
                }
            }
        }
    }
    assert_eq!(incompatible, 18);
    assert_eq!(incompatible, INCOMPATIBLE_CELLS);
}
