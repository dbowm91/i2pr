# Plan 068: staged interoperability evidence and Milestone 3 authority correction

## Status and dependencies

- Status: planned.
- Parent roadmap: Plan 067.
- Must execute before Plans 069 through 073.
- Plan type: architecture/status correction, evidence-tier separation, and validation-policy simplification.
- Changes no production NTCP2 behavior.
- Performs no external interoperability run.

## Objective

Correct the active Milestone 3 planning and evidence model so development interoperability can be exercised on the current Ubuntu host without weakening eventual release qualification.

The plan must:

1. make Plan 067 the active Milestone 3 roadmap;
2. preserve Plan 066 as historical release-qualification evidence rather than active execution authority;
3. remove ADR 0021 rejection from active Java blocker logic because ADR 0022 already accepted the direct Java driver topology;
4. distinguish Level 1 smoke, Level 2 development validation, and Level 3 release qualification;
5. prevent smoke/development records from satisfying release-qualification predicates;
6. remove sealed-namespace, candidate-freeze, two-bundle, reviewer, and full provenance requirements from Level 1 and Level 2;
7. retain exact message/identity correlation, bounded execution, fresh state, and clean teardown at all levels;
8. reduce static and regression checks that validate plan-document markers instead of protocol behavior;
9. keep NTCP2 experimental and non-advertised.

## Current defects to correct

### D1. Stale Java blocker

ADR 0022 explicitly superseded the conclusion of ADR 0021 and accepted a direct Java stripped-router driver. Plans 063 and 065 implemented and wired that driver. Active Plan 066 status and freeze-readiness text nevertheless continue to treat `blocked_java_support_topology_rejected` as a current prerequisite failure.

Required correction:

- ADR 0021 remains historical and Rejected;
- ADR 0022 remains Accepted;
- the rejected support topology may not be selected;
- the direct Java driver is the active Java path;
- Java may still be unavailable because of host/runtime/build defects, but not because ADR 0021 forbids the already accepted replacement architecture.

### D2. Release evidence used as discovery gate

The current canonical path requires release-grade isolation and certification before the first independent handshake. This prevents protocol discovery on the constrained host.

Required correction:

- Level 1 and Level 2 execute directly on the host over loopback;
- Level 3 remains isolated;
- an unavailable Level 3 environment cannot block Level 1 or Level 2.

### D3. Evidence machinery dominates protocol outcome

The existing runner can classify evidence-generation or freeze-readiness failures before running the driver. The initial smoke path must instead prioritize:

```text
build -> process start -> RouterInfo exchange -> TCP -> NTCP2 handshake -> data frame -> I2NP decode -> cleanup
```

The result must preserve the earliest real failure stage.

### D4. Over-broad validation baseline

Historical plans require the complete Python harness suite, all boundary scripts, Clippy, rustdoc, fuzzing, rootless checks, and Multipass checks for every closure. That is disproportionate for a local integration-runner correction.

Required correction:

- focused checks for touched code;
- workspace format/check/test and dependency/runtime boundaries as the default baseline;
- full historical apparatus only at explicit integration checkpoints or when its surface changes.

## Deliverables

### D1. ADR 0023: staged NTCP2 interoperability evidence

Create:

```text
docs/adr/0023-staged-ntcp2-interoperability-evidence.md
```

Required ADR content:

- Status: Accepted after this plan's tests and documentation corrections pass.
- Context: Plan 066 could not execute because it coupled initial external testing to release qualification.
- Decision: Levels 0, 1, 2, 2D, and 3 as defined by Plan 067.
- Level 1 and Level 2 may run on host loopback with explicit feature disablement and optional syscall auditing.
- Level 3 requires isolated no-public-egress execution.
- i2pd is the primary initial validator.
- Emissary is conditional and secondary.
- Java and i2pd are required for release qualification.
- Exact DeliveryStatus and Router Hash correlation remain mandatory for positive external results.
- Smoke records cannot be promoted into Level 3 bundles.
- Plan 066 remains historical and its verifier may be reused only at Level 3.
- ADR 0023 does not supersede ADR 0022's direct-driver decision; it supersedes only the one-tier evidence policy.

### D2. Milestone 3 status correction

Update:

```text
plans/030-milestone-3-closure.md
```

Required active status:

```text
implementation_status = externally-testable
development_validation = pending-i2pd-loopback
release_qualification = blocked-environment
support = experimental
advertised = false
```

The document must state:

- Plan 067 is active;
- Plan 066 is historical;
- no external pass has yet occurred;
- the repository is ready to attempt Level 1;
- Level 3 remains blocked on this host;
- Milestone 4 production activation remains closed;
- later design/development work may continue after Level 2 without treating that as release qualification.

### D3. Plan 066 supersession notice

Do not rewrite Plan 066's historical results. Add an explicit supersession notice in the active planning/documentation surface stating:

- Plan 066 accurately records the failed release-qualification environment;
- Plan 066 is no longer the active gate for the first external run;
- its candidate remains non-executed;
- its certificate verifier remains a Level 3 tool;
- its `blocked_java_support_topology_rejected` active interpretation is superseded by ADR 0022 and ADR 0023.

Preferred locations:

```text
plans/066-closure.md
plans/066-fresh-candidate-and-authoritative-ntcp2-two-run-closure.md
```

Use a short top-level supersession banner. Preserve the remaining historical text.

### D4. Evidence tier types

Add a small shared type/enum module in the existing Python harness, or extend the most appropriate current module, with exact values:

```text
local-conformance
external-loopback-smoke
repeated-development-interop
conditional-differential
release-qualification
```

Rules:

- a record declares exactly one tier;
- `external-loopback-smoke` cannot satisfy development or release predicates;
- `repeated-development-interop` cannot satisfy release predicates;
- release bundle validators reject smoke/development schemas;
- historical bundle readers remain readable;
- no existing release schema is silently reinterpreted.

Recommended file:

```text
tests/integration/ntcp2/harness/evidence_tier.py
```

### D5. Minimal smoke record contract

Define the Level 1 record contract now so Plan 069 has one stable target.

Recommended schema:

```text
schema = i2pr-ntcp2-loopback-smoke-v1
schema_version = 1
```

Required fields:

```text
schema
schema_version
evidence_tier
run_id
source_commit
reference_name
reference_version
reference_revision
direction
started_utc
completed_utc
local_router_hash_sha256
peer_router_hash_sha256
delivery_status_message_id
tcp_connected
ntcp2_authenticated
frame_emitted
frame_authenticated_and_decrypted
i2np_message_decoded
cleanup_clean
network_audit
result
failure_stage
failure_reason
record_sha256
```

Allowed `network_audit` values:

```text
strace-allowlist
configuration-only
not-run
```

Allowed `result` values:

```text
passed
failed
blocked
```

Allowed high-level `failure_stage` values:

```text
none
preflight
build
process-start
router-info
connect
handshake-request
handshake-created
handshake-confirmed
data-frame-write
data-frame-authentication
i2np-decode
correlation
cleanup
network-audit
timeout
```

Rules:

- a passed record requires all positive booleans and `cleanup_clean = true`;
- a passed record may use `configuration-only`, but must not use `not-run`;
- exact Router Hashes are 64 lowercase hex;
- message ID is nonzero `u32`;
- raw payload, private key, Noise state, or full RouterInfo bytes are forbidden;
- `record_sha256` covers canonical JSON excluding itself;
- the record is diagnostic/development evidence only.

### D6. Development validation summary contract

Define a simple Level 2 aggregate schema:

```text
schema = i2pr-ntcp2-development-validation-v1
schema_version = 1
```

Required fields:

```text
schema
schema_version
evidence_tier
source_commit
reference_name
reference_version
reference_revision
directions
required_passes_per_direction
observed_passes_per_direction
negative_controls
cleanup_passed
network_audit_summary
status
summary_sha256
```

Allowed status:

```text
passed
failed
blocked
```

A passed summary requires Plan 079 criteria and cannot be consumed by the Level 3 verifier.

### D7. Static-check simplification

Review:

```text
scripts/check-ntcp2-interoperability.sh
scripts/check-rootless-interop-boundary.sh
scripts/check-multipass-interop-boundary.sh
```

Required changes:

- preserve release-schema and historical-boundary checks;
- remove requirements that every Level 1/2 run possess candidate, reviewer, rootless, Multipass, or bundle markers;
- do not require rootless/Multipass checks for loopback smoke closure;
- reject accidental use of smoke/development schemas in release bundles;
- stop checking for plan-document class names or textual markers unless they enforce a real compatibility boundary;
- keep focused checks for exact hash/message correlation and forbidden synthetic success.

### D8. Documentation updates

Update at minimum:

```text
README.md
AGENTS.md
docs/architecture/interop-apparatus.md
docs/protocol-support.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
plans/030-milestone-3-closure.md
```

Required wording:

- Plan 067 is active;
- Level 1/2 are host-compatible manual integration lanes;
- i2pd is the first required validator;
- Emissary is optional/conditional;
- Java remains a release-qualification requirement;
- release qualification remains blocked on the current host;
- NTCP2 remains experimental and non-advertised.

### D9. Closure record

Create at implementation closure:

```text
plans/068-status.md
```

Record:

- exact commit;
- ADR 0023 status;
- files changed;
- stale Java blocker removal;
- tier contracts;
- static-check simplifications;
- exact validation commands/results;
- explicit statement that no external run occurred;
- next plan: 069.

## Work packages

### WP1. Authority audit

1. Search all active docs, tests, check scripts, and candidate validators for Plan 066 authority, ADR 0021 Java blockers, and `blocked_java_support_topology_rejected`.
2. Classify each occurrence as historical or active.
3. Preserve historical occurrences.
4. Correct active occurrences.

Acceptance:

- no active freeze/readiness logic treats ADR 0021 as blocking the direct Java driver;
- historical records remain intact.

### WP2. Land ADR 0023 and tier types

1. Write ADR 0023.
2. Add evidence-tier constants and validation.
3. Add positive and negative tests.
4. Make release validators explicitly reject lower-tier records.

Acceptance:

- evidence tier is explicit and fail-closed;
- no silent promotion path exists.

### WP3. Define smoke and development schemas

1. Implement canonical serialization and digest checks.
2. Add strict unknown/missing-field rejection.
3. Add secret-bearing-field rejection where applicable.
4. Add representative positive and negative fixtures.

Acceptance:

- Plan 069 can write one record without depending on Plan 052/053 bundles;
- release bundle code cannot consume it as a pass.

### WP4. Simplify validation ownership

1. Identify static checks that merely enforce plan-document structure.
2. Remove or narrow them.
3. Keep protocol/evidence boundary checks.
4. Define the focused closure baseline for Plans 069-071.

Acceptance:

- Level 1 closure does not require rootless, Multipass, candidate, or reviewer checks;
- release checks remain available and fail closed.

### WP5. Update planning and operator documentation

1. Correct Plan 030 active status.
2. Add Plan 066 supersession notices.
3. Update README/AGENTS/architecture/support/skill docs.
4. Create Plan 068 status record only after tests pass.

Acceptance:

- one unambiguous active roadmap exists;
- readers do not infer that Java direct execution is forbidden by ADR 0021;
- no document claims external interoperability.

## Required tests

Add or update tests covering at least:

1. every evidence-tier value accepted;
2. unknown evidence tier rejected;
3. smoke record valid positive fixture;
4. missing smoke field rejected;
5. unknown smoke field rejected;
6. zero message ID rejected;
7. 40-hex Router Hash rejected;
8. pass with a false protocol boolean rejected;
9. pass with `cleanup_clean = false` rejected;
10. pass with `network_audit = not-run` rejected;
11. blocked smoke record with a typed preflight reason accepted;
12. raw payload/private-key field rejected;
13. canonical digest mismatch rejected;
14. development summary valid positive fixture;
15. development summary with fewer than required passes rejected;
16. development summary with missing negative control rejected;
17. release validator rejects smoke schema;
18. release validator rejects development schema;
19. historical Plan 052/053 bundle remains readable;
20. active freeze-readiness no longer includes ADR 0021 support-topology blocker;
21. Plan 066 remains historical/non-executed;
22. Plan 067 is named as active authority.

Recommended focused files:

```text
tests/integration/ntcp2/harness/test_evidence_tier.py
tests/integration/ntcp2/harness/test_loopback_smoke_record.py
tests/integration/ntcp2/harness/test_development_validation.py
tests/integration/ntcp2/harness/test_plan068.py
```

## Validation commands

During implementation:

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_tier.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_development_validation.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan068.py'
```

Closure baseline:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
git diff --check
```

Run the full historical Python suite once at final integration, but do not add it as a mandatory per-edit loop.

## Non-goals

Plan 068 does not:

- build or run i2pd;
- build or run Java I2P;
- implement loopback process orchestration;
- modify production NTCP2 code;
- change protocol pass predicates;
- permit public-network access;
- close development validation;
- close release qualification;
- add CI jobs.

## Stop rules

Stop and record a typed blocker when:

- tier separation cannot be added without allowing lower-tier evidence into Level 3;
- active Java blocker removal would contradict ADR 0022 source-verification facts;
- a static-check simplification would remove exact correlation or synthetic-fallback protections;
- Plan 030 cannot be corrected without erasing historical evidence;
- the implementation expands into runner/process or protocol changes owned by Plans 069/070.

## Closure criteria

Plan 068 closes only when:

- ADR 0023 is Accepted;
- Plan 067 is active authority;
- Plan 066 is historical authority only;
- ADR 0021 is not an active direct-Java blocker;
- evidence tiers are explicit;
- smoke and development schemas exist and are tested;
- lower-tier promotion into release evidence is impossible;
- Level 1/2 validation no longer requires sealed namespaces, Multipass, candidate freeze, or reviewer certification;
- documentation is internally consistent;
- focused and closure checks pass;
- `plans/068-status.md` records no external-run claim.

## Small-model handoff instructions

- Do not edit production crates.
- Do not implement Plan 069 early.
- Use repository search to classify historical versus active blocker text before editing.
- Preserve old plan content except for short supersession banners.
- Prefer one small evidence-tier module and two small schema modules over a generalized framework.
- Avoid introducing registries, plugin systems, or abstract schema engines.
- Keep tests table-driven and bounded.
- Commit architecture/schema work separately from documentation/status work when practical.
