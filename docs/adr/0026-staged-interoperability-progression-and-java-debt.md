# ADR 0026: Staged interoperability progression and retained Java-router compatibility debt

- Status: Accepted
- Date: 2026-09-24
- Decision owner: repository maintainer
- Related: ADR 0021, specs/CONFORMANCE.md, Plans 193, 201, 204, 247, 248

## Context

The M6 mixed-router program accumulated a long Java I2P second-family qualification chain
after the exact-pinned i2pd lane had already demonstrated a complete bidirectional
Streaming matrix. The Java work remained useful diagnostic evidence, but by Plans 244-247
the active boundary had moved into the controlled reference topology and Java Streaming
scheduler rather than an i2pr-visible wire failure.

Current evidence is materially asymmetric:

- Plan 193 passed exact-pinned i2pd 2.61.0 twice end-to-end in both directions, with real
  tunnel delivery, LeaseSet2 publication/lookup, small and multi-packet traffic, reverse
  traffic, close/EOF, sibling isolation, and loss recovery.
- Plan 232 proved raw Java-to-i2pr Destination reverse delivery.
- Plans 243-246 repeatedly established the forward Java Streaming direction.
- Plan 244 bounded the reverse path before a Java response packet was constructed.
- Plan 245 observed a Java scheduler reschedule branch instead of the send branch.
- Plan 246 ended at an observer gap.
- Plan 247 corrected a harness parser/schema defect and prepared live polling, without
  proving a production i2pr defect.

Continuing the Java lane as the next hard gate would primarily investigate exact-pinned
Java SimpleTimer2 / Streaming delayed-ACK behavior in a synthetic topology.

The former gate also exceeded the canonical distinction in specs/CONFORMANCE.md: the
two-independent-implementation rule is for router-to-router protocol conformance.
Streaming, SAM, I2CP, and service-tunnel profiles are client/application surfaces even
when their end-to-end tests traverse router infrastructure.

ADR 0021 already records the related principle that lack of a narrow Java router seam does
not automatically justify building increasingly elaborate support topology.

## Decision

i2pr separates **experimental development progression** from **full router-to-router
conformance / advertisement**.

### Experimental development progression

A non-advertised experimental subsystem may progress after its local conformance
requirements are satisfied and at least one exact-pinned independent implementation
demonstrates the relevant external path in a controlled, fail-closed lane.

For router-stack progression, i2pd is the primary independent router oracle unless a plan
documents why another implementation is a better fit.

Passing this threshold permits later implementation work only. It does not authorize
public-network exposure, production-readiness language, or capability/router.version
advertisement beyond the demonstrated subset.

### Full router-to-router conformance / advertisement

A router-to-router capability intended for a full interoperability claim or broad truthful
advertisement still requires interoperability against at least two independent
implementation families, plus all other specs/CONFORMANCE.md requirements.

Java I2P and I2P+ count as one family. i2pd remains independent. Emissary/go-i2p may count
when the exact surface is sufficiently complete and qualified.

### Client/application protocols

Streaming, SAM, I2CP, and service-tunnel profiles use evidence appropriate to their
surface: independent clients, fixed vectors, router-backed paths, and protocol transcripts
as applicable. They do not inherit a mandatory second full-router-family gate solely
because their tests traverse routers.

### Java disposition

The full Java-router M6 lane is retained as explicit nonblocking compatibility debt at the
Plan 247 boundary. Plan 201 becomes retained/deferred rather than a progression blocker.

No automatic Plan-247 timer/scheduler successor is authorized. Resuming the full Java
router lane requires an explicit new plan-of-record tied to a concrete i2pr compatibility
decision.

Java remains a first-class source/reference implementation for underspecified behavior,
I2CP client qualification, vectors, and later second-family router qualification.

For Java Streaming compatibility, prefer a narrow Java Streaming client-library lane over
I2CP to an already-qualified router before reconstructing the full controlled Java-router
topology.

## Current authority transition

- Plan 193 + retained local M6 evidence satisfies experimental mixed-router progression.
- Plan 247 remains the latest trustworthy full-Java-router diagnostic boundary and is not
  relabeled as a pass.
- Plan 215 remains M10 product authority.
- Plan 204's Java-dependent convergence gate is superseded by Plan 248; Plan 204 is not
  retroactively executed.
- Full two-family M6 router conformance remains not claimed.
- M11 transit-tunnel participation is the next router-development phase.
- M12 floodfill operation follows after controlled M11 transit/resource closure.

## Consequences

This resumes router development without discarding Java evidence or lowering the release /
advertisement bar. Historical Java diagnostics remain available, while the next hard gate
moves to an actual missing router responsibility: accepting and forwarding tunnels for
other routers.

A future production/public phase must return to second-family router-to-router
qualification.

## Rejected alternatives

### Continue Plan 247 live integration as the next hard gate

Rejected as the default path. It would primarily improve observability of reference-router
delayed-ACK scheduling and is not currently tied to an i2pr wire defect.

### Declare Java passed

Rejected. The reverse Streaming direction did not establish.

### Delete the Java harness

Rejected. The source locks and diagnostics remain valuable compatibility evidence.

### Drop the two-independent-router rule

Rejected. It remains mandatory for full router-to-router conformance / broad
advertisement.

## Review triggers

Review this ADR when i2pr prepares broad router capability advertisement or public-network
participation, a Java/i2pd disagreement affects wire compatibility or security, Java gains
a substantially narrower deterministic qualification seam, another independent router
becomes suitable, or M11/M12 demonstrates that one-family development qualification masked
a concrete interop defect.
