# Plan 070: i2pd driver build and first two-way live execution

## Status and dependencies

- Status: planned.
- Parent roadmap: Plan 067.
- Requires Plans 068 and 069 closed.
- Must close before Plan 071.
- Plan type: source-locked test-driver build correction, first real interoperability execution, and protocol failure triage.
- This plan may modify the Plan 064 i2pd test-only driver, observer, build scripts, Python adapter, or i2pr interoperability launcher only when a real build/run result demonstrates a bounded defect.

## Objective

Build and execute the existing pinned i2pd 2.60.0 direct NTCP2 driver on the current Ubuntu host, then obtain one exact authenticated DeliveryStatus exchange in each direction:

```text
i2pr -> i2pd
i2pd -> i2pr
```

The plan must prioritize real execution over additional harness expansion. It must not require a namespace, Multipass, candidate freeze, or Level 3 evidence bundle.

## Required success semantics

A direction passes only when:

```text
reference source lock verified
reference driver built from the pinned tree
fresh i2pr and i2pd mutable state
both signed RouterInfos validated
TCP connection established
both sides report NTCP2 authenticated
sender reports frame emitted
receiver reports frame authenticated and decrypted
receiver reports exact DeliveryStatus decoded
message ID matches scenario, sender, and receiver
peer Router Hash matches expected identities
network audit is strace-allowlist or configuration-only
cleanup is clean
```

A handshake-only connection is not a pass.

## Inputs already present

The implementation must begin by inspecting and attempting to use:

```text
tests/integration/ntcp2/reference-drivers/i2pd/source-lock.json
tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh
tests/integration/ntcp2/reference-drivers/i2pd/run-driver.sh
tests/integration/ntcp2/reference-drivers/i2pd/CMakeLists.txt
tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp
tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.h
tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.cpp
tests/integration/ntcp2/reference-drivers/i2pd/patches/i2pd-2.60.0-interop-observer.patch
tests/integration/ntcp2/harness/i2pd_direct_driver.py
tests/integration/ntcp2/qualification/i2pd-direct-driver.json
```

Do not create a replacement driver until the current driver has been built and its exact failure recorded.

## Phase 1: environment and pinned-source preflight

### Required checks

Record:

- Ubuntu version and architecture;
- available RAM and disk;
- CMake version;
- C/C++ compiler version;
- OpenSSL version and development headers;
- Boost version and required development components;
- Python version;
- Rust/Cargo version;
- strace availability/ptrace result;
- pinned i2pd source location;
- source revision and tree/archive digest verification.

### Dependency installation boundary

A setup script may document/install ordinary Ubuntu build dependencies, but:

- no sudo invocation may be embedded in the runner;
- no package install occurs during a smoke run;
- no network fetch occurs after the execution preflight begins;
- dependency setup remains a separate manual preparation command.

Recommended setup documentation or script:

```text
tests/integration/ntcp2/reference-drivers/i2pd/ubuntu-setup.md
```

A setup shell script is permitted only when idempotent, short, and clearly preparation-only.

### Acceptance

- source lock verifies;
- every required dependency is present or a precise package-level blocker is recorded;
- no protocol execution begins against an unverified source tree.

## Phase 2: build existing driver

Run the existing build script first without modification.

Required outputs:

```text
i2pd_ntcp2_interop_driver_instrumented
i2pd_ntcp2_interop_driver_control
build-manifest-instrumented.json
build-manifest-control.json
linked-library-manifest.txt
```

Required validation:

- both binaries are nonempty and executable;
- binary digests are nonzero and recorded;
- build manifests validate against the committed schema;
- linked libraries resolve with no `not found` entries;
- pristine pinned source remains unchanged;
- observer patch applies with zero fuzz;
- control build uses the pristine tree;
- instrumented and control binaries are distinguishable only by the expected observer surface and normal build metadata.

### Build defect policy

When the existing build fails:

1. capture the first compiler/linker/configuration failure;
2. compare against the pinned i2pd source, not current upstream master;
3. make the smallest test-only correction;
4. add a focused regression test or static build-contract check;
5. rerun from a clean build directory;
6. do not redesign the driver or CMake structure unless the pinned APIs prove it necessary.

Allowed build-fix surfaces:

- include paths;
- exact pinned symbol/API calls;
- target source lists;
- required linked libraries;
- compiler standard/options necessary for pinned i2pd;
- private-copy patch paths;
- manifest generation;
- test-only observer compilation.

Forbidden fixes:

- altering i2pd cryptography or transport acceptance behavior;
- disabling signature/AEAD/I2NP validation;
- replacing real i2pd calls with mocks;
- building against a different unpinned revision to avoid the failure.

## Phase 3: inspect-mode qualification

Before listener/dial execution, run the driver's inspect mode using fresh temporary state.

Inspect mode must prove:

- local i2pd context initializes in the pinned order;
- local signed RouterInfo is produced;
- RouterInfo signature verifies;
- network ID is 99;
- the selected address is NTCP2 IPv4 loopback;
- address host/port, `s`, and `i` match the strict config;
- reported Router Hash is exactly the identity hash;
- shutdown returns cleanly;
- no listener remains;
- no public bootstrap/discovery starts.

Run inspect mode for both instrumented and control binaries.

Acceptance:

- both produce equivalent protocol/configuration results;
- observer events may differ only by expected observer output;
- no execution proceeds while inspect mode is broken.

## Phase 4: i2pr-to-i2pd execution

### Roles

```text
i2pd = listener/responder
i2pr = dialer/initiator
```

### Sequence

1. create fresh run root;
2. render i2pd strict listener config;
3. start instrumented i2pd driver;
4. wait for structured listener-ready and RouterInfo export;
5. strict-validate i2pd RouterInfo;
6. render i2pr scenario with the exact i2pd RouterInfo, Router Hash, NTCP2 key/IV, and message ID;
7. start i2pr dialer;
8. wait for NTCP2 authentication on both sides;
9. have i2pr emit one exact DeliveryStatus frame;
10. require i2pd post-AEAD/post-I2NP observer to report the exact ID and peer Router Hash;
11. shut down and verify cleanup;
12. write Level 1 smoke record.

### Failure triage

When the run fails, preserve:

- first failing stage;
- i2pr typed status reason;
- i2pd structured terminal event;
- sanitized counters;
- relevant RouterInfo field digests;
- no raw payload or key material.

Do not proceed to the reverse direction until either:

- the direction passes; or
- a precise protocol/runtime defect is identified and a bounded correction is committed.

## Phase 5: i2pd-to-i2pr execution

### Roles

```text
i2pr = listener/responder
i2pd = dialer/initiator
```

### Sequence

1. create new fresh run root and identities;
2. start i2pr listener and export RouterInfo;
3. strict-validate i2pr RouterInfo;
4. render i2pd strict dial config with exact peer identity/address/key/IV/message ID;
5. start i2pd dialer;
6. require both sides NTCP2-authenticated;
7. i2pd constructs `CreateDeliveryStatusMsg(message_id)` and calls `Transports::SendMessage` exactly once;
8. require the sender observer to report successful frame write;
9. require i2pr to authenticate/decrypt and decode the exact DeliveryStatus;
10. verify peer Router Hash continuity;
11. shut down and verify cleanup;
12. write Level 1 smoke record.

### Asynchronous semantics

The driver must not treat the immediate return from `SendMessage` as proof of connection or delivery. Success requires later structured events from the established session and successful frame write.

## Phase 6: bounded correction loop

Real protocol failures may require production or test changes. Use this ownership guide:

| Failure location | Likely owner |
| --- | --- |
| RouterInfo serialization/signature/address mismatch | i2pr proto/storage or reference driver config |
| TCP connection refusal/listener mismatch | loopback runner or runtime binding |
| SessionRequest construction/obfuscation/KDF | `i2pr-transport-ntcp2` initiator |
| SessionCreated parse/authentication | corresponding initiator/responder state |
| SessionConfirmed RouterInfo/signature/padding | handshake codec/state |
| first frame length obfuscation/AEAD | data frame state |
| block framing/I2NP conversion | NTCP2 block or I2NP codec |
| message ID mismatch | launcher/correlation wiring |
| duplicate/stale message | runner fresh state or receiver logic |
| cleanup residual | runtime/runner ownership |

Correction rules:

- add one focused regression reproducing the observed defect;
- change only the owning surface;
- rerun local unit/fuzz/vector checks appropriate to that surface;
- rerun the failed live direction from fresh state;
- do not expand evidence schemas to work around a protocol failure;
- do not normalize or ignore reference behavior without specification support.

## Phase 7: control build comparison

After both instrumented directions pass, run a bounded control check with the uninstrumented binary.

The control build must establish NTCP2 and exchange the message, but because its passive observer is disabled, the runner may use i2pr-side exact evidence plus process/transport outcome only for behavior-neutrality comparison. It may not produce an authoritative Level 1 reference-receiver pass record without the receiver observer.

Required comparison:

- same connection/handshake outcome;
- same sender success/failure outcome;
- no observer-dependent timing or retry behavior;
- no instrumented-only protocol success;
- clean shutdown in both builds.

If the instrumented build passes and control build fails at protocol level, treat the observer as behavior-changing and block closure.

## Phase 8: receipts and documentation

Update:

```text
tests/integration/ntcp2/qualification/i2pd-direct-driver.json
```

Do not mark it Level 3 qualified. Add or create a separate development receipt, recommended:

```text
tests/integration/ntcp2/qualification/i2pd-loopback-smoke.json
```

The development receipt records:

- binary and manifest digests;
- source revision;
- inspect results;
- one pass/fail record per direction;
- control comparison;
- network audit level;
- cleanup;
- current status `level1-passed`, `failed`, or `blocked`.

Update architecture/test documentation and create:

```text
plans/070-status.md
```

The status record must identify every code correction made as a result of a real failure.

## Required tests

### Build/driver tests

Retain and correct existing Plan 064 tests. Add cases for each real build/runtime defect found.

At minimum verify:

1. source revision mismatch rejected;
2. pristine tree mutation rejected;
3. patch fuzz rejected;
4. missing linked library rejected;
5. zero binary digest rejected;
6. control/instrumented manifests distinguish expected binaries;
7. inspect RouterInfo signature/network/address validation;
8. loopback address accepted only under explicit test config;
9. non-loopback target rejected by smoke lane;
10. exact NTCP2 `s`/`i` selected from the chosen address;
11. real DeliveryStatus construction requires nonzero ID;
12. `SendMessage` immediate return does not satisfy frame emitted;
13. post-write observer required for sender event;
14. post-AEAD/post-I2NP observer required for receiver event;
15. cleanup order enforced.

### Live acceptance

Required before closure:

- one passed `i2pr-to-i2pd-ipv4` smoke record;
- one passed `i2pd-to-i2pr-ipv4` smoke record;
- fresh mutable state between them;
- no external destination observed or configuration-only limitation recorded;
- control build behavior-neutrality check passed;
- both output records validate;
- no Level 3 qualification claim.

## Validation commands

Build/inspect examples:

```bash
bash tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh \
  --repo-root "$PWD" \
  --i2pd-source-dir <pinned-source> \
  --output-dir target/interop/i2pd-driver

# Exact inspect invocation must be recorded by implementation status.
```

Live examples:

```bash
bash scripts/interop/run-ntcp2-loopback-smoke.sh \
  --direction i2pr-to-i2pd-ipv4 \
  --reference-driver target/interop/i2pd-driver/i2pd_ntcp2_interop_driver_instrumented \
  --output target/interop/smoke/i2pr-to-i2pd.json

bash scripts/interop/run-ntcp2-loopback-smoke.sh \
  --direction i2pd-to-i2pr-ipv4 \
  --reference-driver target/interop/i2pd-driver/i2pd_ntcp2_interop_driver_instrumented \
  --output target/interop/smoke/i2pd-to-i2pr.json
```

Focused checks:

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
bash scripts/check-ntcp2-vectors.sh
```

Closure baseline:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

Run focused fuzz targets when a handshake/frame/parser defect is corrected. Do not run all fuzz targets for a pure build-script fix.

## Non-goals

Plan 070 does not:

- require 3/3 repetition;
- execute the full negative-control matrix;
- build or run Java;
- build or run Emissary;
- produce a candidate or release certificate;
- add CI;
- enable NTCP2 in the normal daemon;
- use public I2P;
- redesign the evidence architecture.

## Stop rules

Stop and record a typed blocker when:

- pinned i2pd cannot build without protocol-behavior modifications;
- the observer cannot remain passive and behavior-neutral;
- loopback execution attempts undeclared external networking;
- source lock or binary provenance cannot be verified;
- exact RouterInfo/message correlation cannot be measured;
- cleanup cannot own and terminate the processes;
- the required fix expands into NetDB/tunnels/SAM/I2CP/SSU2;
- the host lacks an ordinary dependency that cannot be installed or supplied in the preparation domain.

A protocol mismatch is not an environment blocker. Record it precisely and correct the owning i2pr surface.

## Closure criteria

Plan 070 closes only when:

- existing i2pd driver builds from pinned source;
- instrumented and control builds are produced and verified;
- inspect mode passes;
- one real pass exists in each direction;
- exact DeliveryStatus and Router Hash continuity are proven;
- network audit level is recorded;
- cleanup is clean;
- control comparison shows no behavior-changing observer effect;
- real defects found are covered by focused regressions;
- development receipt and `plans/070-status.md` are written;
- NTCP2 remains experimental/non-advertised;
- no release qualification is claimed.

## Small-model handoff instructions

- Attempt the unmodified build before editing anything.
- Save the first failing command and error in the status record.
- Fix one build or runtime defect at a time.
- Do not rewrite the driver wholesale.
- Do not add new schemas unless Plan 068's record cannot represent a required field.
- Run one direction at a time and preserve its earliest failure.
- Commit protocol fixes separately from driver/build fixes.
- Never mark the historical Level 3 receipt qualified from a loopback run.
