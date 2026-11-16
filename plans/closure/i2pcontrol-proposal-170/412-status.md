# Plan 412 — Java no-auth ELS2 requester direction: status

Status: **blocked-java-floodfill-selection-requires-plan437-truthful-bandwidth-tier**.

Plan of record:
[`412-java-els2-noauth-requester-direction.md`](../../implementation/i2pcontrol-proposal-170/412-java-els2-noauth-requester-direction.md).

Classification: capability evidence plus test infrastructure. No live ELS2
attempt was made; this record closes the plan's eligibility audit and hands
the capability attempt to Plan 413 after its external prerequisite passes.

## Blocker and evidence

Plan 412 requires Java client JC to query controlled i2pr floodfill F. Its
readiness omitted the candidate-selection prerequisite already recorded by
Plan 306: at exact-pinned Java I2P 2.13.0, the controlled `fR` RouterInfo is
loaded and parsed but is not selected while its bandwidth tier is `Unknown`.
The plan must not equate a seeded RouterInfo with a real lookup. Plan 437 owns
the truthful bandwidth class and Java floodfill selection proof; Plan 437 is
blocked on Plans 433 and 436. Therefore Plan 412 was not dependency-ready and
could not produce valid live evidence on the available plan path.

Primary evidence:

- `plans/closure/floodfill/306-status.md`, especially the Java selection
  boundary and the prohibition on fabricating a tier.
- `plans/implementation/floodfill/437-truthful-bandwidth-tier-and-java-selection.md`,
  which requires an actual bandwidth-backed class and names Plans 433 and 436
  as hard prerequisites.
- `plans/subsystems/core-router-recovery-roadmap.md` §6, which preserves the
  dependency order 433 → 437 → 438.

## Work retained

Plan 411's diagnosis was incorporated into the Plan 412 handoff. A standalone
stock Java SAM STREAM B33 requester helper was added at
`tests/integration/els2/clients/java_sam_stream_b33.py`. It is bounded, emits
only categorical stages and hashes, and does not retain raw SAM replies or
transient destination material. It compiled with `python3 -m py_compile` and
`scripts/check-tooling-inventory.py` passed after its inventory entry was
updated. The helper was not run against Java because the controlled-F query
prerequisite is unqualified.

The tentative runner and test scaffolding were removed before commit when the
dependency audit showed they could not establish the required F-selection
fact. No production code, Java reference, support metadata, or advertisement
was changed. No external processes were started for Plan 412.

## Requirements and outcomes

| Requirement | Evidence | Result |
|---|---|---|
| preserve Plan 411's explicit loopback DATAGRAM-port finding | Plan 412 implementation handoff and Plan 413 successor | pass |
| determine whether Java can select controlled F under the currently proven profile | Plan 306 closure: tier `Unknown`, candidate withheld | blocked; no honest candidate-selection evidence |
| avoid spending a live attempt without a selectable F | no Plan 412 runner invocation or external process | pass |
| preserve a bounded stock SAM client component | `java_sam_stream_b33.py`; `py_compile` and tooling inventory check | partial infrastructure only |
| complete ELS2 requester lookup/decrypt/payload qualification | no live execution | not passed |

## Findings and limits

- **Critical/high:** none identified.
- **Medium:** Java requester interoperability remains unqualified. No type-5
  store/lookup/decrypt/payload result exists.
- **Low:** Plan 412's initial readiness omitted a known hard dependency on
  truthful Java floodfill selection. Plan 413 carries the corrected gate.
- No protocol support, conformance, RouterInfo capability, or public
  reachability claim follows from this blocked audit.

## Roadmap disposition and unblock audit

Plan 412 is blocked, not passed. Plan 413 is registered blocked on Plan 437;
it must remain inactive until Plan 437's exact Java candidate-selection proof
passes. Plan 375 remains blocked until its Java driver, both directions, and
required matrix are complete. Plan 377 remains blocked on 375; Plan 378
remains blocked on 377.

| Plan | Dependency state | Disposition |
|---|---|---|
| 413 | Plan 437 is blocked on 433 and 436 | registered blocked; do not start |
| 375 | Java ELS2 driver and full matrix incomplete | remains blocked |
| 377 | Plan 375 not passed | remains blocked on 375 |
| 378 | Plan 377 not passed | remains blocked on 377 |

No capability promotion follows.
