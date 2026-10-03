# Plan 317 status — normal-daemon Destination-group pool provider

Status: `blocked-normal-daemon-product-owner-boundary-requires-corrective-plan318`

Plan of record: [`317-normal-daemon-destination-group-pool-provider.md`](../../implementation/anonymity/317-normal-daemon-destination-group-pool-provider.md).

Corrective successor: [Plan 318](../../implementation/anonymity/318-normal-daemon-single-owner-group-product.md).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Integrate Plan 315 groups with the normal daemon's one SSU2 service and inbound owner | `register_ssu2_service` in `crates/i2pr-daemon/src/lib.rs` owns the normal service and its inbound loop. `ServiceProduct::start` in `crates/i2pr-daemon/src/service_product.rs` creates a separate `Ssu2DaemonService`, uses `ChildScope::for_test`, and exposes a manually polled `poll_inbound`. Its call sites are qualification integration tests. | Blocked: the qualification composition cannot be registered in the normal graph without creating a second transport and inbound queue owner. |
| Use authoritative validated router candidates | `Bootstrap::store()` exposes a bounded validated `RouterInfoStore`, but `run_daemon` passes only `Config` into graph construction. The normal inbound path does not share bootstrap state with a group owner. The Plan 315 coordinator has a separate store populated in the qualification product. | Blocked: no shared production store handoff exists. |
| Run exact-three pool builds over the normal owner and gate configured services on usable pools | Plan 315 pool advancement and replenishment run inside `ServiceProduct::poll_inbound`; the normal `register_ssu2_service` instead runs floodfill and the existing router dispatcher. `build_daemon_graph_inner` has no service-tunnel manager registration. | Blocked: selection, pool advancement, consumer routing, readiness, and listener activation are not part of the normal product. |
| Enabled service configuration fails explicitly when no production provider exists | `build_daemon_graph_inner` now rejects any enabled service-tunnel specification with a typed daemon graph error before service registration. A focused regression covers this path. | Implemented as a fail-closed compatibility gate; does not satisfy provider capability. |
| Existing transport and service security boundaries remain unchanged | The guard runs before graph services are constructed. No socket, key, wire format, peer route, or listener behavior changed. SSU2 remains loopback-only and disabled by default. | Met for this partial corrective. |

## Audit and verification

- Implementation commit: `3baea8c` (`fix(daemon): reject unowned enabled service tunnels`).
- `rtk cargo fmt --all --check` — passed.
- `rtk cargo test --locked -p i2pr-daemon --lib daemon_graph_rejects_enabled_service_tunnels_without_provider -- --test-threads=1` — passed, 1 test.
- `rtk cargo test --locked -p i2pr-daemon --lib daemon_graph_ -- --test-threads=1` — passed, 3 graph tests, including the unchanged disabled-service graph path.
- `rtk bash scripts/check-service-tunnel-boundaries.sh` — passed.
- `rtk bash scripts/check-service-anonymity-boundaries.sh` — passed.
- `rtk git diff --check` — passed.
- No full daemon/workspace suite or Plan 314/315 evidence checks were run. The production provider, transport/store handoff, readiness gates, and associated regressions were not implemented, so the plan's capability acceptance criteria cannot be claimed.

## Compatibility, security, and findings

Enabled service configurations now fail at normal graph construction rather than being accepted and silently omitted. Disabled-by-default configurations are unchanged. No dependency, wire format, persistence format, identity relationship, listener bind, or support claim changed. The guard adds no new diagnostic data beyond a static explanation that the provider is unavailable.

| Severity | Finding |
|---|---|
| Critical | None. |
| High | None. |
| Medium | Normal daemon and qualification product have incompatible owners: the latter starts its own SSU2 service, uses a test-only child scope, and consumes its inbound queue separately. Bootstrap's bounded validated store is not passed into the graph, and the normal pump has no group-pool hook. Plan 317 cannot safely achieve its acceptance criteria without a production single-owner product boundary. |
| Low | The explicit graph-construction rejection is a compatibility guard only; it does not activate configured services. |

## Roadmap disposition and unblock audit

Plan 317 is blocked with its unmet provider work transferred to Plan 318, which explicitly owns the normal-daemon single-owner composition, validated store handoff, exact-three delivery, pool consumers, and readiness gates. Plan 318 is registered ready because its Plan 314/315 and ADR 0030 dependencies are closed and the blocked predecessor records are present. Plan 316 remains blocked pending that provider; Plan 311 remains blocked pending Plan 316. Plan 313 was already ready after Plan 312 and remains ready; this closure does not satisfy or alter its dependencies. Plan 308 remains independently blocked on controlled ordinary-HTTP topology evidence. No previously registered plan became ready.

Disposition: Plan 317 is blocked; Plan 318 is registered ready as the corrective replacement. Reassess Plan 316 only after Plan 318 passes.
