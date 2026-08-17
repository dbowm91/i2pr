# Plan 102 amendment closure: exploratory-tunnel dependency

- Status: **closed dependency correction; Plan 114 passed; Plan 115 is the active external-evidence checkpoint**
- Date: 2026-08-17 post-Plan-114 authority reconciliation
- Plan of record: [`plans/102-amendment-exploratory-tunnel-dependency.md`](102-amendment-exploratory-tunnel-dependency.md)
- Parent authority: [`plans/102-milestone-4-routerinfo-netdb-authority-and-roadmap.md`](102-milestone-4-routerinfo-netdb-authority-and-roadmap.md)
- Short-build closure authority: [`plans/114-status.md`](114-status.md)
- Active successor: [`plans/115-qualified-independent-short-build-consumption-and-external-delivery.md`](115-qualified-independent-short-build-consumption-and-external-delivery.md)
- Handoff: [`plans/115-handoff.md`](115-handoff.md)

## Current authority

The Plan 102 dependency correction remains complete: a standards-conformant live RouterInfo lookup depends on usable exploratory inbound/outbound tunnels. Direct NTCP2 `DatabaseLookup` substitution remains forbidden.

The post-Plan-113 terminal-routing defects that temporarily re-gated external delivery were closed by Plan 114. The old `blocked-on-plan114` status is no longer active.

Current state:

```text
plans_103_to_106                = closed-local-netdb-bootstrap-foundation
plan_107                        = closed-exploratory-tunnel-substrate
plans_108_to_110                = superseded-by-corrected-successors
plan_111                        = retained-core-short-build-correction
plan_112                        = passed-outbound-pre-delivery-closure
plan_113                        = passed-inbound-reference-reconciliation
plan_114                        = passed-terminal-routing-chain-correction
plan_115                        = ready-qualified-independent-consumption-and-delivery

qualified_external_delivery     = ACTIVE-PLAN115
milestone5_local_build_control   = strict-established-outbound-and-inbound
milestone5_independent_evidence  = pending-plan115
milestone4b_external_acceptance  = blocked-on-live-exploratory-path
normal_daemon_ntcp2             = disabled-and-unenableable
ntcp2                           = experimental-non-advertised
```

## Retained Plan 102 dependency decision

Plan 102 corrected the earlier assumption that a standards-conformant live `DatabaseLookup` could be completed over a direct router transport. That correction remains authoritative.

The required dependency chain remains:

```text
validated/reseeded RouterInfo
 -> selected peers
 -> exploratory outbound + inbound tunnels
 -> DatabaseLookup through outbound exploratory tunnel
 -> response through inbound exploratory tunnel
 -> normal Plan 103 validation/insertion
 -> Plan 104 persistence
```

A successful independent short-build consumer result under Plan 115 is evidence for tunnel construction compatibility; it does not by itself satisfy the live NetDB dependency above.

## Retained short-build results

The following local results remain closed and are not reopened by Plan 115 absent independent evidence of a specific defect:

- Plan 111: Noise/KDF/record/reply cryptographic corrections and fixed-vector evidence;
- Plan 112: random request/reply padding, role topology, exact count-prefixed payload contract, and vector provenance;
- Plan 113: deployed-reference-compatible inbound originator-fake policy with explicit creator identity and integrity verification;
- Plan 114: explicit terminal routing metadata, intermediate tunnel-ID continuity, and strict high-level inbound/outbound trajectories reaching `Established`.

## Revised future-plan unblock sequence

```text
Plans 103-107 local NetDB + exploratory substrate       [closed]
 -> Plans 111-114 corrected short-build control plane   [closed]
 -> Plan 115 independent native short-build evidence    [ACTIVE]
 -> Plan 116 local tunnel data plane                    [gated on Plan 115 Q0]
 -> Plan 117 live exploratory pair + NetDB acceptance   [gated on Plan 116 + live delivery]
 -> Milestone 6 destination/garlic/LeaseSet/streaming   [not yet authorized]
```

The detailed gate definitions are in [`plans/115-117-external-delivery-to-live-netdb-roadmap.md`](115-117-external-delivery-to-live-netdb-roadmap.md).

## Plan 115 interaction with the historical NTCP2 blocker

Plan 115 intentionally separates independent tunnel-protocol consumption from live transport delivery.

If one pinned independent implementation natively accepts the production-generated STBM but the existing NTCP2 development lane remains blocked, record both facts:

```text
independent_short_build = passed-independent-native-consumer
qualified_live_delivery = blocked-<exact-transport-stage>
plan_116_local_data_plane = unblocked
```

This does **not** unblock Milestone 4B external acceptance. It only prevents the historical transport harness from blocking local Milestone 5 data-plane construction.

Normal-daemon NTCP2 remains disabled/unenableable and non-advertised. Plan 079 remains deferred.

## Full Milestone 4B acceptance still requires

1. usable real exploratory outbound and inbound tunnels involving independent-router code;
2. a real RouterInfo lookup sent through the outbound exploratory path;
3. a valid matching response returned through the inbound exploratory path;
4. normal Plan 103 validation/insertion and Plan 104 persistence; and
5. independently verified local RouterInfo publication.

No direct-router-transport substitute may be reported as satisfying these criteria.

## Evidence and authority

Retained evidence:

- [`plans/107-status.md`](107-status.md)
- [`plans/111-status.md`](111-status.md)
- [`plans/112-status.md`](112-status.md)
- [`plans/113-status.md`](113-status.md)
- [`plans/114-status.md`](114-status.md)
- [`specs/references/short-build-inbound-creator-key.md`](../specs/references/short-build-inbound-creator-key.md)

Active execution:

- [`plans/115-qualified-independent-short-build-consumption-and-external-delivery.md`](115-qualified-independent-short-build-consumption-and-external-delivery.md)
- [`plans/115-handoff.md`](115-handoff.md)

No live external-router, public-network, Milestone 4B, or Milestone 5 exit claim is made by this authority reconciliation.
