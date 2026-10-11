# Plan 455 status: blocked — stock receipt and stock-owned send remain unproven

Closure token: `blocked-stock-inbound-i2np-and-stock-workload-unproven`

Plan: [`455-initiated-i2np-and-stock-peer-workload-recovery.md`](../../implementation/ntcp2-transport/455-initiated-i2np-and-stock-peer-workload-recovery.md)

## Outcome

The bounded direct initiator attempt used the existing `i2pr-runtime` handshake
and authenticated-link owner against an unmodified, exact-pinned i2pd 2.61.0
process. The stock process ran from a new private temporary data root, with
command-line `--datadir`, reseed threshold zero, and only its loopback NTCP2
listener enabled. Its signed RouterInfo and listener readiness were checked
before the attempt.

The initiator authenticated one NTCP2 session and sent one I2NP
`DeliveryStatus` message. The stock process emitted no matching
`DeliveryStatus` log marker, and no correlated reply was observed. This is
positive sender-side evidence but does **not** satisfy Gate A's independent
receiver observation. Gate B's stock-owned outbound workload and Gate C's
stock-to-i2pr direction were not run. The exact first missing event is
stock-side decoded receipt of the sent I2NP message.

Plan 455 therefore remains blocked; this result is not a protocol failure and
does not qualify daemon activation, stock peer selection, or interoperability.
Plan 456 is independent of stock peer selection and remains executable. Plan
457 remains blocked until both its Plan 455 and Plan 456 evidence gates pass.

## Requirement-to-evidence matrix

| Requirement | Evidence / result |
| --- | --- |
| Frozen stock reference | The runner verified cache source revision `635b013a612ff47278ef02acf8580a28e10e26c5` and binary version 2.61.0. |
| Isolated stock process | The runner supplied a new mode-0700 data root with explicit CLI `--datadir` and `--conf`; reseed, SSU2, SAM, I2CP, I2PControl, HTTP, proxies, UPnP, floodfill, and transit were disabled. The scratch root and process were removed on exit. |
| Loopback NTCP2 readiness | The runner waited for the stock `router.info` and its configured NTCP2 listener log marker before connecting. |
| i2pr-initiated TCP and authentication | Passed: status reported `authenticated_sessions=1`. |
| i2pr-initiated I2NP send | Passed sender-side: status reported `i2np_messages_sent=1`. |
| Stock decoded receipt / correlated reply | Not established: `stock_delivery_status_log_markers=0`; no reply was observed. Gate A is incomplete. |
| Legitimate stock-owned outbound workload and stock-to-stock control | Not run. The preceding Plan 452 control remains `no-dial-trigger`; no eligible stock-owned operation with a selected peer was available in this fixture. |
| Stock-to-i2pr direction | Not run; no qualified stock-to-stock trigger and no receiver-side receipt evidence. |
| Negative matrix / repeat | Not run. One bounded run was used after adding explicit listener-readiness attribution to the reproducible runner; no protocol retry was attempted. |

## Commands and outcomes

Executed locally on Linux. The direct stock-process command is expected to
exit nonzero while Gate A receiver evidence is absent; its sanitized output
was:

```text
plan455_initiator_exit=0
plan455_authenticated_sessions=1
plan455_i2np_messages_sent=1
plan455_stock_delivery_status_log_markers=0
```

| Command | Outcome |
| --- | --- |
| `bash tests/integration/ntcp2/plan455_direct_stock_attempt.sh` | Blocked at missing stock receipt marker; initiator itself exited 0. |
| `cargo test --locked -p i2pr-transport-ntcp2 --all-targets` | Passed, 35 tests. |
| `cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1` | Passed, 129 passed / 1 ignored. |
| `cargo test --locked -p i2pr-interop --all-targets` | Passed, 32 tests. |
| `bash scripts/check-ntcp2-vectors.sh` | Passed. |
| `python3 scripts/check-current-pin-ntcp2-runner.py --self-test` | Passed. |
| `bash -n tests/integration/ntcp2/plan455_direct_stock_attempt.sh` | Passed. |
| `bash scripts/check-ntcp2-interoperability.sh` | Passed. |
| `python3 scripts/check-global-plan-number-uniqueness.py` | Passed. |
| `python3 scripts/check-router-readiness-contract.py --self-test` | Passed. |
| `python3 scripts/check-workflow-validity.py` | Passed; 11 workflow files parsed. |

The process runner is [`plan455_direct_stock_attempt.sh`](../../../tests/integration/ntcp2/plan455_direct_stock_attempt.sh). It writes status and raw stock logs only beneath a fresh temporary directory, prints fixed aggregate counts, and removes the directory and child process during cleanup.

## Security, migration, and compatibility

- All sockets were bound to `127.0.0.1`; no public I2P activity or reseed was enabled.
- The stock binary and source pin were not modified.
- No router identity, raw RouterInfo, endpoint, status JSON, or raw reference log was retained in the repository.
- No production code, config schema, `specs/support.toml`, or conformance claim changed.
- Plan 101's normal-daemon NTCP2 default-off and non-advertised posture is unchanged.

## Limitations and findings

Critical: none. High: none localized. Medium: the stock process did not provide
an independently observable receipt for the authenticated I2NP send; the
stock-owned dial trigger is still absent. Low: the one-shot runner recognizes
only explicit stock `DeliveryStatus` log markers, so receiver observation must
also use a correlated protocol reply if the stock logger does not expose that
message class.

## Unblock audit and disposition

- Plan 456's hard dependencies, Plans 450 and 454, are already closed. Plan 455's partial sender-side result does not gate Plan 456; Plan 456 remains active.
- Plan 457 is still blocked on complete Plan 455 evidence and the Plan 456 source/lifecycle pass.
- Plans 434, 435, and 452 retain their existing blocked status; this record does not rewrite their evidence.
- No new plan is registered. Continue Plan 456; resume Plan 455 only with a specific fixture correction or an eligible stock-owned send topology, not another observer-only loop.

Disposition: **blocked**, with partial authenticated sender-side evidence retained and the receiver, stock-to-stock, and reverse-direction gates explicitly open.
