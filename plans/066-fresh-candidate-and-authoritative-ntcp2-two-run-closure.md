# Plan 066: fresh candidate and authoritative NTCP2 two-run closure

## Status and dependencies

> **Supersession notice (Plan 068, ADR 0023 Accepted).** Plan 066 is
> the historical record of the failed release-qualification environment
> on the constrained host. Plan 067 (with Plan 068 as the authority
> correction) is the active Milestone 3 roadmap. The Plan 066
> implementation surface (plan066.py, test_plan066.py,
> candidate/closure markers, static boundary checks for the historical
> freeze-readiness invariants) is mandatory regardless of close outcome
> as an audit record, but the Plan 066 two-run certificate is no longer
> the active gate for the first external protocol run; that role
> belongs to Plan 069 under ADR 0023. The historical text below is
> preserved verbatim.

- Status: historical (superseded by Plan 067 + Plan 068).
- Parent roadmap: Plan 061 (historical); Plan 067 (active).
- The Plan 066 candidate was `declared-not-executable` on this host.
- Plan type: final candidate freeze, authoritative Run A/Run B execution, certificate verification, independent review, and Milestone 3 release-qualification decision (Level 3 tool under ADR 0023).
- Execution-only after freeze. Any implementation/configuration/reference/driver/schema/observer/verifier change retires the candidate and returns work to the owning earlier plan.
- NTCP2 remains experimental and `advertised = false` unless a separate later product-readiness plan changes that status.

## Objective

Cut one fresh candidate from the exact Plan 065 implementation floor and produce two independent complete mixed-router NTCP2 evidence bundles from that candidate.

Each bundle must contain exactly:

1. `i2pr-to-java-ipv4`;
2. `java-to-i2pr-ipv4`;
3. `i2pr-to-i2pd-ipv4`;
4. `i2pd-to-i2pr-ipv4`.

The certificate verifier must report `verified: true` over both bundles, and an independent review must accept the certificate before Milestone 3 closes.

## Candidate principles

### Fresh candidate only

The candidate must:

- descend from the exact Plan 065 implementation-floor commit;
- include all Plan 062 schema/ADR corrections;
- include the qualified Plan 063 Java driver;
- include the qualified Plan 064 i2pd driver/observer;
- include Plan 065 canonical integration and all fixes discovered during live qualification;
- include no uncommitted or guest-local edits;
- include no placeholder or zero provenance values;
- not reuse Plan 056 or Plan 060 candidate records;
- not rewrite a previous candidate SHA in place.

### Candidate record

Create:

```text
plans/066-candidate.md
```

Use the repository's active candidate schema, updated by Plan 062 as needed.

Required fields include:

```text
status = declared
executed_source_commit = <40 lowercase git SHA>
implementation_floor_commit = <Plan 065 closure implementation SHA>
closure_record_commit = <typed absence until closure>
source_tree_sha256
source_archive_sha256
clean_tree = true
launcher_binary_sha256
rustc_version
cargo_version
target_triple
trigger_schema_sha256
reference_event_schema_sha256
observation_schema_sha256
scenario_renderer_sha256
mixed_runner_sha256
bundle_verifier_sha256
java_reference_revision
java_reference_tree_sha256
java_driver_source_sha256
java_driver_binary_sha256
java_classpath_manifest_sha256
java_qualification_receipt_sha256
i2pd_reference_revision
i2pd_reference_tree_sha256
i2pd_observer_patch_sha256
i2pd_instrumented_binary_sha256
i2pd_uninstrumented_binary_sha256
i2pd_linked_library_manifest_sha256
i2pd_qualification_receipt_sha256
reference_lock_sha256
execution_environment_sha256
execution_lane
validation_receipt_sha256
```

Every SHA-256 is full 64 lowercase hex and nonzero.

### Self-reference handling

Do not misrepresent a documentation successor commit as the executed source commit.

Preferred sequence:

1. create candidate declaration inputs without self-referential digest fields;
2. freeze the exact executable source commit/tag;
3. generate an external candidate receipt bound to that commit;
4. execute both runs;
5. commit final candidate/closure records afterward, clearly separating `executed_source_commit` from `closure_record_commit`.

If repository tooling already provides a safe self-consistent method, use it and document it. Never substitute a later documentation commit for the executed binary source.

## Phase 1: verify all prerequisites

Before candidate preparation, verify:

### Plan 062

- ADR 0022 Accepted;
- Plan 060 candidate retired;
- trigger schema v4 active;
- Router Hash 64-hex contract active;
- exact DeliveryStatus correlation active;
- legacy v3/generic-log artifacts non-promotable.

### Plan 063

- Java direct driver receipt `qualified=true`;
- listen and dial 10/10;
- source/classpath/binary digests nonzero;
- exact receive-handler semantics documented;
- cleanup verified.

### Plan 064

- i2pd direct driver receipt `qualified=true`;
- listen and dial 10/10;
- observer behavior-neutrality verified;
- instrumented/uninstrumented binary and linked-library digests nonzero;
- cleanup verified.

### Plan 065

- exact implementation-floor commit recorded;
- one complete four-direction live diagnostic bundle exists;
- independent verifier accepted it;
- no unresolved implementation blocker;
- full local/static validation passed;
- support remains non-advertised.

If any prerequisite fails, stop. Do not create a candidate.

## Phase 2: choose and lock one execution lane

Use exactly one lane for Run A and Run B.

### Lane A: dedicated direct host

Requirements:

- Ubuntu 24.04 amd64;
- positive rootless sealed-namespace probe;
- sufficient CPU/RAM/disk;
- exact tool/reference environment;
- no unrelated router/reference process;
- reliable no-public-egress enforcement;
- parent network state capture/restoration.

### Lane B: owned guest

Requirements:

- one fresh owned Ubuntu 24.04 amd64 guest generation at a time;
- positive rootless probe inside guest immediately before router start;
- outer host lifecycle ownership proof;
- exact source/cache transfer digests;
- sufficient guest resources;
- no host-policy mutation needed;
- isolated execution namespace inside guest;
- reliable evidence export and guest destruction.

### Lane lock record

Record:

```text
lane_kind
outer_host_baseline
guest_generation_policy
guest_probe_outcome
direct_host_probe_outcome
environment_manifest_sha256
vm_manager_version
kernel_version
rootless_probe_sha256
```

Run A and Run B may use different fresh guest generations but must use the same lane kind and equivalent environment contract. Cross-lane certificates fail.

## Phase 3: full pre-freeze validation

Run from a clean tree at the prospective candidate commit:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan066.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan064.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan063.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan062.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'

bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh

git diff --check
```

Also run compact live preflight controls in the selected lane:

- Java listen and dial one-shot controls;
- i2pd instrumented listen and dial one-shot controls;
- i2pd uninstrumented behavior control;
- one i2pr launcher sender exact-ID control;
- one i2pr launcher receiver exact-ID control;
- one bundle verifier positive fixture;
- one wrong-ID negative fixture;
- one no-egress and cleanup control.

Any failure blocks freeze.

## Phase 4: freeze candidate

1. verify repository clean;
2. verify current commit descends from Plan 065 implementation floor;
3. generate source archive from exact commit;
4. compute tree/archive/binary/schema/driver/reference/environment digests;
5. validate both qualification receipts;
6. validate no zero/placeholders;
7. record one lane lock;
8. freeze commit/tag/receipt;
9. prohibit further source/configuration changes.

### Candidate invalidation

Retire candidate immediately when:

- source tree changes;
- launcher or scenario renderer changes;
- Java driver/classpath/reference changes;
- i2pd driver/observer/linked library/reference changes;
- trigger/event/observation schema changes;
- topology or no-egress configuration changes;
- verifier changes;
- a run reveals a required code/configuration fix;
- any provenance digest was wrong;
- an uncommitted guest edit occurred.

Do not “refresh” the candidate by replacing its SHA in the same record.

## Phase 5: provision Run A

Use fresh mutable state for every direction.

Preflight:

- exact candidate archive installed;
- exact launcher built/verified;
- exact Java artifacts verified;
- exact i2pd artifacts verified;
- exact verifier installed;
- rootless probe positive;
- parent network baseline captured;
- no residual router/reference process;
- no residual namespace/veth/port/lock;
- no prior run state reused;
- diagnostics sanitized;
- public route absent in execution namespace.

Recommended run ID:

```text
plan066-a-YYYYMMDDhhmmss-<8hex>
```

Generate four unique nonzero DeliveryStatus IDs.

## Phase 6: authoritative Run A

Use order:

```text
i2pr-to-java-ipv4
java-to-i2pr-ipv4
i2pr-to-i2pd-ipv4
i2pd-to-i2pr-ipv4
```

For each direction:

1. verify candidate/run identity;
2. create fresh i2pr state;
3. create fresh reference state;
4. verify driver/reference/build digests;
5. derive unique DeliveryStatus ID;
6. generate receiver RouterInfo;
7. validate/export/import exact RouterInfo;
8. create sealed two-process topology;
9. verify no default route/DNS/public egress;
10. start receiver and wait for structured readiness;
11. start one-shot sender/dialer;
12. collect trigger v4;
13. collect sender structured events;
14. collect receiver structured events;
15. require exact message ID and Router Hash continuity;
16. stop sender/receiver;
17. verify clean teardown;
18. destroy topology;
19. verify parent network unchanged;
20. write sanitized direction artifacts.

### Direction pass predicate

Use the exact Plan 065 predicate. Do not define a second weaker implementation in Plan 066.

### Run A failure rule

On any failure:

- complete cleanup;
- finalize diagnostic failed bundle when possible;
- do not continue toward certificate closure;
- do not rerun only the failed direction into the same aggregate;
- classify environmental versus implementation failure;
- if any artifact/configuration/source change is required, retire candidate;
- if failure is proven transient environment-only and no artifact changed, a full fresh Run A retry may use a new run ID after explicit operator decision and retained failure evidence.

## Phase 7: finalize Run A

Require exactly four direction IDs in every required artifact class.

1. validate all schemas;
2. validate run/candidate identity;
3. recompute all file digests;
4. generate aggregate manifest;
5. generate checksums;
6. atomically finalize;
7. export to durable sanitized storage;
8. independently verify exported copy;
9. write verification acknowledgement;
10. record bundle manifest SHA-256.

Do not modify finalized bundle.

## Phase 8: provision independent Run B

Run B must be operationally independent:

- new run ID;
- new direction message IDs;
- fresh i2pr identities/static keys;
- fresh Java state;
- fresh i2pd state;
- fresh namespace topology;
- fresh process instances;
- fresh guest generation when using guest lane;
- no writable state copied from Run A;
- same candidate and immutable build/reference artifacts.

Recommended run ID:

```text
plan066-b-YYYYMMDDhhmmss-<8hex>
```

Run B must use reverse direction order:

```text
i2pd-to-i2pr-ipv4
i2pr-to-i2pd-ipv4
java-to-i2pr-ipv4
i2pr-to-java-ipv4
```

This reduces order-dependent false confidence.

## Phase 9: authoritative Run B

Repeat the complete Run A procedure with fresh state and reverse order.

Apply identical pass predicate and failure rules.

Do not reuse any Run A trigger/event/observation/attestation/cleanup file.

## Phase 10: cross-bundle verification

The certificate verifier must reject unless:

- both bundles individually verify;
- both use same candidate commit/tree/archive/launcher;
- both use same Java reference/driver/classpath artifacts;
- both use same i2pd reference/driver/observer/library artifacts;
- both use same schemas/verifier;
- both use same lane kind/environment contract;
- run IDs differ;
- all eight direction IDs are correctly scoped to their bundle;
- all eight DeliveryStatus IDs are distinct as required by policy;
- mutable state digests differ across runs;
- no event/trigger/observation file is reused;
- direction order differs;
- no synthetic fallback appears;
- all sixteen process cleanups are clean;
- parent network state is unchanged for every direction;
- bundle digests differ while immutable candidate digests agree.

Output:

```text
plans/066-certificate.json
```

or the existing canonical certificate path.

Required result:

```json
{
  "verified": true
}
```

## Phase 11: independent review

A reviewer not responsible for the final execution must inspect:

- candidate record;
- Plan 062 ADR/schema changes;
- Java source-lock/receipt;
- i2pd source-lock/observer/behavior-neutrality receipt;
- Plan 065 diagnostic bundle;
- Run A and Run B aggregate manifests;
- certificate verifier result;
- cleanup/network attestations;
- support ledger status.

Review checklist:

1. no support router/SAM/I2CP/HTTP trigger in primary path;
2. Router Hash is 64-hex SHA-256;
3. exact DeliveryStatus ID continuity in all eight directions;
4. Java receiver handler source-proven;
5. i2pd observer after AEAD/`FromNTCP2` and behavior-neutral;
6. sender frame evidence is post-write/completion;
7. receiver evidence is independent;
8. no placeholder/zero digest;
9. no source/provenance mismatch;
10. no candidate mutation;
11. two-run mutable-state independence;
12. cleanup/no-egress exact;
13. no unsupported advertisement.

Record review in:

```text
plans/066-review.md
```

The reviewer must either accept or list concrete blockers. No implicit approval.

## Phase 12: closure decision

Create:

```text
plans/066-closure.md
```

and update:

```text
plans/030-milestone-3-closure.md
README.md
AGENTS.md
docs/architecture/interop-apparatus.md
docs/protocol-support.md
specs/support.toml
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
```

### Milestone 3 may close when

- candidate record valid;
- Run A verifies;
- Run B verifies;
- cross-bundle certificate `verified=true`;
- independent review accepted;
- all local checks pass at candidate;
- exact external run IDs and bundle digests recorded;
- no unresolved blocker;
- support ledger remains truthful.

### Support advertisement remains separate

Milestone 3 transport interoperability closure does not automatically make production daemon support advertised. Keep:

```text
status = experimental
advertised = false
```

unless a separate explicit support-advertisement/readiness plan evaluates daemon wiring, operational policy, public-network exposure, and security review.

## Required Plan 066 tests

Create/update:

```text
tests/integration/ntcp2/harness/test_plan066.py
```

Cover at least:

1. Plan 056 candidate rejected;
2. Plan 060 candidate rejected;
3. candidate below Plan 065 floor rejected;
4. missing Plan 063 receipt rejected;
5. missing Plan 064 receipt rejected;
6. missing Plan 065 diagnostic bundle rejected;
7. placeholder digest rejected;
8. cross-lane bundles rejected;
9. same run ID rejected;
10. same mutable Java state rejected;
11. same mutable i2pd state rejected;
12. same mutable i2pr state rejected;
13. repeated DeliveryStatus ID rejected;
14. one wrong message ID rejected;
15. one wrong Router Hash rejected;
16. v3 trigger rejected;
17. synthetic fallback rejected;
18. missing receiver decrypt rejected;
19. missing receiver decode rejected;
20. sender-only false pass rejected;
21. cleanup failure rejected;
22. parent network drift rejected;
23. artifact reuse across bundles rejected;
24. direction-order independence required;
25. bundle mutation rejected;
26. source commit drift rejected;
27. reference binary drift rejected;
28. observer patch drift rejected;
29. verifier drift rejected;
30. valid two-bundle fixture accepted.

## Non-goals

Plan 066 does not:

- fix implementation defects after freeze;
- weaken four-direction or receiver-proof requirements;
- run public-network tests;
- activate production daemon support;
- qualify IPv6/SSU2;
- perform throughput/load testing;
- publish RouterInfo or join NetDB;
- advertise NTCP2 automatically.

## Stop rules

Stop and retire candidate when:

- any code/config/reference/driver/schema/observer/verifier change is needed;
- any digest is missing/ambiguous/placeholder;
- a direction lacks exact receiver proof;
- any message ID or Router Hash mismatch occurs;
- any process/topology cleanup fails;
- parent network state changes;
- execution lane cannot prove no public egress;
- Run A or Run B requires partial replacement;
- cross-bundle independence fails;
- reviewer cannot verify observer/pass-predicate semantics.

Do not close with partial bundles, one run, a narrative-only record, or a typed blocker represented as success.

## Closure criteria

Plan 066 closes only when:

- fresh candidate descended from Plan 065 is frozen and valid;
- all prerequisites validate;
- Run A contains four passing primary directions;
- Run B contains four passing primary directions;
- all eight directions prove exact authentication, frame emission, frame decryption, and DeliveryStatus decode;
- all exact message IDs and Router Hashes correlate;
- all provenance is exact/nonzero;
- all cleanup/no-egress/network checks pass;
- both exported bundles independently verify;
- cross-bundle certificate reports `verified=true`;
- independent review accepts;
- closure docs record exact SHAs/run IDs/bundle digests;
- Milestone 3 status is updated truthfully;
- NTCP2 remains experimental/non-advertised pending separate readiness decision.

## Small-model handoff instructions

Treat Plan 066 as an operator checklist, not an implementation opportunity.

1. Run prerequisites and stop on first failure.
2. Generate candidate record from measured values only.
3. Freeze once.
4. Execute complete Run A.
5. Verify/export Run A.
6. Recreate fresh mutable environment.
7. Execute complete Run B in reverse order.
8. Verify/export Run B.
9. Run cross-bundle verifier.
10. Obtain independent review.
11. Write closure records last.

Never edit source after freeze, never reuse one direction from a previous run, never abbreviate hashes, never fill a missing digest manually, and never mark a blocker as passed.