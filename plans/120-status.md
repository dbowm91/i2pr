# Plan 120 closure — Milestone 6 destination lifecycle and dedicated tunnel pools

## Status

- **Passed** as `passed-destination-lifecycle-and-pools`.
- Date: 2026-08-19.
- Plan of record: [`plans/120-m6-destination-lifecycle-and-tunnel-pools.md`](120-m6-destination-lifecycle-and-tunnel-pools.md).
- Parent roadmap: [`plans/118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md).
- Source commit: the worktree commit that lands this closure.

## Outcome

Plan 120 lands the first `i2pr-client` destination runtime. The crate
implements every Plan 120 §2–§13 acceptance bullet locally and the
`plan_120_deterministic_local_trajectory` integration test exercises the
full production-seam trajectory (build → reach `Established` → admit real
`EstablishedMaterial` → derive Lease2 entries → build and sign LeaseSet2 →
self-validate through `i2pr-netdb` → advance time → evict → replace →
refresh → shut down).

The next executable plan is **Plan 121** (ECIES-X25519-AEAD-Ratchet
Garlic session layer) under the Milestone 6 router-construction roadmap.
Garlic encryption, remote destination routing, and the streaming protocol
remain deferred to Plans 121/122/123; no NTCP2/SSU2 transport or
public-network behavior is added by Plan 120.

## Code surface

- `crates/i2pr-client/` — new workspace crate.
  - `src/lib.rs` — facade and re-exports.
  - `src/config.rs` — bounded `DestinationConfig` and `RegistryConfig`
    with centralized resource defaults (`MAX_DESTINATION_INBOUND`,
    `MAX_DESTINATION_OUTBOUND`, `MAX_PENDING_DESTINATION_MESSAGES`,
    `MAX_PENDING_DESTINATION_BYTES`, `MAX_AGGREGATE_COMMAND_QUEUE_DEPTH`,
    `MAX_LEASE_PUBLICATION_MARGIN_SECONDS`,
    `MAX_LEASE_ROTATION_MARGIN_SECONDS`).
  - `src/identity.rs` — `DestinationIdentity`, `DestinationId`; the
    identity owner is non-`Clone`, non-`Debug` (redacted), and holds
    independent Ed25519 signing and X25519 static keys.
  - `src/pool.rs` — `DestinationTunnelPool` that wraps
    `i2pr_tunnel::BoundedTunnelPool` (= `ExploratoryPool`) without
    forking tunnel cryptography or the data plane. Direction-mismatched
    registration and the bounded failure threshold are enforced at this
    boundary.
  - `src/leaseset.rs` — `build_signed_lease_set2`,
    `LeaseSetLifecycle`, `LocalLeaseSet`, `LeaseSetDecision`,
    `LeaseSetRotationCause`, `LeaseSetSummary`. The signer is the
    destination's Ed25519 signing key; the validated record flows
    through the Plan 119 `ValidatedLeaseSet2` self-check.
  - `src/message.rs` — `DestinationPayload`, `BoundedPayloadQueue`,
    `RoutingUnavailable` (`AwaitingGarlicSessionLayer`,
    `AwaitingDestinationRouting`).
  - `src/registry.rs` — `DestinationRuntime`, `DestinationState`,
    `DestinationHandle`, `DestinationCommand`, `DestinationEvent`,
    `DestinationRegistry`.
  - `src/testing.rs` — deterministic `established_inbound(seed)` /
    `established_outbound(seed)` fixtures driving the real
    `ShortBuildStateMachine` to `Established`.
  - `tests/plan120_trajectory.rs` — Plan 120 §12 deterministic local
    trajectory.
- `crates/i2pr-tunnel/src/lib.rs` — adds the `BoundedTunnelPool` /
  `BoundedTunnelPoolConfig` type aliases that the destination pool
  reuses. No production code is removed or reworked.
- `crates/i2pr-proto/src/common/hash.rs` — adds `Hash::copy()` so
  callers can return an owned hash from a borrowed accessor without
  dereferencing the inner array directly.
- `scripts/check-dependency-direction.sh` — encodes the
  `i2pr-client → {i2pr-core, i2pr-crypto, i2pr-netdb, i2pr-proto, i2pr-tunnel}`
  allowlist.
- `Cargo.toml` (workspace) — adds `crates/i2pr-client` to the
  workspace members.

## Acceptance criteria

| Plan 120 §16 bullet | Verified by |
| --- | --- |
| `crates/i2pr-client` exists and follows the intended dependency direction. | `scripts/check-dependency-direction.sh` (workspace) and the new `client.destination-lifecycle` surface in `specs/support.toml`. |
| A local destination owns independent signing and static X25519 secrets. | `two_destinations_are_independent` and `deterministic_reconstruction_matches_generated_identity` in `identity.rs`. |
| Secret-bearing destination values do not reveal bytes via `Debug`. | `debug_never_reveals_secret_bytes` in `identity.rs`. |
| At least two local destinations can coexist without shared secret/session or tunnel-pool state. | `two_destinations_are_independent` plus the registry `one_destination_failure_does_not_mutate_the_other` test. |
| The destination registry is explicitly bounded. | `registry_configuration_is_bounded`, `registry_is_bounded_and_rejects_duplicates`, `aggregate_command_queue_depth_is_bounded`. |
| Destination inbound/outbound tunnel counts and build/replacement work are bounded. | `configuration_rejects_zero_and_excess_values`, `build_failures_are_bounded_and_reset_on_success`, `note_build_failure` semantics. |
| Destination pools consume real one-shot `EstablishedMaterial`. | `established_inbound`/`established_outbound` fixtures in `testing.rs` and the `plan_120_deterministic_local_trajectory` integration test. |
| No production placeholder established tunnel path is introduced. | `i2pr-tunnel` is unchanged; the placeholder `register_inbound` / `register_outbound` paths in `ExploratoryPool` remain `#[cfg(test)]`. |
| A usable inbound tunnel maps to the correct Lease2 gateway hash and receive tunnel ID. | `inbound_material_yields_gateway_and_receive_tunnel_id` and the trajectory test. |
| Advertised Lease2 expiry never exceeds actual tunnel usability. | `advertised_expiry_never_exceeds_tunnel_expiry`, `expired_or_failed_tunnel_is_not_advertised`. |
| A destination constructs a canonical Standard LeaseSet2 containing its X25519 type-4 public key. | `first_usable_inbound_set_generates_and_self_validates_ls2`. |
| LS2 is signed with the destination signing private key and self-validates through `i2pr-netdb`. | Same test plus `signature_preimage_uses_the_lease_set2_domain_byte` and `verify_lease_set2` cross-check. |
| LS2 `published` values advance deterministically when replacement is required. | `replacement_has_monotonic_published_time`. |
| Tunnel failure/expiry rotates destination LS2 state without leaking stale leases. | `tunnel_expiry_degrades_and_withdraws_the_lease_set`, `approaching_expiry_rotates_the_lease_set`, `replacement_tunnel_replaces_the_advertised_lease_source`. |
| Destination shutdown releases pool/registry/message state. | `shutdown_releases_pool_registry_and_message_state`, `registry_removal_drops_destination_state`. |
| A deterministic production-seam integration test covers startup → real tunnels → LS2 → rotation → shutdown. | `plan_120_deterministic_local_trajectory` in `crates/i2pr-client/tests/plan120_trajectory.rs`. |
| No Garlic encryption, plaintext tunnel-delivery shortcut, SAM, I2CP, streaming, or external transport activation is added. | Crate has no `tokio`, no `std::net`/`std::fs`, no SAM/I2CP imports, no `send` short-circuit in `enqueue_outbound` (returns `RoutingUnavailable::AwaitingGarlicSessionLayer`). |
| Workspace validation is green. | See "Validation commands" below. |

## Validation commands

Run from the repo root on the closure commit:

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p i2pr-client --all-targets
cargo test -p i2pr-tunnel --all-targets
cargo test -p i2pr-netdb --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-multipass-interop-boundary.sh
```

The pre-existing Plan 046 rootless baseline failure
(`tests/integration/ntcp2/harness/rootless_supervisor.py` retired by
Plan 099) is unrelated to Plan 120 and was not addressed.

## Handoff

```text
plan_120                                  = passed-destination-lifecycle-and-pools
local_destination                        = keys+tunnels+signed-ls2-ready
milestone                                = 6 (router construction resumed)
inbound_short_build                      = locally-reference-compatible (Plan 113, unchanged)
outbound_short_build                     = locally-conformant-pre-delivery (Plan 112, unchanged)
ntcp2                                    = experimental-non-advertised
normal_daemon_ntcp2                      = disabled-after-plan101
external_netdb_over_ntcp2                = blocked
next                                     = plans/121-m6-ecies-garlic-session-layer.md
```
