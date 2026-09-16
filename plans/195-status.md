# Plan 195 status — M10 remote independent service interoperability and final closure

Status: **`blocked-m10-remote-independent-service-pending-plan213-and-plan214`**.

Plan of record: [`195-m10-remote-independent-service-final-closure.md`](195-m10-remote-independent-service-final-closure.md).

## Retained facts

- Plan 181's 29 local rows remain passed.
- Plan 193 proves the lower exact-pinned i2pd mixed-router Streaming stack.
- Plan 208 retained the useful production local-miss -> remote-backend call graph.
- Plan 210 retained useful receive-id ownership and Garlic-dispatch structure.
- Plan 212 materially corrected the remaining product architecture: router-backed per-service Destination state, real per-service tunnel/LS2 provisioning, real inbound owner registration, per-service target lookup, server publication, and Garlic -> canonical Streaming receive.
- Plan 211 retains a useful real curl/jaraco application harness with actual service specs and independent i2pd Destinations.

The remote-service criterion remains open because qualification itself is not yet authoritative.

## Remaining work

```text
Plan 213:
  complete the generic A/B driver
  remove synthetic qualification facts
  provision independent i2pd SAM STREAM reference service/initiator
  prove real router-backed Direction A + Direction B
  pass hosted exact-head twice

then Plan 214:
  harden retained Plan 211 HTTP/IRC evidence
  remove placeholder manager and literal pin/privacy facts
  keep inbound polling active during external clients
  require target-fixture observations + independent per-app counters
  pass hosted full lane twice on one exact SHA
```

## Current authority

```text
plan_195 = blocked-m10-remote-independent-service-pending-plan213-and-plan214
plan_211 = retained-source-harness-superseded-for-final-evidence-by-plan214
plan_212 = source-closure-landed-qualification-owned-by-plan213
plan_213 = registered-executable
plan_214 = registered-blocked-by-plan213

m10_remote_transport_core = not-yet-passed
m10_generic_remote_product = not-yet-passed
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

On Plan 213 + Plan 214 terminal success this status may transition to:

```text
plan_195 = passed-m10-remote-independent-service-via-plan213-and-plan214
```

M10 may close independently of the Java M6 second-family branch. Plan 204 remains the later cross-milestone authority convergence pass.
