# Milestone 3 active-status amendment: Plan 067 staged interoperability recovery

## Authority

- Date: 2026-07-31.
- Status: active planning amendment.
- Parent roadmap: `plans/067-milestone-3-staged-interoperability-corrective-roadmap.md`.
- Applies when active status or execution authority in `plans/030-milestone-3-closure.md`, Plan 066 documents, README, AGENTS, architecture documentation, or support documentation conflicts with Plan 067.
- Does not rewrite or invalidate historical evidence. It corrects which plan governs the next work.

## Corrected active Milestone 3 state

```text
implementation_status = externally-testable
development_validation = pending-i2pd-loopback
release_qualification = blocked-on-current-environment
support = experimental
advertised = false
normal_daemon_activation = disabled
active_roadmap = Plan 067
next_plan = Plan 068
```

The repository has implemented the bounded local NTCP2 protocol/runtime surface and the direct Java/i2pd reference-driver source surfaces needed to attempt external testing. It has not demonstrated a passing mixed-router direction. Both tracked qualification receipts still represent typed absence with zero live attempts.

## Plan 066 status correction

Plan 066 remains a valid historical record of the following facts:

- the rootless sealed-namespace lane was unavailable on the constrained host;
- the Multipass recovery lane was not reliable on that host;
- no two-run Level 3 certificate was produced;
- the candidate was declared non-executable;
- NTCP2 remained experimental and non-advertised.

Plan 066 is no longer the active prerequisite for the first external protocol run. Its candidate/certificate/freeze apparatus is classified as Level 3 release-qualification machinery under Plan 067 and proposed ADR 0023.

Plan 066's unavailable isolation lane does not block:

- Level 1 host-loopback smoke;
- Level 2 repeated i2pd development validation;
- conditional Level 2D Emissary differential testing.

## ADR 0021/0022 correction

ADR 0021 rejected the old Java support-router/topology proposal. ADR 0022 subsequently accepted a direct Java stripped-router driver and explicitly superseded that conclusion for the active four-direction architecture.

Therefore:

- the rejected Java support topology remains forbidden;
- `blocked_java_support_topology_rejected` is historical for Plans 058-060;
- it is not an active blocker to the direct Java driver implemented by Plan 063 and integrated by Plan 065;
- Java may be blocked by build, runtime, host isolation, or real protocol failure, but not by the rejected topology that ADR 0022 replaced.

Any active candidate/readiness code or planning documentation that still treats ADR 0021 rejection as prohibiting the ADR 0022 direct driver must be corrected by Plan 068.

## Staged closure interpretation

### Development continuation gate

The development continuation gate is Plan 071 Level 2:

- pinned i2pd;
- both directions;
- three fresh-state passes per direction;
- exact DeliveryStatus and Router Hash continuity;
- bounded negative controls;
- cleanup and explicit network audit;
- no public network participation.

Passing this gate permits continued design and implementation of later project work while NTCP2 remains experimental/non-advertised.

It does not close release qualification.

### Release qualification gate

The final Milestone 3 release gate is Plan 073 Level 3:

- pinned Java and i2pd;
- both directions for each reference;
- isolated no-public-egress lane;
- reference-driver prequalification;
- two independent complete four-direction runs;
- exact authenticated I2NP correlation;
- cleanup and sanitized durable evidence;
- final support decision.

The current host's inability to execute this gate remains truthful and does not need to be represented as a Level 1/2 failure.

## Active plan sequence

```text
067 roadmap
  -> 068 evidence/authority correction
  -> 069 host-compatible loopback smoke lane
  -> 070 first real i2pd two-way execution
  -> 071 repeated i2pd validation and negative controls
  -> 072 Emissary only if conditionally activated
  -> 073 Java and isolated release qualification when environment exists
```

## Planning-document update requirements

Plan 068 must propagate this active status to:

```text
plans/030-milestone-3-closure.md
plans/066-closure.md
plans/066-fresh-candidate-and-authoritative-ntcp2-two-run-closure.md
README.md
AGENTS.md
docs/architecture/interop-apparatus.md
docs/protocol-support.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
```

The preferred edit is a short supersession/status banner plus a reference to Plan 067. Historical details should not be rewritten.

## Non-claims

This amendment does not claim:

- a successful i2pd run;
- a successful Java run;
- mixed-router interoperability;
- release qualification;
- readiness to advertise NTCP2;
- readiness for public-network operation;
- Milestone 4 production activation.

It states only that the repository is ready to attempt a simpler external test and that the unavailable Level 3 environment should not prevent that attempt.
