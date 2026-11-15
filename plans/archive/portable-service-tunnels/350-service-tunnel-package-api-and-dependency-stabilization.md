# Plan 350 — service-tunnel package API and dependency stabilization

Status: **passed-portable-service-tunnel-package-api-stabilization-publication-blocked-by-license-selection**.

Classification: **infrastructure + polish**. This plan turns the Plan-349-frozen reusable boundary into an externally consumable Rust package/API without changing service-tunnel semantics.

Hard dependency:
- Plan 349 must close with the ownership ADR, portable-core reference specification, complete module/export classification, and static boundary guards.

Roadmap: `plans/subsystems/portable-service-tunnels-roadmap.md`.

Canonical references:
- Plan 349 closure and portability ADR/spec;
- `GUARDRAILS.md`;
- `specs/CONFORMANCE.md`;
- `plans/subsystems/service-tunnels-roadmap.md`;
- `plans/subsystems/anonymity-roadmap.md`.

## Objective

Stabilize `i2pr-service-tunnels` as a package that a separate Rust repository can depend on without importing the rest of the i2pr router workspace.

The pass must:

1. remove accidental/dead internal dependencies proven unnecessary by Plan 349;
2. normalize public API naming/docs around durable service-tunnel semantics rather than implementation-plan history;
3. define the initial supported semver surface;
4. make Cargo package metadata and dependency declarations self-contained enough for an external consumer;
5. prove existing i2pr native consumers compile and behave identically;
6. make packaging mechanically verifiable while keeping actual public publication blocked until the repository owner explicitly selects licensing.

No downstream SAM implementation is part of this plan.

## Why this plan was blocked pending 349

Package/API work creates durable compatibility obligations. It must not proceed until Plan 349 identifies which current exports are intentionally reusable and which are internal accidents.

Plan 349 also decides whether any internal split is actually necessary. Plan 350 must implement that frozen decision rather than invent a second boundary while editing Cargo metadata.

## Current implementation evidence

Registration-time package state:

```toml
[package]
name = "i2pr-service-tunnels"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
repository.workspace = true
description = "Runtime-neutral Milestone 10 service-tunnel configuration, destination references, and policy"
publish = false

[dependencies]
base64ct.workspace = true
i2pr-proto = { path = "../i2pr-proto" }
sha2.workspace = true
subtle.workspace = true
thiserror.workspace = true
zeroize.workspace = true
```

Additional facts:
- workspace package version is currently `0.1.0`;
- repository MSRV is 1.88;
- the repository README explicitly says no repository-wide license has been selected;
- no root LICENSE/COPYING file is present at registration;
- `i2pr-service-tunnels` currently inherits workspace metadata suitable for internal builds but has not been qualified with `cargo package`;
- the current crate-level documentation still contains historical Plan/Milestone wording that should not define an external API contract.

## Invariants that must not regress

1. Plan 349's ownership and runtime-neutrality contract is authoritative.
2. Existing i2pr service-tunnel behavior remains unchanged.
3. No dependency on daemon/runtime/router internals enters the reusable package.
4. Removing a dependency requires compiler/tree/search evidence that it is genuinely unused in every enabled production configuration.
5. Public types stabilized here remain bounded and avoid exposing secret/router-internal material.
6. Public API changes must be explicit and migration-reviewed, not incidental visibility cleanup.
7. No actual crates.io/public package release occurs without explicit owner-selected licensing.
8. The package remains clean-room i2pr-owned code; external SAM/router sources are not copied.

## Scope

### In scope

- full `cargo tree` and source-use audit;
- remove `i2pr-proto` if and only if Plan-349 evidence proves it unused;
- remove any other accidental internal-only dependency proven dead;
- package metadata: description, README path, documentation URL/homepage/categories/keywords as appropriate, explicit package include/exclude hygiene if needed;
- license metadata only after an explicit repository-owner licensing decision already exists; otherwise retain publication gate;
- public API visibility/name/doc cleanup authorized by Plan 349;
- deprecation aliases where needed to avoid unnecessary breakage for current callers;
- explicit semver policy for the initial `0.1.x` reusable surface;
- API snapshot/review mechanism suitable for catching accidental public-surface growth;
- package dry-run and external dependency-resolution proof where legally possible;
- i2pr native adapter regression proof.

### Explicitly out

- selecting the license;
- publishing to crates.io;
- new service-tunnel features;
- SAM protocol/client code;
- serde/config-file compatibility promises unless Plan 349 explicitly froze them;
- Python/C ABI;
- daemon/WebUI/process lifecycle;
- generic async runtime abstraction.

## Required production changes

### 1. Prove and clean dependency direction

Run:
- `cargo tree -p i2pr-service-tunnels --edges normal`;
- full-tree source/reference search;
- all-features/all-targets compile.

If `i2pr-proto` remains unused, remove it and add a regression check preventing accidental reintroduction unless the portability ADR is amended.

If it is used in an overlooked configuration, stop dependency removal and either:
- expose the minimal transport-neutral value in the service-tunnel crate itself; or
- record a justified reusable lower-layer dependency that remains externally consumable.

Do not copy protocol types merely to eliminate a legitimate dependency.

### 2. Normalize package-facing documentation

Replace milestone-only crate framing with durable language such as:
- runtime-neutral I2P service-tunnel configuration/policy;
- Destination/linkability policy;
- HTTP/SOCKS/IRC/CONNECT filters;
- access/rate/resource policy.

Historical Plan references may remain in implementation comments where useful, but rustdoc's top-level contract must not require readers to understand i2pr's milestone history.

### 3. Freeze the supported public surface

Using Plan 349's classification:
- keep intentional reusable types/functions public;
- make accidental implementation helpers crate-private where source-compatible;
- where existing in-repo callers depend on accidental exports, either migrate them in this pass or retain a documented deprecated compatibility alias;
- add an API-surface snapshot/checker appropriate to the repository toolchain.

The supported surface must include enough for Plan 351's external consumer, but no runtime owner.

### 4. External-package metadata

Make `cargo metadata` / `cargo package` inputs self-contained.

The plan must explicitly handle the no-license state:

- If the owner has selected and committed a repository/crate license before implementation, add correct Cargo license metadata and package qualification may proceed through a full dry run.
- If no license has been selected, package contents may be mechanically staged/tested locally, but `publish = false` remains and the closure must state `publication-blocked-by-license-selection`.
- Do not invent a license string, copy another Eggstack repository's license, or infer one from external reference projects.

A Git dependency from an owner-controlled downstream repository may be used for development qualification, but that does not substitute for a public licensing decision.

### 5. Preserve in-repo composition

Update native i2pr consumers only as required by stabilized names/visibility. No daemon logic or service behavior should otherwise change.

## Ordered work packages

1. re-read Plan 349 closure and freeze commit;
2. exact dependency/use audit;
3. public API snapshot baseline;
4. package-facing documentation cleanup;
5. dependency cleanup;
6. supported-surface visibility/name changes plus in-repo migrations;
7. package metadata and licensing gate;
8. package/API checks;
9. full native service-tunnel regressions;
10. docs/registry/roadmap/closure reconciliation.

## Failure / cancellation / restart / contention semantics

No runtime semantics are changed.

Fail the plan rather than broadening scope if:
- the external package requires an i2pr-private dependency not allowed by Plan 349;
- public API stabilization requires a semantic service-tunnel behavior change;
- `cargo package` requires a license that has not been selected;
- API snapshot tooling would require an unjustified heavy build/dependency addition.

If licensing remains unresolved, close only the package/API stabilization portion with an explicit publication blocker; do not relabel that as a public release.

## Compatibility and migration

- Keep the package at the repository's current pre-1.0 semver line unless a separate release decision says otherwise.
- Document the initial external supported surface.
- Avoid needless renames; durable conceptual names are preferred over milestone-number names.
- Existing native i2pr consumers must compile against the same public surface a downstream repository receives.
- If a source-breaking cleanup is necessary, include exact old -> new mapping and prove all in-repo callers migrated.

## Required tests

- `cargo tree`/dependency checker proving no forbidden internal edge;
- all-target/all-feature compile;
- API snapshot check and a positive-control fixture that catches one intentionally inserted public item/change;
- rustdoc with warnings denied;
- package contents inspection;
- `cargo package --allow-dirty --no-verify` and verified package build/test only when licensing/package gate allows it;
- representative service-tunnel policy regressions;
- native daemon/service product regression tests already required by M10;
- planning/static boundary checks.

## Exact verification commands

Focused:

```text
cargo fmt --all --check
cargo tree -p i2pr-service-tunnels --edges normal
cargo check --locked -p i2pr-service-tunnels --all-targets
cargo test --locked -p i2pr-service-tunnels --all-targets -- --test-threads=1
cargo clippy --locked -p i2pr-service-tunnels --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-service-tunnels --no-deps
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

When the package is legally/metadata-eligible:

```text
cargo package -p i2pr-service-tunnels --allow-dirty
```

Inspect the generated package file list and compile/test the packaged crate rather than assuming the dry run is sufficient.

Routine closure floor:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-fixture-manifest.sh
cargo deny check advisories bans sources
```

## Documentation updates

- crate-level rustdoc;
- package README or dedicated architecture doc if Plan 349 selects one;
- portable-core reference spec if package/API details need a non-semantic appendix;
- `docs/architecture/dependency-graph.md`;
- `plans/subsystems/portable-service-tunnels-roadmap.md`;
- `plans/registry.md`;
- Plan 350 closure.

Do not update support claims.

## Acceptance criteria

Plan 350 passes only when:

1. Plan 349 is closed and its ADR/spec are obeyed;
2. dependency audit is complete and every retained internal dependency is justified;
3. dead `i2pr-proto` coupling is removed if proven dead, or its retained role is explicitly justified;
4. public rustdoc describes durable reusable semantics rather than M10 planning history;
5. the supported public API is explicitly frozen/snapshotted;
6. native i2pr consumers pass without behavior change;
7. package metadata is externally coherent;
8. actual publication remains disabled unless an explicit owner-selected license exists;
9. package dry-run/build evidence is recorded when eligible;
10. no runtime/SAM/binding code lands;
11. full routine floor is green;
12. Plan 351 is unblocked with a precise supported API/commit pin.

## Stop conditions

Stop and record a corrective/blocker if:

- licensing is required for the next action and remains undecided;
- removing an internal dependency changes semantics or forces code duplication;
- a required public surface exposes secrets/router-global owners;
- package qualification only succeeds by depending on unpublished private i2pr crates contrary to Plan 349;
- a new dependency would materially increase unsafe/supply-chain surface without review;
- stabilizing the API would require implementing the future external adapter itself.

## Closure evidence required

Include:
- implementation commits;
- Plan 349 closure/freeze reference;
- before/after dependency tree;
- before/after public API snapshot;
- package metadata diff;
- license/publication disposition;
- package dry-run/build evidence when eligible;
- native-consumer regression results;
- exact focused/routine command outcomes;
- dependency/security review;
- known API limitations;
- unblock audit for Plan 351.

## Handoff notes

Treat `publish = false` and missing licensing as two different things. The former is a Cargo switch; the latter is an owner/legal decision. Do not turn a portability plan into an implicit licensing decision.

The key deliverable is a clean, stable Rust dependency boundary that a separate repository can import at an exact revision even before public registry publication is appropriate.
