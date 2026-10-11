# Plan 455 status: blocked — stock-owned send and reverse direction remain unproven

Closure token: `blocked-stock-owned-workload-and-reverse-direction-unproven`

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

Gate B's legitimate stock-owned outbound workload and stock-to-stock dial
control, and Gate C's stock-to-i2pr direction, remain unproven. No reverse
attempt or negative matrix was run. Plan 455 therefore remains blocked at the
first missing event: a real stock-owned I2NP send that selects/dials a peer.

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
| Legitimate stock-owned outbound workload and stock-to-stock control | Not established. Plan 452's two-idle-peer control remains `no-dial-trigger`; no eligible stock-owned send selected a peer or produced a dial. |
| Stock-to-i2pr direction | Not run; Gate B trigger and stock self-control are unavailable. |
| Negative matrix / repeat | Not run. The bounded Gate A probe was repeated to correct and verify its write-completion and status parsing; no protocol retry or negative matrix was attempted. |

## Commands and outcomes

Executed locally on Linux. The sanitized direct stock-process output was:

```text
plan455_initiator_exit=0
plan455_authenticated_sessions=1
plan455_i2np_messages_sent=1
plan455_stock_delivery_status_log_markers=0
plan455_stock_database_store_self_drop_markers=1
```

| Command | Outcome |
| --- | --- |
| `rtk bash tests/integration/ntcp2/plan455_direct_stock_attempt.sh` | Passed Gate A; one authenticated session, one sent I2NP, and one stock NetDb receipt marker. |
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

Critical: none. High: none localized. Medium: the stock-owned outbound I2NP
trigger, selected-peer dial, reference self-control, and reverse stock-to-i2pr
direction remain unproven. Low: the Gate A probe is intentionally one-way and
uses a NetDB self-drop marker; it does not prove stock reply behavior.

## Unblock audit and disposition

- Plan 456 and its hard dependencies, Plans 450 and 454, are closed. Its controlled source/lifecycle result is independent of Plan 455 stock-peer selection.
- Plan 457 has the Plan 456 source/lifecycle pass and Plan 455 Gate A, but remains blocked on Gates B/C and its own dual-family/failure-continuity evidence.
- Plans 434, 435, and 452 retain their existing blocked status; this record does not rewrite their evidence.
- No new plan is registered. Gate A is retained as independent evidence. Continue only with a bounded, eligible stock-owned send topology; do not repeat observer-only attempts.

Disposition: **blocked**, with Gate A passed and Gates B/C explicitly open.
