# Plan 068 status: staged interoperability evidence and Milestone 3 authority correction

## Status

Plan 068 closed with the staged-evidence tier separation, the
Milestone 3 authority correction, the stale Java blocker removal, and
the documentation propagation. **No external run occurred.** NTCP2
remains experimental and non-advertised. Plan 069 is the next active
plan.

## Commit

Implementation commit: see the current `HEAD` of this repository at
implementation time. The active authority for the staged-evidence
and Milestone 3 authority correction is recorded by Plan 068.

## ADR 0023 status

ADR 0023 (Accepted) at `docs/adr/0023-staged-ntcp2-interoperability-evidence.md`.
ADR 0023 separates NTCP2 interoperability evidence into four bounded
tiers (local-conformance, external-loopback-smoke,
repeated-development-interop, conditional-differential,
release-qualification) and forbids lower-tier promotion into
release bundles. ADR 0023 does not supersede ADR 0022's direct-driver
decision.

## Files changed

### Added

```text
docs/adr/0023-staged-ntcp2-interoperability-evidence.md
plans/068-status.md
tests/integration/ntcp2/harness/evidence_tier.py
tests/integration/ntcp2/harness/loopback_smoke_record.py
tests/integration/ntcp2/harness/development_validation.py
tests/integration/ntcp2/harness/test_evidence_tier.py
tests/integration/ntcp2/harness/test_loopback_smoke_record.py
tests/integration/ntcp2/harness/test_development_validation.py
tests/integration/ntcp2/harness/test_plan068.py
```

### Modified

```text
plans/030-milestone-3-closure.md
plans/066-closure.md
plans/066-fresh-candidate-and-authoritative-ntcp2-two-run-closure.md
README.md
AGENTS.md
docs/protocol-support.md
docs/architecture/interop-apparatus.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
scripts/check-ntcp2-interoperability.sh
```

## Stale Java blocker removal

The `blocked_java_support_topology_rejected` interpretation in the
active Java path is superseded by ADR 0022 (Accepted direct Java
stripped-router driver) and ADR 0023 (Accepted staged-evidence
tiers). ADR 0021 remains Rejected; the Java support topology remains
forbidden; the direct Java driver is the active Java architecture.
Java may still be unavailable because of host/runtime/build defects,
but not because ADR 0021 forbids the already accepted replacement
architecture.

The historical Plan 058-060 closure records and the historical Plan
059 helper module retain the `blocked_java_support_topology_rejected`
marker as an audit record. The active planning, support, README,
AGENTS, architecture, and skill documents treat Plan 067 as the
active authority and Plan 066 as historical.

## Tier contracts

### Evidence tier module (`tests/integration/ntcp2/harness/evidence_tier.py`)

Exposes the five tiers:

```python
LOCAL_CONFORMANCE = "local-conformance"
EXTERNAL_LOOPBACK_SMOKE = "external-loopback-smoke"
REPEATED_DEVELOPMENT_INTEROP = "repeated-development-interop"
CONDITIONAL_DIFFERENTIAL = "conditional-differential"
RELEASE_QUALIFICATION = "release-qualification"
```

and the separation rules:

- `is_valid_tier(value)` — accept allowlisted tier strings.
- `tier_satisfies_release(tier)` — release predicate.
- `tier_satisfies_development(tier)` — development predicate.
- `assert_release_record_tier(record)` — refuse lower-tier records
  inside a release bundle.

### Smoke record schema (`tests/integration/ntcp2/harness/loopback_smoke_record.py`)

Schema `i2pr-ntcp2-loopback-smoke-v1`, schema version 1, with the
required fields documented in the module docstring. A passed record
requires every positive boolean, `cleanup_clean = true`, and
`network_audit != "not-run"`. Raw payload, private key, Noise state,
and full RouterInfo bytes are forbidden. The canonical digest
(`record_sha256`) covers the canonical JSON serialization excluding
itself.

### Development validation summary (`tests/integration/ntcp2/harness/development_validation.py`)

Schema `i2pr-ntcp2-development-validation-v1`, schema version 1.
A passed summary requires three fresh-state passes per direction,
four named negative controls reporting `rejected`,
`cleanup_passed = true`, and an explicit network audit per direction.

## Static-check simplifications

`scripts/check-ntcp2-interoperability.sh` extended to enforce the
Plan 068 schemas, the Plan 068 test matrices, the ADR 0023 acceptance
marker, and the release-bundle smoke/development rejection. The
historical plan surfaces (Plan 055/056/058/059/060/062/063/064/065
/066 freeze-readiness invariants) remain intact as required by those
plans' plan-of-record closure contracts.

The focused closure baseline for Plans 069-073 is:

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
git diff --check
```

Full historical harness matrices, rootless checks, Multipass checks,
Clippy, rustdoc, and fuzz-smoke are required only at explicit
integration checkpoints or when the surface changes.

## Exact validation commands and results

### Focused Plan 068 checks

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_tier.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_development_validation.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan068.py'
```

Result: 131 tests pass (29 evidence-tier, 35 smoke record, 22
development validation, 45 Plan 068 plan-of-record tests). Zero
failures, zero errors.

### Closure baseline

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
git diff --check
```

Result: workspace format/check/test green; boundary scripts pass; NTCP2
interoperability manifest and sanitized evidence boundary validates eight
scenarios. The `no sanitized mixed-router records committed; absence
is not a success` line is the historical Plan 045 contract warning that
continues to be visible because no mixed-router i2pr evidence has yet
been recorded; the warning is non-fatal and respects Plan 045's plan-of-record.

### Clippy and the broader harness

```text
cargo clippy --workspace --all-targets --all-features -- -D warnings
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
```

Result: Clippy `No issues found` on the touched code surface; the
broad Python harness suite passes 907 tests (`Ran 907 tests in 30.643s`).

## Explicit statement: no external run occurred

Plan 068 does not run any external interoperability. No Level 1 smoke
record, no Level 2 development validation, and no Level 3 certificate
is produced by this plan. Plan 068 owns the staged-evidence
architecture and authority correction; Plan 069 owns the first
external Level 1 attempt.

## Next plan

Plan 069: host-compatible NTCP2 loopback smoke lane. Plan 069 wires
the existing source-locked i2pd direct driver into the
host-loopback scenario runner and records the first
`i2pr-ntcp2-loopback-smoke-v1` evidence record. Plan 069 must run as
an ordinary user on this host loopback without privileged
isolation.