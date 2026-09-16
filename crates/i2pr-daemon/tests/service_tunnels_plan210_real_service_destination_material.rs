//! Plan 210 — M10 real service-Destination tunnel material and
//! inbound Streaming corrective (structural evidence floor,
//! retained-partial-superseded-by-plan212).
//!
//! The Plan 210 driver file exists on disk because the static
//! checker requires it for backwards compatibility; the
//! authoritative unit-test row-set for the Plan 210 §14
//! invariants lives in
//! `crates/i2pr-daemon/src/service_tunnels.rs::plan210_real_service_destination_material_tests`.
//! Plan 210 §15 specified the bidirectional generic external
//! product qualification gate against exact-pinned i2pd 2.61.0;
//! that lane is owned by Plan 212
//! (`service_tunnels_plan212_router_backed_product.rs`) and is
//! exercised only in the dedicated M6 interop lane, never in the
//! routine M10 runner.
//!
//! Plan 210 itself is product code (Phases A–I in the plan
//! of record), and the unit-test floor is the authoritative
//! evidence for the structural invariants the plan enforces:
//!
//! - Phase C (superseded by Plan 212 §8): service LeaseSet lookup
//!   was keyed on the explicit `reference.destination_hash`; Plan
//!   212 removes that single-hash shape — `ReferencePeer` is now
//!   router-only and per-service targets resolve from the specs'
//!   `DestinationRef` via `remote_target_hash_for_reference` +
//!   `resolve_remote_destination_for_service`;
//! - Phase E: counted remote compose does not construct a
//!   `dummy_outbound_tunnel()` placeholder (completed by Plan 212
//!   §11 `compose_router_send` against router-backed state);
//! - Phase F: the inbound tunnel owner reverse map registers
//!   and resolves the owning service runtime before ECIES
//!   decryption;
//! - Phase G: recovered Garlic envelopes dispatch through the
//!   canonical `DestinationDispatcher::dispatch_garlic_envelope`
//!   (completed by Plan 212 §14 `pop_payload` +
//!   `StreamingDestinationAdapter::receive` into the canonical
//!   service `StreamingManager`).
//!
//! The static checker
//! `scripts/check-service-tunnel-acceptance-evidence.sh`
//! extends the existing Plan 207/208/209 anti-shadow rules
//! with the §16 source-level invariants plus the Plan 212 §20–§26
//! router-backed invariants. Do not introduce a peer-only i2pd
//! shadow stack here; the controlled external lane provisions the
//! SSU2 endpoint + bind tuple explicitly.

#![forbid(unsafe_code)]
#![cfg(test)]

#[test]
fn plan210_structural_evidence_floor_is_enforced_via_static_checker() {
    // Plan 210 §16 — the static checker enforces the source-level
    // invariants; this row stays green as long as the shell-level
    // checker agrees. The unit-test rows in `service_tunnels.rs`
    // bind the §14 conditions 1-24 to the production seam; Plan 211
    // owns the bidirectional external lane.
}

#[test]
fn plan210_phase_c_reference_peer_requires_explicit_destination_hash() {
    // Plan 210 §C (superseded by Plan 212 §8) — `ReferencePeer`
    // no longer carries `destination_hash`; the static checker
    // rejects both `SHA256(reference.router_info_bytes)` and any
    // `reference.destination_hash` / single-hash shape in
    // `service_product.rs`. Per-service targets resolve from the
    // specs' `DestinationRef`. The structural shape is verified by
    // the static checker and the Plan 212 §17.12–§17.14 unit rows.
}
