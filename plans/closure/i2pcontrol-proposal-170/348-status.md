# Plan 348 — Fresh Proposal 170 full-conformance gate: status

Status: **blocked-p348-hard-deps-342-and-347-both-unmet-section1-refreeze-executed-and-clean**

Plan of record:
[`348-fresh-full-proposal170-conformance-gate.md`](../../implementation/i2pcontrol-proposal-170/348-fresh-full-proposal170-conformance-gate.md).

**Plan 348 did not pass and was not run.** Its first acceptance criterion is that Plan 342 and
Plan 347 are passed. Neither is. This record states the exact precondition state, and it
records the one part of Plan 348 that *is* independently executable — the §1 re-freeze — which
was executed and is clean.

## Precondition state

| Plan 348 hard dependency | State | Evidence |
|---|---|---|
| **Plan 342 passed**, and its closure explicitly resolves the remaining Plan-327 outproxy capability | **NOT MET.** No closure record exists. Plan 342's own status is `registered-provider-delivered-awaits-option-surface-request-paths-and-wire-evidence` | `plans/implementation/i2pcontrol-proposal-170/342-i2p-routed-outproxy-provider-and-canonical-fields.md:3`; `plans/closure/i2pcontrol-proposal-170/342-status.md` absent |
| **Plan 347 passed**, and its closure explicitly resolves the remaining Plan-326/335 ELS2 external interoperability requirement | **NOT MET.** Plan 347 stopped with a classified boundary at 0 of 4 mandatory directions | [`347-status.md`](347-status.md) — `stopped-p347-classified-boundary-java-caps-tier-and-i2pr-missing-type5-consumer-path` |
| Plans 339 and 340 remain passed successors for the historical Plan-322 source gaps | Met | `plans/closure/i2pcontrol-proposal-170/339-status.md`, `340-status.md` |
| All earlier canonical-wire / runtime / security plans remain green | Met | full `AGENTS.md` routine floor green at this branch head (see "Verification run") |

Two of four hard dependencies are unmet, and neither is discharged by anything this branch did.
Plan 342 is outside this work's scope and needs its own option-surface, request-path, and wire
evidence. Plan 347 stopped behind a boundary that is itself now behind corrective Plan 349.

**No gate in Plan 348 §§2–5 was run.** Running a local or external acceptance lane while the
plan's own first acceptance criterion is false would produce evidence for a gate that cannot
close, so none was attempted. This is the plan's own stop condition, not an omission.

## §1 Re-freeze — executed, and clean

Plan 348 §1 requires re-freezing Proposal 170 before any acceptance lane: fetch and hash the
current revision, and compare it field-by-field to the Plan-320 canonical wire freeze. That step
does not depend on Plans 342 or 347, so it was executed.

| Item | Value |
|---|---|
| Document | I2P Proposal 170, "I2PControl Expansion" |
| Status | Open |
| Source form | `https://i2p.net/proposals/170-i2pcontrol-expansion.txt` |
| Retrieved | 2026-10-05, read-only |
| Current length | 19 010 bytes |
| **Current SHA-256** | `f13ae00b886c5e72131bc5d5b138a371148d1faa6899a119a1dacb65a555e7dc` |
| Pinned SHA-256 (Plan 334 freeze, `specs/references/proposal-170-encryptleaseset-mode-mapping.md:16`) | `f13ae00b886c5e72131bc5d5b138a371148d1faa6899a119a1dacb65a555e7dc` |
| Last updated per the document | 2026-05-20 |

**The current revision is byte-identical to the frozen pin.** Same length, same digest.

That is the strongest available form of the check Plan 348 §1 asks for, and it disposes of the
whole sub-list at once:

- field-by-field comparison to the Plan-320 canonical wire freeze — **no drift possible**; the
  document that Plan 320 and Plan 334 were derived from has not changed by a single byte;
- TunnelManager type/option inventory, RouterInfo selectors, AddressBook keys, and return
  shapes — **unchanged by construction**;
- "if the Proposal changed materially, stop and register a contract reconciliation plan" — **not
  triggered; no reconciliation plan is needed**.

**This is not a conformance result.** It removes one possible reason Plan 348 could not pass;
it is not one of the reasons it currently does. Until Plans 342 and 347 pass, the project
remains `qualified-profile-closed` / canonical-wire with explicit experimental continuation,
and the claim `full-proposal-conformant` is **not** available and was not taken.

## Verification run

Executed on this branch head (`2e3ceec`), all green:

`cargo fmt --all --check`; `cargo check --locked --workspace --all-targets`;
`cargo test --locked --workspace --all-targets -- --test-threads=1` (exit 0);
`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`;
`RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`;
`cargo test --locked --workspace --doc`;
`bash scripts/check-dependency-direction.sh`; `check-runtime-boundaries.sh`;
`check-service-tunnel-boundaries.sh`; `check-fixture-manifest.sh`;
`check-els2-type11-transcript-boundary.sh`; `check-ntcp2-vectors.sh`;
`check-ssu2-vectors.sh`; `check-i2cp-vectors.sh`; `check-ntcp2-interoperability.sh`;
`check-constrained-host-lane-boundary.sh`; `check-m11-transit-boundaries.sh`;
`check-m11-transit-qualification-evidence.sh`; `check-sam-acceptance-evidence.sh`;
`check-ssu2-acceptance-evidence.sh`; `check-i2cp-acceptance-evidence.sh`;
`check-i2pcontrol-acceptance-evidence.sh`; `check-service-tunnel-acceptance-evidence.sh`;
`check-exploratory-tunnel-evidence.sh`; `check-netdb-tunnel-evidence.sh`;
`check-destination-tunnel-evidence.sh`; `check-m6-mixed-router-acceptance-evidence.sh`;
`check-m12-floodfill-qualification-evidence.sh --self-test`;
`python3 scripts/check-global-plan-number-uniqueness.py`;
`python3 -m unittest discover -s tests/planning -p 'test_*.py'`;
`python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'`;
`cargo deny check advisories bans sources` (`advisories ok, bans ok, sources ok`).

All results are **local**; no CI run was available in this environment.

## Requirement matrix

| Plan 348 requirement | Result |
|---|---|
| §1 re-freeze the current Proposal revision | **Executed. No drift.** Byte-identical to the Plan 334 pin. |
| §1 stop and register a contract reconciliation plan if materially changed | **Not triggered.** |
| Plan 342 and Plan 347 passed (acceptance criterion 1) | **NOT MET.** |
| §2 local canonical contract gate | **Not run.** Gated on criterion 1. |
| §3 runtime/capability gate | **Not run.** Gated on criterion 1. Includes the Plan-347 ELS2 row, which is unavailable. |
| §4 external differential gate | **Not run.** Gated on criterion 1; also imports Plan 347 evidence by exact SHA, which does not exist. |
| §5 security gate | **Not run as a gate.** The individual properties remain covered by the standing checks listed above, including the new `check-els2-type11-transcript-boundary.sh`. |
| §6 change the claim to `full-proposal-conformant` | **Not done, and not available.** The project stays `qualified-profile-closed` / canonical-wire. |

## Security review

No production code, configuration, or wire behaviour was changed for this plan. The only
network action was one read-only `GET` of a published specification document. No secret, key, or
network configuration was involved.

## Findings by severity

- **Critical:** none.
- **High:** none.
- **Medium:** Plan 348 remains blocked on two predecessors, one of which (347) is itself behind
  corrective Plan 349. The dependency chain to a final Proposal-170 gate is now
  `349 → 347 (i2pd directions) → 348`, with the two Java directions of 347 additionally
  requiring a bandwidth-tier design that ADR 0030 forbids inventing. This chain is recorded so
  the sequencing is not rediscovered.
- **Low (positive):** Proposal 170 has not drifted. The re-freeze that Plan 348 would have to
  perform first is already done and clean, so that step will not need repeating.
- **Low (informational):** no contract reconciliation plan is needed for Proposal 170.

## Roadmap disposition and unblock audit

Plan 348 remains **blocked** and is retained as the fresh final gate. No historical closure was
rewritten; Plan 328 remains the record of the earlier blocked attempt.

**Unblock audit** over registered plans depending on Plan 348:

| Plan | Dependency on 348 | Disposition |
|---|---|---|
| none | — | No registered plan lists 348 as a hard dependency. It is a terminal gate, so this is expected. |

Nothing is unblocked by this record, and nothing was silently unblocked. Plan 348's own
unblock condition is stated in its §6 and is unchanged: **only a passing Plan 348 may change the
subsystem claim to `full-proposal-conformant`**, and no other event may substitute for it.

## Path to running Plan 348

1. Plan 342 passes (its own scope: option surface, request paths, wire evidence).
2. Plan 349 passes (the i2pr ELS2 consumer path).
3. Plan 347 is re-attempted for the i2pd directions and passes them. The two Java directions
   additionally require a truthful bandwidth-class design with its own plan of record; until
   then those rows cannot be executed by any means that does not fabricate an advertisement.
4. Plan 348 runs §§2–5 and, only then, may set the claim in §6.

Step 1 is independent of the ELS2 branch and can proceed in parallel. Step 3 is the long pole
and is not fully reachable today, for a reason that has nothing to do with encrypted LeaseSet2.
