# Milestone 1 Plan B closure: common structures and canonical protocol model

## Scope and outcome

This record closes the structural portion of
`plans/012-m1-common-structures.md`. `i2pr-proto` now has bounded, strict
codecs and immutable validated values for the common data required by later
plans. The implementation does not open sockets, perform network operations,
generate secrets, sign or verify records, apply NetDB freshness policy, or
advertise a RouterInfo capability.

Implemented values are:

- `Date`, `Date32`, `Hash`, typed signing/encryption algorithm identifiers,
  public key material, and typed signatures;
- canonical duplicate-free `Mapping` values and bounded protocol-version and
  capability wrappers;
- certificates and key certificates with algorithm-specific length checks;
- `KeyAndCert`, `RouterIdentity`, and `Destination`, including SHA-256 hash
  derivation over the complete canonical encoding;
- `RouterAddress`, `RouterInfo`, and `Lease`, with exact RouterInfo signed-byte
  retention;
- classic `LeaseSet`, with exact signed-byte retention and a typed rejection
  path for LeaseSet2, MetaLeaseSet, and EncryptedLeaseSet variants.

## Changed files

- `Cargo.toml`, `Cargo.lock`, and `crates/i2pr-proto/Cargo.toml` — add the
  reviewed `sha2` dependency only for SHA-256 hash derivation.
- `crates/i2pr-proto/src/codec.rs` — preserve the bounded codec API, make debug
  output redact input/output contents, distinguish error categories, make
  length-prefixed emission preflight its complete field, and add an internal
  raw-byte append used by preserved signed regions.
- `crates/i2pr-proto/src/lib.rs` — export common structures and the expanded
  error taxonomy.
- `crates/i2pr-proto/src/common.rs` — implement the Plan 012 structural model,
  canonical encoders, strict decoders, limits, and unit tests.
- `README.md`, `AGENTS.md`, and `docs/architecture.md` — describe the new
  structural boundary and its non-claiming status.
- `docs/protocol-support.md`, `specs/support.toml`, `specs/SOURCES.md`, and
  `specs/protocols/01-common-identity-crypto.md` — record exact evidence,
  source traceability, and known limitations.

## Limits and security decisions

- Every public top-level decoder and encoder receives a caller-visible maximum;
  the common model documents a 1 MiB maximum used by its convenience methods.
- Mapping bodies are limited to 65,535 bytes, individual mapping strings to
  255 bytes, RouterInfo addresses and peers to 255 entries, and classic leases
  to 16. No hidden unbounded allocation policy was introduced.
- Mappings are represented as immutable sorted vectors. Duplicate keys,
  malformed separators, noncanonical order, invalid UTF-8, and trailing bytes
  are rejected without silent overwrite.
- Algorithm identifiers have explicit `Unknown` variants, but unknown or
  unsupported algorithms fail before type-dependent key allocation. Key and
  signature lengths are checked before values are constructed.
- RouterInfo and LeaseSet retain the exact parsed bytes before the signature;
  verification code in later plans must consume those bytes rather than
  reserializing semantic fields.
- `sha2` is a focused pure-Rust dependency used instead of a locally written
  cryptographic primitive. No cryptographic signing, private key, or secret
  type was added to `i2pr-proto`.
- Codec cursor and encoder debug output reports lengths and offsets only. Public
  key and signature wrappers report type and length, not key/signature bytes.

## Deviations and deferred work

- The plan's selected LeaseSet subset is classic LeaseSet. LeaseSet2,
  MetaLeaseSet, and EncryptedLeaseSet are explicit `Unsupported` paths because
  their offline-signature, encryption-key, blinding, and NetDB semantics belong
  to later crypto/client plans. No guessed length or universal LeaseSet trait
  was added.
- Freshness, future-time, clock-skew, and publication policy remain structural
  data concerns only; NetDB owns those decisions.
- Base32/Base64 API encodings, independent Java I2P/i2pd vectors, property
  testing, and maintained fuzz targets remain follow-up work in Plan 014 or
  later API plans. The local fixed bytes identify their provenance as authored
  protocol expectations; they are not interoperability evidence.
- Plan 014 fuzz entry points are identified as `Mapping::decode`,
  `Certificate::decode`, `RouterIdentity::decode`, `Destination::decode`,
  `RouterAddress::decode`, `RouterInfo::decode`, `Lease::decode`, and
  `LeaseSet::decode`; each takes a caller-visible maximum and has strict
  top-level consumption.
- `ProtocolErrorKind` gained distinct truncation, invalid-value,
  trailing-bytes, and policy-rejected categories. Existing callers only using
  equality on the original variants continue to compile, but callers should
  handle the more precise categories where protocol decisions depend on them.

## Quality-command results

The local validation matrix for this closure is:

| Command | Result |
| --- | --- |
| `rtk cargo fmt --all --check` | passed after formatting |
| `rtk cargo check --workspace` | passed |
| `rtk cargo check --workspace --all-targets` | passed |
| `rtk cargo test --workspace` | passed — 52 tests |
| `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings` | passed |
| `RUSTDOCFLAGS="-D warnings" rtk cargo doc --workspace --no-deps` | passed |
| `rtk bash scripts/check-dependency-direction.sh` | passed |
| `rtk cargo deny check advisories bans sources` | passed |
| `rtk rustup run 1.85.0 cargo check --workspace --all-targets` | passed |

## CI evidence and known limitations

The implementation commit `c9699b7` passed
[GitHub Actions CI run 29388301426](https://github.com/dbowm91/i2pr/actions/runs/29388301426):
MSRV, dependency policy, Ubuntu quality, and macOS quality all passed. GitHub
reported only non-blocking `actions/checkout@v4` Node.js 20 deprecation
annotations. No public-network malformed-traffic or stress testing was run.

The support ledger remains non-advertised and marks the exact structural
surfaces `experimental`; code presence is not an interoperability claim.
Signature verification, identity generation/storage, LeaseSet2-family
interoperability, I2NP, NetDB, transports, tunnels, and all client APIs remain
outside this closure.
