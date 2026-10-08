# Plan 379 — MIT license and portable service-tunnel cleanup

Status: **registered-mit-license-and-portable-service-tunnel-cleanup**

Subsystem: `portable-service-tunnels` with repository-wide licensing/documentation scope.

Repository baseline: `4f4e98f5a0241355af5099c73065088dede1b1de` on `main`.

Registration branch: `plans/379-mit-license-portable-cleanup`.

Primary class: **polish + infrastructure**, with an **invariant** preservation component.

Source roadmap:

- `plans/subsystems/portable-service-tunnels-roadmap.md`

Canonical references:

- `GUARDRAILS.md`
- `specs/CONFORMANCE.md`
- `specs/support.toml`
- `docs/adr/0033-portable-service-tunnel-policy-core-and-adapters.md`
- `specs/references/portable-service-tunnel-core-v1.md`
- `specs/references/portable-service-tunnel-sam-adapter-handoff.md`
- `plans/global-number-collision-ledger.md`

## 1. Objective

Close the remaining repository hygiene around the portable service-tunnel extraction now
that the owner has selected the MIT license, without reopening M10 or rewriting the
historical Plan 349–351 evidence.

The pass must leave one truthful current state:

1. the repository-wide MIT license is represented consistently in repository and Cargo
   metadata;
2. every production package reports the intended license, with exceptions explicit;
3. `i2pr-service-tunnels` remains runtime-neutral and externally Git-consumable at a
   current post-Plan-359 revision;
4. Plan 350's historical “publication blocked by license selection” evidence remains
   immutable, while current planning states that the legal-selection gate is lifted and
   any remaining distribution blocker is technical/package-graph specific;
5. Plan 359 receives a conventional additive closure record instead of remaining only an
   executed-inline plan plus registry claim;
6. the obsolete `plans/349-351-portable-service-tunnels` branch is retired only after its
   useful commits are proven present or superseded on `main`; generated fixture artifacts
   on that branch are never merged;
7. the downstream handoff points at `dbowm91/i2pr-sam` without moving SAM wire/runtime
   ownership back into i2pr.

The registration commit itself records the owner-selected MIT license in `LICENSE`,
`README.md`, and `[workspace.package]`. The implementation pass owns the remaining
metadata, evidence, and cleanup work.

## 2. Why this milestone is ready

Plans 349–351 are closed and their external-consumer proof exists. Plan 359 has already
landed its one permitted `i2pr-proto` edge and mutation-tested guard amendment. The
global collision ledger records the historical plan collisions. The new
`dbowm91/i2pr-sam` repository exists, so the downstream consumer is no longer
hypothetical.

The owner has explicitly selected MIT. That does **not** imply that every crate is
immediately crates.io-publishable.

## 3. Current implementation evidence

At this baseline:

- `i2pr-service-tunnels` is `publish = false`.
- It has one permitted workspace dependency:
  `i2pr-proto = { path = "../i2pr-proto" }`.
- `scripts/check-service-tunnel-boundaries.sh` permits exactly that workspace edge and
  still forbids daemon/runtime/NetDB/transport/tunnel/testkit ownership.
- Plan 351's standalone fixture pins the historical Plan 350 revision
  `fa0824970b67b5ee90d5907765c408ccd0a19941`; that remains valid historical evidence but
  is not a current post-Plan-359 consumer pin.
- The obsolete portable planning branch contains generated `target/` contents under the
  fixture. Those artifacts are absent from `main`; the live checker uses a temporary
  `CARGO_TARGET_DIR`.
- Plan 359 is marked passed in the registry and contains mutation evidence, but
  `plans/closure/portable-service-tunnels/359-status.md` does not exist.
- Plan 350's closure truthfully recorded “publication blocked by license selection” at the
  time it executed. That record must not be rewritten.
- This registration branch adds the standard MIT license, updates the README license
  statement, and adds `license = "MIT"` to `[workspace.package]`.

## 4. Invariants that must not regress

- M10 Plan 215 remains the service-tunnel product authority.
- Plans 349–351 and their closures remain historical records.
- Plan 359 stays narrow: `i2pr-proto` is the sole permitted direct workspace dependency
  of `i2pr-service-tunnels` unless a later plan explicitly changes it.
- The portable core owns no Tokio runtime, socket/listener, filesystem, DNS, process,
  NetDB, transport, tunnel-pool, or router-global lifecycle.
- MIT does not authorize source copying from external implementations; clean-room and
  provenance rules remain in force.
- No protocol-support or anonymity claim changes.
- No crate is published as part of this plan.
- No remote branch is deleted until its commits are classified.

## 5. Scope

### In scope

- repository-wide MIT license metadata reconciliation;
- per-package Cargo license inheritance audit;
- current portable-core package graph and `cargo package` feasibility audit;
- current external-consumer pin/evidence suitable for downstream `i2pr-sam`;
- additive Plan 359 closure normalization;
- roadmap/registry/docs reconciliation;
- obsolete portable branch disposition and generated-artifact audit;
- static license-metadata drift protection where practical.

### Explicitly out of scope

- publishing any crate to crates.io;
- changing the public service-tunnel API for convenience;
- removing the valid `i2pr-proto` encrypted-service dependency;
- implementing SAM in i2pr;
- implementing anything in `i2pr-sam`;
- renumbering historical plan/ADR collisions;
- rewriting Plan 350's closure token;
- adding file-by-file copyright headers;
- changing product behavior, support inventory, listeners, or defaults.

## 6. Required production changes

There should be **no production behavior change**.

### Cargo and package metadata

Audit every workspace package manifest. Where packages inherit repository metadata, use
explicit `license.workspace = true` or an equally unambiguous package-level
`license = "MIT"`. Record intentional tooling/non-publishable exceptions.

For `i2pr-service-tunnels`, determine package-distribution truth rather than assuming
the legal gate was the only gate. Inspect the path-only `i2pr-proto` dependency and
whether crates.io packaging requires a versioned, publishable `i2pr-proto` first. If
that prerequisite is not already satisfied, retain `publish = false` and state the
technical blocker explicitly.

### External-consumer evidence

Refresh or supplement the historical Plan 351 fixture with a current pinned revision
after Plan 379 package metadata changes land. Prove that an external repository can
consume the post-Plan-359 API and its one allowed `i2pr-proto` dependency without
reaching daemon/runtime/private paths.

Do not destroy the historical Plan 351 pin merely to simplify current maintenance.

### Planning/closure normalization

Add `plans/closure/portable-service-tunnels/359-status.md` as an additive status record
pointing to Plan 359's already-landed evidence. It may summarize and verify; it must not
pretend Plan 379 executed Plan 359.

Update current roadmap/registry text so it distinguishes Plan 350's historical legal gate
from the current MIT-selected state and names `dbowm91/i2pr-sam` as the downstream SAM
owner.

### Branch and artifact cleanup

Compare `plans/349-351-portable-service-tunnels` to current `main`. Classify every
unique source/document/test change. Generated `target/` artifacts are disposable and
must never be merged. Delete the obsolete remote branch only after the comparison proves
there is no unique required source/evidence remaining.

## 7. Ordered work packages

### WP A — license metadata convergence

Audit workspace manifests, add explicit inheritance/metadata as needed, and add or extend
a checker if package license drift can otherwise pass silently.

Acceptance: `cargo metadata --no-deps` reports the intended license for every production
package; root `LICENSE`, README, and Cargo metadata agree.

### WP B — current portable package/distribution audit

Inspect `cargo package -p i2pr-service-tunnels`, the `i2pr-proto` edge, and publish
metadata. End with one explicit status: `git-consumable`,
`crates.io-ready-but-unpublished`, or `crates.io-blocked-by-<named prerequisite>`.

Do not publish.

### WP C — rebaseline external-consumer proof

Pin a current external fixture to the post-metadata implementation revision, rerun the
policy/filter conformance matrix, and prove the resolved dependency tree contains only
the reviewed public package graph.

### WP D — normalize Plan 359 and current documentation

Add the missing conventional Plan 359 status record and reconcile roadmap/registry/current
architecture text. Preserve Plan 350/351 closure records.

### WP E — retire obsolete branch

Compare the branch to `main`, record any intentional abandonment, verify generated
artifacts are absent from `main`, then delete the obsolete branch only if no unique
required work remains.

## 8. Failure, cancellation, restart, and contention semantics

Partial failure must leave conservative state. If package feasibility is unclear, keep
`publish = false`. If a current external pin cannot be established reproducibly,
retain the historical fixture and report the blocker. If branch comparison reveals
unique non-generated work, do not delete the branch.

A concurrent change touching Cargo package metadata, the portable API snapshot, boundary
guard, or collision ledger requires rebase and re-evaluation before closure.

## 9. Compatibility and migration

The MIT selection changes repository licensing, not runtime compatibility. No config,
wire, storage, or user migration exists.

Downstream consumers may continue using Git revisions. A future crates.io migration may
require publishing/versioning `i2pr-proto` first and is not authorized here.

## 10. Required tests

- Cargo metadata license audit for production crates.
- Portable-core API snapshot checker.
- Service-tunnel dependency/runtime boundary checker and positive controls.
- Current external-consumer conformance lane.
- Focused `i2pr-service-tunnels` tests.
- Global plan-number and ADR uniqueness tests.
- Negative proof that a non-`i2pr-proto` workspace dependency remains rejected.
- Proof generated fixture target contents are not tracked on `main`.

## 11. Required verification commands

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-service-tunnels --all-targets -- --test-threads=1
cargo clippy --locked -p i2pr-service-tunnels --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked -p i2pr-service-tunnels --no-deps

cargo metadata --format-version 1 --no-deps
cargo tree -p i2pr-service-tunnels --edges normal
python3 scripts/check-portable-service-tunnel-api.py
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-portable-service-tunnel-consumer.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-adr-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

Run `cargo package -p i2pr-service-tunnels` only after the package graph audit says it
is expected to be valid. A path-dependency failure is evidence for a named technical
blocker, not permission to weaken metadata.

The closure must also run the then-current routine floor from `AGENTS.md` and report
only commands actually executed.

## 12. Documentation updates

- root README;
- portable service-tunnel roadmap;
- planning registry;
- Plan 359 additive closure record;
- portable SAM handoff, naming `dbowm91/i2pr-sam`;
- package README/docs if distribution posture changes.

## 13. Acceptance criteria

Plan 379 closes only when:

1. repository license is MIT and Cargo metadata agrees for production packages;
2. clean-room/provenance rules remain explicit;
3. current package distribution posture is named and evidenced;
4. a post-Plan-359 external Git consumer passes against a fixed revision;
5. Plan 359 has a conventional additive closure record;
6. historical Plan 349–351 closures are not rewritten to make present facts retroactive;
7. current planning points SAM client/runtime work to `dbowm91/i2pr-sam`;
8. the obsolete portable branch is safely deleted or retained for a specific reason;
9. generated fixture build artifacts are not tracked on `main`;
10. no runtime/protocol/support behavior changed.

## 14. Stop conditions

Stop and report if:

- third-party provenance conflicts with the repository MIT statement;
- crates.io readiness requires publishing or materially redesigning another crate;
- the obsolete branch contains unique non-generated work not yet classified;
- the current external-consumer pin requires weakening the portable boundary;
- licensing/provenance evidence contradicts the owner's explicit MIT selection.

## 15. Closure evidence required

Record exact SHAs, package-by-package license metadata, package/distribution audit,
external-consumer pin/test output, Plan 359 normalization, branch disposition, generated
artifact absence, focused/routine-floor results, and an explicit no-product-behavior
statement.

## 16. Handoff notes

This is intended as the final i2pr-side portability cleanup before `i2pr-sam` becomes
the active consumer. If that repository discovers a genuinely missing transport-neutral
policy seam, file a new corrective in i2pr with an external failing fixture rather than
moving SAM ownership back into this repository.
