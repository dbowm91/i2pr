# Plan 457 status: blocked — Plan 455 stock-owned send and reverse direction remain unavailable

Closure token: `blocked-plan455-stock-owned-workload-and-reverse-direction`

Plan: [`457-real-process-ntcp2-i2np-and-ssu2-fallback-acceptance.md`](../../implementation/ntcp2-transport/457-real-process-ntcp2-i2np-and-ssu2-fallback-acceptance.md)

## Outcome

Plan 456's controlled normal-daemon NTCP2 source/lifecycle pass is complete. Plan 455 Gate A now passes: one i2pr-initiated NTCP2 session sent a compressed I2NP DatabaseStore, and the pinned stock i2pd NetDb worker logged its own-RouterInfo drop after decoding it. Plan 455 remains blocked because no legitimate stock-owned send trigger or reverse stock-to-i2pr direction has been demonstrated.

Plan 457 requires both real-process directions, decoded/correlated I2NP, simultaneous single-router NTCP2/SSU2 use, and each family continuing after the other is stopped. Gate A resolves stock receipt in the i2pr-to-stock direction, but the missing stock-owned trigger and reverse direction still prevent the required bidirectional real-peer evidence. Those prerequisites are environmental/evidentiary, not evidence of a transport protocol failure. No source change or weakened test was made to manufacture the missing traffic.

## Requirement disposition

| Requirement | Result |
| --- | --- |
| Plan 456 guarded normal-daemon source/lifecycle | Passed; see [`456-status.md`](456-status.md). |
| i2pr initiator to stock peer, authenticated decoded I2NP with correlated response | Plan 455 Gate A receive is established by the stock NetDb marker; a correlated reply is not part of the one-way probe. |
| Stock initiator to normal daemon | Blocked; no stock-owned send trigger/selected peer was demonstrated. |
| Concurrent NTCP2 and SSU2 use by one real daemon and per-family failure continuity | Not run; no qualified real-peer workload exists for the required observation. |
| Negative, contention, restart, and orphan-task matrix under real peer load | Not run at this scope; local lifecycle and runtime controls remain covered by Plan 456 and the full workspace tests. |
| Public/two-family Plan 435 qualification | Out of scope and not established. |

## Commands and outcomes

| Command | Outcome |
| --- | --- |
| `rtk cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed: 4,821 passed, 39 ignored across 187 suites (1,068.53s). This validates repository regressions, not missing peer interoperability. |
| `rtk bash tests/integration/ntcp2/plan455_direct_stock_attempt.sh` | Gate A passed: initiator exit 0, one authenticated session, one I2NP send, and one stock DatabaseStore self-drop marker. |
| `rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Passed. |
| `RUSTDOCFLAGS="-D warnings" rtk cargo doc --locked --workspace --no-deps` | Passed. |
| `rtk cargo test --locked --workspace --doc` | Passed, 29 suites. |
| `rtk cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed previously, 7 local real-process lifecycle cases; does not substitute for independent stock-peer traffic. |

## Security and compatibility

The daemon remains default-off for NTCP2 outside the named loopback qualification profile. No public address, capability, or support-inventory claim changed. No stock router was modified, no public network activity was used, and no second router identity/core was introduced.

## Findings and unblock audit

- Critical/high findings: none.
- Medium blocker: no stock-owned outbound trigger/selected-peer dial or stock-to-i2pr direction; Plan 455 Gate A decoded receipt is now established.
- Plan 455 is not a protocol-failure finding and remains blocked with its sender-side evidence retained.
- Plan 456 is closed at its stated controlled source/lifecycle scope.
- Plan 458 is independent; its bounded SAM product composition is implemented, but its ordinary-process SAM provisioning attempt remains blocked before stream exchange.
- Plans 434/435/452 remain unchanged; this closure makes no public/two-family qualification claim.

Disposition: **blocked** pending the Plan 455 stock-owned workload and reverse-direction evidence, followed by Plan 457's dual-family/failure-continuity gates. Resume when a concrete eligible stock-owned send topology is available.
