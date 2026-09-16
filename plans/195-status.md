# Plan 195 status — M10 remote independent service interoperability and final closure

Status: **`blocked-m10-remote-independent-service-pending-plan212-and-plan211-requalification`**.

Plan of record: [`195-m10-remote-independent-service-final-closure.md`](195-m10-remote-independent-service-final-closure.md).

Retained facts:

- Plan 181's 29 local rows remain passed.
- Plan 193 proves the lower exact-pinned i2pd mixed-router Streaming stack.
- Plan 208 landed the useful production local-miss -> remote-backend call graph.
- Plan 210 landed useful receive-id ownership and Garlic-dispatch structure.
- Plan 211 landed a useful black-box HTTP/IRC application harness with real service specs and public i2pd Destinations.

The remote-service criterion remains open because the service runtime still sources its counted remote outbound role/local LS2 from the localhost-only `SamLocalProductFabric`, while real exploratory material is not installed as service-owned router-backed state and authenticated inbound Garlic payloads are not yet delivered into the canonical service Streaming manager.

Remaining work:

```text
plan212 = install real per-service router-backed tunnel/LS2 state
          + production inbound receive-id ownership
          + per-service target LS2 lookup
          + server LS2 publication
          + complete Garlic -> pop_payload -> Streaming receive
          + mandatory generic Direction A/B external proof

then:
plan211 = rerun retained product-only curl/jaraco HTTP/IRC acceptance
```

Current authority:

```text
plan_195 = blocked-m10-remote-independent-service-pending-plan212-and-plan211-requalification
plan_210 = retained-partial-structural-corrective-superseded-by-plan212
plan_211 = retained-source-harness-blocked-by-plan212
plan_212 = registered-executable
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

On Plan 212 + Plan 211 success this status may transition to:

```text
plan_195 = passed-m10-remote-independent-service-via-plan212-and-plan211
```

M10 may close independently of the Java M6 second-family branch; Plan 204 later performs cross-milestone authority convergence.
