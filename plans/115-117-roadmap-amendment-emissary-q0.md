# Plans 115-117 roadmap amendment: bounded Emissary Q0 and non-blocking local data-plane progression

## Status

- Date: 2026-08-17.
- This amendment supersedes the gate language in
  [`plans/115-117-external-delivery-to-live-netdb-roadmap.md`](115-117-external-delivery-to-live-netdb-roadmap.md)
  that requires a future rootless/Multipass-capable host before Plan 116 can
  begin.
- It does not change the final mixed-router interoperability requirements of
  Milestones 3, 4, or 5.

## Corrected sequencing

The previous roadmap collapsed two separate questions:

1. whether the short-build protocol bytes are independently consumable; and
2. whether the current host can provide an authenticated router transport lane.

Those must remain separate.

The corrected sequence is:

```text
Plan 115 local short-build + canonical I2NP bridge
    already complete
        |
        v
Plan 115 bounded Emissary Q0 completion
    in-process upstream native consumer
    no transport / daemon / namespace required
        |
        +--> native protocol pass
        |       |
        |       v
        |    Plan 116 local tunnel data plane
        |
        +--> native protocol defect
        |       |
        |       v
        |    one narrow protocol correction
        |       |
        |       v
        |    same focused Emissary Q0
        |       |
        |       v
        |    Plan 116
        |
        +--> reference build/tooling blocked before protocol processing
                |
                v
             external evidence deferred
                |
                v
             Plan 116 local tunnel data plane
```

The transport-independent local data plane is no longer blocked merely because
Q1/Q2 cannot run on this host.

## Revised Gate 115

### Goal

Obtain one bounded independent native short-build observation using upstream
Emissary at revision
`9b43484a21d5a1291c4881cdae62a36c527f8c0f`.

The exact plan is
[`plans/115-completion-emissary-native-q0.md`](115-completion-emissary-native-q0.md).

### Sufficient success

The reference must execute its own:

```text
type-25 parse
 -> target record selection
 -> Noise-N request decrypt
 -> short request parse
 -> native transit admission
 -> OBEP reply-record/reply-envelope construction
```

A native accepted OBEP result is sufficient independent protocol evidence for
local progression.

### Q1/Q2 treatment

Authenticated transport delivery and returned live replies are deferred. They
remain useful future integration evidence but are not required before the
TunnelData implementation exists.

This avoids validating transport and tunnel forwarding in the wrong order.

## Revised Gate 116

Plan 116 is local Milestone 5 product construction, not an external validation
plan.

It becomes executable when either:

```text
A. Plan 115 Emissary Q0 passes; or
B. the bounded Emissary Q0 is prevented by reference build/tooling constraints
   after the fixed attempt budget, with no native protocol defect demonstrated.
```

It remains temporarily blocked only when:

```text
C. Emissary reaches native short-build processing and reproducibly localizes an
   i2pr protocol defect.
```

Branch C permits one narrow corrective pass and then the same focused Q0 is
repeated once.

### Required Plan 116 product scope remains

```text
established path metadata + derived keys
 -> local outbound gateway injection
 -> tunnel message fragmentation
 -> fixed-size TunnelData cells
 -> layer/IV transformation
 -> participant forwarding
 -> outbound endpoint reassembly and delivery
 -> inbound gateway/participant/local endpoint path
 -> bounded partial-message state
 -> tunnel expiry/replacement/cleanup
```

Use deterministic in-memory delivery first. Transport remains an injected
boundary.

Do not hard-code NTCP2 into Plan 116.

## Revised Gate 117

Plan 117 remains the integration/live exploratory checkpoint.

It still requires a real router-to-router delivery mechanism before it can make
mixed-router or live NetDB claims. Its role is to integrate already-built
components:

```text
validated RouterInfo
 -> path selection
 -> tunnel construction
 -> TunnelGateway/TunnelData data plane
 -> real transport delivery
 -> independent hop(s)
 -> DatabaseLookup through exploratory tunnel
 -> reply through inbound exploratory tunnel
 -> NetDB validation/persistence
```

If the present development host still cannot provide the transport lane at that
point, the blocker remains an integration/evidence blocker. It must not be
back-propagated into unrelated local router implementation work.

## Milestone claim discipline

The amended roadmap permits implementation progress without weakening final
claims.

The following remain forbidden until independently demonstrated:

```text
NTCP2 interoperable                = false
mixed-router exploratory complete = false
live NetDB over exploratory        = false
production-ready                   = false
anonymity/security suitability     = false
```

A local Plan 116 pass proves only the local tunnel data plane.

## Anti-loop rules

1. Never require a live transport merely to test code that has an existing
   deterministic/in-process boundary.
2. Never rebuild the deleted generic NTCP2/Python harness to answer a tunnel
   protocol question.
3. Prefer a native reference unit-test seam over process orchestration.
4. Separate `protocol defect` from `environment cannot execute evidence`.
5. An environment blocker may defer an interoperability claim; it does not
   automatically block implementation of the next transport-neutral subsystem.
6. A demonstrated independent protocol rejection is different: localize and
   correct it before building further assumptions on that wire format.
7. Every external checkpoint gets a fixed attempt budget and a typed terminal
   outcome.
8. Do not introduce a new validation plan after a successful Emissary Q0. Move
   directly into Plan 116.

## Current authoritative state after this amendment

```text
plan_111                         = retained-core-crypto-corrected
plan_112                         = passed-outbound-pre-delivery-closure
plan_113                         = passed-inbound-reference-reconciliation
plan_114                         = passed-terminal-routing-chain-correction
plan_115_local_bridge            = passed
plan_115_external_q0             = reopened-emissary-native-consumer-pending
short_build_local_outbound       = strict-established
short_build_local_inbound        = strict-established
canonical_i2np_bridge            = locally-conformant-no-double-prefix
Q1_authenticated_transport       = deferred
Q2_external_return_established   = deferred
plan_116_local_data_plane        = next-after-bounded-q0
plan_117_live_integration        = remains-external-lane-dependent
normal_daemon_ntcp2              = disabled-and-unenableable
ntcp2                             = experimental-non-advertised
```
