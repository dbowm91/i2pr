# Plan 280 status — stopped: no acceptable I2P Red25519 provider

- Plan: `plans/implementation/floodfill/280-m12-red25519-provider-qualification.md`
- Parent: Plan 271 close, commit `740e8ff`.
- Disposition: **stopped without provider pass**, per the plan's explicit stop condition. No new
  dependency, wrapper, crypto code, vectors, or behavior was added.

## Candidate review

| Candidate | Finding | Disposition |
|---|---|---|
| Rust `reddsa` 0.6.1 | Maintained Zcash crate; implementation specializations use RedJubjub/RedPallas, Zcash personalization, and Jubjub/Pallas groups. I2P Red25519 requires its own SHA-512 domain and Ed25519 group/order/cofactor behavior. | Incompatible; rejected. |
| Rust `zakura-reddsa` | Zcash RedDSA fork over Zcash curve specializations; no I2P Red25519 specialization or official I2P vectors. | Incompatible; rejected. |
| Other crates.io RedDSA search results | Search surfaced Jubjub/Pallas and unrelated-curve providers; `red25519` search surfaced no Rust I2P-compatible crate. | No acceptable candidate. |
| Go `go-i2p/red25519` | Implements a Go provider, but adding it would cross the Rust runtime-neutral crypto boundary and require FFI/unsafe integration not allowed by this plan. | Not acceptable for this workspace. |

The official I2P specification requires the exact `I2P_Red25519H(x)` hash domain, Ed25519
group/order, little-endian encodings, and cofactor-aware verification. Reusing standard Ed25519
or a generic Zcash RedDSA crate would be incorrect. Source findings are recorded in
`specs/SOURCES.md`.

## Corrective action and unblock audit

Plan 281 narrows the initial M12 validated record floor by deferring EncryptedLeaseSet type 5
until a future, separately reviewed provider qualification passes. Plan 272 remains blocked
until Plan 281 closes; no other plan became eligible. Plans 273–279 retain their predecessor
dependencies. The Plan 280 provider objective remains unmet and must not be reported as passed.

## Verification

- `rtk cargo search red25519 --limit 20` — no I2P-compatible Rust crate returned.
- `rtk cargo info reddsa` — 0.6.1 metadata reviewed (license MIT OR Apache-2.0, Rust 1.88).
- `rtk cargo info zakura-reddsa` — inspected candidate; incompatible Zcash specialization.
- Source inspection of downloaded `reddsa` shows Zcash RedJubjub/RedPallas implementations and domains.
- Official [I2P Red25519 specification](https://i2p.net/en/docs/specs/red25519/) reviewed.
- No crypto crate changes or dependency changes; there are no provider-specific tests to run.
