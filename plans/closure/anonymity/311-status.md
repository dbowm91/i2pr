# Plan 311 status — service startup separation and graceful drain

Status: `blocked-normal-daemon-lacks-group-lifecycle-and-pre-shutdown-owner`

Plan of record: [`311-service-lifecycle-startup-and-graceful-drain.md`](../../implementation/anonymity/311-service-lifecycle-startup-and-graceful-drain.md).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Readiness-gated group activation | `register_service_tunnel_manager` in `crates/i2pr-daemon/src/service_tunnels.rs` prepares and starts the normal configured M10 manager without a router-readiness handle or Plan 315 group-pool owner. `ServiceProduct::start` does provision before listeners, but repository call-site search finds it only in qualification/test drivers, not normal daemon graph composition. | Not integrated into the daemon product. |
| Distinct graceful and hard shutdown | `run_daemon` in `crates/i2pr-daemon/src/lib.rs` turns Ctrl-C directly into `SupervisorHandle::shutdown(Requested)`. `Supervisor::shutdown` in `crates/i2pr-runtime/src/supervisor.rs` immediately cancels root and active service tokens before joining. | No service pre-drain phase or upgrade path exists. |
| Bounded natural group retirement | `Supervisor::MAX_SHUTDOWN_DEADLINE` is 30 seconds. Current service-loop cancellation also governs connection tasks. The Plan 315 pool advance/replenish path is only driven through the separate `ServiceProduct::poll_inbound` composition. | Cannot safely retain daemon transport and existing group streams for the required natural lease/tunnel window. |
| No refresh/replacement after Retiring; hard bypass; coarse status | No daemon-owned group lifecycle state or pre-shutdown control surface exists to own these transitions. | Not implementable at the current composition boundary. |

## Verification and implementation

- Read-only call-graph audit of `run_daemon`, `register_service_tunnel_manager`, `ServiceProduct`, and `Supervisor::shutdown` — confirmed the missing daemon-owned group lifecycle and pre-shutdown integration.
- `rg` call-site audit for `ServiceProduct::start` — only qualification/test drivers construct the Plan 315 service product; the normal daemon service graph does not.
- No production source, tests, or dependencies changed for Plan 311. Plan-required lifecycle tests were not run because no safe runtime consumer exists to exercise the required behavior.
- `git diff --check` — passed for the combined Plan 311/312 documentation and implementation changes.

## Compatibility, security, and limitations

No behavior or configuration changed. The blocker is architectural, not evidence that graceful service retirement is unsafe in principle. Adding a detached lifecycle state machine to the test-only `ServiceProduct` would create infrastructure without a normal daemon consumer and would not satisfy Plan 311's capability acceptance criteria.

| Severity | Finding |
|---|---|
| Critical | None. |
| High | None. |
| Medium | The normal daemon cannot readiness-gate, naturally retire, or preserve Plan 315 group-owned service state during graceful shutdown. No service-uptime correlation reduction is established. |
| Low | None. |

## Unblock audit and roadmap disposition

Plan 316 is registered as the required daemon/runtime integration corrective and is dependency-ready from Plans 315 and ADR 0030. Plan 312 is independent of Plan 311 and has since passed its pinned-i2pd directional handshake baseline; Plan 313 is ready. Plan 308 remains independently blocked on its controlled ordinary-HTTP topology. Plan 310 remains an immutable blocked historical record corrected by Plans 314–315.

Disposition: Plan 311 is blocked at the normal daemon composition boundary. Resume its lifecycle acceptance after Plan 316 passes; continue Plan 312 independently.
