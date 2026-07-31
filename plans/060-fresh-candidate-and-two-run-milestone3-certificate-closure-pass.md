# Plan 060: fresh candidate and two-run Milestone 3 certificate closure pass

## Status and dependencies

- Status: retired (Plan 062 evidence-contract and architecture
  correction pass).
- Plan type: final candidate freeze, authoritative external execution, certificate review, and Milestone 3 closure pass.
- Starts only after Plan 058 and Plan 059 are closed.
- Plan 058 must have retired the Plan 056 candidate and superseded Plan 057.
- Plan 059 must have implemented and live-qualified the i2pd direct helper, receiver observations, and canonical runner integration. The Java support topology is forbidden under ADR 0021 (Rejected by Plan 058); Plan 060 must not start under the current four-direction contract until either a future pinned Java revision is adopted or the closure contract is revised through a new ADR.
- **Plan 062 supersession:** Plan 060 is no longer active execution
  authority. Plan 062 (`plans/062-ntcp2-evidence-contract-and-architecture-correction.md`)
  retires the Plan 060 candidate from all future candidate
  validators and the static boundary checker. The Plan 060 candidate
  record is preserved verbatim at `plans/060-candidate.md` for
  audit. Future candidates must descend from the Plan 065
  implementation floor or later and must use the Plan 062 v4
  trigger schema, the Plan 062 reference-event v1 schema, the Plan
  062 v3 observation schema, and the 64-hex SHA-256 Router Hash
  contract.
- This plan is execution-only after candidate freeze. Any required source, helper, catalog, topology, test, or documentation-contract change invalidates the candidate and restarts the plan.
- Milestone 3 may close only when this plan produces a verified certificate over two independent complete passing bundles.
- NTCP2 remains non-advertised unless a separate later support-advertisement decision explicitly changes it.

## Objective

Cut one fresh candidate from the fully implemented and qualified repository, then produce two independent complete external mixed-router evidence bundles from that exact commit.

Each bundle must contain these four IPv4 directions:

1. `i2pr-to-java-ipv4`;
2. `java-to-i2pr-ipv4`;
3. `i2pr-to-i2pd-ipv4`;
4. `i2pd-to-i2pr-ipv4`.

Every accepted direction must prove:

- exact i2pr source and launcher provenance;
- exact pinned reference and helper provenance;
- exact observation catalog and support-topology provenance;
- RouterInfo continuity for both peers;
- both-side NTCP2 authentication;
- sender frame emission;
- receiver authenticated frame decryption;
- receiver correlated I2NP decode/dispatch;
- sealed rootless execution;
- unchanged parent network state;
- clean process and topology teardown;
- immutable bundle finalization and post-export verification.

The certificate verifier must report `verified: true` over both bundles.

## Candidate-freeze principle

The candidate is the last implementation commit before execution records begin.

The candidate must include:

- all Plan 058 corrections and validators;
- all Plan 059 helper/topology/observation implementations;
- all Plan 059 live qualification fixes;
- all Plan 060-specific fixture tests and driver corrections;
- no known missing external prerequisite.

The candidate must not be inherited from Plan 056 or Plan 057.

After freeze, allowed changes are limited to:

- generated untracked execution artifacts;
- a candidate record that points to the already frozen commit, when that record is committed in a successor documentation commit that is explicitly excluded from source archive execution; or preferably
- a candidate tag/immutable external receipt generated without modifying the source tree.

Preferred approach: create the candidate record before final freeze, then freeze the commit containing that record with a placeholder-free self-consistent method. If self-reference makes this impossible, store the candidate declaration as an execution receipt outside the source archive and commit it only after both runs, clearly separating `executed_source_commit` from `closure_document_commit`.

Do not claim a documentation successor commit was the executed source commit.

## Phase 1: verify Plan 058 and Plan 059 closure

Before candidate preparation, verify:

### Plan 058

- Plan 056 candidate is retired;
- Plan 057 is superseded;
- ADR 0021 has an explicit final decision;
- direct-host and guest lanes are alternatives;
- candidate integrity validation passes;
- no false committed-evidence claims remain.

### Plan 059

- i2pd direct helper is committed and qualified;
- Java support topology is committed and qualified;
- receiver observations are source-qualified and runtime-demonstrated;
- Java selected startup cell passes 10/10;
- one complete live qualification bundle exists;
- canonical runner uses live trigger and observation records;
- no synthetic fallback can produce `passed`;
- all remaining known blockers are resolved or explicitly accepted protocol limitations that do not prevent the bounded four-direction pass.

If any prerequisite is incomplete, stop. Do not create a candidate.

## Phase 2: select one execution lane

Choose exactly one lane for both Run A and Run B.

### Lane A: direct-host

Use when the dedicated Ubuntu execution host itself satisfies:

- Ubuntu 24.04 amd64;
- rootless probe `rootless_sandbox_available`;
- sufficient physical RAM and disk;
- reference build/install requirements;
- sealed no-public-egress execution.

No Multipass layer is used.

### Lane B: guest

Use when an outer host provisions one owned Ubuntu 24.04 amd64 guest whose kernel and policy satisfy the rootless contract.

Outer-host requirements:

- enough resources for one evidence guest at a time;
- reliable VM lifecycle and source/evidence transfer;
- no requirement that the outer host itself pass the unprivileged-userns probe.

Guest requirements:

- rootless probe `rootless_sandbox_available`;
- sufficient guest RAM/disk;
- exact environment manifest;
- reference build/install requirements;
- sealed no-public-egress execution.

### Lane lock

Record:

```text
lane_kind
outer_host_baseline
guest_probe_result (typed absence for direct-host)
environment_manifest_sha256
vm_manager_version (typed absence for direct-host)
```

Run A and Run B must use the same lane kind and equivalent environment contract. They may use different fresh guest generations.

## Phase 3: add Plan 060-specific tests before freeze

Add `tests/integration/ntcp2/harness/test_plan060.py` with at least:

1. retired Plan 056 candidate rejected;
2. superseded Plan 057 rejected;
3. candidate commit must be descendant of Plan 059 closure implementation floor;
4. candidate rejects missing helper binary/source digest;
5. candidate rejects unqualified observation catalog;
6. candidate rejects ADR not Accepted;
7. direct-host lane positive fixture;
8. guest lane positive fixture with restrictive outer host and permissive guest;
9. cross-lane Run A/Run B rejection;
10. same mutable Java state across runs rejected;
11. same support-router state across runs rejected;
12. same trigger correlation nonce across runs rejected;
13. one missing live observation rejects certificate;
14. synthetic-fallback observation rejects certificate;
15. helper/topology digest drift rejects certificate;
16. source commit drift rejects certificate;
17. direction-order independence positive fixture;
18. finalized bundle mutation rejection;
19. untracked raw diagnostics rejection;
20. two independent passing fixture bundles accepted.

Extend static checks so Plan 060 cannot freeze while:

- Plan 059 status is not closed;
- ADR 0021 is not Accepted;
- helper implementation paths are absent;
- observation qualification records are absent;
- Plan 056 candidate is still marked active;
- Plan 057 is still marked active.

## Phase 4: full pre-freeze validation

Run all validation from a clean tree:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan060.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan059.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan058.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'

bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

Also run the compact live controls from Plan 059 in the selected execution lane:

- one i2pd helper positive and wrong-target control;
- one Java topology positive and support-router-removal control;
- one Java selected-cell startup/shutdown control;
- one Java receiver valid/handshake-only control;
- one i2pd receiver valid/handshake-only control;
- one bundle verifier fixture.

Any failure blocks freeze.

## Phase 5: create the fresh candidate

Create a new candidate record, recommended:

```text
plans/060-candidate.md
```

It must contain exactly one authoritative full source SHA:

```text
executed_source_commit = <40 lowercase hex>
implementation_floor_commit = <Plan 059 closure implementation commit>
status = declared
```

Required measured fields:

- source commit object digest;
- source tree digest;
- source archive digest;
- clean/dirty state;
- launcher binary digest;
- rustc/cargo versions;
- target triple;
- reference lock digest;
- Java installed-tree digest;
- i2pd installed-tree digest;
- i2pd helper source/binary/input digests;
- Java support-topology implementation/config digest;
- support-router reference and installed-tree digest;
- observation catalog digest;
- observation qualification receipt digests;
- Java template digest;
- Java selected-cell qualification digest;
- environment manifest digest;
- lane kind;
- certificate verifier digest/version;
- validation receipt digest.

### Candidate consistency rule

Every runtime identity must report the same `executed_source_commit` and measured digests. A later closure documentation commit is recorded separately as `closure_record_commit` and never substituted for the executed source.

### Candidate invalidation

Invalidate immediately when:

- source tree changes;
- helper binary/source changes;
- reference tree changes;
- catalog/qualification receipt changes;
- topology configuration changes;
- verifier changes;
- a failed run reveals a needed code/configuration fix.

A candidate cannot be “refreshed” by changing its SHA in place after execution. Retire it and create a new candidate record.

## Phase 6: provision independent Run A environment

### Direct-host lane

Create fresh per-run roots and verify no prior router/helper processes or state remain.

### Guest lane

Create a fresh collision-resistant owned guest generation. Verify:

- lifecycle ownership;
- provisioning complete;
- source archive and cache digests;
- rootless capability;
- reference/helper builds;
- Java template digest;
- no unrelated active evidence guest;
- offline execution transition.

### Shared Run A preflight

- source commit equals candidate;
- tree/archive/launcher digests equal candidate;
- all reference/helper/catalog/topology digests equal candidate;
- parent network baseline captured;
- no residual router/helper processes;
- diagnostics default `off` or `sanitized`;
- no raw export path.

Recommended run ID:

```text
plan060-a-YYYYMMDDhhmmss-<8hex>
```

## Phase 7: authoritative Run A

Use direction order:

```text
i2pr-to-java-ipv4
java-to-i2pr-ipv4
i2pr-to-i2pd-ipv4
i2pd-to-i2pr-ipv4
```

For each direction:

1. verify frozen run identity;
2. create fresh i2pr identity/static-key state;
3. create fresh reference state;
4. clone Java template when applicable;
5. create fresh support-router state when applicable;
6. capture parent network pre-state;
7. enter sealed rootless topology;
8. start the receiver and verify readiness;
9. invoke the correct one-shot initiator/trigger path;
10. collect sender observation-v2;
11. collect receiver observation-v2;
12. collect trigger record;
13. collect terminal status and bounded counters;
14. stop every process;
15. destroy topology;
16. prove no residual processes, locks, namespaces, sockets, or helper state;
17. capture parent network post-state;
18. write attestation/direction/trigger/observation/cleanup artifacts;
19. cross-check all records against run identity.

### Per-direction pass predicate

A direction passes only when:

```text
actual_typed_result = passed
cleanup_result = clean
sender.ntcp2_authenticated = observed
receiver.ntcp2_authenticated = observed
sender.frame_emitted = observed
receiver.frame_authenticated_and_decrypted = observed
receiver.i2np_message_decoded = observed
parent_network_state_unchanged = true
sandbox_attestation = valid
trigger = authenticated or not-required-i2pr-initiator
```

The receiver decode must correlate to the bounded test message. An unrelated I2NP message cannot satisfy the direction.

### Run A stop rule

On any direction failure:

- finish cleanup;
- finalize a diagnostic failed bundle when structurally possible;
- do not replace only the failed direction;
- do not continue toward certificate closure;
- determine whether failure is environmental, helper/topology, observation, or protocol;
- if any code/config change is needed, retire the candidate and return to Phase 4 after the fix.

## Phase 8: finalize and export Run A

- require exactly four direction IDs in every artifact class;
- verify all semantic schemas;
- verify run identity binding;
- verify manifest and checksum;
- atomically export;
- verify exported bundle;
- write acknowledgement beside the bundle;
- retain sanitized bundle in durable storage;
- record bundle manifest digest.

Do not modify the finalized bundle.

## Phase 9: reset to an independent Run B environment

Run B must not share mutable state with Run A.

Required fresh state:

- run ID;
- bundle staging/export roots;
- i2pr identities/static keys;
- Java cloned data directories;
- i2pd data directories;
- support-router identities/state;
- trigger process and correlation nonce;
- log cursors;
- topology process tree;
- guest generation or verified clean restoration generation.

Allowed immutable reuse:

- source archive;
- verified reference cache;
- frozen Java template;
- helper binaries;
- catalog and qualification receipts.

Verify immutable digests again before Run B.

## Phase 10: authoritative Run B

Use reversed order:

```text
i2pd-to-i2pr-ipv4
i2pr-to-i2pd-ipv4
java-to-i2pr-ipv4
i2pr-to-java-ipv4
```

Repeat all Run A rules with a new run ID, recommended:

```text
plan060-b-YYYYMMDDhhmmss-<8hex>
```

Run B must independently satisfy all four directions. It may not copy any per-run record from Run A.

## Phase 11: certificate verification

Run the canonical verifier:

```bash
python3 tests/integration/ntcp2/harness/verify_milestone3_certificate.py \
  --run-a <run-a-export> \
  --run-b <run-b-export> \
  --output <certificate.json>
```

The verifier must check:

- both bundles individually verify;
- run IDs differ;
- exact source commit/digests match;
- reference/helper/catalog/topology immutable digests match;
- lane kind matches;
- mutable state identifiers differ;
- exact four-direction catalogs exist;
- all eight directions pass;
- all eight cleanups are clean;
- all sender/receiver predicates pass;
- parent network unchanged in all directions;
- no raw diagnostics;
- no undeclared files;
- no copied observations, trigger nonces, RouterInfo digests, or support-state identities where independence requires divergence.

Required output:

```text
schema = i2pr-milestone3-certificate-v1
verified = true
failure_count = 0
```

Any denial prevents closure.

## Phase 12: independent review

A reviewer other than the execution automation should inspect:

- candidate record;
- validation receipt;
- Run A and Run B manifests;
- eight direction outcomes;
- eight trigger records;
- sender and receiver observation sources;
- Java topology support inventory;
- i2pd helper provenance;
- cleanup summaries;
- environment attestations;
- certificate verifier output.

Create a sanitized reviewer record containing:

```text
schema
reviewer_role
review_date_utc
executed_source_commit
run_a_manifest_sha256
run_b_manifest_sha256
certificate_sha256
verifier_sha256
outcome = accepted
```

Do not include private paths, RouterInfo contents, keys, or raw logs.

## Phase 13: commit durable sanitized evidence

Do not claim evidence is committed unless it is tracked.

Choose one repository-approved policy:

### Policy A: commit complete compact sanitized bundles

Track:

```text
tests/integration/ntcp2/evidence/milestone-3/<run-a>/
tests/integration/ntcp2/evidence/milestone-3/<run-b>/
tests/integration/ntcp2/evidence/milestone-3/certificate.json
tests/integration/ntcp2/evidence/milestone-3/reviewer-record.json
```

### Policy B: commit bounded manifests and receipts only

Track:

- run identities;
- manifests and checksums;
- direction/trigger/observation/cleanup/attestation records;
- certificate;
- reviewer record;
- external durable artifact locator and digest.

The locator must be durable and access-controlled according to repository policy. Do not use an ephemeral local `target/` path as the only evidence location.

All tracked files must pass sanitizer and manifest checks.

## Phase 14: closure documentation

Create `plans/060-closure.md` with:

- exact executed source commit;
- separate closure-record commit;
- Plan 059 implementation floor;
- selected lane;
- environment contract;
- Run A and Run B IDs;
- manifest digests;
- certificate digest and verifier digest;
- eight-direction outcome table;
- reference revisions and installed-tree digests;
- helper/topology/catalog/qualification digests;
- Java selected startup model;
- reviewer record digest;
- validation commands/results;
- exact bounded evidence scope;
- remaining limitations.

Update:

- `plans/030-milestone-3-closure.md`;
- `plans/056-closure.md` with a historical pointer only;
- `plans/057-cross-host-milestone-3-external-evidence-run.md` with supersession pointer;
- `specs/support.toml` evidence references;
- `docs/protocol-support.md`;
- README/project status;
- relevant skills.

### Support wording

Acceptable:

> Two independent sanitized external runs demonstrated the bounded IPv4 NTCP2 handshake and DeliveryStatus smoke path with pinned Java I2P 2.12.0 and i2pd 2.60.0 in both initiator and responder roles under the sealed rootless test topology.

Do not claim:

- production readiness;
- Internet-scale interoperability;
- broad I2NP support;
- SSU2 support;
- anonymity/security readiness;
- performance qualification;
- daemon-wide NTCP2 readiness beyond the tested composition seam.

Keep `advertised = false` unless separately authorized.

## Explicit closure criteria

Plan 060 closes only when every item is true:

- [ ] Plans 058 and 059 are closed.
- [ ] Plan 056 candidate is retired and unused.
- [ ] Plan 057 is superseded and unused.
- [ ] One execution lane is selected and locked for both runs.
- [ ] Plan 060 tests and all full validation gates pass before freeze.
- [ ] A new candidate is cut after all implementation work.
- [ ] Candidate record contains exactly one executed source SHA.
- [ ] Candidate is a descendant of the Plan 059 implementation floor.
- [ ] Source, launcher, reference, helper, topology, catalog, qualification, template, verifier, and environment digests are complete and nonzero.
- [ ] Run A uses fresh mutable state and produces a complete verified bundle.
- [ ] All four Run A directions pass the full sender/receiver predicate.
- [ ] Run B uses independently fresh mutable state and produces a complete verified bundle.
- [ ] All four Run B directions pass the full sender/receiver predicate.
- [ ] Run orders differ as specified or an equivalent recorded order-independence scheme is used.
- [ ] All eight cleanup records are clean.
- [ ] Parent network state is unchanged in all eight directions.
- [ ] No direction uses synthetic fallback.
- [ ] No support-router traffic satisfies the Java primary direction.
- [ ] Certificate verifier reports `verified: true` with zero failures.
- [ ] Independent reviewer record reports accepted.
- [ ] Sanitized evidence is actually committed or stored under a durable tracked locator according to the selected policy.
- [ ] Closure documentation distinguishes executed source commit from closure-record commit.
- [ ] `plans/030-milestone-3-closure.md` is updated accurately.
- [ ] `specs/support.toml` remains bounded and `advertised = false` absent a separate decision.
- [ ] Full final validation passes at the closure documentation commit.

## Failure and invalidation semantics

### Code/configuration defect

- retire candidate;
- implement fix in a new commit;
- repeat Plan 059 qualification relevant to the changed area;
- return to Plan 060 Phase 4;
- produce two new runs.

### Run A failure

- retain as diagnostic evidence only;
- do not patch bundle;
- do not start certificate review;
- retire candidate when a fix is needed.

### Run A pass, Run B failure

- Run A remains diagnostic evidence;
- both authoritative runs must be repeated from the next candidate;
- do not pair old Run A with a new Run B.

### Environment failure

Use typed blockers:

```text
blocked_execution_lane_unavailable
blocked_guest_unreachable
blocked_rootless_sandbox_regression
blocked_environment_resource_contract
blocked_reference_cache_drift
blocked_parent_network_state_changed
```

A pure environment failure may permit retry from the same candidate only when no source/configuration/reference/helper/catalog digest changed and the failed run never produced an accepted certificate component. Record the retry decision explicitly.

### Evidence failure

- finalized bundle mutation or verification failure invalidates the whole run;
- never repair finalized bundle contents;
- never copy one direction from another run;
- never modify a record to change `blocked`/`rejected` into `passed`.

## Smaller-model execution guidance

Use this strict sequence:

1. verify Plan 058/059 status files;
2. add and run Plan 060 tests;
3. run full local validation;
4. choose lane;
5. perform compact external controls;
6. freeze one candidate;
7. stop all source edits;
8. run A;
9. verify/export A;
10. reset all mutable state;
11. run B in reverse order;
12. verify/export B;
13. run certificate verifier;
14. obtain reviewer acceptance;
15. commit sanitized evidence;
16. write closure docs.

At every step, compare measured digests to the candidate. On mismatch, stop immediately.

Suggested commits before freeze:

1. `tests: add Plan 060 candidate and certificate regressions`
2. `interop: close final pre-freeze execution defects` only if tests expose defects
3. `docs: prepare Plan 060 candidate contract`

After freeze, do not commit implementation changes. Final commits should contain evidence and documentation only:

4. `evidence: add Plan 060 two-run Milestone 3 certificate`
5. `docs: close bounded Milestone 3 NTCP2 evidence`

## Required handoff artifacts

- `plans/060-candidate.md` or equivalent immutable candidate receipt;
- `tests/integration/ntcp2/harness/test_plan060.py`;
- Run A sanitized bundle;
- Run B sanitized bundle;
- certificate with `verified: true`;
- reviewer record;
- validation receipt;
- `plans/060-closure.md`;
- updated Milestone 3 and support documentation.
