# Plan 064 i2pd direct NTCP2 driver

The Plan 064 i2pd direct driver is the test-only, source-locked i2pd
2.60.0 NTCP2 reference helper. It supersedes the partial Plan 059
helper at `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`
and explicitly eliminates the eight documented Plan 064 defects
(`D1`–`D8`).

The driver initializes the pinned i2pd context in the source-verified
order, uses the real NTCP2 transport, imports one peer RouterInfo
directly, sends one real `CreateDeliveryStatusMsg` in dial mode, and
acts as a real NTCP2 listener in listen mode. The driver includes a
compile-time-gated passive observer after successful AEAD decryption
and I2NP conversion, plus an uninstrumented control build proving the
observer does not alter transport success.

The driver is **test-only** and must never be activated by
`i2pr-daemon`. It is consumed by the harness through the bounded
Python adapter (`tests/integration/ntcp2/harness/i2pd_direct_driver.py`)
which itself composes the Plan 062 v4 trigger schema and the Plan 062
reference-event v1 schema. The driver does not replace the production
code path; it is the canonical reference-side implementation for the
two i2pd mixed-router directions (`i2pr-to-i2pd-ipv4`,
`i2pd-to-i2pr-ipv4`).

## Source lock

The driver is source-locked to:

- **i2pd 2.60.0** revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`
- driver source: `src/i2pd_ntcp2_interop_driver.cpp`
- observer seam: `src/interop_observer.h` + `src/interop_observer.cpp`
- observer patch (applied at build time): `patches/i2pd-2.60.0-interop-observer.patch`
- build contract: `CMakeLists.txt` + `build-driver.sh` + `build-manifest.schema.json`
- runnable seam: `run-driver.sh`
- provenance record: `source-lock.json`

The `source-lock.json` record lists the pinned revision, the linked
library digests, the observer patch digest, the helper source and
binary digests, the documented call graph, and the locked constraints
required by Plan 064 WP1–WP11. The `build-manifest.schema.json` schema
requires measured digests for the instrumented and uninstrumented
driver binaries, the Boost/OpenSSL runtime libraries, the observer
patch, the helper source, and the GCC version.

## D1–D8 defects eliminated

The Plan 059 partial helper carried eight defects that Plan 064
explicitly eliminates:

- **D1** — Router Hash treated as 40-hex SHA-1. Plan 064 uses 32-byte
  SHA-256 (`IdentHash`) encoded as 64 lowercase hex characters. The
  Plan 062 v4 trigger schema rejects any 40-hex value.
- **D2** — Wrong transport static key hashed. Plan 064 selects the
  exact NTCP2 `RouterAddress` used for the target endpoint and hashes
  its `s` field via the i2pd NTCP2 accessor; the SSU2 accessor cannot
  satisfy the validation.
- **D3** — Incomplete initialization. Plan 064 performs the source-
  verified i2pd initialization sequence
  (`config::Init` → `context::ParseConfig` → `fs::SetAppDir` →
  `crypto::Init` → `context::Init` → transport singleton → `netdb.Start`
  → `transports.Start` → `context.Start`); shutdown is the strict
  reverse order.
- **D4** — Null message trigger. Plan 064 dial mode constructs a real
  `CreateDeliveryStatusMsg(delivery_status_message_id)` (or pinned
  equivalent) and submits it through
  `Transports::SendMessage(ident_hash, msg_ptr)` exactly once.
- **D5** — Incorrect asynchronous future interpretation. Plan 064 waits
  boundedly for the established `TransportSession` state via the
  pinned i2pd callback/observation surface; an initial null session
  is not classified as failure until the bounded deadline elapses,
  and a returned future alone never proves frame transfer.
- **D6** — Reserved synthetic endpoint rejection. Plan 064 disables
  reserved-range checking through the rendered i2pd configuration
  before the connect attempt while retaining every other target
  validation.
- **D7** — No exact receiver correlation. The Plan 064 passive
  observer is placed immediately after `HandleData()` completes AEAD
  verification, validates block bounds, and converts the NTCP2
  short-header representation into a valid `I2NPMessage`. The observer
  reports the exact decoded DeliveryStatus `message_id` and the peer
  Router Hash.
- **D8** — Placeholder provenance. Plan 064 binds every source, patch,
  compiler, library, and binary input with measured SHA-256 digests.
  All-zero or placeholder digests fail closed.

## Call graph

The driver follows the source-locked i2pd lifecycle documented in
`tests/integration/ntcp2/reference-drivers/source-verification.md`:

```text
main(argc, argv)
  -> i2p::config::Init()
  -> i2p::context::ParseConfig(rendered_i2pd_conf)
       (datadir, netid = 99, NTCP2 enabled, SSU2 disabled,
        NAT/UPnP disabled, reseed disabled, reserved-range check disabled)
  -> i2p::fs::SetAppDir(data_dir)
  -> i2p::crypto::Init()
  -> i2p::context::Init()
  -> i2p::transport::transports = std::make_shared<Transports>(...)
  -> i2p::data::netdb.Start()
  -> i2p::transport::transports->Start(true, false)   // NTCP2 only
  -> i2p::context.Start()                              // if required by DeliveryStatus dispatch
  -> listen mode:  bind NTCP2 listener, install passive observer,
                   wait for one expected peer DeliveryStatus
       |  or
  -> dial mode:    parse + verify + import peer RouterInfo,
                   CreateDeliveryStatusMsg(message_id),
                   Transports::SendMessage(ident_hash, msg_ptr) exactly once,
                   wait for sender observer completion
  -> shutdown in strict reverse ownership:
       transports->Stop, context.Stop, netdb.Stop, context.~, crypto::Terminate
```

The driver never reaches inside the NTCP2 transport state, never
bypasses authentication, never invents success markers, and never
relies on a generic log phrase.

## Required driver interface

```bash
i2pd-ntcp2-interop-driver <listen|dial|inspect> --config <strict-driver-config.json>
```

The driver supports three modes:

- `inspect` — validate the strict config, emit `process_started` +
  `terminal_clean`, exit. No transport is started and no socket is
  opened.
- `listen` — initialize the pinned runtime, wait for the local
  RouterInfo, wait for one inbound DeliveryStatus on the post-AEAD
  observer seam, emit the full Plan 062 event sequence, and shut down
  cleanly.
- `dial` — initialize the pinned runtime, import one peer RouterInfo
  into the NetDB, submit a real DeliveryStatus through
  `Transports::SendMessage`, emit `process_started`, `listener_ready`,
  `router_info_exported`, `peer_router_info_validated`,
  `tcp_connected`, `ntcp2_authenticated`, `frame_emitted`, and
  `terminal_clean`, then shut down cleanly.

Unknown modes and unknown JSON fields fail closed. Unknown CLI options
fail closed. The driver writes structured events as NDJSON to
`<output_dir>/events.ndjson`. Every event matches the Plan 062
reference-event v1 schema and the Plan 062 v4 trigger record digest
binds the driver binary digest, the observer patch digest, and the
linked library manifest digest.

## Passive observer

The receive observer is guarded by the `I2PD_INTEROP_OBSERVER` macro
and is placed immediately after `HandleData()` completes:

- encrypted frame length accepted;
- AEAD verification succeeded;
- block bounds validated;
- block type is I2NP;
- NTCP2 short-header representation was converted to a valid
  `I2NPMessage`.

The sender observer is placed in the successful branch of
`HandleI2NPMsgsSent()` (or pinned equivalent), with access to the
message vector before destruction. The observer records the exact
DeliveryStatus `message_id`, the peer Router Hash, the frame sequence,
the I2NP type, and the bytes transferred.

The observer API is `noexcept` (or catches all internal exceptions),
writes only to an owned bounded sink, and drops the observation with a
typed local counter if the sink is unavailable rather than changing
transport behaviour. The observer exposes only allowlisted metadata:

```text
peer_router_hash_sha256
transport = ntcp2
direction
frame_sequence
i2np_type
i2np_envelope_message_id
delivery_status_message_id
bytes_transferred
monotonic_ms
```

The observer never logs raw payload bytes, private keys, Noise state,
frame keys, IV state, or transcripts.

## Behavior neutrality

`build-driver.sh` builds two binaries from the exact pinned tree:

1. instrumented binary with `I2PD_INTEROP_OBSERVER` defined;
2. uninstrumented control binary without the macro.

The instrumented binary differs from the uninstrumented binary only
by the observer output and the observer source digest. Any protocol
outcome difference (connection count, frame transfer result, terminal
result, cleanup result, externally visible RouterInfo) blocks
qualification.

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
- applies exactly one reviewed observer patch with `--fuzz=0`
  equivalent behaviour;
- verifies the patched tree digest;
- builds the instrumented driver;
- restores/recreates the pristine tree;
- builds the uninstrumented control driver;
- emits two build manifests;
- performs no network fetch in offline build mode.

The build manifest follows the `i2pr-i2pd-direct-driver-build-manifest-v1`
schema in `build-manifest.schema.json`. The manifest binds the pinned
i2pd source tree SHA-256, the observer patch SHA-256, the helper
source SHA-256, both binary SHA-256 digests, the linked library
manifest SHA-256, and the compiler version. No zero or placeholder
digest is allowed in an attempted run.

## Run

```bash
bash tests/integration/ntcp2/reference-drivers/i2pd/run-driver.sh \
    --driver-binary <instrumented-binary> \
    --strict-config <strict-driver-config.json>
```

The run script requires the compiled driver binary and the strict
config. The driver exits nonzero on every rejected or blocked outcome.

## Constraints

- exactly one outbound dial or inbound listener per invocation (one-shot
  contract);
- no retries, no sleeps-as-state-readiness, no DNS, no public network
  egress;
- the driver may not bypass authentication, inject success, or modify
  transport behaviour;
- the driver is a test executable — production code never depends on
  it;
- the `source-lock.json` provenance is mandatory before the driver
  may run; missing or zero digests fail the Plan 064 B1 control.

## Plan 064 controls

The driver is qualified through the controls enumerated in Plan 064
WP1–WP11. The Plan 064 test matrix
(`tests/integration/ntcp2/harness/test_i2pd_direct_driver.py` and
`test_i2pd_direct_control.py`) covers the source-verification
contract, the strict config contract, the Python harness adapter, the
structured event contract, the i2pd-to-i2pd sealed control, and the
typed host blocker. The 10/10 fresh-state qualification in both
`listen` and `dial` modes requires the authorized Ubuntu 24.04 amd64
host or Multipass guest (the current host is the Plan 046
`apparmor_restrict_on` negative baseline).

The qualification receipt lives at
`tests/integration/ntcp2/qualification/i2pd-direct-driver.json` and
records the typed host blocker plus the locked measured digests. The
receipt is a typed absence on this host; it does not advance any
support row and NTCP2 remains experimental and non-advertised.

## Migration from the Plan 059 partial helper

The Plan 059 partial helper at
`tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`
remains as a thin compatibility wrapper that re-exports its surface
through the Plan 064 driver. The new canonical artifact set lives
under `tests/integration/ntcp2/reference-drivers/i2pd/`. The Plan 064
driver is the only allowlisted NTCP2 driver for the canonical
mixed-router lane in Plan 065.