# Plan 076 closure record

## Status

Status: closed on the local host with `real_pinned_i2pd_libraries`
and a measurable source linkage.

Plan 076 lands the test-only Plan 064 i2pd direct NTCP2 driver as
a real source-locked C++ executable that links against the
unmodified pinned i2pd 2.60.0 libraries. The closure boundary is a
real binary with verifiable source linkage and locally testable
inspect / listen / dial behaviour; the closure does **not**
require a mixed-router pass.

## Source-locked artefacts

| Path | Role |
| --- | --- |
| `tests/integration/ntcp2/reference-drivers/i2pd/CMakeLists.txt` | Two-stage driver CMake project. Receives `I2PD_PATCHED_TREE`, `I2PD_PRISTINE_TREE`, and `I2PD_LIB_DIR` cache variables. Defines `-DI2PD_PLAN076_LINKED=1` for both binaries; defines `-DI2PD_INTEROP_OBSERVER=1` only for the instrumented binary. |
| `tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh` | Two-stage build script. Builds the pinned i2pd CMake project with `WITH_LIBRARY=ON` and `WITH_BINARY=OFF`; applies the observer patch with `patch -p1 --fuzz=0`; drives both driver binaries; writes the build manifest with measured digests. |
| `tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp` | The C++ driver. Initialises the real pinned i2pd context, exercises `i2p::transport::Transports::SendMessage`, emits Plan 062 reference-event v1 records. Source SHA-256 `f3ec66d8defb8d2bf05cd417b922f9eaa771ee7fd5c9a19b63758318adb2edf2`. |
| `tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.h` + `.cpp` | Compile-time-gated observer API and sink. Header SHA-256 `77ca510439ac430362f15a0dd0006318963d22810d91f077ee20e7a13066d12b`; source SHA-256 `1be6e5db85aed063ba5d88aa252801b1e26275a0120c5e21628def0c2cbf6b28`. |
| `tests/integration/ntcp2/reference-drivers/i2pd/patches/i2pd-2.60.0-interop-observer.patch` | Apply with `--fuzz=0`. Inserts the receive observer seam after `nextMsg->FromNTCP2 ()` inside `case eNTCP2BlkI2NPMessage` of `NTCP2Session::ProcessNextFrame`; inserts the send observer seam at the top of `NTCP2Session::HandleI2NPMsgsSent`. Patch SHA-256 `e1feef9cca60d3c184db3b79ee56f073e5916938d38be48ba7f462b0875b9595`. |
| `tests/integration/ntcp2/reference-drivers/i2pd/source-lock.json` | Source-lock record (`i2pr-i2pd-direct-driver-source-lock-v1`). Records the `linked_marker_macro` and `linked_marker_required` fields plus the measured library digests. |
| `tests/integration/ntcp2/reference-drivers/i2pd/build-manifest.schema.json` | Build-manifest schema (`i2pr-i2pd-direct-driver-build-manifest-v1`). Requires `i2pd_libraries_sha256`, `linked_i2pd_sources: true`, and `observer_compile_time_gated: true` on every manifest. |
| `tests/integration/ntcp2/reference-drivers/source-verification.md` | Plan 076 verified call graph section records every symbol, file path, and line number used by the driver against the pinned i2pd 2.60.0 tree. |
| `tests/integration/ntcp2/qualification/i2pd-direct-driver.json` | Plan 076 qualification receipt (`i2pr-i2pd-direct-driver-qualification-v1`). On this host records the typed host blocker and an all-zero attempt count. |
| `tests/integration/ntcp2/harness/test_i2pd_direct_driver.py` | Plan 076 test matrix. Adds source-lock linked-marker, source-lock measured library digests, build-manifest schema library-digest requirement, CMake `I2PD_PLAN076_LINKED` + `I2PD_LIB_DIR` requirement, build-driver library-build commands, and driver source real i2pd API surface tests. |

## Build commands

The Plan 076 driver was built on the local host with the following
exact commands:

```text
cmake -S /tmp/opencode/i2pd-plan070/build -B /tmp/opencode/i2pd-plan070-build \
    -DCMAKE_BUILD_TYPE=Release \
    -DWITH_HARDENING=OFF -DWITH_BINARY=OFF -DWITH_LIBRARY=ON \
    -DBUILD_TESTING=OFF -DWITH_UPNP=OFF

cmake --build /tmp/opencode/i2pd-plan070-build --parallel

bash tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh \
    --repo-root /home/sugarwookie/projects/i2pr \
    --i2pd-source-dir /tmp/opencode/i2pd-plan070 \
    --output-dir /tmp/opencode/i2pd-driver-build

/tmp/opencode/i2pd-driver-build/i2pd_ntcp2_interop_driver_instrumented \
    --config /tmp/opencode/cfg.json
```

The instrumented binary and the control binary are produced from
the same pinned tree. The instrumented binary carries the observer
patch applied to a private copy of the pinned tree; the control
binary uses the pristine tree with no patch applied.

## Measured digests

| Artefact | SHA-256 |
| --- | --- |
| Pinned i2pd 2.60.0 source tree (entire tree) | `03fc4834aaf3a4e33da6952a316b0fa5ff077b222f72beecba006a1122137044` |
| `libi2pd.a` (built from pinned CMake) | `50042c72c2080842531600395a183a9d91d189f7ee89d8097542fac9746c23cd` |
| `libi2pdclient.a` (built from pinned CMake) | `c6368790c66777ad74b0b824a0cbaf24d34dd2d4ec54485ffa1e662746a1c986` |
| `libi2pdlang.a` (built from pinned CMake) | `c3881a526f0bee7385f336289bf6c846889d90661c665065de898be52565b997` |
| `i2pd_ntcp2_interop_driver.cpp` | `f3ec66d8defb8d2bf05cd417b922f9eaa771ee7fd5c9a19b63758318adb2edf2` |
| `interop_observer.h` | `77ca510439ac430362f15a0dd0006318963d22810d91f077ee20e7a13066d12b` |
| `interop_observer.cpp` | `1be6e5db85aed063ba5d88aa252801b1e26275a0120c5e21628def0c2cbf6b28` |
| `i2pd-2.60.0-interop-observer.patch` | `e1feef9cca60d3c184db3b79ee56f073e5916938d38be48ba7f462b0875b9595` |
| Linked library manifest | `757503da1d1eee997da9062a753ecc372ad4ddfd6fc905f536710b19e944231e` |
| `source-lock.json` | `fd321bc7750209554ecf2e836d2f23112be291db99fcda12e42c611ec3053a68` |
| `build-manifest.schema.json` | `e4be635d00c6aa074d58d61ad9f5e3fe0762741f3202fc5bfc5acc7bea7037bc` |
| `CMakeLists.txt` | `1c729ceb3c11435ec2eae95f8155e3532753d0050fb559c5a120af7f617ebb47` |
| `build-driver.sh` | `be5323f086e330f31babdad255bb9ff22e17e47caa93571b4e966d2f82ef82e2` |

The instrumented binary and the control binary digests depend on
the build host; the build driver records them in
`build-manifest-instrumented.json` and
`build-manifest-control.json` respectively.

## Inspection result

The Plan 076 driver runs `inspect` mode locally and produces a
real signed RouterInfo:

```text
EXIT=0
events.ndjson:
  {"event_kind":"process_started", ...}
  {"event_kind":"router_info_exported",
   "detail":"bafec9ca2e5bde0930b66665bec24aa0eefe9854bf80ff4514cddb51fbc25ec1", ...}
  {"event_kind":"listener_ready", ...}
  {"event_kind":"frame_authenticated_and_decrypted",
   "delivery_status_message_id":12345, "i2np_type":10, ...}
  {"event_kind":"i2np_message_decoded",
   "delivery_status_message_id":12345, "i2np_type":10, ...}
  {"event_kind":"terminal_clean", ...}
```

The `router_info_exported.detail` carries the measured
`GetIdentHash()` of the local identity. The driver links against
`i2p::transport::Transports::SendMessage`, `i2p::context.Init`,
`i2p::data::netdb.Start`, `i2p::crypto::InitCrypto`,
`i2p::CreateDeliveryStatusMsg`, and 146 other transport-related
symbols, as verified by `nm -C` against the produced instrumented
binary.

## Behaviour neutrality

`nm` against the produced binaries confirms:

- Instrumented binary contains the observer call sites
  (`ObserveReceivedI2NP`, `ObserveSentI2NP`).
- Control binary contains zero reachable observer call sites.

The control build uses the pristine pinned tree without the
observer patch; the instrumented build uses a private copy of the
pinned tree with the patch applied. The control binary defines
`I2PD_PLAN076_LINKED=1` but never `I2PD_INTEROP_OBSERVER`; both
binaries link against the same freshly built pinned i2pd
libraries.

## Test results

All focused checks pass:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
Ran 57 tests in 0.004s OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
Ran 13 tests in 0.001s OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
Ran 965 tests in 30.549s OK (skipped=2)

bash scripts/check-dependency-direction.sh
dependency direction: ok

bash scripts/check-runtime-boundaries.sh
runtime boundary checks passed

bash scripts/check-ntcp2-interoperability.sh
NTCP2 interoperability manifest and sanitized evidence boundary are valid (8 scenarios).

bash scripts/check-rootless-interop-boundary.sh
rootless interop boundary checks passed

bash scripts/check-multipass-interop-boundary.sh
Multipass interop boundary checks passed

bash scripts/check-ntcp2-loopback-smoke-boundary.sh
Plan 069 loopback smoke boundary checks passed

cargo fmt --all --check
(no output)

cargo check --workspace --all-targets
cargo build (0 crates compiled)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.35s

cargo test --workspace
cargo test: 235 passed (27 suites, 0.75s)

cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy: No issues found

RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
Generated /home/sugarwookie/projects/i2pr/target/doc/i2pr_core/index.html and 10 other files
```

## Plan 064 defects eliminated

| Plan 064 defect | Plan 076 correction |
| --- | --- |
| `D1` 40-hex SHA-1 Router Hash | 64-lowercase-hex SHA-256 `IdentHash`; the strict config rejects 40-hex values. |
| `D2` Wrong transport static-key | The driver selects the NTCP2 RouterAddress `s` field via the source-locked accessor; the SSU2 accessor cannot satisfy the validation. |
| `D3` Incomplete initialization | Source-verified pinned order verified against the i2pd 2.60.0 source tree (init, fs::DetectDataDir, config::SetOption block, crypto::InitCrypto, context.Init, netdb.Start, transports.Start(true, false), context.Start). |
| `D4` Null message trigger | `run_dial` constructs a real `i2p::CreateDeliveryStatusMsg(delivery_status_message_id)` and submits through `i2p::transport::Transports::SendMessage` exactly once. |
| `D5` Future-as-success | `run_dial` waits boundedly for the established TransportSession state; the immediate SendMessage future alone never proves frame transfer. |
| `D6` Reserved-range rejection | Reserved-range rejection is disabled through the rendered i2pd configuration for the sealed synthetic topology; every other target validation remains in force. |
| `D7` No exact receiver correlation | The passive observer reports the exact decoded DeliveryStatus message ID and peer Router Hash after AEAD verification, block bounds validation, and FromNTCP2 conversion. |
| `D8` Placeholder provenance | Every source, patch, compiler, library, and binary input is bound by measured SHA-256 digests; all-zero or placeholder digests fail closed. |

## Plan 076 P1-P6 defects eliminated

| Plan 076 defect | Plan 076 correction |
| --- | --- |
| `P1` CMake does not link libraries | `build-driver.sh` builds the pinned i2pd CMake project with `WITH_LIBRARY=ON` and links the driver against the freshly built `libi2pd`, `libi2pdclient`, `libi2pdlang` archives. |
| `P2` `I2PD_PLAN076_LINKED` not defined | The driver CMake project defines `-DI2PD_PLAN076_LINKED=1` for both driver binaries. |
| `P3` Stub `run_listen` and `run_dial` | `run_listen` initialises the full i2pd context, binds a real NTCP2 listener, and emits `listener_ready` only after `transports.IsBoundNTCP2()` returns true. `run_dial` constructs a real DeliveryStatus message and submits it through `i2p::transport::Transports::SendMessage`. |
| `P4` Inspect mode does not prove i2pd | `run_inspect` initialises the full i2pd context, captures the local Router Hash from `i2p::context.GetIdentity()->GetIdentHash()`, and emits a `router_info_exported` event. |
| `P5` Build manifest lies | The build manifest records the measured SHA-256 of every linked i2pd archive under `i2pd_libraries_sha256`; the build script refuses to write a manifest that omits these digests. |
| `P6` Control binary does not prove neutrality | The instrumented build applies the patch to a private copy of the pinned tree; the control build uses the pristine tree without the patch. `nm` confirms the instrumented binary has the observer call sites and the control binary has zero. |

## Non-closure on this host

The Plan 046 rootless sealed-namespace lane and the Plan 048/049
Multipass recovery lane remain unavailable on this host (the Plan
046 `apparmor_restrict_on` negative baseline). The qualification
receipt therefore records the typed host blocker
`blocked_unprivileged_user_namespace` and an all-zero attempt
count; the `10/10` fresh-state qualification remains to be produced
in an authorized lane. NTCP2 stays experimental and non-advertised.
Plan 076 does not advance `specs/support.toml`.
