# Plan 035: supervised TCP integration, transport link manager, and NTCP2 addresses

## Objective

Connect the pure NTCP2 handshake and data-phase implementations to supervised Tokio TCP services, bounded transport-neutral link management, deterministic dial/listen policy, duplicate-link resolution, address parsing/observation, and complete runtime cleanup.

This plan is the first Milestone 3 phase permitted to open TCP sockets, but only inside controlled local/private test scenarios. Public-network operation, reseeding, NetDB mutation, automatic address publication, tunnels, destinations, SAM, I2CP, and SSU2 remain excluded.

## Preconditions

- `plans/031-closure.md` defines the transport-neutral contracts.
- `plans/032-closure.md` validates cryptographic dependencies and transcript vectors.
- `plans/033-closure.md` validates complete pure handshake state machines.
- `plans/034-closure.md` validates pure authenticated data-phase framing and blocks.
- Plan 025 task ownership and forced-drain invariants remain enforced.

## Scope

Implement:

- NTCP2 RouterAddress option parsing and validation;
- configured listen-address types and resolved dial-target types;
- supervised inbound listener service;
- supervised outbound dial service;
- per-link manager with owned reader/writer child tasks;
- pure-state-machine runtime drivers;
- bounded handshake admission before expensive cryptography;
- per-IP/per-subnet and global pending-handshake limits;
- active-link and per-peer limits;
- outbound I2NP queues and byte budgets;
- read/write/handshake/idle deadlines;
- replay-cache runtime owner;
- dial backoff and cancellation;
- duplicate inbound/outbound link resolution;
- link replacement and draining;
- delivery outcomes and privacy-safe snapshots/events;
- address and reachability observations without direct RouterInfo mutation;
- deterministic simulated-link tests and controlled loopback TCP tests.

Do not implement:

- public listeners enabled by default;
- DNS discovery or reseeding;
- NetDB writes or RouterInfo publication;
- peer scoring beyond bounded transport backoff inputs;
- tunnel selection;
- NAT traversal, UPnP, NAT-PMP, or automatic external-address discovery;
- operator-facing production activation unless the final closure explicitly approves a disabled-by-default test mode.

## Runtime ownership

`i2pr-runtime` remains the sole owner of:

- `TcpListener` and `TcpStream`;
- Tokio tasks and child scopes;
- timeouts and clock reads;
- bounded Tokio channels;
- cancellation tokens;
- socket split halves;
- buffer resource leases;
- listener/dial/link service supervision.

Transport crates provide pure contracts/state machines. Do not add Tokio to `i2pr-transport` or `i2pr-transport-ntcp2`.

## Service graph

Define explicit services, likely including:

```text
transport-manager (essential or restartable)
  ├── ntcp2-listener (restartable/degradable according to configured policy)
  ├── ntcp2-dialer (restartable)
  └── replay-cache owner
```

Each accepted link must be owned by the transport manager or a bounded service child scope. Reader and writer work must be registered children, never detached tasks.

Document classification decisions and readiness behavior. A disabled listener may be a valid configured state; an unexpectedly failed required listener must not be silently treated as ready.

## NTCP2 RouterAddress handling

Implement strict typed parsing for current NTCP2 RouterAddress fields, including as applicable:

- transport style;
- host and port;
- static public key;
- IV/obfuscation material;
- version/capability options;
- IPv4/IPv6 representation;
- expiration and cost inherited from RouterAddress.

Requirements:

- exact base64/encoding and key lengths;
- valid port range;
- no hostname resolution inside pure parsing;
- distinguish configured literal addresses from resolved socket addresses;
- reject duplicate/conflicting options;
- preserve unknown options only if the specification permits;
- no automatic publication or reachability claim;
- default diagnostics redact raw host/port and stable key material.

Define a separate publication-candidate type for later RouterInfo policy; this plan emits observations only.

## Listener admission

Before expensive cryptographic work, enforce:

- global pending inbound handshake limit;
- per-IP limit;
- per-subnet limit with explicit IPv4/IPv6 prefix policy;
- active-link/global socket limit;
- buffered-byte budget;
- handshake deadline;
- optional accept-rate/token-bucket policy if evidence requires it.

Admission must be immediate grant or typed denial. Each accepted socket owns exact leases until handoff or cleanup.

Do not include raw IP addresses in default tracing or snapshots. Per-IP/subnet accounting keys remain internal and bounded.

## Outbound dial policy

Define typed dial requests containing:

- target peer;
- validated NTCP2 address candidate;
- expected peer/static key;
- caller deadline;
- reason/category;
- no arbitrary text.

Implement:

- one pending dial per peer/address candidate unless policy permits otherwise;
- global and per-peer pending dial limits;
- bounded exponential backoff with jitter from injected RNG;
- cancellation-aware connect and handshake deadlines;
- address candidate ordering without peer-scoring scope creep;
- typed outcomes for connect refusal, timeout, handshake failure, identity mismatch, replacement, and resource denial.

Backoff state must be bounded and expire. It must not grow from attacker-selected peer keys without admission limits.

## Runtime handshake driver

Drive Plan 033 actions over partial TCP I/O:

- exact reads for fixed fields;
- bounded reads for variable/padding fields;
- complete writes with cancellation and deadlines;
- no assumption that reads align with handshake messages;
- no holding resource locks across await;
- no blocking filesystem or DNS operations;
- typed mapping of I/O errors without retaining OS text by default;
- zeroization/drop of handshake secrets on every failure path.

After authentication, transfer data-phase key owners and peer identity exactly once into a link owner.

## Runtime data-phase driver

Create owned reader and writer children.

Reader responsibilities:

- read/deobfuscate bounded frame lengths;
- acquire buffer leases before ciphertext allocation/read;
- authenticate/decode frames using Plan 034;
- hand off I2NP messages through bounded channels;
- process control blocks and remote termination;
- apply read/idle deadlines;
- close/cancel the link on protocol failure.

Writer responsibilities:

- receive bounded outbound requests;
- preserve caller deadlines;
- assemble/coalesce frames within explicit bounded scheduling policy;
- apply padding through injected production policy;
- write all bytes with deadline/cancellation;
- return typed delivery outcomes;
- release message/frame leases on success or failure.

The link owner coordinates cancellation. Reader/writer failure must cancel the sibling and join both before closure is reported.

## Link manager and duplicate resolution

Implement the policy selected from Java I2P/i2pd evidence.

The rule must consider at least:

- authenticated peer identity;
- inbound versus outbound direction;
- local/remote router hash ordering if specified;
- establishment time/state;
- existing link health;
- simultaneous connection race;
- replacement/drain behavior.

Requirements:

- deterministic winner for identical inputs;
- stale closure cannot remove a replacement link;
- one accepted active link per peer unless the specification/deployed behavior requires more;
- loser receives bounded termination/drain behavior;
- no repeated churn loop;
- replacement transfers or rejects queued messages explicitly;
- duplicate candidate leases release correctly.

Record cross-implementation evidence before finalizing the rule.

## Replay cache runtime owner

Implement a bounded owner for replay tokens from Plan 033:

- fixed maximum entries;
- deterministic expiry ordering;
- monotonic retention timing;
- capacity and eviction/fail-closed policy;
- no token bytes in diagnostics;
- cleanup on shutdown;
- capacity one/exact/plus-one tests;
- shared safely across inbound handshakes without holding locks across crypto or I/O awaits.

## Queues and delivery

Use existing bounded channel wrappers.

Define capacities and resource charges for:

- manager commands;
- outbound messages per link;
- inbound authenticated I2NP delivery;
- address observations;
- delivery outcomes;
- latest-state link snapshots.

Every command/request send requires deadline and cancellation. Event queues use explicit drop policy. No raw Tokio sender escapes reviewed runtime ownership.

Delivery semantics must distinguish:

- accepted into queue;
- written to socket;
- link closed before write;
- replaced/drained;
- deadline/cancelled;
- resource denied;
- protocol termination.

Do not claim end-to-end I2NP delivery beyond authenticated transport write.

## Address and reachability observations

Emit typed observations such as:

- configured listener bound;
- inbound connection observed;
- outbound address succeeded/failed;
- address family;
- reachability category;
- monotonic time;
- bounded confidence/state.

No direct RouterInfo mutation. No automatic external address inference from one peer. Raw addresses remain available only through explicit opt-in operator diagnostics outside default snapshots/tracing.

## Deadlines

Define and test bounded configuration for:

- TCP connect;
- handshake total and per-stage;
- read idle;
- write;
- outbound queue wait;
- graceful link drain;
- listener restart backoff;
- duplicate-candidate resolution.

All durations must be nonzero, capped, and configuration validated. Tests use paused/manual time where possible.

## Resource limits

Define initial explicit ceilings for:

- pending inbound/outbound handshakes;
- per-IP/per-subnet pending inbound handshakes;
- active links global/per-peer;
- open sockets;
- reader/writer child tasks;
- per-link outbound queue items/bytes;
- global transport buffered bytes;
- replay-cache entries;
- backoff records;
- address observations;
- concurrent graceful drains.

Use existing resource classes where possible. Add classes only with documentation/tests/mechanical count updates.

## Observability

Add fixed events for:

- listener start/stop/failure;
- dial admitted/denied/completed;
- handshake stage/typed failure;
- link authenticated/replaced/draining/closed;
- queue/resource denial;
- replay/skew rejection;
- delivery typed outcome.

Default fields must omit raw addresses, ports, peer hashes, public keys, transcript bytes, frame sizes precise enough for per-peer history where avoidable, and all payload/secret data.

Use local synthetic link IDs and bounded aggregate counters.

## Testing

### Deterministic simulated-link tests

- inbound and outbound complete handshake/data exchange;
- partial read/write at every boundary;
- slowloris reads and stalled writes;
- queue saturation and byte-budget denial;
- cancellation at connect, each handshake stage, authenticated idle, queued write, partial frame, and drain;
- reset/disconnect and sibling-task cancellation;
- replay/skew rejection;
- duplicate simultaneous links in both directions;
- replacement with queued messages;
- link and resource cleanup under normal/failure/forced paths;
- same-seed report reproducibility.

### Loopback TCP tests

Use only loopback or isolated namespace/container networking:

- listener bind/accept/shutdown;
- outbound connect;
- one-byte fragmented writes;
- socket half-close;
- abrupt reset where supported;
- multiple concurrent candidates within limits;
- listener restart behavior;
- IPv4 and IPv6 loopback where CI supports it.

Tests must not contact external addresses.

### Ownership tests

- reader/writer are owned child tasks;
- forced service shutdown joins link children;
- no task counter reaches zero early;
- socket/link/queue/buffer/replay/backoff leases return to expected values;
- release-underflow remains zero;
- stale close cannot remove replacement;
- no detached spawn is introduced.

## Interoperability preparation

Add reproducible private-testnet harness configuration but do not yet claim final interop closure.

The harness should support:

- Java I2P and i2pd container/process versions pinned;
- deterministic/private addresses;
- generated test identities and NTCP2 keys;
- inbound and outbound role selection;
- sanitized trace/event capture;
- timeout and teardown;
- no public bootstrap/reseed;
- artifact manifest with versions/configuration.

Actual required mixed-router evidence is completed in Plan 036.

## Documentation and support metadata

Update:

- `docs/architecture.md` with services, tasks, queues, and link ownership;
- `docs/security-model.md` with socket exhaustion, slowloris, address privacy, duplicate churn, replay-cache, queue, and shutdown threats;
- ADRs for duplicate resolution, admission limits, IPv4/IPv6 scope, and padding/coalescing runtime policy;
- `specs/protocols/03-ntcp2.md` with runtime policy decisions;
- `specs/support.toml` and `docs/protocol-support.md` with non-advertised experimental runtime surfaces;
- `AGENTS.md`, `CONTRIBUTING.md`, and boundary scripts;
- private-testnet harness documentation.

## Required commands

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p i2pr-runtime --all-targets
cargo test -p i2pr-transport --all-targets
cargo test -p i2pr-transport-ntcp2 --all-targets
cargo test -p i2pr-testkit --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
cargo deny check advisories bans sources
cargo +1.85.0 check --workspace --all-targets
cargo +nightly check --manifest-path fuzz/Cargo.toml --offline --all-targets
CARGO_NET_OFFLINE=true bash scripts/fuzz-smoke.sh
git diff --check
```

Add a mechanical no-external-network test/check where practical.

## Closure record

Create `plans/035-closure.md` containing:

- service graph and task ownership;
- address parsing/publication boundary;
- admission/backoff/replay/duplicate policies;
- queue/resource/deadline tables;
- runtime driver API inventory;
- simulated and loopback test matrix;
- cleanup evidence;
- private-testnet harness status;
- exact local/CI results;
- support-ledger state;
- explicit Plan 036 prerequisites.

## Acceptance criteria

Plan 035 closes only when:

- live TCP is confined to supervised runtime services;
- pure handshake/data state machines are driven correctly under partial I/O;
- inbound/outbound admission, deadlines, replay cache, backoff, queues, and duplicate resolution are bounded;
- reader/writer children are joined on every path;
- link replacement is deterministic and stale-safe;
- address observations do not mutate RouterInfo/NetDB;
- simulated and loopback tests return tasks, sockets, queues, buffers, replay entries, and leases to expected bounds;
- no public-network traffic or capability advertisement occurs;
- CI, MSRV, dependency policy, vectors, docs, and fuzz compilation pass;
- `plans/035-closure.md` exists.

## Stop conditions

Stop and record the issue if:

- correct runtime integration requires Tokio in transport crates;
- partial I/O requires unbounded buffering;
- duplicate-link behavior causes reproducible churn against reference implementations;
- per-IP/subnet policy cannot support IPv6 safely;
- cleanup cannot prove child/socket termination;
- address publication policy leaks into transport runtime;
- tests require public network access;
- queue ownership requires cloning large payloads;
- a socket or task must be detached.