# Plan 469 status: passed — stock selected i2pr and completed fresh NTCP2 publication

Closure token: `passed-stock-publish-fresh-ntcp2-product-and-correlated-reply`

Plan: [`469-stock-routerinfo-publish-trigger-and-fresh-ntcp2-initiator.md`](../../implementation/ntcp2-transport/469-stock-routerinfo-publish-trigger-and-fresh-ntcp2-initiator.md)

## Outcome

The bounded private fixture used pristine i2pd 2.61.0 at
`635b013a612ff47278ef02acf8580a28e10e26c5` and the ordinary `i2pr run`
composition. The candidate's signed `caps=f` RouterInfo was admitted into
four independent stock NetDBs, each with exactly five loaded routers and five
eligible floodfills. The stock source selector was precomputed with its
current-day routing key, XOR order, and floodfill self-exclusion; the live
publisher target receipts matched that model.

Repeated strict live executions selected i2pr from one or two of four normal
stock `RouterContext::Publish` operations; the source-owned selector
precomputation matched each live target set. Their processes had ordinary
`SSU2` disabled for the publication phase while retaining the already signed,
dual-transport floodfill inventory. This selected an unambiguous NTCP2 path
without editing or falsifying any RouterInfo. Every selected stock publisher
confirmed the nonzero-token publication. In the latest strict replay, the
daemon observed three fresh authenticated inbound NTCP2 links, seven forwarded
I2NP messages, one accepted third-party RouterInfo store with response owed,
one submitted floodfill response, and one correlated daemon probe. A preceding
same-source replay observed two selected candidates and corresponding
product/reply counts.

The same execution exercised Plan 467's real daemon request/reply action:
after each accepted stock store, i2pr submitted a signed local RouterInfo
DatabaseStore through the normal `RouterDeliveryService`, with a fresh
nonzero body token, direct reply tunnel zero, and a reply gateway bound to its
own signed RouterHash. It received and correlated two stock DeliveryStatus
replies on the same authenticated peer and link generations, with fresh
timestamps and pending requests. No raw token or identity was emitted.

This proves the bounded controlled-loopback stock-to-i2pr NTCP2 capability,
real product consumption, response ownership, and fresh stock initiation. It
does not promote public reachability, normal-default transport enablement,
or protocol support inventory.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Plan 468 admitted i2pr RouterInfo and healthy stock topology | Passed: four stock owners each loaded exactly five routers and five floodfills; signed candidate was present in each NetDB. |
| Source-owned normal publish selected i2pr | Passed on repeated runs: one or two of four live targets were candidate per run; each precomputed selection matched the live target set; zero no-tunnel outcomes. |
| Fresh stock NTCP2 initiator | Passed: publisher processes had no active SSU2 transport, used normal stock `RouterContext::Publish`, and created new authenticated inbound NTCP2 links at i2pr. |
| Actual stock I2NP and product effect | Passed: signed stock RouterInfo DatabaseStore crossed the authenticated ingress, was accepted by the normal floodfill product, and generated a genuine response. |
| Correlated response | Passed: both candidate-targeted publishers confirmed; daemon separately submitted and correlated two reverse tokened stores on current peer/link generations. |
| Wrong/missing admission input and selector controls | Passed in deterministic fixture tests: malformed/absent inventory, wrong source spelling, four-of-five threshold, invalid date, owner exclusion, and bounded live publisher config are checked. Stock candidate preflight covers network ID, version, `caps=f`, address styles, loopback ports, and static/`i` keys. |
| I2NP token, expiry, peer/link, and process lifecycle controls | Passed in focused suites: reserved/wrong token and stale/future timestamp; invalid signed RouterInfo; active-generation registry and stale close; queue-full and cancellation behavior; replay/key/network/expiry transport checks; daemon restart and teardown. |
| Public/reseed/config boundary | Passed: no public reseed, explicit private loopback and `reservedrange=false`, no fake role/version/bandwidth, no patched stock source, and no support-inventory promotion. |

The negative matrix is exercised at the owning parser, codec, transport,
registry, product, and lifecycle layers. The run does not claim a separate
end-to-end stock process for every malformed RouterInfo variant; stock-side
selection and admission are exercised with the exact valid signed topology.

## Commands and outcomes

| Command | Outcome |
| --- | --- |
| `rtk python3 tests/integration/ntcp2/plan468_private_netdb.py` | Passed again after the final timestamp-matcher change. Latest sanitized receipts: 4/4 inventories at 5 routers / 5 floodfills; selector predicted and observed 1 candidate target; 1 candidate confirmation; 3 fresh daemon NTCP2 links; 7 I2NP forwarded; 1 store accepted; 1 product reply submitted; 1 daemon reply-token probe submitted and correlated. A preceding strict replay selected 2 candidates and also matched all product and correlation counts. |
| `rtk python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan468_inventory.py'` | Passed, 6 parser/selector/config tests. |
| `rtk cargo test --locked -p i2pr-transport-ntcp2 --all-targets` | Passed, 35 tests. |
| `rtk cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1` | Passed, 134 passed, 1 ignored. |
| `rtk cargo test --locked -p i2pr-daemon --lib router_i2np -- --test-threads=1` | Passed after the timestamp matcher change, 24 passed, 1 ignored. |
| `rtk cargo test --locked -p i2pr-daemon --lib floodfill_reply_token_probe_is_explicit_and_controlled_only -- --test-threads=1` | Passed, 1 test. |
| `rtk cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed, 9 tests. |
| `rtk cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1` | Passed, 282 tests. |
| `rtk bash scripts/check-ntcp2-vectors.sh` and `rtk bash scripts/check-runtime-boundaries.sh` | Passed. |
| Final serial workspace floor | Passed after the final timestamp matcher and test-config change: 4,844 passed, 39 ignored across 187 suites (1,059.27s). Clippy, rustdoc generation, doc tests, planning tests, and repository boundary/evidence checkers also passed. |

Temporary identities, RouterInfos, logs, and datadirs were removed by the
runner. The committed record contains only bounded counts and exact source pin.

Disposition: **passed** for the controlled private stock-to-i2pr NTCP2 lane.
Plan 460 can be reassessed against this evidence; Plan 459's requirement for
an i2pr-initiated NTCP2 connection and Plan 461's dual-family continuity remain
separate acceptance gates.
