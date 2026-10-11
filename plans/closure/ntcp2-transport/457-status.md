# Plan 457 status: blocked — Plan 455 stock-to-i2pr and dual-family gates remain open

Closure token: `blocked-plan455-stock-to-i2pr-and-dual-family-continuity`

Plan: [`457-real-process-ntcp2-i2np-and-ssu2-fallback-acceptance.md`](../../implementation/ntcp2-transport/457-real-process-ntcp2-i2np-and-ssu2-fallback-acceptance.md)

## Outcome

Plan 456's controlled normal-daemon NTCP2 source/lifecycle pass is complete. Plan 455 Gates A and B now have evidence: i2pr sent a compressed I2NP DatabaseStore that the pinned stock NetDb decoded, and two pinned stock SAM peers completed exact-byte STREAM exchanges over a four-peer NTCP2-only topology. Plan 455 remains blocked because stock-to-i2pr delivery and the negative matrix have not been demonstrated.

Plan 457 requires both real-process directions, decoded/correlated I2NP, simultaneous single-router NTCP2/SSU2 use, and each family continuing after the other is stopped. Gates A/B establish i2pr-to-stock decoded receipt and the stock-only workload control, but stock-to-i2pr delivery and dual-family continuity remain unproven. Those prerequisites are environmental/evidentiary, not evidence of a transport protocol failure. No source change or weakened test was made to manufacture the missing traffic.

## Requirement disposition

| Requirement | Result |
| --- | --- |
| Plan 456 guarded normal-daemon source/lifecycle | Passed; see [`456-status.md`](456-status.md). |
| i2pr initiator to stock peer, authenticated decoded I2NP with correlated response | Plan 455 Gate A receive is established by the stock NetDb marker; a correlated reply is not part of the one-way probe. |
| Stock initiator to normal daemon | Blocked; the stock-only SAM workload passes, but the latest bounded Plan 458 attempt fails at the fixed `inbound-build` stage before an i2pr router-backed SAM destination can be provisioned, so no stock-to-i2pr stream/delivery was demonstrated. |
| Concurrent NTCP2 and SSU2 use by one real daemon and per-family failure continuity | Not run; no qualified real-peer workload exists for the required observation. |
| Negative, contention, restart, and orphan-task matrix under real peer load | Not run at this scope; local lifecycle and runtime controls remain covered by Plan 456 and the full workspace tests. |
| Public/two-family Plan 435 qualification | Out of scope and not established. |

## Commands and outcomes

| Command | Outcome |
| --- | --- |
| `rtk cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed: 4,821 passed, 39 ignored across 187 suites (1,068.53s). This validates repository regressions, not missing peer interoperability. |
| `rtk bash tests/integration/ntcp2/plan455_direct_stock_attempt.sh` | Gate A passed: initiator exit 0, one authenticated session, one I2NP send, and one stock DatabaseStore self-drop marker. |
| `rtk python3 tests/integration/ntcp2/plan455_stock_sam_trigger.py` | Stock SAM STREAM byte exchanges completed in both directions with NTCP2 as the only enabled reference transport; the initial outer runner then failed its additional debug-marker check. |
| `rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Passed. |
| `RUSTDOCFLAGS="-D warnings" rtk cargo doc --locked --workspace --no-deps` | Passed. |
| `rtk cargo test --locked --workspace --doc` | Passed, 29 suites. |
| `rtk cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed previously, 7 local real-process lifecycle cases; does not substitute for independent stock-peer traffic. |

## Security and compatibility

The daemon remains default-off for NTCP2 outside the named loopback qualification profile. No public address, capability, or support-inventory claim changed. No stock router was modified, no public network activity was used, and no second router identity/core was introduced.

## Findings and unblock audit

- Critical/high findings: none.
- Medium blocker: no stock-to-i2pr I2NP delivery, correlated reverse data, or dual-family continuity; Plan 455 Gates A/B have independent evidence.
- Plan 455 is not a protocol-failure finding and remains blocked with its sender-side evidence retained.
- Plan 456 is closed at its stated controlled source/lifecycle scope.
- Plan 458 is independent; its bounded SAM product composition is implemented, but its ordinary-process SAM provisioning attempt remains blocked before stream exchange.
- Plans 434/435/452 remain unchanged; this closure makes no public/two-family qualification claim.

Disposition: **blocked** pending Plan 455 stock-to-i2pr delivery and Plan 457's dual-family/failure-continuity gates. The stock-owned workload control is retained; resume at the router-backed destination provisioning boundary.
