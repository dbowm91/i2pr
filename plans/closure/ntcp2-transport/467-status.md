# Plan 467 status: passed — stock DeliveryStatus and normal-daemon reply-token round trip

Closure token: `passed-normal-daemon-ntcp2-routerinfo-deliverystatus-roundtrip`

Plan: [`467-normal-daemon-ntcp2-databasestore-deliverystatus-roundtrip.md`](../../implementation/ntcp2-transport/467-normal-daemon-ntcp2-databasestore-deliverystatus-roundtrip.md)

## Outcome

Both required directions completed against unmodified, pinned i2pd
2.61.0 (`635b013a612ff47278ef02acf8580a28e10e26c5`). The Plan 455 initiator
runner's Plan 467 mode sends a signed self-RouterInfo DatabaseStore with a
nonzero, nonreserved body reply token, reply tunnel ID zero, and the
authenticated initiator's RouterHash. Stock i2pd emitted a matching
DeliveryStatus over the authenticated link; its normal self-RouterInfo drop
marker followed. The stock response was correlated by its body token, not by
the independent envelope message ID.

The normal `i2pr run` controlled floodfill owner also completed the reverse
request/reply using its real product path. After accepting a stock RouterInfo
DatabaseStore, the floodfill owner submitted its own signed compressed
RouterInfo DatabaseStore with a fresh nonzero token, tunnel zero, and a reply
gateway derived from its own signed identity. It sent through the existing
`RouterDeliveryService` and current authenticated NTCP2 peer route. The daemon
consumed the returned DeliveryStatus only when peer, link generation, token,
pending expiry, and status timestamp matched. No token or peer identity was
logged.

The explicit `floodfill.send_reply_token_probe` action is default-off and is
accepted only with the controlled-loopback floodfill profile. It sends at
most one probe per active NTCP2 link generation and keeps pending correlations
bounded by the configured active-link ceiling. The remote stock router's
DeliveryStatus is handler/transport evidence; the distinct stock Store
acceptance and normal floodfill effects are recorded separately by the live
product owner.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Gate A: i2pr initiator to pinned stock | Passed: one authenticated NTCP2 session, one tokened I2NP sent, stock self-RouterInfo drop marker, exact token `4670105` DeliveryStatus observed and correlated on generation 1. |
| Gate B: real `i2pr run` product sender and consumer | Passed: repeated strengthened private-NetDB runs submitted and correlated every daemon probe. The latest strict replay correlated 1/1; earlier same-source executions correlated 2/2 and 3/3. Each probe used a stock-created authenticated NTCP2 generation and returned with the exact expected body token and fresh timestamp. |
| Signed request and response route | Passed: request builder verifies the local signed RouterInfo, uses its RouterHash as key and gateway, nonzero token, and tunnel ID zero. |
| Correlation negatives | Passed: zero/reserved tokens, wrong token, stale/future timestamp, invalid signature, and expired status are refused by focused daemon/runtime tests. Link peer and generation are part of the live pending key. |
| Store acceptance distinction | Preserved: `DeliveryStatus` is not treated as acceptance; third-party stores are separately observed through floodfill `Stored { acknowledgement: Some(_) }` effects. |
| Defaults and network surface | Passed: the action defaults off and config parsing rejects it outside `controlled_loopback`; NTCP2 and floodfill remain loopback-only, non-advertised, and default-off. |

## Commands and outcomes

| Command | Outcome |
| --- | --- |
| `rtk proxy bash -c 'export I2PR_PLAN467_REPLY_TOKEN=4670105; bash tests/integration/ntcp2/plan455_direct_stock_attempt.sh'` | Passed: 1 session, 1 I2NP message, 1 stock self-store marker, matching token `4670105`, link generation 1. |
| `rtk python3 tests/integration/ntcp2/plan468_private_netdb.py` | Passed again after the final timestamp-matcher change: exact-pin inventory and selector matched; the latest strict replay selected i2pr once and the daemon submitted/correlated its one reply-token probe. Earlier successful strict executions are recorded in Plan 469 status. |
| `rtk cargo test --locked -p i2pr-transport-ntcp2 --all-targets` | Passed, 35 tests. |
| `rtk cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1` | Passed, 134 passed, 1 ignored. |
| `rtk cargo test --locked -p i2pr-daemon --lib router_i2np -- --test-threads=1` | Passed after the timestamp matcher change, 24 passed, 1 ignored; includes signed-probe encoding, token/timestamp rejection, link-generation and queue cases. |
| `rtk cargo test --locked -p i2pr-daemon --lib floodfill_reply_token_probe_is_explicit_and_controlled_only -- --test-threads=1` | Passed, 1 test. |
| `rtk cargo test --locked -p i2pr-interop plan467 -- --test-threads=1` | Passed, 2 tests. |
| `rtk cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed, 9 tests. |
| `rtk cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1` | Passed, 282 tests. |
| `rtk bash scripts/check-ntcp2-vectors.sh` and `rtk bash scripts/check-runtime-boundaries.sh` | Passed. |
| Final serial workspace floor | Passed after the final timestamp matcher and test-config change: 4,844 passed, 39 ignored across 187 suites (1,059.27s). Clippy, rustdoc generation, doc tests, planning tests, and repository boundary/evidence checkers also passed. |

Raw RouterInfos, stock identities, response tokens, and process logs remained in
secured temporary directories and were removed by the runner. Only sanitized
counts and the intentionally fixed test token are recorded.

Disposition: **passed**. This closes the Plan 467 request/reply contract. It
does not by itself close Plan 459's separate requirement for a daemon-initiated
NTCP2 connection, nor Plan 460's stock-to-i2pr capability beyond the distinct
fresh-stock evidence recorded there.
