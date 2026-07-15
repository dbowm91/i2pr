# Plan 032: NTCP2 cryptographic foundation, transcript model, and fixed vectors

## Objective

Select and integrate reviewed cryptographic dependencies, implement the non-I/O NTCP2 cryptographic foundation and transcript/KDF model, add versioned transport-static-key persistence, and establish fixed deterministic vectors for every primitive and transcript stage required by the later handshake plan.

This plan does not implement a complete initiator or responder handshake, data-phase frame processing, live sockets, link management, or interoperability claims. Its output is a small, auditable set of cryptographic wrappers and deterministic evidence that Plan 033 can compose into explicit state machines.

## Preconditions

- `plans/031-closure.md` exists and confirms the final crate graph.
- `i2pr-transport-ntcp2` remains Tokio-free and filesystem-free.
- The official NTCP2 specification and external cryptographic standards are pinned in `specs/SOURCES.md`.
- Any source ambiguity affecting transcript or KDF bytes has been identified before dependency selection is finalized.

## Governing principle

Do not implement cryptographic primitives locally.

The repository may implement protocol composition, transcript sequencing, field encoding, KDF label selection, key-state ownership, and I2P-specific obfuscation wiring, but the primitive operations must come from reviewed Rust crates with acceptable maintenance, licensing, feature, MSRV, zeroization, and constant-time behavior.

A generic Noise implementation is not automatically correct. It may be used only if it exposes enough control to reproduce the exact I2P transcript, prologue, static/ephemeral key handling, obfuscation, padding, and message boundary behavior without forking primitive code or bypassing authenticated state.

## Dependency review and ADR

Create an ADR recording the selected crates and rejected alternatives for:

- X25519;
- ChaCha20-Poly1305;
- SHA-256;
- HMAC-SHA256;
- HKDF-SHA256;
- AES-256 or the exact AES mode used for NTCP2 key obfuscation;
- SipHash behavior required for frame-length obfuscation;
- constant-time comparison;
- zeroization and secret wrappers;
- optional Noise transcript/state support.

For every dependency record:

- exact crate/version requirement;
- enabled and disabled features;
- MSRV evidence against Rust 1.85;
- `unsafe` policy and whether unsafe code is confined inside the reviewed dependency;
- maintenance and release recency;
- license;
- known advisories;
- zeroization guarantees and limitations;
- whether secret-bearing types implement `Clone`, `Debug`, or serde;
- reason the crate is preferable to alternatives.

Prefer existing workspace dependencies when they provide the exact primitive safely. Do not use deprecated `std` hashing APIs when the wire algorithm requires exact specified SipHash behavior.

## Noise integration decision

Evaluate at least two approaches:

### Approach A: reviewed Noise state library

Use a reviewed Noise implementation configured for the exact `Noise_XK_25519_ChaChaPoly_SHA256` pattern, while implementing I2P-specific pre/post-processing around it.

Accept only if the library permits:

- exact prologue and protocol-name bytes;
- deterministic test ephemeral keys;
- access to or reproducible evidence for intermediate transcript hashes/KDF outputs;
- exact static-key ownership and peer-static binding;
- controlled payload boundaries;
- transition into independent transmit/receive cipher states;
- no hidden randomness, timers, socket I/O, or unbounded allocation;
- safe zeroization and teardown.

### Approach B: protocol transcript over reviewed primitives

Implement the Noise symmetric-state sequencing and I2P-specific transcript composition locally while delegating every primitive operation to reviewed crates.

This approach may be selected only when:

- the implementation is narrowly scoped to the single NTCP2 pattern;
- every mix-hash, mix-key, encrypt-and-hash, decrypt-and-hash, split, and nonce transition has fixed vectors;
- no generic cryptographic framework is created;
- the code is reviewed as protocol composition, not a new primitive;
- differential vectors exist against an independent implementation.

Record the selected approach before merging transcript code.

## Secret type inventory

Define protocol-specific secret owners in `i2pr-transport-ntcp2` or reuse a suitable existing crypto wrapper only when semantics match exactly.

Expected secret categories include:

- persistent NTCP2 transport static private key;
- ephemeral X25519 private key;
- Noise chaining key;
- handshake cipher key;
- data-phase transmit and receive keys;
- SipHash frame-length keys or key material;
- IV/obfuscation material where secret;
- intermediate KDF output that remains sensitive;
- serialized transport-static-key record.

Requirements:

- no secret type implements `Clone`, serde, `Display`, or payload-bearing `Debug`;
- secret access is borrowed and narrowly scoped;
- constructors validate exact lengths;
- all-zero X25519 shared secrets are rejected according to library/spec behavior;
- zeroization occurs on drop and on reconstruction failures;
- transcript and key-state objects cannot be reused after a consuming split/transition;
- nonce/counter progression uses checked arithmetic and fails before wrap;
- test-only constructors are clearly isolated and cannot expose production keys through diagnostics.

Public keys, transcript hashes, and tags require typed exact-length wrappers and redacted/default-safe diagnostics.

## NTCP2 static-key persistence

Extend `i2pr-storage` with a separate versioned NTCP2 transport-key record. Do not reuse the router identity private-key record or derive the NTCP2 static key from the RouterIdentity.

Required properties:

- independent X25519 static key generation;
- injected cryptographic RNG;
- fixed or bounded versioned record format;
- integrity protection and public-key rederivation validation;
- `0600` file mode and creation-time private directory policy on Unix;
- atomic no-replace creation;
- strict exact-length reads;
- bounded file size;
- no secret-bearing debug output;
- zeroizing encode/read buffers;
- fail-closed symlink and permission checks consistent with existing identity storage;
- explicit behavior when a key already exists;
- no silent rotation;
- no coupling to address publication.

Decide whether the record shares the existing private identity directory or uses a transport-specific subpath. Record path and migration policy in an ADR or closure record.

Rotation is deferred unless required by the current specification or interoperability. If deferred, document that replacing the key changes the published NTCP2 static key and must be coordinated with RouterInfo publication in a later plan.

## Protocol constants

Create a dedicated constants module with specification-linked values for:

- protocol name and prologue bytes;
- handshake message fixed fields and minimum/maximum lengths needed by cryptographic processing;
- KDF labels and context bytes;
- key, nonce, IV, hash, tag, and public-key lengths;
- AES/obfuscation block sizes;
- timestamp field widths;
- padding bounds that affect transcript processing;
- frame-length key derivation inputs;
- rekey thresholds or nonce bounds needed later.

Each constant must cite the pinned source in a nearby comment or dossier reference. Do not scatter numeric literals across transcript code.

## Pure cryptographic operations

Implement narrowly scoped wrappers for:

- X25519 public-key derivation;
- X25519 shared-secret calculation with invalid/all-zero handling;
- SHA-256 transcript hashing;
- HMAC/HKDF steps with exact labels and output partitioning;
- ChaCha20-Poly1305 seal/open with checked nonce progression;
- AES-based NTCP2 ephemeral-key obfuscation/deobfuscation;
- SipHash-derived frame-length mask preparation if the specification requires it at this stage;
- constant-time tag/public-key checks where applicable;
- Noise symmetric-state transitions selected by the ADR.

The API should expose protocol actions, not generic cryptographic utilities. Avoid public functions such as arbitrary HKDF or generic AEAD encryption.

## Transcript model

Implement a deterministic transcript object or state sequence that can represent the cryptographic progression across the three NTCP2 handshake messages without yet implementing full wire state machines.

Required properties:

- explicit initiator and responder role;
- peer static public key bound at construction where required;
- exact prologue and protocol-name initialization;
- consuming state transitions;
- intermediate transcript hash inspection available only through test/evidence APIs;
- no direct clock, RNG, padding-policy, or socket access;
- deterministic ephemeral/private inputs accepted for tests;
- production constructors require injected RNG or externally generated ephemeral keys;
- failure leaves no partially reusable state;
- split produces distinct transmit and receive key owners and invalidates handshake state.

Plan 032 may implement stage-level helpers for SessionRequest, SessionCreated, and SessionConfirmed cryptographic portions, but it must not claim complete message parsing or handshake state behavior.

## Fixed vector corpus

Create a committed vector corpus under a clearly named path such as:

```text
tests/fixtures/ntcp2/crypto/
```

Use a manifest with:

- fixture ID;
- category;
- source and exact revision;
- independent/local provenance;
- deterministic private/public inputs represented only in test fixtures;
- expected public outputs and intermediate hashes;
- expected error category for malformed vectors;
- SHA-256 of each fixture file;
- licensing/redistribution note;
- generator command or source location.

Never use operational keys, public-network captures, or real peer addresses.

Required positive vectors:

- X25519 public-key derivation and shared secret;
- invalid/all-zero X25519 case;
- AES obfuscation/deobfuscation of the ephemeral public-key field;
- transcript initialization hash;
- every mix-hash and mix-key stage used by all three handshake messages;
- every handshake AEAD seal/open stage;
- final split keys;
- frame-length key/material derivation if defined here;
- deterministic static-key storage encode/decode fixture.

Required malformed/mutation vectors:

- one-bit changes in static and ephemeral public keys;
- wrong prologue/protocol-name input;
- wrong KDF label/context;
- authentication-tag mutation;
- truncated exact-length key/nonce/tag values;
- invalid nonce/counter boundary;
- wrong peer static key;
- storage checksum, version, length, public-key mismatch, and permission errors.

## Independent evidence

At least one set of transcript vectors must be produced independently from the Rust implementation.

Preferred sources:

1. a small deterministic harness around Java I2P NTCP2 internals;
2. a small deterministic harness around i2pd NTCP2 internals;
3. official published vectors if available and redistributable.

Do not copy implementation code into the Rust tests. The independent harness may emit inputs and expected bytes/hashes. Record exact implementation version, build command, patch/harness source, and licensing status.

If intermediate state is not externally accessible, use the smallest permissible test instrumentation and document it. If no independent vector can be produced for a stage, mark that stage local-only and block advancement beyond experimental status.

## Required tests

### Primitive tests

- exact-length constructor acceptance/rejection;
- zeroization/redacted debug compile/runtime assertions where practical;
- X25519 positive and invalid shared-secret cases;
- AEAD success and tag mutation failure;
- nonce/counter exact limit and overflow rejection;
- HKDF output partitioning;
- AES obfuscation round trip and fixed bytes;
- constant-time wrapper result correctness.

### Transcript tests

- deterministic initiator/responder stages produce identical shared transcript state;
- every committed positive vector matches exact bytes;
- every malformed vector returns a typed category;
- consuming transition prevents accidental reuse;
- wrong role or peer key fails;
- final transmit/receive keys cross-match correctly between roles;
- all intermediate secret owners are dropped on failure;
- no secret material appears in debug output.

### Storage tests

- generate/save/load/rederive;
- create-only race;
- Unix creation-time permissions;
- symlink and permissive-directory rejection;
- malformed version/length/checksum/public-key mismatch;
- zeroizing temporary buffers;
- no silent replacement or rotation.

### Dependency tests

- Rust 1.85 compilation;
- no default feature expansion beyond the ADR;
- cargo-deny advisories/bans/sources;
- no Tokio or filesystem use in `i2pr-transport-ntcp2`;
- no local primitive implementation identified by review.

## Fuzzing

Add bounded fuzz targets only for non-secret, parser-like or state-sequence inputs appropriate to this plan, such as:

- exact-length public-key/obfuscation field parsing;
- transcript command sequencing with synthetic keys;
- transport-static-key record decode.

Do not fuzz by logging secret intermediates or retaining generated private corpora. Keep the fuzz workspace separate from production.

## Documentation updates

Update:

- `docs/architecture.md` with crypto/state ownership;
- `docs/security-model.md` with static/ephemeral key compromise, nonce reuse, transcript confusion, all-zero X25519, storage, and diagnostic threats;
- `docs/adr/` with dependency/Noise decision and static-key persistence decision;
- `specs/protocols/03-ntcp2.md` with exact evidence paths and resolved decisions;
- `specs/support.toml` with narrowly scoped experimental, non-advertised crypto/transcript surfaces;
- `AGENTS.md` and `CONTRIBUTING.md` with secret and vector rules;
- fixture manifest/check scripts or add a dedicated NTCP2 vector validator.

## Required local commands

Run the repository-wide gates plus:

```text
cargo test -p i2pr-transport-ntcp2 --all-targets
cargo test -p i2pr-storage --all-targets
bash scripts/check-ntcp2-vectors.sh
cargo +nightly check --manifest-path fuzz/Cargo.toml --offline --all-targets
CARGO_NET_OFFLINE=true bash scripts/fuzz-smoke.sh
```

Create `scripts/check-ntcp2-vectors.sh` or an equivalent deterministic validator that enforces one-to-one manifest/file coverage and hashes.

## Closure record

Create `plans/032-closure.md` containing:

- selected dependencies and feature table;
- Noise integration decision;
- protocol constant inventory;
- secret-owner and zeroization inventory;
- transport-static-key storage format and path policy;
- fixed vector manifest summary;
- independent evidence source and exact versions;
- tests and exact commands;
- CI results or absence;
- support-ledger state;
- unresolved stages;
- Plan 033 handoff prerequisites.

## Acceptance criteria

Plan 032 closes only when:

- every required primitive comes from a reviewed dependency;
- the dependency/Noise ADR is accepted;
- protocol-specific secret wrappers and consuming transitions are implemented;
- static transport keys are independently generated and persist through a hardened versioned store;
- protocol constants are centralized and source-linked;
- deterministic transcript stages and final split have fixed tests;
- at least one independent implementation contributes reproducible transcript evidence;
- malformed and one-bit mutation cases fail with typed errors;
- no secret appears in debug, tracing, snapshots, fixtures from operational sources, or panic text;
- vector and fixture integrity is mechanically checked;
- MSRV, dependency-policy, fuzz compilation, and workspace quality gates pass;
- no complete-handshake, data-phase, socket, or support claim is introduced;
- `plans/032-closure.md` exists.

## Stop conditions

Stop and record the conflict if:

- no reviewed crate supports a required primitive under Rust 1.85;
- generic Noise behavior cannot reproduce exact I2P transcript bytes;
- transcript vectors disagree between official specification, Java I2P, and i2pd;
- independent evidence cannot be produced for security-critical KDF or split stages;
- correct behavior requires copying cryptographic primitive code;
- zeroization or secret ownership cannot be made explicit;
- transport-static-key persistence would require silent key replacement;
- a proposed dependency expands network/runtime features into the protocol crate;
- any fixture would contain operational key material or non-redistributable captures.