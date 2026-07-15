# Plan 042: Runtime-owned NTCP2 wire driver

## Objective

Replace the non-production `blocked_missing_driver` seam in `tools/i2pr-interop` with a bounded runtime-owned NTCP2 wire driver that can initiate and accept authenticated sessions with the pinned Java I2P and i2pd references inside the Plan 038/039 private Ubuntu harness.

The driver must compose the existing runtime-neutral NTCP2 handshake and data-phase state machines with the Tokio-owned socket, deadline, cancellation, admission, replay, backoff, queue, and link-lifecycle primitives in `i2pr-runtime`.

This plan does not activate `i2pr-daemon`, reseed, NetDB participation, public RouterInfo publication, tunnels, SAM, I2CP, SSU2, or public-network behavior. The dedicated launcher remains an interoperability/test composition root.

## Prerequisites

- Plan 040 is complete.
- Plan 041 reference-only crosscheck passes on the target Ubuntu host.
- Exact Java I2P and i2pd runtime caches are available offline.
- The current runtime and protocol ownership boundaries remain mechanically enforced.
- The pinned references' private network ID, RouterInfo import paths, and readiness/authentication observations are proven.

## Architectural boundary

The ownership model is mandatory:

```text
tools/i2pr-interop
  -> parses bounded scenario configuration
  -> creates disposable local identity/RouterInfo material
  -> constructs a Tokio runtime and supervised service scope
  -> invokes runtime-owned NTCP2 listener/dial/driver APIs

crates/i2pr-runtime
  -> owns TcpListener/TcpStream
  -> owns Tokio tasks, timers, deadlines, cancellation, queues, and process-safe snapshots
  -> executes HandshakeAction requests
  -> retains replay, admission, active-link, and backoff owners
  -> promotes authenticated streams into the data-phase/link owner

crates/i2pr-transport-ntcp2
  -> remains runtime-neutral
  -> owns protocol sequencing, transcript, cryptographic state, framing, and typed protocol errors
  -> does not open sockets, access files, read clocks, spawn tasks, or install tracing subscribers
```

Do not add Tokio, filesystem, environment, process, or network-namespace dependencies to `i2pr-transport-ntcp2`.

## Deliverable 1: Typed interoperability scenario input

Replace the current launcher behavior that merely checks for a small file with a strict scenario schema.

The launcher input must contain only bounded, non-secret execution instructions, including:

- schema version;
- scenario ID;
- role: initiator or responder;
- address family;
- local literal address and port;
- peer literal address and port where applicable;
- private network ID;
- local identity/state directory under the run root;
- peer RouterInfo input path under the run root when dialing;
- handshake, read, write, queue, and drain deadlines;
- padding profile;
- selected I2NP smoke-message profile;
- deterministic test seed where deterministic policy is appropriate;
- expected result class;
- output status path under the run root.

Reject:

- DNS names;
- unspecified or wildcard peer targets;
- paths outside the run root;
- public or non-scenario addresses;
- unbounded timeouts, sizes, or counts;
- unknown fields;
- unsupported network IDs;
- requests to activate daemon, reseed, NetDB, SSU2, tunnels, or client services.

The launcher must not accept raw private keys or identities through command-line arguments.

## Deliverable 2: Disposable local identity and RouterInfo owner

Create a focused test-only identity owner using existing `i2pr-crypto`, `i2pr-proto`, and `i2pr-storage` primitives.

The owner must:

- create a valid router identity using approved cryptographic wrappers;
- create a valid NTCP2 static key;
- build the exact NTCP2 RouterAddress material required by the pinned protocol specification;
- use the synthetic namespace address and selected port;
- use the private network ID;
- sign a canonical RouterInfo;
- write mutable state only under the scenario run root with restrictive permissions;
- reload and revalidate state before use;
- expose RouterInfo bytes to the handshake action executor under an explicit maximum;
- never log or include keys, identities, RouterInfo bytes, or endpoints in sanitized output;
- delete all state through the harness finalizer.

Do not write random placeholder bytes into files named as identities or static keys. Every artifact consumed by the handshake must use the repository's validated types and formats.

Add deterministic constructors only for tests. Authorized interoperability execution should use OS randomness unless the protocol test explicitly requires a reproducible fixture and the resulting keys remain disposable.

## Deliverable 3: Handshake action executor

Add a runtime-owned executor for the runtime-neutral handshake machine. Suggested placement:

```text
crates/i2pr-runtime/src/ntcp2_driver.rs
```

The executor must repeatedly obtain the next protocol action and fulfill it under the caller's service scope.

### Read actions

For `ReadExact` and `ReadBounded`:

- use cancellation-aware Tokio I/O;
- apply the configured total handshake deadline and per-operation bounds;
- never allocate beyond the action maximum;
- distinguish EOF, timeout, cancellation, malformed length, and I/O failure;
- feed only complete bounded input back to the state machine;
- avoid wall-clock sleeps.

The implementation must follow the NTCP2 message-size semantics from the pinned specification and current state-machine contract. Do not treat one arbitrary TCP read as one protocol message.

### Write actions

For `Write`:

- enforce the action's existing bounded byte ownership;
- write all bytes under the configured deadline;
- classify cancellation, timeout, and I/O failure;
- avoid copying secret-bearing transcript data into logs or error strings.

### Timestamp actions

For timestamp requests:

- use an injected runtime clock source;
- use real UTC Unix time for authorized reference interoperability;
- use deterministic time in unit tests;
- preserve the existing clock-skew policy;
- emit only coarse typed skew outcomes.

### Replay actions

Connect replay requests to the runtime `ReplayCache`:

- map fresh, replayed, and full decisions exactly;
- retain entries for the requested bounded duration;
- clear the cache during explicit launcher teardown;
- expose only aggregate entry/capacity counters.

### Padding actions

Implement a bounded padding policy selected by scenario:

- minimum;
- deterministic representative;
- maximum valid;
- boundary/maximum-plus-one negative injection where the scenario owns malformed input.

Positive scenarios must generate spec-valid padding. Negative scenarios must identify whether malformed bytes are generated by i2pr or an external adversarial fixture.

### RouterInfo actions

Supply the locally generated, signed RouterInfo only after:

- strict revalidation;
- network-ID validation;
- size validation;
- NTCP2 address/static-key consistency validation.

### Terminal actions

Map `Authenticated` and `Terminate` into stable runtime outcomes. Do not collapse typed protocol errors into one generic failure before evidence collection.

## Deliverable 4: Initiator driver

Implement the outbound path:

1. parse and validate the peer RouterInfo;
2. select the exact scenario NTCP2 address without DNS resolution;
3. derive a privacy-safe dial key;
4. consult bounded backoff;
5. connect through `Ntcp2RuntimeService::dial`;
6. run the initiator handshake executor;
7. mark the dial authenticated only after the complete handshake succeeds;
8. admit one active-link lease;
9. transfer the authenticated stream and split keys into the data-phase owner;
10. exchange the selected I2NP smoke message;
11. perform orderly termination and bounded drain;
12. return aggregate typed counters.

A TCP connection must never clear backoff or increment authenticated-link evidence.

The peer identity, network ID, static key, and RouterInfo binding must match the expected peer RouterInfo. A mismatch is a typed terminal failure.

## Deliverable 5: Responder driver

Implement the inbound path:

1. bind only the scenario literal local address and port;
2. start the listener under a supervised child scope;
3. accept one stream under global, per-IP, per-subnet, and queue limits;
4. retain the pending-inbound permit through authentication;
5. run the responder handshake executor;
6. enforce replay and clock-skew decisions;
7. validate the peer RouterInfo and network ID;
8. release pending admission on every terminal path;
9. admit one active-link lease after authentication;
10. transfer the authenticated stream and split keys into the data-phase owner;
11. exchange the selected I2NP smoke message;
12. drain and shut down under bounded deadlines.

Listener readiness must be emitted separately from authenticated readiness so the Python harness can sequence reference startup without confusing the two.

## Deliverable 6: Authenticated data-phase owner

The current runtime link façade counts raw bytes and owns bounded read/write children, but the interoperability driver must preserve NTCP2 frame boundaries and authenticated receive/transmit states.

Add a test-only/runtime composition that:

- retains `TransmitState` and `ReceiveState` from the authenticated handshake;
- frames outbound NTCP2 blocks using the existing protocol module;
- reads exact encrypted frame lengths under bounds;
- authenticates and decrypts frames;
- parses only supported block types;
- rejects oversized, malformed, authentication-failed, or unexpected blocks with typed outcomes;
- never sends plaintext protocol payloads through a raw-byte link path that bypasses frame state;
- keeps all queues bounded by item count and bytes;
- propagates cancellation between reader and writer owners;
- reports aggregate frames/messages/bytes without payload contents.

If the existing `LinkHandle` abstraction cannot carry frame-level receive events without discarding bytes, extend it through a bounded typed channel or add a dedicated authenticated-link owner. Preserve active-link admission and supervised child ownership.

## Deliverable 7: Minimal I2NP smoke exchange

Select one already-supported, non-network-mutating I2NP message suitable for confirming bidirectional authenticated data exchange. Prefer a small message such as DeliveryStatus if its codec and reference behavior are already supported and verified.

The selection must be documented with:

- exact message type;
- why it is safe in the private test;
- maximum encoded size;
- expected response or acknowledgement behavior;
- reference implementation compatibility evidence;
- parser and serializer tests.

Do not add NetDB publication, tunnel build, garlic, or public routing behavior merely to obtain a smoke message.

For each positive path, evidence must show:

- authenticated handshake completed;
- at least one valid outbound I2NP message framed and sent;
- at least one valid inbound I2NP message framed, authenticated, parsed, and attributed to the link;
- orderly link shutdown.

If one reference does not naturally echo or reply to the selected message, define a bounded two-message exchange using message types already in Milestone 3 scope. Do not count arbitrary encrypted padding as I2NP exchange.

## Deliverable 8: Launcher lifecycle and status protocol

Convert `tools/i2pr-interop` into an async launcher with explicit subcommands:

```text
i2pr-interop ntcp2 listen --scenario-config <path>
i2pr-interop ntcp2 dial --scenario-config <path>
i2pr-interop ntcp2 inspect --state-dir <path>
```

`listen` must emit a bounded machine-readable readiness record once the listener is active, then a separate authenticated result when a session completes.

`dial` must emit one terminal typed result.

`inspect` may validate disposable state and emit redacted metadata, but must not expose identity or key material.

Status output must:

- use a versioned JSON schema;
- contain only fixed result/reason categories and bounded counters;
- avoid endpoints, paths, hashes of identities, peer hashes, RouterInfo bytes, I2NP payloads, and log excerpts;
- flush records so the parent can observe readiness;
- return nonzero for blocked, rejected, timeout, authentication failure, cleanup failure, or missing prerequisite.

Do not treat `blocked_missing_driver` as a readiness token after this plan. Remove the compatibility behavior from `I2prAdapter.wait_ready`.

## Deliverable 9: Python harness integration

Update the runner so non-environment scenarios:

- generate valid i2pr state and RouterInfo through the launcher or a reviewed helper;
- exchange RouterInfo with the selected reference using the proven adapter path;
- select initiator/responder sequencing from the scenario;
- start the reference and i2pr launcher in separate namespaces;
- wait for listener readiness when required;
- enforce a total scenario deadline;
- parse typed launcher status rather than raw logs;
- corroborate authentication using reference-side observation where available;
- collect process and runtime counters;
- finalize sanitized evidence outside the run root;
- remove all secret-bearing state.

Implement explicit primary IPv4 cases:

```text
java-i2pr-inbound-ipv4     # Java initiates; i2pr responds
i2pr-java-outbound-ipv4    # i2pr initiates; Java responds
i2pd-i2pr-inbound-ipv4     # i2pd initiates; i2pr responds
i2pr-i2pd-outbound-ipv4    # i2pr initiates; i2pd responds
```

Existing scenario IDs may be retained if direction is expanded into separate required runs, but the evidence must make each direction independently visible.

## Deliverable 10: Negative and resource profiles

After all four positive paths pass, wire the existing negative profiles to real execution.

Required classes:

- malformed handshake lengths;
- invalid transcript/authentication;
- replayed SessionRequest token;
- clock skew outside policy;
- excessive cleartext or authenticated padding;
- oversized frame;
- malformed block sequence;
- slow read and slow write peer;
- pending-inbound exhaustion;
- active-link exhaustion;
- queue item and byte exhaustion;
- duplicate/simultaneous connection race;
- cancellation during handshake;
- cancellation during data exchange;
- peer disconnect during each major phase.

Each case must assert:

- exact typed result category;
- bounded wall-clock completion;
- no panic;
- no secret-bearing diagnostics;
- pending/active/queue permits return to baseline;
- child tasks join;
- cleanup succeeds.

Do not run adversarial cases against references until positive handshake and data exchange pass for that reference.

## Deliverable 11: Unit, deterministic, and privileged tests

### Unit tests

Add tests for:

- scenario parsing and path confinement;
- identity/RouterInfo generation and reload;
- action-to-I/O result mapping;
- replay-cache mapping;
- padding profiles;
- typed status serialization and sanitation;
- authenticated result gating;
- I2NP smoke codec selection;
- cleanup counter validation.

### Deterministic in-memory tests

Use duplex/in-memory streams and deterministic clock/RNG interfaces to test:

- complete initiator/responder handshake driver composition;
- fragmented reads and writes;
- timeout and cancellation at every action;
- replay and skew rejection;
- data-frame exchange;
- malformed frame and block handling;
- duplicate-link policy inputs;
- resource release.

### Authorized Ubuntu tests

Run the four primary reference paths, then the negative matrix. These tests must use namespace-isolated reference caches and produce sanitized evidence.

## Required validation sequence

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-runtime-boundaries.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
cargo build --locked --package i2pr-interop
sudo -E bash scripts/interop/run-matrix.sh --profile handshake-smoke
python3 scripts/interop/validate-evidence.py
sudo -E bash scripts/interop/cleanup.sh
```

The handshake-smoke command must fail unless all four primary IPv4 direction gates pass.

## Stop conditions

Stop and record a typed blocker if:

- the current protocol state-machine contract cannot be driven without ambiguous TCP message boundaries;
- a required reference behavior needs unsupported NTCP2 or I2NP protocol scope beyond Milestone 3;
- authenticated status cannot be corroborated;
- the selected I2NP message is not supported by both references in this context;
- socket/task/filesystem ownership would have to move into a runtime-neutral crate;
- the driver can authenticate only by bypassing strict RouterInfo, static-key, network-ID, replay, or transcript validation;
- cleanup counters do not return to baseline;
- execution contacts the public network.

## Exit criteria

Plan 042 is complete when:

- `i2pr-interop` no longer returns `blocked_missing_driver` for supported scenarios;
- outgoing and incoming handshake action executors are complete and bounded;
- authenticated data framing is owned correctly;
- the selected I2NP smoke exchange is proven;
- i2pr initiates and accepts against Java I2P and i2pd over IPv4;
- all four direction gates have real sanitized evidence;
- positive results require authentication and I2NP exchange, not TCP readiness;
- negative/resource scenarios produce expected bounded typed outcomes after positive gating;
- all runtime, process, namespace, and secret-bearing state returns to baseline;
- runtime/protocol dependency boundaries remain enforced.

Completion of this plan supplies the core Milestone 3 interoperability evidence, but final closure remains gated on Plan 043 build-system reproducibility and the separate Milestone 3 closure review.
