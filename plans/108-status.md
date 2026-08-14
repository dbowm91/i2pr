# Plan 108 handoff status

- Status: **ready for implementation**
- Date: 2026-08-14
- Plan-of-record: `plans/108-live-ecies-x25519-short-tunnel-build-construction.md`
- Predecessor: Plan 107
- Milestone: 5

## Authority

Plan 108 is the active tunnel-construction implementation plan.

It deliberately narrows the broader pre-existing Plan 107 handoff text:

```text
Plan 107 substrate
  -> Plan 108 local ECIES-X25519 short-build construction
  -> narrow external-delivery checkpoint
  -> live mixed-router tunnel evidence only when a qualified delivery path exists
```

Plan 108 does not reopen normal-daemon NTCP2 activation and does not require live interoperability for closure.

## Required starting state

```text
plan_103                         = passed
plan_104                         = passed
plan_105                         = passed
plan_106                         = passed-local-bootstrap-integration
plan_107                         = passed-exploratory-substrate
plan_108                         = ready
exploratory_tunnel_substrate     = implemented
build_cryptography_seam          = implemented-stub
live_ecies_x25519_build          = unavailable
external_build_delivery          = unavailable
normal_daemon_ntcp2              = disabled-and-unenableable
ntcp2                            = experimental-non-advertised
```

## Plan 108 closure target

```text
plan_108                         = passed-local-short-build-construction
ecies_x25519_short_build_crypto  = implemented-local
short_build_state_machine        = implemented-local
success_gated_pool_registration  = implemented-local
external_build_delivery          = unavailable
live_mixed_router_build          = blocked-on-qualified-delivery
live_routerinfo_lookup           = blocked-on-live-exploratory-path
normal_daemon_ntcp2              = disabled-and-unenableable
```

## Important protocol correction owned by Plan 108

Plan 107 scaffolding currently carries a `ShortTunnelBuild` message-type value of `225`. The current official I2NP specification defines `ShortTunnelBuild` as type `25` and `OutboundTunnelBuildReply` as type `26`.

Plan 108 must audit and correct that constant with a regression test before building cryptographic/state-machine behavior on top of it.

## Execution rule

Implement the plan-of-record in order. If live transport, full I2NP dispatch, SSU2, reseed HTTPS, Java/i2pd/Emissary execution, privileged networking, or a new Python harness appears necessary to satisfy a Plan 108 acceptance criterion, treat that as a scope leak. Keep the local construction boundary explicit and defer external delivery to the post-108 checkpoint.
