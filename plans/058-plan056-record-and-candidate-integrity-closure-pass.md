# Plan 058: Plan 056 record and candidate integrity closure pass

## Status and dependencies

- Plan type: narrow corrective documentation, provenance, and execution-contract closure pass.
- Starting branch: `main` after Plan 056 closed with a typed host blocker and Plan 057 was opened.
- Starting repository state at authoring: current `main` contains the Plan 053-056 implementation surface, `plans/056-candidate.md`, `plans/056-closure.md`, and `plans/057-cross-host-milestone-3-external-evidence-run.md`.
- This plan does **not** implement the i2pd direct helper, Java support topology, or external mixed-router execution. Those belong to Plan 059 and Plan 060.
- This plan must close before a new external candidate is frozen.
- Milestone 3 remains open. NTCP2 remains experimental and non-advertised.

## Objective

Restore one internally consistent source-of-truth for the Milestone 3 evidence program before further implementation or external execution.

The completed pass must:

1. remove contradictory Plan 056 candidate/source identities;
2. accurately distinguish locally generated ignored diagnostics from committed evidence;
3. retire the current Plan 057 execution contract because it inherits a stale candidate and requires missing code while forbidding source edits;
4. replace the combined host-plus-guest preflight with two explicit alternative execution lanes;
5. establish an enforceable rule that a candidate may be frozen only after every required implementation and qualification artifact is committed;
6. make ADR 0021 approval or rejection an explicit repository decision rather than an implied future action;
7. add static and unit checks that prevent the same record-integrity regressions.

## Current defects owned by this plan

### D1. Candidate identity contradiction

The current records mention multiple source identities for one supposed candidate:

- `plans/056-candidate.md` declares `fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf`;
- its validation narrative also refers to `2457b74`;
- `plans/056-closure.md` later describes `1eb6cd640ce3c3e5141b62910fcae8d42f72c54a` as the source commit;
- required pipeline fixes landed after the declared freeze.

A candidate record may name exactly one full 40-character source SHA. Every measured source field must come from that checkout. No narrative alias, short SHA, successor commit, or “same tree digest” explanation may substitute for exact commit identity.

### D2. Local diagnostics described as committed

The current closure says the two Plan 056 diagnostic bundles and certificate are committed under `target/interop/evidence/plan056/`. The repository does not track those files. The correct status is that they were generated locally under an ignored working directory unless a bounded tracked receipt is explicitly added.

### D3. Plan 057 is not executable as written

Plan 057:

- inherits the stale Plan 056 candidate;
- forbids source changes during execution;
- requires an i2pd direct helper that does not exist;
- requires a Java support topology that does not exist and whose ADR remains Proposed;
- requires live observation markers that have not been qualified.

The plan must be marked superseded before another agent attempts it.

### D4. Host and guest gates are conflated

The current Plan 057 requires the physical host to pass the rootless user-namespace probe and also requires a Multipass guest. These are separate execution choices:

- direct-host lane: the execution host itself satisfies the rootless contract;
- guest lane: the outer host only needs to run the VM safely; the guest satisfies the rootless contract.

The negative host probe must not automatically reject a valid guest lane.

### D5. ADR 0021 has no explicit decision transition

ADR 0021 is Proposed. No Java support topology implementation may begin until the repository records either:

- `Accepted`, authorizing the bounded topology; or
- `Rejected`, which keeps `java-to-i2pr-ipv4` blocked for the pinned Java revision and prevents Milestone 3 closure under the current four-direction contract.

## Scope boundaries

### In scope

- `plans/056-candidate.md`
- `plans/056-closure.md`
- `plans/057-cross-host-milestone-3-external-evidence-run.md`
- `plans/030-milestone-3-closure.md`
- `docs/adr/0021-minimal-java-support-topology.md`
- `README.md`, `AGENTS.md`, relevant skills, and interop documentation where they repeat the stale contract
- a small machine-readable candidate-record validator if needed
- unit/static tests for candidate and closure consistency
- a `plans/058-status.md` closure record

### Out of scope

- implementing reference helpers;
- modifying NTCP2 protocol code;
- changing pinned reference versions;
- running the Java matrix;
- running mixed-router directions;
- creating a new external candidate;
- changing `specs/support.toml` to advertise NTCP2.

## Required end state

After Plan 058:

- Plan 056 remains an honest local pipeline/verifier exercise closed with a typed environment blocker.
- The old candidate is explicitly retired and cannot be used for external evidence.
- Plan 057 is explicitly superseded and cannot be treated as active execution authority.
- Plan 059 is the only active implementation/qualification pass.
- Plan 060 is the only future candidate-freeze and two-run certificate pass.
- ADR 0021 has an explicit repository decision.
- Local ignored diagnostics are described accurately.

## Phase 1: inventory every stale statement

Search the tracked tree for all of:

```text
fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf
2457b74
1eb6cd640ce3c3e5141b62910fcae8d42f72c54a
Plan 056 candidate
diagnostic bundles are committed
target/interop/evidence/plan056
Plan 057
host.apparmor-restrict-on
rootless_sandbox_available
```

Create a checklist in `plans/058-status.md` listing each affected file and the correction made. Do not rely on one global replacement: each occurrence must be reviewed semantically.

### Small-model guidance

Use this table while editing:

| Statement type | Correct action |
| --- | --- |
| Exact historical commit describing an implementation commit | Keep it. |
| Claim that one commit is the authoritative external candidate | Replace with “retired candidate” and the exact reason. |
| Claim that ignored `target/` output is committed | Replace with “locally generated and intentionally untracked.” |
| Plan 057 described as active | Mark it superseded by Plans 059 and 060. |
| Host rootless gate applied to guest lane | Split into direct-host and guest-lane requirements. |

Do not delete historical SHAs merely because they differ. The defect is using multiple SHAs as one candidate, not recording commit history.

## Phase 2: retire the Plan 056 candidate

Update `plans/056-candidate.md`:

- change status to `retired; never externally executed`;
- retain the historical declared SHA for auditability;
- state that required pipeline fixes landed afterward;
- state that no external Run A or Run B was produced from the candidate;
- state that the candidate must not be used by Plan 060;
- remove wording implying it remains the current candidate;
- retain measured historical fields only under a clearly marked historical snapshot section.

Required wording shape:

```text
Status: retired; never used for an authoritative external run.

The historical candidate SHA was <full SHA>. It is not eligible for future
Milestone 3 evidence because required execution-path fixes and missing
reference-side implementation landed or remain to land after the freeze.
A successor candidate may be cut only under Plan 060 after Plan 059 closes.
```

Do not rewrite historical measurements as if they were measured from current `main`.

## Phase 3: correct Plan 056 closure claims

Update `plans/056-closure.md` so it states precisely:

- the bundle pipeline and certificate verifier were exercised locally with synthetic blocked-direction inputs;
- the two diagnostic bundles and certificate were generated under ignored `target/` paths;
- those artifacts are not tracked repository evidence unless a separately committed bounded receipt exists;
- no mixed-router handshake was executed by the local driver;
- the verifier correctly denied the certificate;
- Plan 056 did not produce an external candidate suitable for later use;
- the closure remains a typed local-environment blocker, not Milestone 3 closure.

Replace ambiguous wording such as:

```text
prepared, executed, finalized, exported, and audited
```

with:

```text
constructed from typed synthetic blocked-direction inputs, finalized,
exported locally, and audited by the certificate verifier
```

### Optional bounded receipt

If retaining reproducibility evidence in the repository is useful, add one small tracked JSON receipt outside `target/`, for example:

```text
tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json
```

The receipt may contain only:

- schema/version;
- generator commit;
- driver path;
- run IDs;
- verifier schema;
- verifier outcome `false`;
- typed failure-count summary;
- statement `artifact_storage = local-untracked`.

It must not claim to contain the bundles or their exact manifests unless those manifests are actually tracked.

## Phase 4: explicitly supersede Plan 057

Update the top of `plans/057-cross-host-milestone-3-external-evidence-run.md`:

```text
Status: superseded before execution by Plans 058, 059, and 060.
```

Document why:

- inherited candidate was stale;
- reference-side implementation was missing;
- candidate freeze occurred before implementation closure;
- direct-host and guest-lane gates were conflated.

Do not delete Plan 057. Preserve it as an audit record of the invalid ordering.

Add a short supersession section mapping old responsibilities:

| Plan 057 responsibility | New owner |
| --- | --- |
| record/candidate correction | Plan 058 |
| i2pd direct helper | Plan 059 |
| Java topology and ADR decision | Plan 059 |
| receiver marker qualification | Plan 059 |
| new candidate freeze | Plan 060 |
| two external runs and certificate | Plan 060 |

## Phase 5: decide ADR 0021

A repository maintainer must change ADR 0021 from `Proposed` to one of:

### Accepted

Use when the project agrees that the bounded two-router sealed support topology is an acceptable test prerequisite for Java 2.12.0.

The ADR must record:

- approval date;
- approving role/name or `repository maintainer decision`;
- implementation owner Plan 059;
- unchanged prohibitions on transport/cryptographic behavior patches;
- statement that support-router traffic can never satisfy the primary direction.

### Rejected

Use when the topology is not acceptable.

The ADR and Plan 058 status must then state:

- `java-to-i2pr-ipv4` remains a typed blocker for Java 2.12.0;
- the four-direction Milestone 3 contract cannot close with the current pinned Java revision;
- a separate future plan must choose a different pinned Java revision or revise the closure contract through a new ADR;
- Plan 060 must not start.

### No silent default

Leaving the ADR Proposed does not close Plan 058.

## Phase 6: define two alternative execution lanes

Add or update an execution-contract section in the active interop documentation.

### Lane A: direct-host rootless execution

The execution host itself must satisfy:

- Ubuntu 24.04 amd64;
- rootless probe `rootless_sandbox_available`;
- reference build/runtime prerequisites;
- sufficient RAM/disk;
- no public egress during authoritative directions.

Multipass is not required.

### Lane B: guest rootless execution

The outer host must satisfy:

- enough resources to run one evidence guest;
- working VM manager and lifecycle ownership controls;
- safe source/cache transfer and evidence export.

The guest must satisfy:

- Ubuntu 24.04 amd64;
- rootless probe `rootless_sandbox_available`;
- reference build/runtime prerequisites;
- sealed execution contract.

The outer host may itself return `blocked_unprivileged_user_namespace`; that result is recorded as a host baseline but does not reject a valid guest lane.

### Shared rule

Exactly one lane is selected for a candidate. The run identity records the lane and its environment manifest. A certificate may not combine Run A from one lane and Run B from another.

## Phase 7: add candidate-integrity validation

Implement a small validator or extend the existing static checker so future candidate records fail closed.

Recommended module:

```text
tests/integration/ntcp2/harness/candidate_record.py
```

Recommended locked fields:

```text
schema = i2pr-interop-candidate-v1
candidate_commit = 40 lowercase hex
status = declared | retired | executed
implementation_floor_commit = 40 lowercase hex
plan = integer
source_tree_sha256 = 64 lowercase hex
validation_receipt_sha256 = 64 lowercase hex or typed absence
```

Required invariants:

- `candidate_commit` resolves in Git history;
- candidate record contains exactly one authoritative SHA;
- declared/executed candidate cannot be older than the implementation floor;
- retired candidate cannot be consumed by Plan 060 tooling;
- candidate document may contain historical SHAs only in a non-authoritative history list;
- active candidate record cannot mention missing required implementation artifacts;
- local-untracked diagnostics cannot be described as committed evidence.

A simpler Markdown-only validator is acceptable if it enforces the same invariants deterministically.

## Required tests

Add `tests/integration/ntcp2/harness/test_plan058.py` with at least:

1. one-authoritative-SHA positive case;
2. two-authoritative-SHA rejection;
3. short-SHA rejection;
4. retired candidate rejected by execution tooling;
5. candidate before implementation floor rejected;
6. local-untracked receipt accepted only with explicit storage classification;
7. “committed bundle” claim rejected when tracked artifact path is absent;
8. direct-host lane accepts host rootless success without Multipass;
9. guest lane accepts restrictive outer host when guest rootless succeeds;
10. guest lane rejects guest rootless failure;
11. cross-lane Run A/Run B combination rejected;
12. ADR Proposed prevents Plan 058 closure;
13. ADR Accepted permits Plan 059 activation;
14. ADR Rejected blocks Plan 060 activation.

Extend `scripts/check-ntcp2-interoperability.sh` to verify:

- Plan 057 contains the superseded marker;
- Plan 056 candidate contains the retired marker;
- Plan 060 is named as the only future candidate-freeze owner;
- ADR 0021 is not Proposed when Plan 058 status says closed;
- no documentation calls ignored Plan 056 diagnostic bundles committed evidence.

## Validation commands

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan058.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'

bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh

cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
```

Documentation-only changes still require the relevant static and full local gates because candidate status is consumed by execution tooling.

## Explicit closure criteria

Plan 058 closes only when every item is true:

- [ ] `plans/056-candidate.md` is marked retired and names one historical authoritative full SHA.
- [ ] The retired candidate is explicitly forbidden for future external execution.
- [ ] `plans/056-closure.md` accurately describes local diagnostic artifacts as locally generated and untracked, unless tracked artifacts actually exist.
- [ ] The closure no longer implies a mixed-router handshake was executed by the synthetic driver.
- [ ] `plans/057-cross-host-milestone-3-external-evidence-run.md` is marked superseded before execution.
- [ ] Responsibilities are reassigned to Plans 058-060.
- [ ] ADR 0021 is explicitly Accepted or Rejected.
- [ ] Direct-host and guest execution lanes are documented as alternatives.
- [ ] Outer-host rootless failure does not reject a valid guest lane.
- [ ] Candidate validation rejects multiple authoritative SHAs and pre-implementation freezes.
- [ ] Candidate validation rejects retired candidates.
- [ ] Static checks reject false “committed evidence” claims for absent tracked artifacts.
- [ ] `test_plan058.py` passes.
- [ ] Full Python, Rust, and boundary validation passes.
- [ ] `plans/058-status.md` records exact commands/results and remaining work.
- [ ] Milestone 3 remains open and NTCP2 remains non-advertised.

## Failure and stop conditions

Stop Plan 058 without claiming closure when:

- ADR 0021 remains Proposed;
- maintainers reject both the Java topology and any alternative four-direction path;
- documentation still contains multiple authoritative candidate SHAs;
- candidate tooling can consume a retired candidate;
- local ignored artifacts are still called committed;
- changing the support contract would require modifying Milestone 3 scope without a new ADR.

Emit a typed status such as:

```text
blocked_adr_decision_missing
blocked_candidate_record_inconsistent
blocked_evidence_storage_claim_inaccurate
blocked_execution_lane_contract_ambiguous
```

## Suggested commit sequence for a smaller model

1. `docs: retire Plan 056 candidate and correct local evidence claims`
2. `docs: supersede Plan 057 and split execution lanes`
3. `docs: decide ADR 0021 Java support topology`
4. `interop: add candidate record integrity validation`
5. `tests: add Plan 058 regression coverage`
6. `docs: record Plan 058 closure status`

Do not combine the ADR decision, validator implementation, and closure record into one commit. Smaller commits make it possible to verify the decision boundary independently.

## Required handoff artifacts

- corrected `plans/056-candidate.md`;
- corrected `plans/056-closure.md`;
- superseded `plans/057-cross-host-milestone-3-external-evidence-run.md`;
- decided `docs/adr/0021-minimal-java-support-topology.md`;
- candidate integrity validator/static gate;
- `tests/integration/ntcp2/harness/test_plan058.py`;
- `plans/058-status.md`;
- optional bounded local-diagnostic receipt with explicit untracked storage classification.
