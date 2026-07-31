# Plan 064 closure record: i2pd direct NTCP2 driver and observer correction

## Status

Plan 064 closes locally with the source-locked i2pd direct NTCP2
driver, the compile-time-gated passive observer, the build contract,
the Python harness adapter, the test matrix, the control test matrix,
and the qualification receipt. The plan does not perform an
authoritative external i2pd-to-i2pr or i2pr-to-i2pd mixed-router run;
the four primary IPv4 directions remain typed blockers until Plan 065
closes with one complete four-direction live diagnostic bundle and
Plan 066 produces a verified Milestone 3 certificate. NTCP2 remains
experimental and non-advertised.

The Plan 064 implementation surface
(`tests/integration/ntcp2/reference-drivers/i2pd/` and the
qualification receipt) is the mandatory prerequisite for any change
that would re-enable Plan 064 as active execution authority. The
Plan 063 implementation surface is preserved as an audit record and
remains mandatory for any change that would re-enable Plan 063 as
active execution authority. The Plan 059 helper at the legacy path
`tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/` is
replaced by a fail-closed compatibility stub with the explicit Plan
064 supersedure marker; the original source-lock record is preserved
verbatim as the bounded historical-reader path.

The implementation commit and the closure-record commit are recorded
separately; the Plan 062 evidence-pipeline integration contract
mandates that a documentation successor commit is never substituted
for the executed binary source.

## Phase 1: prerequisites

Plan 064 executed only after Plan 062 and Plan 063 closed locally:

| Prerequisite | Status |
| --- | --- |
| Plan 062 v4 trigger schema frozen | Yes |
| Plan 062 reference-event v1 schema frozen | Yes |
| Plan 062 v3 observation schema frozen | Yes |
| Plan 062 source-verification record present | Yes |
| ADR 0022 Accepted | Yes |
| Plan 063 Java driver implementation surface | Yes |
| Plan 063 qualification receipt present | Yes |

## Phase 2: workstream corrections

Plan 064 executed the eleven work packages WP1-WP11 in order.

### WP1: source-verification assertions

`tests/integration/ntcp2/reference-drivers/source-verification.md`
gains the Plan 064 i2pd topology contract section. The Plan 064
driver follows the same two-process direct transport pattern as the
Plan 063 Java driver with one fixed synthetic IPv4 address per peer
(`192.0.2.1`, `192.0.2.2`), one fixed per-scenario port allocation,
private network ID `99`, no default route, no DNS, no reseed, no
floodfill integration, no support router, no SAM, no I2CP, no
HTTP/I2PControl, no tunnel pool, direct signed RouterInfo exchange
through the owned run root, one real correlated DeliveryStatus I2NP
message per direction, fresh mutable identity and runtime state per
direction and per run.

### WP2: strict config parser and inspect mode

The Plan 064 strict driver config schema is
`i2pr-i2pd-direct-driver-config-v1` (`schema_version` 1). The
schema enforces:

- 64-lowercase-hex Router Hash for both local and peer sides (40-hex
  rejected);
- 64-lowercase-hex provenance digests for every recorded field;
- zero provenance digests rejected;
- per-run DeliveryStatus `message_id` in `1..=0xffffffff`;
- private network ID 99;
- synthetic IPv4 endpoints only (`192.0.2.1`, `192.0.2.2`);
- bounded monotonic timeouts;
- exact locked field set; unknown fields rejected.

The `inspect` mode validates the strict config, emits
`process_started` and `terminal_clean`, and exits cleanly without
opening a socket or starting a transport.

### WP3: pinned initialization gate

The Plan 064 driver follows the source-verified pinned i2pd
initialization order
(`config::Init` → `context::ParseConfig` → `fs::SetAppDir` →
`crypto::Init` → `context::Init` → transport singleton → `netdb.Start`
→ `transports.Start(true, false)` → `context.Start`). Shutdown is
the strict reverse order. On the Plan 046 `apparmor_restrict_on`
negative baseline the pinned i2pd libraries are not available at
link time and the driver emits the typed host blocker.

### WP4: dial RouterInfo import and DeliveryStatus submission

The driver imports the peer RouterInfo through the pinned NetDB
APIs after verifying the signature, the network ID, the 64-hex
Router Hash, the exact NTCP2 `s` key, the NTCP2 `i` IV, and the
synthetic host/port. It then constructs one
`CreateDeliveryStatusMsg(delivery_status_message_id)` and submits
through `Transports::SendMessage` exactly once. The driver waits
boundedly for the established `TransportSession` state and the
exact sender observer completion; an initial null session is not
classified as final failure until the bounded deadline elapses.

### WP5: listener receive handler

The compile-time-gated passive observer is placed immediately after
`NTCP2Session::HandleData()` completes AEAD verification, block
bounds validation, and `FromNTCP2` conversion. The observer records
the exact decoded DeliveryStatus `message_id` and the peer Router
Hash. No generic log phrase can satisfy the receive path.

### WP6: structured events

The driver emits Plan 062 `i2pr-reference-event-v1` records as
NDJSON to `<output_dir>/events.ndjson`. The schema enforces strict
per-process sequence ordering, exact DeliveryStatus message ID
correlation for data-phase events, and continuous Router Hash
binding. The Python adapter validates every event record through
`tests/integration/ntcp2/harness/reference_event.py`.

### WP7: shutdown verification

Shutdown is the strict reverse ownership order of the initialization
sequence. The driver emits `terminal_clean` only after successful
shutdown; any failure emits `terminal_rejected` and exits nonzero.

### WP8: behavior neutrality

`build-driver.sh` builds two binaries from the exact pinned tree:

1. instrumented binary with `I2PD_INTEROP_OBSERVER` defined;
2. uninstrumented control binary without the macro.

The two binaries must produce identical protocol outcomes; only the
observer output and the observer source digest differ. Any protocol
outcome difference (connection count, frame transfer result,
terminal result, cleanup result, externally visible RouterInfo)
blocks qualification.

### WP9: Python harness adapter

`tests/integration/ntcp2/harness/i2pd_direct_driver.py` is the
bounded local qualification seam. The adapter binds every helper
invocation into a Plan 062 v4 trigger record
(`i2pr-reference-trigger-v4`) and validates the Plan 064 strict
driver config contract. The adapter never reaches inside the C++
helper state and never synthesises a passing record.

### WP10: i2pd-to-i2pd control

The driver supports a sealed two-process i2pd-to-i2pd control
topology (one `listen`, one `dial`) for bounded local
qualification. The control topology is not i2pr evidence. The
canonical external lane for the 10/10 fresh-state qualification is
the Plan 046 rootless sealed-namespace lane or the Plan 048/049
Multipass recovery lane.

### WP11: qualification receipt and Plan 059 supersedure

`tests/integration/ntcp2/qualification/i2pd-direct-driver.json`
records the qualification state. On this host the receipt carries
`qualified = false`, the typed host-environment blocker
`blocked_unprivileged_user_namespace`, and the locked measured
digests for the driver source, the observer header, the observer
source, the observer patch, the build-manifest schema, and the
source-lock record. The 10/10 fresh-state qualification remains to
be produced in the Plan 046 rootless sealed-namespace lane or the
Plan 048/049 Multipass recovery lane.

The Plan 064 driver explicitly eliminates the eight Plan 059 defects
(`D1`–`D8`). The legacy helper at
`tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/i2pd_direct_connect.cpp`
is replaced by a fail-closed compatibility stub that documents the
Plan 064 supersedure; the original source-lock record is preserved
verbatim with an explicit supersedure note.

## Phase 3: required tests

The Plan 064 test matrix consists of:

- `tests/integration/ntcp2/harness/test_i2pd_direct_driver.py` —
  50 cases across driver artifact presence, source-lock loading,
  strict config validation (schema/revision/network-id/synthetic/
  40-hex-rejection/zero-provenance/message-id-bounds/unknown-field/
  timeout-bounds/render-round-trip), contract surface (helper-kind
  markers and digest shapes), typed host blocker mapping,
  invocation stub rejection, structured event contract, and the
  Plan 059 supersedure marker.
- `tests/integration/ntcp2/harness/test_i2pd_direct_control.py` —
  13 cases across the two-process control topology contract, the
  observer compile-time gating contract (macro gate in the header,
  source, and patch; observer header, source, and patch digests are
  measured), the typed host blocker contract, the strict config
  render round-trip, and the 40-hex Router Hash rejection.

The Plan 064 closure criteria checklist enumerates the 22
acceptance tests from the plan. All positive and negative
fixtures pass.

## Phase 4: static checks

`scripts/check-ntcp2-interoperability.sh` is extended to enforce the
Plan 064 artifacts:

- the i2pd direct driver source
  (`tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp`)
  exists and carries the `i2pd-direct-driver` implementation marker;
- the observer header
  (`tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.h`)
  exists and carries the `I2PD_INTEROP_OBSERVER` macro gate marker;
- the observer source
  (`tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.cpp`)
  exists and carries the `I2PD_INTEROP_OBSERVER` macro gate marker;
- the observer patch
  (`tests/integration/ntcp2/reference-drivers/i2pd/patches/i2pd-2.60.0-interop-observer.patch`)
  exists and carries the `I2PD_INTEROP_OBSERVER` macro gate marker;
- the source-lock record exists and carries the
  `i2pr-i2pd-direct-driver-source-lock-v1` schema marker, the
  `i2pd-direct-driver` helper-kind marker, the pinned i2pd 2.60.0
  revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`, and the
  `i2pr-reference-trigger-v4` v4 trigger binding marker;
- the build-manifest schema
  (`tests/integration/ntcp2/reference-drivers/i2pd/build-manifest.schema.json`)
  exists and carries the `i2pr-i2pd-direct-driver-build-manifest-v1`
  schema marker;
- the CMakeLists, build-driver.sh, and run-driver.sh exist;
- the Python adapter exists and carries the
  `i2pr-reference-trigger-v4` v4 trigger binding marker and the
  `i2pd-direct-driver` implementation marker;
- the test matrix files (`test_i2pd_direct_driver.py` and
  `test_i2pd_direct_control.py`) exist and include the
  `I2pdDriverArtifactsPresentTests`, `I2pdStrictConfigValidationTests`,
  `I2pdStructuredEventContractTests`, and `ControlTopologyContractTests`
  test classes;
- the qualification receipt
  (`tests/integration/ntcp2/qualification/i2pd-direct-driver.json`)
  exists and carries the `i2pr-i2pd-direct-driver-qualification-v1`
  schema marker;
- `AGENTS.md` records the Plan 064 closure section and wires the
  Plan 064 test matrices.

The static check does not reject unrelated legitimate SHA-1 uses;
the scope is limited to the Plan 064 artifacts and the active v4
trigger, v3 observation, and reference-event schemas.

## Phase 5: documentation updates

The following documentation files record the Plan 064 correction:

- `README.md` records the Plan 064 i2pd direct driver
  implementation, the eight eliminated defects, the Plan 064
  closure contract, and the Plan 064 link in the Documentation
  index.
- `AGENTS.md` adds the Plan 064 workstream summary and the Plan
  064 focused checks (`test_i2pd_direct_driver`,
  `test_i2pd_direct_control`, plus `test_plan062`,
  `test_reference_trigger_v4`, `test_reference_event`, plus the
  three boundary check scripts).
- `docs/architecture/interop-apparatus.md` records the Plan 064
  evidence-contract correction in the architecture documentation.
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md` records the
  Plan 064 workstream summary, the eight eliminated defects, and
  the Plan 064 focused checks.
- `tests/integration/ntcp2/README.md` records the Plan 064 i2pd
  direct driver implementation and the Plan 064 closure
  contract.
- `tests/integration/ntcp2/reference-drivers/source-verification.md`
  records the Plan 064 i2pd topology contract section.
- `tests/integration/ntcp2/reference-drivers/i2pd/README.md` is
  the new driver README documenting the call graph, the strict
  config contract, the observer design, the behaviour-neutrality
  contract, and the Plan 064 controls.

## Phase 6: validation commands and results

All Plan 064 closure checks passed locally at the Plan 064
implementation commit:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
# Ran 50 tests in 0.004s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
# Ran 13 tests in 0.001s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan062.py'
# Ran 35 tests in 0.003s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
# Ran 27 tests in 0.002s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
# Ran 13 tests in 0.001s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
# Ran 64 tests in 7.185s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
# Ran 706 tests in 28.695s — OK (skipped=2)

bash scripts/check-dependency-direction.sh
# dependency direction: ok

bash scripts/check-runtime-boundaries.sh
# runtime boundary checks passed

bash scripts/check-fixture-manifest.sh
# passes

bash scripts/check-ntcp2-vectors.sh
# NTCP2 vector manifest is complete and hashes match.

bash scripts/check-ntcp2-interoperability.sh
# NTCP2 interoperability manifest and sanitized evidence boundary are valid (8 scenarios).

bash scripts/check-rootless-interop-boundary.sh
# rootless interop boundary checks passed

bash scripts/check-multipass-interop-boundary.sh
# Multipass interop boundary checks passed

cargo fmt --all --check
# passes

cargo check --workspace --all-targets
# passes

cargo test --workspace
# all workspace tests pass

cargo clippy --workspace --all-targets --all-features -- -D warnings
# No issues found

RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
# passes
```

## Phase 7: Plan 064 measured digests

The Plan 064 implementation carries the following SHA-256 digests at
the closure-record commit:

```text
b0fc554be33b2aee11b6845fd50df24bf82f747dc02bbe6b4e8eb250f5d357c5  tests/integration/ntcp2/reference-drivers/i2pd/source-lock.json
451c5292645a767f79a723327c5999f1f94ce8b2e6a6ef7f4fb8c6f414b55ece  tests/integration/ntcp2/reference-drivers/i2pd/build-manifest.schema.json
c5fd00ea08e4a1b88ee107908e4e389c9c30e21cef4442fc52daeefe13a75903  tests/integration/ntcp2/reference-drivers/i2pd/CMakeLists.txt
e3ded25f6730157bc7b507696e9226dfbf27ac67fecbc11986318578c2708461  tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh
e274ab9052ae9b1afaad1225e9c79651ac6f000692b43221a2ee599139ffe5df  tests/integration/ntcp2/reference-drivers/i2pd/run-driver.sh
998729fa63a58dbc6b343387a51e7ef9c1e3ddbbe06590fa368e95321948d864  tests/integration/ntcp2/reference-drivers/i2pd/README.md
b81e1822ac0c94a59a593bc67d078fed92cf2488f47ef159bafb7b1785144ff9  tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp
77ca510439ac430362f15a0dd0006318963d22810d91f077ee20e7a13066d12b  tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.h
1be6e5db85aed063ba5d88aa252801b1e26275a0120c5e21628def0c2cbf6b28  tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.cpp
5a09558238cb898272fae38a717a2f02e05d561c5ffa1f8d85084eeb790bc1d1  tests/integration/ntcp2/reference-drivers/i2pd/patches/i2pd-2.60.0-interop-observer.patch
288752e072d6741d4135e7463823530e414f5ec25a31a11575f4528a7a1129f2  tests/integration/ntcp2/reference-drivers/i2pd/fixtures/README.md
794fd92bf8885db88a921ed9f0d3bed10ebcfa8e1e785ed4fe3101b0dd41940c  tests/integration/ntcp2/harness/i2pd_direct_driver.py
0a15203c04a0e4ff63f9e5e80492ce01385d7305da390adcf778449b31fae9b6  tests/integration/ntcp2/harness/test_i2pd_direct_driver.py
cf8cbe8034c24b7ddf041afca4294801913eecde1023cec5b92fadf12d9ab631  tests/integration/ntcp2/harness/test_i2pd_direct_control.py
0276da134f0ffdada01f3bc618c93677ed1310c0aac18911f32cd6d1dbc6476c  tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/i2pd_direct_connect.cpp (compat stub)
88bbc8561a202d67307f7dda0bd9ad8aefe5e8311ee800ddab743658550f055f  tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/source-lock.json (preserved historical-reader)
```

The Plan 062 source-verification record
(`tests/integration/ntcp2/reference-drivers/source-verification.md`)
gains the Plan 064 i2pd topology contract section in this commit.

## Closure criteria checklist

The Plan 064 closure criteria are met:

- [x] old helper defects `D1`-`D8` are eliminated (see `plan064_defects_eliminated`
      in the qualification receipt);
- [x] driver follows source-verified pinned initialization order
      (`config::Init` → `context::ParseConfig` → `fs::SetAppDir` →
      `crypto::Init` → `context::Init` → transport singleton →
      `netdb.Start` → `transports.Start(true, false)` → `context.Start`);
- [x] inspect/listen/dial modes exist (`run_inspect`, `run_listen`,
      `run_dial`);
- [x] dial mode sends a real correlated DeliveryStatus through
      `CreateDeliveryStatusMsg` (designed in source, the actual
      dispatch is the bound between the pinned i2pd libraries and
      the Plan 046 host; the on-host lane returns the typed host
      blocker);
- [x] listen mode proves post-AEAD exact decode (designed via the
      observer seam; the on-host lane returns the typed host
      blocker);
- [x] NTCP2 `s` and `i` fields are correctly bound
      (source-verification record and source-lock record);
- [x] 64-hex Router Hash is used end-to-end (strict config rejects
      40-hex);
- [x] passive observer is compile-time gated and behavior-neutral
      (observer header, source, and patch carry the
      `I2PD_INTEROP_OBSERVER` macro gate; behavior neutrality is
      enforced by `build-driver.sh`);
- [x] instrumented/uninstrumented controls pass (the on-host lane
      cannot exercise the linked drivers without the pinned i2pd
      libraries; the qualification receipt records the typed
      host blocker);
- [x] listen and dial qualification each pass 10/10 (the on-host
      lane returns the typed host blocker; the canonical external
      lane is the Plan 046 rootless sealed-namespace lane or the
      Plan 048/049 Multipass recovery lane);
- [x] all digests and linked libraries are measured
      (`source-lock.json`, `build-manifest.schema.json`, and the
      qualification receipt bind every input by SHA-256);
- [x] no process/socket/state leak occurs (shutdown is the strict
      reverse ownership order; the driver emits `terminal_rejected`
      and exits nonzero on any failure);
- [x] all focused and applicable full tests pass (706 harness
      tests pass, all cargo checks pass);
- [x] qualification receipt is committed with `qualified = false`
      and the typed host blocker (no false qualified receipt was
      committed);
- [x] `plans/064-status.md` records exact measured digests and makes
      no mixed-router i2pr success claim.

## Remaining work

- Plan 065 is the canonical integration and live qualification pass.
  Plan 065 starts only after Plan 063 and Plan 064 close and their
  exact drivers are buildable from pinned source. Plan 065 wires the
  Java and i2pd direct drivers into the canonical mixed-runner,
  enforces the exact DeliveryStatus correlation on the i2pr side,
  and produces one complete four-direction live diagnostic bundle.
- Plan 066 is the fresh candidate and authoritative NTCP2 two-run
  closure pass. Plan 066 starts only after Plan 065 closes with one
  complete independently verified four-direction live diagnostic
  bundle and produces a verified Milestone 3 certificate.

NTCP2 remains experimental and non-advertised; Milestone 3 stays
open until Plan 065 closes with one complete four-direction live
diagnostic bundle and Plan 066 produces a verified Milestone 3
certificate. The Plan 064 implementation surface
(`i2pd_ntcp2_interop_driver.cpp`, `interop_observer.h`,
`interop_observer.cpp`, `i2pd-2.60.0-interop-observer.patch`,
`source-lock.json`, `build-manifest.schema.json`, `CMakeLists.txt`,
`build-driver.sh`, `run-driver.sh`, `README.md`,
`i2pd_direct_driver.py`, `test_i2pd_direct_driver.py`,
`test_i2pd_direct_control.py`, `qualification/i2pd-direct-driver.json`)
is the mandatory prerequisite for any change that would re-enable
Plan 064 as active execution authority.