# Plan 194 status — M6 Java I2P second-family qualification and final mixed-router closure

Status: **`in-progress-scaffolding-landed-blocked-by-plan196-topology-corrective`**:
the §8 cross-family ledger/checker/workflow scaffold landed in Plan
189 and the Plan 194 second-family lane wiring now sits on top of it,
but the first counted Java run stopped fail-closed at the §11
first-run topology blocker. Plan 196 is now the registered narrow
corrective for that blocker; Plan 194 resumes only after Plan 196
proves a stock exact-pinned Java controlled topology plus authenticated
SSU2 preflight.

Plan of record:
[`plans/194-m6-java-second-family-mixed-router-closure.md`](194-m6-java-second-family-mixed-router-closure.md).

Topology corrective:
[`plans/196-m6-java-controlled-first-run-topology-corrective.md`](196-m6-java-controlled-first-run-topology-corrective.md).

Historical `plans/189-m6-java-i2p-second-family-qualification-and-closure.md`
remains retained for the already-landed Plan 189 cross-family
evidence scaffold; Plan 194 supersedes its execution role so the
active sequence remains monotonic and unambiguous.

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = in-progress-scaffolding-landed-blocked-by-plan196-topology-corrective
plan_196 = registered-executable-m6-java-controlled-first-run-topology-corrective
plan_195 = registered-blocked-by-plan194
m6_second_family_java = structurally-wired-topology-corrective-pending-plan196
milestone6_interoperable = not-yet-claimed
next_executable_plan = 196
resume_after_plan196 = 194
remaining_sequence = 196 -> resume-194 -> 195
```

Plan 193 recorded passing exact-head i2pd Streaming evidence
(33/33 rows, two passes on `3687189`,
`m6_streaming = passed-via-i2pd-2.61.0`). Plan 194 closed the
scaffolding step; the actual Java qualification is paused at the
controlled-topology boundary until Plan 196 passes.

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
  Java router with a fresh disposable data dir, waits for
  `router.info` + the SAM bridge, runs the second-family external
  driver through its explicit `--ignored --exact` selection, and
  writes sanitized evidence under
  `target/interop/m6-java-evidence/evidence.{json,md}`. Records
  every row through `record_guarded` / `m6_row` / `m6_key_row` /
  `ref_row` / `blocked_row` (no hard-coded `passed`); the
  `blocked_row` helper honours the Plan 194 §11 stop provenance
  (`plan194-java-stop`). Plan 196 now owns correction of the
  first-run provisioning/startup portion of this script only.

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
  `failed` literal). Java rows stay `failed` until Plan 196 passes
  and the resumed Plan 194 second-family harness emits green
  per-layer evidence.

scripts/check-m6-mixed-router-acceptance-evidence.sh (extended)
  Plan 194 structural checker wiring. Adds `run-streaming.sh` /
  `check-streaming-tunnel-evidence.sh` (Plan 193) and
  `run-java.sh` / `fetch-m6-java.sh` (Plan 194) to the
  required-artifacts list, so routine CI + the manual external
  workflow both fail closed if either side of the two-family
  contract drifts. The cross-family aggregator script
  (`run-m6-mixed-router.sh`) must still reference both pins and
  keep its `cross_family_row` helper.

.github/workflows/m6-mixed-router-external.yml (extended)
  Plan 194 hosted-workflow wiring. The build-dependencies step
  installs the Java I2P stack alongside i2pd (ant,
  default-jdk-headless, gettext-base); the workflow runs
  `scripts/interop/fetch-m6-java.sh --rebuild` before the
  per-layer harnesses and pipes the build log to the failure
  log tail. The Java second-family run is fail-closed: missing
  Java cache or failed Java lane → cross-family aggregator records
  Java rows failed/blocked with stop provenance, never passed.
```

The existing Plan 161/162/184–193 per-layer static checkers and
the Plan 189 cross-family static checker remain retained. Plan 194
added the second-family artifacts and per-layer exit-code binding;
Plan 196 is not allowed to weaken these gates.

## Evidence before Plan 196

Local/static lane:

```text
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
# exit 0; cross-family scaffolding wired to both pins and Java artifacts
# verified by the static checker.

bash tests/integration/m6-interop/run-m6-mixed-router.sh
# exit 1; Java harness reaches the recorded topology stop and Java rows
# remain blocked/failed. i2pd first-family rows retain Plan 193 authority.

bash tests/integration/m6-interop/run-java.sh
# exit 1; first-run Java topology prevents the controlled SSU2 preflight.
```

The repository quality floor on the Plan 194 scaffold was green:
fmt/check, 2312 tests, clippy, rustdoc, static evidence/boundary
checks, and dependency policy. Later routine CI also added the
Plan 185–194 static evidence checkers to the Linux floor.

## §11 stop provenance and Plan 196 correction

The first real Java lane demonstrated that the harness's prewritten
configuration approach did not control a pristine stock Java I2P
startup: the router rewrote/generated startup state, selected a
random UDP port, and performed public reseed activity. That violates
Plan 194 §3's controlled/private topology contract, so no Java
qualification row was allowed to pass from that run.

Exact-pinned source inspection after the stop established that stock
Java I2P itself provides the required supported solution:

- `net.i2p.router.Router(Properties)` accepts environment/config
  properties before any router thread starts;
- the exact-pinned upstream `MultiRouter` utility uses this path for
  isolated routers with fixed loopback UDP ports and
  `router.reseedDisable=true`;
- upstream explicitly warns that `i2p.vmCommSystem=true` bypasses
  UDP/TCP, so Plan 196 forbids that shortcut;
- exact-pinned testnet/client configuration supplies the canonical
  leak-control and SAM property names.

Plan 196 therefore owns only the controlled first-run topology and
authenticated SSU2 preflight. It must not implement Plan 194's
remaining tunnel/NetDB/Destination/Streaming acceptance rows.

Until Plan 196 passes:

```text
milestone6_java_mixed_router_interop = blocked-by-plan196-java-first-run-topology
milestone6_interoperable = not-yet-claimed
next_executable_plan = 196
```

## Handoff

Execute Plan 196. On Plan 196 pass, resume this Plan 194 at the
first unproven Java second-family row after authenticated SSU2
preflight. Plan 194 remains the only plan allowed to close the
retained two-family Milestone 6 criterion.

Expected post-Plan-196 authority:

```text
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_194 = in-progress-resume-java-second-family-qualification
plan_195 = registered-blocked-by-plan194
m6_second_family_java = topology-and-authenticated-ssu2-preflight-passed-via-plan196
milestone6_interoperable = not-yet-claimed
next_executable_plan = 194
remaining_sequence = resume-194 -> 195
```

Plan 195 (M10 remote service interop) remains blocked until Plan 194
actually flips the required Java/cross-family rows to passed and
sets the bounded M6 interoperability claim.
