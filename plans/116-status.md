# Plan 116 status: passed-final-local-closure

- Status: **passed-final-local-closure**
- Date: **2026-08-18**
- Current implementation floor: `0330fb2e9e64dd0877472c930606ab4219ac18a9` (closure pass landed on top of `78f1024c47ca5ba110656e9fe2936ca2719c319f`)
- Original plan: [`116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- Completion/correction pass: [`116-completion-correction.md`](116-completion-correction.md)
- Final closure pass: [`116-final-closure.md`](116-final-closure.md)
- Predecessor: [`115-status.md`](115-status.md)
- Plan 117: **unblocked, ready for planning**

## Closure result

Plan 116 final closure landed all five closure-level defects. The
`i2pr-tunnel` crate now exposes a real established-material transfer,
test-only placeholder APIs, exact fragment-boundary sizing with the
canonical I2P Tunnel Message Specification overheads, retained first-
fragment delivery metadata through reassembly, and the complete
outbound-to-inbound tunnel trajectory with exact-byte equality — both
in the unfragmented case (DeliveryStatus body) and the fragmented case
(Data body across multiple TunnelData cells on both sides).

```text
F1 state-machine material transfer                = landed
F2 production placeholder APIs                    = removed (#[cfg(test)])
F3 fragment sizing + boundary tests               = landed
F4 fragmented delivery metadata retention         = landed
F5 outbound-to-inbound trajectory (unfragmented)  = passed-exact-bytes
F5 outbound-to-inbound trajectory (fragmented)    = passed-exact-bytes
```

## Acceptance evidence

The Plan 116 closure pass is supported by the following active tests
in `crates/i2pr-tunnel/src/`:

| Surface | Tests |
| --- | --- |
| F1 real transfer | `take_established_material_before_established_fails`, `take_established_material_double_take_fails`, `outbound_take_established_material_topology_round_trip`, `inbound_take_established_material_topology_round_trip`, `registrar_admit_established_machine_inserts_into_pool` |
| F3 fragment boundaries | `boundary_local_round_trips_at_each_boundary`, `boundary_router_round_trips_at_each_boundary`, `boundary_tunnel_round_trips_at_each_boundary`, `fragment_complete_message_outputs_are_accepted_by_build_cells` |
| F4 delivery retention | `reassembler_retains_first_delivery_through_complete_message`, `reassembler_clears_first_delivery_on_conflicting_duplicate`, `reassembler_emits_local_delivery_for_local_unfragmented`, `reassembler_unfragmented_message_clears_first_delivery`, `reassembler_rejects_conflicting_follow_on_with_retained_delivery` |
| F5 trajectory | `outbound_to_inbound_tunnel_trajectory_exact_bytes`, `outbound_to_inbound_fragmented_trajectory_exact_bytes` |
| Independent reference | `plan111_reference_vectors_match_production_open`, `plan111_reference_vectors_match_production_seal`, `plan111_reference_vectors_match_production_layer_keys`, `plan111_reference_vectors_match_pure_rust_oracle`, `plan111_reference_vectors_sealed_envelope_re_encrypts_to_same_bytes` |

Total: 229 lib tests + 5 integration tests, all green locally.

## Architectural surface added by the final closure pass

- `ShortBuildStateMachine::take_established_material(&mut self, established_at_seconds: u64) -> Result<EstablishedMaterial, ShortBuildConstructionError>` produces the canonical `EstablishedMaterial` directly from a successful `StatePhase::Established` machine. Outbound paths keep their `[Participant*]+OBEP` topology; inbound paths keep their `[IBGW, Participant*]` topology with the originator as terminal next-hop. The second call returns `EstablishedMaterialAlreadyTaken`.
- `HopCryptoContext::take_layer_keys(&mut self)` performs a zeroing `mem::replace` swap into the owned `LayerKeys`. The context refuses subsequent layer-key access after the swap.
- `ShortBuildRegistrar` exposes `admit_material(EstablishedMaterial, now_seconds)` and the canonical `admit_established_machine(&mut machine, now_seconds)` surface. The legacy `admit(&ShortBuildOutcome, slot, now_seconds)` fails closed: `EstablishedMaterialRequired` for `Established` outcomes, `NotEstablished` otherwise.
- `ExploratoryPool::register_inbound`, `register_outbound`, and the internal `build_placeholder_established` are now `#[cfg(test)]`. Production callers must go through `register_inbound_with_material` / `register_outbound_with_material`.
- `data::unfragmented_overhead(DeliveryInstruction) -> usize`, `fragmented_first_overhead(DeliveryInstruction) -> usize`, and the `FOLLOW_ON_OVERHEAD = 7` constant now express the canonical I2P Tunnel Message Specification overheads. `fragment_complete_message` uses them.
- `fragment::ReassembledFragment { message: Vec<u8>, delivery: Option<DeliveryInstruction> }` and `BoundedReassembler::insert_with_delivery(key, fragment, Option<DeliveryInstruction>) -> Result<Option<ReassembledFragment>, ReassemblyError>` carry the first-fragment delivery instruction through reassembly. Conflicting duplicates clear it. The unfragmented path returns `delivery: Some(...)`.
- `roles::action_from_unspecified` is removed. `OutboundEndpointRole::assemble_actions` consumes the reassembled fragment's `delivery` and returns `TunnelRoleError::UnspecifiedDeliveryInstruction { message_id }` when the reassembler returns no delivery. `LocalInboundEndpointRole::process` uses `insert_with_delivery`, requires `DeliveryInstruction::Local`, and rejects non-local retained delivery with `LocalInboundNonLocalDelivery`.
- `OutboundGatewayRole::forward_cells(&self, header, complete_message, rng, now_ms) -> Result<Vec<OBGWRouterDelivery>, TunnelRoleError>` returns the ordered multi-cell forward path. `InboundGatewayRole::process_cells(&self, gateway, rng, now_ms) -> Result<Vec<OutboundCell>, TunnelRoleError>` returns the ordered multi-cell fragment set. The single-cell `forward` and `process` paths remain for small messages.

## Mandatory current token table

```text
plan_115_Q0                          = passed-emissary-native-consumer
Q1_authenticated_transport            = deferred
Q2_external_return_established        = deferred
plan_116_wire_format                  = corrected-local
plan_116_aes_layer                    = corrected-local
plan_116_rng                          = corrected-local
plan_116_state_machine_material       = landed-real-transfer
plan_116_pool_real_material_only      = landed-cfg-test-only-placeholders
plan_116_fragment_boundaries          = landed-exact-overheads
plan_116_fragment_delivery_state      = landed-first-fragment-retained
plan_116_outbound_router              = passed-local
plan_116_outbound_to_inbound          = passed-exact-bytes
plan_116_fragmented_cross_tunnel      = passed-exact-bytes
plan_116                              = passed-final-local-closure
plan_117                              = unblocked-ready-for-planning
qualified_external_delivery           = blocked-on-host-execution-lane
normal_daemon_ntcp2                   = disabled-and-unenableable
ntcp2                                 = experimental-non-advertised
```

## Fixed anti-loop scope

The final closure pass does **not** reopen:

```text
Emissary/i2pd/Java runtime testing
NTCP2 activation/correction
SSU2
Q1/Q2
rootless namespaces
Docker/Multipass/VMs
Python interop harnesses
public I2P network
Plan 117 execution
```

The Plan 117 planning/execution pass remains a separate future plan.
This status file documents the closure of the local Rust substrate
that Plan 117 will compose.