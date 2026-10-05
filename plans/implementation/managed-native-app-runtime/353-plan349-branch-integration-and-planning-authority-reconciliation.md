# Plan 353 — Plan-349 branch integration and planning-authority reconciliation

Status: **registered-plan349-branch-integration-reconciliation**.

Classification: **corrective invariant + polish/integration**. This plan makes the `codex/plan-349-managed-app-v1-corrective` work line mergeable without rewriting executed Plan-345/349 or portable-service-tunnel evidence.

Hard dependencies:
- managed-runtime Plan 349 is closed;
- portable-service-tunnel Plans 349–351 are closed on this branch;
- Plan 352 may execute before or during this pass, but final branch integration/verification must include its result if Plan 352 has landed.

This plan is not a managed-app capability milestone. It owns branch integration hygiene, planning-authority consistency, generated-artifact cleanup, ADR identifier reconciliation, and final rebase verification.

## Objective

Correct the integration defects introduced when two independently developed work lines were combined on one branch.

Observed branch baseline before this plan:
- branch head: `b9a03e8c3dc1e04933c993044d59b0e955b6370c`;
- branch is 26 commits ahead and 3 commits behind `main`;
- branch diff contains roughly 300 changed files;
- 231 tracked files are under `tests/portable-service-tunnel-consumer/target/`;
- managed-native-app-runtime and portable-service-tunnels both own global Plan 349;
- both Plan-349 authorities are already closed and cross-referenced;
- `docs/adr/` contains both:
  - `0032-managed-native-app-process-and-capability-boundary.md`;
  - `0032-portable-service-tunnel-policy-core-and-adapters.md`;
- `plans/README.md` still documents only the older 296/297 collision even though the collision ledger/checker now also encode the qualified 349 collision.

The branch must not merge while generated build outputs are tracked or while planning/ADR authority is internally contradictory.

## Core decisions

### 1. Preserve the executed Plan-349 collision as a qualified historical collision

Do **not** renumber either closed Plan 349.

The repository planning skill states that global plan numbers are load-bearing and closed plan/status identifiers are not renumbered because status tokens, supersession chains, evidence scripts, and doc/spec references depend on them.

The two Plan-349 work lines were independently executed before their branch histories were combined. Their collision is therefore treated as a discovered integration-time historical collision, using the repository's existing qualified-collision mechanism.

The correction is to make every planning authority agree:
- `plans/global-number-collision-ledger.md`;
- `scripts/check-global-plan-number-uniqueness.py`;
- planning tests;
- `plans/README.md`.

No third Plan-349 owner is permitted.

Future plans, including Plans 352 and 353, must remain globally unique.

### 2. Preserve the earlier managed-app ADR 0032; renumber the later portable-service-tunnel ADR

ADR identifiers are not yet guarded globally, but two accepted ADR 0032s are ambiguous.

For this collision:
- preserve `0032-managed-native-app-process-and-capability-boundary.md`;
- rename the later portable-service-tunnel ADR to the next available unique ADR number, expected `0033-portable-service-tunnel-policy-core-and-adapters.md`;
- update every reference to the portable-service-tunnel ADR;
- do not rewrite the accepted decision text except for self-identification/link maintenance required by the rename.

This plan deliberately does **not** repair the pre-existing duplicate ADR 0030 records. The planning skill already records that known gap. Expanding into all historical ADR numbering is out of scope.

### 3. Generated Cargo target trees are never repository evidence

Remove all tracked content under:

`tests/portable-service-tunnel-consumer/target/`

The source fixture, lockfile, conformance docs, and test crate remain.

Add an ignore rule that prevents nested test-fixture Cargo target directories from being recommitted. Prefer a narrow rule such as `/tests/**/target/` or an equivalently scoped repository rule rather than hiding arbitrary directories named `target` outside build contexts.

No closure/evidence record may depend on compiled artifacts in the tracked tree.

### 4. Integrate current main before final closure

After content corrections, rebase the work line onto current `main` or perform the repository's accepted equivalent integration operation.

Final closure evidence must be from the integrated head, not the pre-rebase branch.

## Why this corrective is ready

Every defect is directly observable from repository state and has a bounded correction:
- tracked build artifacts are generated files, not source/evidence;
- the 349 collision already has an explicit qualified-collision mechanism;
- the planning README omission is an authority drift, not an unresolved design question;
- ADR 0032 ordering is deterministic from commit history;
- the branch has a finite divergence from `main`.

No product behavior needs to change.

## Invariants

1. Do not modify the semantic conclusions or executed evidence of:
   - managed-runtime Plans 345/349;
   - portable-service-tunnel Plans 349–351.
2. Do not renumber closed global plans.
3. Do not add a wildcard collision exception; only exact qualified historical paths may be permitted.
4. No third owner of Plan 349 may pass the uniqueness checker.
5. Generated Cargo output must not remain tracked.
6. The portable-service-tunnel external consumer fixture must still build/test from source after its `target/` tree is deleted.
7. The managed-app contract/runtime boundaries remain unchanged by this integration plan.
8. Rebase conflict resolution must not silently change capability/support claims.
9. No user-visible router capability or advertisement changes.

## Scope

### In scope

- delete tracked nested Cargo build outputs;
- extend `.gitignore` narrowly to prevent recurrence;
- reconcile `plans/README.md` with the exact qualified Plan-349 collision already recorded in the collision ledger/checker;
- verify the Plan-349 uniqueness checker remains exact and rejects a third owner;
- rename the portable-service-tunnel ADR 0032 to unique ADR 0033 and repair all references;
- reconcile registry/roadmaps only where branch integration made statements stale;
- rebase/integrate current `main`;
- rerun relevant focused tests and the full routine floor;
- closure record.

### Out of scope

- renumbering any closed global plan;
- repairing the older duplicate ADR 0030 pair;
- changing managed-app wire/policy semantics except via Plan 352;
- changing portable-service-tunnel API semantics;
- changing router support inventory/capability claims;
- new runtime/network functionality;
- deleting source fixtures merely because their generated build output was tracked.

## Ordered work packages

### A. Remove generated artifacts

1. Delete every tracked path below `tests/portable-service-tunnel-consumer/target/`.
2. Confirm no other newly tracked nested Cargo target tree exists in this branch.
3. Add a narrow ignore rule preventing recurrence.
4. Prove the external consumer fixture rebuilds from a clean state.

Required evidence:

```text
git ls-files '*/target/*'
```

must return no tracked build output covered by this plan.

### B. Reconcile Plan-349 authority

1. Keep both closed Plan-349 implementation/closure files under their subsystem-qualified paths.
2. Keep the exact pair in `plans/global-number-collision-ledger.md`.
3. Keep/check the exact-path exception in `scripts/check-global-plan-number-uniqueness.py`.
4. Update `plans/README.md` so its naming rule explicitly names the qualified managed-runtime/349 + portable-service-tunnels/349 collision alongside the older 296/297 historical collisions.
5. Ensure wording remains explicit that **new** collisions are forbidden.
6. Planning tests must prove:
   - exact known collision passes;
   - third Plan-349 owner fails;
   - unrelated duplicate number fails.

Do not spread current plan-state tables into skills/README; this is a durable numbering-rule exception, not a live status ledger.

### C. Reconcile ADR 0032

1. Establish commit-order evidence showing managed-app ADR 0032 predates the portable-service-tunnel ADR.
2. `git mv` the later portable ADR to:
   `docs/adr/0033-portable-service-tunnel-policy-core-and-adapters.md`
   if 0033 remains free at execution time.
3. Update its internal title/identifier only as needed to identify it as ADR 0033.
4. Update all references in architecture docs, roadmap/plans, README/specs, and source comments if any.
5. Search for the old filename and portable-service-tunnel "ADR 0032" wording; no live reference may remain.
6. Leave the older duplicate ADR 0030 records untouched and explicitly record that limitation.

If ADR 0033 is no longer free at execution time, stop before renaming and choose the next unique ADR number in the same commit, documenting the reason in closure.

### D. Rebase/integrate main

1. Fetch current `main`.
2. Rebase the branch onto current `main` or use the repository-approved equivalent.
3. Resolve conflicts by authority order:
   closure/status > tests/scripts > ADR > prose.
4. Re-run the Plan-352 focused lane if Plan 352 is present.
5. Re-run portable-service-tunnel focused consumer/API guards.
6. Run the routine floor on the integrated head.

### E. Final planning reconciliation

At closure:
- branch must be zero commits behind the chosen current-main baseline used for verification;
- registry/roadmaps must not claim Plan 349 active;
- if Plan 352 is still ready/active, managed-app successors remain blocked on it;
- if Plan 352 has closed, managed-app successors may be marked ready for bounded plan drafting;
- Plan 353 itself moves to closed/recently-closed history.

## Failure and compatibility semantics

This plan changes repository metadata/history integration, not runtime behavior.

- deleting `target/` artifacts must not remove source evidence;
- a clean rebuild failure is a real defect and blocks closure;
- plan collision checker exceptions remain exact path sets;
- ADR rename must preserve content/history via `git mv` or an equivalent detectable rename;
- rebase conflicts that alter executable code require rerunning all affected subsystem tests;
- no forceful conflict resolution based solely on "ours"/"theirs" without content review.

## Required tests and checks

### Generated-artifact hygiene

- `git ls-files '*/target/*'` shows no tracked Cargo target output covered by the plan;
- clean external consumer fixture builds/tests from source;
- ignore behavior prevents re-adding its target tree.

### Plan-number authority

- global uniqueness checker passes;
- planning unit tests pass;
- injected third Plan-349 owner fails;
- an unrelated duplicate plan number fails;
- `plans/README.md`, collision ledger, checker, and tests name the same exact historical exception set.

### ADR authority

- portable service-tunnel ADR has a unique number;
- no live reference to its old `0032-portable-service-tunnel-policy-core-and-adapters.md` path remains;
- no live prose calls the portable decision "ADR 0032";
- managed-app ADR 0032 references remain intact;
- existing ADR 0030 duplication is recorded as pre-existing/out-of-scope, not accidentally "fixed" without a plan.

### Branch integration

- compare against `main` shows no behind commits at closure baseline;
- branch diff no longer contains generated target artifacts;
- affected docs/links pass repository checks;
- focused managed-app and portable-service-tunnel lanes pass;
- routine floor passes.

## Exact verification commands

At minimum:

```text
git ls-files '*/target/*'
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-portable-service-tunnel-consumer.sh
python3 scripts/check-portable-service-tunnel-api.py
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
```

Also run the complete routine floor in `AGENTS.md` and any Plan-352 focused commands if 352 has landed.

## Documentation updates

Required:
- `plans/README.md`;
- `plans/global-number-collision-ledger.md` only if wording needs reconciliation;
- `docs/adr/0033-portable-service-tunnel-policy-core-and-adapters.md` (renamed authority);
- all references to the renamed ADR;
- `.gitignore`;
- `plans/registry.md`;
- affected subsystem roadmaps if status changes;
- `plans/closure/managed-native-app-runtime/353-status.md` at closure.

No change to `specs/support.toml` is expected from this integration pass.

## Acceptance criteria

Plan 353 passes only when:

1. no generated Cargo output under the portable consumer fixture remains tracked;
2. a narrow ignore rule prevents recurrence;
3. the clean portable consumer fixture rebuilds/tests successfully;
4. the exact qualified Plan-349 collision is consistently documented by README, collision ledger, checker, and tests;
5. no additional Plan-349 owner is permitted;
6. no closed global plan was renumbered;
7. the portable service-tunnel ADR has a unique identifier and all references are repaired;
8. managed-app ADR 0032 remains unchanged in identity;
9. pre-existing ADR 0030 duplication is explicitly left out of scope;
10. branch is integrated with the chosen current `main` baseline and no longer behind it;
11. conflict resolution does not change support/capability claims without evidence;
12. Plan-352 status is reflected truthfully in managed-app successor readiness;
13. focused subsystem checks and full routine floor pass;
14. no runtime/product capability changes land as part of this plan.

## Stop conditions

Stop and register a narrower successor if:
- deleting tracked target output reveals that closure evidence depends on generated binaries rather than source;
- Plan-349 references are so externally published that the existing qualified-collision mechanism cannot preserve traceability;
- ADR 0033 becomes occupied and choosing a new identifier has cross-repo implications;
- rebase conflict resolution requires a substantive managed-app or service-tunnel behavior change;
- current `main` introduces a new planning authority that invalidates the execution order assumed here.

## Closure evidence required

The closure record must include:
- before/after tracked-target file counts;
- exact ignore rule added;
- clean external-consumer rebuild result;
- Plan-349 collision authority matrix;
- checker positive/negative results;
- ADR rename commit and reference-search result;
- pre-existing ADR 0030 limitation statement;
- pre/post rebase SHAs and branch divergence result;
- affected subsystem focused tests;
- full routine-floor results;
- final registry/roadmap unblock audit;
- explicit statement that no runtime capability changed.

## Handoff notes

This plan is integration hygiene, not an excuse to rewrite already-executed history.

Preserve qualified closed Plan-349 identities, remove generated artifacts, make durable planning rules agree, give the later portable ADR a unique identifier, integrate current main, and then verify from a clean source tree.

If Plan 352 is still open at the end of this work, do not unblock managed-app successor planning merely because the branch is clean.
