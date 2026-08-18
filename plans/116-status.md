# Plan 116 status: terminal-cleanup-pending

- Status: **terminal-cleanup-pending**
- Date: **2026-08-18**
- Current repository planning floor: `8057c34c7208dd5f561d4ebf2b51b582293e8432` plus [`116-terminal-cleanup.md`](116-terminal-cleanup.md).
- Substantive local data-plane implementation: `0330fb2e9e64dd0877472c930606ab4219ac18a9`.
- Original plan: [`116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- Completion/correction pass: [`116-completion-correction.md`](116-completion-correction.md)
- Prior final-closure pass: [`116-final-closure.md`](116-final-closure.md)
- **Execute now:** [`116-terminal-cleanup.md`](116-terminal-cleanup.md)
- Handoff: [`116-handoff.md`](116-handoff.md)
- Predecessor: [`115-status.md`](115-status.md)
- Plan 117: **blocked until the terminal cleanup passes**.

## Why Plan 116 is temporarily reopened

The `0330fb2e...` implementation successfully landed the major Plan 116 data-plane work. A strict post-closure audit found three remaining technical closure defects and one authority/evidence inconsistency. These are narrow local issues; they do not invalidate the major wire/AES/material-transfer work and they do not justify reopening external validation.

Current remaining defects:

```text
T1 exact duplicate fragments increase aggregate retained-byte accounting
   even though the partial stores no additional bytes; duplicate handling
   also refreshes last_touched_ms before idempotence is established.

T2 first-fragment delivery metadata is not part of duplicate identity;
   a body-identical duplicate first fragment with a different delivery
   target can be silently treated as an idempotent duplicate.

T3 the fragmented outbound -> inbound exact-byte role trajectory is
   executed only in canonical fragment order. The Plan 116 final closure
   explicitly required an out-of-order full role-level trajectory.

T4 status/handoff authority diverged and the prior status listed several
   intended F4 test names rather than exact implemented identifiers.
```

Detailed correction and acceptance criteria live in [`116-terminal-cleanup.md`](116-terminal-cleanup.md).

## Work that remains accepted

The following surfaces remain accepted and should **not** be rewritten:

```text
F1 state-machine material transfer                = passed-local
F2 production placeholder APIs                    = test-only / production blocked
F3 fragment sizing + boundary tests               = passed-local
F4 first-fragment delivery retention basic path   = passed-local
F5 outbound-to-inbound unfragmented trajectory    = passed-exact-bytes-local
F5 outbound-to-inbound fragmented trajectory      = passed-exact-bytes-ordered-local
Tunnel Message checksum/control framing           = corrected-local
AES participant/creator transforms                = corrected-local
production IV/padding randomness                  = corrected-local
```

The terminal cleanup is about bounded reassembly integrity and the missing out-of-order acceptance dimension only.

## Current implementation facts

### Real established material transfer

`ShortBuildStateMachine::take_established_material()` now consumes retained per-hop `LayerKeys` after a real `Established` transition and constructs the existing `EstablishedMaterial` topology. `ShortBuildRegistrar::admit_established_machine()` passes that material to the pool. The material-less legacy registrar path fails closed.

### Material-only production pool

`ExploratoryPool::register_inbound()` / `register_outbound()` and `build_placeholder_established()` are `#[cfg(test)]`. Production registration uses `register_*_with_material()`.

### Fragment boundaries

`fragment_complete_message()` now uses exact LOCAL/ROUTER/TUNNEL unfragmented and first-fragment overheads plus the 7-byte follow-on overhead. Active boundary tests run generated fragment sets through `build_cells()` and parsing.

### Cross-tunnel trajectory

The current role test executes:

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

for both small and fragmented messages. The fragmented case currently delivers inbound cells to the endpoint in canonical order; the terminal cleanup must add the required out-of-order role-level proof.

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
plan_116_fragmented_cross_tunnel    = passed-exact-bytes-ordered-only
plan_116_duplicate_accounting       = correction-required
plan_116_duplicate_expiry           = correction-required
plan_116_first_delivery_identity    = correction-required
plan_116                             = terminal-cleanup-pending
plan_117                             = blocked-on-plan116-terminal-cleanup
qualified_external_delivery         = blocked-on-host-execution-lane
normal_daemon_ntcp2                 = disabled-and-unenableable
ntcp2                               = experimental-non-advertised
```

## Required terminal result

Do not restore Plan 116 closure until the terminal cleanup proves:

```text
exact_duplicate_retained_bytes       = unchanged
exact_duplicate_at_budget_limit      = accepted-noop
exact_duplicate_expiry               = no-refresh
completion_after_duplicates          = retained-bytes-zero
first_delivery_exact_duplicate       = idempotent
first_delivery_conflict              = rejected-partial-invalidated
fragmented_cross_tunnel_out_of_order = passed-exact-bytes
current_test_evidence_names           = exact-source-identifiers
status_handoff_authority              = synchronized
workspace_validation                  = passed-or-preexisting-env-blocker-only
```

Only then record:

```text
plan_116 = passed-final-local-closure
plan_117 = unblocked-ready-for-planning
```

## Fixed anti-loop scope

This terminal cleanup must not reopen:

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

This is an ordinary local Rust correctness pass over `fragment.rs`, the fragmented trajectory in `roles.rs`, and closure documentation.