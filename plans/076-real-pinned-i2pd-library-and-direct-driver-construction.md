# Plan 076: real pinned i2pd library and direct driver construction

## Status and dependencies

- Status: planned.
- Parent roadmap: Plan 074.
- Requires Plan 075 closed.
- Must close before Plan 077 execution-lane qualification and Plan 078 live execution.
- Plan type: test-only reference-driver implementation correction.

## Objective

Replace the Plan 064 terminal-stub helper with a real source-locked i2pd 2.60.0 test executable that initializes genuine i2pd transport code, exports and imports signed RouterInfo, listens or dials through NTCP2, exchanges one real DeliveryStatus I2NP message, emits passive structured events, and shuts down cleanly.

This plan deliberately has no mixed-router pass requirement. Its closure boundary is a real executable with verifiable source linkage and locally testable inspect/control behavior.

## Existing defects

The implementation model must verify and correct all of these before claiming closure:

1. The helper CMake project includes i2pd headers but does not compile or link the actual pinned i2pd library targets.
2. `I2PD_PLAN064_LINKED` is not established by the build.
3. `run_listen()` and `run_dial()` are terminal rejection stubs.
4. Inspect mode does not prove real i2pd initialization or RouterInfo production.
5. Build manifests describe linked i2pd behavior that the current binary does not contain.
6. A control binary that omits observer calls is not sufficient unless both binaries execute the same genuine transport path.

## Source-lock boundary

Use the existing lock:

```text
i2pd 2.60.0
revision f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
```

Before implementation:

- verify exact revision/tree/archive digests;
- inspect the pinned CMake targets and supported library-build options;
- inspect actual initialization and shutdown APIs;
- inspect direct RouterInfo insertion and DeliveryStatus construction APIs;
- inspect NTCP2 transport start/listen/dial paths;
- update `source-verification.md` with exact pinned file paths and symbols.

Current upstream examples are guidance only; pinned source wins.

## Target build architecture

Preferred order:

1. Build pinned i2pd using its own CMake project with the library option supported by that pinned revision, commonly `WITH_LIBRARY=ON` or its exact pinned equivalent.
2. Add the test driver as a target inside a private copied/patched build tree, or link it against real exported/static i2pd library targets produced by that build.
3. Keep the committed driver and observer sources in i2pr; do not add a production dependency.
4. Build instrumented and uninstrumented control binaries from the same pinned source and flags.
5. Record all linked i2pd/Boost/OpenSSL/runtime artifacts and digests.

Do not continue using a standalone CMake executable that merely sees headers.

## Required real call graph

### Initialization

The driver must execute the source-verified pinned order for:

- filesystem/data directory setup;
- crypto context;
- router context and identity;
- NetDB or minimal direct peer store required by transport APIs;
- transports and NTCP2 server/session objects;
- test observer registration when instrumented.

Disable or avoid through supported test configuration:

- reseed;
- public peer discovery;
- SSU2/UDP;
- UPnP;
- tunnels;
- SAM/I2CP/client services;
- web console/control services;
- floodfill and transit participation;
- public RouterInfo publication.

### Inspect mode

Inspect mode must initialize genuine i2pd state and prove:

- real context initialized;
- fresh signed RouterInfo created;
- RouterInfo signature verifies;
- network ID is 99;
- selected address is NTCP2 IPv4;
- configured host/port and NTCP2 `s`/`i` fields match;
- Router Hash is identity-derived;
- no public bootstrap starts;
- shutdown is clean.

A mode that only emits `process_started` and `terminal_clean` is insufficient.

### Listen mode

Listen mode must:

- initialize real NTCP2 transport;
- bind the configured endpoint supplied by the execution lane;
- export RouterInfo after the actual listener address is final;
- emit `listener_ready` only after bind/listen succeeds;
- accept exactly one peer for the bounded run;
- emit authentication and receive events only after real protocol operations;
- decode and correlate the exact DeliveryStatus;
- stop all owned singleton/thread/event-loop state.

### Dial mode

Dial mode must:

- validate and import the exact peer RouterInfo;
- select its NTCP2 address;
- initiate exactly one bounded connection;
- construct `CreateDeliveryStatusMsg(message_id)` or the exact pinned equivalent;
- submit through the real transport send path;
- not treat immediate queue/future return as frame-write proof;
- emit sender success only after the real asynchronous write boundary;
- shut down cleanly.

## Observer constraints

The instrumented build may observe only:

- post-authenticated NTCP2 session establishment;
- successful encrypted frame write completion;
- post-AEAD frame authentication/decryption;
- post-NTCP2 block conversion into I2NP;
- exact DeliveryStatus decode and peer identity.

The patch must not alter:

- cryptographic input/output;
- transcript state;
- frame boundaries;
- buffering;
- retry or timer decisions;
- error propagation;
- route or peer selection;
- return values.

The control build must contain no reachable observer call sites. Instrumented and control protocol behavior must be equivalent.

## Build deliverables

Retain or replace narrowly:

```text
tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh
tests/integration/ntcp2/reference-drivers/i2pd/CMakeLists.txt
tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp
tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.*
tests/integration/ntcp2/reference-drivers/i2pd/patches/*
```

Required outputs:

```text
i2pd_ntcp2_interop_driver_instrumented
i2pd_ntcp2_interop_driver_control
build-manifest-instrumented.json
build-manifest-control.json
linked-library-manifest.txt
inspect-instrumented.json
inspect-control.json
```

Build manifests must record:

- exact reference revision/tree digest;
- exact build options;
- compiler/CMake versions;
- driver and observer source digests;
- observer patch digest;
- executable digests;
- every linked i2pd/Boost/OpenSSL/runtime library path and digest;
- whether observer support is enabled;
- no placeholder or synthetic values.

## Work packages

### WP1. Pinned-source verification

Write exact symbol/file/API findings. Stop if the pinned revision cannot expose the required direct test path without changing protocol behavior.

### WP2. Real i2pd library build

- build the pinned project/library targets;
- validate symbols with `nm`, `readelf`, or platform-equivalent tools;
- prove driver links real i2pd code;
- reject unresolved or accidentally system-installed i2pd libraries.

### WP3. Real inspect mode

Implement genuine initialization, RouterInfo creation/validation, and shutdown. Run both binaries.

### WP4. Real listen/dial paths

Implement one-shot transport behavior and exact message submission/receipt.

### WP5. Passive observer and control comparison

Prove observer call-site placement and behavior neutrality with local unit/control tests. No external pass is required.

### WP6. Manifest, docs, and closure

Update reference-driver documentation and create `plans/076-status.md` with exact build commands and digests.

## Required tests

At minimum:

1. wrong pinned revision rejected;
2. source-tree mutation rejected;
3. system i2pd library substitution rejected;
4. no real i2pd symbols in binary rejected;
5. instrumented/control binary digest zero rejected;
6. inspect mode initializes real context;
7. inspect RouterInfo signature/network ID/address verified;
8. listener-ready cannot emit before real bind;
9. dial requires exact validated peer RouterInfo;
10. nonzero message ID required;
11. immediate send return cannot prove frame write;
12. receive event requires post-AEAD and post-I2NP boundary;
13. wrong message ID or peer hash rejected;
14. observer patch with fuzz rejected;
15. control build contains no observer call sites;
16. shutdown leaves no owned threads/processes/listeners;
17. public bootstrap/service features remain disabled.

## Acceptance criteria

Plan 076 closes only when:

- the driver executable contains and invokes real pinned i2pd code;
- stub `pinned-libraries-not-linked` listen/dial behavior is removed;
- inspect mode proves genuine i2pd context and RouterInfo behavior;
- listen and dial code paths are implemented and reachable;
- real DeliveryStatus construction and send path are present;
- passive observer events are bound to genuine protocol success boundaries;
- instrumented and control builds are produced from the same pinned revision;
- manifests contain measured provenance only;
- focused tests and local build/inspect checks pass;
- no mixed-router interoperability claim is made.

## Validation commands

The implementation status must record exact pinned build commands. Minimum local checks:

```bash
bash tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh \
  --repo-root "$PWD" \
  --i2pd-source-dir <verified-pinned-tree> \
  --output-dir target/interop/i2pd-driver

ldd target/interop/i2pd-driver/i2pd_ntcp2_interop_driver_instrumented
nm -C target/interop/i2pd-driver/i2pd_ntcp2_interop_driver_instrumented | grep -E 'Transports|NTCP2|CreateDeliveryStatus'

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
git diff --check
```

## Non-goals

Plan 076 does not:

- choose or provision Docker/QEMU/remote execution;
- run i2pr against i2pd;
- modify production i2pr NTCP2 behavior;
- add broad CI;
- patch i2pd cryptography or protocol semantics;
- close Level 1, Level 2, or Level 3 validation.

## Stop rules

Stop when:

- the pinned i2pd revision lacks a usable library/direct transport seam;
- required APIs are private and exposing them would modify protocol behavior rather than add a test-only adapter;
- a passive observer cannot be inserted without affecting semantics;
- the only available build uses a different unpinned i2pd revision.

Record the exact source/API blocker and return to architecture planning. Do not replace real i2pd with mocks.

## Small-model execution guidance

- Verify pinned APIs before editing C++.
- Make the real library link succeed before implementing listen/dial.
- Make inspect mode real before transport mode.
- Implement listener and dialer separately.
- Keep observer changes in a distinct commit.
- Never mark Plan 076 closed based only on schema tests or a binary that lacks i2pd symbols.