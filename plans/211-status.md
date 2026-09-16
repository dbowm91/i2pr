# Plan 211 status — M10 product-only remote HTTP/IRC closure and exact-head acceptance

Status: **`registered-blocked-by-plan210`**.

Plan of record: [`211-m10-product-only-remote-http-irc-final-acceptance-corrective.md`](211-m10-product-only-remote-http-irc-final-acceptance-corrective.md).

Source floor: `16f569a1b3ef57310f038d0776b95ac0d2a4ad9d` plus a passed Plan 210 implementation.

## Scope

Plan 211 is the final M10 acceptance/evidence pass after Plan 210 proves real per-service destination tunnel material and bidirectional generic remote product behavior.

It retains useful Plan 209 work:

- `ServiceProduct` as the product composition boundary;
- real system `curl` execution;
- exact-pinned `jaraco/irc` execution;
- exact-pinned i2pd external service infrastructure;
- anti-shadow static checking.

It corrects the remaining Plan 209 evidence problems:

- counted service specs must be real enabled HTTP/IRC services, not an empty `ServiceTunnelSet`;
- actual product listener addresses must come from the running product;
- i2pd HTTP/IRC public Destinations must be installed into the real M10 alias/fixed-target configuration;
- mandatory success facts must be command/fixture/snapshot-derived rather than literal assignments;
- HTTP and IRC must each have their own before/after production counter deltas;
- final remote rows must run through Plan 210's real service-Destination material and inbound Streaming path.

## Authority

```text
plan_210 = registered-executable
plan_211 = registered-blocked-by-plan210
remote-independent-http-eepsite = blocked pending plan211
remote-independent-irc-service = blocked pending plan211
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

## Handoff rule

Do not execute or promote Plan 211 until Plan 210 has passed both generic external directions against exact-pinned unmodified i2pd.

When Plan 210 passes, Plan 211 becomes the next M10 executable plan. On command-derived exact-head Plan 211 success, M10 may close independently of the still-separate Java M6 second-family branch; Plan 204 remains the later cross-milestone convergence/normalization pass.
