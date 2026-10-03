# Plan 316 closure — daemon-owned service-group lifecycle

Status: `passed-daemon-owned-service-group-lifecycle-integration`

Plan of record: [`316-daemon-owned-service-group-lifecycle-integration.md`](../../implementation/anonymity/316-daemon-owned-service-group-lifecycle-integration.md).

Status amendment: this closure supersedes the current disposition recorded in [`316-status.md`](316-status.md) after Plan 318 supplied the normal-daemon provider. The original blocked record and [`316-unblock-amendment-plan318-provider-passed.md`](316-unblock-amendment-plan318-provider-passed.md) remain unchanged historical evidence; this amendment records the lifecycle implementation and current passing status.

Implementation commit: `8c5127d` (`feat(daemon): add bounded service group drain`).

## Requirement-to-evidence matrix

| Requirement | Result |
|---|---|
| Normal-daemon group provider and readiness gate | Plan 318's existing single SSU2 owner remains the only transport and inbound receiver. `ServiceProduct::start_over_existing_daemon` sets lifecycle Active only after the group runtimes are prepared, usable inbound pools pass their minimum, and service supervisors start. Plan 318's closure and graph/readiness tests remain the provider evidence. Startup-time signals see Starting and request immediate shutdown. |
| Inbound-first pool startup | `provision_all_service_router_material` builds the configured inbound pool first, verifies the usable minimum, waits one second through a cancellation-aware Tokio timer, then submits the outbound builds. `outbound_start_waits_full_second_and_observes_cancellation` uses paused Tokio time to check 999 ms, 1,000 ms, and cancellation. Startup messages consumed while waiting for either build phase retain Plan 318's bounded ordered replay. |
| Admission stop is separate from established work | `ServiceRuntime` now has admission and runtime cancellation tokens. Every service profile's listener/admission loop observes the admission token. Connection tasks keep the runtime token and continue while the normal SSU2 pump dispatches inbound data. `graceful_admission_stop_preserves_runtime_until_hard_shutdown` verifies admission cancellation does not cancel existing runtime work, includes TCP/Streaming and Streamr subscriber counts in the drain total, and confirms hard shutdown then cancels runtime work. |
| Bounded retirement | Retirement waits until the latest inbound lease expiry and tracked active application/Streamr work has drained, or the 11-minute hard cap. The target is at most one 10-minute tunnel lifetime. Tokio monotonic time governs target and hard deadlines; the published lease's wall-clock expiry is converted once to a remaining duration at retirement start. `published_leases_and_connections_retire_with_target_and_hard_cap` covers expiry, open connections, and the hard cap under manual time; `zero_group_state_drains_without_waiting` proves the empty case. |
| No replacement or publication refresh while Retiring | Entry to Retiring stops admissions, cancels pending destination builds, clears publication retry work, and skips group-pool replenishment, bridge LeaseSet refresh, and publication. Inbound delivery and tunnel expiry/route cleanup continue through the normal owner. `process_inbound` no longer registers late build results after Retiring. The pool-advance guard is covered by the lifecycle transition tests and source path; Plan 315's pool-expiry and consumer tests cover the underlying expiration operations. |
| Graceful, hard, and fatal shutdown ordering | `run_daemon` owns the signal wait alongside the supervisor future. The first signal starts a drain only when lifecycle is Active; after terminal status it cancels the supervisor. A second signal requests hard shutdown immediately. Startup signals, signal-handler failure, and supervisor/fatal failure bypass the long drain. `hard_shutdown_bypasses_graceful_deadline` covers hard upgrade, and existing supervisor failure/forced-shutdown tests cover fatal teardown. |
| Restart and cleanup | Each normal-daemon startup reloads the configured persistent group identity and rebuilds fresh inbound/outbound tunnel material from validated RouterInfos. No retired leases are reused. Plan 315's persistent group identity restart test remains the storage evidence. Manager hard shutdown cancels both tokens and delivery drivers; the service child scope closes its owned loops. Existing per-connection Tokio tasks remain detached as in the pre-existing service implementation, but observe runtime cancellation and release permits/accounting when they return. Graceful completion waits for the aggregate active count to reach zero; process hard shutdown also drops the enclosing Tokio runtime. |
| Coarse local lifecycle status | The watch controller retains only one command and one status. Status values contain phase and coarse remaining-time buckets; no peer, Destination, service, or tunnel identifier is stored or logged. |
| Documentation and scope | Daemon and service-tunnel architecture pages explain signal ordering, token ownership, drain conditions, reference timing, and limits. No config, persisted identity, wire behavior, capability advertisement, or anonymity claim changed. |

## Verification

Local commands and outcomes:

- `rtk cargo fmt --all --check` — passed after final implementation changes.
- `rtk cargo check --locked --workspace --all-targets` — passed after final implementation changes.
- `rtk cargo test --locked --workspace --all-targets -- --test-threads=1` — 3,672 passed, 35 ignored across 132 suites in 722.44 seconds. This exact-head run includes the complete inbound-first startup refactor. A subsequent focused test covers the final aggregate drain counter addition for Streamr subscribers.
- `rtk cargo test --locked -p i2pr-daemon service_lifecycle::tests --lib -- --test-threads=1` — 4 passed.
- `rtk cargo test --locked -p i2pr-daemon graceful_admission_stop_preserves_runtime_until_hard_shutdown --lib -- --test-threads=1` — 1 passed after the final drain-counter change.
- `rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — passed after final implementation changes.
- `RUSTDOCFLAGS="-D warnings" rtk cargo doc --locked --workspace --no-deps` — passed.
- `rtk cargo test --locked --workspace --doc` — passed; no doctests across 18 suites.
- `rtk bash scripts/check-dependency-direction.sh`, `rtk bash scripts/check-runtime-boundaries.sh`, `rtk bash scripts/check-service-tunnel-boundaries.sh`, and `rtk bash scripts/check-service-anonymity-boundaries.sh` — passed.
- Fixture manifest and NTCP2/SSU2/I2CP vector checks — passed.
- NTCP2 interoperability, constrained-host, M11 transit boundary/qualification, SAM, SSU2, I2CP, service-tunnel, exploratory-tunnel, NetDB, Destination, Streaming, and M6 mixed-router evidence checks — passed. The Streaming checker emitted its existing unbound-label warnings and passed. M11 boundaries emitted the existing `expire(u64::MAX)` source match and passed.
- `rtk python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` — 18 passed.
- `rtk cargo deny check advisories bans sources` — advisories, bans, and sources passed; pre-existing duplicate `block-buffer` and `windows-sys` lockfile warnings remain.
- `rtk git diff --check` — passed before closure records were added.

No new dependencies were added. The focused regression after the final subscriber-count change and full workspace check/clippy/boundary checks passed; the complete serial test command immediately preceded that narrow counter-only change.

## Compatibility, security, and limitations

- Existing per-connection deadlines remain unchanged; the 10/11-minute limits apply only to normal-daemon group retirement.
- Group retirement does not keep a failed supervisor alive. Fatal/essential service failures still follow the supervisor's immediate cancellation path.
- Per-connection tasks are not individually joined by a `JoinSet`; they remain cancellation-aware detached Tokio tasks under the existing M10 service design. The aggregate connection/subscriber count prevents graceful completion while those handlers are active, and hard shutdown cancels their runtime token.
- The 11-minute hard cap is monotonic and bounded. Published lease expiry is observed from the group pool; active TCP/Streaming connections and Streamr subscribers are counted before natural drain completion.
- The change is local lifecycle policy, not a claim about hostile timing observers, global anonymity, Java I2P equivalence, public-network participation, or external multi-router reachability.
- No critical, high, medium, or low implementation findings remain.

## Unblock audit and roadmap disposition

- Plan 311's blocked closure remains immutable history. Its other dependencies are satisfied: Plan 309 and Plan 315 passed, ADR 0030 is accepted, and the blocked closure is recorded. Plan 316 closes the normal-daemon lifecycle integration prerequisite; Plan 311 moves to `ready` for lifecycle acceptance.
- Plan 313 remains independently ready after Plan 312. Plan 308 remains blocked on the controlled ordinary-HTTP topology and three-family captures. Plan 317 remains the immutable blocked qualification-owner attempt. No other plan becomes eligible from this closure.
- The future integrated anonymity successor remains gated on Plans 308, 311, and 313. This closure does not change M12/mainline authority or make an anonymity claim.

Disposition: Plan 316 passed; Plan 311 is ready to resume.
