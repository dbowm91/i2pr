# Plan 045 closure attempt (2026-07-22)

## Honest outcome

One of the four Plan 045 directional mixed-router scenarios produced a real
sanitized `actual_typed_result: passed` evidence record inside the Plan 049
Multipass guest running on the Plan 046 rootless sealed-namespace lane. The
other three reached a classified typed rejection before the data-phase oracle
could observe an authenticated handshake-and-directional-data-phase outcome.

This document records what was attempted, why each rejection typed out as it
did, and which of the underlying constraints remain unverified. It does not
close Milestone 3.

## Direction-by-direction outcome

| scenario_id              | actual_typed_result | reason_code                          | evidence_sha256                                                                |
| ---                      | ---                 | ---                                  | ---                                                                             |
| i2pr-to-i2pd-ipv4        | passed              | mixed-router-direction-authenticated | `6daee55ddb3ddcfcb336be33a1a58a564e473ff228e28cafc8e21209f38adb3f`             |
| i2pd-to-i2pr-ipv4        | rejected            | i2pr-responder-handshake-failed      | `d3bb8b…(cleaned by collect.py; not yet exported)`                             |
| i2pr-to-java-ipv4        | rejected            | typed-harness-operation-failed       | `ba0ee287df2ac8282ff08524dae05b256719a5714b38b4e96d96b07800d5683c`             |
| java-to-i2pr-ipv4        | rejected            | i2pr-responder-handshake-failed      | `f2…(cleaned by collect.py; not yet exported)`                                 |

`i2pr_router_info_sha256`, `reference_router_info_sha256`,
`reference_version`, `reference_revision`, `artifact_sha256`,
`installed_tree_sha256`, `configuration_sha256`,
`namespace_topology_sha256`, `sandbox_attestation_sha256`, and `direction`
all match the strict launcher schema on every record that `mixed_runner.py`
emitted. `cleanup_result: clean` on every record.

The launching commit is `1d7f4821fded10d6cd17a64d96f54a1db2dc1117` (see
"Source / cache / commit IDs" below). The guest
`/var/lib/i2pr-interop/environment.json` was reconciled to that commit
before the lane ran.

## i2pr-to-i2pd-ipv4 — the one protocol pass

i2pr bound `192.0.2.1:45680`, listed as listener-ready, then dialed
`192.0.2.2:45679` where i2pd was already listening. The handshake completed,
i2pd emitted `NTCP2: SessionConfirmed sent` at `debug`, i2pr emitted
`authenticated: 1, frames_sent: 1, frames_received: 0, i2np_sent: 1,
i2np_received: 0`, the non-echo data-phase oracle recorded `observed`
on the i2pr side, and the per-direction sentinel dump
`run-dir/raw/trigger-result.json` confirmed no SAM or HTTP trigger was
issued (i2pr is the initiator in this direction).

`expected_observation: i2pr-sent-only` was the typed prediction that i2pr
sends and i2pd does not echo. The recorded resource counters match. The
authenticated-link handshake is bounded to one fixed-size DeliveryStatus
message per direction (Plan 042 smoke scope).

## i2pd-to-i2pr-ipv4 — rejected; typed tunnel-pool blocker

i2pr bound `192.0.2.1:45680` (listener_ready emitted). i2pd bound
`192.0.2.2:45679` and exposed its SAM v3 bridge on `127.0.0.1:7656`. The
MixedRunner SAM v3 trigger opened a TCP session to `127.0.0.1:7656`,
exchanged `HELLO VERSION MIN=3.0 MAX=3.0` (one line read per request,
not a single `sendall` for both lines — see "Implementation changes
inspected and committed" below), then issued
`SESSION CREATE STYLE=STREAM ID=i2pr-interop-<ts> DESTINATION=TRANSIENT`.

i2pd logged `SAM: Session create: STYLE=STREAM ID=i2pr-interop-… DESTINATION=TRANSIENT`,
allocated a fresh local destination
`m46qhrtqecfjktpps7v63glilzuvie2sfycchzj7kvp6mleo35ba.b32.i2p`, and
blocked the `SESSION STATUS` reply under `SAM.cpp:474` until the
destination's pool reported `IsReady()`, which the i2pd pool check
(`Destination.h:151`) defines as a non-expired `LeaseSet` plus at least
one outbound tunnel.

The Plan 038 isolated net has no floodfill, no reseed URL, and no transit
configuration (`notransit = true`), so the pool logs repeatedly:

```
Tunnels: Can't select first hop for a tunnel. Trying already connected
Tunnels: Can't select next hop for u4DjtHgToBE3V6g94uy4OzEgiITEQ~lgmSps1kytjlM=
Tunnels: Can't create outbound tunnel, no peers available
Router: Can't find floodfill to publish our RouterInfo
```

The SAM trigger therefore never receives a `SESSION STATUS RESULT=OK`,
the harness sees no observed dial from the reference, and
`_evaluate_pass_predicate` rejects with
`i2pr-responder-handshake-failed`. i2pd 2.60.0 has no I2PControl JSON-RPC,
no `ConnectPeer`, and `HTTP_COMMAND_RUN_PEER_TEST` only dispatches an SSU2
peer test, not a direct NTCP2 dial.

Closing this direction in the current lane would require seeding the testnet
with at least one floodfill peer (Java or i2pd running `floodfill = true`)
or a multi-floodfill collective, both of which are out of scope for the
Plan 045 closure criterion.

## i2pr-to-java-ipv4 — rejected; typed JVM random-source env blocker

Java I2P 2.12.0 was launched inside the Plan 046 rootless sealed namespace
on a fresh per-direction data directory. The JVM reached `Starting I2P
2.12.0-0`, loaded `libjcpuid-x86-linux.so`, and entered the `EDH Precalc`
thread which then threw:

```
java.lang.IllegalStateException: Random is shut down - do you have a static ref?
    at gnu.crypto.prng.BasePRNGStandalone.nextBytes
    at net.i2p.util.FortunaRandomSource.nextBytes
    at net.i2p.crypto.KeyGenerator.generatePKIKeys
```

The router subsequently logged `Shutdown(3)` on two startup attempts roughly
3 and 7 seconds apart, never bound NTCP2, and the harness's
`wait_for_eventlog_started` exhausted the 240-second budget.

Java I2P uses `FortunaRandomSource` seeded from `/dev/random` and
`/dev/urandom` and falls back to `RandomSource` instances whose state is
shared across the JVM shutdown path. The pre-existing
`/var/lib/i2pr-interop/reference-data` directory that was prepared during
cloud-init had a working `prngseed.rnd` and `keyBackup` contents, but the
fresh per-direction data directory starts from an empty `prngseed.rnd`. In
the unprivileged user namespace the freshly-mounted `/dev/urandom` is
readable but the seeded `FortunaRandomSource` instance appears to be shut
down between the `EDH Precalc` thread's first call and the main router
thread's first call.

This is a Plan 046 environment blocker, not a Plan 044 / Plan 045 protocol
result. `I2P_DEBUG=1`, `log4j2.xml` reconfiguration, and running
`runplain.sh` (i.e. bypassing the `wrapper` startup script) inside the
namespace have not yet reproduced the same crash on every run; the
crashes are intermittent within the same guest and across restarts. The
same Java I2P binary boots reliably on the host outside the namespace.

Closing this direction requires a follow-up plan that either (a) makes the
rootless namespace expose a working `/dev/random` and `/dev/urandom` to the
user-namespace child and confirms `FortunaRandomSource` can seed from
them, or (b) pre-seeds a deterministic `prngseed.rnd` and a non-empty
`keyBackup` into every per-direction data directory the harness generates,
or (c) reduces the per-direction data directory to a shared immutable
seeded data directory owned by the guest host (which the Plan 046 lane
explicitly rejects).

## java-to-i2pr-ipv4 — rejected; same typed tunnel-pool blocker (Java side)

The shared `reference-data` directory's Java I2P boot succeeded in the
Phase-1 reference-control run earlier in this lane. The per-direction fresh
data directory for `java-to-i2pr-ipv4` had the same `FortunaRandomSource`
shutdown symptom (see previous section). Java I2P therefore did not bind
NTCP2, the Java SAM trigger could not reach the SAM bridge, and the
harness rejected with `i2pr-responder-handshake-failed`.

This is the same Java random-source env blocker, surfaced this time as the
harness-side `i2pr-responder-handshake-failed` typed reason rather than
the reference-side `typed-harness-operation-failed` reason because the
gate order is `i2pr terminal → reference authenticated → oracle observed`
and i2pr's responder terminal did not reach `passed` either.

## Implementation changes inspected and committed

The following Plan 045 corrective-pass defects were tracked, committed, and
verified by the static harness unit test (`python3 -m unittest discover -s
tests/integration/ntcp2/harness -p 'test_*.py'`; ran 203 tests, OK
skipped=1) and the Plan 046 `i2pr-to-i2pd-ipv4` direction was the first
to validate them end-to-end:

- `a705515` interop: allow `i2pr-responder-handshake-failed` known-deviation.
- `4666e05` interop: emit sanitized evidence for non-pass terminal results.
- `4a6ad7a` interop: force i2pd NTCP2 outbound via SAM v3 trigger.
- `86d8d00` harness: dump `trigger_result` to `run-dir/raw/trigger-result.json`.
- `82391fb` interop: connect SAM probe to `127.0.0.1` and capture
  stderr/stdout (the live `mix` between `ref_endpoint.local_address =
  192.0.2.2` and i2pd's `SAM bridge listening on 127.0.0.1:7656` was
  blocking SESSION CREATE from even reaching the parser).
- `d59fa64` interop: split SAM HELLO and SESSION CREATE into separate
  socket writes; the previous single-`sendall` payload closed the socket
  before i2pd's read of SESSION CREATE.
- `06430eb` interop: use `DESTINATION=TRANSIENT` (i2pd-only) plus
  `STREAM CONNECT id=… destination=<i2pr-public-destination>`; the
  i2pd `SAM_SESSION_CREATE_DUPLICATED_DEST` path requires `TRANSIENT`
  for ephemeral destinations.
- `868b418` interop: dump full stderr/stdout from the failed Java SAM probe
  to surface `KeyError: 'hello'` (java-i2p SAM probe used the legacy
  `{"host", "port", "payload"}` config while `i2pd` now uses the split
  `{"hello", "session_create", "stream_connect"}` schema).
- `1d7f482` interop: update Java SAM probe to the split schema with a
  typed SESSION-reply check.

The Plan 045 D1–D10 defect list (in the active plan and in `AGENTS.md`) is
satisfied for `i2pr-to-i2pd-ipv4`: the sanitized evidence record carries
real SHA-256 digests for both RouterInfos, a typed `data_phase_mode`, and
the expected `expected_observation`.

## Source / cache / commit IDs

- Last committed source: `1d7f4821fded10d6cd17a64d96f54a1db2dc1117`.
- `target/interop/multipass/state/plan049-20260721105135-3d7e7a68/source-transfer.json`
  records `source_archive_sha256: cf6e3115a12acd900bfca89a363714fa069c26bc236c74b48881e05e1547edb9`,
  `source_commit: 868b418ff0b8374b41075ca6489f037eec5f6847`, and a positive
  guest-side `source_tree.py --verify` after `transfer-source.sh`.
- Cache: `target/interop/multipass/state/plan049-20260721105135-3d7e7a68/cache-transfer.json`
  records the cached Java I2P 2.12.0 and i2pd 2.60.0 artifacts and
  `outcome: verified-cache-only`.
- Guest filesystem: `/home/i2ptest/i2pr/.i2pr-source-manifest.json`,
  `/home/i2ptest/i2pr/.source-listing.txt`, and `/home/i2ptest/i2pr/target/interop/evidence/*.json`.

## Why this is not a closure

Plan 045's closure record was supposed to demonstrate four directions of
authenticated NTCP2 handshake-and-bounded-data-phase evidence between
i2pr and each pinned reference. We delivered one direction under the
strict Plan 046 rootless sealed-namespace topology; the other three reach
typed rejections that are environmental, not protocol-level failures:

- Two directions require the reference to dial i2pr. In the isolated
  Plan 038 testnet, neither reference has a tunnel-pool infrastructure
  to make a direct dial through SAM v3.
- One direction uses Java I2P as the responder and another uses Java I2P
  as the initiator. Both surface the same Java `FortunaRandomSource`
  shutdown in the per-direction data-directory scenario inside the
  Plan 046 rootless namespace.

Both blockers should be addressed by a follow-up plan that is out of scope
for the Plan 045 certificate milestone itself. The current state is the
best honest sanitized evidence produced this lane.

## Reproduce

```bash
# inside the Multipass guest as i2ptest, with PATH including
# /home/i2ptest/.cargo/bin:
source_commit=1d7f4821fded10d6cd17a64d96f54a1db2dc1117

cd /home/i2ptest/i2pr

# one direction passes (the only recorded PASSED record):
I2PR_INTEROP_KEEP_RUN_DIR=1 I2PR_INTEROP_DUMP_RUN_LOGS=1 \
  bash scripts/interop/rootless-enter.sh \
    --scenario i2pr-to-i2pd-ipv4 \
    --reference i2pd \
    --build-cache /home/i2ptest/i2pr/target/interop/cache \
    --run-root /home/i2ptest/i2pr/target/interop/runs

# the other three report typed rejections:
bash scripts/interop/rootless-enter.sh --scenario i2pd-to-i2pr-ipv4 \
    --reference i2pd --build-cache … --run-root …
bash scripts/interop/rootless-enter.sh --scenario i2pr-to-java-ipv4 \
    --reference java_i2p --build-cache … --run-root …
bash scripts/interop/rootless-enter.sh --scenario java-to-i2pr-ipv4 \
    --reference java_i2p --build-cache … --run-root …
```

The combined evidence emission is
`target/interop/evidence/mixed-<ts>-<pid>-<8-hex>-<reference>.json`,
one file per direction. The `parent_network_state_unchanged` flag is `true`
for every record produced under the rootless sealed-namespace topology.
