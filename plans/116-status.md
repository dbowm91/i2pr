# Plan 116 status: passed-final-local-closure

- Status: **passed-final-local-closure**
- Date: **2026-08-18**
- Current repository implementation floor: this commit (terminal cleanup pass).
- Substantive local data-plane implementation: `0330fb2e9e64dd0877472c930606ab4219ac18a9`.
- Terminal cleanup pass: [`116-terminal-cleanup.md`](116-terminal-cleanup.md) (defects `T1`–`T4`).
- Original plan: [`116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- Completion/correction pass: [`116-completion-correction.md`](116-completion-correction.md)
- Prior final-closure pass: [`116-final-closure.md`](116-final-closure.md)
- Handoff: [`116-handoff.md`](116-handoff.md)
- Predecessor: [`115-status.md`](115-status.md)
- Plan 117: **unblocked — ready for planning or execution**.

## Why this status supersedes the terminal-cleanup-pending record

The prior closure pass landed the major Plan 116 work (real
established material transfer, material-only pool, exact Tunnel
Message overheads, first-fragment delivery retention, exact-byte
cross-tunnel trajectories). A strict post-closure audit found four
remaining closure defects the prior pass had not resolved. The
terminal cleanup pass corrects every one of them in place.

```text
T1 exact duplicate fragments are now no-ops for memory,
   expiry, and aggregate budget (was: charged aggregate budget,
   refreshed last_touched_ms before idempotence).

T2 first-fragment delivery metadata is now part of duplicate
   identity; conflicting delivery invalidates only the affected
   partial; follow-on delivery metadata is rejected fail-closed.

T3 out-of-order full role-level fragmented trajectory now
   passes exact-byte recovery with no premature completion.

T4 plans/116-status.md and plans/116-handoff.md now agree on
   closure state and Plan 117 successor state, and the recorded
   test names match the actual implemented identifiers.
```

The terminal cleanup does not reopen any external validation and
does not change the crate's runtime-neutral / transport-neutral
contract.

## Work that remains accepted

The following surfaces remain accepted and are not touched by the
terminal cleanup:

```text
F1 state-machine material transfer                = passed-local
F2 production placeholder APIs                    = test-only / production blocked
F3 fragment sizing + boundary tests               = passed-local
F4 first-fragment delivery retention basic path   = passed-local
F5 outbound-to-inbound unfragmented trajectory    = passed-exact-bytes-local
F5 outbound-to-inbound fragmented trajectory      = passed-exact-bytes-ordered-local
F6 outbound-to-inbound fragmented out-of-order    = passed-exact-bytes-local
Tunnel Message checksum/control framing           = corrected-local
AES participant/creator transforms                = corrected-local
production IV/padding randomness                  = corrected-local
```

## Current implementation facts

### Real established material transfer

`ShortBuildStateMachine::take_established_material()` now consumes retained per-hop `LayerKeys` after a real `Established` transition and constructs the existing `EstablishedMaterial` topology. `ShortBuildRegistrar::admit_established_machine()` passes that material to the pool. The material-less legacy registrar path fails closed.

### Material-only production pool

`ExploratoryPool::register_inbound()` / `register_outbound()` and `build_placeholder_established()` are `#[cfg(test)]`. Production registration uses `register_*_with_material()`.

### Fragment boundaries

`fragment_complete_message()` uses exact LOCAL/ROUTER/TUNNEL unfragmented and first-fragment overheads plus the 7-byte follow-on overhead. Active boundary tests run generated fragment sets through `build_cells()` and parsing.

### Duplicate-accounting classification

`PartialMessage::classify()` returns a `FragmentInsertDisposition`
of `Inserted { added_bytes }` or `ExactDuplicate`. The
`BoundedReassembler::insert_with_delivery` / `insert` entry points
classify before applying the aggregate budget check, so an exact
duplicate is accepted as a no-op even when the budget is already
full. Exact duplicates never refresh `last_touched_ms` and never
charge `aggregate_bytes`.

### Delivery-metadata integrity

`ConflictingFirstMetadata` rejects a same-body first fragment whose
delivery instruction differs from the retained delivery; only the
affected partial is invalidated. `UnexpectedFollowOnDeliveryInstruction`
rejects follow-on fragments that carry a delivery instruction on
the wire (the wire format does not allow this).

### Cross-tunnel trajectories

The canonical outbound → inbound trajectory now executes:

```text
OutboundGatewayRole
 -> outbound participant
 -> OutboundEndpointRole
 -> TunnelGatewayMessage
 -> InboundGatewayRole
 -> inbound participant
 -> LocalInboundEndpointRole
 -> exact original standard I2NP bytes
```

for both small and fragmented messages. The fragmented case is now
proven twice: in canonical fragment order and with at least one
follow-on delivered before the first fragment.

## Terminal cleanup evidence

The terminal cleanup tests are recorded under the crate's existing
`cargo test` surface. Exact identifiers:

```text
fragment::tests::exact_duplicate_first_does_not_increase_retained_bytes
fragment::tests::exact_duplicate_follow_on_does_not_increase_retained_bytes
fragment::tests::exact_duplicate_at_aggregate_limit_is_accepted_as_noop
fragment::tests::exact_duplicate_does_not_refresh_expiry
fragment::tests::reassembly_completion_returns_aggregate_bytes_to_zero_after_duplicates
fragment::tests::exact_duplicate_first_with_same_delivery_is_idempotent
fragment::tests::conflicting_first_router_target_invalidates_partial
fragment::tests::conflicting_first_tunnel_id_invalidates_partial
fragment::tests::conflicting_first_tunnel_gateway_invalidates_partial
fragment::tests::unexpected_follow_on_delivery_fails_closed
roles::tests::outbound_to_inbound_fragmented_out_of_order_trajectory_exact_bytes
```

Suggested evidence commands:

```bash
cargo test --locked -p i2pr-tunnel --lib exact_duplicate -- --nocapture
cargo test --locked -p i2pr-tunnel --lib conflicting_first -- --nocapture
cargo test --locked -p i2pr-tunnel --lib unexpected_follow_on -- --nocapture
cargo test --locked -p i2pr-tunnel --lib reassembly_completion -- --nocapture
cargo test --locked -p i2pr-tunnel --lib outbound_to_inbound_fragmented -- --nocapture
cargo test --locked -p i2pr-tunnel --lib
cargo test --locked -p i2pr-tunnel --all-targets
```

## Current token table

```text
plan_115_Q0                          = passed-emissary-native-consumer
Q1_authenticated_transport          = deferred
Q2_external_return_established      = deferred
plan_116_wire_format                = corrected-local
plan_116_aes_layer                  = corrected-local
plan_116_rng                        = corrected-local
plan_116_state_machine_material     = passed-real-transfer
plan_116_pool_real_material_only    = passed-cfg-test-only-placeholders
plan_116_fragment_boundaries        = passed-exact-overheads
plan_116_fragment_delivery_state    = passed-basic-retention
plan_116_outbound_router            = passed-local
plan_116_outbound_to_inbound        = passed-exact-bytes
plan_116_fragmented_cross_tunnel    = passed-out-of-order-exact-bytes
plan_116_duplicate_accounting       = passed-noop-exact-duplicates
plan_116_duplicate_expiry           = passed-no-refresh
plan_116_first_delivery_identity    = passed-conflict-detected
plan_116                             = passed-final-local-closure
plan_117                             = unblocked-ready-for-planning
qualified_external_delivery         = blocked-on-host-execution-lane
normal_daemon_ntcp2                 = disabled-and-unenableable
ntcp2                               = experimental-non-advertised
```

## Fixed anti-loop scope

The terminal cleanup did not reopen:

```text
Emissary/i2pd/Java runtime testing
NTCP2 activation/correction
SSU2
Q1/Q2
rootless namespaces
Docker/Multipass/VMs
Python interoperability harnesses
public I2P network
NetDB live integration
Plan 117 execution
```

This was an ordinary local Rust correctness pass over `fragment.rs`,
the fragmented trajectory in `roles.rs`, and closure documentation.
