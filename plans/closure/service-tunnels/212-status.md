# Plan 212 status — M10 router-backed service-Destination material and canonical inbound Streaming

Status: **`passed-source-and-generic-external-qualification-via-plan213`**.

Plan of record: [`212-m10-router-backed-service-destination-material-and-canonical-inbound-streaming-corrective.md`](../../implementation/service-tunnels/212-m10-router-backed-service-destination-material-and-canonical-inbound-streaming-corrective.md).

Source-closure head: `232be0f87469175a1f01152a7488ecf026b27eeb`.

External qualification: Plan 213 closed on commit
`ef59fb3848060f4fcf5ab5a73019e967aaf6ad4c` (hosted
`router-generic` runs `35169304975` + `35169987455`, both
`P213-N-passed`; routine CI run `35169271477` green on the same
SHA). Authority: `m10_remote_transport_core =
passed-via-plan212-and-plan213`, `m10_generic_remote_product =
passed-via-plan213` (see `plans/closure/service-tunnels/213-status.md`).

## Retained source closure

Plan 212 materially corrected the M10 product architecture:

- `RouterDestinationNetworkState` separates counted remote-capable service state from the localhost-only `SamLocalProductFabric` seam;
- remote composition uses router-backed state and fails closed rather than falling back to local-fabric tunnel material;
- `ServiceProduct::start` performs per-service real outbound/inbound provisioning before supervisors start;
- service local LS2s are derived from real inbound lease metadata;
- real inbound receive ids are registered to owning service runtimes;
- application target lookup is per service Destination rather than embedded in router bootstrap metadata;
- server LS2 publication is part of service provisioning where required;
- recovered Garlic is drained through `DestinationDispatcher::pop_payload` and `StreamingDestinationAdapter::receive` into the same canonical service `StreamingManager` used by the application pump;
- standard-first / short-transport-fallback inbound I2NP decode parity landed;
- focused Plan 212 unit/static floors landed and routine CI was green on the source-closure head.

These source changes are retained and should not be rewritten without evidence from a real qualification run.

## Qualification audit after source closure

The Plan 212 external driver itself is not yet a valid passing proof.

At the source-closure head:

- Direction A/B success booleans in `service_tunnels_plan212_router_backed_product.rs` remain immutable `false`;
- the driver loops poll inbound but do not actually open the counted GenericClient TCP connection, move small/large bytes, or consume responses;
- Direction B does not yet initiate a real independent i2pd STREAM connection into the i2pr GenericServer Destination;
- the normal M10 runner skips the generic prerequisite unless `PLAN212_GENERIC_*` is externally provisioned;
- several required qualification facts are written as unconditional success literals rather than command/state-derived evidence.

Therefore Plan 212 is **source-closed but not externally qualified**.

Plan 213 is the executable qualification corrective. It must complete the existing generic A/B driver, provide an independent i2pd SAM STREAM reference service/initiator, remove synthetic evidence, and pass twice on one exact hosted SHA.

## Current authority

```text
plan_208 = retained-partial-production-call-graph-corrective
plan_209 = retained-partial-black-box-composition-harness
plan_210 = retained-partial-structural-corrective-superseded-by-plan212
plan_211 = retained-source-harness-superseded-for-final-evidence-by-plan214
plan_212 = passed-source-and-generic-external-qualification-via-plan213
plan_213 = passed-m10-router-backed-generic-external-qualification
plan_214 = source-landed-hosted-double-pass-pending

m10_local_rows = passed (29/29 retained)
m10_remote_transport_core = passed-via-plan212-and-plan213
m10_generic_remote_product = passed-via-plan213
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

## Execution graph

```text
Plan 212 source closure (retained)
  -> Plan 213 generic router-backed Direction A + Direction B external qualification
  -> Plan 214 product-only HTTP + IRC external requalification
  -> M10 final closure only if both qualification plans pass
```

## Success transition

Only after Plan 213 passes may this status advance to:

```text
plan_212 = passed-source-and-generic-external-qualification-via-plan213
m10_remote_transport_core = passed-via-plan212-and-plan213
m10_generic_remote_product = passed-via-plan213
```

Plan 214 then owns the independent HTTP/IRC application rows and final M10 closure.

The Java M6 second-family branch remains independent.
