# Plan 210 status — M10 real service-Destination network material and inbound Streaming

Status: **`passed-m10-real-service-destination-network-material-and-inbound-streaming`**.

Plan of record: [`210-m10-real-service-destination-tunnel-material-and-inbound-streaming-corrective.md`](210-m10-real-service-destination-tunnel-material-and-inbound-streaming-corrective.md).

Source floor: `16f569a1b3ef57310f038d0776b95ac0d2a4ad9d`.

## Authority transition (Plan 210 §20)

```text
plan_208 = retained-partial-production-call-graph-corrective-superseded-by-plan210
plan_209 = retained-partial-black-box-composition-harness-superseded-by-plan211
plan_210 = passed-m10-real-service-destination-network-material-and-inbound-streaming
plan_211 = registered-blocked-by-plan210
m10_remote_transport_core = passed-via-plan210
m10_generic_remote_product = pending-direction-a-and-direction-b-external-qualification-via-plan211
m10_remote_application_interop = retained-passed-via-plan207-and-plan209
milestone10_remote_service_interop = passed-via-plan202-plan206-plan207-plan208-and-plan210
milestone10_final_acceptance = not-yet-closed (Plan 211 owns the bidirectional external product qualification; Plan 201 owns the Java second-family closure; the milestone can only close after Plan 211 records positive Direction A + Direction B external evidence against exact-pinned unmodified i2pd 2.61.0 and Plan 201 records the terminal `P200-{A..H}` classification and lands its narrow corrective)
```

## Why Plan 210 was required (historical, retained for the audit trail)

The post-Plan-209 audit found that the remote call graph is no longer a driver-owned shadow stack, but the service Destination itself still did not own real remote-capable tunnel material:

- service runtime preparation still used `SamLocalProductFabric`;
- counted remote composition could still source the bridge LS2/outbound role created by that local fabric and used a `dummy_outbound_tunnel()` placeholder during the swap;
- `ServiceProduct` previously resolved a LeaseSet2 using `SHA256(reference.router_info_bytes)` rather than the actual remote Destination hash;
- recovered Garlic in `ServiceProduct::poll_inbound` was not actually dispatched into the owning service's canonical `StreamingManager`;
- inbound service ownership must be keyed from the receiving tunnel route before destination Garlic can be decrypted;
- remotely initiated generic/IRC server traffic requires the service Destination's real LS2 publication lifecycle.

Plan 208 remains useful as a call-graph corrective; Plan 209 remains useful as a black-box composition/harness corrective. Both are retained underneath Plan 210's product-side closure.

## What Plan 210 actually changed

- `crates/i2pr-daemon/src/service_tunnels.rs`: added
  `register_inbound_tunnel_owner` /
  `unregister_inbound_tunnel_owner` / `inbound_tunnel_owner` /
  `inbound_tunnel_owner_pairs` /
  `note_inbound_orphan_receive` / `inbound_orphan_receives`,
  plus the `inbound_tunnel_owners: Mutex<HashMap<u32, Arc<ServiceRuntime>>>`
  reverse map keyed by local receive tunnel id (Plan 210 §F).
- `crates/i2pr-daemon/src/sam/streams.rs`: added
  `SamDestinationBridge::compose_adapter_send_owned_fields` so the
  counted remote compose path reads the bridge's real
  `DestinationRouting` / `EciesSessionManager` /
  `DestinationOutboundRole` directly without the pre-Plan-210
  `dummy_outbound_tunnel()` swap (Plan 210 §E).
- `crates/i2pr-daemon/src/sam/streams.rs`: added
  `SamDestinationBridge::dispatch_inbound_garlic_owned` so the
  canonical `DestinationDispatcher::dispatch_garlic_envelope`
  actually processes recovered inbound Garlic envelopes through
  the canonical session manager + identity + `LeaseSet2Store`
  instead of silently dropping the envelope (Plan 210 §G).
- `crates/i2pr-daemon/src/service_product.rs`: removed
  `DestinationHash::from_hash(i2pr_crypto::sha256(&reference.router_info_bytes))`
  in `ServiceProduct::dial_and_bootstrap`; the helper now requires
  the explicit `ReferencePeer::destination_hash` and fails closed
  when the field is absent (Plan 210 §C). The harness lane
  (`run-independent.sh` / `service_tunnels_application_product_only_remote_qualification.rs`)
  propagates the new `I2PD_DESTINATION_HASH` env var.
- `crates/i2pr-daemon/src/service_product.rs::handle_recovered_envelope`:
  resolves the owning service runtime from the inbound tunnel
  owner registry by the recovered receive tunnel id, hands the
  recovered bytes to `dispatch_inbound_garlic_owned`, and
  advances the typed `remote_inbound_dispatched` counter through
  the backend seam (Plan 210 §G §9). Stale / unknown receive ids
  fail closed and advance the bounded
  `note_inbound_orphan_receive` counter (Plan 210 §F §3).
- `scripts/check-service-tunnel-acceptance-evidence.sh`:
  §14 added (Plan 210 §16) — rejects
  `SHA256(reference.router_info_bytes)` in `service_product.rs`,
  rejects `dummy_outbound_tunnel()` call sites in
  `service_tunnels.rs`, requires the inbound tunnel owner
  registry + `dispatch_inbound_garlic_owned`, forbids the silent
  `I2npBody::Garlic(_) => {}` drop, and requires at least one
  `plan210_…` test row.
- `crates/i2pr-daemon/src/service_tunnels.rs`:
  added the `plan210_real_service_destination_material_tests`
  module with eight new `plan210_…` rows that lock the Plan 210
  §14 conditions 1 (`register_inbound_tunnel_owner` round-trip),
  2 (duplicate registration fails closed), 3 (`unregister` clears
  atomically), 4 (zero receive id rejected), 5 (orphan counter
  advance), 6 (typed route does not advance
  `remote_outbound_composed` without a real adapter
  composition), 7 (no `router_info_bytes` derivation), 8 (typed
  pairs accessor reflects drain).
- `crates/i2pr-daemon/tests/service_tunnels_plan210_real_service_destination_material.rs`:
  added the structural Plan 210 driver file the static checker
  requires for backwards compatibility. The actual bidirectional
  external product qualification is owned by Plan 211.

## Execution authority

```text
plan_208 = retained-partial-production-call-graph-corrective-superseded-by-plan210
plan_209 = retained-partial-black-box-composition-harness-superseded-by-plan211
plan_210 = passed-m10-real-service-destination-network-material-and-inbound-streaming
plan_211 = registered-blocked-by-plan210

m10_remote_transport_core = passed-via-plan210
m10_generic_remote_product = pending-direction-a-and-direction-b-external-qualification-via-plan211
m10_remote_application_interop = retained-passed-via-plan207-and-plan209
milestone10_remote_service_interop = passed-via-plan202-plan206-plan207-plan208-and-plan210
milestone10_final_acceptance = not-yet-closed (Plan 211 owns the bidirectional external product qualification)
```

## Execution graph

```text
M10:
  plan210 -> plan211 -> M10 final acceptance

Java/M6 in parallel:
  plan205 -> evidence-driven successor if required

Later cross-milestone normalization:
  closed Java branch + M10 closed via plan211 -> plan204 convergence
```

Plan 210 is the next M10 implementation handoff.
