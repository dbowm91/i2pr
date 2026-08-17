# Plans 115-117 roadmap amendment: Emissary Q0 and non-blocking local progression

- Date: 2026-08-17.
- Supersedes the original 115-117 roadmap only where it requires a future
  rootless/Multipass-capable host before Plan 116 can start.
- Final mixed-router interoperability requirements remain unchanged.

## Corrected sequence

```text
Plan 115 local short-build + I2NP bridge     = complete
        |
        v
bounded upstream Emissary Q0
        |
        +--> native pass --------------------------> Plan 116
        |
        +--> native protocol defect
        |       -> one narrow correction
        |       -> same focused Q0 ----------------> Plan 116
        |
        +--> reference/build blocker before native processing
                -> external evidence deferred -----> Plan 116
```

The exact Q0 plan is
[`plans/115-completion-emissary-native-q0.md`](115-completion-emissary-native-q0.md).

## Gate 115

Use pinned upstream Emissary revision
`9b43484a21d5a1291c4881cdae62a36c527f8c0f` and its existing
`TestTransitTunnelManager<MockRuntime> -> handle_short_tunnel_build` seam.

Sufficient success is native accepted processing through:

```text
I2NP type-25 parse
 -> target record selection
 -> Noise-N decrypt
 -> short request parse
 -> transit admission
 -> OBEP reply-envelope construction
```

Q1 authenticated transport delivery and Q2 live returned reply are deferred.
They are integration evidence, not prerequisites for implementing TunnelData.

## Gate 116

Plan 116 is local product construction and remains transport-neutral:

```text
established path metadata + keys
 -> TunnelGateway injection
 -> fragmentation
 -> TunnelData construction
 -> layer/IV transformation
 -> participant forwarding
 -> endpoint reassembly/delivery
 -> inbound path
 -> expiry/replacement/cleanup
```

Use deterministic in-memory delivery first. Do not hard-code NTCP2.

Plan 116 is temporarily blocked only if Emissary reaches native processing and
reproducibly demonstrates an i2pr short-build protocol defect.

A reference build/tooling blocker without native protocol evidence does **not**
block Plan 116.

## Gate 117

Plan 117 remains the live integration checkpoint. It must eventually prove:

```text
real router delivery
 -> independent tunnel hop(s)
 -> TunnelData forwarding
 -> exploratory DatabaseLookup
 -> reply through inbound exploratory tunnel
 -> NetDB validation/persistence
```

If the environment still lacks a qualified transport lane then, record that as
an integration/evidence blocker. Do not back-propagate it into unrelated local
router implementation work.

## Anti-loop rules

1. Prefer native in-process reference seams over process/network harnesses.
2. Do not rebuild deleted Python/NTCP2 orchestration to answer tunnel-protocol
   questions.
3. Separate `protocol defect` from `environment cannot execute evidence`.
4. Every external checkpoint has a fixed attempt budget and typed terminal
   outcome.
5. A successful Emissary Q0 goes directly to Plan 116; no extra validation plan.
6. Environment blockers may defer interoperability claims, but they do not
   automatically halt transport-neutral implementation.

Current authority:

```text
plan_115_local_bridge          = passed
plan_115_external_q0           = reopened-emissary-native-consumer-pending
Q1_authenticated_transport     = deferred
Q2_external_return_established = deferred
plan_116_local_data_plane      = next-after-bounded-q0
plan_117_live_integration      = external-lane-dependent
normal_daemon_ntcp2            = disabled-and-unenableable
ntcp2                           = experimental-non-advertised
```
