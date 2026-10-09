# Plan 390 closure — blocked

Status: **blocked-post-registration-inbound-owner-installation-plan-391**.

Plan: `plans/implementation/i2pcontrol-proposal-170/390-destination-role-registry-capacity-corrective.md`.

## Scope result

The bounded mixed-role capacity corrective is implemented. `DataPlaneCapacity`
now represents finite u16 ceilings; standalone coordinators retain their
configured exploratory-pool bounds. The product coordinator derives inbound
capacity as the exploratory maximum plus 32 service groups × 8 effective
directional roles (264 with the default pool), and outbound as the exploratory
maximum plus one synchronous activation slot. Capacity release on inbound
owner removal and the coordinator's derived bounds have focused regression
coverage. The architecture deep-dives document the ownership and derivation.

## Exact-pinned evidence

Clean NONE run:
`target/interop/els2-evidence-plan390-none-capacity-20261009`

- Stock i2pd 2.61.0 at pinned SHA
  `635b013a612ff47278ef02acf8580a28e10e26c5`; `MAX_ATTEMPTS=1`.
- The authority payload passed and the control-created reverse server
  committed.
- Snapshot: 39 inbound submissions, 33 completed, 4 failed, 33 successfully
  activated/registered, 3 outbound successfully activated, and zero failures
  in every registration stage including coordinator role activation.
- The server still had zero current inbound pool registrations, zero usable
  leases, and no LS2. Lease publication reached 36 attempts, 0 accepted, 36
  failed, 1 pending, with `MissingLeaseSet`; reverse payload was not returned.
- Evidence SHA-256: `evidence.json`
  `9641318123489fdcc6a98430260d20bc4b402a316ae8ce89fd3638fc7f543349`,
  `results.tsv`
  `0376951c01f9653fa558049139e55b04dee0257921cfd30104f77ecae76553c9`,
  `driver-evidence.tsv`
  `704ddbfa377b64a1b44515ff64716830ee2fd065c7a0bec4cc1fc79f11ad8bcf`.

The aggregate capacity root is corrected: the lane moved from 61 role
activation failures to 33 successful inbound and 3 successful outbound
activations. The plan's full reverse NONE acceptance remains blocked at a
later post-activation owner/lease retention boundary. No PSK/DH row or routine
floor was run because NONE did not return the payload.

## Verification

- `rtk cargo fmt --all --check` — pass.
- Managed-app sibling build — pass.
- Coordinator capacity regression — pass.
- Data-plane inbound max+1 and removal/reuse regression — pass.
- ELS2 runner and evidence checker self-tests — pass.
- Evidence checker mutation table — 20 detected, 0 missed, 2 controls.
- Live evidence checker — pass.
- Exact-pinned NONE — did not meet reverse publication/payload acceptance as
  described above.

No dependency, wire, support inventory, conformance, capability, or
advertisement change. Diagnostics remain bounded and contain no peer identity,
role id, attempt id, or secret-bearing value.

## Successor and unblock audit

Plan 391 owns destination-scoped attribution after role activation: prove that
the activated inbound role remains in its `DestinationRuntime` pool and that
its receive owner and bridge projection install successfully, then correct the
first failing step. Plan 374 remains blocked on the complete live matrix; Plan
375 remains independently blocked on Java source-lock and live evidence. Plan
376 remains passed. Plans 377 and 378 remain blocked by their existing hard
dependencies. No downstream plan is unblocked.
