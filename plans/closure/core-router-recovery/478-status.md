# Plan 478 status — inbound service route and external SAM product remain open

**Status token:** `in-progress-normal-outbound-backed-inbound-route-not-implemented`  
**Disposition:** Plan 477's outer/header, per-hop forwarding, and terminal reply-ID distinctions are now explicit in the production composition. The ordinary service-destination path still submits inbound builds through direct `RouterDeliveryService` and waits for inbound leases before starting pending outbound builds. Plan 478 is not closed.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
| --- | --- | --- |
| Preserve Plan 474 transactional owner publication | Existing Plan 474 owner-generation regressions and status | Retained; not requalified by the failed SAM lane |
| Use a stable Plan 477 build-ID boundary | `DestinationBuildRequest.message_id` is the terminal reply ID; `BridgeHeader::ShortTransport.message_id` is independently generated for the outer header; per-hop IDs are assigned by the runtime-neutral builder | Interface available |
| Compose normal inbound build over an established outbound tunnel | `provision_all_service_router_material` currently direct-submits inbound and defers outbound builds; `RouterDeliveryService` resolves direct authenticated links. Existing `outbound_lookup` TunnelData composition is used for NetDB, not service build creation | Not implemented |
| Keep a bounded bootstrap exception separate from ordinary Destination construction | No typed bootstrap-only service-build route exists | Not implemented |
| Produce genuine nonzero-hop signed LS2, complete owner generation, bidirectional external SAM STREAM and lifecycle recovery | The two Plan 458 attempts stopped at inbound-build before owner summary and STREAM | Not established |

## Verification

| Command | Outcome |
| --- | --- |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd` | Passed before focused daemon tests |
| `cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1` | Passed (4 tests) |
| `cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed (9 tests) |
| `bash tests/integration/m6-interop/run-preflight.sh` | Passed against exact-pinned i2pd 2.61.0; Plan 184 control |
| `bash tests/integration/m6-interop/run-tunnels.sh` | Passed against exact-pinned i2pd 2.61.0; Plan 185 control |
| `python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` after Plan 477 changes | Failed at inbound-build on both runs; one run had one observed attempt and no reply match, the next had two attempts with one authenticated reply/install and one timeout |

The M6 controls prove their existing lower-tier contract only. They do not prove outbound-backed service-destination builds or SAM application traffic.

## Invariants and remaining work

No fake lease, zero-hop tunnel, or summary readiness was introduced. Inbound owner publication remains transactional. No public network, support inventory, RouterInfo advertisement, transit, or anonymity claim changed. The external SAM failure did not produce an attributable stock route and cannot justify loosening correlation.

The next implementation must expose an owned outbound role before ordinary inbound service builds, compose the IBGW-targeted request through that role as TunnelData/garlic where required, and reserve direct delivery for an explicit bounded bootstrap transition. It must then prove exact correlated build effects, signed usable leases, complete owner generation, bidirectional binary SAM STREAM, and expiry/replenishment or restart recovery.

**Roadmap disposition:** active; no dependent capability is unblocked.
