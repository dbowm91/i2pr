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

## Consequences

The dependency graph adds curve arithmetic and random-source transitive code,
which is justified by the concrete current identity operations. Dependency
licenses and advisories remain subject to `cargo deny`; future primitives must
receive their own review rather than expanding this into a provider plugin.

## Review triggers

Review on a major-version update, a new primitive, a changed default feature,
an MSRV change, a security advisory, or a requirement to support legacy or
hybrid algorithms.
