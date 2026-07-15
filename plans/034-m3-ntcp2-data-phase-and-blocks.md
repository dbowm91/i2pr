# Plan 034: NTCP2 data phase, frame protection, and payload blocks

## Objective

Implement the runtime-neutral NTCP2 authenticated data phase after a successful Plan 033 handshake.

This plan covers frame-length obfuscation, AEAD-protected frames, nonce/counter progression, required payload blocks, rekey behavior, termination handling, and bounded I2NP message handoff. It does not add live TCP sockets, dial/listen policy, duplicate-link management, RouterInfo publication, NetDB mutation, or public-network interoperability.

## Preconditions

- `plans/033-closure.md` exists.
- Handshake output provides distinct transmit/receive key owners, frame-length key material, peer identity, and negotiated parameters.
- All relevant constants and cryptographic operations are fixed by Plan 032.
- Transport-neutral encoded-I2NP ownership is fixed by Plan 031.

## Scope

Implement:

- transmit and receive data-phase state objects;
- obfuscated two-byte frame lengths;
- strict frame size validation before allocation;
- ChaCha20-Poly1305 frame seal/open;
- nonce and rekey progression;
- canonical payload block encoding;
- strict authenticated plaintext block parsing;
- required blocks for I2NP, RouterInfo, timestamp, padding, termination, and options;
- bounded unknown-block handling where permitted;
- coalescing and padding decision inputs without runtime waits;
- typed frame actions and outcomes;
- deterministic partial-read/write drivers in testkit;
- fuzz and malformed corpora.

Do not implement:

- real sockets;
- transport-manager queue selection;
- link replacement;
- live idle timers;
- NetDB or RouterInfo publication policy;
- tunnel routing;
- capability advertisement.

## State model

Use separate transmit and receive owners. They must not share mutable nonce/key state.

Representative states:

```text
TransmitReady -> FramePrepared -> TransmitReady | Rekeyed | Terminated
ReceiveReady  -> LengthDecoded -> CiphertextCollected -> ReceiveReady | Rekeyed | Terminated
```

Requirements:

- counters advance exactly once per accepted frame;
- failed authentication does not advance into a reusable ambiguous state;
- counter exhaustion fails before wrap;
- rekey happens exactly at the specified threshold;
- terminated state cannot process more frames;
- direction-specific keys cannot be swapped accidentally;
- state objects are runtime-neutral and do not wait or allocate beyond explicit bounds.

## Frame length protection

Implement exact length-mask generation and progression.

Requirements:

- two-byte length is deobfuscated before allocation;
- zero/minimum/maximum/maximum-plus-one lengths are tested;
- invalid lengths produce typed errors;
- length-mask state advances exactly as specified;
- no attacker-controlled length can reserve a full maximum buffer before policy/resource admission;
- partial length reads are handled by the runtime driver, not by assuming TCP frame alignment.

Document the relationship between wire ciphertext length, AEAD tag, plaintext length, and block payload capacity.

## Frame buffer ownership

Define explicit owners for:

- two-byte obfuscated length;
- bounded ciphertext frame;
- authenticated plaintext frame;
- block iterator/decoder;
- encoded outbound frame.

No default cloning of full frames or I2NP payloads. Prefer consuming handoff from encoded I2NP owner into block/frame construction.

Resource leases must be representable for:

- queued outbound message bytes;
- outbound frame bytes;
- inbound ciphertext bytes;
- authenticated plaintext bytes;
- pending partial frame state.

Plan 035 will connect these to runtime queues, but Plan 034 tests must prove exact release under success/failure/drop.

## Payload block inventory

Implement only blocks required by the current NTCP2 specification and Milestone 3 exit criteria.

At minimum assess and implement:

- I2NP message block;
- RouterInfo block;
- timestamp block;
- padding block;
- termination block;
- options block.

For every block define:

- type code;
- minimum/maximum length;
- canonical encoder;
- strict decoder;
- multiplicity rules;
- ordering constraints if any;
- whether unknown instances may be skipped;
- whether duplicates are allowed;
- authenticated semantic output;
- typed malformed categories.

Do not add speculative blocks not required by current deployed interoperability.

## I2NP block handling

The I2NP block must:

- accept the bounded encoded-message owner from `i2pr-transport`;
- preserve complete I2NP message bytes;
- reject zero or oversized payloads according to the transport/protocol limit;
- return a consuming authenticated delivery object;
- avoid decoding/re-encoding unless validation requires it;
- provide redacted `Debug` and no payload logging;
- hold resource leases through exact receiver handoff.

Define whether initial authenticated delivery validates the I2NP envelope checksum/header immediately or hands canonical bounded bytes to the next service. Record the tradeoff and avoid duplicate unbounded parsing.

## RouterInfo block handling

RouterInfo blocks must:

- have explicit maximum size;
- use strict existing RouterInfo decoding and signature verification where required;
- distinguish structural validity from policy acceptance;
- emit a typed observation/update candidate, not mutate NetDB;
- prevent peer identity replacement within an authenticated link;
- handle duplicate RouterInfo blocks according to the specification.

## Timestamp and options

Timestamp blocks must use injected time policy and typed skew observations. They must not expose precise peer timing histories in default diagnostics.

Options blocks must:

- be bounded;
- reject malformed key/value structures;
- preserve unknown options only when permitted;
- avoid stringly typed policy leaking into transport-neutral code.

## Padding and coalescing

Define a pure outbound frame assembly policy input:

- candidate blocks;
- maximum frame size;
- desired bounded padding range;
- whether coalescing is allowed for the current call;
- deterministic test RNG decision.

The pure data-phase code may assemble a frame but must not sleep waiting for more blocks. Runtime scheduling/coalescing delay belongs to Plan 035.

Production padding must remain compliant and avoid a fixed fingerprint. Test padding is deterministic.

## Termination

Implement typed local/remote termination reasons and exact termination block encoding/decoding.

Requirements:

- remote reason codes map to bounded enums;
- arbitrary text is not retained;
- after local termination is emitted, only bounded drain/close behavior is allowed;
- after remote termination is authenticated, no further application payload is accepted;
- authentication failure is not answered with detailed protocol diagnostics;
- abrupt EOF/reset remains an I/O category for Plan 035, distinct from authenticated termination.

## Rekey and counters

Implement exact specification behavior for:

- frame nonce progression;
- rekey trigger threshold;
- rekey derivation;
- resetting/continuing counters as specified;
- length-obfuscation key progression;
- termination before impossible counter wrap.

Add fixed vectors around threshold-1, threshold, threshold+1, and maximum counter boundaries.

## Unknown and duplicate blocks

Document and test:

- allowed unknown-block skipping;
- maximum total unknown bytes;
- maximum block count per frame;
- duplicate required/control blocks;
- conflicting termination and application blocks;
- invalid ordering;
- zero-length blocks;
- trailing plaintext bytes.

Unknown skipping must occur only after frame authentication and must never bypass total size/count bounds.

## Error taxonomy

At minimum:

- truncated frame length;
- invalid/oversized frame length;
- ciphertext truncation;
- authentication failure;
- nonce/counter exhaustion;
- invalid block header/length;
- excessive block count;
- excessive unknown bytes;
- duplicate/conflicting block;
- invalid block order;
- oversized I2NP/RouterInfo/options/padding;
- peer identity mismatch in RouterInfo;
- invalid RouterInfo signature;
- remote termination;
- local state violation;
- resource denial.

Errors must not include payloads, tags, keys, frame bytes, peer addresses, or arbitrary remote text.

## Deterministic vectors

Extend the NTCP2 fixture corpus with:

- exact length-mask bytes across several frames;
- exact sealed/opened frames;
- each required block encoding;
- multi-block frames;
- minimum and maximum padding;
- termination frame;
- rekey boundary frames;
- independent Java I2P or i2pd plaintext/block/frame evidence where accessible.

Add malformed fixtures for every error category and one-bit mutation of lengths, tags, block types, block lengths, and control fields.

## Partial-I/O and fault tests

Using testkit drivers, test:

- length bytes split 1+1;
- ciphertext split at every boundary;
- multiple frames in one read;
- frame followed by partial next frame;
- one-byte writes;
- stalled write/read deadlines;
- truncation before tag;
- disconnect/reset after each byte boundary;
- duplicated ciphertext;
- delayed/reordered scheduler units while preserving stream semantics;
- cancellation with partially retained frame;
- backpressure and bounded buffered bytes;
- queue/resource limit one, exact, plus one;
- teardown returns all leases and buffers.

## Fuzzing

Add targets for:

- authenticated plaintext block parser;
- block sequence validator;
- length-mask/counter state transitions;
- frame command sequences using fixed test keys;
- RouterInfo/options block payloads;
- termination/control block combinations.

Fuzz invariants:

- no panic or unbounded allocation;
- unauthenticated ciphertext never yields blocks;
- accepted blocks obey count/size bounds;
- state/counter progression is deterministic;
- terminal state never resumes;
- no secret/payload data appears in errors.

## Documentation and support metadata

Update:

- `docs/architecture.md` with frame/buffer/block ownership;
- `docs/security-model.md` with frame length, AEAD, nonce, padding, block confusion, memory pressure, and termination threats;
- `specs/protocols/03-ntcp2.md` with block set and limits;
- `specs/support.toml` with non-advertised experimental data-phase surfaces;
- `docs/protocol-support.md` with exact evidence and non-claims;
- `AGENTS.md` and `CONTRIBUTING.md` with data-phase mutation/fuzz requirements;
- vector manifests/check scripts.

## Required commands

```text
cargo fmt --all --check
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
bash scripts/check-ntcp2-vectors.sh
cargo deny check advisories bans sources
cargo +1.85.0 check --workspace --all-targets
cargo +nightly check --manifest-path fuzz/Cargo.toml --offline --all-targets
CARGO_NET_OFFLINE=true bash scripts/fuzz-smoke.sh
git diff --check
```

## Closure record

Create `plans/034-closure.md` containing:

- transmit/receive state diagrams;
- frame and block layout tables;
- buffer/resource ownership inventory;
- block support matrix;
- nonce/rekey policy;
- vector provenance;
- partial-I/O, malformed, fuzz, and cleanup evidence;
- exact local and CI results;
- support-ledger state;
- Plan 035 prerequisites.

## Acceptance criteria

Plan 034 closes only when:

- authenticated transmit/receive frame states are explicit and direction-safe;
- frame lengths are deobfuscated and bounded before allocation;
- required blocks encode/decode canonically after authentication;
- nonce and rekey behavior has fixed vectors;
- malformed, unknown, duplicate, oversized, and terminal sequences fail deterministically;
- partial-I/O/backpressure/cancellation tests return all resources;
- no live sockets, manager policy, NetDB mutation, or capability claims are introduced;
- CI, MSRV, dependency policy, vectors, docs, and fuzz compilation pass;
- `plans/034-closure.md` exists.

## Stop conditions

Stop and record the conflict if:

- deployed block behavior differs materially across Java I2P and i2pd;
- frame limits cannot be reconciled with current implementations;
- correct parsing requires attacker-sized allocation;
- rekey behavior remains ambiguous;
- required I2NP handoff forces payload cloning;
- runtime scheduling leaks into the pure data-phase crate;
- independent evidence contradicts local vectors;
- public-network traffic is needed to continue.