# Plan 052 reference trigger contracts

This document is the source-inspection record required by Plan 052 F2. It
records, for each pinned reference router, the candidate source paths and
symbols that could provide a direct NTCP2 transport dial seam usable by
the Plan 045 reference-initiated directions.

The document is a working document; entries are added only after the
symbol has been confirmed against the pinned revision. Symbols that are
not present in the pinned revision, or that depend on streaming, tunnel,
or floodfill infrastructure, are explicitly rejected.

## Conventions

- **revision** — the full 40-character pinned commit SHA.
- **source path** — repository-relative path inside the pinned source tree.
- **symbol** — exact symbol or class name as defined in the source.
- **call graph** — sketch of how the symbol reaches the NTCP2 transport
  connection handler.
- **prerequisites** — NetDB, tunnel, or floodfill requirements.
- **disposition** — `selected`, `rejected`, `pending-source-inspection`.

## Java I2P 2.12.0 (`2800040deee9bb376567b671ef2e9c34cf3e30b6`)

| Field | Value |
| --- | --- |
| revision | `2800040deee9bb376567b671ef2e9c34cf3e30b6` |
| version | `2.12.0` |
| artifact SHA-256 | pinned under `tests/integration/ntcp2/references.lock.toml` |
| installed-tree SHA-256 | pinned under `tests/integration/ntcp2/references.lock.toml` |

### Candidate seams (status: pending-source-inspection)

1. `net.i2p.router.transport.ntcp.NTCP2Transport`
   - Source path: `router/java/src/net/i2p/router/transport/ntcp/NTCP2Transport.java`
   - Symbol: `establishConnection()`
   - Call graph: pulls the destination from the comm system facade,
     constructs an `NTCP2Connection` on the outbound side.
   - Prerequisites: requires a `RouterContext`, which implies NetDB
     lookup of the destination hash.
   - Disposition: rejected for transport-only test, depends on NetDB.

2. `net.i2p.router.communicator.ClientManagerFacade`
   - Source path: `router/java/src/net/i2p/router/communist/ClientManagerFacade.java`
   - Symbol: request routing entry points
   - Call graph: ClientMessage dispatch → SAM bridge.
   - Prerequisites: requires the SAM bridge + a registered destination.
   - Disposition: rejected; matches the existing SAM trigger.

3. `net.i2p.client.streaming.StreamManager`
   - Source path: `router/java/src/streaming/StreamManager.java`
   - Symbol: `connect()`
   - Call graph: streaming → outbound message queue → NTCP2 connection.
   - Prerequisites: requires a registered streaming destination and
     outbound tunnel pool.
   - Disposition: rejected; depends on streaming + tunnels.

### Required follow-up

A test-only helper compiled against the pinned router jars must:

- Source-link against the pinned revision's jars without transport
  behavior patches.
- Construct a minimal `RouterContext` bound to a per-attempt data
  directory.
- Invoke the transport facade directly to request one NTCP2 connection
  to the imported i2pr RouterInfo, bypassing the streaming manager and
  the SAM bridge.
- Emit one structured trigger record containing the requested RouterInfo
  hash, the resolved synthetic endpoint, the connection outcome, and a
  bounded log excerpt.
- Reject unknown target RouterInfos and unknown endpoints.

This implementation is gated on the Plan 052 F4 control experiments
(positive, wrong-RouterInfo, wrong-address, no-trigger, reference-code).
Until the helper exists, the Java reference-initiated direction is a
typed blocker (`java-reference-direct-trigger-not-source-locked`).

## i2pd 2.60.0 (`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`)

| Field | Value |
| --- | --- |
| revision | `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e` |
| version | `2.60.0` |
| artifact SHA-256 | pinned under `tests/integration/ntcp2/references.lock.toml` |
| installed-tree SHA-256 | pinned under `tests/integration/ntcp2/references.lock.toml` |

### Candidate seams (status: pending-source-inspection)

1. `i2pd::transports::Transports::ConnectToPeer`
   - Source path: `libi2pd/Transports.cpp`
   - Symbol: `i2pd::transports::Transports::ConnectToPeer`
   - Call graph: TransportManager → NTCP2 session handler → TCP connect.
   - Prerequisites: requires a known `RouterInfo` in the NetDB; the
     reference must import the i2pr RouterInfo before invoking.
   - Disposition: candidate — pending source inspection of the actual
     symbol presence in the pinned revision.

2. `i2pd::data::RouterInfo::Load`
   - Source path: `libi2pd/RouterInfo.cpp`
   - Symbol: `i2pd::data::RouterInfo::Load`
   - Call graph: parses a RouterInfo file from disk.
   - Prerequisites: none beyond the file itself.
   - Disposition: supporting helper for `ConnectToPeer`.

3. SAM v3 `STREAM CONNECT` (existing)
   - Source path: `daemon/SAM.cpp`
   - Prerequisites: requires a SAM bridge, a registered destination,
     outbound tunnel pool.
   - Disposition: rejected for transport-only test, depends on streaming.

### Required follow-up

A test-only executable linked against the pinned i2pd libraries must:

- Load the imported i2pr RouterInfo via `RouterInfo::Load`.
- Register it with the running transport manager.
- Invoke `Transports::ConnectToPeer(hash, callback)` exactly once.
- Emit one structured trigger record.
- Reject unknown target RouterInfos and unknown endpoints.

This implementation is gated on the Plan 052 F4 control experiments.
Until the helper exists, the i2pd reference-initiated direction is a
typed blocker (`i2pd-reference-direct-trigger-not-source-locked`).

## Decision matrix

| Reference | Direct transport seam | Decision |
| --- | --- | --- |
| Java I2P 2.12.0 | none usable | **Decision 2.4**: typed blocker; fall back only to a Plan 052 F5 ADR-approved minimal sealed support topology. |
| i2pd 2.60.0 | candidate `ConnectToPeer` | **Decision 2.1**: source-inspect then implement the helper. |

Until the helper implementations are committed, the two reference-
initiated directions (`java-to-i2pr-ipv4` and `i2pd-to-i2pr-ipv4`) cannot
satisfy the Plan 052 directional predicate. They remain typed blockers,
not skipped successes.

## Reference trigger helper inventory

The committed helper inventory, when added, lives under
`tests/integration/ntcp2/reference-drivers/`:

```text
tests/integration/ntcp2/reference-drivers/
  java_i2p_direct_connect.py
  i2pd_direct_connect.py
  README.md
```

The `README.md` is the bridge between this document and the per-reference
helper. It cross-references the locked symbols and the trigger record
schema.

## ADR dependency

If both helpers fail source inspection, an ADR (likely 0020) must
justify a Plan 052 F5 minimal sealed support topology. Until the ADR is
written, the topology alternative is forbidden.

## Acceptance

The trigger contracts in this document close only when:

1. The helper files are committed and source-locked to the pinned
   revisions.
2. All four Plan 052 F4 control experiments pass.
3. The two reference-initiated directions reach `passed` under the
   receiver-side observation predicate.
4. Two complete evidence runs reproduce the four-direction outcome.

Until all four conditions are met, this document remains a working
diagnostic record.