# Plan 063 closure record: Java I2P stripped-router direct NTCP2 driver

## Status

Plan 063 closes locally with all required artifacts, the source-locked
Java driver implementation, the build contract, the Python harness
adapter, the test matrix, and the qualification receipt. The plan
does not perform an authoritative external Java-to-i2pr or
i2pr-to-Java mixed-router run; the four primary IPv4 directions
remain typed blockers until Plan 065 closes with one complete
four-direction live diagnostic bundle and Plan 066 produces a
verified Milestone 3 certificate. NTCP2 remains experimental and
non-advertised.

The Plan 063 implementation is committed across the same source
commit as this closure record. The Plan 063 implementation commit
SHAs (helper source, source-lock, classpath manifest, build-manifest
schema, build script, run script, README, Python adapter, test
matrix, control test matrix, qualification receipt) are recorded
in this document.

## Phase 1: prerequisites

Plan 063 executed only after Plan 058 and Plan 059 closed (Plan 058
rejected ADR 0021, Plan 059 closed with the typed blocker
`blocked_java_support_topology_rejected`) and Plan 062 closed with
ADR 0022 Accepted and the v4 trigger / reference-event v1 / v3
observation schemas frozen for this implementation cycle.

| Prerequisite | Status |
| --- | --- |
| Plan 058 candidate record validator present | Yes |
| Plan 059 typed blocker marker | Yes |
| Plan 060 candidate retired | Yes |
| ADR 0022 Accepted | Yes |
| Plan 062 v4 trigger schema | Yes |
| Plan 062 reference-event v1 schema | Yes |
| Plan 062 v3 observation schema | Yes |
| Plan 062 source-verification record | Yes |

## Phase 2: workstream corrections

Plan 063 executed the ten work packages WP1-WP10 in order.

### WP1: source-verification assertions

`tests/integration/ntcp2/reference-drivers/source-verification.md`
records the source-locked API surface for the pinned Java I2P 2.12.0
revision `2800040deee9bb376567b671ef2e9c34cf3e30b6`. The Plan 063
topology contract section is appended to the same record.

### WP2: strict config parser and inspect mode

The Plan 063 strict driver config schema is
`i2pr-java-direct-driver-config-v1` (`schema_version` 1). The
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
opening a socket or starting a router process.

### WP3: common embedded-router startup/readiness/export

`runRouterSession()` constructs the embedded
`net.i2p.router.Router` with the Plan 063 mandatory property set
(see `JavaNtcp2InteropDriver.java:buildRouterProperties`). The
driver waits for `router.getRouterInfo()` to return non-null with a
bounded polling interval and monotonic deadline, then exports the
signed `RouterInfo` to the declared exchange path and verifies
`verifySignature()`, `getNetworkId() == 99`, and the exact 64-hex
Router Hash.

### WP4: listen receive handler

The driver registers a `HandlerJobBuilder` for
`DeliveryStatusMessage.MESSAGE_TYPE` (constant value 10) on the
real `InNetMessagePool`. The receive handler verifies the decoded
`getMessageId()` equals the scenario
`delivery_status_message_id` and emits
`frame_authenticated_and_decrypted` plus `i2np_message_decoded`
from the handler invocation (downstream of NTCP2 AEAD
verification, per the Plan 062 source-verification record).

### WP5: dial RouterInfo import and DeliveryStatus send

`runDialSend()` reads the peer `RouterInfo` from
`peer_router_info_path`, verifies the signature, computes the
SHA-256 of the identity bytes, compares against
`expected_peer_router_hash_sha256`, verifies the selected NTCP2
`host`/`port`, and inserts the verified RouterInfo into the dummy
NetDB via `DummyNetworkDatabaseFacade.store(...)`. It then
constructs one `DeliveryStatusMessage` with the exact
`delivery_status_message_id`, wraps it in `OutNetMessage`, and
submits through `outNetMessagePool.add(...)` exactly once.

### WP6: structured events

The driver emits Plan 062 `i2pr-reference-event-v1` records as
NDJSON to `<output_dir>/events.ndjson`. The schema enforces
strict per-process sequence ordering, exact DeliveryStatus message
ID correlation for data-phase events, and continuous Router Hash
binding. The Python adapter validates every event record through
`tests/integration/ntcp2/harness/reference_event.py`.

### WP7: shutdown verification

`shutDownEmbeddedRouter()` invokes
`Router.shutdownGracefully()` and joins the embedded router
thread with the configured `shutdown_timeout_ms`. The driver
emits `terminal_clean` only after successful shutdown; any failure
emits `terminal_rejected` and exits nonzero.

### WP8: Python harness adapter

`tests/integration/ntcp2/harness/java_direct_driver.py` is the
bounded local qualification seam. The adapter binds every helper
invocation into a Plan 062 v4 trigger record
(`i2pr-reference-trigger-v4`) and validates the Plan 063 strict
driver config contract. The adapter never reaches inside the Java
helper state and never synthesises a passing record.

### WP9: Java-to-Java control

The driver supports a sealed two-process Java-to-Java control
topology (one `listen`, one `dial`) for bounded local
qualification. The control topology is not i2pr evidence. The
canonical external lane for the 10/10 fresh-state qualification is
the Plan 046 rootless sealed-namespace lane or the Plan 048/049
Multipass recovery lane.

### WP10: qualification and status record

`tests/integration/ntcp2/qualification/java-direct-driver.json`
records the qualification state. On this host the receipt carries
`qualified = false`, the typed host-environment blocker
`blocked_unprivileged_user_namespace`, and the locked measured
digests for the helper source, the source-lock record, the
classpath manifest, the build-manifest schema, and the pinned
Java `router.jar`. The 10/10 fresh-state qualification remains to
be produced in the Plan 046 rootless sealed-namespace lane or the
Plan 048/049 Multipass recovery lane.

## Phase 3: required tests

The Plan 063 test matrix consists of:

- `tests/integration/ntcp2/harness/test_java_direct_driver.py` —
  44 cases across strict config validation, source-lock loading,
  Python harness adapter contract, structured event contract, and
  the local inspect-mode round-trip where the pinned Java cache is
  available.
- `tests/integration/ntcp2/harness/test_java_direct_control.py` —
  7 cases across the two-process control topology, the typed
  host-environment blocker contract, and the inspect-mode
  round-trip.

The Plan 063 closure criteria checklist enumerates the 22
acceptance tests from the plan. All positive and negative
fixtures pass.

## Phase 4: static checks

`scripts/check-ntcp2-interoperability.sh` is extended to enforce
the Plan 063 artifacts:

- the Java direct driver source
  (`tests/integration/ntcp2/reference-drivers/java/src/JavaNtcp2InteropDriver.java`)
  exists and carries the `java-direct-driver` and
  `i2pr-reference-event-v1` markers;
- the source-lock record exists and carries the
  `i2pr-java-helper-source-lock-v1` schema marker, the
  `java-direct-helper` helper-kind marker, and the pinned Java
  I2P 2.12.0 revision `2800040deee9bb376567b671ef2e9c34cf3e30b6`;
- the classpath manifest
  (`i2pr-java-helper-classpath-manifest-v1`) exists;
- the build-manifest schema
  (`i2pr-java-helper-build-manifest-v1`) exists;
- the build script (`build-driver.sh`) and the run script
  (`run-driver.sh`) exist;
- the Python adapter (`java_direct_driver.py`) exists and carries
  the `i2pr-reference-trigger-v4` v4 trigger binding marker and
  the `java-direct-driver` implementation marker;
- the test matrix files
  (`test_java_direct_driver.py` and `test_java_direct_control.py`)
  exist and include the `JavaHelperArtifactsPresentTests`,
  `JavaStrictConfigValidationTests`, and
  `ControlTopologyContractTests` test classes;
- the qualification receipt
  (`tests/integration/ntcp2/qualification/java-direct-driver.json`)
  exists and carries the `i2pr-java-direct-driver-qualification-v1`
  schema marker;
- `AGENTS.md` records the Plan 063 closure section and wires the
  Plan 063 test matrices.

The static check does not reject unrelated legitimate SHA-1 uses;
the scope is limited to the Plan 063 artifacts and the active v4
trigger, v3 observation, and reference-event schemas.

## Phase 5: documentation updates

The following documentation files record the Plan 063 correction:

- `README.md` records the Plan 063 Java direct driver
  implementation and the Plan 063 closure contract. The Java
  direct driver is now committed; no support topology is required
  for the primary four directions; no live qualification has
  occurred yet; Plan 060 remains retired; NTCP2 remains
  experimental/non-advertised.
- `AGENTS.md` adds the Plan 063 workstream summary and the Plan
  063 focused checks (`test_java_direct_driver`,
  `test_java_direct_control`, plus `test_plan062`,
  `test_reference_trigger_v4`, `test_reference_event`, plus the
  three boundary check scripts).
- `docs/architecture/interop-apparatus.md` records the Plan 063
  evidence-contract correction in the architecture documentation.
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md` records the
  Plan 063 workstream summary and the Plan 063 focused checks.
- `tests/integration/ntcp2/README.md` records the Plan 063
  Java direct driver implementation and the Plan 063 closure
  contract.
- `tests/integration/ntcp2/reference-drivers/source-verification.md`
  records the Plan 063 topology contract section.

## Phase 6: validation commands and results

All Plan 063 closure checks passed locally at the Plan 063
implementation commit:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_java_direct_driver.py'
# Ran 44 tests in 3.797s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_java_direct_control.py'
# Ran 7 tests in 7.913s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan062.py'
# Ran 35 tests in 0.003s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
# Ran 27 tests in 0.002s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
# Ran 13 tests in 0.001s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
# Ran 64 tests in 7.185s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
# Ran 643 tests in 53.266s — OK (skipped=2)

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

RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
# passes
```

## Phase 7: Plan 063 measured digests

The Plan 063 implementation carries the following SHA-256 digests at
the closure-record commit:

```text
221a7ad85ef7a40e632b38a03208ebc957c100cbc5040d51aacfd2f83e4384f7  tests/integration/ntcp2/reference-drivers/java/src/JavaNtcp2InteropDriver.java
71ac76a84da13bc824d6de351078ec233cb413cfc2130507a2eb00f7e451748b  tests/integration/ntcp2/reference-drivers/java/source-lock.json
9e6ac79de60ed418343f8eda0e0951b6c41366836371cdcaf8827586045b6b63  tests/integration/ntcp2/reference-drivers/java/classpath-manifest.json
ae2682b96a278d08c1cf5a003129bb305a883e35b468dc87757f45769d7ba594  tests/integration/ntcp2/reference-drivers/java/build-manifest.schema.json
369ad99bf8e8856cc13a82fcda1ba04a052f7af01c7d78599b419bfa76841bdb  tests/integration/ntcp2/reference-drivers/java/build-driver.sh
803197ca664cb5d6c9d1c3dd165ff76df0d146fda0f1421c87e889fb440c3c6b  tests/integration/ntcp2/reference-drivers/java/run-driver.sh
69152e62dd27577858fc4b3021c347d13ef51a951f15af4a63670de3768a95c6  tests/integration/ntcp2/reference-drivers/java/README.md
34bbf115f088f0769bca394846fab98c1e388b4b29af85548bdb1f06fa3bce6a  tests/integration/ntcp2/harness/java_direct_driver.py
70b88141bca9fdd464dbdb0b8a1aba88f3ca533be5e6ec586cd671691b050e5c  tests/integration/ntcp2/harness/test_java_direct_driver.py
c329c60de735c4d9159ace9d0c2876e8dff5a65fe65485d9b1972f4561e95d5f  tests/integration/ntcp2/harness/test_java_direct_control.py
a5eec42773d2860df4fe3e9043bd62d702a02a433d667a75642d562ae7738dd9  tests/integration/ntcp2/qualification/java-direct-driver.json
```

The Plan 062 source-verification record
(`tests/integration/ntcp2/reference-drivers/source-verification.md`)
gains the Plan 063 topology contract section in this commit.

## Closure criteria checklist

The Plan 063 closure criteria are met:

- [x] source-verification assertions are recorded in the Plan 062
      source-verification record (Plan 063 topology contract
      section appended);
- [x] strict config parser and inspect mode are implemented in the
      Java helper and the Python adapter;
- [x] common embedded-router startup/readiness/export is
      implemented (`runRouterSession`);
- [x] listen receive handler (`HandlerJobBuilder` for
      `DeliveryStatusMessage.MESSAGE_TYPE`) is implemented;
- [x] dial RouterInfo import and DeliveryStatus submission is
      implemented;
- [x] structured events validate against the Plan 062
      reference-event v1 schema;
- [x] shutdown verification is implemented
      (`shutDownEmbeddedRouter`);
- [x] Python harness adapter is implemented
      (`java_direct_driver.py`);
- [x] Java-to-Java control tests cover the topology contract and
      the inspect-mode round-trip;
- [x] qualification state is recorded in
      `qualification/java-direct-driver.json` with the typed
      host-environment blocker;
- [x] all required positive and negative tests pass (44
      `test_java_direct_driver.py` cases and 7
      `test_java_direct_control.py` cases);
- [x] static checks reject reintroduction of unmeasured
      provenance (`scripts/check-ntcp2-interoperability.sh`);
- [x] full applicable local validation passes;
- [x] `plans/063-status.md` records the Plan 063 implementation
      surface and makes no mixed-router i2pr success claim.

## Remaining work

- Plan 064 implements the i2pd direct NTCP2 driver with a
  compile-time-gated passive observer. Plan 064 may execute in
  parallel with Plan 063.
- Plan 065 is the canonical integration and live qualification
  pass. Plan 065 starts only after Plans 063 and 064 close and
  their exact drivers are buildable from pinned source.
- Plan 066 is the fresh candidate and authoritative NTCP2 two-run
  closure pass. Plan 066 starts only after Plan 065 closes with
  one complete independently verified four-direction live
  diagnostic bundle.

NTCP2 remains experimental and non-advertised; Milestone 3 stays
open until Plan 065 closes with one complete four-direction live
diagnostic bundle and Plan 066 produces a verified Milestone 3
certificate. The Plan 063 implementation surface
(`JavaNtcp2InteropDriver.java`, `source-lock.json`,
`classpath-manifest.json`, `build-manifest.schema.json`,
`build-driver.sh`, `run-driver.sh`, `java_direct_driver.py`,
`test_java_direct_driver.py`, `test_java_direct_control.py`,
`qualification/java-direct-driver.json`) is the mandatory
prerequisite for any change that would re-enable Plan 063 as
active execution authority.
