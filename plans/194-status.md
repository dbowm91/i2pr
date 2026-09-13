# Plan 194 status — M6 Java I2P second-family qualification and final mixed-router closure

Status: **`in-progress-resume-java-second-family-qualification`**:
the §8 cross-family ledger/checker/workflow scaffold landed in Plan
189 and the Plan 194 second-family lane wiring sits on top of it,
but the first counted Java run stopped fail-closed at the §11
first-run topology blocker. Plan 196 is now the registered narrow
corrective for that blocker; Plan 194 resumes only after Plan 196
proves a stock exact-pinned Java controlled topology plus authenticated
SSU2 preflight through the new
`tests/integration/m6-interop/java/ControlledRouter.java` test-only
launcher.

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
plan_194 = in-progress-resume-java-second-family-qualification
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_195 = registered-blocked-by-plan194
plan_197 = passed-m6-pq-ssu2-option-support-corrective
m6_second_family_java = topology-and-authenticated-ssu2-preflight-passed-via-plan196-and-197
m6_ssu2_pq_option_tolerance = landed-via-plan197-typed-parser-surface
milestone6_java_mixed_router_interop = in-progress-resume-194 (Plan 196 external-session-established-java flipped failed -> passed; seven §11 stop-provenance install-dependent rows are the next unblocked work)
milestone6_interoperable = not-yet-claimed
next_executable_plan = 194 (resume §5.3 tunnel-over-tunnels + §5.4(b)/(c) bidirectional destination delivery + §5.5 Streaming qualification against the proven controlled Java topology + authenticated SSU2 preflight + STYLE=RAW SAM bridge)
resume_after_plan196_external = 194 (resume Java second-family qualification)
remaining_sequence = 196-execute -> resume-194 -> 195
```

Plan 193 recorded passing exact-head i2pd Streaming evidence
(33/33 rows, two passes on `3687189`,
`m6_streaming = passed-via-i2pd-2.61.0`). Plan 194 closed the
scaffolding step; the actual Java qualification is paused at the
controlled-topology boundary until a fresh external run proves the
Plan 196 implementation.

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

tests/integration/m6-interop/java/ControlledRouter.java (new — Plan 196)
  Out-of-tree test-only launcher that compiles against the staged
  Java I2P `lib/` jars and invokes the stock public
  `net.i2p.router.Router(Properties)` + `setKillVMOnEnd(false)` +
  `runRouter()` lifecycle. Never compiled into or against the
  exact-pinned source checkout. Refuses `i2p.vmCommSystem=true`;
  writes a disposable `clients.config` containing only the SAM
  bridge; mirrors the four-file no-reseed fallback chain in
  `ReseedChecker.java:107-109`; sets exact-pinned upstream property
  names (`router.reseedDisable=true`, `router.floodfillParticipant=true`,
  `i2np.ntcp.enable=false`, `i2np.ntcp2.enable=false`,
  `i2np.udp.addressSources=local`, `i2np.upnp.enable=false`,
  `router.rejectStartupTime=0`, `router.newsRefreshFrequency=0`,
  `router.updateDisabled=true`, `time.disabled=true`,
  `i2cp.tcp.host=127.0.0.1`, loopback I2CP port). Never calls
  private/internal Java methods to seed RouterInfo, install
  tunnels, populate NetDB, create destinations, or manipulate
  Streaming state.

tests/integration/m6-interop/run-java.sh (rewritten — Plan 196)
  Plan 194 §5 / §11 second-family harness, reworked at the
  topology/startup boundary only. Reserves fixed loopback Java
  SSU2 / SAM / I2CP ports before startup; compiles ControlledRouter
  into scratch with the staged `lib/` jars; drives the JVM with
  explicit `-D` system properties + the controlled launcher
  classpath; waits for `router.info` + selected SAM port; asserts
  every controlled-topology invariant in the post-startup
  `router.config` / `clients.config` / `noreseed.i2p` /
  UDP-port-bound / SAM-port-listening; never mutates
  `${JAVA_CACHE}/clients.config`; passes the actual selected
  SAM/SSU2/I2CP endpoints to `java_tunnel_external.rs`; emits
  `record_stop` provenance only if the external driver recorded
  `plan194-java-stop`. The post-run `cleanup` trap computes a
  fingerprint of the Java cache's `.config`/`runplain.sh` and
  asserts no drift across the run.

crates/i2pr-daemon/tests/java_tunnel_external.rs (existing)
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
  `failed` literal). Java rows stay `failed` until Plan 196
  external-execution produces green per-layer evidence.

scripts/check-m6-mixed-router-acceptance-evidence.sh (extended — Plan 196)
  Plan 194 / Plan 196 structural checker wiring. Adds the Plan 196
  ControlledRouter.java source as a required artifact; rejects
  `i2p.vmCommSystem=true`; rejects the obsolete Plan 194 keys
  (`i2np.reseed.enable`, `router.isFloodfill`, `i2np.ntcp2.enabled`)
  in their `setProperty` / assignment form; rejects `sed` mutations
  of the exact-pinned Java cache's `clients.config` /
  `clients.config.d`; rejects non-loopback `i2p.reseedURL` URLs;
  rejects `|| true` forgiveness of `cargo test` / `cargo fmt` /
  `cargo check` / `bash [[ ]]` / `javac -cp` / `java -D` calls
  in the harness. The Plan 189 cross-family aggregator wiring,
  both-pin verification, and `workflow_dispatch` invariants remain
  in force.

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
Plan 196 extended the checker with the controlled-topology
invariants. Neither is allowed to weaken these gates.

## Evidence before Plan 196 external-execution

Local/static lane:

```text
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
# exit 0; cross-family scaffolding + Plan 196 controlled-topology
# invariants wired into both pins and Java artifacts verified by the
# static checker.

bash tests/integration/m6-interop/run-m6-mixed-router.sh
# exit 1; Java harness still awaits the Plan 196 controlled-topology
# external run; i2pd first-family rows retain Plan 193 authority.

bash tests/integration/m6-interop/run-java.sh
# exit 1; the controlled topology rows (daemon-strict-profile,
# external-reference-verified, external-session-established,
# java-routerinfo-port-bound, java-reseed-disabled,
# java-floodfill-capable, java-ntcp-disabled,
# java-sam-bridge-configured, external-reseed-disabled) flip to
# `passed` only after a fresh external run drives the
# destination_message_plane_against_java driver against the
# controlled reference. Without the external Java execution, the
# harness remains fail-closed at the post-Plan-188 inbound-delivery
# layer + Plan 191/192 inbound-delivery boundary.
```

The repository quality floor on the Plan 194 scaffold was green:
fmt/check, 2312 tests, clippy, rustdoc, static evidence/boundary
checks, and dependency policy. Later routine CI also added the
Plan 185–194 static evidence checkers to the Linux floor; Plan 196
extends the M6 cross-family checker with controlled-topology
invariants on top of that floor.

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

Plan 196 therefore owns the controlled first-run topology and
authenticated SSU2 preflight. The Plan 196 implementation lands on
the working branch (controlled launcher + rewritten harness +
extended static checker); the next external Java execution flips
Plan 196 from `in-progress-corrective-implementation-landed-static-checks-green`
to `passed-m6-java-controlled-first-run-topology-corrective`. Plan
196 must not implement Plan 194's remaining
tunnel/NetDB/Destination/Streaming acceptance rows.

Until Plan 196 external-execution re-run passes (Plan 197 parser tolerance already landed):

```text
milestone6_java_mixed_router_interop = blocked-by-plan196-external-execution-of-landed-pq-parser-tolerance
milestone6_interoperable = not-yet-claimed
next_executable_plan = 196 (re-run external lane; session-established-java row must flip on the exact-pinned Java 2.13.0 cache)
```

## Handoff

Execute Plan 196 external-execution. On Plan 196 pass, resume this
Plan 194 at the first unproven Java second-family row after
authenticated SSU2 preflight. Plan 194 remains the only plan
allowed to close the retained two-family Milestone 6 criterion.

Expected post-Plan-196 authority:

```text
plan_197 = passed-m6-pq-ssu2-option-support-corrective
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_194 = in-progress-resume-java-second-family-qualification
plan_195 = registered-blocked-by-plan194
m6_second_family_java = topology-and-authenticated-ssu2-preflight-passed-via-plan196-and-197
m6_ssu2_pq_option_tolerance = landed-via-plan197-typed-parser-surface
milestone6_interoperable = not-yet-claimed
next_executable_plan = 194
remaining_sequence = resume-194 -> 195
```

Plan 195 (M10 remote service interop) remains blocked until Plan 194
actually flips the required Java/cross-family rows to passed and
sets the bounded M6 interoperability claim.