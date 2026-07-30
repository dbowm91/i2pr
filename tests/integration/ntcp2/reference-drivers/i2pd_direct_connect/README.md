# Plan 059 Workstream B: i2pd direct connect helper

The `i2pd_direct_connect` helper is the source-locked i2pd direct
NTCP2 trigger for the `i2pd-to-i2pr-ipv4` reference-initiated
direction. It is a separately-built test executable that links
against the pinned i2pd 2.60.0 libraries
(`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`) and exercises the
documented `i2pd::transport::Transports::SendMessage` call graph.

## Source lock

The helper is source-locked to:

- **i2pd 2.60.0** revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`
- helper source: `i2pd_direct_connect.cpp`
- build contract: `CMakeLists.txt`
- provenance record: `source-lock.json`

The `source-lock.json` record records the pinned revision, the
required Boost/OpenSSL components, the helper source and binary
digests, the documented call graph, and the locked constraints
required by Plan 055 Workstream A and Plan 059 Workstream B.

## Call graph

The helper invokes the pinned i2pd transport path documented in
`tests/integration/ntcp2/reference-trigger-contracts.md`:

```text
Transports::SendMessage(ident, msg)
  -> Transports::ConnectToPeer(ident, peer)
     -> NTCP2Session::NTCP2Session(...)
     -> NTCP2Server::Connect(session)
        -> SessionConfirmed sent (NTCP2Session::SendSessionConfirmed)
```

The helper never reaches inside the handshake or AEAD frame state.
The pinned libraries own the protocol behaviour; the helper only
orchestrates a one-shot outbound dial.

## Required helper interface

```bash
i2pd_direct_connect \
  --data-dir <fresh-run-dir> \
  --router-info <i2pr-router-info-path> \
  --expected-router-hash <40hex> \
  --expected-host 192.0.2.1 \
  --expected-port 45680 \
  --run-id <id> \
  --scenario-id <id> \
  --correlation-nonce <nonce> \
  --run-identity-sha256 <64hex> \
  --helper-binary-sha256 <64hex> \
  --helper-source-sha256 <64hex> \
  --source-inspection-record-sha256 <64hex> \
  --result <trigger-record.json>
```

The helper must:

1. validate the target RouterInfo before starting transports;
2. reject a target hash mismatch (`rejected-target-router-info`);
3. reject a target endpoint mismatch (`rejected-target-endpoint`);
4. add only the declared RouterInfo to the disposable reference
   NetDB;
5. start the minimum required i2pd transport context with SSU2
   disabled;
6. request exactly one outbound peer connection;
7. wait for a bounded callback/terminal result (default 15 s, max
   600 s, configurable via `--dial-timeout-seconds`);
8. emit one `i2pr-reference-trigger-v3` record;
9. shut down all helper-owned reference state before exit;
10. return nonzero for rejected/blocked outcomes.

## Exit codes

| Exit code | Outcome |
| --- | --- |
| 0 | `connected` — outbound dial established |
| 64 | invalid command-line arguments |
| 65 | `rejected-target-router-info` (hash mismatch, parse error, or unreachable) |
| 66 | `direct-trigger-callback-timeout` (no callback within the bounded window) |
| 70 | result file write failed |
| 71 | `cleanup-failed` (transport teardown raised) |
| 73 | `direct-trigger-helper-failed` (data directory setup failed) |

## Build

```bash
cmake -S tests/integration/ntcp2/reference-drivers/i2pd_direct_connect \
      -B build/i2pd_direct_connect \
      -DI2PD_SOURCE_DIR=<path-to-pinned-i2pd-source>
cmake --build build/i2pd_direct_connect
```

The pinned i2pd source tree must be available locally; the helper
never compiles against an unverified revision. The build never
modifies the pinned source tree and never enables SSU2.

## Constraints

- exactly one outbound dial per invocation (one-shot contract);
- no retries, no sleeps, no DNS, no public network egress;
- the helper may not bypass authentication, inject success, or
  modify transport behaviour;
- the helper is a test executable — production code never depends
  on it;
- the `source-lock.json` provenance is mandatory before the helper
  may run; missing digests fail the Plan 059 B7 control.

## Plan 059 controls

The helper is qualified through the eight controls enumerated in
Plan 055 Workstream B4 (positive target, wrong RouterInfo, wrong
endpoint, no listener, no invocation, duplicate attempt, stale
helper binary digest, changed pinned i2pd tree). Plan 059 Workstream
B4 implements the helper, but the external qualification run requires
an authorized Ubuntu 24.04 amd64 host or Multipass guest (the current
host is the Plan 046 `apparmor_restrict_on` negative baseline).
