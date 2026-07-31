# Plan 076 i2pd direct NTCP2 driver

The Plan 076 i2pd direct driver is the test-only, source-locked
i2pd 2.60.0 NTCP2 reference helper. It supersedes the Plan 064
helper that emitted `pinned-libraries-not-linked` for every
listen/dial invocation.

The Plan 076 driver is a real source-locked C++ executable that
links against the unmodified pinned i2pd 2.60.0 libraries built
from the pinned CMake project. The driver:

1. Initialises the full pinned i2pd context in the source-verified
   order from the Plan 062 / Plan 064 source-verification record.
2. Uses the real NTCP2 transport. SSU2 is disabled.
3. Imports one peer RouterInfo directly via
   `i2p::data::netdb.AddRouterInfo`.
4. Sends one real `i2p::CreateDeliveryStatusMsg` I2NP message in
   dial mode through `i2p::transport::Transports::SendMessage`.
5. Acts as a real NTCP2 listener in listen mode.
6. Includes a compile-time-gated passive observer after successful
   AEAD decryption and I2NP conversion.
7. Builds both the instrumented and uninstrumented control binaries
   from the same pinned tree. The control binary contains zero
   reachable observer call sites.

The driver is **test-only** and must never be activated by
`i2pr-daemon`. It is consumed by the harness through the bounded
Python adapter
(`tests/integration/ntcp2/harness/i2pd_direct_driver.py`) which
composes the Plan 062 v4 trigger schema and the Plan 062
reference-event v1 schema.

## Source lock

The driver is source-locked to:

- **i2pd 2.60.0** revision
  `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`
- driver source:
  `src/i2pd_ntcp2_interop_driver.cpp`
- observer seam: `src/interop_observer.h` + `src/interop_observer.cpp`
- observer patch (applied at build time):
  `patches/i2pd-2.60.0-interop-observer.patch`
- build contract: `CMakeLists.txt` + `build-driver.sh` +
  `build-manifest.schema.json`
- runnable seam: `run-driver.sh`
- provenance record: `source-lock.json`

The pinned i2pd source tree digest is
`03fc4834aaf3a4e33da6952a316b0fa5ff077b222f72beecba006a1122137044`
and is verified by `find … -type f | sort -z | xargs -0 sha256sum`
against the pinned revision.

## Build contract

`build-driver.sh` performs two ordered stages:

1. **Stage 1 — Pinned i2pd library build.** Configure and build the
   unmodified pinned i2pd CMake project at
   `i2pd/build/CMakeLists.txt` with `WITH_LIBRARY=ON`,
   `WITH_BINARY=OFF`, `WITH_HARDENING=OFF`, `WITH_UPNP=OFF`, and
   `BUILD_TESTING=OFF`. The pinned source tree is never mutated.
   The build produces three static archives: `libi2pd.a`,
   `libi2pdclient.a`, `libi2pdlang.a`.
2. **Stage 2 — Driver build.** Copy the pinned tree to a private
   directory, copy the observer header and source into the copy,
   apply the observer patch with `patch -p1 --fuzz=0`, configure
   the Plan 076 driver CMake project against the freshly built
   pinned i2pd libraries via the `I2PD_PATCHED_TREE`,
   `I2PD_PRISTINE_TREE`, and `I2PD_LIB_DIR` cache variables, build
   the instrumented and control driver binaries, and write the
   build manifests with measured digests.

The driver CMake project fails closed when `I2PD_LIB_DIR` is not
supplied or when no `libi2pd*.a` archives are present. The driver
binary defines `-DI2PD_PLAN076_LINKED=1`; the runtime
`pinned_libraries_linked()` gate fails closed with exit 66 when the
marker is not defined.

## Verified call graph

The driver links against the following symbols (verified at build
time through `nm -C`):

| Symbol | Source file |
| --- | --- |
| `i2p::transport::Transports::Start(bool, bool)` | `libi2pd/Transports.cpp` |
| `i2p::transport::Transports::Stop()` | `libi2pd/Transports.cpp` |
| `i2p::transport::Transports::IsBoundNTCP2` | `libi2pd/Transports.h` (line 146) |
| `i2p::transport::Transports::SendMessage` | `libi2pd/Transports.h` (line 157) |
| `i2p::context.Init / Start / Stop` | `libi2pd/RouterContext.cpp` |
| `i2p::data::netdb.Start / Stop` | `libi2pd/NetDb.cpp` |
| `i2p::data::netdb.AddRouterInfo` | `libi2pd/NetDb.hpp` (line 85) |
| `i2p::data::netdb.FindRouter` | `libi2pd/NetDb.hpp` (line 89) |
| `i2p::CreateDeliveryStatusMsg` | `libi2pd/I2NPProtocol.cpp` |
| `i2p::crypto::InitCrypto` | `libi2pd/Crypto.cpp` |
| `i2p::fs::DetectDataDir` | `libi2pd/FS.cpp` |

The receive observer seam fires only after:

1. `m_Handler.AEADChaCha20Poly1305Decrypt` has returned success
   (`NTCP2Session::HandleReceived`, `NTCP2.cpp` line 1253).
2. `ProcessNextFrame` has validated block bounds and read the
   `eNTCP2BlkI2NPMessage` block header (`NTCP2.cpp` line 1322).
3. `nextMsg->FromNTCP2()` has converted the NTCP2 short-header
   representation into a valid `I2NPMessage` (`NTCP2.cpp` line
   1335).

The send observer seam fires only at the top of
`NTCP2Session::HandleI2NPMsgsSent` (`NTCP2.cpp` line 1452) when
the asynchronous socket write has completed successfully.

## D1–D8 defects eliminated

The Plan 064 driver carried eight documented defects that Plan 076
eliminates:

- **D1** — Router Hash treated as 40-hex SHA-1. Plan 076 uses
  32-byte `IdentHash` (SHA-256) encoded as 64 lowercase hex
  characters. The Plan 062 v4 trigger schema rejects any 40-hex
  value.
- **D2** — Wrong transport static-key. Plan 076 selects the exact
  NTCP2 RouterAddress `s` field via the source-verified accessor;
  the SSU2 accessor cannot satisfy the validation.
- **D3** — Incomplete initialization. Plan 076 follows the
  source-verified pinned order documented in
  `tests/integration/ntcp2/reference-drivers/source-verification.md`.
- **D4** — Null message trigger. Plan 076 dial mode constructs a
  real `i2p::CreateDeliveryStatusMsg(delivery_status_message_id)`
  and submits through `i2p::transport::Transports::SendMessage`
  exactly once.
- **D5** — Future-as-success. Plan 076 waits boundedly for the
  established TransportSession state; an immediate SendMessage
  future alone never proves frame transfer.
- **D6** — Reserved synthetic endpoint rejection. Plan 076 disables
  reserved-range checking through the rendered i2pd configuration
  while retaining every other target validation.
- **D7** — No exact receiver correlation. The Plan 076 passive
  observer reports the exact decoded DeliveryStatus message ID
  and peer Router Hash after AEAD verification, block bounds
  validation, and FromNTCP2 conversion.
- **D8** — Placeholder provenance. Plan 076 binds every source,
  patch, compiler, library, and binary input with measured SHA-256
  digests. All-zero or placeholder digests fail closed.

## Plan 076 P1-P6 defects eliminated

The Plan 076 driver corrects six additional defects of the Plan 064
implementation surface:

- **P1** — CMake project included i2pd headers but did not compile
  or link the actual pinned i2pd library targets. Plan 076 links
  the driver against the freshly built pinned i2pd libraries.
- **P2** — `I2PD_PLAN076_LINKED` was not established by the build.
  Plan 076 defines the marker through the driver CMake project.
- **P3** — `run_listen()` and `run_dial()` were terminal rejection
  stubs. Plan 076 implements both with the real i2pd NTCP2
  transport.
- **P4** — Inspect mode did not prove real i2pd initialization or
  RouterInfo production. Plan 076 inspect mode initialises the
  full i2pd context and emits a `router_info_exported` event with
  the measured `GetIdentHash()`.
- **P5** — Build manifests described linked i2pd behaviour the
  current binary did not contain. Plan 076 records the measured
  SHA-256 of every linked i2pd archive under `i2pd_libraries_sha256`.
- **P6** — A control binary that omits observer calls was not
  sufficient unless both binaries execute the same genuine
  transport path. Plan 076 builds both binaries from the same
  pinned tree via the `I2PD_PATCHED_TREE` / `I2PD_PRISTINE_TREE` /
  `I2PD_LIB_DIR` CMake cache variables.

## Behaviour neutrality

`build-driver.sh` builds two binaries from the exact pinned tree:

1. **Instrumented binary** with `I2PD_INTEROP_OBSERVER=1` and
   `I2PD_PLAN076_LINKED=1`. The observer patch is applied to a
   private copy of the pinned tree.
2. **Control binary** without `I2PD_INTEROP_OBSERVER` and with
   `I2PD_PLAN076_LINKED=1`. The pristine tree is used without the
   patch applied.

The instrumented binary differs from the uninstrumented binary
only by the observer output and the observer source digest. Any
protocol outcome difference (connection count, frame transfer
result, terminal result, cleanup result, externally visible
RouterInfo) blocks qualification. `nm -C` confirms that the
instrumented binary contains exactly two reachable observer call
sites and the control binary contains zero.

## Build

```bash
bash tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh \
    --repo-root <repo-root> \
    --i2pd-source-dir <pinned-i2pd-source-dir> \
    --output-dir <owned-output-dir>
```

The build script:

- verifies the pristine pinned source tree digest against the locked
  revision;
- configures and builds the pinned i2pd CMake project with
  `WITH_LIBRARY=ON` and `WITH_BINARY=OFF`;
- applies the observer patch to a private copy of the pinned tree
  with `patch -p1 --fuzz=0`;
- builds the instrumented driver;
- restores the pristine tree;
- builds the uninstrumented control driver;
- emits two build manifests and a linked library manifest with
  measured digests;
- performs no network fetch in offline build mode.

The build manifest follows the
`i2pr-i2pd-direct-driver-build-manifest-v1` schema in
`build-manifest.schema.json`. The manifest binds the pinned i2pd
source tree SHA-256, the linked i2pd archive SHA-256 digests, the
observer patch SHA-256, the helper source SHA-256, both binary
SHA-256 digests, the linked library manifest SHA-256, the CMake
version, and the compiler version. No zero or placeholder digest
is allowed in an attempted run.

## Run

```bash
bash tests/integration/ntcp2/reference-drivers/i2pd/run-driver.sh \
    --driver-binary <instrumented-binary> \
    --strict-config <strict-driver-config.json>
```

The run script requires the compiled driver binary and the strict
config. The driver exits nonzero on every rejected or blocked
outcome.

## Constraints

- exactly one outbound dial or inbound listener per invocation
  (one-shot contract);
- no retries, no sleeps-as-state-readiness, no DNS, no public
  network egress;
- the driver may not bypass authentication, inject success, or
  modify transport behaviour;
- the driver is a test executable — production code never depends
  on it;
- the `source-lock.json` provenance is mandatory before the driver
  may run; missing or zero digests fail the Plan 076 B1 control.

## Closure boundary

Plan 076 does not require a mixed-router pass; the closure is a
real binary with verifiable source linkage and locally testable
inspect / control behaviour. The `10/10` fresh-state qualification
remains to be produced in the Plan 046 rootless sealed-namespace
lane or the Plan 048/049 Multipass recovery lane. On this host
(the Plan 046 `apparmor_restrict_on` negative baseline) the
qualification receipt records the typed host blocker and an
all-zero attempt count. NTCP2 stays experimental and
non-advertised.

## Migration from the Plan 064 helper

The Plan 064 implementation is replaced by the Plan 076 driver.
The Plan 064 helper at
`tests/integration/ntcp2/reference-drivers/i2pd/` (the same path)
was rewritten in place. The Plan 059 helper at
`tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`
remains as a thin compatibility wrapper with the explicit Plan 064
supersedure marker. The Plan 076 driver is the only allowlisted
NTCP2 driver for the canonical mixed-router lane in Plan 065.
