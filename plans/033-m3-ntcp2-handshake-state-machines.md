# Plan 033: NTCP2 initiator and responder handshake state machines

## Objective

Implement complete, runtime-neutral NTCP2 initiator and responder handshake state machines over the cryptographic foundation established by Plan 032.

The state machines must parse and produce SessionRequest, SessionCreated, and SessionConfirmed exactly as specified; bind the authenticated peer identity and expected transport static key; enforce replay, skew, padding, and message-size policy; and expose explicit bounded I/O actions for a later runtime adapter.

This plan does not add live TCP sockets, listeners, dial policy, duplicate-link resolution, data-phase frame processing, RouterInfo publication, or public-network operation.

## Preconditions

- `plans/031-closure.md` confirms transport contracts and crate boundaries.
- `plans/032-closure.md` confirms dependencies, transcript/KDF behavior, static-key persistence, and independent fixed vectors.
- Every handshake cryptographic stage needed here has exact deterministic evidence.
- `i2pr-transport-ntcp2` remains Tokio-free and filesystem-free.

## Scope

Implement:

- explicit initiator and responder state types;
- exact SessionRequest parsing and encoding;
- exact SessionCreated parsing and encoding;
- exact SessionConfirmed parsing and encoding;
- handshake options and padding validation;
- timestamp extraction and typed skew decisions;
- replay-token extraction and replay-decision interfaces;
- RouterInfo payload parsing/validation needed for authenticated peer binding;
- expected-peer/static-key validation for outbound handshakes;
- authenticated handshake result containing role, peer identity, negotiated parameters, and data-phase key owners;
- runtime-neutral actions for reading exact bounded lengths, writing owned bounded bytes, obtaining time/policy decisions, and reporting typed termination;
- deterministic simulated-I/O tests.

Do not implement:

- Tokio `TcpStream` integration;
- data-frame block processing;
- link replacement or backoff;
- NetDB mutation or RouterInfo publication;
- address discovery;
- capability advertisement.

## State-machine architecture

Use consuming states or an equivalent type-safe model. A representative structure is:

```text
InitiatorStart
  -> AwaitSessionCreated
  -> ProduceSessionConfirmed
  -> AuthenticatedInitiator

ResponderStart
  -> AwaitSessionRequest
  -> ProduceSessionCreated
  -> AwaitSessionConfirmed
  -> AuthenticatedResponder
```

Each transition must:

- accept one bounded input or policy result;
- return the next state plus explicit actions;
- reject invalid transitions with typed errors;
- consume secret-bearing prior state;
- prevent message retransmission or state reuse unless the protocol explicitly permits it;
- expose no direct async, socket, clock, RNG, filesystem, NetDB, or tracing-subscriber behavior.

Do not implement one mutable “handshake” object with all methods valid at all times.

## Runtime-neutral action model

Define a narrow action/result vocabulary sufficient for the runtime adapter:

- write exact owned handshake bytes;
- read exact fixed/bounded bytes;
- request current monotonic/wall-clock timestamp through an injected policy interface;
- request replay-cache decision using a typed bounded replay token;
- request padding bytes from an injected policy/RNG source;
- request local RouterInfo bytes from a bounded authenticated source;
- report authenticated result;
- report typed termination.

The protocol state machine must never wait. It returns actions; the runtime fulfills them and feeds results back.

Avoid a generalized effect system. Model only immediate NTCP2 needs.

## Handshake message layouts

Document exact byte layouts and implement strict parsers/encoders for all three messages.

For each message record:

- fixed and variable fields;
- minimum and maximum total lengths;
- cleartext, obfuscated, encrypted, authenticated, and padding regions;
- transcript inclusion order;
- length and padding fields;
- timestamp location and units;
- RouterInfo payload boundaries;
- permitted optional fields;
- exact rejection categories.

Parsers must:

- reject trailing bytes unless the specification permits them;
- reject impossible lengths before allocation;
- distinguish truncation from malformed values;
- cap padding before reading/allocating it;
- never expose unauthenticated plaintext as trusted structured data;
- preserve authenticated bytes needed for transcript verification.

## Clock-skew policy

Create an explicit policy type rather than embedding a literal duration throughout the code.

Requirements:

- exact accepted past/future windows are documented from the current specification and implementation evidence;
- boundary values are tested;
- local clock is injected;
- failures return typed stale/future categories;
- repeated skew consequences are deferred to link/backoff policy;
- no peer-specific wall-clock values enter default logs or snapshots.

If Java I2P and i2pd differ, record the chosen compatibility policy and test both boundaries.

## Replay protection interface

The pure handshake crate defines replay material and decisions but does not own a runtime cache.

Define:

- exact replay key/token bytes derived from the handshake;
- bounded token wrapper with redacted diagnostics;
- retention duration or minimum policy input;
- decisions such as fresh, replayed, cache-full, or unavailable;
- fail-open/fail-closed behavior for cache exhaustion.

Preferred security posture is fail-closed for authenticated handshake admission when replay checking cannot be performed, unless deployed compatibility evidence requires a narrower exception.

Plan 035 may own the runtime cache, but Plan 033 must provide a deterministic in-memory reference cache for tests.

## Peer identity and RouterInfo binding

Outbound handshakes must validate:

- the expected RouterIdentity or router hash;
- the expected NTCP2 static public key from the selected RouterAddress;
- the authenticated RouterInfo identity received during the handshake;
- network identifier and signature validity where required;
- that transport options correspond to the authenticated peer.

Inbound handshakes must:

- authenticate the peer RouterInfo before producing an authenticated result;
- distinguish malformed RouterInfo, invalid signature, unsupported identity type, and transport-key mismatch;
- avoid mutating NetDB directly;
- emit only a typed authenticated peer result for later transport-manager admission.

Do not treat structural parsing as authentication.

## Padding policy boundary

Define an injected padding-policy interface or deterministic decision input.

Requirements:

- enforce specification minimum/maximum bounds;
- authenticate padding where required;
- avoid fixed production padding that fingerprints `i2pr`;
- deterministic fixed padding for vectors/tests only;
- no unbounded delay waiting to coalesce handshake writes;
- no remote-controlled allocation from padding lengths.

Record the initial production distribution or policy as an ADR decision, even if the runtime adapter is not yet live.

## Error taxonomy

Use bounded typed categories, including at minimum:

- truncated message;
- invalid fixed length;
- excessive padding;
- malformed options;
- deobfuscation failure;
- authentication failure;
- transcript mismatch;
- invalid/all-zero key agreement;
- wrong network;
- stale timestamp;
- future timestamp;
- replay detected;
- replay cache unavailable/full;
- peer identity mismatch;
- transport static-key mismatch;
- RouterInfo malformed;
- RouterInfo signature invalid;
- unsupported peer key/signature type;
- state violation;
- local resource/policy denial.

Do not return arbitrary remote text, raw bytes, keys, tags, addresses, or identity material.

## Resource and size bounds

Introduce constants/config types for:

- maximum SessionRequest bytes;
- maximum SessionCreated bytes;
- maximum SessionConfirmed bytes;
- maximum handshake padding per message;
- maximum RouterInfo payload accepted in SessionConfirmed;
- maximum option bytes;
- maximum replay-cache entries in reference tests;
- maximum handshake actions/steps;
- maximum buffered input retained by the state machine.

Every bound must cite the specification or compatibility evidence. Test zero/minimum/maximum/maximum-plus-one where meaningful.

## Deterministic tests

### Fixed-message tests

- exact SessionRequest bytes for initiator and responder processing;
- exact SessionCreated bytes;
- exact SessionConfirmed bytes;
- full deterministic initiator/responder transcript reaches matching authenticated keys;
- independent vector compatibility from Plan 032;
- RouterInfo identity/static-key binding.

### Mutation tests

- one-bit changes at every field boundary;
- malformed lengths and padding;
- wrong timestamp/network/static key/identity;
- replayed request and created material;
- tag/ciphertext mutation;
- duplicate or unknown options;
- truncated RouterInfo;
- invalid RouterInfo signature;
- state-step reordering and duplication.

### Simulated partial-I/O tests

Using `i2pr-testkit`, feed every handshake message:

- one byte at a time;
- split at every field boundary;
- with delayed final bytes;
- with disconnect/reset before each boundary;
- with write backpressure;
- with cancellation before and after every action;
- with deadline expiry at each stage.

The state machine itself remains pure; a test driver performs the partial-I/O adaptation.

### Replay/skew tests

- exact accepted boundaries;
- one unit beyond past/future limits;
- duplicate token;
- cache capacity one, exact limit, plus one;
- expiration and reuse after retention;
- deterministic ordering;
- cache-full policy.

## Fuzzing

Add targets for:

- SessionRequest parser after the minimum safe deobfuscation boundary;
- SessionCreated parser;
- SessionConfirmed authenticated plaintext/layout parser;
- handshake state command sequences;
- RouterInfo binding inputs;
- replay/skew policy inputs.

Fuzz invariants:

- no panic;
- no unbounded allocation;
- deterministic typed result;
- no secret/error payload leakage;
- state cannot authenticate without all required checks;
- failed state cannot resume.

## Documentation and support metadata

Update:

- `docs/architecture.md` with handshake action/state ownership;
- `docs/security-model.md` with replay, skew, identity-binding, padding, parser, and oracle threats;
- `specs/protocols/03-ntcp2.md` with resolved layouts/policies;
- `specs/support.toml` with non-advertised experimental handshake surfaces;
- `docs/protocol-support.md` with exact local evidence and non-claims;
- `AGENTS.md` and `CONTRIBUTING.md` with handshake test requirements;
- vector manifests and validation scripts.

## Required commands

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
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

## Closure record

Create `plans/033-closure.md` containing:

- state diagrams;
- action/result API inventory;
- exact message layout tables;
- skew/replay policy;
- peer-binding rules;
- size/resource constants;
- fixed/malformed/partial-I/O/fuzz evidence;
- exact command and CI results;
- support-ledger status;
- unresolved compatibility questions;
- explicit Plan 034 prerequisites.

## Acceptance criteria

Plan 033 closes only when:

- initiator and responder state machines are explicit and consuming;
- all three handshake messages have strict bounded codecs;
- deterministic full handshakes produce matching authenticated data-phase keys;
- replay, skew, identity, static-key, network, padding, and RouterInfo checks are enforced;
- partial-I/O/cancellation/deadline scenarios are reproducible;
- no sockets, runtime tasks, data frames, link policy, NetDB mutation, or capability claims are added;
- vectors, fuzz compilation, MSRV, dependency policy, CI, and documentation pass;
- `plans/033-closure.md` exists.

## Stop conditions

Stop and record the conflict if:

- deployed implementations disagree materially on message layout or transcript inputs;
- replay/skew behavior cannot be reconciled without weakening authentication;
- RouterInfo identity binding is ambiguous;
- partial input requires unbounded buffering;
- a pure state machine cannot express required behavior without runtime access;
- a generic abstraction begins modeling unrelated transports;
- independent vectors fail and the cause cannot be isolated;
- public-network traffic would be required to proceed.