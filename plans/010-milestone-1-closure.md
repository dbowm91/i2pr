# Aggregate Milestone 1 closure: corrective handoff

## Scope and evidence

This record aggregates Plans 011–015. Plans 011–014 remain the detailed
records for their original bounded changes; this file records the corrective
delta and the final handoff boundary. Plan 015 was limited to protocol,
crypto, storage, evidence, and documentation hygiene. It adds no sockets,
reseed, NetDB behavior, tunnel state machine, runtime supervision, client/API
surface, or capability advertisement.

The completing commits are the implementation and CI-evidence commits listed
in [`plans/011-closure.md`](011-closure.md),
[`plans/012-closure.md`](012-closure.md),
[`plans/013-closure.md`](013-closure.md), and
[`plans/014-closure.md`](014-closure.md), followed by the Plan 015 corrective
commit recorded below after validation. The earlier records remain linked
evidence rather than being duplicated here.

## Plan 015 changed files

- `crates/i2pr-crypto/src/lib.rs`, `crates/i2pr-storage/src/lib.rs`, and the
  two crate manifests/lockfiles: zeroizing ownership, storage buffers, and
  creation-time directory policy.
- `crates/i2pr-proto/src/lib.rs`, `src/common_impl.rs`, `src/common/`,
  `src/i2np_impl.rs`, `src/i2np/`, and `tests/i2np_fixtures.rs`: grouped
  protocol namespaces, non-cloneable reply secrets, early build-count checks,
  and fixture-backed regressions.
- `tests/fixtures/i2np/*.hex`, `manifest.tsv`, `README.md`, and
  `scripts/check-fixture-manifest.sh`: the positive/malformed evidence corpus
  and metadata validation.
- `README.md`, `AGENTS.md`, `CONTRIBUTING.md`, `docs/architecture.md`,
  `docs/security-model.md`, `docs/protocol-support.md`, ADRs 0005–0007,
  `specs/SOURCES.md`, `specs/protocols/01-common-identity-crypto.md`,
  `specs/support.toml`, and Plans 012/014: reconciled current guidance and
  evidence paths.
- This aggregate record: closure evidence, deviations, risks, and handoff
  prerequisites.

## Final crate and module graph

The six production crates remain:

```text
i2pr-proto <- i2pr-crypto <- i2pr-storage
     ^              ^               ^
     |              |               |
 i2pr-core <------ i2pr-daemon  (composition root)
     ^
     |
 i2pr-testkit (test/simulation dependency only)
```

`i2pr-proto` retains its crate-root public re-export façade. Its internal
ownership namespaces are `codec/`, `common/` (`date`, `keys`, `mapping`,
`certificate`, `identity`, `router_info`, `lease`), and `i2np/` (`header`,
`netdb`, `delivery`, `tunnel`, `deferred`). The private implementation glue
in `common_impl.rs` and `i2np_impl.rs` retains helper visibility and exact
decode/encode behavior while the grouped modules make domain ownership
explicit. No generic universal wire-codec or secret-management framework was
introduced.

## Public API inventory

- `i2pr-proto`: bounded `DecodeCursor`, `EncodeBuffer`, `CodecError`, exact
  decode/encode helpers; common dates, hashes, mappings, certificates,
  algorithm/key values, identities, addresses, RouterInfo, Lease, and classic
  LeaseSet; I2NP headers, typed bodies, deferred/opaque framing, and the
  non-cloneable `ReplySecret<N>` wrapper.
- `i2pr-crypto`: type-7 Ed25519 and type-4 X25519 private/public wrappers,
  injected RNG generation/reconstruction, RouterInfo signing/verification,
  SHA-256, constant-time comparison, and the explicit identity bundle.
- `i2pr-storage`: version-1 fixed-format `IdentityStore`, bounded load,
  create-only atomic install, permission validation, and fail-closed errors.
- `i2pr-daemon`: explicit non-networked `identity generate` and `identity
  inspect` lifecycle commands plus side-effect-free config/dry-run behavior.
- `i2pr-core` and `i2pr-testkit`: runtime-neutral lifecycle/resource
  vocabulary and deterministic test helpers from Milestone 0.

## Exact implemented structural surfaces

The supported subset is experimental local structural evidence only:

- common primitive, mapping, certificate, key-certificate, RouterIdentity,
  Destination, RouterAddress, RouterInfo, Lease, and classic LeaseSet codecs;
- type-7 Ed25519 signing/verification and type-4 X25519 public-key derivation;
- version-1 private identity storage with SHA-256 integrity, strict lengths,
  public-key rederivation, atomic no-replace installation, and Unix file mode
  `0600`;
- I2NP message identifiers, standard header, obsolete SSU short header, and
  NTCP2/SSU2 short header with bounded lengths/checksum validation;
- structural DatabaseLookup, DatabaseSearchReply, DeliveryStatus, and
  DatabaseStore framing; fixed TunnelData and nested TunnelGateway framing;
  variable/short tunnel-build record framing; and bounded Garlic/Data deferred
  lengths;
- zeroizing, redacted, non-cloneable DatabaseLookup reply keys/tags. This is
  memory hygiene, not encrypted reply semantics.

`specs/support.toml` and `docs/protocol-support.md` remain non-advertised and
experimental. LeaseSet2, EncryptedLeaseSet, MetaLeaseSet, compression/
decompression, encrypted replies, garlic, tunnel cryptography, routing,
duplicate/expiry policy, transport authentication, NetDB behavior, and all
mixed-router interoperability remain deferred.

## Algorithms, dependencies, and storage policy

The generated identity profile is I2P signature type 7 (Ed25519) plus router
encryption type 4 (X25519), bound by a type-5 key certificate. The reviewed
direct dependency choices and versions remain in ADR 0005: `ed25519-dalek`
2.2, `x25519-dalek` 2.0.1, `zeroize` 1.9 lock family, `subtle` 2.6,
`rand_core` 0.9, and workspace `sha2` 0.10, with default features restricted
as documented there.

The private identity record is exactly 184 bytes: a 24-byte fixed header, 128
bytes of two private seeds and their derived public keys, and a 32-byte
SHA-256 integrity value. New Unix identity directories are created with
`DirBuilderExt::mode(0o700)` and only the final component is created; missing
intermediate parents are not recursively created. Existing symlinks,
non-directories, and group/world-permissive directories fail closed. The
identity file and temporary write file use `0600`; install is atomic and
no-replace. This is permission/integrity protection, not encryption at rest.

## Secret-owner and transient-copy inventory

| Material | Owner/copy | Classification and disposal |
| --- | --- | --- |
| Ed25519 seed | `SigningPrivateKey(Zeroizing<[u8; 32]>)` | Durable secret owner; no `Debug`, `Display`, `Clone`, or serde; zeroizes on drop |
| X25519 seed | `EncryptionPrivateKey(Zeroizing<[u8; 32]>)` | Durable secret owner; same restrictions and drop behavior |
| RNG output/reconstruction arrays | `Zeroizing<[u8; 32]>` consumed by wrapper construction | Temporary secret owners; the compatibility-preserving fixed-array constructor copy is followed by zeroization of the source on every return path |
| Dalek temporary signing/static-secret values | Reviewed library-owned temporary values | Library zeroization support; public outputs contain only public material/signatures |
| Serialized private identity | `Zeroizing<Vec<u8>>` from encode through write | Temporary private serialization; zeroizes on success and every return path |
| File-read identity buffer | `Zeroizing<Vec<u8>>` through strict decode | Zeroizes on malformed, integrity-failure, and successful load paths |
| Decoded private/public/checksum arrays | `Zeroizing<[u8; N]>` reader owners | No ordinary array remains after decode; private arrays transfer into crypto wrappers |
| DatabaseLookup reply key/tag | `ReplySecret<N>(Zeroizing<[u8; N]>)` | Protocol secret/tag; non-cloneable, redacted `Debug`, borrowed only during encoding |
| Hashes, public keys, signatures, integrity digests | Ordinary typed public/integrity values | Not private secret owners; debug output remains redacted where needed |
| Fixture bytes | Hex protocol shapes only | No private keys, live identities, peer addresses, destinations, or captures |

These controls do not defeat process compromise, allocator copies, swap,
hibernation, core dumps, crash reporters, process snapshots, or an attacker
with parent-directory write access. Non-Unix permission and durability
semantics remain platform limitations.

## Fixed vectors and malformed evidence

`tests/fixtures/i2np/manifest.tsv` now contains 15 positive and 16 malformed
locally authored I2NP fixtures. Positive coverage includes standard Delivery-
Status, obsolete SSU and NTCP2/SSU2 short headers, all three DatabaseLookup
reply layouts, DatabaseSearchReply, classic LeaseSet and compressed-RouterInfo
DatabaseStore framing, TunnelData, nested TunnelGateway, variable/short build
framing, and Garlic/Data deferred length framing. Malformed coverage includes
header truncation, checksum/length/trailing/type failures, invalid lookup
flags/tag/exclusion limits, excessive search peers, tunnel ID/length failures,
zero/excess build counts, malformed nested messages, and maximum-plus-one
deferred length.

Every manifest row records classification, SHA-256, official source and exact
revision, local deterministic generator/input, expected decoded type or error
category, license note, and local/independent provenance. The fixture-backed
integration tests consume every entry, re-encode every positive canonical
value, test selected truncation prefixes, and assert typed malformed errors.

An independent-vector search covered official examples and the pinned Java
I2P/I2P+/i2pd/Emissary evidence sources without copying implementation code.
No suitable redistributable binary vector was identified for this corrective
pass. The result is recorded as experimental; no interoperability or full
implementation claim follows from these local fixtures.

## Fuzz inventory

The existing separate nightly fuzz workspace retains 17 bounded targets for
common decoders, all three direct I2NP header variants, and complex I2NP body
dispatch. Its seed corpus remains locally authored/provenance-recorded,
sanitized, hashed, and free of private material or operational captures.
Fuzzing is optional and non-production; `scripts/fuzz-smoke.sh` is the bounded
local lane.

## Quality and CI evidence

The final local command results are recorded below after the corrective tree
was formatted and tested:

| Command | Result |
| --- | --- |
| `rtk cargo fmt --all --check` | passed |
| `rtk cargo check --workspace` | passed |
| `rtk cargo check --workspace --all-targets` | passed |
| `rtk cargo test --workspace` | passed — 80 tests |
| `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings` | passed |
| `RUSTDOCFLAGS="-D warnings" rtk cargo doc --workspace --no-deps` | passed |
| `rtk bash scripts/check-dependency-direction.sh` | passed |
| `rtk bash scripts/check-fixture-manifest.sh` | passed after final fixture hash update |
| `rtk cargo deny check advisories bans sources` | passed; existing duplicate `rand_core` 0.6/0.9 warning |
| `rtk cargo +1.85.0 check --workspace --all-targets` | passed |
| `rtk cargo +nightly check --manifest-path fuzz/Cargo.toml --offline --all-targets` | passed |
| `CARGO_NET_OFFLINE=true rtk bash scripts/fuzz-smoke.sh` | passed — 17 targets × 32 runs |
| GitHub Actions CI ([run 29399571987](https://github.com/dbowm91/i2pr/actions/runs/29399571987)) | passed — Ubuntu quality, MSRV, macOS quality, and dependency policy |

## Deviations and unresolved ambiguities

- The requested namespace split uses private compatibility implementation
  units plus grouped leaf façades rather than widening helpers or introducing
  a brittle universal trait. This is a deliberate visibility-preserving
  deviation; future work can move individual implementations when concrete
  ownership contracts are stable.
- Recursive identity-directory creation was not retained. The standard-library
  policy requires an existing parent and creates only the final directory with
  restrictive mode, eliminating create-then-chmod and intermediate-component
  exposure.
- Reply-secret wrappers provide zeroization/redaction only. The I2NP combined
  key-derivation mode remains unsupported because its pinned format is
  ambiguous.
- No independent redistributable binary vectors were found. This blocks
  interoperability/full-support claims but does not block this corrective
  structural closure.
- LeaseSet2, EncryptedLeaseSet, MetaLeaseSet, freshness, replay/duplicate
  policy, routing, transport authentication, NetDB actions, tunnel crypto,
  garlic decryption, and capability advertisement remain unresolved later-plan
  work rather than guessed here.

## Milestone 2 and 3 prerequisites

Before Milestone 2 closes, runtime-neutral lifecycle/resource contracts must be
connected to supervised bounded services with deterministic cancellation and
testkit fault injection. Before Milestone 3 transport work begins, NTCP2/SSU2
plans must define authenticated framing, network-ID/replay policy, queue and
session budgets, and private mixed-router testnet evidence. Neither milestone
may treat this structural codec or local identity evidence as transport
interoperability.

## Plan 015 completing commit

The validated implementation was committed as `97e216e` (`Close Milestone 1
corrective plan`) and pushed to `main`. GitHub Actions run
[29399571987](https://github.com/dbowm91/i2pr/actions/runs/29399571987) passed
the Ubuntu quality, MSRV, macOS quality, and dependency-policy jobs. This
closure-record update is the follow-up CI-evidence commit.
