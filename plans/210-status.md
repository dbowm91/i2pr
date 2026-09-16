# Plan 210 status — M10 real service-Destination network material and inbound Streaming

Status: **`registered-executable-m10-real-service-destination-network-material-and-inbound-streaming`**.

Plan of record: [`210-m10-real-service-destination-tunnel-material-and-inbound-streaming-corrective.md`](210-m10-real-service-destination-tunnel-material-and-inbound-streaming-corrective.md).

Source floor: `16f569a1b3ef57310f038d0776b95ac0d2a4ad9d`.

## Why Plan 210 is required

The post-Plan-209 audit found that the remote call graph is no longer a driver-owned shadow stack, but the service Destination itself still does not own real remote-capable tunnel material:

- service runtime preparation still uses `SamLocalProductFabric`;
- counted remote composition can still source the bridge LS2/outbound role created by that local fabric and uses a `dummy_outbound_tunnel()` placeholder during the swap;
- `ServiceProduct` currently resolves a LeaseSet2 using `SHA256(reference.router_info_bytes)` rather than the actual remote Destination hash;
- recovered Garlic in `ServiceProduct::poll_inbound` is not actually dispatched into the owning service's canonical `StreamingManager`;
- inbound service ownership must be keyed from the receiving tunnel route before destination Garlic can be decrypted;
- remotely initiated generic/IRC server traffic requires the service Destination's real LS2 publication lifecycle.

Plan 208 therefore remains useful as a call-graph corrective, but it is not sufficient remote transport closure. Plan 209 remains useful as a black-box composition/harness corrective, but it is not sufficient application closure.

## Execution authority

```text
plan_208 = retained-partial-production-call-graph-corrective-superseded-for-execution-by-plan210
plan_209 = retained-partial-black-box-composition-harness-superseded-for-execution-by-plan211
plan_210 = registered-executable-m10-real-service-destination-network-material-and-inbound-streaming
plan_211 = registered-blocked-by-plan210

m10_remote_transport_core = not-yet-passed
m10_generic_remote_product = not-yet-passed
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
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
