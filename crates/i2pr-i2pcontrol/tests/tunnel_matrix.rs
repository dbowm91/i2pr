//! Plan 292 matrix completeness and disposition-shape tests.
//!
//! The frozen inventory fixes 46 options and 12 types; the mask
//! populations fix 336 applicable cells. These tests pin the Plan 292
//! disposition census so no option silently disappears and no residual
//! hides outside the named owner plans (293, 296, 297).

use i2pr_i2pcontrol::tunnel::TUNNEL_TYPES;
use i2pr_i2pcontrol::tunnel_matrix::{
    APPLY_CELLS, BLOCKED_293_CELLS, CORRECTIVE_296_CELLS, CORRECTIVE_297_CELLS, CellDisposition,
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
    assert_eq!(APPLY_CELLS, 227);
    assert_eq!(NOT_APPLICABLE_CELLS, 37);
    assert_eq!(BLOCKED_293_CELLS, 30);
    assert_eq!(CORRECTIVE_296_CELLS, 39);
    assert_eq!(CORRECTIVE_297_CELLS, 3);
    assert_eq!(
        APPLY_CELLS
            + NOT_APPLICABLE_CELLS
            + BLOCKED_293_CELLS
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
            CellDisposition::BlockedPrimitive { plan, primitive } => {
                assert_eq!(plan, 293, "blocked cell names plan {plan}, want 293");
                assert!(!primitive.is_empty());
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
    // (server, use_ssl) -> Plan 297 TLS identity residual.
    assert!(matches!(
        find_cell(1, 6).expect("use_ssl cell").disposition,
        CellDisposition::CorrectivePending { plan: 297, .. }
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
    // (client, sig_type) -> Plan 293 algorithm agility.
    assert!(matches!(
        find_cell(0, 40).expect("sig_type cell").disposition,
        CellDisposition::BlockedPrimitive { plan: 293, .. }
    ));
    // (streamrserver, encrypt_lease_set) -> Plan 293 LeaseSet security.
    assert!(matches!(
        find_cell(11, 41).expect("encrypt cell").disposition,
        CellDisposition::BlockedPrimitive { plan: 293, .. }
    ));
    // (client, reply_bundling) -> Plan 296 garlic primitive.
    assert!(matches!(
        find_cell(0, 35).expect("reply cell").disposition,
        CellDisposition::CorrectivePending { plan: 296, .. }
    ));
    // (httpclient, use_outproxy_plugin) -> Plan 293 provider semantics.
    assert!(matches!(
        find_cell(2, 45).expect("outproxy cell").disposition,
        CellDisposition::BlockedPrimitive { plan: 293, .. }
    ));
}
