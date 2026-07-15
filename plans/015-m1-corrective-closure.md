# Milestone 1 corrective and aggregate closure plan

## Purpose

Close the remaining Milestone 1 gaps before asynchronous runtime or transport work begins. This pass is limited to strengthening the existing protocol, cryptographic, storage, evidence, and documentation foundation. It must not add sockets, reseeding, NetDB behavior, tunnel state machines, runtime supervision, or capability advertisement.

Milestone 1 is implementation-complete but not yet eligible for unconditional aggregate closure. The existing Plan 011–014 closure records remain valid evidence for their bounded changes. This plan adds the corrective work needed to make their combined result a durable handoff to Milestone 2 and Milestone 3.

## Governing material

Read before implementation:

- `GUARDRAILS.md`
- `plans/010-milestone-1-overview.md`
- `plans/011-m1-codec-foundation.md`
- `plans/012-m1-common-structures.md`
- `plans/013-m1-identity-crypto-storage.md`
- `plans/014-m1-i2np-evidence-fuzzing-closure.md`
- `plans/011-closure.md` through `plans/014-closure.md`
- `docs/architecture.md`
- `docs/security-model.md`
- `docs/protocol-support.md`
- `specs/README.md`
- `specs/CONFORMANCE.md`
- `specs/SOURCES.md`
- `specs/support.toml`
- the common-structure and I2NP dossiers under `specs/protocols/`

Specification ambiguity must be recorded and left unsupported. Do not resolve ambiguity by copying another router's implementation.

## Corrective findings

This plan addresses five bounded findings:

1. Temporary private-key and serialized identity buffers are not consistently zeroized after use.
2. Secret-bearing DatabaseLookup reply material is structurally modeled as freely cloneable ordinary arrays and vectors.
3. The committed I2NP fixture corpus is too narrow to support unconditional Milestone 1 evidence closure.
4. `common.rs` and `i2np.rs` have grown into large multi-domain modules that should be decomposed before concurrent transport and NetDB development.
5. The roadmap, support matrix, and machine-readable support ledger contain minor milestone and scope wording drift.

It also hardens Unix identity-directory creation so private directories are restrictive from creation rather than hardened only after creation.

## Constraints

- Preserve wire behavior and public semantics unless a documented defect requires correction.
- Preserve the six existing production crates. Do not add a runtime, transport, NetDB, tunnel, client, API, or service-tunnel crate in this pass.
- Avoid broad new traits or universal secret-container frameworks.
- No private keys, live identities, peer addresses, destinations, or operational captures may enter fixtures.
- No protocol status may become `implemented` or `advertised = true` without the conformance evidence required by `specs/CONFORMANCE.md`.
- Keep normal and Rust 1.85 MSRV checks green.

## Workstream A: transient secret-memory hygiene

### A1. Inventory secret copies

Document every current location that owns or copies:

- Ed25519 private seeds;
- X25519 private keys;
- serialized private identity bytes;
- private identity file read buffers;
- derived private-key reconstruction arrays;
- DatabaseLookup reply keys and session tags.

The inventory belongs in the aggregate closure record. It must distinguish durable secret owners, temporary secret owners, public values, integrity hashes, and protocol tags whose disclosure would weaken reply confidentiality.

### A2. Generation and reconstruction

Refactor `i2pr-crypto` so temporary generation and reconstruction buffers are zeroized on success and failure.

Required behavior:

- Use narrow zeroizing ownership such as `zeroize::Zeroizing<[u8; N]>` or an equivalent explicit drop guard.
- Avoid copying a private seed merely to move it between constructors.
- Preserve injected `TryCryptoRng` behavior and deterministic testability.
- Preserve non-`Debug`, non-`Display`, non-serde private wrappers.
- Do not add `Clone` to private keys or bundles.
- Ensure randomness failure wipes every partially filled buffer.

API changes are allowed where they reduce copies, but keep the API concrete to the selected type-7/type-4 identity profile.

### A3. Storage encoding and decoding

Refactor `i2pr-storage` so buffers containing private material are zeroized after use.

Required behavior:

- The serialized identity buffer used for writing must zeroize on all return paths.
- The file-read buffer must zeroize after decode, including malformed and integrity-failure paths.
- Private arrays extracted during decode must be zeroizing owners or transferred without leaving unwiped duplicate arrays.
- Error values must not retain raw bytes.
- Existing strict length, checksum, public-key rederivation, version, and create-only behavior must remain unchanged.

Do not claim that ordinary heap zeroization defeats process compromise, swap inspection, core dumps, allocator copies, or an attacker controlling the parent directory. Record these limitations explicitly.

### A4. DatabaseLookup reply material

Prevent reply keys and tags from becoming ordinary freely cloneable protocol values before NetDB work begins.

Implement the smallest workable design satisfying all of the following:

- reply key material zeroizes on drop;
- default `Debug` remains redacted;
- secret-bearing values do not implement `Clone` unless an explicit security justification is recorded;
- decoding does not leave unnecessary duplicate buffers;
- encoding borrows the secret for the shortest practical duration;
- the containing I2NP types no longer derive `Clone` merely for test convenience if that would clone secrets.

A narrow internal or protocol-facing sensitive-byte wrapper is acceptable. It must not become a general cryptographic provider, serialization framework, or cross-project secret-management abstraction.

Add compile-time or source-level tests where practical to prevent accidental revealing formatting or cloning from returning unnoticed.

## Workstream B: identity-directory creation hardening

On Unix, create new identity directories with restrictive permissions from inception.

Required behavior:

- Prefer `std::os::unix::fs::DirBuilderExt::mode(0o700)` or an equivalent safe standard-library path.
- Nested directory creation must not silently create intermediate components with broader modes than the documented threat model permits.
- Existing directories must still be rejected when group or world permission bits are present.
- Symlink and non-directory rejection must remain fail-closed.
- Tests must cover new-directory creation, existing private directories, permissive directories, and symlink paths.

If safe recursive creation cannot be achieved with the current standard-library-only design, stop and choose one of these explicit policies:

1. require the parent data directory to exist and be private; or
2. add a narrowly reviewed filesystem dependency through an ADR.

Do not retain a check-then-chmod window while claiming creation-time privacy.

## Workstream C: protocol module decomposition

Split the large source files into internal modules while preserving stable public exports.

Target shape, adjustable only with documented justification:

```text
crates/i2pr-proto/src/
  codec/
    mod.rs
    cursor.rs
    encoder.rs
    error.rs
  common/
    mod.rs
    date.rs
    mapping.rs
    certificate.rs
    keys.rs
    identity.rs
    router_info.rs
    lease.rs
  i2np/
    mod.rs
    header.rs
    netdb.rs
    delivery.rs
    tunnel.rs
    deferred.rs
```

Requirements:

- Decomposition must not weaken visibility boundaries merely to make modules compile.
- Keep internal decode/encode helpers private or `pub(crate)`.
- Preserve exact signed-byte handling.
- Preserve caller-visible bounds and strict top-level decoding.
- Keep secret handling out of unrelated modules.
- Avoid a generic `WireCodec` trait unless a concrete duplicate implementation cannot otherwise be removed.
- Existing public imports from `i2pr_proto` should remain source-compatible where practical.

Add module-level documentation identifying ownership and explicitly deferred semantics.

## Workstream D: fixed vectors and malformed evidence

Expand ordinary committed fixtures beyond the current DeliveryStatus example. Fuzz corpora remain fuzz inputs and do not substitute for fixed evidence.

### Required positive vectors

Add locally authored or independently generated fixed bytes for at least:

- standard DeliveryStatus;
- obsolete SSU short envelope;
- NTCP2/SSU2 short envelope;
- DatabaseLookup without encrypted reply;
- DatabaseLookup with legacy reply-key/tag framing;
- DatabaseLookup with ECIES reply-key/tag framing;
- DatabaseSearchReply;
- DatabaseStore with classic LeaseSet framing;
- DatabaseStore compressed-RouterInfo framing without decompression claims;
- TunnelData;
- TunnelGateway with nested standard I2NP message;
- variable tunnel-build framing;
- short tunnel-build framing;
- Garlic deferred length framing;
- Data deferred length framing.

### Required malformed vectors

Include representative fixtures for:

- truncated standard header;
- checksum mismatch;
- declared payload length larger than available bytes;
- trailing bytes;
- unknown message type;
- DatabaseLookup invalid flag combination;
- zero or excessive reply-tag count;
- excessive DatabaseLookup exclusions;
- excessive DatabaseSearchReply peer count;
- zero tunnel ID where forbidden;
- invalid tunnel-data length;
- zero or excessive build-record count;
- malformed nested TunnelGateway message;
- maximum-plus-one deferred payload.

### Fixture requirements

Each fixture manifest entry must record:

- stable identifier and path;
- positive or negative classification;
- expected decoded type or expected error category;
- source and exact specification revision;
- generator and deterministic input when generated;
- license or redistribution note;
- SHA-256 hash;
- whether the bytes were independently produced or locally authored.

Tests must consume the committed fixtures. Do not add dead fixture files that are only hash-checked.

### Independent evidence

Attempt to obtain at least one independently produced common-structure or I2NP vector from an official specification example, reference test tool, Java I2P/I2P+, or i2pd without copying implementation code.

If no suitable redistributable vector is available, record the search and retain the status as experimental. Lack of an independent vector does not block this corrective pass, but it blocks any interoperability or full implementation claim.

## Workstream E: property and regression coverage

Add focused properties rather than generic round-trip assertions alone:

- canonical encode is deterministic;
- decode of canonical fixed bytes succeeds;
- re-encoding decoded canonical bytes returns the same bytes where the format is canonical;
- appended bytes fail strict decoders;
- every truncation prefix of selected compound fixtures fails without panic;
- unsupported identifiers never map to a supported default;
- mutation of exact RouterInfo signed bytes invalidates verification;
- private storage round trips preserve identity and clear transient serialization buffers through test-only instrumentation where practical;
- reply-secret debug output contains lengths/types only;
- maximum and maximum-plus-one inputs produce stable error categories.

Do not expose production-only introspection hooks merely to observe zeroization. Test narrow owner/drop behavior and conduct source review where direct observation would require unsafe code.

## Workstream F: documentation and ledger reconciliation

Correct documentation drift in the same implementation sequence.

Required updates:

- Align `docs/protocol-support.md` milestone numbers with `plans/000-mvp-roadmap.md`: minimal streaming belongs to Milestone 6, SAM to 7, SSU2 to 8, I2CP to 9, and service tunnels to 10.
- Narrow the machine-readable LeaseSet surface to the exact classic LeaseSet structural subset.
- Keep LeaseSet2, EncryptedLeaseSet, and MetaLeaseSet entries explicitly deferred.
- Record that DatabaseLookup reply-secret wrappers provide memory hygiene, not completed encrypted-reply semantics.
- Update architecture documentation after module decomposition and secret-owner changes.
- Update the security model with transient-copy, allocator, core-dump, swap, filesystem-parent, and non-Unix limitations.
- Update `AGENTS.md` and `CONTRIBUTING.md` with fixture and sensitive-value rules if their existing guidance is insufficient.

No support row may be marked `implemented`, interoperable, or advertised solely because this correction adds fixtures or zeroization.

## Aggregate Milestone 1 closure record

Create `plans/010-milestone-1-closure.md` after all corrective work is complete.

The record must include:

- commits completing Plans 011–015;
- final crate and module graph;
- public API inventory;
- exact implemented structural surfaces;
- selected cryptographic algorithms and dependency versions;
- private identity format and filesystem policy;
- secret-owner and transient-copy inventory;
- positive and malformed fixture inventory;
- independent-evidence result;
- fuzz target and seed-corpus inventory;
- local and CI command results;
- deviations from every Milestone 1 plan;
- unresolved specification ambiguities;
- exact non-claims and deferred work;
- explicit prerequisites for Milestones 2 and 3.

The individual closure records remain linked as detailed evidence. The aggregate closure must not duplicate them verbatim.

## Validation matrix

Run at minimum:

```text
cargo fmt --all --check
cargo check --workspace
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-fixture-manifest.sh
cargo deny check advisories bans sources
cargo +1.85.0 check --workspace --all-targets
cargo +nightly check --manifest-path fuzz/Cargo.toml --all-targets
bash scripts/fuzz-smoke.sh
```

Also run:

- tests on Linux and macOS CI;
- the identity storage suite on a Unix filesystem;
- targeted output inspection for secret-bearing error and debug paths;
- a bounded fuzz smoke run for every maintained target;
- `git diff --check`;
- a repository scan confirming no private fixtures or generated crash artifacts were committed.

Record exact toolchain versions and any offline-mode limitations.

## Acceptance criteria

This plan is complete only when:

- temporary private-key and private-file buffers have explicit zeroizing ownership;
- DatabaseLookup reply secrets are not ordinary freely cloneable values;
- Unix identity directories are restrictive at creation or the implementation adopts a stricter documented parent-directory policy;
- protocol modules are decomposed without public-behavior regression;
- every currently typed I2NP surface has fixed positive and representative malformed evidence;
- every fixture is consumed by tests and has validated provenance metadata;
- roadmap, support matrix, and support ledger agree on milestone and exact surface scope;
- the aggregate Milestone 1 closure record exists;
- all normal, MSRV, dependency, fixture, documentation, and fuzz-smoke gates pass;
- no network behavior or capability advertisement was introduced.

## Stop conditions

Stop and report rather than improvising if:

- secret zeroization requires unsafe code;
- module decomposition changes signed or checksummed byte regions;
- a fixed vector disagrees with the pinned specification;
- independent evidence conflicts with the local codec;
- safe restrictive directory creation cannot be achieved under the documented filesystem model;
- removing secret-bearing `Clone` exposes an ownership problem that requires NetDB state-machine design;
- tests pass only after relaxing strict decoding, bounds, or error classification;
- a dependency addition would violate MSRV or dependency policy.

## Handoff

The implementation handoff must identify changed APIs, sensitive-value ownership, fixture coverage, module movement, documentation corrections, commands/results, CI run, remaining non-claims, and the exact commit that establishes aggregate Milestone 1 closure.