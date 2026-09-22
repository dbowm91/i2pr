# Plan 239 — M6 Java Streaming Router-A dispatch observer

Status: **registered-ready-m6-java-streaming-router-a-dispatch-observer**

## 1. Objective

One bounded outcome: observe the next Router-A stage of the Plan-238
proven streaming response epoch (`P237-D-CLIENT-MESSAGE-NOT-ADMITTED`
on `08df6ea`, distribute delta 3 / dispatch deltas 0, zero failures)
and stop at the earliest proven D/E stage.

This is a harness/observability successor first. Do not modify
production Rust or the pinned Java implementation unless exact
expected TunnelData reaches an i2pr-owned failing stage.

## 2. Why ready

Hard dependency closed:

```text
plan_238 = passed-m6-java-streaming-router-a-admission-observer-with-client-message-not-admitted-boundary
```

Plan 238 proved Router-A I2CP admission (distribute delta exactly 3
on three identical counted executions) with proven dispatch absence
(dispatch deltas 0 with rates known) and left all post-admission D
stages as explicit Unknown. The only missing piece for the next
attribution step is a read-only Router-A dispatch observer on the
streaming response epoch: why three admitted client messages never
advance `client.dispatchTime`. Interface dependencies are stable: the
`P238-ADMISSION` delta surface, the extended source lock (seven P238
needles), and the `run-java.sh` Router-A diagnostic epoch are landed
and checker-enforced (§27).

## 3. Current implementation evidence

- `plans/closure/mixed-router-interop/238-status.md` (authoritative;
  per-attempt pre/post/deltas, enablement proof, retained baselines).
- `P238-ADMISSION` + `p238_router_stages_from_admission` + twelve
  `p238_*` unit rows (`crates/i2pr-daemon/tests/java_tunnel_external.rs`).
- Existing read-only probe precedent: `P238Probe` (StatManager
  lifetime counts) and `P231Probe` (installed-tunnel snapshots,
  single-participant HopConfig) behind diagnostic commands — reuse
  the pattern, do not copy router internals.

## 4. Invariants that must not regress

Retain unchanged: exact pins (Java I2P 2.13.0 `9134f80…`, i2pd 2.61.0
`635b013…`); Plan 232 route-derived lease-gateway correction and raw
reverse pass; Plan 234/235 boundaries; Plan 236/237/238 source lock
(structural path + seven stock signals + seven Router-A needles);
frozen topology, profile policy, publication target, timing windows
(45 s); fail-closed lanes; sanitized counts-only evidence (raw logs
scratch-only); no `|| true` forgiveness.

## 5. Scope

In:

- One read-only Router-A dispatch observation surface for the
  streaming response epoch, in order of preference: (1) stock
  Router-A `StatManager` lifetime counters for the OCMOSJ
  lease-selection path (`client.leaseSetFoundRemoteTime` /
  `client.leaseSetFailedRemoteTime` / `client.dispatchNoTunnels`
  family, queried through a tiny public diagnostic command); (2)
  read-only installed outbound client-tunnel snapshot for the
  helper client DBID on the streaming epoch (P231Probe precedent,
  exact DBID, hop-0 send id only when exactly one tunnel is
  installed); (3) stock Router-A log extraction scoped to Router A
  and the SYN epoch.
- Wire it into `streaming_through_java` as the next `P237RouterStages`
  inputs (replacing the current Unknowns for `target_leaseset_selected`,
  `outbound_tunnel_selected`, `dispatch_outbound_called`), keeping the
  Plan-237/238 B/C/D ordering and the no-later-terminal-while-earlier-
  Unknown rule.
- Static-checker invariants for the new surface (no literals, no
  placeholder satisfaction, no dispatch claim before admission, no
  production surface, no timeout/topology/pin changes, no raw-log
  promotion).

Explicitly out:

- Publication/LeaseSet2, tunnel policy/profile, exploratory/client
  tunnel changes, SAM/I2CP behavior changes, broad probe frameworks,
  Java source patching, reflection/private-field mutation, packet
  injection, forced scheduler execution, production wire changes.

Required production changes: none anticipated. A production corrective
requires a separate plan-of-record and exact expected TunnelData at an
i2pr-owned failing stage.

## 6. Ordered work packages

1. Identify the minimum stock Router-A signals for OCMOSJ lease
   selection and outbound-tunnel selection of the helper's response
   client message (exact-pinned source review; extend the source lock
   only if new assertions are needed).
2. Add the read-only diagnostic surface (existing `ControlledRouter`
   command pattern; bounded public facts only).
3. Feed it into the streaming driver epoch (baseline + post snapshots,
   deltas where the signal is cumulative) and populate the next
   `P237RouterStages` from real observations.
4. Extend the M6 checker (§28) with the §11-shaped invariants for the
   new surface.
5. Focused unit rows locking the new stage ordering (admission
   retained first; earliest D wins; i2pr stages still gated on
   expected TunnelData).

## 7. Failure / cancellation / restart / contention semantics

- Maximum three counted attempts per implementation SHA; no
  between-attempt tuning; helper/log/stat configuration fixed before
  attempt 1; fresh isolated helper per counted attempt.
- A parser/logging defect restarts the budget on a new commit.
- Pre-commit exploratory runs are not closure evidence.
- Environment-gated lane stays `#[ignore]`-gated and fail-closed.

## 8. Compatibility and migration

No wire, config, storage, or API change. Evidence TSVs gain additive
Router-A keys only; existing keys and shapes stay frozen.

## 9. Required tests

Focused unit rows (ordinary floor, no external env) covering at
minimum: admission-proven precedes any dispatch claim; earliest-D
wins; i2pr stages still require expected TunnelData;
accept/socket-surface still satisfy nothing; no production change.
Full `p236_`/`p237_`/`p238_` suites stay green.

## 10. Exact verification commands

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test java_tunnel_external p236_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p237_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p238_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p239_ -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh <exact-pinned-source> <sanitized-tsv>
javac <all staged Java helpers against exact-pinned jars>
cargo deny check advisories bans sources
```

The full serial workspace floor must also be attempted; a stall in
`sam_stream_final_acceptance` is recorded as
`P239-V-WORKSPACE-FLOOR-INCOMPLETE-SAM-HANG`, never worked around in
production code.

## 11. Documentation updates

Closure record `plans/closure/mixed-router-interop/239-status.md` with
the §19-shaped evidence (SHAs, pins, source lock, mechanism, logger
proof, pre/post/deltas, per-attempt terminals, retained baselines, no-
production-change proof, verification, workspace result, unblock audit).

## 12. Acceptance criteria

Plan 239 closes correctly when the streaming response epoch carries at
least one real Router-A dispatch observation (or a proven dispatch
absence with its stat enablement proof) and stops at the earliest
proven D/E stage on repeated counted execution — with all Plan-238
invariants retained.

## 13. Stop conditions

Stop at the first proven stage among the Plan-237 D/E/F ordering;
never emit a later terminal while an earlier stage is Unknown; never
claim i2pr-owned failure before exact expected TunnelData; never close
M6 from a single direction.

## 14. Closure evidence required

Implementation SHAs, pins, source lock, observation mechanism, logger
enablement proof, pre/post counters and deltas, sanitized log counts,
per-attempt exact terminals, retained Plan-232/235/236/237/238
baseline, proof of no production change, focused + workspace
verification, Plan-201/204 unblock audit.

## 15. Handoff notes

Start from `plans/closure/mixed-router-interop/238-status.md` §§
Observation mechanism / Counted evidence / Limitations. The narrowest
useful observer wins: a single Router-A dispatch boolean (lease-found
vs no-tunnels) with enablement proof is worth more than a broad new
framework. If dispatch proves present, continue down the existing D/E
ordering; do not invent stages.

## 16. Registration disposition

```text
plan_238 = passed-m6-java-streaming-router-a-admission-observer-with-client-message-not-admitted-boundary
plan_239 = registered-ready-m6-java-streaming-router-a-dispatch-observer

plan_201 = blocked-after-plan238-client-message-not-admitted-pending-router-a-dispatch-observer-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan239
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 239-m6-java-streaming-router-a-dispatch-observer
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```
