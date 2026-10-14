# Plan 478 status — inbound service route and external SAM product remain open

**Status token:** `in-progress-outbound-tunnel-route-wired-product-qualification-open`
**Disposition:** Service build composition now supports router-directed TunnelData through an activated outbound role. Startup creates at most one initial direct outbound bootstrap per daemon, then routes inbound, replacement, and additional outbound service builds through an existing outbound role. No network product qualification has run against this change. Plan 478 is not closed.

## 2026-10-14 bounded-retry + send-path attribution (commits `d58b29b`, `0ece629`, `a4fee2a`, `6fddcd3`, `35972c9`)

Narrowly controlled work inside this active plan; no general verification loop.

**Source changes (all in-plan, no new plan number):**

- Session-preferring inbound terminal reselection (`service_product.rs`): up to 8 bounded reselects prefer a terminal with a live SSU2 session, fail-open to the last qualified path. No new dial; Plan 482 authority routes dials only through the outbound first hop. Outcome is now observable as a redacted `inbound_terminal_reselected` log (`session_backed`, `reselections`, no identities).
- Bounded outbound-bootstrap retry: an emitted-but-unanswered bootstrap (`OutboundBuildMissing` + `AllOriginatorDeliveriesEmitted`/`PartialEmission` + live token) is cancelled (pending slot freed, identifiers replay-retained) and resubmitted exactly once on a freshly selected path with fresh tunnel/message identifiers and a new dial authority, logged as `bootstrap_retry`. Explicit rejection, invalid replies, local delivery failure, unobserved writes, and cancellation never loop. Pure predicate has unit coverage.
- Redacted send-path diagnostics on every build submit: `delivery_family` (`ssu2`/`ntcp2`/`mixed`/`unobserved`, via the existing `RouterDeliveryTransport::as_str`) and ephemeral selected-link `delivery_links` generations (numbers only). The harness counts families into the attempt diagnostic.
- Harness: ANSI-escape stripping in receipt parsers (prior receipts collapsed to empty), `I2PR_PLAN458_KEEP_SCRATCH=1` to preserve lane scratch for post-failure window correlation.

**Live evidence (pinned i2pd 2.61.0, four loopback floodfills, ordinary `i2pr run`):**

- Outbound direct bootstrap installs first-try in most runs (25 ms reply). One run fired the new retry correctly: first attempt emitted-but-unanswered → `bootstrap_retry` → fresh path on a new link generation → second attempt also unanswered → failed closed with `outbound-build`. Mechanism validated; success not yet observed.
- Two tunneled-inbound runs: outbound installs, 2 TunnelData cells socket-accepted (`inbound:all-emitted`, SSU2 family), then `inbound_build_wait_expired` with peer matches but zero message-ID matches. Reselection found a session-backed terminal (`session_backed=true`, 4 selections) with no effect.
- Stock-source correlation (ephemeral, counts only): **zero** `TunnelData`/`TunnelGateway` (I2NP types 18/19) receipts on any of the four peers across whole runs. The pinned source logs every I2NP message unconditionally at debug (`I2NPProtocol.cpp` `HandleI2NPMessage`), so socket-accepted cells never reached any stock I2NP dispatcher. Zero decrypt failures, zero next-hop rejections, zero reply-send failures in the windows. This is a systematic send-side non-arrival, not the 477 RI-resolvability divergence (receipt needs no RI over the existing first-hop session).
- Mesh-churn finding: stock-to-stock builds also time out in this 4-peer mesh (5 pending-build timeouts on one peer in one window; per-peer ShortTunnelBuild rates vary 2–3× run to run; our RI reached 2/4, 2/4, then 0/4 peers across runs). Our non-transit presence is a likely churn amplifier (stock exploratory builds select us, time out, retry). Per-run variance is high; single-run outcomes are weak evidence either way.
- No LS2 owner summary, publication, STREAM, or recovery stage was reached in any run.

**Next (in-plan):** symmetric accounted inbound retry (same predicate shape; the inbound wait currently fails closed without one); pre-run mesh-churn observability in the lane receipt (no gating); then one bounded lane run reading tunneled link generations. Own-RI `DatabaseStore` publication stays a conditional separate plan-of-record, not smuggled into 478.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
| --- | --- | --- |
| Preserve Plan 474 transactional owner publication | Existing Plan 474 owner-generation regressions and status | Retained; not requalified by the failed SAM lane |
| Use a stable Plan 477 build-ID boundary | `DestinationBuildRequest.message_id` is the terminal reply ID; `BridgeHeader::ShortTransport.message_id` is independently generated for the outer header; per-hop IDs are assigned by the runtime-neutral builder | Interface available |
| Compose normal inbound build over an established outbound tunnel | `provision_all_service_router_material` currently direct-submits inbound and defers outbound builds; `RouterDeliveryService` resolves direct authenticated links. Existing `outbound_lookup` TunnelData composition is used for NetDB, not service build creation | Not implemented |
| Compose normal inbound build over an established outbound tunnel | `submit_destination_via_outbound_tunnel` wraps the short build as router-directed `TunnelPayloadHeader` content, transforms it through `OutboundGatewayRole::forward_cells`, and sends bounded TunnelData cells to that role's first hop. Startup now establishes the first outbound role before submitting inbound builds; replacements and remaining outbound builds select a usable role and fail closed if none exists | Implemented at source composition; live interop not yet run |
| Keep a bounded bootstrap exception separate from ordinary Destination construction | The first outbound build of the daemon may use direct delivery when no outbound role exists. It must install before ordinary inbound construction; no route means service startup fails closed. | Implemented in startup ordering; no independent direct inbound fallback |
| Produce genuine nonzero-hop signed LS2, complete owner generation, bidirectional external SAM STREAM and lifecycle recovery | The two Plan 458 attempts stopped at inbound-build before owner summary and STREAM | Not established |

## Verification

| Command | Outcome |
| --- | --- |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd` | Passed before focused daemon tests |
| `cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1` | Passed (4 tests) |
| `cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed (9 tests) |
| `cargo test --locked -p i2pr-daemon --lib exploratory_build -- --test-threads=1` | Passed (14 tests), including the TunnelData first-hop versus build-gateway target check |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed after the final startup ordering and outbound TunnelData changes: 4,864 passed, 39 ignored across 187 suites, including service-product tests |
| `bash scripts/check-service-tunnel-boundaries.sh` | Passed after the outbound role routing change |
| `python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` | Failed at inbound-build after the new route. The direct bootstrap outbound build installed, then i2pd observed 27 ShortTunnelBuild records / 10 transit-create events and the inbound wait saw 3 expected-peer candidates but no expected message-ID match. No LS2 owner summary or STREAM exchange was reached. |
| `bash tests/integration/m6-interop/run-preflight.sh` | Passed against exact-pinned i2pd 2.61.0; Plan 184 control |
| `bash tests/integration/m6-interop/run-tunnels.sh` | Passed against exact-pinned i2pd 2.61.0; Plan 185 control |
| `python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` after Plan 477 changes | Failed at inbound-build on both runs; one run had one observed attempt and no reply match, the next had two attempts with one authenticated reply/install and one timeout |

The M6 controls prove their existing lower-tier contract only. They do not prove outbound-backed service-destination builds or SAM application traffic.

## Invariants and remaining work

No fake lease, zero-hop tunnel, or summary readiness was introduced. Inbound owner publication remains transactional. No public network, support inventory, RouterInfo advertisement, transit, or anonymity claim changed. The external SAM failure did not produce an attributable stock route and cannot justify loosening correlation.

The source path now exposes the outbound role before ordinary inbound service builds and composes the IBGW-targeted request through that role as TunnelData. The pinned lane confirms the first outbound bootstrap installs and the stock network creates transit tunnels, but the subsequent inbound return still fails strict message-ID matching. Remaining work includes source attribution for that return, signed usable leases, complete owner generation, bidirectional binary SAM STREAM, and expiry/replenishment or restart recovery. Do not claim those gates from the local composer test.

**Roadmap disposition:** active; no dependent capability is unblocked.
