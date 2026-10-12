# Plan 460 status amendment: passed — source-selected fresh stock NTCP2 product path

Closure token: `passed-stock-to-i2pr-without-client-sam-tunnel-plan469-evidence`

This is a new evidence disposition for Plan 460. It preserves the historical
`460-status.md` record, which accurately described the earlier missing
floodfill admission and trigger. Plan 468 has since closed the signed-RI
admission prerequisite, and Plan 469 executed the source-owned normal stock
publisher trigger against that admitted candidate.

## Outcome

The exact pinned unmodified stock process selected the controlled i2pr
floodfill from its frozen five-floodfill NetDB using normal
`RouterContext::Publish`. Its publisher process had SSU2 disabled for the send
phase, so its selected message used NTCP2. i2pr had no preexisting link to that
publisher; it authenticated fresh inbound stock NTCP2, forwarded the stock
I2NP into the ordinary product owner, accepted the RouterInfo store, and
submitted its legitimate floodfill reply. Stock confirmed the publication.
The same run also completed Plan 467's normal-daemon tokened request/reply on
those current authenticated generations.

Sanitized final-run values:

| Receipt | Count |
| --- | ---: |
| Stock owners with exactly five loaded routers/floodfills | 4 |
| Source-predicted / live i2pr candidate targets | 2 / 2 |
| Candidate publications confirmed by stock | 2 |
| Fresh authenticated i2pr inbound NTCP2 links | 4 |
| I2NP messages forwarded to the normal product owner | 12 |
| Third-party DatabaseStores accepted with response owed | 2 |
| Floodfill product responses submitted | 2 |
| Daemon-originated reply-token probes submitted / correlated | 2 / 2 |

No i2pr SAM destination, public I2P, stock patch, helper-owned transport
send, or fabricated role/reachability was used. The candidate floodfill was
the real controlled-loopback role with a signed `caps=f` dual-address
RouterInfo and five live eligible floodfills in each stock owner's NetDB.

## Negative and lifecycle evidence

The owning test layers cover malformed RouterInfo/signature and network
validation, wrong/reserved/stale response tokens, stale timestamps, expired
messages, malformed NTCP2 blocks, replay and key/network mismatch, duplicate
or stale link generations, queue saturation, cancellation, controlled-profile
default-off behavior, process restart, and teardown. The private topology
fixture checks malformed/missing inventory, the exact `floodfils` spelling,
four-versus-five admission, source-selector self-exclusion, and requires no
reseed. It uses `reservedrange=false` only for the loopback fixture. There is
no separate live stock execution for every counterfactual RouterInfo field;
the positive stock selection and normal product chain are live and source
audited.

| Command | Outcome |
| --- | --- |
| `rtk python3 tests/integration/ntcp2/plan468_private_netdb.py` | Passed exact-pin normal publisher, fresh NTCP2, product store/reply, and reverse token-correlation gates. |
| `rtk proxy bash -c 'export I2PR_PLAN467_REPLY_TOKEN=4670105; bash tests/integration/ntcp2/plan455_direct_stock_attempt.sh'` | Passed i2pr-to-stock lower tier with matching DeliveryStatus. |
| `rtk cargo test --locked -p i2pr-transport-ntcp2 --all-targets` | Passed, 35 tests. |
| `rtk cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1` | Passed, 134 passed, 1 ignored. |
| `rtk cargo test --locked -p i2pr-daemon --lib router_i2np -- --test-threads=1` | Passed after the timestamp matcher change, 24 passed, 1 ignored. |
| `rtk cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed, 9 tests. |
| `rtk cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1` | Passed, 282 tests. |

The final serial workspace floor passed after the final source changes:
4,844 passed and 39 ignored across 187 suites (1,059.27s); clippy, rustdoc,
doc tests, planning tests, and repository boundary/evidence checkers passed.
No public RouterInfo, conformance, or support inventory is promoted.

Disposition: **passed** for Plan 460's bounded controlled stock-to-i2pr
capability. Historical 455/457 closures remain unchanged; public/dual-family
support remains gated by Plans 435 and 461.
