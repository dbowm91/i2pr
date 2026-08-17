# Plan 102 amendment closure: exploratory-tunnel dependency

- Status: **closed dependency correction; external-delivery handoff temporarily re-gated by Plan 114**
- Date: 2026-08-17 post-Plan-113 terminal-routing audit amendment
- Plan of record: [`plans/102-amendment-exploratory-tunnel-dependency.md`](102-amendment-exploratory-tunnel-dependency.md)
- Parent authority: [`plans/102-milestone-4-routerinfo-netdb-authority-and-roadmap.md`](102-milestone-4-routerinfo-netdb-authority-and-roadmap.md)
- Active corrective successor: [`plans/114-short-build-terminal-routing-chain-correction.md`](114-short-build-terminal-routing-chain-correction.md)
- Handoff: [`plans/114-handoff.md`](114-handoff.md)

## Post-closure audit amendment

The Plan 102 dependency correction itself remains complete: standards-conformant live RouterInfo lookup still depends on exploratory inbound/outbound tunnels and direct NTCP2 lookup substitution remains forbidden.

However, the previous statement that the qualified external-delivery checkpoint was immediately unblocked after Plans 112/113 is superseded by Plan 114.

A post-Plan-113 code audit found that `ShortBuildPath -> build_hop_specs()` still uses the terminal real hop's own router hash as the terminal `next_router_hash`. This cannot correctly represent an outbound OBEP reply router and is also wrong for the final inbound remote hop, which must route toward the creator. The same audit found that intermediate `next_tunnel` IDs are only checked for nonzero values and are not required to equal the following hop's `receive_tunnel` ID.

These are narrow runtime-neutral composition defects, not a reopening of the cryptographic or transport-validation work.

Current authority:

```text
plans_103_to_106               = closed local NetDB/bootstrap foundation
plan_107                       = closed exploratory-tunnel substrate
plans_108_to_110               = superseded by corrected successors
plan_111                       = closed core short-build correction
plan_112                       = closed outbound pre-delivery closure
plan_113                       = closed inbound reference reconciliation
plan_114                       = ready terminal-routing-chain correction

qualified_external_delivery    = BLOCKED-ON-PLAN114
milestone4b_external_acceptance = blocked-on-qualified-independent-router-evidence
normal_daemon_ntcp2            = disabled-and-unenableable
ntcp2                          = experimental-non-advertised
```

Plan 114 must close before the external-delivery checkpoint is treated as unblocked again.

## Retained closure result

The amendment's original dependency correction remains valid. It corrected the assumption that a standards-conformant live `DatabaseLookup` could be completed over a direct router transport. The repository records the required exploratory inbound/outbound tunnel dependency and keeps direct NTCP2 lookup substitution forbidden.

Plans 111-113 also retain their valid local results:

- Plan 111: Noise/KDF/record/reply cryptographic corrections and fixed-vector evidence;
- Plan 112: random request/reply padding, role topology, exact count-prefixed payload contract, and vector provenance;
- Plan 113: deployed-reference-compatible inbound originator-fake policy with explicit creator identity and integrity verification.

Plan 114 is not authorized to reopen those surfaces absent new evidence.

## Future-plan unblock audit

The correct sequence is now:

```text
Plans 103-107 local NetDB + exploratory substrate     [closed]
 -> Plans 108-111 short-build architecture/crypto     [closed/superseded as recorded]
 -> Plan 112 outbound pre-delivery closure             [closed]
 -> Plan 113 inbound reference reconciliation          [closed]
 -> Plan 114 terminal routing + tunnel chain correction [NEXT]
 -> narrow qualified external-delivery checkpoint      [blocked on Plan 114]
 -> full exploratory inbound/outbound acceptance
 -> Milestone 4B external NetDB acceptance
```

Full Milestone 4B acceptance still requires:

1. real exploratory tunnel build evidence against an independent router;
2. a RouterInfo lookup and valid matching response through the exploratory path;
3. normal Plan 103 validation/insertion/persistence; and
4. independently verified local RouterInfo publication.

Plan 079 is not reactivated. Normal-daemon NTCP2 remains disabled and unenableable, and all NTCP2 support remains experimental and non-advertised.

## Evidence and authority

Retained evidence:

- [`plans/107-status.md`](107-status.md)
- [`plans/111-status.md`](111-status.md)
- [`plans/112-status.md`](112-status.md)
- [`plans/113-status.md`](113-status.md)
- [`specs/references/short-build-inbound-creator-key.md`](../specs/references/short-build-inbound-creator-key.md)

Active correction:

- [`plans/114-short-build-terminal-routing-chain-correction.md`](114-short-build-terminal-routing-chain-correction.md)
- [`plans/114-handoff.md`](114-handoff.md)

No live external-router or public-network result is claimed by this amended closure record.
