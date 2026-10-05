# Plan 351 — external adapter conformance and SAM handoff contract

Status: **passed-portable-service-tunnel-external-adapter-conformance-and-sam-handoff**.

Classification: **infrastructure**. This plan proves that the stabilized service-tunnel core can be consumed outside the i2pr workspace and freezes a downstream transport-adapter handoff, using a future clean-room SAM client/tunnel manager as the motivating consumer without implementing SAM in i2pr.

Hard dependencies:
- Plan 349 closed: reusable ownership/adapter semantics frozen.
- Plan 350 closed: supported public API/package revision frozen and externally consumable.

Roadmap: `plans/subsystems/portable-service-tunnels-roadmap.md`.

Canonical references:
- Plan 349 ADR/spec and closure;
- Plan 350 supported-API/package closure;
- `GUARDRAILS.md`;
- `specs/CONFORMANCE.md`;
- `plans/subsystems/service-tunnels-roadmap.md`;
- `plans/subsystems/anonymity-roadmap.md`;
- `plans/subsystems/sam-roadmap.md`.

## Objective

Prove the reusable boundary from the perspective of an independent repository.

This plan must add a deterministic external-consumer/conformance fixture that:

1. is not a member of the i2pr Cargo workspace;
2. depends only on the Plan-350-supported service-tunnel package/revision and ordinary third-party crates;
3. exercises representative client/server/filter/linkability behavior exclusively through public APIs;
4. models transport input through a tiny fake adapter rather than i2pr daemon/runtime internals;
5. demonstrates the exact metadata and lifecycle contract a future SAM implementation must supply;
6. produces a clean-room downstream handoff document for the separate SAM library/tunnel-manager repository.

No live SAM router is required for this plan. The proof target is API sufficiency and semantic reuse, not SAM interoperability.

## Why this plan was blocked pending 350

A true external fixture is meaningful only against a frozen supported API/package boundary. Building it earlier would cause the fixture itself to drive accidental API design and could stabilize private M10 details by convenience.

Plan 350 must provide:
- exact package/revision;
- supported public API snapshot;
- dependency boundary;
- publication/license disposition;
- native-consumer compatibility.

## Current implementation evidence

At registration:
- no external repository fixture imports `i2pr-service-tunnels`;
- existing service-tunnel tests execute inside the i2pr workspace;
- no conformance document maps external authenticated peer/session/identity semantics into the reusable policy core;
- no downstream handoff explains how explicit Destination groups should map to a SAM shared-session identity without conflating router/SAM implementation with policy ownership;
- no fixture proves HTTP/SOCKS/IRC/access policies can be reused without `i2pr-daemon` or `i2pr-runtime`.

## Invariants that must not regress

1. **Public-only consumption.** The fixture may not reach private modules, path-import source files, or depend on daemon/runtime/testkit internals.
2. **No SAM implementation in i2pr.** SAM commands, version negotiation, PRIMARY/MASTER quirks, Datagram2/3 wire formats, reconnection logic, and router probing remain downstream.
3. **Policy reuse, not duplication.** The fixture must call the reusable HTTP/SOCKS/IRC/access/linkability logic rather than recreate equivalent filters locally.
4. **Authenticated identity preservation.** Fake inbound peer metadata must distinguish authenticated Destination identity from unauthenticated/local transport metadata.
5. **Explicit linkability.** Dedicated/shared Destination behavior is driven by core policy, not guessed by the fake adapter.
6. **No runtime coupling.** The external fixture owns any fake clocks/local state it needs and imports no Tokio requirement unless independently chosen by the fixture.
7. **Boundedness and redaction remain intact.**
8. **Clean-room SAM handoff.** Existing SAM implementations may be cited as behavioral references, but no source/code is copied into the fixture or handoff.

## Scope

### In scope

- out-of-workspace Rust consumer fixture under a clearly non-workspace test/fixture path or sibling temporary harness strategy;
- deterministic fake stream transport/identity provider;
- external-consumer compile/test script;
- public API conformance matrix;
- downstream SAM adapter handoff specification;
- explicit mapping of service-tunnel concepts to transport responsibilities;
- negative tests proving forbidden/private dependencies are absent;
- exact Git/package pinning mechanism suitable for a future separate repository.

### Explicitly out

- starting Java I2P/i2pd/i2pr SAM bridges;
- implementing SAM client or server wire logic;
- Python/C bindings;
- WebUI/daemon/process supervisor;
- config-file parser compatibility with Java/i2pd/sam-forwarder;
- crates.io release;
- application packaging;
- changes to service-tunnel semantics merely to make the fixture simpler.

## Required external-consumer fixture

The fixture must prove at least these rows:

### Generic client/server

- construct a validated generic client service spec using only public types;
- construct a validated generic server spec/target;
- demonstrate the adapter receives enough policy metadata to open a local listener/target without core socket ownership;
- reject invalid/unbounded identifiers/endpoints using the same core validation.

### Destination/linkability

- two dedicated services produce distinct logical Destination-group owners;
- two explicitly shared services resolve to one logical linkability group;
- mixed client/server sharing is represented only when the core policy permits/configures it;
- no adapter heuristic merges services merely because type/ports match;
- persistent/key-reference semantics remain metadata/policy, with actual key storage outside the core.

### Authenticated server peer

- normalize an authenticated remote Destination hash into `ServerAccessPolicy`;
- allow/deny precedence matches native behavior;
- rate-limit decisions use an adapter-supplied monotonic clock;
- unauthenticated nickname/hostname/socket-address input cannot be passed as authenticated peer identity.

### HTTP

- client request privacy rewrite strips/normalizes the same headers as native i2pr;
- server filter rejects/rewrites the same spoofing/hop-by-hop cases;
- local target/Host projection remains adapter-owned data passed into the core;
- no router/SAM implementation identifier is added to application traffic.

### SOCKS / CONNECT / IRC

- parse/validate representative SOCKS5/CONNECT requests through public APIs;
- IRC privacy filtering uses the core policy;
- malformed/max+1 inputs produce the same bounded typed errors.

### Generation/reload

- build old/new service sets and obtain deterministic diff classification;
- fake adapter demonstrates start/stop/replace ordering from the diff without requiring the core to own tasks;
- failure of one fake start does not mutate policy state invisibly; adapter retains explicit lifecycle ownership.

## SAM downstream handoff contract

Create a reference document, suggested path:
`specs/references/portable-service-tunnel-sam-adapter-handoff.md`.

It must state the following without implementing them.

### SAM library responsibilities

A future separate repository owns:
- SAM HELLO/version/capability negotiation;
- SAM command/reply codec and state machines;
- STREAM connect/accept/forward;
- DATAGRAM/RAW and modern DATAGRAM2/3 where implemented;
- PRIMARY/MASTER/shared-session compatibility;
- naming lookup and Destination generation/persistence integration;
- router-specific capability/quirk profiles;
- reconnect/backoff/session teardown;
- async runtime and blocking facade;
- C ABI/Python bindings;
- tunnel daemon/config parser/WebUI/application-sidecar packaging.

### Shared identity/session mapping

The external adapter must preserve core linkability semantics:
- a dedicated core Destination group maps to a distinct SAM Destination/session ownership domain;
- an explicitly shared core Destination group maps to one shared SAM Destination identity;
- when multiple SAM subsessions are used under that identity, the adapter may use the router-compatible SAM shared-session spelling/mechanism, but that wire choice is not exposed to `i2pr-service-tunnels`;
- the adapter must not merge unrelated groups to save tunnel resources;
- it must not split a shared group in a way that falsifies configured identity/linkability semantics.

### Inbound server identity

For profiles requiring peer identity/access/rate/IRC hostname projection:
- the SAM adapter must obtain the authenticated remote I2P Destination/identity from the accepted stream/session surface;
- it must translate that identity into the core's canonical hash representation;
- local TCP peer address, SAM session ID, nickname, or unverified claimed name is insufficient;
- if the transport/router mode cannot provide authenticated peer identity, peer-dependent policy must fail closed or the profile must be rejected as unsupported.

### Runtime ownership

The downstream adapter owns:
- sockets/listeners;
- task spawning;
- bounded connection pumps/backpressure;
- timeouts and monotonic clock source;
- SAM connection/session lifecycle;
- local target connection;
- retries/reconnect;
- key/persistence filesystem operations;
- logging/metrics transport details.

The core owns policy decisions only.

### Filtering placement

The handoff must include byte-flow diagrams showing:
- client-side local app -> core protocol/privacy filter -> SAM transport -> I2P;
- server-side I2P/SAM accepted stream -> authenticated peer/access filter -> core application filter -> local target;
- response traffic through the corresponding response privacy filter where the profile defines one.

This prevents a downstream daemon from accidentally bypassing filters by wiring raw SAM streams directly to local applications.

## Ordered work packages

1. consume Plan 350 exact supported API/revision;
2. create the non-workspace fixture and dependency pin;
3. implement fake adapter metadata/lifecycle only;
4. add positive generic/linkability/access/filter rows;
5. add negative private-dependency/authentication/boundedness rows;
6. add deterministic generation/reload simulation;
7. freeze the SAM downstream handoff contract;
8. add CI/script entry point for the external consumer;
9. routine full-workspace regression;
10. docs/registry/roadmap/closure reconciliation.

## Failure / cancellation / restart / contention semantics

The fake adapter must model enough lifecycle semantics to prove ownership:
- policy validation precedes runtime start;
- generation diff is deterministic;
- adapter start failure is surfaced as an adapter failure and does not alter the core contract;
- restart reconstructs from validated spec plus adapter-owned persistent identity/key reference;
- connection/resource counters are bounded;
- concurrent fake connections cannot bypass shared access/rate limits when the caller serializes the documented mutable policy owner.

Do not invent production orchestration APIs solely for the fake fixture.

## Compatibility and migration

- The fixture pins the exact Plan-350-supported revision/package contract.
- It must be possible to copy the fixture's dependency/API usage into a separate repository without changing paths to private workspace modules.
- A later SAM repository may implement richer config/FFI surfaces, but it should not need to fork the policy crate.
- If the fixture exposes a missing public seam, add it only if Plan 349's ownership contract clearly says it belongs in the core; otherwise stop and record the missing downstream adapter responsibility.

## Required tests

- external fixture builds/tests outside the workspace;
- a guard proves it has no dependencies on `i2pr-daemon`, `i2pr-runtime`, `i2pr-testkit`, private source paths, or SAM implementation crates;
- all matrix rows above;
- max+1/malformed input rows;
- API snapshot unchanged except intentional Plan-351 additions;
- native i2pr service-tunnel regression suite;
- static boundary/planning checks.

## Exact verification commands

The implementation must provide one canonical external-consumer script, for example:

```text
bash scripts/check-portable-service-tunnel-consumer.sh
```

That script must create/use a clean Cargo target/cache location as needed, build the fixture as a non-workspace package, run its tests, and verify the dependency graph.

Also run:

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

- external-consumer fixture README;
- SAM adapter handoff reference;
- portable-core reference spec cross-links;
- `docs/architecture/dependency-graph.md`;
- `plans/subsystems/portable-service-tunnels-roadmap.md`;
- `plans/registry.md`;
- Plan 351 closure.

No SAM or router support claims are promoted.

## Acceptance criteria

Plan 351 passes only when:

1. Plans 349–350 are closed and their frozen boundaries are obeyed;
2. a true non-workspace consumer depends only on the supported reusable package/public API;
3. generic client/server, Destination-group, access/rate, HTTP, SOCKS/CONNECT, IRC, and generation/diff representative rows pass;
4. authenticated peer identity cannot be confused with local/unauthenticated metadata;
5. explicit shared/dedicated linkability semantics are preserved by the fake adapter;
6. no filter/policy logic is reimplemented in the fixture;
7. dependency checks prove no daemon/runtime/testkit/private-module coupling;
8. the SAM downstream handoff cleanly assigns all wire/runtime/FFI/daemon responsibilities outside i2pr;
9. byte-flow/filter-placement diagrams prevent raw-transport bypass ambiguity;
10. native i2pr behavior remains green;
11. full routine floor is green;
12. closure declares the portable service-tunnel work line complete or names a narrowly bounded corrective.

## Stop conditions

Stop and record a corrective rather than broadening scope if:

- the external fixture requires a private router/runtime type;
- authenticated peer policy cannot be expressed without transport-specific internals;
- preserving Destination sharing requires SAM-specific types in the core;
- the fixture can only work by copying filter logic;
- a missing seam belongs clearly to downstream runtime rather than core policy;
- publication/licensing becomes a prerequisite and remains unresolved;
- live SAM interoperability would be required to prove an API property in this plan.

## Closure evidence required

Include:
- implementation commits;
- exact Plan-350 package/API revision;
- external fixture dependency graph;
- external fixture command outputs;
- conformance-matrix results;
- native service-tunnel regression results;
- static-boundary/private-dependency proof;
- SAM downstream handoff document hash/revision;
- security review of authenticated identity, linkability, filter placement, and lifecycle ownership;
- explicit statement that no SAM implementation/binding/daemon exists in i2pr from this plan;
- disposition for the future separate SAM repository.

## Handoff notes

This is the last i2pr-side portability milestone before downstream work should move to its own repository.

A future SAM repository should be free to evolve its protocol implementation, runtime, C ABI, Python layer, daemon, config formats, and WebUI independently while importing the i2pr-owned service-tunnel core as the source of truth for shared tunnel filtering/policy semantics.
