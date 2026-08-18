# Plans 115-117 roadmap amendment: Q0 passed, local data plane active

- Date: 2026-08-18.
- This amendment is the authority where older 115-117 text conflicts with the
  Plan 115 Emissary Q0 completion.
- Final mixed-router interoperability requirements remain unchanged.

## Current sequence

```text
Plan 115 short-build + I2NP bridge                 PASSED
        |
        v
Plan 115 pinned upstream Emissary native Q0        PASSED
        |
        v
Plan 116 local transport-neutral tunnel data plane ACTIVE / NEXT
        |
        v
Plan 117 live exploratory + NetDB integration      GATED ON PLAN 116
```

There is no intervening Q1/Q2 requirement before Plan 116.

## Gate 115 — closed for local progression

Pinned upstream Emissary revision:

```text
9b43484a21d5a1291c4881cdae62a36c527f8c0f
emissary-core 0.4.0
```

The production i2pr type-25 ShortTunnelBuild reached Emissary's native
`handle_short_tunnel_build()` path and produced the accepted OBEP
TunnelGateway/Garlic reply path.

Therefore:

```text
Q0 independent native short-build consumption = passed
Q1 authenticated transport delivery            = deferred
Q2 live reply -> i2pr Established               = deferred
```

Q1/Q2 remain useful later integration evidence. They are not prerequisites for
implementing the established tunnel data plane.

Do not rerun Q0 merely because the temporary test patch digest was not retained;
that is a recorded evidence-bookkeeping limitation, not a demonstrated protocol
defect.

## Gate 116 — executable now

Plan-of-record:
[`plans/116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)

Handoff:
[`plans/116-handoff.md`](116-handoff.md)

Required progression:

```text
real established tunnel material + keys
 -> real pool registration
 -> TunnelData payload builder/parser
 -> fragmentation/reassembly
 -> AES layer/IV transforms
 -> participant/endpoint processing
 -> deterministic outbound tunnel
 -> deterministic outbound-to-inbound tunnel delivery
 -> local exploratory pair closure
```

Plan 116 is local product construction. Use deterministic in-memory role
delivery. Do not hard-code NTCP2 or require a live independent router.

## Gate 117 — not executable yet

Plan 117 starts only after:

```text
plan_116 = passed-local-tunnel-data-plane
```

Then the project may revisit the smallest available real router-delivery lane to
prove:

```text
real transport delivery
 -> independent tunnel hop(s)
 -> TunnelData forwarding
 -> outbound exploratory DatabaseLookup
 -> response through inbound exploratory tunnel
 -> NetDB validation/persistence
 -> RouterInfo publication verification
```

If the current environment still cannot execute the real transport lane at that
point, record the limitation as a **Plan 117 integration/evidence blocker**. Do
not back-propagate it into Plan 116 or reopen short-build construction.

## Anti-loop rules

1. Plan 115 Q0 is complete; do not create another short-build validation plan
   without new affirmative defect evidence.
2. Do not rebuild historical Python/NTCP2 orchestration to implement the local
   tunnel data plane.
3. Separate protocol defects, implementation defects, and environment/evidence
   limitations.
4. Prefer deterministic production-code trajectories before external runtime
   integration.
5. Plan 116 must end with router capability, not additional validation
   infrastructure.
6. Q1/Q2 belong to later integration once the TunnelData data plane exists.

## Current authority

```text
plan_115                         = passed-emissary-q0-construction-and-obep-reply-only
Q0_native_emissary               = passed
Q1_authenticated_transport       = deferred
Q2_external_return_established   = deferred
plan_116_local_data_plane        = unblocked-and-next
plan_117_live_integration        = blocked-until-plan116-passes
normal_daemon_ntcp2              = disabled-and-unenableable
ntcp2                             = experimental-non-advertised
```
