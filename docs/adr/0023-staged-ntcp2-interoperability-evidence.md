# ADR 0023: Staged NTCP2 interoperability evidence

- Status: Accepted
- Date: 2026-07-31
- Decision owner: repository maintainer
- Parent roadmap: Plan 067
- Parent plan: Plan 068
- Supersedes: only the one-tier evidence policy of Plan 066/058/060 and the
  one-tier evidence expectation in Plan 036/038. Does not supersede ADR 0022
  (the direct reference router NTCP2 interop drivers decision).

## Context

The Plan 038/040/041/043/044/045/052/053/054/055/056/058/059/060/066 plan
sequence implemented extensive bounded local NTCP2 protocol and runtime
evidence, a source-locked Java direct driver, a source-locked i2pd direct
driver, exact DeliveryStatus correlation, and a canonical evidence pipeline.
However, the Plan 066 candidate was `declared-not-executable` on this host
because both the Plan 046 rootless sealed-namespace lane and the Plan
048/049 Multipass recovery lane are unavailable here (the host kernel
activates `kernel.apparmor_restrict_unprivileged_userns=1`, and the host
cannot sustain the Multipass guest per Plan 051).

The previous evidence model coupled the first external handshake to the
release-qualification apparatus: rootless namespace isolation, Multipass
lifecycle, two-run candidate freeze, two-bundle certificate verification,
and reviewer review were all required before any simple two-process
interoperability result could be recorded. That coupling prevents
protocol discovery on the constrained host, leaves the substantial
existing implementation untested against any independent implementation,
and lets an unavailable Level 3 environment silently block every Level 1
or Level 2 attempt.

Plan 067 is the active Milestone 3 corrective roadmap. Plan 068
corrects the active planning premise, the ADR 0021 Java blocker (because
ADR 0022 already accepted the direct Java driver topology), and the
static-check scope. This ADR records the staged-evidence decision
proposed by Plan 067 and implemented by Plan 068.

## Decision

ADR 0023 is **Accepted**. The repository separates NTCP2 interoperability
evidence into four tiers:

1. **Level 1 — external loopback smoke** (`evidence_tier =
   external-loopback-smoke`). Two real processes (one i2pr process and one
   independent reference process) on the host loopback. Fresh temporary
   identity and state directories, ephemeral ports, network ID 99, no
   reseed/NetDB bootstrap/SAM/I2CP/SSU2/DNS, exact DeliveryStatus and
   Router Hash correlation, bounded deadlines, clean shutdown, and a
   concise non-authoritative smoke record. Optional `strace-allowlist`
   network syscall audit or `configuration-only` audit when ptrace is
   unavailable. No rootless namespace, no Multipass, no candidate freeze,
   no two-bundle certificate, no reviewer review, and no Java I2P required.

2. **Level 2 — repeated development interoperability**
   (`evidence_tier = repeated-development-interop`). Both directions
   against the primary independent validator (pinned i2pd 2.60.0), three
   independent fresh-state repetitions per direction, exact message and
   identity correlation, bounded negative controls, clean process/socket/
   state teardown, and an explicit network audit per direction. No rootless
   namespace, no Multipass, no candidate freeze, no two-bundle certificate,
   and no reviewer review required.

3. **Level 2D — conditional Emissary differential validation**
   (`evidence_tier = conditional-differential`). A second-implementation
   comparison only when (a) i2pr and i2pd disagree and the failure
   cannot be localized from the specification and structured events,
   (b) a third implementation materially reduces uncertainty, or (c)
   the project explicitly chooses to increase confidence before Java
   qualification. Optional, non-blocking, never the sole conformance
   authority.

4. **Level 3 — release qualification** (`evidence_tier =
   release-qualification`). Java I2P 2.12.0 and i2pd 2.60.0, both
   directions for each reference, isolated no-public-egress lane,
   reproducible source/reference provenance, exact authenticated
   data-phase message correlation, independent fresh state, cleanup and
   residual-state verification, sanitized durable evidence, and the
   final Milestone 3 release decision. The Plan 066 certificate
   verifier may be reused or simplified at Level 3.

### Tier separation rules

- A record declares exactly one tier. Promotion from a lower tier to a
  higher tier is forbidden. A `release-qualification` bundle validator
  must refuse a record that carries `external-loopback-smoke`,
  `repeated-development-interop`, or `conditional-differential` tier.
- `external-loopback-smoke` cannot satisfy development or release
  predicates. `repeated-development-interop` cannot satisfy release
  predicates. `conditional-differential` cannot substitute for the
  required Java + i2pd release-qualification matrix.
- Historical Plan 052/053/056/058/059/066 bundle readers remain
  readable for audit; no existing release schema is silently
  reinterpreted.

### Architecture invariant

ADR 0023 does not supersede ADR 0022's direct-driver decision. The Java
direct driver and the i2pd direct driver remain the active reference
drivers. The Java support topology remains forbidden. Java may still be
unavailable because of host/runtime/build defects, but not because ADR
0021 forbids the already accepted replacement architecture.

### i2pd as the primary initial validator

For Plan 069/070/071 the pinned i2pd 2.60.0 is the primary independent
validator because the source-locked i2pd direct driver is host-buildable
on Ubuntu 24.04 and does not require the unavailable Plan 046 rootless
sealed-namespace lane. Java remains required for release qualification
(Plan 073).

### Validation policy

ADR 0023 narrows the static-check scope:

- Plan 038/045/052/053/056/058/059/066 plan-document marker checks remain
  for code that explicitly implements those plans.
- The closure baseline for Plans 069-073 is the focused touched-code test
  suite plus `cargo fmt --all --check`, `cargo check --workspace
  --all-targets`, `cargo test --workspace`,
  `scripts/check-dependency-direction.sh`, and
  `scripts/check-runtime-boundaries.sh`.
- Full historical harness matrices, rootless checks, Multipass checks,
  and Clippy/rustdoc are required only at explicit integration
  checkpoints or when the surface changes.

### Why this matters now

The repository has substantial implemented NTCP2 protocol and runtime
code, but the previous evidence model required release-grade isolation
before any simple two-process interoperability result was recorded. The
constrained host cannot create the required isolation. ADR 0023 makes
that decoupling explicit so that:

- protocol discovery (Level 1) and development validation (Level 2) can
  proceed on the current host without representing the unavailable
  Level 3 environment as a failure;
- later design and implementation work can continue after Level 2
  without treating that continuation as release qualification;
- the Plan 066 release-qualification apparatus remains available and
  authoritative for Level 3 and is not silently weakened by these
  tiers.

## Consequences

Positive:

- The repository can attempt the first external NTCP2 handshake on the
  current host.
- The Milestone 3 evidence model gains honest separation between
  discovery, development, and release.
- The static-check scope shrinks to a focused touched-code baseline
  for Level 1 and Level 2 plans.
- NTCP2 stays experimental and non-advertised; the support ledger is
  not advanced by lower-tier evidence.

Negative:

- A Level 1 or Level 2 pass does not justify advertising or enabling
  NTCP2 beyond experimental development use.
- The Plan 066 candidate/certificate/freeze machinery is unused for
  Level 1 and Level 2; it remains a Level 3 tool.

## Plan owners

- Plan 067: roadmap.
- Plan 068: this ADR, the staged evidence tier types, the smoke and
  development schemas, and the static-check simplification.
- Plan 069: Level 1 host-compatible loopback smoke lane.
- Plan 070: first real i2pd two-way execution.
- Plan 071: Level 2 repeated i2pd validation and negative controls.
- Plan 072: conditional Emissary differential lane.
- Plan 073: deferred Java and release qualification closure (Level 3).