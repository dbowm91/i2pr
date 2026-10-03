# Plan 318 closure — normal-daemon single-owner Destination-group product

Status: `passed-normal-daemon-single-owner-group-product-readiness-gated`

Implementation commits: `8d114f9` (`feat(daemon): compose destination groups over normal owner`), `0ec0639` and `d61a98a` (`fix(daemon): bound group startup receive lifecycle`).

## Requirement-to-evidence matrix

| Requirement | Result |
|---|---|
| One normal SSU2 socket owner and one inbound receiver | `run_daemon` passes the already-started `Ssu2DaemonHandle` into `ServiceProduct::start_over_existing_daemon`; `NormalSsu2Owner` wraps either that handle or the group product containing it. The production constructor does not start `Ssu2DaemonService`, allocate a test scope, or start a second receiver. `daemon_graph_registers_enabled_groups_with_bootstrap_provider` verifies the enabled normal graph registers the existing `ssu2-router` owner. Runtime and service-tunnel boundary checks pass. |
| Bounded validated bootstrap handoff | `Bootstrap::validated_router_info_snapshot` caps the copied, validated store at 256 records; the service product seeds its authoritative group store through `DestinationTunnelCoordinator::insert_validated_router_info` and the store's normal capacity/conflict outcomes. `group_provider_snapshot_is_bounded_and_contains_only_validated_records` verifies the cap with 257 validated records. |
| Plan 314 selection and selected-peer transport | Group builds project candidates only from the validated group store, preserve the exact-three request contract, resolve the selected first-hop endpoint and keys from that same validated record, restrict the selected dial to loopback, and dial through the existing owner. The workspace suite includes the Plan 314 selector and short-build trajectory tests; Plan 314 evidence guard passes. |
| Plan 315 pool and Destination consumers | Every configured runtime is provisioned before its supervisor starts. The canonical Plan 315 group pool remains authoritative for inbound/outbound material, LeaseSet sources, data, lookup, and publication. The workspace suite includes Plan 315 pool, lease refresh, consumer, expiry, failure, and persistence tests; Plan 315 evidence guard passes. |
| Inbound dispatch remains intact during startup | Messages read while provisioning waits for build replies are placed in an ordered FIFO and replayed through the normal daemon pump, which still performs floodfill handling, group processing, and router dispatch. Replay is capped at 1,024 messages and 8 MiB; overflow fails startup instead of discarding router traffic. `startup_replay_preserves_order_and_fails_closed_at_byte_bound` verifies order, accounting, and byte-cap rejection. `poll_inbound` uses the same queue for the qualification adapter. |
| Readiness and activation gates | Enabled tunnels require a bootstrap provider, enabled loopback SSU2, and `[service_tunnels].enabled = true`. The manager prepares runtimes without starting their supervisors; startup requires each group to meet its usable inbound-pool minimum before supervisors start and before SSU2 readiness is signalled. The graph registration test covers the enabled provider path; `daemon_graph_requires_the_service_tunnel_subsystem_switch` covers the configuration gate; `normal_owner_fails_closed_on_empty_validated_store_and_releases_listener` verifies candidate scarcity fails and releases the staged client listener. |
| Cancellation, bounds, restart, and cleanup | `wait_for_startup_inbound` selects cancellation against the bounded inbound wait; `startup_receive_is_interruptible_by_cancellation` verifies the cancelled path. The bootstrap snapshot, replay FIFO, configured NetDB store, Plan 314 coordinator, and Plan 315 per-group pool all retain explicit bounds. Persistent group identity reload remains covered by Plan 315's restart test; pool secrets are rebuilt from fresh RouterInfo material on product startup. Failure paths stop the shared SSU2 owner, cancel the service token, shut down staged manager state, and close the passed child scope. |
| Architecture and operational documentation | Updated daemon, runtime, and service-tunnel deep-dives with the owner flow, validated-store handoff, replay bounds, readiness behavior, startup failure gates, and loopback-only scope. No public-network or anonymity claim was added. |

## Verification

Local commands and outcomes:

- `cargo fmt --all --check` — passed after final source changes.
- `cargo check --locked --workspace --all-targets` — passed after final source changes.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` — 3,665 passed, 35 ignored across 132 suites. The ignored rows remain environment-gated external lanes.
- `cargo test --locked -p i2pr-daemon --lib -- --test-threads=1` — 434 passed after the final graph, replay, and cancellation tests.
- `cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1` — 100 passed, 1 ignored across 5 suites.
- `cargo test --locked -p i2pr-daemon --test service_tunnels_plan212_router_backed_product -- --test-threads=1` — 9 passed, 1 ignored.
- `cargo test --locked -p i2pr-daemon --test service_tunnels_application_product_only_remote_qualification -- --test-threads=1` — 21 passed, 1 ignored.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — passed with no issues after the lint findings were fixed.
- `RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps` — passed.
- `cargo test --locked --workspace --doc` — passed; 0 doctests across 18 suites.
- `bash scripts/check-dependency-direction.sh`, `bash scripts/check-runtime-boundaries.sh`, `bash scripts/check-service-tunnel-boundaries.sh`, and `bash scripts/check-service-anonymity-boundaries.sh` — passed.
- `bash scripts/check-destination-tunnel-evidence.sh`, `bash scripts/check-exploratory-tunnel-evidence.sh`, and `bash scripts/check-service-tunnel-acceptance-evidence.sh` — passed.
- `bash scripts/check-fixture-manifest.sh`, `bash scripts/check-ntcp2-vectors.sh`, `bash scripts/check-ssu2-vectors.sh`, and `bash scripts/check-i2cp-vectors.sh` — passed.
- NTCP2 interoperability, constrained-host, M11 transit boundary/qualification, SAM, SSU2, I2CP, service-tunnel, NetDB, Streaming, and M6 mixed-router evidence checks — passed. The M6 checker emitted its existing unbound-label warnings while exiting successfully.
- `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` — 18 passed.
- `cargo deny check advisories bans sources` — advisories, bans, and sources passed; existing duplicate `block-buffer` and `windows-sys` lockfile warnings remain.
- `git diff --check` — passed.

The first workspace Clippy run reported five issues in the new owner/product code (three needless borrows, one identity conversion, and the enum's large variant). Those were corrected in `0ec0639`; the final workspace Clippy run is clean. The full workspace test ran before the final lint-only and additional-regression-test commits; the current daemon library suite, workspace check, Clippy, format check, focused cancellation/replay/graph regressions, and both qualification test binaries passed after those commits.

## Compatibility, security, and limitations

- No dependency, configuration schema, persisted identity format, or wire behavior changed. No production identity is cloned or minted for a second transport owner.
- SSU2 remains loopback-only, non-advertised, and disabled by default. Explicit group configuration fails closed if its source, peers, selected endpoint, build material, or usable pool is insufficient.
- Staged listener sockets are not assigned service supervisors until pool readiness succeeds; startup failure tears them down. Router/floodfill dispatch remains in the sole normal inbound pump, including messages consumed during group provisioning.
- Tests prove the local composition/selection/pool contracts and their normal graph registration. No external multi-router topology, public-network reachability, deployed-path independence, global-observer resistance, or production anonymity claim is made. The ignored i2pd qualification rows were not run.
- No critical, high, medium, or low implementation findings remain. The retained external-topology limitation is outside Plan 318's local owner/readiness scope.

## Unblock audit and roadmap disposition

Plan 316's provider gap is closed. Its other hard dependencies are satisfied: Plan 315 passed, ADR 0030 is accepted, and Plan 311's blocked closure is recorded. Plan 316 is moved to `ready` through the accompanying status amendment; Plan 311 remains blocked pending Plan 316. Plan 313 remains independently ready after Plan 312. Plan 308 remains independently blocked on controlled ordinary-HTTP topology evidence. Plan 317 remains the immutable blocked record for the incompatible qualification-owner approach and is superseded for provider work by Plan 318. No other eligible plan changes state.

Disposition: Plan 318 passed. Plan 316 is ready to resume; Plan 311 remains blocked behind its lifecycle acceptance.
