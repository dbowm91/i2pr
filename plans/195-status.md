# Plan 195 status — M10 remote independent service interoperability and final closure

Status: **`blocked-m10-remote-independent-service-pending-plan210-and-plan211`**.

Plan of record: [`195-m10-remote-independent-service-final-closure.md`](195-m10-remote-independent-service-final-closure.md).

Plan 195's positive remote-service criterion remains open after the post-Plan-209 audit.

Retained facts:

- Plan 181's 29 local rows remain passed.
- Plan 193 proves the lower exact-pinned i2pd mixed-router Streaming stack.
- Plan 208 landed the useful production local-miss -> remote-backend call graph.
- Plan 209 landed the useful `ServiceProduct` black-box composition boundary plus real curl/jaraco client scaffolding.

Remaining work is now precisely split:

```text
plan210 = real service-Destination tunnel material + destination-aware lookup + complete inbound Garlic -> canonical Streaming + bidirectional generic remote product proof
plan211 = real enabled HTTP/IRC service specs + actual i2pd public Destinations + causal curl/jaraco evidence + exact-head M10 closure
```

Current authority:

```text
plan_195 = blocked-m10-remote-independent-service-pending-plan210-and-plan211
plan_208 = retained-partial-production-call-graph-corrective-superseded-by-plan210
plan_209 = retained-partial-black-box-composition-harness-superseded-by-plan211
plan_210 = registered-executable
plan_211 = registered-blocked-by-plan210
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

Plan 195 requires no separate implementation branch while Plans 210/211 execute. On Plan 211 success it may transition to:

```text
plan_195 = passed-m10-remote-independent-service-via-plan210-and-plan211
```

M10 may close via Plan 211 independently of the Java M6 second-family branch. Plan 204 remains a later cross-milestone convergence/normalization pass and must preserve the already-proven M10 result.
