# Plan 056: NTCP2 Milestone 3 two-run external evidence closure pass

## Status and dependencies

- Plan type: final authorized external execution and evidence review pass.
- Starting branch: `main` after Plans 053, 054, and 055 are complete.
- Hard dependencies:
  - Plan 053: canonical Plan 052 evidence path fully integrated and bundle verifier corrected.
  - Plan 054: stable Java per-direction state model and source-locked receiver observations qualified.
  - Plan 055: both reference-initiated directions have qualified triggers/topology and controls.
- This plan owns no broad feature development. It owns final preflight, two independent complete evidence runs, review, closure documentation, and support-inventory reconciliation.
- Milestone 3 may close only through this plan.

## Objective

Produce two independently executed, complete, sanitized, durable Plan 052 evidence bundles from the same exact clean i2pr source commit. Each bundle must contain four accepted IPv4 mixed-router directions:

1. `i2pr-to-java-ipv4`;
2. `java-to-i2pr-ipv4`;
3. `i2pr-to-i2pd-ipv4`;
4. `i2pd-to-i2pr-ipv4`.

Every direction must prove:

- exact source and binary provenance;
- exact reference provenance;
- RouterInfo continuity;
- both-side NTCP2 authentication;
- sender frame emission;
- receiver authenticated frame decryption;
- receiver correlated I2NP message decode/dispatch;
- rootless isolation attestation;
- unchanged parent network state;
- clean process and topology teardown;
- immutable bundle verification before and after export.

The two bundles must be independently reproducible and must not share mutable run state.

## Closure philosophy

This is an evidence-production pass, not a debugging pass.

If code, schemas, helper logic, source-lock records, reference configuration, or pass predicates must change after the first authoritative run starts, that run is invalidated. Make the correction in a new commit and restart Plan 056 from preflight with two new run IDs.

Do not patch a failed bundle. Do not replace one failed direction inside an existing bundle. Do not combine directions from different commits, guests, run IDs, or environment identities.

## Fixed run contract

### Source contract

Both runs use one exact 40-character i2pr commit SHA. The commit must:

- exist on `main`;
- have a clean source tree;
- include all Plans 053-055 implementation and status records;
- pass all local validation gates;
- be archived and transferred using the canonical source-transfer path;
- produce the same source-tree digest and launcher digest in both runs.

If any source-derived digest differs between runs, stop and investigate. Do not accept narrative explanations for drift.

### Reference contract

Both runs use:

- Java I2P 2.12.0 at `2800040deee9bb376567b671ef2e9c34cf3e30b6`;
- i2pd 2.60.0 at `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`;
- identical verified artifact and installed-tree digests;
- identical helper source/binary digests;
- identical machine-readable observation and trigger catalogs.

### Environment contract

Use the qualified owned Multipass/rootless lane unless a previously documented dedicated Ubuntu fallback has been selected and attested. The environment must satisfy:

- Ubuntu 24.04 amd64;
- rootless user/network/mount/PID namespace support;
- no public network during authoritative direction execution;
- exact environment manifest digest;
- one active evidence guest at a time on the constrained host;
- no unrelated router processes;
- sufficient memory and disk preflight;
- parent network state measurement before and after each direction;
- no host-global sysctl or AppArmor policy changes.

## Independent-run requirement

Run A and Run B must be independent in all mutable state:

- distinct run IDs;
- distinct bundle staging roots;
- distinct per-direction run roots;
- distinct Java cloned data directories;
- distinct i2pd data directories;
- distinct i2pr identity/static-key state directories;
- fresh log cursors;
- fresh trigger processes;
- fresh support-router identities/state when a support topology is used;
- no direction record copied or relabeled between runs.

Recommended environment independence:

- Run A uses a fresh owned guest generation.
- After export and cleanup, destroy or restore to an approved pre-execution snapshot.
- Run B uses a new owned guest generation or a cryptographically verified clean snapshot restoration with a new lifecycle generation.

Do not run both bundles in one long-lived dirty guest without a verified reset contract.

## Direction order

Use different direction orders to expose hidden ordering dependence.

Recommended:

```text
Run A:
  i2pr-to-java-ipv4
  java-to-i2pr-ipv4
  i2pr-to-i2pd-ipv4
  i2pd-to-i2pr-ipv4

Run B:
  i2pd-to-i2pr-ipv4
  i2pr-to-i2pd-ipv4
  java-to-i2pr-ipv4
  i2pr-to-java-ipv4
```

The order is recorded in the run identity and bundle diagnostics summary. All directions still use fresh per-direction state.

## Phase 1: freeze the candidate commit

### 1.1 Candidate declaration

Create `plans/056-candidate.md` containing:

- candidate commit SHA;
- source tree digest;
- expected launcher binary digest after canonical build;
- pinned reference digests;
- helper binary/source digests;
- observation catalog digest;
- trigger catalog digest;
- environment manifest digest;
- exact validation commands;
- operator and date.

This file is descriptive and must not override measured runtime values.

### 1.2 Full local validation

Run and retain sanitized receipts for:

```bash
cargo fmt --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'

bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

Also run the Plan 053-055 narrow qualification commands documented in their status files.

A failure blocks candidate freeze.

### 1.3 Verify no temporary/debug residue

Reject the candidate if the tracked tree contains:

- raw logs;
- guest absolute-path helper scripts;
- temporary reference source patches;
- generated private identities or keys;
- exported evidence with unresolved placeholder digests;
- stale `known_deviation` success paths;
- undocumented environment overrides.

## Phase 2: environment preflight

### 2.1 Host preflight

Record:

- host OS/kernel;
- physical memory and available-memory bucket;
- disk-free bucket;
- Multipass version;
- active instance inventory;
- owned instance inventory;
- no colliding active evidence guest;
- host negative baseline where applicable.

Stop unrelated owned guests when safely permitted. Do not mutate unowned instances.

### 2.2 Guest creation and ownership

Create one fresh collision-resistant owned guest. Verify:

- host lifecycle reservation;
- guest ownership record;
- cloud-init/provisioning completion;
- exact environment manifest digest;
- rootless probe returns `rootless_sandbox_available`;
- source and cache transfer verification;
- launcher/reference/helper builds and digests;
- offline transition contract;
- no unexpected route or process.

### 2.3 Pre-certificate controls

Before Run A, execute a compact control gate from Plans 054 and 055:

- one Java cloned-state startup/shutdown control;
- one Java receiver no-data negative control;
- one i2pd receiver no-data negative control;
- one Java trigger wrong-RouterInfo control;
- one i2pd trigger wrong-RouterInfo control;
- bundle verifier self-test using a fixture;
- responder reason propagation fixture.

These controls do not enter the certificate bundles. Any failure stops Plan 056.

## Phase 3: authoritative Run A

### 3.1 Freeze run identity

Generate Run A identity from measured values. Recommended ID:

```text
plan056-a-YYYYMMDDhhmmss-<8hex>
```

Verify the identity digest before every direction.

### 3.2 Execute four directions

For each direction:

1. create fresh state roots;
2. verify reference template/cache/helper digests;
3. capture parent network pre-state;
4. enter rootless sealed topology;
5. start receiver and verify readiness;
6. execute the selected trigger/initiator path exactly once;
7. collect sender and receiver observation-v2 records;
8. collect terminal status and bounded counters;
9. stop all processes;
10. destroy topology;
11. verify no residual processes/state;
12. capture parent network post-state;
13. write all five per-direction artifact classes;
14. cross-check run identity and digests.

Do not continue to the next direction after an invariant or cleanup failure. Finalize the current run as diagnostic/failed and restart later from a new candidate execution.

### 3.3 Run A pass conditions

Each direction must have:

```text
actual_typed_result = passed
cleanup_result = clean
both_authenticated = true
sender_frame_emitted = true
receiver_frame_authenticated_and_decrypted = true
receiver_i2np_message_decoded = true
parent_network_state_unchanged = true
```

All digests must be nonzero and cross-checked.

### 3.4 Finalize and export Run A

- complete all direction classes;
- finalize manifest/checksum;
- verify staging;
- atomically export;
- verify exported bundle;
- write acknowledgement beside bundle;
- copy only the sanitized bundle to the durable evidence location;
- record final manifest digest in a host-side candidate ledger.

Do not commit the bundle yet. Run B must first complete.

## Phase 4: reset between runs

### 4.1 Clean Run A environment

Verify:

- no i2pr, Java, i2pd, helper, or support process remains;
- no rootless namespace child remains;
- no stale lock files;
- no old log cursor is reused;
- no per-direction data directory will be used by Run B;
- Java frozen template digest is unchanged;
- reference cache digests unchanged;
- source tree and launcher digests unchanged.

### 4.2 Recreate or restore environment

Use a new owned guest generation or approved clean snapshot restoration. Re-run ownership, provisioning, rootless, source, cache, and offline-transition verification.

If any source-derived or reference-derived digest changes, stop. Run B cannot proceed under a different contract.

## Phase 5: authoritative Run B

Repeat Phase 3 using:

- a new run ID;
- reverse/rotated direction order;
- fresh all mutable state;
- same exact source/reference/helper/catalog/environment contract.

Run B must independently satisfy every direction predicate. It must not import any observation, trigger, cleanup, or RouterInfo record from Run A.

## Phase 6: independent bundle review

### 6.1 Add or use a certificate verifier

Provide one command that accepts both exported bundle paths and validates:

- both bundles verify individually;
- run IDs differ;
- source commit matches;
- source tree digest matches;
- launcher binary digest matches;
- reference artifact/tree digests match;
- helper source/binary digests match;
- catalog digests match;
- environment contract matches or differs only in allowlisted lifecycle-generation fields;
- exactly four direction IDs exist in each class;
- all eight direction records passed;
- all eight cleanup records are clean;
- all eight receiver observations satisfy the v2 predicate;
- all eight sender observations show frame emission;
- all attestations are valid;
- parent network state unchanged for every direction;
- RouterInfo/private state is not leaked;
- no raw diagnostics exist;
- no file is undeclared by the manifest.

Example command shape:

```bash
python3 tests/integration/ntcp2/harness/verify_milestone3_certificate.py \
  --run-a target/interop/evidence/milestone-3/<run-a> \
  --run-b target/interop/evidence/milestone-3/<run-b>
```

### 6.2 Cross-run variability checks

Expected to match:

- source and binary provenance;
- reference provenance;
- helper/catalog provenance;
- topology and privilege model;
- scenario definitions;
- data-phase mode.

Expected or allowed to differ:

- run ID;
- lifecycle generation;
- timestamps;
- per-run identities and RouterInfo digests;
- correlation nonce/message ID;
- bounded runtime timing values.

A verifier must reject identical copied per-run RouterInfo/observation files when independent generation is required.

### 6.3 Manual review checklist

A reviewer must inspect:

- manifest and checksum verification output;
- run identities;
- direction table;
- receiver observation source codes;
- trigger helper kinds and target hashes;
- cleanup summaries;
- environment attestation summaries;
- no-support-claim boundaries.

The reviewer signs a sanitized review record containing only name/role, date, bundle manifest digests, verifier version, and outcome.

## Phase 7: repository evidence and closure record

### 7.1 What may be committed

Commit only sanitized, bounded artifacts. Depending on repository policy, either:

- commit both complete compact bundles under an approved evidence directory; or
- commit their immutable manifest/checksum, run identity, direction, observation, trigger, cleanup, and attestation records while storing larger sanitized diagnostics externally under a durable referenced artifact.

Do not commit raw logs, private keys, RouterInfo contents, absolute guest paths, or mutable caches.

### 7.2 Closure document

Create `plans/056-closure.md` containing:

- exact candidate commit;
- Run A and Run B IDs;
- bundle manifest digests;
- verifier commit/version;
- eight-direction outcome table;
- exact reference revisions/digests;
- exact helper/catalog digests;
- environment contract summary;
- validation commands and results;
- reviewer record digest;
- remaining limitations;
- statement that the evidence covers bounded IPv4 NTCP2 handshake plus DeliveryStatus smoke scope only.

### 7.3 Reconcile status documentation

Update the relevant status documents to say exactly what is proven.

Acceptable wording:

> Two independent sanitized mixed-router evidence runs demonstrated the bounded IPv4 NTCP2 handshake and DeliveryStatus smoke direction with pinned Java I2P 2.12.0 and i2pd 2.60.0 in both initiator/responder roles under the rootless sealed test topology.

Do not claim:

- full production NTCP2 maturity;
- Internet-scale interoperability;
- SSU2 support;
- daemon integration beyond what is actually wired;
- anonymity/security readiness;
- broad I2NP coverage;
- performance qualification.

### 7.4 Support inventory

Milestone 3 closure permits updating the NTCP2 evidence status from unverified to externally demonstrated for the bounded scope. `advertised = false` should remain unless a separate support-advertisement/release-readiness decision explicitly changes it.

Do not conflate evidence closure with production advertisement.

## Failure and invalidation rules

### A direction fails

- finish cleanup;
- write a complete typed failed/diagnostic bundle;
- do not replace only that direction;
- inspect failure;
- if code/config/catalog changes are needed, create a new candidate commit;
- restart with two new authoritative runs.

### Run A passes, Run B fails

Run A remains useful diagnostic evidence but does not close Milestone 3. After correction, produce new Run A and Run B under the corrected exact commit. Do not pair the old Run A with a new Run B from a different commit.

### Environment becomes unstable

Stop and classify:

```text
blocked_environment_resource_contract
blocked_guest_unreachable
blocked_rootless_sandbox_regression
blocked_reference_cache_drift
blocked_parent_network_state_changed
```

Do not weaken checks or increase timeouts without evidence.

### Bundle verification fails

Treat the entire run as invalid. Never repair finalized bundle contents in place.

## Tests required before external execution

- certificate verifier positive fixture with two independent bundles;
- same run ID rejected;
- source commit mismatch rejected;
- launcher digest mismatch rejected;
- reference digest mismatch rejected;
- copied observation file across runs rejected where independence fields must differ;
- missing direction rejected;
- one rejected direction prevents certificate;
- receiver handshake-only observation prevents certificate;
- cleanup failure prevents certificate;
- parent-network change prevents certificate;
- raw diagnostics or undeclared file prevents certificate;
- support topology mismatch rejected;
- malformed reviewer record rejected.

## Suggested execution commits

The implementation should already be complete before authoritative runs. Plan 056 should need few commits:

1. `interop: add Milestone 3 two-bundle certificate verifier`
2. `tests: close Plan 056 certificate verification matrix`
3. `docs: freeze Plan 056 candidate`
4. External execution with no source edits.
5. `evidence: add sanitized Milestone 3 run A and run B records`
6. `docs: close NTCP2 Milestone 3 bounded evidence scope`

If substantial code commits are needed after candidate freeze, abandon the candidate and restart.

## Smaller-model execution guidance

### Treat the candidate commit as immutable

Once `plans/056-candidate.md` names the commit, do not edit source during an authoritative run. A smaller model must not “quick fix” a script in the guest and continue.

Incorrect:

```text
Run A direction 3 fails
edit mixed_runner.py inside guest
rerun direction 3
keep directions 1 and 2
```

Correct:

```text
finalize failed Run A as diagnostic
commit correction on main
freeze new candidate
produce new Run A and Run B from scratch
```

### Verify rather than narrate

Do not write “digests match” unless the verifier checked them. Retain the sanitized verifier receipt.

### One guest at a time

On the constrained host, stop or remove owned stale guests before the authoritative run. Never stop or delete unowned guests.

### Never merge evidence manually

The bundle builder owns file collection. Do not copy a passing direction JSON from an earlier run into the current staging directory.

### Keep claims bounded

The final closure language must match the fixed DeliveryStatus smoke scope. Avoid “NTCP2 fully supported.”

### Stop conditions

Stop immediately when:

- the source tree is dirty;
- a measured digest differs from the candidate;
- rootless probe changes outcome;
- reference cache or helper digests drift;
- one direction needs an unplanned retry inside the same bundle;
- cleanup is not clean;
- parent network state changes;
- bundle verification fails;
- raw logs appear under the export root;
- a code edit appears necessary.

## Explicit acceptance criteria

Plan 056 and Milestone 3 are complete only when all criteria below are met:

1. One exact clean source commit is frozen and used for both runs.
2. All local, static, Rust, Python, Plan 053, Plan 054, and Plan 055 gates pass at that commit.
3. Run A uses a fresh immutable run identity and fresh mutable state.
4. Run B uses a distinct immutable run identity and fresh mutable state.
5. Both runs use identical source, launcher, reference, helper, catalog, and scenario provenance.
6. Both exported bundles verify individually before and after export.
7. The cross-run certificate verifier passes.
8. Each bundle contains exactly four primary direction records in every required artifact class.
9. All eight direction records have `actual_typed_result = passed`.
10. All eight cleanup records are clean.
11. All eight sender observations prove correlated frame emission.
12. All eight receiver observations prove authenticated frame decryption and correlated I2NP decode/dispatch.
13. Both sides authenticate in all eight directions.
14. Trigger records prove the exact intended target and helper/topology path.
15. Every rootless attestation is valid.
16. Parent network state remains unchanged for every direction.
17. No raw logs, private key material, RouterInfo contents, or undeclared files exist in the bundles.
18. Run A and Run B are independent and not assembled from copied direction artifacts.
19. A sanitized reviewer record approves both bundle manifest digests.
20. `plans/056-closure.md` accurately records the bounded scope and limitations.
21. Milestone 3 status is updated consistently across repository documentation.
22. NTCP2 remains non-advertised unless a separate explicit decision changes advertisement status.

## Required final handoff artifacts

- `plans/056-candidate.md`.
- Certificate verifier and tests.
- Run A sanitized evidence bundle.
- Run B sanitized evidence bundle.
- Cross-run verification receipt.
- Reviewer record.
- `plans/056-closure.md`.
- Reconciled support/status documentation.
