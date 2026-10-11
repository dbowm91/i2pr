# Plan 455 status: blocked — stock-to-i2pr direction and negative matrix remain open

Closure token: `blocked-stock-to-i2pr-and-negative-matrix-unproven`

Plan: [`455-initiated-i2np-and-stock-peer-workload-recovery.md`](../../implementation/ntcp2-transport/455-initiated-i2np-and-stock-peer-workload-recovery.md)

## Outcome

The bounded direct initiator attempt used the existing `i2pr-runtime` handshake
and authenticated-link owner against an unmodified, exact-pinned i2pd 2.61.0
process. The stock process ran from a new private temporary data root, with
command-line `--datadir`, reseed threshold zero, and only its loopback NTCP2
listener enabled. Its signed RouterInfo and listener readiness were checked
before the attempt.

Gate A passed: the initiator authenticated one NTCP2 session, waited for the
frame writer to report a completed write, and sent one valid compressed I2NP
`DatabaseStore` containing the stock peer's own signed RouterInfo. The pristine
stock NetDB worker emitted its source-owned “Database store with own RouterInfo
received, dropped” marker. This independently establishes decoded receipt. The
old DeliveryStatus marker remains absent and no reply was expected from this
one-way probe.

Gate B passed its stock-owned workload and stock-to-stock control. Four pinned
stock routers were configured with NTCP2 enabled and SSU2 disabled; an external
SAM driver completed exact-byte STREAM exchanges in both directions between
two stock routers. Because NTCP2 was the only enabled router transport, the
successful stock-generated traffic demonstrates the real NTCP2 message path.
The pinned debug `SessionConfirmed sent` and authenticated-frame log marker
set was incomplete in this run, so source-level stage attribution remains a
limitation; the independent stock application exchange itself completed.

Gate C, the stock-to-i2pr direction, and the bounded negative matrix remain
open. The adjacent Plan 458 ordinary-process attempt reaches i2pr's SAM
listener but its router product rejects `SESSION CREATE` before a stream can
start. Plan 455 remains blocked at the first missing event: a stock-owned
request successfully delivered to an i2pr NTCP2 responder.

Plan 455 therefore remains blocked; this result is not a protocol failure and
does not qualify daemon activation, stock peer selection, or bidirectional
interoperability. Plan 456 is closed at its independent controlled
source/lifecycle scope. Plan 457 remains blocked until its full Plan 455 and
real-peer acceptance gates pass.

## Requirement-to-evidence matrix

| Requirement | Evidence / result |
| --- | --- |
| Frozen stock reference | The runner verified cache source revision `635b013a612ff47278ef02acf8580a28e10e26c5` and binary version 2.61.0. |
| Isolated stock process | The runner supplied a new mode-0700 data root with explicit CLI `--datadir` and `--conf`; reseed, SSU2, SAM, I2CP, I2PControl, HTTP, proxies, UPnP, floodfill, and transit were disabled. The scratch root and process were removed on exit. |
| Loopback NTCP2 readiness | The runner waited for the stock `router.info` and its configured NTCP2 listener log marker before connecting. |
| i2pr-initiated TCP and authentication | Passed: status reported `authenticated_sessions=1`. |
| i2pr-initiated NTCP2 and I2NP send | Passed: one session authenticated; one compressed DatabaseStore was reported written by the runtime writer. |
| Stock decoded receipt | Passed: exact-pinned stock NetDB emitted one own-RouterInfo DatabaseStore drop marker. This is the Gate A receive evidence. |
| Legitimate stock-owned outbound workload and stock-to-stock control | Passed at whole-application scope: two pinned stock SAM peers exchanged exact bytes over a four-peer topology with SSU2 disabled and NTCP2 listeners active. The required debug handshake/frame marker set was incomplete. |
| Stock-to-i2pr direction | Not completed. The stock SAM client has no i2pr destination because router-backed `SESSION CREATE` fails before stream setup (Plan 458). |
| Negative matrix / repeat | Not run. One Gate A probe and one stock workload topology were exercised; no negative matrix or repeat was attempted. |

## Commands and outcomes

Executed locally on Linux. The sanitized direct stock-process output was:

```text
plan455_initiator_exit=0
plan455_authenticated_sessions=1
plan455_i2np_messages_sent=1
plan455_stock_delivery_status_log_markers=0
plan455_stock_database_store_self_drop_markers=1
```

The stock-owned SAM driver completed these exact-byte exchanges before its
outer checker stopped at an optional debug-marker check:

```text
stock-f-to-stock-c=passed bytes=1816
stock-c-to-stock-f=passed bytes=2328
```

| Command | Outcome |
| --- | --- |
| `rtk bash tests/integration/ntcp2/plan455_direct_stock_attempt.sh` | Passed Gate A; one authenticated session, one sent I2NP, and one stock NetDb receipt marker. |
| `rtk python3 tests/integration/ntcp2/plan455_stock_sam_trigger.py` | Both stock SAM STREAM directions exchanged exact bytes on the NTCP2-only topology; the first invocation then returned 1 because the additional debug-marker check was absent. The runner now treats those debug counts as auxiliary and was not rerun under the one-topology budget. |
| `rtk python3 -m py_compile tests/integration/ntcp2/plan455_stock_sam_trigger.py tests/integration/core-router-recovery/plan458_sam_driver.py` | Passed. |
| `rtk cargo test --locked -p i2pr-interop --all-targets` | Passed, 34 tests. |
| `rtk cargo clippy --locked -p i2pr-interop --all-targets --all-features -- -D warnings` | Passed. |
| `rtk cargo test --locked -p i2pr-transport-ntcp2 --all-targets` | Prior Plan 455 run passed, 35 tests. |
| `rtk cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1` | Prior Plan 455 run passed, 129 passed / 1 ignored. |
| `rtk cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed, 4,821 tests; 39 ignored across 187 suites (1,068.53s). |
| `rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Passed. |
| `rtk cargo doc --locked --workspace --no-deps` with `RUSTDOCFLAGS="-D warnings"` | Passed; 33 crates documented. |
| `rtk cargo test --locked --workspace --doc` | Passed, 29 suites. |
| `rtk bash scripts/check-ntcp2-vectors.sh` and `rtk python3 scripts/check-current-pin-ntcp2-runner.py --self-test` | Passed. |
| `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'` | Passed, 51 tests. |
| Remaining `AGENTS.md` boundary/evidence guards and `rtk cargo deny check advisories bans sources` | Passed. |

The process runner is [`plan455_direct_stock_attempt.sh`](../../../tests/integration/ntcp2/plan455_direct_stock_attempt.sh). It writes status and raw stock logs only beneath a fresh temporary directory, prints fixed aggregate counts, and removes the directory and child process during cleanup.

## Security, migration, and compatibility

- All sockets were bound to `127.0.0.1`; no public I2P activity or reseed was enabled.
- The stock binary and source pin were not modified.
- No router identity, raw RouterInfo, endpoint, status JSON, or raw reference log was retained in the repository.
- No production code, config schema, `specs/support.toml`, or conformance claim changed.
- Plan 101's normal-daemon NTCP2 default-off and non-advertised posture is unchanged.

## Limitations and findings

Critical: none. High: none localized. Medium: stock SAM traffic passed in an
NTCP2-only stock topology, but exact transport stage markers were incomplete;
the stock-to-i2pr direction and negative matrix remain open. Low: Gate A is a
one-way NetDB self-drop probe and does not prove stock reply behavior.

## Unblock audit and disposition

- Plan 456 and its hard dependencies, Plans 450 and 454, are closed. Its controlled source/lifecycle result is independent of Plan 455 stock-peer selection.
- Plan 457 has the Plan 456 source/lifecycle pass and Plan 455 Gate A, but remains blocked on Gates B/C and its own dual-family/failure-continuity evidence.
- Plans 434, 435, and 452 retain their existing blocked status; this record does not rewrite their evidence.
- No new plan is registered. Gates A and B are retained as independent evidence. Continue only with the normal-daemon responder/product path needed for Gate C; do not repeat the stock-only topology or observer-only attempts.

Disposition: **blocked**, with Gates A/B passed at the evidence scope above and Gate C plus the negative matrix open.
