# Plan 114 handoff

- Status: **completed; do not execute again**
- Date: 2026-08-17 post-closure reconciliation
- Plan-of-record: [`plans/114-short-build-terminal-routing-chain-correction.md`](114-short-build-terminal-routing-chain-correction.md)
- Closure: [`plans/114-status.md`](114-status.md)
- Active successor: [`plans/115-qualified-independent-short-build-consumption-and-external-delivery.md`](115-qualified-independent-short-build-consumption-and-external-delivery.md)
- Active handoff: [`plans/115-handoff.md`](115-handoff.md)

## Closure state

Plan 114 landed and passed. The previous implementation instructions in this handoff are historical and must not be re-executed.

```text
plan_111                        = retained-core-crypto-corrected
plan_112                        = passed-outbound-pre-delivery-closure
plan_113                        = passed-inbound-reference-reconciliation
plan_114                        = passed-terminal-routing-chain-correction

intermediate_router_chain       = exact
intermediate_tunnel_id_chain    = exact
outbound_terminal_route         = explicit-reply-router-and-tunnel
inbound_terminal_route          = explicit-creator-router-and-tunnel
high_level_outbound_e2e         = strict-established
high_level_inbound_e2e          = strict-established

qualified_external_delivery     = active-plan115
milestone4b                     = blocked-on-independent-live-exploratory-evidence
normal_daemon_ntcp2             = disabled-and-unenableable
ntcp2                           = experimental-non-advertised
```

## Retained implementation boundary

Plan 114 corrected `ShortBuildPath -> build_hop_specs() -> MultiRecordHopSpec` by:

- adding explicit outbound reply-router metadata;
- deriving terminal `next_router_hash` from the direction-specific route rather than terminal self-hash;
- enforcing every intermediate `next_tunnel == following.receive_tunnel` relationship at high- and lower-level construction boundaries;
- requiring strict inbound and outbound high-level trajectories to reach `Established`.

These corrections are prerequisites for Plan 115 and must not be weakened.

## What not to do from this file

Do not:

- add another local terminal-routing correction without new evidence;
- reopen Plans 109-113 crypto/layout work;
- restart the old NTCP2 interop sequence;
- enable daemon NTCP2;
- add Python harness code, rootless namespaces, containers, or public-network requirements;
- treat this file as the active execution plan.

## Continue here

Read, in order:

1. [`plans/115-handoff.md`](115-handoff.md)
2. [`plans/115-qualified-independent-short-build-consumption-and-external-delivery.md`](115-qualified-independent-short-build-consumption-and-external-delivery.md)
3. [`plans/115-117-external-delivery-to-live-netdb-roadmap.md`](115-117-external-delivery-to-live-netdb-roadmap.md)

Plan 115's minimum useful goal is one independent implementation natively consuming the exact production-generated i2pr short-build message. Live transport/reply evidence is pursued only through the smallest existing lane and may not recreate the historical harness program.
