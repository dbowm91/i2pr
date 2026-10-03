# Plan 315 closure — Destination-group pool ownership and Destination operations

Status: passed-plan310-destination-group-pool-ownership-and-destination-operations

Implementation commits: `cf697de` (`feat(anonymity): integrate destination group pools`); `612dd0e` (`test(anonymity): cover pool lease refresh and cancellation`).

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| One canonical pool per Destination group | The manager's `DestinationRuntime` owns the `DestinationTunnelPool`; `ServiceProduct` keeps one runtime per `DestinationId` and deduplicates provisioning by id. Shared server group and distinct group tests retain one or separate canonical pools as configured. |
| Only Plan 314 established material enters a group pool | The product takes the one-shot Plan 314 established-material handoff, registers it into the owning `DestinationRuntime`, then transfers outbound secret role material to a slot-keyed data-plane projection. Pool registration remains the authority for direction, capacity, expiry, and inbound LeaseSet metadata. |
| Target replenishment is bounded and does not overshoot | Each lifecycle poll computes per-direction deficits after live registrations and pending attempts. Pending counts are scoped by Destination and direction; submissions also respect the group's build concurrency and global `MAX_PENDING_BUILDS`. Peer selection remains the exact-three Plan 314 path with no shorter fallback. |
| Expiry, replacement failure, and pause behavior | The poll advances each group pool from its injected millisecond clock, removes expired inbound owners/outbound projections by canonical pool slot, refreshes usable state, and then reevaluates deficits even when no LeaseSet/owner projection changed. Destination failures increment only the owning pool's threshold; exploratory failure counting remains operational and resets on a successful establishment. The group pool test covers two inbound and two outbound paths, capacity rejection, LeaseSet sources, and deterministic expiry; the explicit failure-counter test covers pause and successful reset. |
| Pending work is coalesced and cancelled by owner | The coordinator exposes pending counts by group and direction and cancels all pending attempts for a removed generation/group. The deterministic cancellation test leaves another group's attempt intact and verifies pending capacity is released. |
| LeaseSet contents and publication gate | Signed LeaseSets are built from all current usable `InboundLeaseSource` values. The Plan 315 snapshot test validates two current leases and exact receive ids, then verifies an empty/below-minimum snapshot makes router-backed state unusable and removes stale receive-owner ids. Server publication is scheduled only for a changed usable LeaseSet, with bounded retry; no publication is attempted below minimum. |
| Lookup, publication, and service send use the group pool | All three composition paths borrow through the same rotating `router_outbound_role_ref` selector. The role projection test proves selection rotates across live roles and removing one pool slot removes only its corresponding role. Operations fail closed when router-backed state or a usable outbound role is absent. |
| Inbound ownership tracks live group entries | Startup uses the exact creator receive id and validates its inbound gateway against the selected path. Active registration installs the matching service owner; eviction removes both the coordinator inbound registration and manager owner mapping. Projection refresh derives ids from remaining canonical inbound registrations. |
| Shared groups, distinct groups, identity persistence, and restart behavior | `shared_server_group_has_one_persistent_destination_and_two_ports` and `distinct_destination_groups_have_distinct_canonical_pools` pass in the daemon suite; Plan 309 restart coverage remains intact. Shared service specs reuse one group identity/pool and distinct groups keep separate pools. This plan adds no persistent tunnel secret or identity format; server group identity persistence remains owned by Plan 309, while ephemeral pool material is rebuilt on restart. |
| No exploratory/direct/other-group fallback | Destination work is routed through its scoped build outcome and exact-three selector. Consumer composition returns an error when group-backed state is unavailable; no exploratory pool, LocalZeroHop, other group, or shorter path is selected. Service-anonymity and service-tunnel boundary checkers pass. |

## Verification

Local commands and outcomes:

- `cargo fmt --all --check` — passed after the final source changes.
- `cargo check --locked --workspace --all-targets` — passed after the final source changes.
- `cargo test --locked -p i2pr-client --all-targets -- --test-threads=1` — 209 passed.
- `cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1` — 400 passed.
- `cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1` — 176 passed.
- `cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1` — 1,361 passed, 33 ignored across 60 suites.
- `cargo test --locked -p i2pr-service-tunnels --all-targets -- --test-threads=1` — 228 passed.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` — 3,302 passed, 34 ignored across 110 suites.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — passed with no issues after the clippy findings were fixed.
- `RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps` — passed.
- `cargo test --locked --workspace --doc` — passed; 0 doctests across 16 suites.
- `bash scripts/check-service-anonymity-boundaries.sh` — passed.
- `bash scripts/check-service-tunnel-boundaries.sh` — passed.
- `bash scripts/check-runtime-boundaries.sh` — passed.
- `bash scripts/check-dependency-direction.sh` — passed.
- `git diff --check` — passed.

Focused final regressions:

- `cargo test --locked -p i2pr-client --lib group_pool_holds_two_inbound_and_outbound_paths_then_expires_them -- --test-threads=1` — 1 passed.
- `cargo test --locked -p i2pr-tunnel --lib explicit_build_failures_pause_and_success_resets_the_counter -- --test-threads=1` — 1 passed.
- `cargo test --locked -p i2pr-daemon --lib plan315_group_role_projection_rotates_and_removes_by_pool_slot -- --test-threads=1` — 1 passed.
- `cargo test --locked -p i2pr-daemon --lib plan315_pool_snapshot_uses_only_current_inbound_leases_and_fails_closed_below_minimum -- --test-threads=1` — 1 passed.
- `cargo test --locked -p i2pr-daemon --lib destination_build_failures_are_scoped_and_do_not_pause_exploratory_pool -- --test-threads=1` — 1 passed.
- `cargo test --locked -p i2pr-daemon --lib destination_pending_builds_are_counted_by_direction_and_cancelled_by_owner -- --test-threads=1` — 1 passed.
- `cargo test --locked -p i2pr-daemon --lib -- --test-threads=1` — 316 passed after all Plan 315 regression tests were added.

The full workspace suite ran before the two final focused daemon regression tests were added and before a semantics-preserving clippy cleanup. Both new tests passed individually; the final daemon library suite passed with all 316 tests, and final workspace check/clippy passed after those edits. The only earlier verification failure was a daemon regression test exposing that exploratory failures no longer advanced the shared pool counter; the counter update was restored and the complete daemon all-target rerun passed. No external i2pd or multi-router execution was required or claimed.

## Compatibility, security, and limitations

- No dependency, wire format, persisted identity format, or user configuration changed.
- Established secret material is transferred through the one-shot handoff and activated once; the canonical pool retains public registration metadata while the bridge owns the active data-plane role projection. Receive-owner ids are public tunnel identifiers and are removed with their pool registrations.
- There is no new identity minting path. Shared service groups continue to intentionally share one Destination; distinct groups retain separate identities and pools.
- Replacement and global pending work remain bounded. A failed group build degrades and can pause only that group's replacement policy; router/exploratory failures continue to use their own counter.
- Pool advancement, LeaseSet refresh, and replenish run from the product's `poll_inbound` lifecycle. Plan 311 owns readiness gates and graceful retirement semantics.
- Evidence proves the local exact-three build and group-pool consumer contracts. It does not prove live multi-router routing, deployed-path diversity, resistance to a global observer, or production anonymity. No external reference lane was run.

## Findings

- Critical: none.
- High: none.
- Medium: external multi-router behavior remains unqualified and is not a Plan 315 acceptance requirement; Plan 312 remains the pinned-i2pd evidence plan.
- Low: none.

## Unblock audit and roadmap disposition

Plan 315's hard dependencies are satisfied: Plan 314 passed, Plan 309 passed, and ADR 0030 is accepted. Plans 311 and 312 both list Plan 315 as their remaining hard dependency; Plan 311's other dependencies (Plan 309 and ADR 0030) are closed, and Plan 312's ADR and exact i2pd 2.61.0 pin are stable. Both plans are moved to `ready`. Plan 313 remains blocked on Plan 312; Plan 308 remains independently blocked on its controlled ordinary-HTTP topology. Plan 310 remains an immutable historical blocked record, with its corrective production requirements passed by Plans 314 and 315. No other plan can be unblocked by this closure.

Disposition: Plan 315 passed. Plans 311 and 312 are ready; Plan 313 remains blocked on Plan 312.
