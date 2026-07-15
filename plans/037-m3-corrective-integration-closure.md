# Plan 037: Milestone 3 corrective integration and closure

## Objective

Correct the remaining lifecycle, resource-accounting, wire-conformance, and runtime-composition defects in the current NTCP2 implementation, then complete the bounded socket-to-state-machine adapter required for controlled Java I2P and i2pd interoperability.

This plan does not begin Milestone 4. It exists to bring Milestone 3 back to its stated acceptance criteria without weakening the architecture established in Plans 031–036.

Milestone 3 remains blocked until this plan is complete and a fresh aggregate closure record proves:

- the runtime owns and drives the complete initiator and responder handshake;
- authenticated data-phase frames cross supervised sockets in both directions;
- inbound admission remains held through expensive handshake work;
- configured deadlines and resource bounds are enforced by the actual I/O paths;
- queue and link accounting returns to zero on success, failure, timeout, cancellation, and forced teardown;
- data-phase block parsing accepts all specification-permitted sequences and rejects only invalid sequences;
- Java I2P and i2pd interoperability evidence exists for both directions in an authorized isolated testnet.

## Controlling evidence

Use the following as the controlling baseline:

- `plans/030-milestone-3-overview.md`
- `plans/030-milestone-3-closure.md`
- `plans/031-closure.md`
- `plans/032-closure.md`
- `plans/033-closure.md`
- `plans/034-closure.md`
- `plans/035-closure.md`
- `plans/036-closure.md`
- `specs/protocols/03-ntcp2.md`
- `specs/SOURCES.md`
- `docs/architecture.md`
- `docs/security-model.md`
- `docs/private-testnet.md`
- `GUARDRAILS.md`

The current aggregate closure is intentionally blocked. Do not revise it to “complete” until this plan’s local and mixed-router gates are satisfied.

## Scope

This plan may modify:

- `crates/i2pr-runtime/src/ntcp2_runtime.rs`;
- `crates/i2pr-runtime/src/lib.rs` and observability support;
- `crates/i2pr-transport/src/manager.rs`, resource owners, and snapshots;
- `crates/i2pr-transport-ntcp2/src/block.rs`, `frame.rs`, `handshake.rs`, and `state_machine.rs`;
- `crates/i2pr-testkit/src/ntcp2.rs` and Milestone 3 integration tests;
- the NTCP2 private-testnet harness and evidence format;
- CI and mechanical policy checks;
- ADRs, architecture/security documentation, support metadata, and closure records.

This plan must not add:

- NetDB mutation or RouterInfo publication;
- reseeding;
- tunnel construction;
- client APIs, SAM, I2CP, or service tunnels;
- SSU2;
- public-network testing;
- automatic NAT traversal;
- capability advertisement;
- Tokio or socket ownership outside `i2pr-runtime`;
- arbitrary remote error text or payload logging.

## Corrective track A: inbound admission ownership

### Current defect

`InboundChunk` owns an `InboundPermit`, but `into_stream()` drops the permit before the stream is transferred into handshake processing. The configured global, exact-IP, and subnet pending-handshake limits therefore cease to apply before expensive cryptographic work begins.

### Required design

Replace the current stream-only transfer with an ownership-preserving handoff.

Acceptable forms include:

```text
AdmittedInboundStream {
    stream,
    permit,
    family,
}
```

or a consuming handshake owner that receives the stream and permit together.

Required properties:

- the permit is non-cloneable;
- the permit remains held from accept admission until one of:
  - successful authenticated-link registration;
  - typed handshake failure;
  - handshake timeout;
  - cancellation;
  - peer disconnect;
  - runtime teardown;
- successful authentication may consume or replace the pending-handshake permit with an active-link resource lease;
- the handoff must not briefly release admission between pending and active ownership;
- drop during any intermediate state releases exactly once;
- snapshots remain privacy-safe and expose only aggregate counts.

### Required tests

- exact global, per-IP, and per-subnet capacity during active cryptographic handshakes;
- rapid accept-to-consumer handoff cannot exceed the configured limit;
- timeout releases one permit;
- malformed SessionRequest releases one permit;
- cancellation releases one permit;
- successful authentication transitions pending count down and active-link count up without a gap visible to deterministic snapshots;
- panic/unwind in the adapter does not leak or double-release a permit;
- 100-iteration deterministic admission/teardown repetition.

## Corrective track B: active-link and transport resource ownership

### Current defect

Runtime configuration declares active-link, queue-item, queue-byte, replay, and backoff bounds, but several are not connected to one exact end-to-end owner.

### Required design

Unify runtime link creation with the existing transport resource and manager contracts.

The runtime must not create an authenticated `LinkHandle` unless it has:

- complete authenticated peer evidence;
- a successful transport-manager candidate decision;
- an exact active-link lease;
- a bounded writer queue owner;
- reader and writer child ownership;
- a local link ID that matches the transport-manager record.

Required ownership sequence:

```text
pending inbound/outbound attempt
  -> authenticated candidate
  -> duplicate/resource decision
  -> active-link lease + manager registration
  -> supervised reader/writer owner
  -> close/drain
  -> manager removal + lease release
```

No step may create a live socket task without the corresponding accounting owner.

### Required tests

- global active-link exact limit and plus-one denial;
- per-peer link limit and duplicate candidate handling;
- stale close cannot remove a replacement link;
- duplicate loser drains and releases all leases;
- manager registration failure closes the socket and releases the pending permit;
- active-link snapshot count equals actual registered runtime links;
- all active-link and queue resource usage returns to zero after teardown;
- no release-underflow signal on valid paths.

## Corrective track C: per-operation deadlines and cancellation

### Current defect

The runtime exposes bounded `read_exact` and `write_all_exact` helpers, but the link reader and writer children use unrestricted `read()` and `write_all()` calls. Configured `read_idle` and `write` deadlines are therefore not enforced by the actual link tasks.

### Required design

The complete adapter must use explicit deadlines for every blocking transport operation:

- TCP connect;
- handshake total duration;
- each exact handshake field read/write;
- data-frame length read;
- data-frame ciphertext read;
- frame write;
- queue admission;
- duplicate-link drain;
- idle read policy;
- orderly termination write where attempted.

Deadline behavior must be typed and stage-aware. Do not retain operating-system error strings.

The runtime may implement reusable internal I/O drivers, but all deadline calculation remains in `i2pr-runtime`. The protocol crate continues to receive complete bounded inputs and return typed actions.

Required rules:

- one absolute handshake deadline bounds the complete three-message exchange;
- per-read operations cannot extend the handshake deadline;
- idle reads use the configured read-idle bound;
- writes use the configured write bound, including queued frames;
- cancellation wins promptly over pending reads, writes, queue waits, and drains;
- zero remaining duration fails immediately;
- deadline failures close both halves and release every owner;
- partial progress does not reset a total handshake deadline unless explicitly required by the specification and recorded.

### Required tests

- stalled SessionRequest, SessionCreated, and SessionConfirmed at every field boundary;
- one-byte-progress slowloris still hits the total handshake deadline;
- stalled frame-length read;
- stalled ciphertext read;
- stalled writer;
- queue-wait timeout;
- cancellation racing each I/O stage;
- deadline versus disconnect classification;
- paused-time tests only; no wall-clock sleeps.

## Corrective track D: queue accounting and RAII

### Current defect

The current writer decrements queued item/byte counters only after a successful write. Write failure, cancellation, receiver drop, and link shutdown can leave nonzero counters.

### Required design

Replace manual counter increments/decrements with an exact queue-entry owner.

A queued item should contain:

- the bounded encoded frame or transport payload owner;
- an item-count lease;
- a byte-count lease;
- optional delivery completion sender;
- no cloneable large payload by default.

The queue-entry owner releases its leases on every drop path. The writer may consume the entry only once.

Required invariants:

- failed queue admission acquires no lasting lease;
- accepted queue admission owns exactly one item and byte lease;
- successful write releases both leases;
- write timeout releases both leases;
- write error releases both leases;
- link cancellation drains or drops queued entries and releases all leases;
- writer task panic/unwind releases retained entries;
- sender/receiver closure releases all queued ownership;
- snapshot counters derive from authoritative leases or exact queue owners, not independent best-effort atomics.

### Required tests

- queue capacity one, exact limit, plus-one denial;
- byte limit exact and plus-one denial;
- first queued write fails while later items remain;
- cancellation with multiple queued items;
- receiver dropped before write;
- writer child failure;
- forced supervisor shutdown;
- 100-iteration queue teardown repetition;
- final item and byte usage equals zero and underflow remains zero.

## Corrective track E: data-phase block conformance

### Current defect

The general data-phase implementation currently applies stricter rules than the NTCP2 specification by rejecting duplicate non-padding block types and by requiring Termination to be the first non-padding block. These rules risk rejecting valid Java I2P or i2pd traffic.

### Required source reconciliation

Re-read the current pinned NTCP2 specification and compare Java I2P and i2pd behavior for:

- block ordering;
- repeated DateTime, Options, RouterInfo, I2NP, and unknown blocks;
- Termination placement;
- Padding placement and multiplicity;
- behavior after Termination;
- unknown experimental/future block handling;
- RouterInfo flood flag behavior;
- zero-length blocks where permitted.

Record the result in an ADR or an amendment to ADR 0013.

### Required parser separation

Maintain separate rule sets for:

1. SessionConfirmed part-two payload, which remains structurally strict according to the handshake specification.
2. General data-phase frames, which must accept every specification-permitted block sequence.

Do not reuse one stricter parser mode for both contexts unless the official specification proves the rules identical.

### Expected data-phase policy

Unless contradicted by the pinned source:

- Padding may appear at most once and must be last;
- Termination must be the last non-padding block;
- multiple I2NP blocks are permitted;
- multiple other non-padding block types are not rejected merely because they repeat;
- blocks after Termination are limited to permitted Padding;
- unknown blocks remain authenticated, bounded, and skipped according to the allowed type policy;
- no parser path reads beyond the authenticated plaintext boundary.

### Required tests

Add fixed positive and malformed vectors for:

- repeated timestamp blocks;
- repeated options blocks;
- repeated RouterInfo blocks if permitted;
- multiple I2NP blocks;
- Termination after earlier valid blocks;
- Padding after Termination;
- invalid block after Termination;
- duplicate Padding;
- non-final Padding;
- mixed unknown/known sequences;
- Java I2P-produced and i2pd-produced plaintext/block sequences once available.

Every changed rule must have a fixture or exact byte test, not only constructed Rust values.

## Corrective track F: complete socket-to-state-machine adapter

### Objective

Build the missing production-shaped but still non-advertised adapter that composes:

- Plan 033 handshake states;
- Plan 034 transmit/receive data states;
- Plan 035 socket, admission, replay, backoff, duplicate, and supervised task owners;
- transport-neutral manager and delivery contracts.

### Architecture

Add narrowly scoped runtime-owned connection drivers, for example:

```text
InboundHandshakeDriver
OutboundHandshakeDriver
AuthenticatedNtcp2Link
Ntcp2ReaderDriver
Ntcp2WriterDriver
```

Names may differ, but ownership must remain explicit.

The drivers must:

- receive protocol state by value;
- perform only the requested bounded I/O action;
- feed complete results back into consuming state transitions;
- hold cancellation, deadlines, admission, replay, and resource owners;
- never expose raw sockets to lower crates;
- never move cryptographic protocol logic into runtime;
- never create a task without a retained `ChildScope` owner.

### Inbound handshake flow

At minimum:

1. Accept socket and pending permit.
2. Read exactly the SessionRequest minimum/fixed fields needed to determine bounded remaining bytes.
3. Reject declared lengths before allocation.
4. Drive responder state with injected current time and replay decision.
5. Write SessionCreated under the total handshake deadline.
6. Read SessionConfirmed parts under the same deadline.
7. Verify RouterInfo, peer identity, transport static key, network, skew, and authentication.
8. Resolve duplicate/resource policy.
9. Atomically transition pending ownership to active-link ownership.
10. Spawn authenticated reader/writer children.

### Outbound handshake flow

At minimum:

1. Consult dial backoff before connecting.
2. Acquire pending outbound/handshake resources.
3. Connect under deadline.
4. Construct initiator state using validated target RouterInfo/static key.
5. Write SessionRequest.
6. Read and validate SessionCreated.
7. Write SessionConfirmed.
8. Register the authenticated candidate through duplicate/resource policy.
9. Clear backoff on success; record bounded failure on typed failure.
10. Spawn authenticated reader/writer children.

### Authenticated reader

The reader must:

- read exactly two obfuscated length bytes;
- decode and validate length before allocation;
- acquire bounded buffer/byte resources;
- read exactly the ciphertext under idle/deadline policy;
- authenticate before parsing;
- parse general data-phase blocks;
- deliver complete I2NP owners through bounded transport channels;
- handle RouterInfo/address observations without direct NetDB mutation;
- process Termination and EOF as typed closure;
- release frame buffers and leases on every path.

### Authenticated writer

The writer must:

- receive typed bounded outbound delivery entries;
- compose required I2NP/control/padding blocks;
- apply a documented compliant coalescing and padding policy;
- seal exactly one frame with consuming counter progression;
- write under the configured deadline;
- report typed delivery outcomes;
- release queue and byte leases on every path;
- attempt orderly Termination only when it can be done safely within the shutdown budget;
- never reuse a terminal cipher state.

### Required integration tests

Use deterministic local sockets and testkit streams to cover:

- complete initiator/responder self-handshake through the runtime adapter;
- authenticated I2NP exchange in both directions;
- one-byte partial reads/writes across every handshake and frame boundary;
- malformed request/created/confirmed inputs;
- wrong target identity/static key/network;
- replay and clock skew;
- frame tag mutation and oversized declaration;
- multiple data frames;
- queue saturation and backpressure;
- duplicate simultaneous inbound/outbound links;
- graceful and forced shutdown;
- zero final task, queue, buffer, pending-handshake, active-link, and replay usage where expected.

A pure self-handshake remains local evidence only. It is required before the external matrix but does not close Milestone 3.

## Corrective track G: dial backoff and replay composition

### Dial backoff

`Ntcp2RuntimeService::dial()` must not bypass `DialAdmission`.

Required behavior:

- consult backoff before connect;
- distinguish wait, exhausted, resource denied, cancelled, deadline, and socket failure;
- record only policy-approved failures;
- clear on authenticated success, not merely TCP connect;
- bound the entry count;
- avoid peer/address values in diagnostics;
- deterministic paused-time tests for expiration and capped growth.

### Replay cache

The responder adapter must call the runtime replay cache at the exact protocol stage required by the handshake state.

Required behavior:

- token derivation remains in the protocol crate;
- cache storage and monotonic expiry remain in runtime;
- full/poisoned/unavailable cache fails closed;
- replay entries are retained for the configured policy interval;
- teardown may clear test-owned caches, but normal link close must not erase replay protection prematurely;
- snapshots reveal only count and capacity.

## Corrective track H: observability and failure taxonomy

Add fixed categories sufficient to distinguish:

- inbound admission denial;
- handshake stage timeout;
- replay rejection;
- skew rejection;
- peer identity/static-key/network mismatch;
- malformed handshake;
- authentication failure;
- frame length/tag/block failure;
- duplicate reject/replace/drain;
- queue/resource denial;
- read/write/idle deadline;
- orderly remote termination;
- local cancellation;
- cleanup failure.

Do not add:

- raw IPs or ports to default events;
- RouterIdentity or static-key bytes;
- transcript hashes or replay tokens;
- ciphertext/plaintext;
- I2NP message contents;
- remote additional termination text;
- dynamic peer-derived metric labels.

Update snapshot tests and debug redaction tests for every new public type.

## Corrective track I: private-testnet interoperability execution

The repository-side manifest and evidence boundary already exist. Extend them only as needed to execute the real matrix.

### Required reference targets

Use the pinned versions already recorded unless a newer source revision is deliberately adopted and documented:

- Java I2P 2.12.0 / pinned revision;
- i2pd 2.60.0 / pinned revision.

Any pin change requires:

- exact version and source revision;
- binary/image hash;
- reason for the change;
- synchronized manifest and closure updates.

### Required scenarios

For each reference implementation:

- i2pr outbound initiator to reference responder;
- reference outbound initiator to i2pr responder;
- IPv4;
- IPv6 where supported by the controlled environment;
- authenticated I2NP exchange in both directions;
- minimum and maximum accepted padding samples;
- stale/future timestamp rejection;
- replay rejection;
- wrong identity/static key/network rejection;
- malformed and oversized handshake fields;
- slow read and slow write behavior;
- frame tag/length/block failures;
- queue/resource saturation;
- simultaneous duplicate inbound/outbound race;
- orderly close and abrupt disconnect;
- zero/expected cleanup counters after every scenario.

### Evidence format

Retain only sanitized evidence:

- reference name/version/revision;
- binary or image hash;
- configuration hash;
- i2pr commit SHA;
- scenario ID;
- direction and address family category;
- fixed typed outcome;
- bounded duration/counter summary;
- artifact hash;
- exact reproduction command;
- CI/manual run identifier.

Do not retain private keys, identities, raw peer lists, packet captures, payloads, full logs, or operational endpoints.

### Failure handling

If Java I2P and i2pd disagree:

- preserve both results;
- identify whether the official specification resolves the difference;
- prefer official specification plus dominant deployed behavior only after explicit review;
- add a compatibility policy rather than silently special-casing one implementation;
- reopen the relevant Plan 032–035 closure if transcript, frame, block, duplicate, padding, or address behavior is wrong.

## CI and mechanical gates

Add or strengthen scripts so CI verifies:

- no Tokio/socket dependency outside runtime/testkit;
- no raw unbounded channels;
- no runtime NTCP2 reader/writer path using unrestricted socket I/O without an approved deadline wrapper;
- no stream handoff that drops an inbound permit before handshake completion;
- no writer queue item without RAII accounting ownership;
- NTCP2 vector manifest completeness;
- interoperability evidence manifest consistency;
- support rows remain non-advertised until evidence exists.

A static grep check may supplement tests but must not replace ownership tests.

## Documentation updates

Update at minimum:

- `README.md`;
- `AGENTS.md`;
- `CONTRIBUTING.md`;
- `docs/architecture.md`;
- `docs/security-model.md`;
- `docs/private-testnet.md`;
- `docs/protocol-support.md`;
- `specs/protocols/03-ntcp2.md`;
- `specs/CONFORMANCE.md`;
- `specs/support.toml`;
- ADR 0013 and/or a new corrective integration ADR;
- `plans/030-milestone-3-closure.md` only after all gates pass.

Documentation must clearly distinguish:

- local structural evidence;
- runtime self-integration evidence;
- Java I2P evidence;
- i2pd evidence;
- advertised support state.

## Required test matrix

### Focused deterministic tests

- transport-manager candidate and lease transitions;
- inbound permit lifetime;
- queue RAII release;
- deadline behavior at each stage;
- data-phase block conformance;
- dial backoff integration;
- replay-cache integration;
- duplicate-link winner/drain behavior;
- socket-to-state-machine self-handshake;
- authenticated I2NP exchange;
- full cleanup snapshots.

### Repetition

Run at least:

- 100 forced/cancelled handshake cleanup iterations;
- 100 queue failure/drain iterations;
- 100 duplicate-link race iterations;
- deterministic scheduler seeds `0..=255` for the complete local adapter;
- focused fuzz campaigns for handshake, frame, block, and transcript targets.

### Fuzzing

Retain the existing targets and add a bounded runtime-adapter command/state target if it can remain socket-free and deterministic.

At minimum run:

```text
cargo +nightly check --manifest-path fuzz/Cargo.toml --offline --all-targets
CARGO_NET_OFFLINE=true bash scripts/fuzz-smoke.sh
cargo fuzz run --fuzz-dir fuzz ntcp2_handshake -- -runs=1000 -seed=<recorded>
cargo fuzz run --fuzz-dir fuzz ntcp2_frames -- -runs=1000 -seed=<recorded>
cargo fuzz run --fuzz-dir fuzz ntcp2_blocks -- -runs=1000 -seed=<recorded>
cargo fuzz run --fuzz-dir fuzz ntcp2_transcript -- -runs=1000 -seed=<recorded>
```

Record sanitizer/environment deviations exactly.

## Required local validation

Run and record:

```text
cargo fmt --all --check
cargo check --workspace
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p i2pr-core --all-targets
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
bash scripts/check-ntcp2-interoperability.sh
cargo deny check advisories bans sources
cargo +1.85.0 check --workspace --all-targets
cargo +nightly check --manifest-path fuzz/Cargo.toml --offline --all-targets
CARGO_NET_OFFLINE=true bash scripts/fuzz-smoke.sh
git diff --check
```

Do not claim remote CI evidence unless a fresh post-push run is identified and every required job passes.

## Closure records

Create:

```text
plans/037-closure.md
```

Then update:

```text
plans/030-milestone-3-closure.md
```

The Plan 037 closure must include:

- implementation commits;
- exact changed files;
- corrected ownership diagrams;
- inbound permit lifetime table;
- active-link and queue lease inventory;
- deadline table by I/O stage;
- general data-phase block conformance table;
- public API changes;
- secret-owner changes or explicit absence;
- test counts and exact commands;
- deterministic repetition results;
- fuzz results;
- Java I2P and i2pd matrix results;
- sanitized evidence hashes and run IDs;
- CI evidence;
- support-ledger state;
- unresolved deviations;
- explicit Milestone 4 readiness decision.

## Acceptance criteria

Plan 037 closes only when all conditions below are true:

- inbound pending-handshake permits remain owned until authentication or terminal failure;
- pending ownership transitions atomically to active-link ownership;
- active-link limits are enforced by the actual runtime path;
- every socket read/write is bounded by cancellation and the correct configured deadline;
- writer queue items and bytes use exact RAII ownership and return to zero on every path;
- general data-phase block rules match the pinned specification and deployed reference behavior;
- the runtime composes the pure handshake and data-phase owners into a complete NTCP2 adapter;
- self-handshake and authenticated I2NP exchange pass through actual runtime-owned sockets;
- dial backoff, replay cache, duplicate policy, and transport manager are exercised end to end;
- all normal, malformed, timeout, cancellation, disconnect, saturation, and forced-shutdown paths release tasks, buffers, queues, permits, leases, and replay entries as expected;
- Java I2P and i2pd complete inbound and outbound authenticated NTCP2 handshakes in the authorized private testnet;
- bounded I2NP messages cross in both directions with both implementations;
- duplicate-link races do not churn;
- malformed/adversarial scenarios terminate within bounds;
- sanitized evidence and exact reproduction commands are committed;
- all support rows remain truthful and are advanced only to the level justified by evidence;
- local, MSRV, dependency-policy, fuzz, and fresh remote CI gates pass;
- `plans/037-closure.md` exists;
- `plans/030-milestone-3-closure.md` is updated from blocked to complete only if every Milestone 3 criterion is met.

## Stop conditions

Stop and record the issue rather than improvising if:

- Java I2P and i2pd disagree on transcript, frame, block, duplicate, or padding behavior and the specification does not resolve it;
- correct permit-to-active-link transition cannot be expressed without a resource ownership gap;
- deadline enforcement requires moving async ownership into the protocol crate;
- queue cleanup cannot be made exact without redesigning the delivery owner;
- the official data-phase block rules differ from the current pinned dossier;
- a complete adapter requires NetDB mutation or RouterInfo publication;
- interoperability requires public-network access;
- reference binaries or configurations cannot be reproduced and hashed;
- a failure can only be diagnosed by retaining secrets, payloads, identities, or raw peer addresses;
- the MSRV cannot support a required dependency without an approved project-wide change;
- any support claim would exceed the available mixed-router evidence.

Milestone 4 planning and implementation remain blocked until the updated aggregate Milestone 3 closure explicitly records successful Java I2P and i2pd interoperability and zero/expected cleanup evidence.