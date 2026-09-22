# Plan 238 — M6 Java Streaming Router-A admission observer

Status: **registered-ready-m6-java-streaming-router-a-admission-observer**

## 1. Objective

One bounded outcome: observe the first Router-A stage of the Plan-237
proven streaming response epoch (`P237-D-ROUTER-I2CP-NOT-OBSERVED` on
`a1065d1`, deltas scheduler 2 / construction 1 / sendMessage 3, zero
failures) and stop at the earliest proven D/E stage.

This is a harness/observability successor first. Do not modify production
Rust or the pinned Java implementation unless exact expected TunnelData
reaches an i2pr-owned failing stage.

## 2. Why ready

Hard dependency closed:

```text
plan_237 = passed-m6-java-streaming-stock-response-observability-corrective-with-router-i2cp-not-observed-boundary
```

Plan 237 proved `I2PSession.sendMessage` returned past the send call on
three identical counted executions and left Router-A/C/B stages as
explicit Unknown (all-false `P237RouterStages`, no probe). The only
missing piece for D attribution is a read-only Router-A observer on the
streaming response epoch. Interface dependencies are stable: the
`REPORT_RESPONSE_STATS` delta surface, the extended source lock, and the
`run-java.sh` fresh-helper epoch are landed and checker-enforced (§26).

## 3. Current implementation evidence

- `plans/closure/mixed-router-interop/237-status.md` (authoritative;
  per-attempt pre/post/deltas, logger proof, retained baselines).
- `REPORT_RESPONSE_STATS` + `p237_classify_response` + twelve `p237_*`
  unit rows (`crates/i2pr-daemon/tests/java_tunnel_external.rs`).
- Existing read-only probe precedent: `P231Probe` (`statManager()`
  lifetime counts, installed-tunnel snapshots, single-participant
  HopConfig) behind `P231-GATEWAY` / `P231-CLIENT-OUTBOUND` /
  `P231-PARTICIPATING` diagnostic commands — reuse the pattern, do not
  copy router internals.

## 4. Invariants that must not regress

Retain unchanged: exact pins (Java I2P 2.13.0 `9134f80…`, i2pd 2.61.0
`635b013…`); Plan 232 route-derived lease-gateway correction and raw
reverse pass; Plan 234/235 boundaries; Plan 236/237 source lock
(structural path + seven stock signals); frozen topology, profile policy,
publication target, timing windows (45 s); fail-closed lanes; sanitized
counts-only evidence (raw logs scratch-only); no `|| true` forgiveness.

## 5. Scope

In:

- One read-only Router-A observation surface for the streaming response
  epoch, in order of preference: (1) stock Router-A `StatManager`
  lifetime counters queried through a tiny public diagnostic command;
  (2) stock Router-A log extraction scoped to Router A and the SYN epoch;
  (3) a minimal out-of-tree read-only probe compiled against the exact
  pinned jars (P231Probe precedent).
- Wire it into `streaming_through_java` as the `P237RouterStages` input
  (replacing the current all-false Unknown), keeping the Plan-237
  B/C ordering and the no-later-terminal-while-earlier-Unknown rule.
- Static-checker invariants for the new surface (no literals, no
  placeholder satisfaction, no Router-A claim before sendMessage,
  no production surface, no timeout/topology/pin changes, no raw-log
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

1. Identify the minimum stock Router-A signals for I2CP admission of
   the helper's response client message (exact-pinned source review;
   extend the source lock only if new assertions are needed).
2. Add the read-only diagnostic surface (existing `ControlledRouter`
   command pattern; bounded public facts only).
3. Feed it into the streaming driver epoch (baseline + post snapshots,
   deltas where the signal is cumulative) and populate
   `P237RouterStages` from real observations.
4. Extend the M6 checker (§27) with the §11-shaped invariants for the
   new surface.
5. Focused unit rows locking the new stage ordering (scheduler/ACK/
   sendMessage retained first; earliest D/E wins; i2pr stages still
   gated on expected TunnelData).

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

Focused unit rows (ordinary floor, no external env) covering at minimum:
sendMessage-proven precedes any D claim; earliest-D wins; i2pr stages
still require expected TunnelData; accept/socket-surface still satisfy
nothing; no production change. Full `p236_`/`p237_` suites stay green.

## 10. Exact verification commands

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test java_tunnel_external p236_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p237_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p238_ -- --test-threads=1
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
`P238-V-WORKSPACE-FLOOR-INCOMPLETE-SAM-HANG`, never worked around in
production code.

## 11. Documentation updates

Closure record `plans/closure/mixed-router-interop/238-status.md` with
the §19-shaped evidence (SHAs, pins, source lock, mechanism, logger
proof, pre/post/deltas, per-attempt terminals, retained baselines, no-
production-change proof, verification, workspace result, unblock audit).

## 12. Acceptance criteria

Plan 238 closes correctly when the streaming response epoch carries at
least one real Router-A observation (or a proven Router-A absence with
its logger/stat enablement proof) and stops at the earliest proven D/E
stage on repeated counted execution — with all Plan-237 invariants
retained.

## 13. Stop conditions

Stop at the first proven stage among the Plan-237 D/E/F ordering; never
emit a later terminal while an earlier stage is Unknown; never claim
i2pr-owned failure before exact expected TunnelData; never close M6
from a single direction.

## 14. Closure evidence required

Implementation SHAs, pins, source lock, observation mechanism, logger
enablement proof, pre/post counters and deltas, sanitized log counts,
per-attempt exact terminals, retained Plan-232/235/236/237 baseline,
proof of no production change, focused + workspace verification,
Plan-201/204 unblock audit.

## 15. Handoff notes

Start from `plans/closure/mixed-router-interop/237-status.md` §§
Observation mechanism / Counted evidence / Limitations. The narrowest
useful observer wins: a single Router-A I2CP-admission boolean with
enablement proof is worth more than a broad new framework. If Router-A
admission proves present, continue down the existing D/E ordering; do
not invent stages.

## 16. Registration disposition

```text
plan_237 = passed-m6-java-streaming-stock-response-observability-corrective-with-router-i2cp-not-observed-boundary
plan_238 = registered-ready-m6-java-streaming-router-a-admission-observer

plan_201 = blocked-after-plan237-router-i2cp-not-observed-pending-router-a-observer-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan238
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 238-m6-java-streaming-router-a-admission-observer
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```
