# Plan 063 Java I2P stripped-router direct NTCP2 driver

The Plan 063 Java direct driver is the test-only, source-locked Java
I2P 2.12.0 NTCP2 reference helper. It uses the upstream embedded
`net.i2p.router.Router` and `net.i2p.router.RouterContext` with the
pinned dummy facades, the real NTCP/NTCP2 transport implementation,
the real outbound message pool, and the real inbound message pool.
The driver does **not** patch NTCP2 cryptography, the Noise
handshake, framing, or RouterInfo signature verification.

The driver is **test-only** and must never be activated by
`i2pr-daemon`. It is consumed by the harness through the bounded
Python adapter (`tests/integration/ntcp2/harness/java_direct_driver.py`)
which itself composes the Plan 062 v4 trigger schema and the Plan
062 reference-event v1 schema. The driver does not replace the
production code path; it is the canonical reference-side
implementation for the four primary IPv4 mixed-router directions.

## Source lock

The driver is source-locked to:

- **Java I2P 2.12.0** revision `2800040deee9bb376567b671ef2e9c34cf3e30b6`
- driver source: `src/JavaNtcp2InteropDriver.java`
- build contract: `build-driver.sh` + `classpath-manifest.json` + `build-manifest.schema.json`
- runnable seam: `run-driver.sh`
- provenance record: `source-lock.json`

The `source-lock.json` record lists the pinned revision, the required
build inputs, the documented call graph, and the locked constraints
required by Plan 063 WP1-WP10. The `classpath-manifest.json` record
binds every pinned jar in `target/interop/cache/java_i2p/<tree>/lib/`
to its purpose; no Maven Central dependency may be introduced.

## Call graph

The driver follows the upstream `SSUDemo`-style stripped-router
pattern documented in `tests/integration/ntcp2/reference-drivers/source-verification.md`:

```text
net.i2p.router.Router.runRouter() (embedded, on a dedicated I2PThread)
  -> RouterContext.router(), .netDb(), .inNetMessagePool(), .outNetMessagePool()
  -> net.i2p.router.dummy.DummyNetworkDatabaseFacade.store(Hash, RouterInfo)
  -> net.i2p.router.OutNetMessage + outNetMessagePool.add()
     -> net.i2p.router.transport.ntcp.NTCPTransport.send(OutNetMessage)
  -> net.i2p.router.HandlerJobBuilder registered for DeliveryStatusMessage
     -> I2NPMessage (DeliveryStatusMessage.MESSAGE_TYPE == 10) at the
        receiver after NTCP2 AEAD verification and I2NP conversion
```

The driver never reaches inside the NTCP2 transport state, never
bypasses authentication, never invents success markers, and never
relies on a generic log phrase.

## Required driver interface

```bash
java ... i2pr.ntcp2.JavaNtcp2InteropDriver <listen|dial|inspect> \
    --config <strict-driver-config.json>
```

The driver supports three modes:

- `inspect` — validate the strict config, emit
  `process_started` + `terminal_clean`, exit. No router process is
  started and no socket is opened.
- `listen` — construct the embedded router, wait for the local
  RouterInfo, wait for one inbound DeliveryStatus on the I2NP
  inbound handler, emit the full Plan 062 event sequence, and shut
  down cleanly.
- `dial` — construct the embedded router, import one peer RouterInfo
  into the dummy NetDB, submit a real DeliveryStatus through
  `OutNetMessagePool`, emit `process_started`, `listener_ready`,
  `router_info_exported`, `peer_router_info_validated`,
  `tcp_connected`, `ntcp2_authenticated`, `frame_emitted`, and
  `terminal_clean`, then shut down cleanly.

Unknown modes and unknown JSON fields fail closed.

The driver writes structured events as NDJSON to
`<output_dir>/events.ndjson`. Every event matches the Plan 062
reference-event v1 schema and the Plan 062 v4 trigger record
digest binds the driver binary digest.

## Build

```bash
bash tests/integration/ntcp2/reference-drivers/java/build-driver.sh \
    --repo-root <repo-root> \
    --java-cache <pinned-java-cache-dir> \
    --output-dir <owned-output-dir>
```

The build script:

- requires the exact pinned source/build cache;
- requires JDK 17 or the exact repository-declared compatible JDK;
- uses a deterministic sorted source list;
- uses an explicit classpath;
- emits no download;
- builds into an owned output directory;
- produces a build manifest before the binary can run.

The build manifest follows the `i2pr-java-helper-build-manifest-v1`
schema in `build-manifest.schema.json`. The manifest binds the
pinned jars' SHA-256 digests, the driver source digest, the driver
binary digest, the classpath manifest digest, and the JDK versions.

## Run

```bash
bash tests/integration/ntcp2/reference-drivers/java/run-driver.sh \
    --driver-jar <driver.jar> \
    --java-cache <pinned-java-cache-dir> \
    --mode <listen|dial|inspect> \
    --config <strict-driver-config.json> \
    [--output-dir <owned-output-dir>]
```

The run script requires the exact pinned Java cache directory and the
compiled driver jar. The driver exits nonzero on every rejected or
blocked outcome.

## Constraints

- exactly one outbound dial per invocation when in dial mode (one-shot
  contract);
- no retries, no sleeps-as-state-readiness, no DNS, no public network
  egress;
- the driver may not bypass authentication, inject success, or modify
  transport behaviour;
- the driver is a test executable — production code never depends on
  it;
- the `source-lock.json` provenance is mandatory before the driver
  may run; missing digests fail the Plan 063 B1 control.

## Plan 063 controls

The driver is qualified through the twelve controls enumerated in
Plan 063 WP1-WP10. The Plan 063 test matrix
(`tests/integration/ntcp2/harness/test_java_direct_driver.py`) covers
the source-verification contract, the strict config contract, the
Python harness adapter, the structured event contract, and a local
inspect-mode round-trip where the pinned Java cache is available.
The 10/10 fresh-state qualification in both `listen` and `dial`
modes requires the authorized Ubuntu 24.04 amd64 host or Multipass
guest (the current host is the Plan 046 `apparmor_restrict_on`
negative baseline).

The qualification receipt lives at
`tests/integration/ntcp2/qualification/java-direct-driver.json` and
records the typed host blocker plus the locked measured digests. The
receipt is a typed absence on this host; it does not advance any
support row and NTCP2 remains experimental and non-advertised.
