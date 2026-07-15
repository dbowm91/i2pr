# Milestone 3 execution overview: NTCP2 and transport-neutral link management

## Purpose

Implement the first interoperable router-to-router transport while preserving the ownership, boundedness, privacy, and evidence standards established in Milestones 1 and 2.

Milestone 3 introduces NTCP2 protocol state, transport-neutral link contracts, supervised TCP integration, controlled mixed-router interoperability, and adversarial transport validation. It does not introduce NetDB operation, reseeding, tunnel construction, destinations, SAM, I2CP, service tunnels, proxies, SSU2, automatic NAT traversal, or public-network operation.

The milestone is complete only when `i2pr` can establish authenticated NTCP2 links in both directions with at least Java I2P and i2pd in an authorized controlled testnet, exchange required I2NP messages over those links, reject malformed and abusive peers within explicit bounds, and tear down every transport-owned task, queue, buffer, key, and resource lease.

## Prerequisites

Implementation must begin from the closure state recorded in:

- `plans/010-milestone-1-closure.md`
- `plans/020-milestone-2-closure.md`
- `plans/025-closure.md`
- `specs/protocols/03-ntcp2.md`
- `specs/SOURCES.md`
- `docs/architecture.md`
- `docs/security-model.md`
- `GUARDRAILS.md`

Plan 025 is a hard dependency. Do not weaken confirmed child-task draining, cancellation-aware completion classification, resource invariant reporting, physical protocol-module ownership, or the runtime/fixture CI gates.

## Authoritative protocol sources

Use the source hierarchy already defined by the repository. For NTCP2, the primary sources are:

1. The current official NTCP2 specification pinned in `specs/SOURCES.md`.
2. Incorporated official proposals and the Noise Protocol Framework revision named by the specification.
3. RFC 7748 and the exact external cryptographic standards referenced by the official specification.
4. Java I2P behavior when the specification is ambiguous.
5. i2pd as the second independent interoperability reference.
6. I2P+ and Emissary/go-i2p as additional compatibility and hardening evidence.

Implementation evidence must not override clear specification text. Any ambiguity that affects transcript bytes, key derivation, replay behavior, padding, frame limits, duplicate-link handling, or peer identity binding requires a recorded decision and a minimal differential test.

## Planned crate and dependency shape

Milestone 3 should add two production crates unless a narrower ADR demonstrates a better boundary:

```text
crates/i2pr-transport
crates/i2pr-transport-ntcp2
```

The intended direction is:

```text
i2pr-proto -----------+
                      |
i2pr-crypto ----------+--> i2pr-transport-ntcp2
                      |             |
i2pr-core ------------+-------------+--> i2pr-transport
                                      \
                                       +--> i2pr-runtime --> i2pr-daemon

i2pr-testkit may depend on transport crates for tests only.
```

Detailed constraints:

- `i2pr-transport` owns runtime-neutral transport-manager vocabulary, link identities, delivery contracts, admission outcomes, address observations, duplicate-resolution inputs, and privacy-safe snapshots.
- `i2pr-transport-ntcp2` owns NTCP2 address parsing, protocol constants, cryptographic state wrappers, handshake and data-phase state machines, block codecs, and deterministic transcript evidence.
- `i2pr-runtime` remains the only production crate that owns Tokio tasks, Tokio TCP sockets, timers, channels, wakeable cancellation, and supervised reader/writer children.
- `i2pr-daemon` remains the composition root. Public operator activation may remain disabled until the final milestone gate.
- `i2pr-storage` may be extended for versioned NTCP2 static-key persistence, but transport crates must not perform filesystem I/O directly.
- `i2pr-testkit` may add stream adapters and controlled interoperability helpers, but no production crate may depend on it.

If the proposed dependency graph creates a cycle or requires Tokio in a lower crate, stop and write an ADR before proceeding.

## Milestone plan sequence

Execute the following plans in order:

1. `plans/031-m3-transport-contracts-and-crate-boundaries.md`
2. `plans/032-m3-ntcp2-crypto-transcript-and-vectors.md`
3. `plans/033-m3-ntcp2-handshake-state-machines.md`
4. `plans/034-m3-ntcp2-data-phase-and-blocks.md`
5. `plans/035-m3-runtime-link-manager-and-addresses.md`
6. `plans/036-m3-interoperability-adversarial-validation-closure.md`

A later plan may begin only after the prior plan has a closure record, local validation evidence, and no unresolved stop condition that affects its inputs.

## Milestone-wide architecture rules

### Protocol versus runtime

NTCP2 protocol logic must be expressible as deterministic state transitions over explicit inputs and outputs. The protocol crate must not:

- open sockets;
- spawn tasks;
- sleep or read wall-clock time directly;
- install tracing subscribers;
- mutate NetDB;
- select tunnels;
- own daemon configuration;
- retry indefinitely;
- allocate from attacker-selected lengths without prior bounds checks.

The runtime adapter owns partial socket I/O, deadlines, task supervision, queue waiting, cancellation, and resource leases. It drives the protocol state machine and converts typed protocol actions into bounded I/O operations.

### Transport-neutral ownership

The transport manager owns:

- dial admission and backoff;
- inbound admission before expensive cryptographic work;
- one bounded link set per peer;
- duplicate-link resolution and replacement;
- outbound I2NP queue admission;
- link lifecycle and delivery outcomes;
- address and reachability observations;
- transport-level resource accounting.

NTCP2 owns protocol authentication, transcript state, frame/block processing, and typed termination reasons. It must not own peer scoring, RouterInfo publication policy, NetDB mutation, tunnel selection, or application routing.

### Secret handling

All transport static keys, ephemeral keys, chaining keys, cipher keys, nonces, transcript intermediates that remain secret, and serialized private transport-key records require:

- protocol-specific wrapper types;
- non-`Clone` secret ownership unless a concrete protocol requirement justifies duplication;
- redacted or absent `Debug` and `Display`;
- zeroization on drop and on every error path where feasible;
- no serde implementation for private material;
- no payload/key material in tracing, replay records, snapshots, or panic text.

Public keys, hashes, transcript digests, and authenticated peer identities must still use typed wrappers and bounded diagnostics.

### Resource bounds

Define and test explicit limits for at least:

- concurrent incoming handshakes;
- concurrent outgoing handshakes;
- per-IP and per-subnet pending inbound attempts;
- active links globally and per peer;
- outbound queued I2NP messages and bytes;
- inbound undecoded bytes;
- handshake message and padding lengths;
- data-frame ciphertext and plaintext lengths;
- block counts and unknown-block bytes;
- pending writes;
- replay-cache entries and retention duration;
- duplicate-link candidates;
- address observations;
- backoff entries;
- tracing/snapshot entry counts.

Every accepted operation must own its exact resource lease until completion, handoff, or drop. No transport queue may be unbounded.

## Required design decisions before wire implementation

The following decisions must be recorded in ADRs or plan closure records before dependent code is merged:

1. Reviewed Rust cryptographic crates and feature sets for X25519, ChaCha20-Poly1305, SHA-256/HMAC/HKDF, AES obfuscation, SipHash behavior, and constant-time comparison.
2. Whether to use a reviewed Noise state-machine library, lower-level reviewed primitives with an I2P-specific transcript implementation, or a narrowly wrapped combination.
3. Versioned persistence format and rotation policy for NTCP2 static keys.
4. Exact handshake clock-skew window and replay-cache policy.
5. Initial incoming/outgoing handshake limits and per-IP/per-subnet admission limits.
6. Duplicate-link winner and replacement rules, reconciled against Java I2P and i2pd.
7. Buffer ownership between TCP reads, handshake parsing, decrypted frame blocks, and I2NP dispatch.
8. Padding policy that remains specification-compliant without creating an unnecessarily stable `i2pr` fingerprint.
9. Initial IPv4/IPv6 scope. The default expectation is explicit configured dual-stack support where the host permits it, with no automatic UPnP, NAT-PMP, or address discovery.
10. The exact minimum block set required to complete authenticated I2NP exchange with the two target implementations.

Do not hide an unresolved decision behind a generic trait or placeholder default.

## Capability and support claims

All NTCP2 support entries begin as `experimental` and `advertised = false`.

A local codec, successful self-handshake, or deterministic transcript vector is not interoperability evidence. The support ledger may be advanced only when its evidence paths exist and the relevant conformance requirements are met.

Controlled test RouterInfo values may contain an NTCP2 address solely for authorized interoperability scenarios. This is not permission to publish a live address or enable public-network operation.

## Testing strategy

Milestone 3 requires four evidence layers:

1. **Pure deterministic protocol tests**
   - fixed keys, timestamps, IVs, padding decisions, and message bytes;
   - every handshake transcript/KDF stage;
   - state transitions and typed failures;
   - frame and block canonical bytes;
   - one-bit and boundary mutations.

2. **Deterministic simulated-I/O tests**
   - partial reads/writes at every boundary;
   - delay, truncation, duplication, reset, disconnect, and backpressure;
   - queue and resource saturation;
   - cancellation and deadline cleanup;
   - fixed-seed replay.

3. **Fuzzing and malformed corpora**
   - handshake parsers after deobfuscation/decryption boundaries where safe;
   - authenticated plaintext block parsing;
   - frame/block dispatch;
   - state-machine command sequences;
   - bounded corpus provenance.

4. **Controlled mixed-router interoperability**
   - Java I2P and i2pd, both inbound and outbound;
   - optional I2P+ and Emissary/go-i2p expansion;
   - exact versions, configurations, seeds, addresses, and observed outcomes recorded without private keys or payloads;
   - no public-network traffic.

## Privacy-aware observability

Add only fixed transport event names and bounded typed fields. Allowed fields include:

- transport kind;
- link direction;
- validated static service/channel identifiers;
- lifecycle state;
- typed handshake stage;
- typed failure or termination category;
- bounded counters and byte counts;
- monotonic durations;
- synthetic test peer/link identifiers.

Do not log:

- IP addresses or ports by default;
- RouterIdentity or Destination bytes;
- full hashes;
- public keys when they become stable peer correlators;
- transcript bytes;
- ciphertext or plaintext payloads;
- I2NP contents;
- private or ephemeral keys;
- nonces, chaining keys, authentication tags, or replay tokens;
- arbitrary remote error text;
- dynamic per-peer metric labels.

Any operator-only address diagnostics must be explicit, opt-in, and outside default tracing/snapshot paths.

## Documentation and evidence requirements

Each implementation plan must create a matching closure record:

```text
plans/031-closure.md
plans/032-closure.md
plans/033-closure.md
plans/034-closure.md
plans/035-closure.md
plans/036-closure.md
```

The final aggregate record must be:

```text
plans/030-milestone-3-closure.md
```

Every closure record must include:

- implementation commits;
- exact changed files;
- dependency and feature changes;
- constants and bounds introduced;
- public API inventory;
- secret-owner inventory where applicable;
- test matrix and exact commands;
- CI evidence or explicit absence;
- interoperability evidence or explicit absence;
- deviations and unresolved ambiguities;
- support-ledger changes;
- handoff prerequisites for the next plan.

## Milestone-wide quality gates

At minimum, retain and pass:

```text
cargo fmt --all --check
cargo check --workspace
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
cargo deny check advisories bans sources
cargo +1.85.0 check --workspace --all-targets
git diff --check
```

Add focused transport boundary, dependency, fixture, vector, and fuzz gates as the plans require. No closure may claim a test or interoperability result that was not executed.

## Milestone exit criteria

Milestone 3 closes only when all of the following are true:

- The intended transport crates and dependency boundaries are documented and mechanically checked.
- NTCP2 address options, static keys, handshake messages, transcript/KDF behavior, data frames, and required blocks are implemented with fixed evidence.
- Initiator and responder state machines are explicit, bounded, cancellable, and independent of direct socket ownership.
- Runtime TCP adapters use supervised tasks and bounded channels/resource leases.
- Replay, skew, padding, frame, block, duplicate-link, backoff, and shutdown policies are explicit and tested.
- `i2pr` completes inbound and outbound NTCP2 handshakes with Java I2P and i2pd in an authorized controlled testnet.
- Required I2NP messages cross an authenticated link in both directions.
- Malformed handshakes, replay attempts, wrong identities, excessive padding, oversized frames, slow reads/writes, queue saturation, and disconnects fail within bounded resources.
- Every normal, failed, cancelled, and forced transport path returns tasks, queues, buffers, replay entries, and resource usage to expected bounds.
- Default diagnostics reveal no payload, secret, address, identity, or peer-derived high-cardinality data.
- Support metadata remains truthful and links to concrete evidence.
- Normal, MSRV, dependency-policy, cross-platform, transport-boundary, fixture/vector, and fuzz compilation gates pass.
- `plans/030-milestone-3-closure.md` exists with exact interoperability and CI evidence.

## Stop conditions

Stop and record the issue rather than improvising if:

- official specification and deployed implementations disagree on transcript or KDF bytes;
- no reviewed Rust dependency can provide a required primitive under the MSRV and feature constraints;
- a generic Noise library cannot expose the exact I2P transcript/obfuscation behavior without unsafe patching or copied cryptographic primitives;
- correct partial-I/O handling requires unbounded buffering;
- a proposed duplicate-link rule causes repeatable churn against Java I2P or i2pd;
- replay/skew policy cannot be reconciled with current deployed behavior;
- transport implementation requires Tokio outside `i2pr-runtime` without an approved architecture change;
- a state machine must inspect NetDB or tunnel policy directly;
- private or public-network testing would be required to continue;
- interoperability cannot be reproduced from recorded versions and configuration;
- a support claim would exceed the available evidence.

Milestone 4 planning may begin only after the aggregate Milestone 3 closure explicitly confirms authenticated link interoperability and bounded cleanup.