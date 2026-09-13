# Plan 194 status — M6 Java I2P second-family qualification and final mixed-router closure

Status: **`in-progress-scaffolding-landed-blocked-at-java-first-run-topology`**:
the §8 cross-family ledger/checker/workflow scaffold landed in Plan
189 and the Plan 194 second-family lane wiring now sits on top of it,
but the lane stops fail-closed at the §11 first-run topology blocker
— stock Java I2P 2.13.0 overwrites its own `router.config` on first
start, binds a random UDP port, and runs reseed against the public
I2P network, which violates Plan 194 §3's "no public reseed or
public-network dependency for green evidence" rule.

Plan of record:
[`plans/194-m6-java-second-family-mixed-router-closure.md`](194-m6-java-second-family-mixed-router-closure.md).

Historical `plans/189-m6-java-i2p-second-family-qualification-and-closure.md`
remains retained for the already-landed Plan 189 cross-family
evidence scaffold; Plan 194 supersedes its execution role so the
active sequence is monotonic and unambiguous.

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = in-progress-scaffolding-landed-blocked-at-java-first-run-topology
plan_195 = registered-blocked-by-plan194
m6_second_family_java = structurally-wired-topology-blocker-recorded
milestone6_interoperable = not-yet-claimed
next_executable_plan = 194-followup-topology-corrective (to be registered)
```

Plan 193 recorded passing exact-head i2pd Streaming evidence
(33/33 rows, two passes on `3687189`,
`m6_streaming = passed-via-i2pd-2.61.0`). Plan 194 closed the
scaffolding step; the actual Java qualification is blocked at the
Plan 194 §3 controlled-topology boundary (see §11 below).

## What landed

```text
scripts/interop/fetch-m6-java.sh (new)
  Plan 194 §3 / Plan 189 §3 second-family fetch. Clones the
  exact-pinned Java I2P 2.13.0 source, verifies the pin + remote,
  builds via `ant updater preppkg` (no IzPack 5 GUI installer — the
  staged `pkg-temp/` IS the install directory, so the upstream
  `ant pkg5` step that requires downloading IzPack 5.2.4 from
  Maven Central is deliberately skipped), stamps `build-metadata.txt`
  with the exact pin + repository + per-OS launcher probe, and
  substitutes a foreground-exec launcher so the harness can observe
  stdout/stderr for the ready token.

tests/integration/m6-interop/run-java.sh (new)
  Plan 194 §5 / §11 second-family harness. Provisions one ephemeral
  Java router with a fresh disposable data dir (loopback SSU2, no
  public reseed, SAM loopback), waits for `router.info` + the SAM
  bridge on 127.0.0.1:7656, runs the second-family external driver
  through its explicit `--ignored --exact` selection, and writes
  sanitized evidence under
  `target/interop/m6-java-evidence/evidence.{json,md}`. Records
  every row through `record_guarded` / `m6_row` / `m6_key_row` /
  `ref_row` / `blocked_row` (no hard-coded `passed`); the
  `blocked_row` helper honours the Plan 194 §11 stop provenance
  (`plan194-java-stop`).

crates/i2pr-daemon/tests/java_tunnel_external.rs (new)
  Plan 194 §5.1 + §5.4(a) second-family external driver
  (`#[ignore = "Plan 194: requires exact-pinned external Java I2P environment"]`).
  Proves the daemon-owned SSU2 runtime establishes an authenticated
  session with the exact-pinned Java reference, verifies + bootstraps
  the Java RouterInfo through the ordinary validation path, asserts
  the controlled floodfill flag, and creates a reference SAM
  `STYLE=RAW` destination through Java's public SAM bridge. The
  driver records `plan194-java-stop` so the harness marks every
  install-dependent row `blocked` (never `passed`, never silently
  skipped) until §5.2/§5.3/§5.4(b)/(c)/§5.5 lift into the driver.

tests/integration/m6-interop/run-m6-mixed-router.sh (extended)
  Plan 194 cross-family aggregator wiring. Adds `run-java.sh` as a
  fifth per-layer harness, runs it under a 600 s bounded timeout
  with the same `I2PR_M6_EVIDENCE_DIR` per-layer shim the i2pd
  harnesses use, and binds each Plan 189 §8 guarded row to a
  family + the actual `run-java.sh` exit code (not a hard-coded
  `failed` literal). Java rows stay `failed` until the topology
  corrective lands and the second-family harness emits green
  per-layer evidence.

scripts/check-m6-mixed-router-acceptance-evidence.sh (extended)
  Plan 194 structural checker wiring. Adds `run-streaming.sh` /
  `check-streaming-tunnel-evidence.sh` (Plan 193) and
  `run-java.sh` / `fetch-m6-java.sh` (Plan 194) to the
  required-artifacts list, so the routine CI + manual external
  workflow both fail closed if either side of the two-family
  contract drifts. The cross-family aggregator script
  (`run-m6-mixed-router.sh`) must still reference both pins and
  keep its `cross_family_row` helper.

.github/workflows/m6-mixed-router-external.yml (extended)
  Plan 194 hosted-workflow wiring. The build-dependencies step
  already installs the Java I2P stack alongside i2pd (ant,
  default-jdk-headless, gettext-base) for the second-family
  lane; the workflow now also runs
  `scripts/interop/fetch-m6-java.sh --rebuild` before the
  per-layer harnesses and pipes the build log to the failure
  log tail. The Java second-family run is fail-closed: missing
  Java cache → cross-family aggregator records Java rows
  `failed` with stop provenance, never `passed`.
```

The existing Plan 161/162/184–193 per-layer static checkers and
the Plan 189 cross-family static checker are unchanged in
behaviour; Plan 194 only adds to the required-artifacts list and
the per-layer exit-code binding. No `crates/` source change
beyond the new `java_tunnel_external.rs` driver and its
build-metadata companion.

## Evidence

Local lane (no external process):

```text
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
# exit 0; the cross-family aggregator scaffolding is wired to
# both pins, and the new Java second-family artifacts are
# verified by the static checker.

bash tests/integration/m6-interop/run-m6-mixed-router.sh
# exit 1; the cross-family aggregator exercises the Java
# harness on a fresh scratch router (which emits `plan194-java-stop`
# from the topology blocker below), then records every Java row
# `blocked` (never `passed`, never silently skipped). The
# i2pd-first-family rows stay green per Plan 193.

bash tests/integration/m6-interop/run-java.sh
# exit 1; the second-family harness provisions the Java router
# but the §11 topology blocker (random UDP port, public reseed,
# auto-generated router.config) prevents the SSU2 session row
# from flipping passed. Per Plan 194 §11 the harness records the
# stop key and refuses to mark any row `passed`.
```

External lane (manual workflow, exact-pinned i2pd + Java):

```text
# Workflow .github/workflows/m6-mixed-router-external.yml is
# extended in this plan; the Java second-family invocation now
# runs end-to-end (fetch + per-layer + cross-family aggregator)
# but the second-family rows stay `failed` until the first-run
# topology corrective lands.
```

## §11 Stop provenance

Plan 194 §11 stop provenance recorded by the second-family
external driver (mirrored by the `blocked_row` helper in
`run-java.sh`):

```text
plan194-java-stop = stock Java I2P 2.13.0 overwrites its own
                    router.config on first start, binds a random
                    UDP port (17587 in the recorded run), and
                    runs reseed against the public I2P network;
                    the controlled-topology rows (fixed port,
                    disabled reseed, bound floodfill) cannot be
                    set without an advanced-configuration
                    injection the stock build does not provide.
```

Per Plan 194 §11 "create one narrow corrective if needed", a
follow-up corrective plan must land a bounded Java-side
configuration shim — either a pre-built data-dir replay, or a
router-context wrapper that injects the controlled settings
after the Java router initializes its private router.context.
Until that lands:

```text
milestone6_java_mixed_router_interop = blocked-at-plan194-java-first-run-topology
milestone6_interoperable = not-yet-claimed
next_executable_plan = 194-followup-topology-corrective
```

## Handoff

Plan 194 closes the Plan 183 mixed-router cross-family
scaffolding. The two-family ledger is structurally complete and
the i2pd-first-family rows stay green per Plan 193. The Java
second-family rows stay `failed` with stop provenance pending the
first-run topology corrective. Plan 195 (M10 remote service
interop) remains blocked-by-plan194 until the second-family
qualification actually flips the Plan 194 rows to `passed`.
