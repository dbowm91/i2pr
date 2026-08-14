# Plan 108: ECIES-X25519 short tunnel-build construction core

- Status: **ready for implementation**
- Date: 2026-08-14
- Parent authority: Plan 102 + `plans/102-amendment-exploratory-tunnel-dependency.md`
- Predecessor: Plan 107 (`plans/107-status.md`)
- Milestone: 5 — network tunnel data plane and exploratory tunnels
- Scope class: **bounded Rust implementation pass; no live-network acceptance gate**

## 1. Purpose

Plan 107 deliberately stopped at the tunnel substrate boundary. It added the runtime-neutral exploratory tunnel pool, tunnel identity/types, short/variable build-record layout surface, `BuildCryptography` seam, and NetDB reply-path provider. It did **not** provide the cryptographic implementation or the state machine that turns a set of selected peers into an established exploratory tunnel.

Plan 108 closes that local implementation gap.

The sole product goal of this plan is:

> Given a fully specified all-ECIES exploratory path, explicit time/randomness, and a transport-independent delivery seam, i2pr can construct a standards-shaped Short Tunnel Build request, protect every per-hop record with the ECIES-X25519 tunnel-build cryptography, track the build as a bounded state machine, validate/decrypt the corresponding reply, and register the tunnel in `ExploratoryPool` **only after a successful build result**.

This is a deterministic construction pass. It is intentionally **not** the live interoperability pass.

Plan 108 must not reopen the Milestone 3 NTCP2 harness program merely because external delivery is still unavailable. The result of this plan should make the remaining external dependency smaller and more explicit, not absorb it.

## 2. Authoritative protocol basis

Implementation must be clean-room and checked against the current official I2P specifications, not copied from Java I2P, i2pd, Emissary, or other router source.

Primary protocol authorities:

1. I2P **Tunnel Creation Specification (ECIES-X25519)** — current normative source for ECIES tunnel-build request/reply encryption, KDFs, preprocessing, record construction, reply handling, and short-record rules.
2. I2P **I2NP Specification** — message framing and message types for `ShortTunnelBuild` and `OutboundTunnelBuildReply`.
3. I2P **Low-level Cryptography Specification** — X25519, ChaCha20, ChaCha20-Poly1305, HKDF/SHA-256, AES tunnel-layer primitives where applicable.
4. Existing repository protocol dossiers under `specs/`, especially tunnel/I2NP/common-crypto material.

Current protocol facts which this plan treats as hard requirements:

- Modern all-ECIES tunnel builds use the short 218-byte encrypted record format.
- A short plaintext request record is 154 bytes.
- Short builds use the ECIES-X25519 Noise-N construction for per-hop asymmetric request protection.
- Every ECIES hop/build record requires a unique ephemeral X25519 keypair; ephemeral reuse across hops or builds is forbidden.
- Per-hop layer/reply material is derived by the protocol KDF rather than carried directly in the short plaintext record.
- The hop's own reply record uses ChaCha20-Poly1305 authentication; the additional record preprocessing/layering rules defined by the tunnel-creation specification must be applied in their specified order.
- Short-build support is only valid for an all-ECIES path. Mixed ElGamal/ECIES construction remains outside this plan.

If repository constants or Plan 107 comments disagree with the current official specification, the implementation must correct the repository surface and record that correction in Plan 108 closure notes rather than preserving a known-invalid constant for compatibility with scaffolding.

## 3. Current starting point

Plan 107 established the following surfaces:

- `crates/i2pr-tunnel`
- `BuildRecordLayout::{Short, Variable}`
- `BuildRequestKind`
- `BuildCryptography`
- `BuildCryptographyError`
- `LayerKeys`
- `NoBuildCryptography`
- `ExploratoryPool`
- `ExploratoryPoolReplyPathProvider`
- NetDB reply-path provider integration

The repository still has these intentional limitations:

```text
live_ecies_x25519_build         = unavailable
build_cryptography_default      = NoBuildCryptography
exploratory_pool_population     = injected/test registration
live_mixed_router_build         = unavailable
qualified_external_transport    = unavailable
normal_daemon_ntcp2             = disabled-and-unenableable
live_routerinfo_lookup          = blocked
```

Plan 108 changes only the first three lines sufficiently to provide a real local construction engine. It does not claim the last three are solved.

## 4. Scope lock

### 4.1 In scope

Plan 108 owns exactly these implementation areas:

1. Current ECIES-X25519 **short** build-request record encoding.
2. Per-hop ephemeral X25519 generation through an injected RNG/source.
3. Normative request-record KDF and AEAD protection.
4. Per-hop derived tunnel layer/reply key ownership.
5. Required multi-record preprocessing/layering for a short build message.
6. Short build reply decryption/authentication and response-code validation.
7. A bounded, runtime-neutral tunnel-build state machine.
8. Explicit outbound delivery actions/events with no socket/runtime ownership.
9. Timeout/cancellation/failure semantics using caller-supplied time.
10. Success-only registration into `ExploratoryPool`.
11. Deterministic fixtures/tests proving the complete local construction/reply path.
12. Narrow documentation/support-matrix updates that describe only actually implemented behavior.
13. Removal/correction of stale Plan 107 comments referring to “Plan 008” where the intended successor is Plan 108.

### 4.2 Explicitly out of scope

The following are prohibited scope expansion for this plan:

- repairing or enabling normal-daemon NTCP2;
- changing Plan 101's NTCP2 activation guard;
- adding an NTCP2 listener or outbound production dial path;
- SSU2 implementation;
- mixed ElGamal/ECIES tunnel construction;
- legacy 528-byte ECIES build implementation beyond preserving existing parsing/layout types;
- full I2NP router dispatch;
- generic peer/link management redesign;
- live Java I2P, i2pd, or Emissary interoperability as a closure requirement;
- public-I2P-network testing;
- privileged namespaces, Docker, Multipass, rootless networking, or a new Python harness;
- HTTPS reseed acquisition;
- floodfill operation;
- NetDB live lookup/publication acceptance;
- LeaseSet, garlic, streaming, SAM, I2CP, HTTP proxy, or SOCKS work;
- transit-tunnel participation;
- performance benchmarking beyond basic boundedness/regression checks;
- a new CI workflow solely for this feature.

If one of these becomes necessary to prove a local algorithmic property, stop and record the dependency. Do not silently turn Plan 108 into an integration program.

## 5. Architectural constraints

### 5.1 Runtime-neutral ownership

`i2pr-tunnel` remains runtime-neutral.

It must not directly depend on:

- Tokio runtime APIs;
- sockets;
- DNS;
- filesystem persistence;
- HTTP clients;
- NTCP2 implementation types;
- daemon configuration types.

The tunnel construction engine emits typed actions and accepts typed results/events. Runtime/transport integration belongs to a later plan.

### 5.2 Safe-Rust and secret handling

Maintain `#![forbid(unsafe_code)]` / workspace unsafe-code policy.

Secret-bearing material must:

- not implement `Debug` with raw bytes;
- not derive `Clone` unless a protocol requirement proves duplication is necessary;
- not implement serde;
- use zeroization on drop for private ephemeral keys, derived AEAD keys, reply keys, IV/key material, and tunnel layer keys where practical;
- expose bytes only through narrowly scoped borrows;
- never be included in error messages, tracing fields, panic text, snapshots, or fixtures committed as live secrets.

Test-only deterministic keys are acceptable when clearly marked as public fixtures and never reused as production defaults.

### 5.3 Explicit time and randomness

The core engine must not call wall-clock or operating-system randomness internally.

Use explicit inputs/traits for:

- current time / build timestamp;
- message IDs / tunnel IDs where random allocation is required;
- ephemeral X25519 private material;
- record padding.

Production adapters may later bind those seams to OS randomness and the runtime clock.

This keeps tests deterministic and makes accidental ephemeral-key reuse testable.

### 5.4 Bounded allocation and state

Every collection and byte buffer introduced by Plan 108 must have a protocol- or policy-derived maximum.

No unbounded:

- path length;
- build records;
- pending builds;
- reply buffers;
- retry queues;
- failure histories;
- event queues.

Use the existing protocol maximum for build record count. Reject impossible lengths before allocating based on untrusted input.

## 6. Work package A — correct and freeze short-build wire constants

Before implementing cryptography, audit the existing tunnel/I2NP constants against the current official specification.

Required checks include:

- encrypted short record size: 218 bytes;
- plaintext short request size: 154 bytes;
- plaintext short reply size: 202 bytes;
- maximum build-record count;
- `ShortTunnelBuild` I2NP message type;
- `OutboundTunnelBuildReply` I2NP message type;
- field endianness;
- ephemeral public-key byte order;
- accepted current request expiration behavior;
- request flags for inbound gateway / outbound endpoint / intermediate participant;
- current layer-encryption-type encoding;
- reserved/unused bytes and zero requirements;
- build options Mapping encoding requirements.

### Important corrective requirement

Plan 107's current `BuildRequestKind::ShortTunnelBuild` numeric constant must be verified rather than trusted. The official I2NP documentation defines `ShortTunnelBuild` as type **25** and `OutboundTunnelBuildReply` as type **26**. If the repository currently carries `225`, correct it in this plan and add a regression test.

Do not widen this correction into unrelated I2NP codec work.

### Deliverables

Prefer small protocol-specific constants/types in `i2pr-proto` when they are wire facts; keep tunnel-construction policy in `i2pr-tunnel`.

Add compile-time/runtime tests pinning the normative sizes/message IDs.

## 7. Work package B — typed short request/reply records

Introduce a typed internal representation for the short plaintext request record rather than constructing protocol records with ad hoc byte offsets throughout the builder.

A representative shape may be:

```rust
pub struct ShortBuildRequestRecord {
    receive_tunnel: TunnelId,
    next_tunnel: TunnelId,
    next_router: RouterHash,
    role: HopRole,
    layer_encryption_type: LayerEncryptionType,
    request_time: BuildRequestTime,
    expiration: BuildExpiration,
    next_message_id: u32,
    options: BuildOptions,
}
```

Exact names are implementation discretion; wire semantics are not.

Requirements:

- reject zero receive/next tunnel IDs where forbidden by the spec;
- enforce mutually exclusive gateway/endpoint flags;
- intermediate-hop flag encoding must be canonical;
- enforce supported expiration values/range;
- reject unsupported layer-encryption types explicitly;
- emit reserved fields canonically;
- encode empty options canonically;
- bound options before encoding;
- exactly produce the normative plaintext byte length;
- parsing a response must reject malformed Mapping/options before looking at response status where the spec requires that ordering.

Do not expose partially validated plaintext records as a general public protocol API unless another crate actually needs them.

## 8. Work package C — ECIES-X25519 short-build cryptography

Replace `NoBuildCryptography` as the only usable implementation with a real ECIES-X25519 short-build implementation.

Suggested implementation type:

```rust
pub struct EciesX25519BuildCryptography { ... }
```

The implementation should satisfy the existing `BuildCryptography` abstraction only if that abstraction accurately represents the normative algorithm. If Plan 107's trait is too lossy (for example, one `LayerKeys` value for an operation that requires distinct per-hop contexts), change the seam now rather than forcing protocol state into an incorrect API.

### 8.1 Per-hop crypto context

Define a non-cloneable, zeroizing per-hop context retaining only what the creator needs to validate the future reply and operate the tunnel after success.

Conceptually it should retain:

- peer RouterHash;
- peer static X25519 public key;
- creator ephemeral private/public key as required during construction;
- chaining/hash/KDF outputs required by the specification;
- reply AEAD material;
- tunnel layer encryption material;
- any IV/key derivation outputs required for data-plane operation;
- request-record index / role needed for reply processing.

Destroy temporary intermediate secrets as early as possible.

### 8.2 Ephemeral uniqueness

For an N-hop build, generate N independent ephemeral X25519 private keys.

Tests must prove:

- one build never reuses an ephemeral key between hops;
- consecutive builds do not reuse deterministic sequence entries accidentally;
- a deliberately repeating RNG/test source is detected or makes a test fail closed if the production abstraction can detect it;
- no API allows one per-hop crypto context to be reused for another hop by accident.

Do not add a global process cache of used ephemeral keys merely to prove uniqueness; that would be unbounded state. Prefer ownership/type structure plus a bounded build-local uniqueness check if needed.

### 8.3 Noise-N/KDF implementation

Implement the current official tunnel-build derivation literally from the specification:

- initialization/chaining/hash values;
- X25519 ephemeral-static DH;
- HKDF/SHA-256 steps;
- associated-data/hash treatment;
- ChaCha20-Poly1305 nonce construction;
- request payload encryption;
- derivation of reply keys and tunnel layer keys;
- any per-record key derivations used by short records.

Do not reuse NTCP2's complete handshake state machine. Shared low-level crypto helpers may be factored into `i2pr-crypto` if, and only if, they are genuinely protocol-neutral primitives with identical semantics.

### 8.4 Record protection

For each hop:

1. build exactly one canonical plaintext request record;
2. generate exactly one ephemeral X25519 keypair;
3. derive the record crypto context;
4. produce the spec-shaped 218-byte protected record;
5. retain only the minimal creator-side reply/layer context.

Any AEAD failure, invalid peer key, all-zero DH result, malformed input, unsupported key type, or length mismatch must return a typed error and produce no partially usable build.

## 9. Work package D — multi-record preprocessing and build-message assembly

Tunnel-build security depends on more than encrypting each independent record. Implement the normative record ordering/preprocessing/layering required before the message is sent and required when processing the reply.

Requirements:

- randomly/deterministically place each hop's record into a unique record slot using the injected randomness source;
- preserve a private creator-side mapping from hop to slot;
- perform the required symmetric preprocessing in exactly the official order;
- ensure every hop sees only the transformation intended for that hop;
- prevent duplicate slot assignment;
- validate message record count before assembly;
- never expose slot mapping or crypto keys via logs/diagnostics;
- generate a standards-shaped `ShortTunnelBuild` I2NP payload/message representation consumable by a later delivery adapter.

The implementation must not assume that path order equals record slot order.

Negative tests must cover reordered records, altered ciphertext, removed/extra record shapes where representable, and slot-map corruption.

## 10. Work package E — reply processing

Implement creator-side handling of the short-build reply path for the build types supported by this plan.

Requirements:

- accept only the expected response message kind for the active build;
- enforce build/message identity correlation before expensive crypto when possible;
- enforce exact record count/size bounds;
- reverse the required preprocessing/layering in normative order;
- authenticate/decrypt each hop's own response record with its derived context;
- reject any AEAD failure;
- parse response options with existing bounded Mapping rules;
- accept only response codes defined by current specification/policy;
- treat any non-success hop response as build failure;
- retain per-hop rejection category for local policy/metrics without exposing secret material;
- never register a partial tunnel if only some hops accepted.

Unknown/noncanonical response codes should fail closed unless the official spec explicitly provides forward-compatible semantics.

## 11. Work package F — runtime-neutral tunnel-build state machine

Add a bounded state machine which owns one attempted build from path specification through terminal outcome.

Representative states:

```text
Prepared
  -> Protecting
  -> ReadyForDelivery
  -> AwaitingReply
  -> Established

or terminal:
  -> Rejected
  -> TimedOut
  -> Cancelled
  -> InvalidReply
  -> CryptoFailed
  -> DeliveryFailed
```

Exact enum names are implementation discretion.

### 11.1 Inputs

A new build must consume a fully validated path specification containing, at minimum:

- direction (inbound/outbound);
- exploratory role;
- ordered hop identities;
- peer RouterHashes;
- peer ECIES-X25519 static encryption keys extracted from validated RouterInfos;
- tunnel IDs / next-hop IDs;
- message ID(s);
- explicit creation time/lifetime;
- injected randomness source.

Do not make the builder itself query NetDB.

If a required peer RI/key is unavailable, the caller must resolve it before construction or receive a typed `MissingPeerKey`/equivalent result.

### 11.2 Actions

The state machine must expose transport/runtime-neutral actions such as:

```rust
BuildAction::Deliver {
    first_hop: RouterHash,
    message: OutboundI2npMessage,
    deadline: ...,
}
```

For inbound/outbound build semantics where an existing tunnel/reply path is required by the official protocol, model that requirement explicitly as a typed delivery/reply route. Do not secretly substitute a direct transport.

No action may contain private keys or derived secret material.

### 11.3 Events

Accept explicit events such as:

- delivery accepted/failed;
- build reply received;
- deadline reached;
- cancellation;
- shutdown.

Invalid state/event combinations must return typed errors or be harmlessly ignored according to documented semantics; they must not panic.

### 11.4 Deadlines and retries

Plan 108 should implement **one bounded attempt** per state-machine instance.

Do not add complex retry/backoff/peer-reselection policy here. A higher-level pool/registrar may create another attempt after a terminal failure.

A build must have one explicit absolute deadline derived from caller policy. Late replies must not resurrect a terminal build.

## 12. Work package G — success-only exploratory pool registration

Add a narrow coordinator/registrar that connects a successfully completed build to `ExploratoryPool`.

Hard rule:

> `ExploratoryPool` must never contain a tunnel merely because a build request was constructed or delivered.

Registration is permitted only after all expected hop responses validate and accept.

On success, transfer only the established-tunnel material the pool/data plane needs:

- tunnel identity/direction;
- ordered peers;
- expiration/lifetime;
- established layer key material through an appropriate secret-bearing owner;
- endpoint/gateway routing identifiers needed later by the data plane.

This may require evolving Plan 107's `TunnelPeer`/pool record so it can own the established cryptographic context. Do so narrowly.

On any failure, all attempted-build secret material must be dropped/zeroized and no pool entry created.

Add tests asserting pool length/state is unchanged for every terminal failure category.

## 13. Work package H — deterministic local simulation

Plan 108 must prove the full local algorithm without requiring a real router process.

Create a deterministic in-process test peer/hop model capable of:

- receiving one protected short request record;
- using a fixed ECIES static keypair to decrypt/validate it according to the same normative algorithm from the responder side;
- producing a standards-shaped accepted/rejected response record;
- applying the correct reply transformations;
- returning a reply to the creator state machine.

### Critical independence rule

Do not make the simulation a trivial mirror that calls the creator's `seal()` implementation and therefore proves only self-consistency.

Where practical:

- test responder-side derivation through separate functions/interfaces;
- use published/spec-derived fixed vectors for primitive/KDF stages;
- assert intermediate public values (ephemeral public key, hashes/KDF outputs, ciphertext/tag) for deterministic fixtures when authoritative vectors are available;
- keep end-to-end simulation tests in addition to vector/negative tests.

The simulation is evidence that the local implementation is internally complete. It is **not** evidence of Java/i2pd interoperability.

## 14. Required test matrix

At minimum, add coverage for the following.

### 14.1 Wire/record tests

- short request encodes to exactly 154 plaintext bytes;
- protected short record is exactly 218 bytes;
- short reply plaintext is exactly 202 bytes;
- `ShortTunnelBuild` message type is normative;
- `OutboundTunnelBuildReply` message type is normative;
- flags encode gateway/endpoint/intermediate roles correctly;
- invalid simultaneous endpoint+gateway flags are rejected;
- zero/invalid tunnel IDs rejected;
- unsupported expiration/layer encryption type rejected;
- options Mapping bounds enforced.

### 14.2 Crypto tests

- deterministic X25519/KDF fixture;
- deterministic protected-request fixture;
- deterministic reply-decrypt fixture;
- altered ephemeral public key rejected/fails authentication;
- altered ciphertext rejected;
- altered Poly1305 tag rejected;
- wrong peer static key rejected/fails authentication;
- wrong reply key rejected;
- all-zero/invalid DH result handled safely;
- distinct hop ephemeral public keys in one build;
- distinct ephemeral public keys across deterministic sequential builds.

### 14.3 Multi-hop/preprocessing tests

For representative 1-hop and multi-hop paths:

- every slot unique;
- path order independent of slot order;
- preprocessing/reversal round-trip succeeds;
- record reordering detected or causes authenticated failure as defined by protocol;
- modifying one record does not yield a partially accepted tunnel;
- reply processing maps each response to the correct hop.

### 14.4 State-machine tests

- prepare -> delivery action -> accepted reply -> Established;
- delivery failure -> terminal, no pool registration;
- timeout -> terminal, late reply ignored/rejected;
- cancellation -> terminal, secrets dropped;
- malformed reply -> InvalidReply;
- wrong message/build correlation rejected;
- one-hop rejection -> whole build rejected;
- duplicate event does not duplicate registration;
- terminal state cannot be reactivated.

### 14.5 Pool registration tests

- success registers exactly one tunnel;
- failure registers none;
- expired result cannot be registered;
- already-present conflicting tunnel ID handled deterministically;
- established key material is not printable/serializable;
- removal/expiry drops secret-bearing context.

### 14.6 Boundary/regression tests

- `i2pr-tunnel` still has no Tokio/socket dependency;
- normal daemon still rejects `transport.ntcp2.enabled = true`;
- no NTCP2 service appears in normal daemon graph;
- no test requires Internet access;
- no test requires Java I2P/i2pd/Emissary;
- no Python harness added;
- no privileged namespace assumption introduced.

## 15. Fuzz/property testing

Extend existing Rust fuzz/property infrastructure only where it already fits the repository.

High-value targets:

- short request plaintext parser/encoder if parser is exposed;
- reply Mapping/options parser;
- protected record/message length validation;
- state-machine event sequencing;
- build-record slot permutation logic.

Properties:

- arbitrary bytes never panic;
- malformed lengths never trigger oversized allocations;
- terminal states remain terminal;
- no failure path registers a pool entry;
- valid encode/decode fixtures are canonical where round-trip APIs exist.

Do not create a separate testing framework or Python corpus generator for Plan 108.

## 16. Dependency policy

Prefer existing workspace dependencies and existing `i2pr-crypto` primitives.

Before adding a crate, verify that the required primitive is not already available through:

- `x25519-dalek`;
- `chacha20poly1305`;
- `sha2`;
- `hmac`/existing HKDF helper;
- `zeroize`;
- existing AES helper if required by the specified tunnel layer/data-plane key surface.

If raw ChaCha20 stream functionality required by the normative preprocessing is not exposed by the existing AEAD crate, adding a focused pure-Rust RustCrypto primitive crate is acceptable. Keep features minimal and record why it is required.

Do not add OpenSSL/libsodium/system crypto.

Maintain MSRV 1.85 even though the repository's pinned development toolchain may be newer.

## 17. Error taxonomy

Avoid collapsing protocol/crypto/build failures into strings.

The public construction boundary should preserve categories equivalent to:

```text
UnsupportedPeerEncryption
MissingPeerKey
InvalidPath
InvalidTunnelId
InvalidBuildRecord
InvalidBuildMessage
InvalidDhResult
RequestAuthenticationFailed
ReplyAuthenticationFailed
UnsupportedReplyCode
HopRejected
DeliveryFailed
DeadlineExceeded
Cancelled
PoolRegistrationConflict
```

Names may differ, but callers must be able to distinguish:

- local programmer/configuration errors;
- peer/protocol rejection;
- cryptographic authentication failure;
- delivery failure;
- timing/cancellation;
- resource/policy limit failure.

Do not expose cryptographic key bytes in errors.

## 18. Documentation changes

On implementation, update only documentation whose support claim changes:

- `docs/architecture/i2pr-tunnel.md`;
- `docs/protocol-support.md`;
- `specs/support.toml` if that is the active support registry;
- README status summary if necessary;
- Plan 108 closure record.

Correct stale Plan 107 comments that say “Plan 008” / “Plan 008+” for this work.

Do **not** mark any of the following implemented merely because Plan 108 passes local simulation:

- live tunnel builds;
- mixed-router interoperability;
- live RouterInfo lookup;
- normal daemon transport;
- public-network operation.

Use wording such as:

```text
ecies_x25519_short_build_crypto = implemented-local
short_build_state_machine       = implemented-local
exploratory_pool_registration   = success-gated-local
external_build_delivery         = unavailable
live_mixed_router_build         = blocked-on-qualified-delivery
```

## 19. Validation commands

Closure must run the repository's normal Rust validation surface. At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

Run existing relevant fixture/protocol checkers as required by current repository policy, but do not resurrect retired rootless/live-router harnesses merely to make an old historical script green.

If the repository's pinned toolchain requires `cargo +<version>`, use the repository-pinned version for closure and separately preserve MSRV compatibility according to existing CI/project policy.

## 20. Explicit acceptance criteria

Plan 108 is complete only when **all** of the following are true.

### Protocol correctness

- [ ] Existing short-build constants were audited against current official I2P specifications.
- [ ] `ShortTunnelBuild` and reply message IDs are correct and regression-tested.
- [ ] Short request plaintext encoding is canonical and exactly sized.
- [ ] Short response parsing is bounded and exactly sized.
- [ ] Request ECIES-X25519/Noise-N derivation matches normative specification/vectors.
- [ ] Reply authentication/decryption matches normative specification/vectors.
- [ ] Required record preprocessing/layering order is implemented and tested.
- [ ] Each ECIES hop receives a unique ephemeral X25519 keypair.

### Construction behavior

- [ ] A validated all-ECIES path can be transformed into a complete standards-shaped `ShortTunnelBuild` message without sockets/runtime access.
- [ ] The engine emits an explicit delivery action for the first-hop route.
- [ ] A valid deterministic multi-hop reply drives the build to `Established`.
- [ ] Any hop rejection prevents establishment.
- [ ] Any authentication/format failure prevents establishment.
- [ ] Timeout/cancel/delivery failure are terminal.
- [ ] Late/duplicate events cannot resurrect or double-register a build.

### Pool/security behavior

- [ ] `ExploratoryPool` registration occurs only after full successful reply validation.
- [ ] Every tested failure path leaves the pool unchanged.
- [ ] Secret-bearing build/tunnel material is zeroized where practical and is not `Debug`/serde exposed.
- [ ] No unbounded build-local collection or externally controlled allocation is introduced.

### Architecture/scope

- [ ] `i2pr-tunnel` remains runtime- and transport-neutral.
- [ ] Normal-daemon NTCP2 remains disabled and unenableable.
- [ ] No SSU2, generic I2NP dispatch, reseed HTTPS, SAM/I2CP, streaming, transit, or floodfill scope was added.
- [ ] No new Python interoperability harness or privileged environment requirement was added.
- [ ] No live-network claim is made from deterministic simulation.

### Repository quality

- [ ] Workspace formatting/check/tests/clippy/docs pass.
- [ ] Existing dependency/runtime boundary checks pass or any pre-existing unrelated baseline failure is documented precisely.
- [ ] Stale “Plan 008” tunnel-build comments are corrected.
- [ ] Documentation/support matrices describe the new support level conservatively.
- [ ] A `plans/108-status.md` closure record states exact achieved and blocked states.

## 21. Required closure state

A successful Plan 108 should end with approximately this authority record:

```text
plan_103                         = passed
plan_104                         = passed
plan_105                         = passed
plan_106                         = passed-local-bootstrap-integration
plan_107                         = passed-exploratory-substrate
plan_108                         = passed-local-short-build-construction
routerinfo_validation            = implemented
local_netdb                      = implemented
persistent_routerinfo_cache      = implemented
su3_reseed_verification          = implemented
netdb_query_state_machine        = implemented
exploratory_tunnel_substrate     = implemented
ecies_x25519_short_build_crypto  = implemented-local
short_build_state_machine        = implemented-local
success_gated_pool_registration  = implemented-local
external_build_delivery          = unavailable
live_mixed_router_build          = blocked-on-qualified-delivery
live_routerinfo_lookup           = blocked-on-live-exploratory-path
normal_daemon_ntcp2              = disabled-and-unenableable
ntcp2                            = experimental-non-advertised
milestone4_full_exit             = pending-cross-milestone-checkpoint
```

## 22. Handoff after Plan 108

Do not automatically make Plan 109 “fix NTCP2.”

First inspect the completed Plan 108 action boundary and answer one narrow question:

> What is the smallest standards-conformant mechanism available in the current environment for delivering one constructed I2NP tunnel-build request to an independent I2P router and receiving the corresponding reply?

At that checkpoint, evaluate the real available choices against the current repository/environment:

- narrowly repair/reuse NTCP2 if its remaining defect can now be localized against this concrete consumer;
- use another already-available qualified router-to-router transport if one exists by then;
- add the minimum missing router dispatch/runtime adapter if transport is already sufficient;
- use deterministic/external fixture evidence if no live delivery mechanism is possible, while recording that live acceptance remains blocked.

Any external interoperability pass should have one explicit success condition and one bounded failure budget. It must not recreate the broad historical NTCP2 harness/testing architecture.

## 23. Execution guidance for a smaller implementation model

Implement in this order and do not skip directly to integration:

1. Audit/fix constants and short-record typed encoding.
2. Add deterministic primitive/KDF vectors.
3. Implement one-hop request ECIES protection.
4. Implement one-hop reply authentication/decryption.
5. Generalize to per-hop contexts with guaranteed unique ephemerals.
6. Implement multi-record slot placement and preprocessing.
7. Implement the runtime-neutral build state machine.
8. Add deterministic responder simulation.
9. Connect success to `ExploratoryPool` registration.
10. Run negative/adversarial tests.
11. Run full repository validation.
12. Update support docs and write `plans/108-status.md`.

If a protocol uncertainty appears in steps 1–6, stop that subtask and check the official specification before inventing behavior. If external delivery becomes necessary before step 9, the architecture has leaked scope: correct the local seam rather than adding networking to Plan 108.
