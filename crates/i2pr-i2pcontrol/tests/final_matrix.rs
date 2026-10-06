//! Plan 295 final public-contract census.
//!
//! Every Proposal 170 dimension counted in one place, generated from
//! the frozen inventories (never prose-counted). Qualitative
//! per-cell dimensions (runtime owner, persistence, isolation,
//! security rule, evidence test) live in the dossier tables
//! (`specs/protocols/13-`, `14-`, `16-`); this test pins the exact
//! wire/source dispositions those tables classify.

use i2pr_i2pcontrol::tunnel_matrix::{CellDisposition, MATRIX};
use i2pr_i2pcontrol::{
    CLIENT_SERVICES, METHODS, ROUTER_INFO_SELECTORS, ROUTER_INFO_SOURCE_MATRIX, SET_CONFIG_KEYS,
    SOURCE_MATRIX_NEUTRAL_COUNT, SourceAvailability, TUNNEL_ACTIONS, TUNNEL_OPTIONS, TUNNEL_TYPES,
};

/// Full Proposal 170 inventory census with final Plan 295
/// dispositions.
#[test]
fn plan295_final_public_contract_census() {
    // Methods: Authenticate + RouterInfo + AddressBook + TunnelManager
    // + ClientServicesInfo (Plan 286 frozen, unchanged).
    assert_eq!(METHODS.len(), 5);

    // RouterInfo selectors: 30 frozen keys with the final source
    // census 27 available + 1 publish-gated (router.hash, Plan 288)
    // + 1 unavailable (router news, unsupported by Plan 295
    // determination) + 1 permitted-neutral (clock skew, justified).
    assert_eq!(ROUTER_INFO_SELECTORS.len(), 30);
    assert_eq!(ROUTER_INFO_SOURCE_MATRIX.len(), 30);
    let mut available = 0;
    let mut gated = 0;
    let mut unavailable = 0;
    let mut neutral = 0;
    for row in ROUTER_INFO_SOURCE_MATRIX {
        match row.availability {
            SourceAvailability::Available => available += 1,
            SourceAvailability::PublishedGated { .. } => gated += 1,
            SourceAvailability::Unavailable { .. } => unavailable += 1,
            SourceAvailability::PermittedNeutral { .. } => neutral += 1,
        }
    }
    assert_eq!((available, gated, unavailable, neutral), (27, 1, 1, 1));
    assert_eq!(SOURCE_MATRIX_NEUTRAL_COUNT, 1);

    // ClientServicesInfo: 6 frozen keys, every row answerable.
    assert_eq!(CLIENT_SERVICES.len(), 6);

    // AddressBook SetConfig: 13 frozen keys.
    assert_eq!(SET_CONFIG_KEYS.len(), 13);

    // TunnelManager: 7 actions, 12 types, 52 options.
    assert_eq!(TUNNEL_ACTIONS.len(), 7);
    assert_eq!(TUNNEL_TYPES.len(), 12);
    assert_eq!(TUNNEL_OPTIONS.len(), 52);

    // Type-by-option cells: 309 apply + 37 not-applicable + 16
    // explicit incompatibilities + 0 corrective-296
    // (Plan 296 closed every residual into an apply owner) +
    // 0 corrective-297 (Plan 297 closed the TLS residual).
    //
    // Plan 334 moved 12 cells: the three LeaseSet security options
    // (`encrypt_lease_set`, `leaseset_password`, `leaseset_client_auth`) on
    // the four publishing kinds went from incompatible to apply, because
    // they now have the real ELS2 owners.
    //
    // Plan 342 added 26 cells and moved 2: the seven outproxy options
    // (`use_outproxy_plugin`, `proxy_list`, `outproxy_auth`,
    // `outproxy_username`, `outproxy_password`, `outproxy_type`,
    // `ssl_proxies`) each apply to the four proxy client kinds, because the
    // provider that makes them meaningful now exists. Plan 293's
    // `use_outproxy_plugin` determination had been recorded against the
    // narrower `httpclient | connectclient` mask, so its two cells moved and
    // two more kinds gained the flag; the plan of record is not edited and
    // the determination is carried forward as superseded.
    //
    // What remains incompatible is 16 cells: `sig_type` (12) and
    // `leaseset_blinding_secret` (4).
    let mut apply = 0;
    let mut not_applicable = 0;
    let mut incompatible = 0;
    let mut corrective_296 = 0;
    let mut corrective_297 = 0;
    for cell in MATRIX {
        match cell.disposition {
            CellDisposition::Apply { .. } => apply += 1,
            CellDisposition::NotApplicable { .. } => not_applicable += 1,
            CellDisposition::ExplicitIncompatibility { .. } => incompatible += 1,
            CellDisposition::CorrectivePending { plan, .. } => match plan {
                296 => corrective_296 += 1,
                297 => corrective_297 += 1,
                other => panic!("cell carries unexpected corrective plan {other}"),
            },
        }
    }
    assert_eq!(MATRIX.len(), 362);
    assert_eq!(apply, 309);
    assert_eq!(not_applicable, 37);
    assert_eq!(incompatible, 16);
    assert_eq!(corrective_296, 0);
    assert_eq!(corrective_297, 0);
    assert_eq!(
        apply + not_applicable + incompatible + corrective_296 + corrective_297,
        MATRIX.len()
    );
}
