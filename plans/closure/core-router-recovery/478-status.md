# Plan 478 status — inbound service route and external SAM product remain open

**Status token:** `in-progress-outbound-tunnel-route-wired-product-qualification-open`
**Disposition:** Service build composition now supports router-directed TunnelData through an activated outbound role. Startup creates at most one initial direct outbound bootstrap per daemon, then routes inbound, replacement, and additional outbound service builds through an existing outbound role. No network product qualification has run against this change. Plan 478 is not closed.

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
