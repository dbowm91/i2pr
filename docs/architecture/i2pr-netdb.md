# i2pr-netdb

Runtime-neutral RouterInfo validation, bounded in-memory NetDB store,
SU3/reseed verification, peer-selection primitives, and local signed
RouterInfo construction.

> Status: experimental. Not production-ready. See `README.md` and
> `GUARDRAILS.md`.

## Purpose

`i2pr-netdb` is the first stateful router-information subsystem in
`i2pr`. It owns:

- cryptographic, freshness, and key-binding validation for `RouterInfo`
  records (`ValidatedRouterInfo`);
- a bounded in-memory store with deterministic
  replacement/conflict/expiry semantics (`RouterInfoStore`);
- I2P Base64 codec for reseed filenames (`base64`);
- SU3 reseed container parser and RSA-SHA512-4096 signature verifier
  (`reseed`);
- pure routing primitives (`xor_distance`, `nearest`,
  `nearest_floodfill`);
- local signed RouterInfo construction without advertising unqualified
  transports (`LocalRouterInfoBuilder`).

## Module layout

| Module | Purpose |
| --- | --- |
| `router_info` | `RouterHash` derivation, `ValidatedRouterInfo` boundary, `ValidationContext` |
| `store` | `RouterInfoStore` with checked arithmetic, byte quotas, replacement/conflict/expiry |
| `base64` | I2P Base64 alphabet codec (RFC 4648 with `~` for `=` and `-?` for `+/`) |
| `reseed` | SU3 parser, RSA signature verification, ZIP ingestion, `ReseedSignerTrustSet` |
| `routing` | XOR distance, deterministic nearest-N floodfill selection |
| `local` | `LocalRouterInfoBuilder`, self-validation, capability policy |

## Dependency boundary

```text
i2pr-netdb -> i2pr-crypto, i2pr-proto + thiserror, base64ct, sha2, x509-parser, zip, rsa
```

Runtime-neutral: no `tokio`, no `std::net`, no `std::fs`, no sockets,
no DNS. Filesystem I/O belongs to `i2pr-storage` (raw-byte seam) and
`i2pr-netdb-persist` (composition owner).

## Plan 104 surfaces

- `base64::decode` / `base64::encode` — I2P Base64 codec for
  reseed filenames.
- `reseed::parse_su3` / `verify_su3` / `verify_su3_with_signers` /
  `verify_su3_archive` — full SU3 trust + signature verification
  pipeline.
- `reseed::ReseedSignerTrustSet` — typed signer trust store with
  certificate validity enforcement.
- `reseed::ReseedLimits` — tuned scan budgets for the reseed pipeline.
- `reseed::TrustedSigner` — parsed certificate with modulus, exponent,
  and validity interval.

## Key contracts

- `ValidatedRouterInfo::from_router_info` is the only constructor;
  there is no unchecked insertion path.
- `RouterInfoStore::insert` is the only public store entry point; it
  enforces byte quotas and replacement/conflict/expiry semantics
  deterministically.
- SU3 signature verification delegates to the `rsa` crate's
  PKCS#1 v1.5 + SHA-512 implementation; the repository does not
  implement RSA primitives locally.
- `ReseedSignerTrustSet` maps signer identifiers to parsed certificates;
  the verifier requires an exact match before signature verification
  proceeds.
- ZIP ingestion is bounded by `ReseedLimits`; one archive exceeding
  any limit fails closed with zero records accepted.

## Tests

All tests run locally. No root, namespaces, Java I2P, i2pd, or
Internet connection is required. The SU3 end-to-end test uses a
deterministic 2048-bit RSA test signer and verifies the full SU3
header + signature + ZIP archive + RouterInfo validation path.
