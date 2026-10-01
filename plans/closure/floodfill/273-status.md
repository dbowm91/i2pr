# Plan 273 status — passed bounded DatabaseStore service

Status: **passed-m12-bounded-databasestore-service**

Implementation commit: recorded in Git history for this closure.

## Requirement-to-evidence

| Requirement | Evidence |
|---|---|
| Explicit disabled/serving gate, authenticated direct/router-tunnel/client-tunnel classification, caller wall/monotonic time | `crates/i2pr-netdb/src/floodfill_service.rs` (`FloodfillRole`, `FloodfillIngress`, `FloodfillTime`) |
| One validated store path for RouterInfo and types 1/3/7; unsupported type 5 remains rejected | `FloodfillStoreService::validate` and `all_plan_272_lease_record_types_use_the_single_validating_store_path` |
| Validation before server authority mutation; typed stale/conflict/capacity/idempotent outcomes | `floodfill_service.rs`; `server_store.rs` |
| Reply-token acknowledgment, route tuple copied, zero-token no-ack/no-reflood, direct-publisher-only replication effect | service tests `direct_publisher_store_is_validated_acknowledged_and_offered_for_replication`, `zero_token_replica_is_stored_without_ack_or_reflood`, `router_tunnel_ack_route_is_copied_but_never_replication_authority` |
| Bounded global/per-source/per-key request windows, byte ceilings, crypto-work ceiling, caller-time reset | `FloodfillStorePolicy`, bounded throttle maps/counters, and `source_and_key_windows_throttle_then_reset_on_supplied_time`, `byte_and_crypto_windows_are_independent_hard_ceilings` |
| Self-key collision and disabled role fail closed; privacy-safe diagnostics | `disabled_role_and_self_key_collision_do_not_mutate_storage`; redacted `FloodfillIngress` and `RecordId` debug; aggregate-only `FloodfillStoreStats` |
| Hidden RouterInfo and unpublished/blinded LeaseSets do not become answer/replication material | `ServerNetDb::router_info_for_answer` and `ServerNetDb::may_replicate` policy checks |
| Runtime/dependency boundary remains enforced | `scripts/check-m12-floodfill-boundaries.sh`, dependency direction and runtime boundary scripts |
| No normal-daemon behavior or support advertisement enabled | no daemon composition/config/support capability change; support matrix remains non-advertised |

## Verification

Commands run locally on the implementation checkout:

- `rtk cargo fmt --all --check` — passed.
- `rtk cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1` — 157 passed.
- `rtk cargo test --locked -p i2pr-proto --all-targets` — 154 passed.
- `rtk cargo clippy --locked -p i2pr-netdb --all-targets -- -D warnings` — passed.
- `rtk cargo test --locked -p i2pr-daemon --test netdb_integration -- --test-threads=1` — 38 passed.
- `rtk bash scripts/check-dependency-direction.sh` — passed.
- `rtk bash scripts/check-runtime-boundaries.sh` — passed.
- `rtk bash scripts/check-m12-floodfill-boundaries.sh` — passed.

## Security, operational limits, and findings

The service is synchronous and runtime-neutral. Source/key throttle tables have hard cardinality
ceilings and expire by the supplied monotonic clock. Record validation and server-store admission
are bounded; counters omit peer and key identifiers. DeliveryStatus and replication are effects
only; this code does not dispatch them. Type 5 remains unsupported. There is no DatabaseLookup
service, replication transport, persistence, daemon lifecycle activation, or `caps=f` claim.

No dependency was added. No critical/high findings remain open. Medium/low findings: none
recorded.

## Unblock audit and roadmap disposition

Plan 274's hard dependency is Plan 273. It now has a stable query and outcome contract over
`ServerNetDb` and may move to ready. Plans 275–279 remain blocked in dependency order. Plan 280
remains stopped pending a reviewed I2P-compatible Red25519 provider; Plan 281's type-5 deferral
continues to govern. No other registered blocked plan is unblocked by this closure.
