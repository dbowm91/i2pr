# ADR 0005: Reviewed cryptographic dependency selection

- Status: Accepted
- Date: 2026-07-15

## Context

Plan 013 forbids local implementations of cryptographic primitives and
requires a dependency review before adding each cryptographic library. The
workspace already uses `sha2` for structural hash derivation but has no
Ed25519, X25519, zeroization, or constant-time comparison wrapper.

## Decision

`i2pr-crypto` uses these direct dependencies with default features disabled:

| Crate | Pinned Cargo range/lock family | Role | Enabled features |
| --- | --- | --- | --- |
| `ed25519-dalek` | 2.2 | Ed25519 signing and strict verification | `std`, `zeroize` |
| `x25519-dalek` | 2.0.1 | X25519 static public-key derivation | `static_secrets`, `zeroize` |
| `zeroize` | 1.8 range, lock 1.9.0 | Secret wrapper erasure and derive support | `derive` |
| `subtle` | 2.6 | Constant-time integrity comparison | none |
| `rand_core` | 0.9 | Injected `TryCryptoRng` and `OsRng` seam | `os_rng` at the crypto boundary |
| `sha2` | workspace 0.10 | SHA-256 wrapper and identity/storage digest | workspace existing dependency |

The selected crates are established RustCrypto/dalek ecosystem components,
have MSRVs below the workspace's Rust 1.85 declaration, and are pure-Rust
implementations in the reviewed direct and critical transitive path. The
workspace lints deny unsafe code in `i2pr-crypto`; no local primitive or
serialization format is delegated to these crates. No serde, PEM, PKCS#8,
batch, getrandom, or broad default feature is enabled accidentally.

The lockfile resolves the reviewed direct versions to `ed25519-dalek` 2.2.0,
`x25519-dalek` 2.0.1, `zeroize` 1.9.0, `subtle` 2.6.1, `rand_core` 0.9.5,
and `sha2` 0.10.9. `ed25519-dalek` and `x25519-dalek` provide zeroization support for their secret
types; `i2pr-crypto` additionally owns non-cloneable private wrappers so secret
bytes do not acquire public protocol-type traits. `rand_core::OsRng` is passed
into generation explicitly rather than hidden in a constructor.

`i2pr-storage` uses the same workspace `zeroize` dependency directly for its
serialized write buffer, file-read buffer, and decoded fixed arrays. It does
not expose a general secret-management abstraction or serialize secrets for
logging.

## Consequences

The dependency graph adds curve arithmetic and random-source transitive code,
which is justified by the concrete current identity operations. Dependency
licenses and advisories remain subject to `cargo deny`; future primitives must
receive their own review rather than expanding this into a provider plugin.

## Review triggers

Review on a major-version update, a new primitive, a changed default feature,
an MSRV change, a security advisory, or a requirement to support legacy or
hybrid algorithms.

## Amendment (Plan 329, 2026-10-04): intended direct `curve25519-dalek` dependency

Reviewed and accepted for use by Plan 330, not yet added as a dependency at Plan 329 time.
Full review: [`specs/references/red25519-clean-room-freeze.md`](../../specs/references/red25519-clean-room-freeze.md) §4.

`curve25519-dalek 4.1.3` is already present in `Cargo.lock` through `ed25519-dalek` and
`x25519-dalek`, so adopting it as a direct `i2pr-crypto` dependency causes no version churn. It
owns low-level curve arithmetic only: wide scalar reduction, canonical scalar decoding, compressed
Edwards point decoding/encoding, basepoint and variable-base multiplication, point addition, and
cofactor/small-order/torsion predicates. i2pr will own only the I2P Red25519 scheme composition
(domain separation, `HStar`, alpha derivation, re-randomization, sign/verify composition) and typed
bounds, error, and zeroization semantics.

Conditions on the adoption, all carried into the Plan 330 requirement set:

- declared centrally in the workspace manifest with `default-features = false` plus only the
  features actually compiled (`group` and the precomputed basepoint table; not `group-bits`, not
  `legacy_compatibility`);
- `#![forbid(unsafe_code)]` in `i2pr-crypto` continues to hold, and no dalek `unsafe` surface is
  exposed to the workspace;
- variable-time dalek entry points are permitted only where every operand is public;
- MSRV 1.60.0 is below the workspace MSRV 1.88; license is MIT OR Apache-2.0, already covered by the
  existing `cargo deny` allowlist;
- no second curve crate may be introduced for this purpose. Any future need for a different curve
  API is a review trigger for this ADR, not an ad-hoc dependency.

The independent-implementation policy of Plan 329 is unaffected by this amendment: composing a
reviewed curve library is required, and a local field/scalar/point implementation is prohibited.
