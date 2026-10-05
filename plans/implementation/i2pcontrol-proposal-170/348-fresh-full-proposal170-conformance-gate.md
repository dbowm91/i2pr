# Plan 348 — Fresh Proposal 170 full-conformance gate after ELS2 and outproxy closure

Status: **registered-prop170-final-gate-blocked-on-plans342-and347**

Classification: final conformance/evidence gate. No feature implementation should originate here.

Hard dependencies:
- Plan 342 passed and its closure explicitly resolves the remaining Plan-327 outproxy capability.
- Plan 347 passed and its closure explicitly resolves the remaining Plan-326/335 ELS2 external
  interoperability requirement.
- Plans 339 and 340 remain passed successors for the historical Plan-322 source gaps.
- All earlier canonical-wire/runtime/security plans remain green.

## Objective

Replace historical blocked Plan 328 with a fresh final gate that evaluates the **current** Proposal
170 implementation rather than the dependency state frozen when Plan 328 closed.

No historical closure is rewritten. Plan 328 remains the record of the earlier blocked attempt.

## 1. Re-freeze Proposal 170 before testing

Proposal 170 is Open. Before running any acceptance lane:

1. fetch and hash the current Proposal revision;
2. compare it field-by-field to the Plan-320 canonical wire freeze;
3. compare TunnelManager type/option inventory, RouterInfo selectors, AddressBook keys, and return
   shapes;
4. if the Proposal changed materially, stop and register a contract reconciliation plan before
   continuing.

Do not silently test a 2026-05-20 contract and call it current full conformance.

## 2. Local canonical contract gate

Require all current:
- authentication/JSON-RPC/TLS contract tests;
- canonical RouterInfo + ClientServicesInfo rows;
- canonical AddressBook rows and live subscription semantics;
- all twelve TunnelManager families;
- full applicable option matrix;
- transactional edit/restart/rollback;
- Red25519/ELS2 control modes;
- outproxy control fields and request paths;
- secret-redaction and resource-bound checks.

Zero parser-only/inert accepted cells.

## 3. Runtime/capability gate

Prove through production composition:
- one shared service manager;
- real publication/lookup;
- RouterInfo values from named owners;
- AddressBook ordinary resolution and administrative views share one owner;
- ELS2 externally qualified behavior from Plan 347 remains green;
- outproxy routes clearnet only through an I2P outproxy, never direct;
- disabled I2PControl remains isolated and does not perturb router startup.

## 4. External differential gate

Run the current canonical API corpus against:
- eggstack/emissary as the project-owned Proposal-170 behavioral oracle, under existing provenance
  restrictions;
- Java I2PControl PR/reference environment for the exact methods it implements;
- i2pd for the adopted base/RouterInfo/ClientServicesInfo semantics it implements.

Classify unsupported reference rows; do not treat lack of implementation in a reference as an i2pr
failure.

The ELS2 live cross-router evidence is imported from Plan 347 by exact SHA/hash, not approximated by
control-plane fixtures.

## 5. Security gate

Re-run:
- non-loopback TLS fail-closed behavior;
- authentication before protected dispatch;
- throttle/body/batch/table/task ceilings;
- no secret echo in get/rawConfig/logs/errors/evidence;
- no direct-clearnet outproxy fallback;
- no generic dual-transcript type-11 verifier;
- ELS2 deployed-transcript use confined to the typed type-5 boundary;
- path confinement for all file-bearing options;
- dependency/unsafe/MSRV policy.

## 6. Claim vocabulary

Only a passing Plan 348 may change the subsystem claim to:

`full-proposal-conformant`

That claim means conformance to the exact Proposal revision frozen by this plan. It does **not**
mean:
- I2PControl enabled by default;
- non-loopback exposure by default;
- production/stable API status;
- every historical I2P algorithm implemented;
- frontend/UI completion.

The feature may remain `experimental` even when the Proposal contract is complete.

## Acceptance criteria

Plan 348 passes only if:
- Plan 342 and Plan 347 are passed;
- no material Proposal revision drift is unresolved;
- every Proposal-required applicable row has a real owner/effect or Proposal-sanctioned explicit
  implementation-dependent disposition;
- all local security/resource/persistence gates pass;
- required external differential lanes execute;
- exact-head ordinary CI is green;
- `specs/support.toml`, `specs/CONFORMANCE.md`, roadmap and registry agree on the final claim.

Until then the project remains `qualified-profile-closed` / canonical-wire with explicit
experimental continuation, not unqualified full Proposal-170 support.
