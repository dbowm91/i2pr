# Plan 102 amendment closure: exploratory-tunnel dependency

- Status: **closed; dependency-correction-and-handoff-complete**
- Date: 2026-08-15
- Plan of record: [`plans/102-amendment-exploratory-tunnel-dependency.md`](102-amendment-exploratory-tunnel-dependency.md)
- Parent authority: [`plans/102-milestone-4-routerinfo-netdb-authority-and-roadmap.md`](102-milestone-4-routerinfo-netdb-authority-and-roadmap.md)

## Closure result

The amendment is implemented and formally closed. It corrected the original
Plan 102 assumption that a standards-conformant live `DatabaseLookup` could
be completed over a direct router transport. The repository now records the
standard exploratory inbound/outbound tunnel dependency and keeps direct
NTCP2 lookup substitution forbidden.

The implementation and planning sequence reached the required local state:

```text
plans_103_to_106 = closed local NetDB/bootstrap foundation
plan_107         = closed exploratory-tunnel substrate
plans_108_to_110 = superseded by corrected successors
plan_111         = closed final local short-build correction
plan_112         = closed outbound pre-delivery closure
plan_113         = closed reference-compatible inbound reconciliation
```

Plan 113 selected the deployed-reference-compatible inbound policy. The final
specification's creator-key sentence remains explicitly documented as a
`reference-compatible-spec-text-discrepancy`; no private field was invented
and no strict final-spec claim is made for that semantic.

## Future-plan unblock audit

The narrow qualified external-delivery checkpoint is now **unblocked and is
the next executable action**. Both outbound and inbound local short-build
surfaces are eligible for that checkpoint, subject to its independent-router
and transport qualification requirements.

Full Milestone 4B acceptance remains blocked until the checkpoint produces:

1. real exploratory tunnel build evidence against an independent router;
2. a RouterInfo lookup and valid matching response through the exploratory
   path; and
3. normal Plan 103 validation/insertion/persistence plus independently
   verified local publication.

Plan 079 is not unblocked by this record: its repeated NTCP2 development lane
remains deferred to the separate pre-normal-activation/public-network
checkpoint. Normal-daemon NTCP2 remains disabled and unenableable, and all
NTCP2 support remains experimental and non-advertised.

## Evidence and authority

The closure is supported by:

- [`plans/107-status.md`](107-status.md)
- [`plans/111-status.md`](111-status.md)
- [`plans/112-status.md`](112-status.md)
- [`plans/113-status.md`](113-status.md)
- [`specs/references/short-build-inbound-creator-key.md`](../specs/references/short-build-inbound-creator-key.md)

No live external-router or public-network result is claimed by this closure.
