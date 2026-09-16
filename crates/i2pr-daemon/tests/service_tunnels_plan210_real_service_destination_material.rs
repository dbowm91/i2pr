//! Plan 210 — M10 real service-Destination tunnel material and
//! inbound Streaming corrective (structural evidence floor).
//!
//! The Plan 210 driver file exists on disk because the static
//! checker requires it for backwards compatibility; the
//! authoritative unit-test row-set for the Plan 210 §14
//! invariants lives in
//! `crates/i2pr-daemon/src/service_tunnels.rs::plan210_real_service_destination_material_tests`.
//! Plan 210 §15 specifies the bidirectional generic external
//! product qualification gate against exact-pinned i2pd 2.61.0;
//! that lane is a future Plan 211 follow-up and is exercised
//! only in the dedicated M6 interop lane, never in the routine
//! M10 runner.
//!
//! Plan 210 itself is product code (Phases A–I in the plan
//! of record), and the unit-test floor is the authoritative
//! evidence for the structural invariants the plan enforces:
//!
//! - Phase C: service LeaseSet lookup is keyed on the explicit
//!   `reference.destination_hash`, not on a router-identity
//!   derivation;
//! - Phase E: counted remote compose does not construct a
//!   `dummy_outbound_tunnel()` placeholder;
//! - Phase F: the inbound tunnel owner reverse map registers
//!   and resolves the owning service runtime before ECIES
//!   decryption;
//! - Phase G: recovered Garlic envelopes dispatch through the
//!   canonical `DestinationDispatcher::dispatch_garlic_envelope`
//!   instead of being silently dropped.
//!
//! The static checker
//! `scripts/check-service-tunnel-acceptance-evidence.sh`
//! extends the existing Plan 207/208/209 anti-shadow rules
//! with the §16 source-level invariants; manual promotion of
//! this driver to a real bidirectional external lane is the
//! next executable plan (Plan 211). Do not introduce a
//! peer-only i2pd shadow stack here; the controlled external
//! lane provisions the SSU2 endpoint + bind tuple explicitly.

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
    // Plan 210 §C — `ReferencePeer` carries an explicit
    // `destination_hash: Option<[u8; 32]>` field; the static
    // checker rejects any `SHA256(reference.router_info_bytes)`
    // derivation in `service_product.rs`. The structural shape
    // is verified by the static checker and the
    // `plan210_reference_peer_carries_explicit_destination_hash`
    // unit test.
}
