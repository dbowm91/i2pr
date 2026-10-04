# Plan 311 status — service startup separation and graceful drain

Status: `passed-service-startup-separation-and-graceful-drain-lifecycle-acceptance`

Date: 2026-10-04

Plan of record: [`311-service-lifecycle-startup-and-graceful-drain.md`](../../implementation/anonymity/311-service-lifecycle-startup-and-graceful-drain.md).

Historical blocked authority: [`311-status.md`](311-status.md).

Unblock amendment: [`311-unblock-amendment-plan316-passed.md`](311-unblock-amendment-plan316-passed.md).

Implementation commit: `299cad11c77e83674a771c9a52cb1c3ec4d5e845` (`feat(daemon): report coarse service lifecycle status`).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Router and usable-group readiness gates service activation | `ServiceProduct::start_over_existing_daemon` provisions every enabled group and requires a non-empty ready group set before starting service supervisors; `register_ssu2_service` signals runtime readiness only after the product reports router/group readiness. Plan 318's production-provider closure records this composition. | Passed; no service supervisor starts from router identity or elapsed time alone. |
| Initial inbound-first pool sequence | Production provisioning waits one cancellable second before outbound submission after the inbound minimum is established; `wait_for_outbound_pool_start` uses Tokio time. | Passed; manual-time test checks 999 ms remains staged, then activates at 1 s and checks cancellation. |
| Graceful shutdown stops admission and replacement/publication while preserving existing work | `advance_destination_pools_at` stops admission, cancels pending builds, clears pending publication retries, and skips LeaseSet refresh/replenishment once retiring; `ServiceTunnelManager::stop_admission` cancels admission only, while the Plan 316 regression proves active runtime work survives until hard shutdown. | Passed; existing server leases and streams remain until their natural deadlines or bounded cap. |
| Natural expiry and bounded retirement | `RetirementState` caps the natural lease deadline at one 10-minute tunnel lifetime and imposes an 11-minute hard maximum; terminal status causes `run_daemon` to begin supervisor shutdown. | Passed; deterministic trace covers lease expiry, a still-open connection, later drain, and hard-cap completion. |
| Zero groups, hard bypass, and hard escalation | `RetirementState::begin` drains an empty group set immediately; `request_hard` upgrades a graceful command and the daemon's second interrupt routes directly to supervisor shutdown. | Passed; focused tests cover zero groups, hard bypass, and graceful-to-hard escalation. |
| Coarse local lifecycle status without identities | Lifecycle transitions log only one of the fixed local phase labels and a bounded remaining-time bucket. No peer, Destination, address, or tunnel identifiers enter the status event. | Passed; repeated equal status is suppressed and unit coverage checks the fixed labels. |
| Restart and identity recovery | Persistent server Destination behavior remains covered by the existing service-tunnel restart tests; each new normal-daemon group product re-runs the validated readiness/provisioning gate before service activation. | Passed for persistent identity and startup gate; no shutdown timer or retirement state is persisted across process restart. |

## Verification

Local verification on the implementation head:

- `cargo fmt --all --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked -p i2pr-daemon service_lifecycle::tests --lib -- --test-threads=1` — 6 passed, 0 failed.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` — passed; external/reference tests remained ignored by their ordinary-run contract.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — passed.
- `RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps` — passed.
- `cargo test --locked --workspace --doc` — passed.
- Dependency direction, runtime, service-tunnel, fixture, NTCP2/SSU2/I2CP vector, NTCP2 interoperability, constrained-host, M11 boundary/evidence, SAM/SSU2/I2CP/service-tunnel acceptance, exploratory tunnel, NetDB, destination tunnel, and streaming evidence scripts — passed.
- `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` — passed.
- `cargo deny check advisories bans sources` — passed. Cargo deny reported the repository's existing duplicate `windows-sys` lock entries as warnings; advisories, bans, and sources succeeded.
- `git diff --check` — passed.

The mixed-router acceptance-evidence checker exited successfully but printed existing warnings that some guarded M6 labels are not bound by its legacy runner. This Plan311 change does not edit or rely on those rows.

## Compatibility, security, and limitations

No configuration migration or dependency change. Hard/emergency shutdown remains independent of lifecycle smoothing and does not wait for the long drain. Lifecycle status is local and deliberately coarse. The 10-minute retirement target and 11-minute cap are i2pr policy choices; Java's router-level 0–11 minute graceful wait is only a lifecycle reference, not the source of an i2pr service-tunnel timer. This work reduces a simple process-edge correlation and does not establish resistance to a global timing adversary or an external anonymity claim.

| Severity | Finding |
|---|---|
| Critical | None. |
| High | None. |
| Medium | No global-timing or externally measured anonymity property is claimed; exact external lifecycle observation is outside Plan311's local deterministic acceptance. |
| Low | None. |

## Unblock audit and roadmap disposition

Plan311's hard dependencies are closed: Plan309 passed, Plan315 passed, ADR0030 is accepted, and Plan316 passed over the normal-daemon provider. The original blocked closure remains unchanged as historical evidence; this record supersedes its current status.

No registered plan becomes newly ready. Plan308 remains independently blocked on controlled ordinary-HTTP topology evidence. Plan313 remains ready after Plan312 and still needs its measured Streaming convergence work. The future integrated anonymity successor remains gated on Plans308 and 313; it has no registered implementation plan to activate. Plans307, 309, 312, 314–316, and 318 retain their recorded statuses. No M12/mainline readiness or production-anonymity claim changes.

Disposition: Plan311 passed its local lifecycle acceptance target and is closed.
