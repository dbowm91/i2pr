# Plan 349 — portable service-tunnel boundary and ownership contract

Status: **passed-portable-service-tunnel-boundary-and-ownership-contract**.

Classification: **invariant + infrastructure**. This plan freezes the reusable ownership boundary around `i2pr-service-tunnels`. It does not implement a SAM client, tunnel daemon, Python/C binding, WebUI, application sidecar, or new router capability.

Hard dependencies: none. M10 product authority is already closed at Plan 215. This plan is a parallel portability/reuse pass and must not reinterpret that closure.

Roadmap: `plans/subsystems/portable-service-tunnels-roadmap.md`.

Canonical references:
- `GUARDRAILS.md`
- `specs/CONFORMANCE.md`
- `specs/support.toml`
- `docs/architecture/overview.md`
- `docs/architecture/dependency-graph.md`
- `plans/subsystems/service-tunnels-roadmap.md`
- `plans/subsystems/anonymity-roadmap.md`
- `plans/subsystems/sam-roadmap.md`
- `plans/subsystems/managed-native-app-runtime-roadmap.md`

## Objective

Freeze one durable contract that lets i2pr continue owning service-tunnel filtering/privacy/linkability semantics while allowing a future independent transport implementation — specifically a clean-room modern SAM client/tunnel manager — to consume those semantics without copying them.

The plan must:

1. inventory the current `i2pr-service-tunnels` public and internal surface;
2. distinguish reusable policy/protocol semantics from i2pr-only runtime/orchestration concerns;
3. add an ADR and reference specification describing that ownership boundary;
4. define the adapter-facing data contract needed by an external transport owner;
5. add/strengthen static dependency/runtime guards so the reusable crate cannot drift into router/runtime ownership;
6. leave all package publication, semver surgery, and external-consumer fixtures to Plans 350–351.

The outcome is a contract/invariant freeze. No public service behavior changes are required for closure.

## Why this plan is ready

The reusable seam already exists:

- `i2pr-service-tunnels` describes itself as runtime-neutral policy/protocol code.
- It currently owns access/rate policy, destination references and grouping, HTTP client/server filtering, SOCKS5, IRC, CONNECT, Streamr profile configuration, idle/resource policy, outproxy policy, generation/diff helpers, and bounded events/errors.
- The crate owns no sockets, Tokio tasks, timers, filesystem access, transport internals, NetDB mutation, or Garlic/I2NP construction.
- `i2pr-daemon` remains the composition/socket owner for native service tunnels.
- M10 is closed, so this plan can audit/stabilize the boundary without being used as an excuse to reopen feature work.

A future SAM repository therefore needs an explicit contract and stable public surface, not another copy of the service-tunnel logic.

## Current implementation evidence

At registration:

- `crates/i2pr-service-tunnels/Cargo.toml` is `publish = false`.
- The package inherits workspace version `0.1.0`, edition 2024, and MSRV 1.88.
- The manifest declares `i2pr-proto = { path = "../i2pr-proto" }`.
- Registration-time source review found no obvious direct `i2pr_proto` use in the top-level service-tunnel modules reviewed; Plan 350 must prove the full-tree result before deleting the dependency.
- The repository README explicitly states that no repository-wide license has been selected.
- No durable document currently states that the service-tunnel policy crate is intended for external transport consumers.
- No explicit adapter contract currently describes how an external runtime supplies authenticated peer identity, destination/linkability ownership, local listener/target metadata, or lifecycle clocks without reaching into i2pr daemon/runtime internals.
- No out-of-workspace consumer test currently proves that the public surface is sufficient.

These are the baseline facts. Do not infer publication readiness from the crate being runtime-neutral.

## Invariants that must not regress

1. **Single policy owner.** HTTP/SOCKS/IRC/CONNECT filtering, access/rate policy, destination/linkability policy, and bounded service-profile semantics remain owned in one reusable i2pr crate rather than copied into transport adapters.
2. **Runtime neutrality.** The reusable crate owns no Tokio, sockets, timers, filesystem I/O, DNS, process lifecycle, TLS listeners, NetDB stores, tunnel pools, or router-global context.
3. **No transport authority.** The reusable crate may consume normalized metadata but does not connect/accept SAM, Streaming, I2CP, or clearnet sockets.
4. **Authenticated peer identity.** Server access/rate policy may only act on authenticated I2P peer identity supplied by the transport. A string hostname, SAM nickname, local socket address, or unauthenticated datagram source may not be substituted.
5. **Explicit linkability.** Destination sharing remains an explicit configured linkability domain. An adapter may not silently merge dedicated identities or split a shared group.
6. **M10 behavior preservation.** Existing i2pr service-tunnel behavior must not change as a side effect of making the crate portable.
7. **No new SAM implementation.** The existing i2pr SAM server authority and future external SAM client implementation remain separate concerns.
8. **Clean-room boundary.** External SAM implementations/libraries are behavioral/specification references only; no source is copied into this work line.
9. **Boundedness.** Existing ceilings and fail-closed behavior stay enforceable through public APIs; portability must not require unbounded strings, buffers, lists, or diagnostics.
10. **No license assumption.** This plan does not select a license or authorize package publication.

## Scope

### In scope

- full dependency/public-API inventory for `i2pr-service-tunnels`;
- classification of each module/type as reusable core, i2pr adapter concern, or candidate for later split;
- durable ADR for ownership and dependency direction;
- `specs/references/portable-service-tunnel-core-v1.md` (or equivalent) freezing the adapter-facing contract;
- static guard updates proving no forbidden runtime/router dependencies;
- documentation of Destination-group/linkability semantics for external transports;
- documentation of authenticated peer identity requirements;
- documentation of lifecycle ownership: adapter owns sockets/tasks/reconnect/timers; core owns policy/state decisions only;
- exact successor interface for Plans 350–351.

### Explicitly out

- changing `publish = false`;
- crates.io publication;
- selecting or adding a repository license;
- removing dependencies merely because they look unused before a full proof;
- adding async runtime traits;
- adding SAM command parsing/session logic;
- adding Python/C ABI;
- adding daemon/WebUI/config-file syntax;
- moving M10 runtime code out of `i2pr-daemon`;
- changing `specs/support.toml` capability claims.

## Required production/planning changes

### 1. Freeze the ownership ADR

Create the next available ADR recording:

- `i2pr-service-tunnels` is the canonical service-profile policy/filter owner.
- Native i2pr and external transports are adapters/consumers.
- Runtime concerns remain outside the core.
- Explicit Destination-group semantics are transport-independent.
- Authenticated peer identity is a required input for server peer policies.
- Public API evolution follows semver once Plan 350 declares the supported surface.
- SAM-specific protocol/session behavior belongs in a downstream repository, not this crate.
- no package publication is permitted until licensing is explicitly resolved.

The ADR must distinguish architectural reuse from code-copying from external routers/libraries.

### 2. Produce the module/type inventory

For every production module under `crates/i2pr-service-tunnels/src`, record:

- current public exports;
- direct crate dependencies;
- whether the item is transport-independent;
- whether any type names/docs expose M10-only implementation history;
- whether an external transport can supply all required inputs without i2pr private types;
- whether it should remain public, become crate-private, or be deferred to Plan 350 for stabilization.

Do not mechanically make more items public in this plan.

### 3. Define the external adapter contract

The reference specification must describe, at minimum:

- validated service specification/set input;
- destination reference and explicit linkability-group semantics;
- local listener/target description ownership;
- outbound destination resolution boundary;
- inbound authenticated peer identity/hash representation;
- stream direction and local/remote port metadata where profile logic requires it;
- monotonic-time input for rate/idle policy;
- lifecycle/generation-diff contract;
- resource admission/release ownership;
- error classification and redaction expectations;
- what is intentionally *not* part of the contract: sockets, reconnect loop, DNS resolver, SAM version/capabilities, file persistence, key storage, process lifecycle.

Prefer value types and pure decisions over async traits.

### 4. Strengthen static guards

Extend the existing service-tunnel boundary checker (or add a narrowly named companion) so CI fails if the reusable crate gains forbidden production dependencies or source ownership.

At minimum guard against:
- `i2pr-daemon`, `i2pr-runtime`, NetDB stores, transport implementations;
- Tokio/async runtime ownership;
- std/Tokio socket/listener types in production code;
- process/filesystem/DNS ownership where the current boundary forbids it.

Include positive-control tests so the checker cannot silently become vacuous.

## Ordered work packages

1. baseline inventory and dependency graph;
2. ADR + portable-core reference spec freeze;
3. public-surface classification table;
4. adapter-contract examples using abstract/fake transport metadata only;
5. static guard changes and teeth tests;
6. docs/roadmap/registry reconciliation;
7. focused and routine verification;
8. closure record and successor unblock audit.

Do not start Plan 350 package/public-API changes before the ADR/spec freeze commit.

## Failure / cancellation / restart / contention semantics

This plan adds no runtime owner, so production failure semantics must remain unchanged.

Planning/implementation failures are fail-closed:
- if a needed external consumer seam would require router-global state, stop and record the missing abstraction rather than exposing internals;
- if a pure policy currently depends on hidden runtime behavior, record the dependency and move it only under an explicit corrective/split decision;
- if a candidate public type cannot be bounded or redacted safely, do not stabilize it;
- concurrent service-generation semantics remain exactly as M10/anonymity authorities currently define them.

## Compatibility and migration

No consumer-visible behavior change is expected.

The plan may add documentation, ADRs, reference specs, static checks, and visibility annotations only when they preserve existing i2pr callers. Any source-breaking public change belongs in Plan 350 after this plan identifies the supported external surface.

Existing M10 closure tokens and historical plans remain untouched.

## Required tests

- static guard positive/negative fixtures;
- full source/dependency inventory checker or equivalent deterministic evidence;
- compile tests proving the reference examples use no private i2pr daemon/runtime types;
- regression tests for representative:
  - Destination/group policy;
  - server access policy;
  - HTTP client privacy rewrite;
  - HTTP server filter;
  - SOCKS request policy;
  - IRC filter;
  - generation/diff behavior;
- existing service-tunnel boundary/acceptance tests;
- planning uniqueness/tests.

## Exact verification commands

Focused:

```text
cargo fmt --all --check
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

At implementation/closure:
- next available portability/ownership ADR;
- new portable-core reference specification;
- `docs/architecture/dependency-graph.md`;
- `docs/architecture/overview.md` if the reusable boundary is absent/stale;
- crate-level `i2pr-service-tunnels` documentation removing milestone-only framing from the intended reusable contract where appropriate;
- this roadmap §7/§12;
- `plans/registry.md`;
- closure record.

Do not promote router support claims.

## Acceptance criteria

Plan 349 passes only when:

1. durable ADR authority defines the reusable service-tunnel owner and adapter boundary;
2. every production module/public export has been classified for portability;
3. the external adapter contract can be satisfied without `i2pr-daemon`, `i2pr-runtime`, NetDB stores, transport managers, or router-global state;
4. Destination/linkability and authenticated-peer semantics are explicit and transport-independent;
5. runtime ownership remains outside the reusable crate;
6. static guards mechanically detect prohibited dependency/runtime drift and have positive controls;
7. representative policy/profile regression rows are green;
8. no M10 behavior/support claim changes;
9. no SAM client/daemon/binding code lands;
10. no publication/license choice is made;
11. full routine floor is green;
12. closure identifies the exact Plan 350 public/package changes now justified by the frozen contract.

## Stop conditions

Stop and record a corrective/decision rather than broadening scope if:

- an external adapter would require a router-global/private type to exercise existing policy;
- making the boundary portable requires moving sockets/tasks/timers into the core;
- a service profile is discovered to depend on transport-specific behavior not represented in the current policy model;
- the only proposed solution is a generic async transport trait coupled to Tokio;
- the work would change M10 semantics without a separately proven defect;
- a licensing/publication decision becomes necessary before the owner has selected one;
- any external implementation source would need to be copied rather than referenced behaviorally.

## Closure evidence required

The closure record must include:

- implementation commits;
- ADR/spec freeze commit preceding public/package changes;
- complete module/export classification;
- current dependency graph and forbidden-edge proof;
- requirement-to-evidence matrix;
- exact focused/routine command outcomes;
- static-guard positive-control evidence;
- compatibility/security review covering authenticated peer identity, Destination sharing/linkability, privacy filters, and error redaction;
- explicit statement that no SAM implementation or new user-visible tunnel capability exists;
- unblock audit for Plan 350.

## Handoff notes

The main review question is whether an independently implemented transport can drive the same service-tunnel semantics without importing router internals or reimplementing filters.

Do not optimize for the future SAM repository's convenience by putting SAM concepts into the core. The reusable boundary should describe I2P service semantics; a SAM adapter is one consumer.
