# Plan 032 closure: NTCP2 crypto transcript and vectors

## Status

The bounded Plan 032 crypto/transcript foundation is implemented and locally
validated. It remains experimental and non-advertised. This closure does not
claim a complete handshake, data phase, sockets, runtime integration, Java I2P
interoperability, i2pd interoperability, or public-network operation.

## Implemented scope

- `i2pr-crypto` now owns zeroizing `X25519PrivateKey` and
  `X25519SharedSecret` wrappers with injected generation and all-zero DH
  rejection. `TransportStaticKey` is a semantic storage alias only.
- `i2pr-transport-ntcp2` owns source-linked constants, typed public/hash/error
  values, checked ChaCha20-Poly1305 cipher states, AES-256-CBC ephemeral-key
  state, SipHash-2-4 length state, and a consuming role-aware transcript for
  SessionRequest, SessionCreated, and SessionConfirmed cryptographic portions.
- SessionConfirmed part one uses the retained SessionRequest cipher owner at
  nonce 1 as required by the current NTCP2 specification; `split` consumes the
  transcript and maps `k_ab`/`k_ba` by role.
- `i2pr-storage` owns a separate version-1 `ntcp2.static.key` record with an
  independently generated X25519 static key, rederived public key, IV,
  checksum, strict decoding, create-only atomic installation, and permission
  and symlink checks.

## Dependency and Noise decision

ADR 0011 selects the narrow protocol-composition approach over a generic Noise
state library. Direct dependency choices are:

| Crate | Version | Features | Role |
| --- | --- | --- | --- |
| `x25519-dalek` | 2.0.1 | `static_secrets`, `zeroize` | X25519 through `i2pr-crypto` |
| `sha2` | 0.10 workspace | workspace defaults | SHA-256 |
| `hmac` | 0.12.1 | none | HMAC-SHA256 KDF |
| `chacha20poly1305` | 0.10.1 | `alloc` | ChaCha20-Poly1305 |
| `aes` | 0.8.4 | none | AES-256 block primitive |
| `siphasher` | 1.0.3 | default features disabled | SipHash-2-4 |
| `subtle` | 2.6 workspace | none | constant-time equality |
| `zeroize` | 1.8 workspace | `derive` | secret owners and buffers |

No generic `noise-protocol` or `hkdf` API was added. NTCP2's exact HMAC
sequencing and labels are protocol composition over reviewed primitives.

## Constant and secret inventory

`crates/i2pr-transport-ntcp2/src/constants.rs` centralizes the protocol name,
empty prologue, 32-byte keys/hashes, 12-byte nonce, 16-byte tags/IVs, 65535
frame ceiling, SessionConfirmed limits, current Java non-PQ padding ceilings,
nonce maximum, and `ask`/`siphash` labels. Comments and this record point to
the pinned `specs/protocols/03-ntcp2.md`/official specification.

Secret owners are `X25519PrivateKey`, `X25519SharedSecret`, `AeadKey`,
`ChainKey`, `CipherState`, and the private portions of `Transcript` and
`SplitKeys`. They do not implement `Clone`, `Debug`, `Display`, serde, or
payload-bearing formatting. Public keys and transcript hashes are typed and
diagnostics are redacted. Cipher nonces use checked arithmetic and reject
before the forbidden `2^64 - 1` value.

## Static-key storage

The path is `<private router data directory>/ntcp2.static.key`, separate from
`router.identity`. Version 1 is a fixed 132-byte record:

```text
magic(8) version(2) reserved(2) algorithm(2)
private_len(2) public_len(2) iv_len(2)
private_x25519(32) public_x25519(32) iv(16) sha256(32)
```

The checksum covers every preceding byte. Reads require exact length, bounded
file metadata, regular non-symlink files, private Unix modes, a secure parent
directory, valid lengths/algorithm/version, checksum, and public-key
rederivation. Writes use a 0600 temporary file, sync, atomic no-replace
hard-link installation, cleanup, and directory sync. Existing material is
never replaced and no rotation is silently performed.

## Vector corpus and independent evidence

The corpus is under `tests/fixtures/ntcp2/crypto/`:

- `vectors.tsv` — X25519 public/shared values, protocol/initial/final hashes,
  all three handshake-stage AEAD values, AES-CBC X/Y values, and split KDF
  outputs. It was generated independently with Python `cryptography` 41.0.7,
  hashlib, and HMAC from synthetic fixed inputs.
- `storage-static-key.hex` — deterministic version-1 storage bytes from the
  seed-25 format generator.
- `manifest.tsv` — exact SHA-256 hashes and provenance.

`scripts/check-ntcp2-vectors.sh` enforces one-to-one manifest/file coverage,
hashes, required rows, categories, and provenance. The fixture-backed Rust
tests consume all primitive, transcript-stage, split, and storage values.
No operational key, peer address, or network capture is present.

## Tests and evidence

Focused checks completed during this closure:

```text
cargo fmt --all --check
cargo test --offline -p i2pr-transport-ntcp2 --all-targets
cargo test --offline -p i2pr-storage --all-targets
cargo test --offline -p i2pr-crypto --all-targets
bash scripts/check-ntcp2-vectors.sh
```

Repository-wide checks also passed locally:

```text
cargo check --offline --workspace --all-targets
cargo test --offline --workspace                 # 161 tests
cargo clippy --offline --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --offline --workspace --no-deps
cargo +1.85.0 check --offline --workspace --all-targets
cargo deny check advisories bans sources
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
cargo +nightly check --manifest-path fuzz/Cargo.toml --offline --all-targets
CARGO_NET_OFFLINE=true bash scripts/fuzz-smoke.sh
```

The fuzz smoke script exercised the existing parser targets plus the new
`ntcp2_transcript` and `ntcp2_storage` targets. No CI result is claimed by
this local closure.

## Support-ledger state

The machine-readable ledger adds only experimental, `advertised = false`
surfaces for the NTCP2 cryptographic foundation, transcript composition, and
transport static-key storage. The protocol-support matrix continues to state
that complete NTCP2 support is not implemented. No RouterInfo address,
capability, or daemon behavior is changed.

## Unresolved and deferred work

- Plan 033 must add explicit initiator/responder wire state machines and consume
  this transcript without reimplementing KDF stages.
- Plan 033/034 must add exact message/RouterInfo/block/frame parsing, bounded
  padding policy, and data-phase rekey behavior.
- Plan 035 must integrate admission, ownership, deadlines, duplicate-link
  policy, runtime I/O, and cleanup.
- Plan 036 must produce authorized mixed-router evidence with Java I2P and
  i2pd and may reopen this plan for any transcript discrepancy.
- Static-key/IV rotation, RouterInfo publication, and migration remain
  deferred; replacing either material changes the published address contract.
- Fuzz campaigns beyond the bounded smoke run remain deferred; the committed
  targets cover transport-static-key decoding and synthetic transcript
  commands without operational keys, filesystem access, or network access.

## Plan 033 handoff prerequisites

Plan 033 may consume `Transcript` role-specific stages, `PublicKeyBytes`,
`CipherState`, `SplitKeys`, and the source-linked constants. It must preserve
consuming transitions, the message-1 cipher-state owner rule, all-zero DH
rejection, checked nonces, exact padding hash order, typed authentication
errors, and the experimental/non-advertised support state until mixed-router
evidence exists.
