# Plan 316 status — daemon-owned service-group lifecycle integration

Status: `blocked-normal-daemon-has-no-production-group-pool-provider`

Plan of record: [`316-daemon-owned-service-group-lifecycle-integration.md`](../../implementation/anonymity/316-daemon-owned-service-group-lifecycle-integration.md).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Normal daemon consumes the Plan 315 group owner | `register_service_tunnel_manager` is defined at `crates/i2pr-daemon/src/service_tunnels.rs:4918`; `rtk rg -n 'register_service_tunnel_manager\(' crates/i2pr-daemon` reports only that definition. `build_daemon_graph_inner` in `crates/i2pr-daemon/src/lib.rs` does not register it and has no service-tunnel graph branch. | Blocked: no production composition caller exists. |
| Group owner shares the daemon's router transport and inbound pump | `ServiceProduct::start` at `crates/i2pr-daemon/src/service_product.rs:515` constructs and starts a new `Ssu2DaemonService`, creates `ChildScope::for_test`, and owns `poll_inbound`. Its only call sites are two integration qualification tests. The normal `register_ssu2_service` separately constructs another `Ssu2DaemonService` and consumes the inbound queue in its own pump. | Blocked: using the qualification product directly would create a second SSU2 owner and split inbound ownership. |
| Usable selected peers and group pools gate application activation | The normal SSU2 inbound pump calls `dispatch_router_i2np` but does not compose `DestinationTunnelCoordinator`, `ExploratoryBuildCoordinator`, `DestinationPeerSelector`, or the Plan 315 pool advance/build path. There is no normal-daemon source that transfers validated peer material into these owners or signals usable group-pool readiness. | Blocked: no production pool provider exists to satisfy the gate. |
| Graceful drain precedes root/service cancellation | `Supervisor::shutdown` in `crates/i2pr-runtime/src/supervisor.rs` cancels the root and every active manager before joining them. `run_daemon` requests that shutdown directly on Ctrl-C. | Not implementable safely before a production group owner and its drain controls exist. |
| No parallel or premature service activation | Current normal daemon graph has no service-tunnel manager call site. `service_tunnels.enabled` is parsed but is not consumed in `build_daemon_graph_inner`. | No accidental listener activation today; adding the manager without a shared pool provider would violate Plan316 readiness invariants. |

## Audit and verification

- Attempt baseline: `c101b839a8b16cd43a4623912a608b5c6fd2608b` (Plan312 commit; first parent is current `origin/main` `2b0ca6b231987f8c2e6665bc9d45a9cd25ddc804`).
- Implementation commits: none. No production code was changed because the normal daemon has no usable Plan315 pool provider to consume.
- `rtk git status --short --branch` — clean at start on `work/anonymity-plans-311-312`.
- `rtk git fetch origin main` — passed; `origin/main` is `2b0ca6b231987f8c2e6665bc9d45a9cd25ddc804`, the first parent of the current Plan312 commit.
- `rtk rg -n 'register_service_tunnel_manager\(' crates/i2pr-daemon` — one result, the function definition only.
- `rtk rg -n 'ServiceProduct::start\(' crates/i2pr-daemon` — both call sites are integration qualification tests.
- `rtk rg -n 'Ssu2DaemonService::new|dispatch_router_i2np\(&inbound' crates/i2pr-daemon/src/lib.rs crates/i2pr-daemon/src/service_product.rs` — confirms separate normal-daemon and qualification SSU2 construction/pump paths.
- `rtk rg -n 'service_tunnels.enabled' crates/i2pr-daemon/src/lib.rs crates/i2pr-daemon/src/*.rs` — no normal graph consumption; only config defaults/tests appear.
- No production source, tests, or dependencies changed for Plan316. No tests were run because the required product composition and ready pool provider do not exist; implementing lifecycle hooks without that consumer would leave infrastructure disconnected and would not satisfy acceptance.
- `rtk git diff --check` — passed for this closure and the corrective registration.

## Compatibility, security, and findings

No runtime behavior, configuration, dependency, or support claim changed. The existing strict loopback-only, non-advertised SSU2 profile remains intact. Plan316 deliberately did not start a second SSU2 service, activate listeners without pools, or treat a qualification-only `ChildScope::for_test` owner as the production daemon owner.

There is no migration or rollback action because the attempted plan landed no product changes. No new secret, identity, peer, or address diagnostic surface was added. Contention, graceful-drain, and hard-upgrade tests were not run because there is no production group lifecycle consumer yet.

| Severity | Finding |
|---|---|
| Critical | None. |
| High | None. |
| Medium | The normal daemon has no router-backed provider for validated peer selection, Plan315 pool construction, or usable-pool readiness. Plan316 cannot integrate lifecycle ownership until that provider shares the daemon's existing SSU2 and inbound-message ownership. |
| Low | The parsed `service_tunnels.enabled` setting is not consumed by the production service graph. |

## Roadmap disposition and unblock audit

Plan317 is registered as the ready prerequisite that composes a router-backed Destination-group provider into the normal daemon using the existing SSU2 owner, validated peer facts, and Plan315 pools. Plan316 remains blocked pending Plan317; Plan311 remains blocked pending Plan316. Plan312 passed and independently unblocked Plan313, which remains ready. Plan308 remains independently blocked on controlled ordinary-HTTP topology evidence. No other eligible plan changed state.

Disposition: Plan316 is blocked at the missing normal-daemon product/provider boundary. Reopen it after Plan317 passes; then resume Plan311.
