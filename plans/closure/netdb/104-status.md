# Plan 104: closure record

- Status: **implemented and closed on the local host**.
- Date: 2026-08-13.
- Parent authority: Plan 102.
- Baseline: Plan 103 closure with Plan 103 `i2pr-netdb` as-is.
- Implementation source: `crates/i2pr-storage/src/lib.rs` (cache_seam),
  `crates/i2pr-netdb/src/reseed.rs`, `crates/i2pr-netdb/src/base64.rs`,
  `crates/i2pr-netdb-persist/` (new crate).
- Next executable implementation: **Plan 105** (transport-neutral NetDB
  query state machines).

## Closure summary

Plan 104 made the Plan 103 RouterInfo trust boundary durable and
bootstrap-capable without activating an I2P transport.

### Work package 1 — persistent RouterInfo cache

- `i2pr_storage::cache_seam::ByteCache` manages the bounded
  `netdb/routers/` subdirectory with strict 64-lowercase-hex filename
  validation, 64 KiB per-file ceiling, 32 MiB aggregate scan ceiling,
  and atomic replace via same-directory temporary + `hard_link` install.
- Failed replacement preserves the prior valid file; temporary files are
  never eligible records.
- One bad cache file does not make all valid peers unavailable; the
  loader isolates invalid entries and continues within global scan
  limits.

### Work package 2 — storage and validation acyclic

Plan 104 closure ownership direction:

```text
i2pr-storage: bounded read/write/remove of canonical bytes (cache_seam)
i2pr-netdb:   SU3/reseed validation, ZIP ingestion, I2P Base64 codec (pure, no I/O)
i2pr-netdb-persist: composition owner that ties them together
```

`i2pr-netdb` remains filesystem-free. The composition owner
(`i2pr-netdb-persist`) reads from `ByteCache`, decodes through
`RouterInfo`, validates through `ValidatedRouterInfo`, and inserts
through `RouterInfoStore::insert`.

### Work package 3 — bounded SU3 reseed parser

- `i2pr_netdb::parse_su3` validates magic, format version, reserved
  fields, signature/content type, and all length fields before any
  archive parsing begins.
- Exact signed-byte region is retained for cryptographic verification.

### Work package 4 — explicit reseed signer trust store

- `ReseedSignerTrustSet` maps signer identifiers to parsed certificates
  with modulus, exponent, and validity interval.
- `TrustedSigner` is constructed from `trust_signer_from_certificate`
  (parses DER X.509, extracts RSA public key, enforces validity
  interval) or directly in test harnesses.

### Work package 5 — SU3 signature verification

- RSA-SHA512-4096 (signature type 6) is verified through the reviewed
  `rsa` crate's PKCS#1 v1.5 implementation.
- Tests prove rejection after content tampering, unknown signer, and
  expired certificate.

### Work package 6 — bounded ZIP processing

- Archive entry count, per-entry uncompressed bytes, and cumulative
  uncompressed bytes are bounded by `ReseedLimits`.
- Path separators, directories, symlinks, duplicate names, and
  unsupported compression methods are rejected.
- Filename hashes use strict I2P Base64 with the I2P-specific
  alphabet.

### Work package 7 — staged ingestion policy

- SU3 trust failure or archive-level limit failure: zero records
  accepted, zero NetDB state mutated.
- Individual RouterInfo failure inside an authentic archive: rejected
  record counted, archive scan continues, no in-memory store
  mutation.

### Work package 8 — reseed source/acquisition seam

The HTTPS acquisition adapter is explicitly left for Plan 106.
Plan 104 exposes a clean `bytes -> verified/staged records` API
(`ReseedIngestor::ingest_su3_into` and
`ReseedIngestor::ingest_verified_archive_into`) and a
`ReseedIngestLimits` configuration type so Plan 106 does not
redesign trust semantics.

### Work package 9 — fixtures and tests

- 7 base64 codec tests (encode/decode round-trip, I2P alphabet,
  padding, rejection).
- 3 reseed header tests (magic, format version, file/content type).
- 5 reseed filename tests (hash extraction, round-trip, short prefix).
- 4 reseed end-to-end tests (valid SU3 + ingestion, tampered content,
  wrong filename, expired/unknown signer).
- 3 cache loader tests (missing cache, invalid filename, load report).
- 4 reseed ingest composition tests (unknown signer, summary counts,
  cache loader report, verified bundle).

### Work package 10 — documentation and support state

After closure, truthful support:

```text
local RouterInfo validation    = implemented (Plan 103)
bounded local NetDB            = implemented (Plan 103)
local RouterInfo construction  = implemented (Plan 103)
persistent RouterInfo cache    = implemented, untrusted-on-load (Plan 104)
SU3 reseed verification        = implemented (Plan 104)
reseed ingestion               = implemented from verified bytes (Plan 104)
automatic daemon reseed        = not yet active (Plan 106)
live NetDB query               = not implemented (Plan 105)
I2P peer connectivity          = still blocked/experimental NTCP2
NTCP2 advertised               = false
```

## Validation commands and results

Each command ran from the repository root on the local host.

```text
$ cargo fmt --all --check
(no output)

$ cargo check --locked --workspace --all-targets
Finished `dev` profile [unoptimized + debuginfo] target(s) in N.NNs

$ cargo test --locked --workspace
<all tests pass>

$ cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
No issues found

$ RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
Generated docs without warnings

$ bash scripts/check-dependency-direction.sh
dependency direction: ok

$ bash scripts/check-runtime-boundaries.sh
runtime boundary checks passed
```

## Handoff to Plan 105

The exact APIs for Plan 105 consumption:

```text
ValidatedRouterInfo::from_router_info(router_info, expected_key?, context)
RouterInfoStore::insert(validated) -> InsertOutcome
RouterInfoStore::iter() -> Iterator<(&RouterHash, &ValidatedRouterInfo)>
ValidatedRouterInfo::encoded(maximum) -> Vec<u8>
ValidatedRouterInfo::advertises_floodfill() -> bool
ValidatedRouterInfo::key() -> RouterHash

ReseedIngestor::ingest_su3_into(bundle, now, context, store, cache)
ReseedIngestor::ingest_verified_archive_into(archive, context, store, cache)
```

Plan 105 consumes the populated `RouterInfoStore` without knowing SU3,
ZIP, X.509, HTTPS, or cache file layout. Plan 105 may enumerate
floodfill advertisers, derive routing keys, and drive iterative lookup
state machines purely from the in-memory store.

## Status

Plan 104 is closed. NTCP2 remains experimental and non-advertised.
The next executable implementation is **Plan 105** (transport-neutral
NetDB query state machines).
