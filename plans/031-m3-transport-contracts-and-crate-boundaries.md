# Plan 031: transport contracts and crate boundaries

## Objective

Create the transport-neutral ownership and delivery contracts that NTCP2 and future transports will use, establish the Milestone 3 crate graph, and provide deterministic synthetic evidence without implementing handshake cryptography, data-phase encryption, or live TCP behavior.

This plan is the architectural foundation for Milestone 3. It must remain narrow. The output is a set of bounded contracts and crate skeletons that later plans can implement without inventing incompatible queue, link, address, or lifecycle abstractions.

## Preconditions

- `plans/025-closure.md` exists and remains satisfied.
- `plans/030-milestone-3-overview.md` is the controlling milestone document.
- Existing runtime ownership and dependency checks are green.
- No unresolved change to the Milestone 3 crate boundary is hidden in implementation work.

## Scope

This plan may add:

- `crates/i2pr-transport/`
- `crates/i2pr-transport-ntcp2/` as an initially non-cryptographic skeleton
- workspace membership and dependency-direction checks
- an ADR for transport boundaries and Tokio confinement
- transport-neutral typed values, state, outcomes, snapshots, and test-only factories
- resource classes or bounded snapshot fields that are demonstrably required by transport ownership
- deterministic synthetic tests below the socket and wire-crypto boundary

This plan must not add:

- NTCP2 key derivation or Noise transcript code;
- SessionRequest, SessionCreated, or SessionConfirmed wire implementation;
- encrypted data frames;
- real sockets, DNS, listeners, or dialers;
- RouterInfo publication or live reachability claims;
- NetDB mutation, peer scoring, tunnels, or client delivery;
- capability advertisement.

## Required ADR

Create an ADR that records:

1. Final crate boundaries and dependency direction.
2. Why `i2pr-runtime` remains the sole Tokio/socket owner.
3. Why protocol state machines are driven through explicit input/output actions rather than async traits.
4. Why transport-neutral contracts are deliberately narrow and do not model every future transport feature.
5. Buffer and message ownership at the transport-manager boundary.
6. Rejected alternatives, including:
   - putting Tokio directly in `i2pr-transport-ntcp2`;
   - a generic plugin transport framework;
   - raw `Vec<u8>` everywhere without validated bounds;
   - exposing raw Tokio channels or sockets across crate boundaries;
   - merging all transport logic into `i2pr-runtime`.

## Target dependency graph

The expected graph is:

```text
i2pr-core <--------- i2pr-transport <--------- i2pr-runtime
     ^                      ^                         ^
     |                      |                         |
i2pr-proto ---------- i2pr-transport-ntcp2 ----------+
     ^                      ^
     |                      |
i2pr-crypto ---------------+

i2pr-daemon depends on runtime and transport contracts as composition root.
i2pr-testkit may depend on all transport crates for tests only.
```

Refine this graph if necessary, but preserve these invariants:

- no dependency from core/proto/crypto/storage into transport;
- no dependency from production crates into testkit;
- no Tokio dependency outside runtime and testkit;
- no dependency from transport crates into daemon;
- no cycle between transport-neutral and NTCP2-specific crates.

Update `scripts/check-dependency-direction.sh` and `scripts/check-runtime-boundaries.sh` to enforce the final graph.

## Transport-neutral type inventory

Implement only types with immediate Milestone 3 use.

### Transport identity

Define a closed transport kind enumeration with at least `Ntcp2`. Do not create a runtime plugin registry or stringly typed transport identifier.

Define bounded internal link identifiers that:

- are generated locally;
- are not derived directly from peer identity bytes;
- are safe for snapshots and tracing;
- distinguish successive links to the same peer;
- do not become public protocol identifiers.

Define link direction as inbound or outbound.

### Peer reference

Use an existing typed router hash or introduce a narrowly scoped `PeerId` wrapper around the correct existing public identity digest. The type must:

- avoid exposing full identity bytes in default `Debug`;
- support equality and map keys;
- provide an explicitly redacted or shortened operator representation only if necessary;
- not carry mutable peer profile state.

Do not create a universal identity abstraction shared with destinations unless the protocol requires it.

### Bounded I2NP transport payload

Define the unit delivered over an authenticated router link.

The plan must resolve and document whether the transport boundary carries:

1. canonical encoded I2NP message bytes in a validated bounded owner; or
2. a typed `I2npMessage` plus explicit encoding at the link boundary.

Preferred direction: a bounded owned encoded-message wrapper that preserves authenticated bytes and avoids repeated decode/re-encode. It must record or validate:

- nonzero length where required;
- maximum accepted I2NP wire size;
- ownership of the allocated bytes;
- redacted `Debug`;
- no implicit clone of large payloads;
- explicit consuming handoff.

If typed messages are used instead, record how canonical bytes and signed/authenticated regions remain stable.

### Delivery request and result

Define an outbound delivery request containing only:

- target peer;
- bounded encoded I2NP message owner;
- caller-visible deadline or monotonic expiry;
- optional bounded priority class only if current NTCP2 scheduling requires it;
- a one-shot response path owned by the runtime channel wrapper.

Define typed delivery outcomes such as:

- accepted by an authenticated link;
- no active link;
- dial scheduled or already pending;
- queue full;
- resource denied;
- deadline elapsed;
- cancelled;
- link replaced;
- link closed before write completion;
- protocol termination;
- peer identity mismatch.

Do not report raw remote errors or payload-dependent text.

### Link lifecycle

Define a finite link state model, for example:

```text
Candidate
Handshaking
Authenticated
Draining
Closing
Closed
Failed
```

The exact states may differ, but transitions must be explicit and tested. Authentication must be a one-way transition for a link instance. A failed or closed link cannot return to authenticated state.

Define typed termination categories with no arbitrary strings. Include categories needed later for local shutdown, remote termination, authentication failure, timeout, replay/skew rejection, malformed framing, queue/resource exhaustion, duplicate replacement, and I/O closure.

### Address and reachability observations

Define transport-neutral observation events rather than direct RouterInfo mutation. An observation may contain:

- transport kind;
- configured versus observed origin;
- address family category;
- reachability category;
- monotonic observation time;
- bounded confidence or validation state if concretely required.

Default snapshots and tracing must not contain raw addresses or ports. A separate typed operator inspection path may be designed later.

### Link snapshots

Define bounded, privacy-safe snapshots with:

- local link ID;
- transport kind;
- direction;
- lifecycle;
- authenticated/not authenticated;
- bounded queued message and byte counts;
- monotonic age or durations rounded/bounded as needed;
- typed last termination category;
- resource usage categories;
- no payload, peer address, key, transcript, identity, or dynamic error text.

Snapshots are observations, not authoritative ownership. Task and link ownership remains with the runtime/service manager.

## Transport manager contracts

Define narrow runtime-neutral contracts for:

- registering an authenticated link candidate;
- resolving a candidate against current peer links;
- obtaining a link delivery capability;
- reporting link closure and typed outcomes;
- recording dial/backoff state without performing time waits;
- emitting address/reachability observations;
- inspecting bounded snapshots.

Avoid async traits in this plan. Prefer commands, decisions, and state transitions that runtime-owned services can drive through existing bounded channels.

A candidate registration decision should be able to return:

- accept as first link;
- replace existing link;
- reject new duplicate;
- retain existing and drain new;
- reject because peer/global limit is reached;
- reject because identity/authentication evidence is incomplete.

The actual duplicate winner rule is deferred to Plan 035, but the decision surface must not preclude it.

## Resource governance

Review existing `ResourceClass` values before adding new ones. Reuse:

- `PendingHandshakes`
- `ActiveLinks`
- `BufferedBytes`
- queue item classes where appropriate

Add a class only when existing classes cannot express an independently bounded resource. Any new class must be:

- added to `ResourceClass::ALL`;
- covered by deterministic snapshot tests;
- documented in architecture/security guidance;
- included in the maximum class count;
- used by at least one concrete Milestone 3 path.

Define initial infrastructure ceilings in code or configuration types, but do not guess production defaults without evidence. Tests must exercise capacity one, exact limit, and limit-plus-one.

## NTCP2 crate skeleton

Create `i2pr-transport-ntcp2` with:

- crate-level scope documentation;
- `#![forbid(unsafe_code)]`;
- no Tokio or filesystem dependency;
- modules reserved for address, constants, crypto, handshake, frame, block, and state-machine ownership;
- only minimal public placeholder types needed by Plan 031 contracts;
- no false support claim.

Do not add empty APIs that imply completed handshake or frame behavior. Module placeholders may remain private or documentation-only until implemented.

## Testkit additions

Add only synthetic transport helpers needed to test contracts:

- deterministic local peer IDs;
- bounded encoded-I2NP payload factories with no live identity material;
- link candidate factories;
- state-transition test helpers;
- resource-budget fixtures;
- snapshot redaction assertions.

Do not open sockets or create interoperability containers in this plan.

## Required tests

### Type and bound tests

- invalid and maximum link identifier bounds;
- peer-reference debug redaction;
- zero, maximum, and maximum-plus-one payload sizes;
- no payload-bearing `Debug` output;
- lifecycle valid and invalid transitions;
- termination category mapping;
- deterministic snapshot ordering and entry caps.

### Manager decision tests

- first-link acceptance;
- candidate rejection at global/per-peer limit;
- duplicate decision surface supports accept/replace/reject outcomes;
- closure removes exactly one link owner;
- stale closure cannot remove a replacement link;
- delivery to no-link and closed-link states;
- bounded queue/resource denial outcomes;
- cancellation and deadline propagation remain typed.

### Resource tests

- capacity one, exact, and plus-one active links;
- pending-handshake lease release on all synthetic outcomes;
- buffered-byte lease release when delivery is rejected or dropped;
- no release-underflow signal on valid paths;
- snapshots return to zero after synthetic teardown.

### Boundary tests

- transport crates compile without Tokio features;
- production crates do not depend on testkit;
- no raw socket or Tokio imports in transport crates;
- no direct NetDB/tunnel/client dependency;
- default snapshots contain no peer/address/payload/key data.

## Documentation updates

Update at minimum:

- `README.md` current status;
- `AGENTS.md` transport ownership rules;
- `CONTRIBUTING.md` focused commands;
- `docs/architecture.md` crate graph and transport manager boundary;
- `docs/security-model.md` link exhaustion, identity correlation, queue pressure, and ownership threats;
- `docs/protocol-support.md` with non-advertised Plan 031 status;
- `specs/support.toml` only if adding a clearly scoped experimental contract surface;
- the new ADR;
- dependency and runtime boundary scripts.

## Required local commands

```text
cargo fmt --all --check
cargo check --workspace
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p i2pr-transport --all-targets
cargo test -p i2pr-transport-ntcp2 --all-targets
cargo test -p i2pr-testkit --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
cargo deny check advisories bans sources
cargo +1.85.0 check --workspace --all-targets
git diff --check
```

## Closure record

Create `plans/031-closure.md` with:

- final crate graph;
- ADR decision summary;
- public type inventory;
- payload ownership decision;
- lifecycle and delivery outcome table;
- resource classes and ceilings;
- tests and exact results;
- CI evidence or absence;
- support-ledger state;
- deviations;
- explicit Plan 032 prerequisites.

## Acceptance criteria

This plan closes only when:

- both transport crates exist with the intended non-Tokio boundaries;
- dependency direction is mechanically enforced;
- transport-neutral peer/link/delivery/lifecycle contracts are bounded and tested;
- encoded I2NP ownership is explicitly resolved;
- duplicate-resolution inputs and outputs are representable without embedding policy prematurely;
- resource admission and release are integrated with existing leases;
- synthetic teardown returns links, queues, bytes, and invariant counters to expected values;
- snapshots and diagnostics are privacy-safe;
- no handshake, encryption, frame, socket, NetDB, or capability behavior is falsely implemented or claimed;
- all required local gates pass;
- `plans/031-closure.md` exists.

## Stop conditions

Stop and record the conflict if:

- preserving Tokio confinement creates a dependency cycle;
- the transport boundary requires cloning large I2NP payloads by default;
- a proposed peer identifier exposes stable identity material in default diagnostics;
- transport-neutral contracts require knowledge of NetDB, tunnels, or client sessions;
- duplicate-link policy must be guessed before implementation evidence is collected;
- a generic abstraction grows beyond immediate NTCP2 and future SSU2 needs;
- resource leases cannot be held through exact queue/link ownership;
- the public API cannot remain narrow without exposing raw sockets or runtime channels.