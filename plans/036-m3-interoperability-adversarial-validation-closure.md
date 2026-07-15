# Plan 036: NTCP2 interoperability, adversarial validation, support evidence, and Milestone 3 closure

## Objective

Validate the completed NTCP2 implementation against independent router implementations in an authorized controlled testnet, exercise adversarial and resource-exhaustion behavior, reconcile support metadata with actual evidence, and produce the aggregate Milestone 3 closure record.

This plan is evidence and integration work. It may correct defects discovered during interoperability, but it must not broaden scope into reseeding, public NetDB participation, tunnel construction, destinations, SAM, I2CP, service tunnels, SSU2, automatic NAT traversal, or public-network operation.

## Preconditions

- `plans/031-closure.md` exists.
- `plans/032-closure.md` validates crypto/transcript vectors.
- `plans/033-closure.md` validates initiator/responder handshakes.
- `plans/034-closure.md` validates data-phase frames and blocks.
- `plans/035-closure.md` validates supervised TCP/link management and private-testnet harnesses.
- All prior stop conditions affecting wire behavior are resolved or explicitly accepted as blockers.

## Required independent implementations

The mandatory interoperability targets are:

1. Java I2P, pinned to an exact released revision/version.
2. i2pd, pinned to an exact released revision/version.

Optional additional evidence:

- I2P+;
- Emissary/go-i2p.

Both required implementations must be tested in both roles where technically supported:

- `i2pr` initiator to reference responder;
- reference initiator to `i2pr` responder.

A successful handshake in only one direction does not satisfy the milestone.

## Controlled testnet requirements

The test environment must:

- be isolated from the public I2P network;
- disable public reseed/bootstrap;
- use loopback, container networks, private namespaces, or dedicated lab hosts;
- use generated disposable router identities and NTCP2 static keys;
- pin implementation versions and configuration files;
- record network ID and clock configuration;
- prohibit use of operational identities, addresses, peer lists, or traffic captures;
- tear down all processes and remove secret artifacts after each scenario;
- retain only sanitized evidence permitted by repository policy.

Create a reproducible harness under a clearly documented test/integration path. Container images or external binaries must have version and digest/hash evidence.

## Interoperability scenario matrix

### Handshake matrix

For Java I2P and i2pd, run:

- inbound and outbound handshake;
- IPv4 loopback/private address;
- IPv6 loopback/private address where supported by CI/lab environment;
- minimum permitted padding;
- representative variable padding;
- maximum permitted padding;
- exact accepted clock skew boundaries;
- wrong/stale/future timestamp rejection;
- expected peer/static-key mismatch;
- wrong network identifier;
- replayed SessionRequest/SessionCreated material;
- simultaneous inbound/outbound connection race.

Record exact typed outcomes and reference-router logs after sanitization.

### Data-phase matrix

After authentication, exchange at minimum:

- DeliveryStatus or another small required I2NP message;
- a bounded RouterInfo block/update candidate;
- timestamp and padding blocks;
- options block if required by current deployed behavior;
- orderly termination.

Verify:

- complete authenticated I2NP bytes arrive in both directions;
- frame lengths and nonces progress correctly;
- multiple frames are exchanged;
- partial/coalesced writes interoperate;
- duplicate/unknown block behavior matches deployed routers;
- rekey behavior is tested if reachable within a bounded test or fixed accelerated harness without altering production constants.

Do not claim NetDB success merely because a Database message crosses the link.

### Duplicate-link matrix

Exercise:

- simultaneous inbound and outbound establishment;
- existing healthy link plus new inbound candidate;
- existing healthy link plus new outbound candidate;
- stale closure after replacement;
- loser drain/termination;
- queued message handling during replacement;
- repeated race to detect churn.

Run each scenario repeatedly against both Java I2P and i2pd. Record the exact winner rule and any implementation-specific differences.

## Adversarial validation

### Handshake abuse

Test:

- slowloris byte delivery at every handshake stage;
- oversized and maximum-plus-one padding;
- truncated fixed fields;
- one-bit key/tag/options mutations;
- repeated invalid DH/static-key attempts;
- replay storms within bounded test limits;
- stale/future timestamp storms;
- per-IP/per-subnet/global admission saturation;
- connection reset after each boundary;
- handshake completion followed by immediate reset;
- malformed RouterInfo and invalid signature;
- unsupported key/signature types.

### Data-phase abuse

Test:

- zero, maximum, and oversized frame declarations;
- authentication-tag mutation;
- ciphertext truncation;
- excessive block count;
- excessive unknown bytes;
- invalid/duplicate/conflicting control blocks;
- oversized I2NP/RouterInfo/options/padding blocks;
- nonce/rekey boundary mutations;
- termination followed by application payload;
- repeated empty/minimal frames if permitted;
- queue saturation and stalled reader/writer;
- disconnect during partial frame and partial write.

### Resource exhaustion

At capacity one, exact limit, and limit-plus-one, validate:

- pending inbound handshakes;
- pending outbound dials;
- per-IP/per-subnet admissions;
- active links;
- queued messages;
- queued bytes;
- inbound frame buffers;
- replay-cache entries;
- backoff records;
- graceful drains;
- child tasks.

Final assertions must include zero or expected steady-state values and zero release-underflow signals on valid paths.

## Security and privacy review

Perform a focused review of:

- static and ephemeral key lifetime;
- transcript/cipher state disposal;
- authentication failure oracles;
- timing/skew/replay behavior;
- duplicate-link churn and denial of service;
- buffer amplification;
- peer/address correlation in logs and metrics;
- error redaction;
- test artifact sanitation;
- default listener/activation state;
- address observation versus publication boundary;
- cleanup after panic/cancellation/forced abort.

Search committed source, tests, fixtures, logs, and documentation for accidental inclusion of:

- private keys;
- ephemeral keys;
- chaining/cipher keys;
- authentication tags/nonces where sensitive;
- payload bytes;
- operational RouterIdentity/Destination values;
- public network addresses;
- raw peer hashes or high-cardinality labels.

Add mechanical checks where stable patterns are possible.

## Fuzzing and extended deterministic campaigns

Run more than smoke compilation for critical pure boundaries.

Required targets include:

- handshake message parsers/state sequences;
- authenticated plaintext block parser;
- frame/block state command sequences;
- RouterInfo/options payloads;
- replay/skew policy;
- length/rekey transitions.

Record:

- target names;
- exact duration or iteration count;
- seed corpus hashes;
- crashes/timeouts/OOM results;
- minimized regressions committed as sanitized fixtures;
- tooling version.

Run a fixed-seed integrated simulation matrix large enough to cover scheduling variation, for example seeds `0..255`, while retaining bounded runtime. Any failure must record the seed and sanitized replay metadata.

## Interoperability evidence format

Create a manifest and per-scenario records containing:

- scenario ID;
- date;
- i2pr commit;
- reference implementation/version/commit;
- image/binary digest;
- configuration hash;
- direction;
- address family;
- padding profile;
- expected result;
- actual typed result;
- sanitized evidence artifact hashes;
- known deviations;
- reproduction command.

Do not store private keys, full peer identities, payload captures, or raw addresses. Use synthetic labels and private-testnet configuration templates.

## Support ledger transitions

Review every NTCP2 surface in `specs/support.toml`.

Possible statuses must remain evidence-driven:

- pure local/vector-only surfaces may remain `experimental`;
- handshake/data/runtime surfaces may advance only to the repository-defined status justified by both Java I2P and i2pd evidence;
- `advertised` remains false unless the daemon actually enables and publishes NTCP2 under an explicit controlled configuration and all conformance requirements are met;
- no public-network or production-ready claim is permitted.

Each ledger row must link to exact closure, vector, test, and interoperability evidence paths.

Update `docs/protocol-support.md` in parallel and ensure wording matches the machine-readable ledger.

## Operator activation boundary

Decide and document one of the following:

1. Keep live `run`/listener activation disabled after Milestone 3, with test harnesses as the only socket entry point.
2. Add an explicitly experimental, disabled-by-default private-testnet mode requiring affirmative configuration and warnings.

Do not enable public listening, automatic publication, reseeding, or public I2P participation by default.

If an experimental mode is added, require:

- explicit bind address;
- explicit peer RouterInfo/address inputs;
- no reseed;
- no NetDB discovery;
- no RouterInfo publication;
- clear non-production warning;
- bounded limits and shutdown;
- private/test network ID validation.

## Closure corrections

Interoperability defects may require changes to Plans 032–035 outputs. For each correction:

- add a regression vector/test before or with the fix;
- record the reference implementation behavior;
- update the relevant earlier closure record transparently;
- do not rewrite history or hide failed evidence;
- rerun the full affected matrix.

A material transcript/KDF discrepancy requires reopening Plan 032. A handshake layout/policy discrepancy requires reopening Plan 033. A frame/block discrepancy requires reopening Plan 034. Ownership/admission/duplicate issues require reopening Plan 035.

## CI and dedicated integration lanes

Normal CI must continue to run:

- formatting;
- all-target workspace check;
- workspace tests;
- Clippy;
- rustdoc warnings denied;
- dependency direction;
- runtime boundaries;
- fixture/vector manifests;
- MSRV;
- cargo-deny.

Add dedicated integration workflows or documented authorized manual lanes for Java I2P and i2pd. They must:

- never connect to the public network;
- pin versions/digests;
- enforce timeouts;
- upload only sanitized artifacts;
- clean up containers/processes;
- report skipped/unavailable environments honestly.

Do not claim mixed-router CI evidence if only local manual runs occurred. Record each form separately.

## Aggregate Milestone 3 closure

Create:

```text
plans/030-milestone-3-closure.md
```

It must include:

- implementation and evidence commit sequence;
- final crate/dependency graph;
- public API inventory;
- cryptographic dependency/feature table;
- protocol constant and bound inventory;
- secret-owner inventory;
- handshake state diagrams;
- frame/block support matrix;
- runtime service/task/queue ownership;
- admission, replay, skew, padding, backoff, duplicate, and address policies;
- Java I2P interoperability matrix;
- i2pd interoperability matrix;
- optional implementation evidence;
- adversarial/resource test matrix;
- fuzz campaign results;
- exact local commands/results;
- exact CI/integration run IDs;
- support-ledger transitions;
- deviations and unresolved limitations;
- explicit Milestone 4 readiness decision.

Also create `plans/036-closure.md` for this plan-specific execution record.

## Required commands

At minimum:

```text
cargo fmt --all --check
cargo check --workspace
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

Also run and record:

- extended fuzz commands/durations;
- fixed-seed integrated simulation matrix;
- Java I2P inbound/outbound harness commands;
- i2pd inbound/outbound harness commands;
- duplicate-link repeated race commands;
- adversarial/resource scenario suite;
- artifact sanitation checks.

## Acceptance criteria

Milestone 3 closes only when:

- Java I2P and i2pd both complete inbound and outbound NTCP2 handshakes with `i2pr` in an authorized controlled testnet;
- required I2NP messages cross authenticated links in both directions;
- peer identity and static-key binding are verified;
- replay, skew, padding, malformed handshake, frame, block, slow-peer, queue, and resource attacks fail within explicit bounds;
- duplicate-link resolution is deterministic and does not churn against either required implementation;
- reader/writer/manager tasks, sockets, queues, buffers, replay entries, backoff records, and leases clean up on all tested paths;
- default diagnostics and stored artifacts contain no prohibited sensitive data;
- support metadata exactly matches evidence and remains non-production;
- normal CI and all available integration lanes pass;
- `plans/036-closure.md` and `plans/030-milestone-3-closure.md` exist with exact evidence;
- no public-network behavior, NetDB participation, tunnel behavior, or automatic publication is introduced.

## Milestone 4 gate

Milestone 4 planning may begin only when the aggregate closure explicitly confirms:

- interoperable authenticated NTCP2 links with both required implementations;
- stable transport-neutral delivery contracts;
- bounded and supervised runtime ownership;
- truthful address observations suitable for later publication policy;
- reproducible private-testnet harnesses;
- no unresolved transcript, frame, duplicate-link, replay, or cleanup blocker.

Milestone 4 implementation must treat transport observations as inputs, not bypass the transport/NetDB boundary.

## Stop conditions

Stop and record the blocker if:

- either Java I2P or i2pd cannot interoperate in both directions;
- successful interoperability requires deviating from the official specification without a documented compatibility decision;
- reference implementations disagree in a way that cannot be safely reconciled;
- duplicate-link races cause persistent churn;
- any adversarial case causes unbounded memory, task, socket, queue, or CPU growth;
- cleanup cannot prove resource/task termination;
- sensitive artifacts cannot be sanitized reliably;
- the harness contacts the public I2P network;
- support claims would exceed actual evidence;
- a defect requires scope expansion into NetDB, tunnels, SSU2, or public publication.