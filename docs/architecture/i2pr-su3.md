# `i2pr-su3` — Bounded SU3 Envelope Verification

Runtime-neutral, dependency-free-of-workspace-crates validation of the common
SU3 signed container, plus RSA-SHA512 (signature type 6) signature
verification against an **explicitly caller-supplied** RSA public key.

Path: `crates/i2pr-su3/`

## Purpose

`i2pr-su3` owns exactly two things:

- **Envelope framing validation.** Parsing a complete SU3 container under
  caller-owned byte ceilings, rejecting reserved-field abuse, truncation,
  and trailing bytes, and returning zero-copy borrows of the signed region,
  the content, and the signature.
- **Signature verification math** for the one currently implemented SU3
  signature type (6 = RSA over SHA-512, PKCS#1 v1.5), plus a minimal helper
  that lifts the RSA modulus/exponent out of a DER X.509 SubjectPublicKeyInfo.

The crate's own module comment states the boundary
(`crates/i2pr-su3/src/lib.rs:1-6`): *"Content-specific policy (such as reseed
ZIP parsing or NEWS XML handling) belongs to the consuming subsystem. This
crate validates the common SU3 envelope and verifies signatures against an
explicit caller-provided RSA key."*

What the crate does **not** own, and where that work actually lives:

| Concern | Real owner |
| --- | --- |
| Reseed SU3 ZIP entry count / expansion limits, RouterInfo validation inside the archive, signer allowlist and signature-type enum (`ReseedSignatureType::RsaSha512_4096`) | `i2pr-netdb` — `crates/i2pr-netdb/src/reseed.rs` |
| Signed-NEWS content/file-type gate, XML/GZIP parsing and expansion limits, ETag/Last-Modified conditionals, cache retention policy, restart re-verification | `i2pr-daemon` — `crates/i2pr-daemon/src/news.rs` |
| CA-chain construction and the trust decision for the signer certificate | The caller. The caller's pinned certificate *is* the trust anchor. |

This crate performs **no decompression of any kind**. There is no ZIP, GZIP,
or XML code here, and therefore no expansion-ratio or zip-bomb defence
here — that defence belongs to the consuming subsystem, which is the only
layer that ever decompresses the content these calls return.

It also has no I/O, no clock, no filesystem, no network, no `async fn`, no
ambient trust store, and `#![forbid(unsafe_code)]` (`lib.rs:8`).

## Module layout

Single-file crate. There is no `src/` submodule, no `benches/`, and no
`tests/` directory.

| File | Lines | Responsibility | Public types |
| --- | --- | --- | --- |
| `src/lib.rs` | 402 total (1–335 production, 336–402 `#[cfg(test)]`) | Constants, limits, header parse, certificate SPKI lift, signature verification, unit tests | `DEFAULT_MAX_SU3_BYTES`, `MAX_SIGNER_ID_BYTES`, `MAX_VERSION_BYTES`, `Su3Limits`, `Su3Header`, `RsaSha512Signer`, `Su3Error`, `rsa_signer_from_certificate`, `parse`, `verify_rsa_sha512` |

## Public surface

Complete inventory — this is the whole API.

| Item | Kind | Source |
| --- | --- | --- |
| `DEFAULT_MAX_SU3_BYTES` | `pub const usize` = `8 * 1024 * 1024` (8 388 608) | `lib.rs:15` |
| `MAX_SIGNER_ID_BYTES` | `pub const usize` = `256` | `lib.rs:16` |
| `MAX_VERSION_BYTES` | `pub const usize` = `64` | `lib.rs:17` |
| `Su3Limits` | struct — `Clone, Copy, Debug, Eq, PartialEq` + `Default` | `lib.rs:22` |
| `Su3Limits::max_file_bytes` | `pub usize` | `lib.rs:23` |
| `Su3Limits::max_content_bytes` | `pub usize` | `lib.rs:24` |
| `Su3Limits::max_signer_id_bytes` | `pub usize` | `lib.rs:25` |
| `Su3Limits::max_version_bytes` | `pub usize` | `lib.rs:26` |
| `Su3Header` | struct — `Clone, Debug, Eq, PartialEq` | `lib.rs:42` |
| `Su3Header::signature_type` | `pub u16` | `lib.rs:43` |
| `Su3Header::signature_length` | `pub usize` | `lib.rs:44` |
| `Su3Header::content_length` | `pub usize` | `lib.rs:45` |
| `Su3Header::file_type` | `pub u8` | `lib.rs:46` |
| `Su3Header::content_type` | `pub u8` | `lib.rs:47` |
| `Su3Header::version` | `pub String` | `lib.rs:48` |
| `Su3Header::signer_id` | `pub String` | `lib.rs:49` |
| `Su3Header::content_offset`, `signature_offset`, `total_length` | **private** `usize` | `lib.rs:50-52` |
| `Su3Header::signed_bytes<'a>(&self, input: &'a [u8]) -> Result<&'a [u8], Su3Error>` | method | `lib.rs:57` |
| `Su3Header::content<'a>(&self, input: &'a [u8]) -> Result<&'a [u8], Su3Error>` | method | `lib.rs:64` |
| `Su3Header::signature<'a>(&self, input: &'a [u8]) -> Result<&'a [u8], Su3Error>` | method | `lib.rs:71` |
| `RsaSha512Signer` | struct — `Clone, Debug, Eq, PartialEq` | `lib.rs:81` |
| `RsaSha512Signer::signer_id` | `pub String` | `lib.rs:82` |
| `RsaSha512Signer::modulus` | `pub Vec<u8>` (unsigned big-endian, no sign padding) | `lib.rs:83` |
| `RsaSha512Signer::exponent` | `pub Vec<u8>` (unsigned big-endian, no sign padding) | `lib.rs:84` |
| `RsaSha512Signer::not_before` | `pub u64` (Unix seconds) | `lib.rs:85` |
| `RsaSha512Signer::not_after` | `pub u64` (Unix seconds) | `lib.rs:86` |
| `Su3Error` | enum — `Clone, Debug, Error, Eq, PartialEq`, 17 variants | `lib.rs:90` |
| `rsa_signer_from_certificate(signer_id: &str, certificate_der: &[u8]) -> Result<RsaSha512Signer, Su3Error>` | fn | `lib.rs:130` |
| `parse(input: &[u8], limits: Su3Limits) -> Result<Su3Header, Su3Error>` | fn | `lib.rs:188` |
| `verify_rsa_sha512(input: &[u8], header: &Su3Header, signer: &RsaSha512Signer, now_seconds: u64) -> Result<(), Su3Error>` | fn | `lib.rs:285` |

Private, and therefore not part of the contract: `HEADER_PREFIX: usize = 25`
(`lib.rs:18`) and `unsigned_big_endian(integer: &[u8]) -> Vec<u8>`
(`lib.rs:178`).

## Key contracts

### `Su3Limits` and the bound constants

`Su3Limits` is four caller-owned ceilings. `Default` maps each field to the
crate constant of the same name (`lib.rs:29-38`):

| Field | Default | Constant | Enforced at |
| --- | --- | --- | --- |
| `max_file_bytes` | `DEFAULT_MAX_SU3_BYTES` (8 MiB) | `lib.rs:15` | `lib.rs:189`, first check in `parse` |
| `max_content_bytes` | `DEFAULT_MAX_SU3_BYTES` (8 MiB) | `lib.rs:15` | `lib.rs:210`, against the decoded header field |
| `max_signer_id_bytes` | `MAX_SIGNER_ID_BYTES` (256) | `lib.rs:16` | `lib.rs:239`, against the encoded signer length |
| `max_version_bytes` | `MAX_VERSION_BYTES` (64) | `lib.rs:17` | `lib.rs:216`, against the encoded version length |

`DEFAULT_MAX_SU3_BYTES` is used for **both** the file and the content default,
so out of the box the container ceiling and the content ceiling coincide.
Consumers narrow per-subsystem: `i2pr-daemon` passes its configured
`max_su3_bytes` for the file and a smaller NEWS-specific ceiling for content
(`crates/i2pr-daemon/src/news.rs:257-266`).

`MAX_SIGNER_ID_BYTES` is also used directly outside the parse path, as a
configuration bound on the operator-supplied signer id
(`crates/i2pr-daemon/src/config.rs:2813`).

### When limits are enforced

Every cap is applied **inside `parse`, before any content is exposed to the
caller**, and in a fixed order that front-loads the expensive-input guard:

1. `input.len() > max_file_bytes` → `FileTooLarge` (`lib.rs:189`). This is the
   first statement in the function, before the 25-byte prefix is even checked,
   so an oversized input costs one length comparison and no allocation.
2. `input.len() < HEADER_PREFIX` → `InvalidLength` (`lib.rs:192`).
3. Magic, format version, and reserved bytes (`lib.rs:195-203`).
4. `content_length > max_content_bytes` → `ContentTooLarge` (`lib.rs:210`),
   checked against the **decoded declared length**, so a large *claimed*
   content size is rejected before any slice is taken.
5. `version_length` bound and printable-ASCII validation
   (`lib.rs:216`, `lib.rs:225-231`).
6. `signer_length` bound, UTF-8 decoding, and control-byte rejection
   (`lib.rs:239`, `lib.rs:246-257`).
7. Exact-consumption check `total_length != input.len()` → `InvalidLength`
   (`lib.rs:264`).

`Su3Header` is returned only after step 7 passes. Nothing is handed back
partially validated.

### SU3 header layout as parsed

`HEADER_PREFIX = 25` (`lib.rs:18`) is the fixed portion before the version
string.

| Offset | Size | Field | Handling |
| --- | --- | --- | --- |
| 0 | 6 | Magic | Must equal `b"I2Psu3"`, else `MagicMismatch` (`lib.rs:195`) |
| 6 | 1 | Format version | Must be `1`, else `UnsupportedFormatVersion` (`lib.rs:198`) |
| 7 | 3 | Reserved | All zero, else `NonZeroReserved` (`lib.rs:201`) |
| 10 | 2 | Signature type | `u16` LE → `signature_type` (`lib.rs:204`) |
| 12 | 2 | Signature length | `u16` LE → `signature_length` (`lib.rs:205`) |
| 14 | 4 | Content length | `u32` LE → `content_length`; `u32`→`usize` failure is `InvalidLength` (`lib.rs:206-209`) |
| 18 | 1 | File type | `u8` → `file_type` (`lib.rs:213`) |
| 19 | 1 | Content type | `u8` → `content_type` (`lib.rs:214`) |
| 20 | 3 | Reserved | All zero, else `NonZeroReserved` (`lib.rs:201`) |
| 23 | 2 | Version length | `u16` LE (`lib.rs:215`) |
| 25 | n | Version string | ASCII `0x20..=0x7e` only, else `InvalidVersion` (`lib.rs:222-231`) |
| 25+n | 2 | Signer id length | `u16` LE (`lib.rs:235-238`) |
| 25+n+2 | m | Signer id | UTF-8, no NUL, no ASCII control, else `InvalidSignerId` (`lib.rs:246-257`) |
| 25+n+2+m | content_length | Content | `content_offset` = this offset (`lib.rs:243`) |
| … | signature_length | Signature | `signature_offset`; `total_length` must equal `input.len()` (`lib.rs:258-266`) |

Every offset is produced with `checked_add` (`lib.rs:219`, `233`, `243`, `258`,
`261`) and every read uses `slice::get(..)` with a typed error, so a
malformed length cannot overflow or panic. The three accessors return
borrowed slices of the caller's original buffer — nothing is copied:

- `signed_bytes` = `input[..signature_offset]` — magic, header, and content
  (`lib.rs:57-61`). This is the exact byte range the signature covers.
- `content` = `input[content_offset..signature_offset]` (`lib.rs:64-68`).
- `signature` = `input[signature_offset..total_length]` (`lib.rs:71-75`).

### `Su3Error` — every variant and what produces it

Seventeen variants, all fail-closed. No variant is a catch-all `Other`, and
none embeds caller-supplied bytes except `UnsupportedKeyType`, which carries
a certificate OID string.

| Variant | Produced by |
| --- | --- |
| `FileTooLarge` | `input.len() > max_file_bytes` (`lib.rs:189`) |
| `MagicMismatch` | `input[..6] != b"I2Psu3"` (`lib.rs:195`) |
| `UnsupportedFormatVersion` | `input[6] != 1` (`lib.rs:198`) |
| `NonZeroReserved` | Non-zero byte in `input[7..10]` or `input[20..23]` (`lib.rs:201`) |
| `InvalidLength` | Shorter than 25 bytes (192); `u32`→`usize` failure (206); any `checked_add` overflow on `version_end` / signer-length / `content_offset` / `signature_offset` / `total_length` (219, 233, 243, 258, 261); `total_length != input.len()` (264) |
| `ContentTooLarge` | Declared `content_length > max_content_bytes` (`lib.rs:210`) |
| `InvalidSignerId` | `signer_length` zero or over cap (239); signer bytes not UTF-8 (246); NUL or ASCII-control byte in signer id (252); in `rsa_signer_from_certificate`, an empty / over-`MAX_SIGNER_ID_BYTES` / control-bearing operator signer id (134) |
| `InvalidVersion` | `version_length` zero or over cap (216); version bytes outside printable ASCII (225); version bytes not UTF-8 (273) |
| `UnsupportedSignatureType(u16)` | `signature_type != 6`, returned with the offending value (`lib.rs:303`) |
| `SignatureLength` | `signature.len() != signer.modulus.len()` (`lib.rs:313`) |
| `UntrustedSigner` | `signer.signer_id != header.signer_id` (`lib.rs:306`) |
| `ExpiredSigner` | `now_seconds < not_before \|\| now_seconds > not_after` (`lib.rs:309`) |
| `InvalidSignature` | Modulus/exponent bit-length `checked_mul` overflow (316, 320); `BoxedUint::from_be_slice` failure (324, 326); `RsaPublicKey::new` failure (328); `Signature::try_from` failure (330); final `Verifier::verify` failure (333) |
| `InputChanged` | Any accessor whose slice range is out of bounds (60, 67, 74); `reparsed != header` in `verify_rsa_sha512` (300) |
| `CertificateParse` | `X509Certificate::from_der` failure (`lib.rs:146`) |
| `UnsupportedKeyType(String)` | `subject_pki.parsed()` failure, carrying the OID (157); a non-`PublicKey::RSA` SPKI, carrying the OID (160) |
| `CertificateValidity` | Negative `notBefore`/`notAfter` timestamp not convertible to `u64` (162, 164); `not_after < not_before` (166) |

### The signature-verification contract and its trust boundary

`verify_rsa_sha512` (`lib.rs:285-334`) is where the security boundary sits.
Read in order:

1. **Re-validate, do not trust the header.** The function re-parses `input`
   using limits derived from the header itself (`max_file_bytes: input.len()`,
   `max_content_bytes: header.content_length`,
   `max_signer_id_bytes: header.signer_id.len()`,
   `max_version_bytes: header.version.len()` — `lib.rs:291-299`) and then
   requires the whole `Su3Header` to compare equal (`lib.rs:300`). Because
   `content_offset`, `signature_offset`, and `total_length` are **private**,
   a caller outside this crate cannot construct a `Su3Header` at all — only
   `parse` can. A header can therefore never go stale relative to the bytes
   being verified; tampering yields `InputChanged` before any cryptography
   runs.
2. **Algorithm gate first.** `signature_type != 6` → `UnsupportedSignatureType`
   (`lib.rs:303`). This rejects unknown algorithms *before* any key material is
   touched, so no other SU3 signature type can reach the verifier, and no
   fallback or default algorithm exists.
3. **Signer binding.** `signer.signer_id != header.signer_id` → `UntrustedSigner`
   (`lib.rs:306`). The identifier inside the signed region must match the
   identifier the caller associated with this key.
4. **Validity window, caller's clock.** `now_seconds` is a parameter, not an
   internal clock read; the crate has no time source. The comparison is
   inclusive at both ends (`lib.rs:309`).
5. **Length agreement.** `signature.len() != signer.modulus.len()` →
   `SignatureLength` (`lib.rs:313`).
6. **Key construction.** Bit lengths are *derived from the caller-supplied
   vectors* — `modulus.len() * 8` and `exponent.len() * 8` — via checked
   multiplication (`lib.rs:316-323`), then `BoxedUint::from_be_slice`,
   `RsaPublicKey::new`, and `Signature::try_from`. Any failure is
   `InvalidSignature`.
7. **Verify.** `VerifyingKey::<Sha512>::new(public_key).verify(header.signed_bytes(input)?, &signature)`
   (`lib.rs:331-333`).

**Hash and padding.** SHA-512 with PKCS#1 v1.5 padding, via
`sad_rsa::sha2::Sha512` and `sad_rsa::pkcs1v15::{Signature, VerifyingKey}`
(`lib.rs:10-12`). Note the source reaches SHA-512 through the `sad-rsa`
re-export rather than a local `sha2::` path.

**The trust boundary, stated plainly.** The RSA public key is an **explicit
argument supplied by the caller**. This crate never fetches, discovers, or
trusts a key from the SU3 file itself — the container carries only a signer
*identifier*, never a key. `RsaSha512Signer` is constructed either by
`rsa_signer_from_certificate` from a DER certificate the **caller pinned in
configuration**, or by the caller filling in the fields directly (which is
what `i2pr-netdb` does at `crates/i2pr-netdb/src/reseed.rs:911-917`).
`rsa_signer_from_certificate` (`lib.rs:130-176`) performs only a minimal lift:
DER parse, `SubjectPublicKeyInfo.parsed()`, require the `PublicKey::RSA`
variant, convert the two validity timestamps, and reject an inverted
interval. It deliberately does **not** build or validate a CA chain, does not
check the certificate's own signature, and does not consult any trust store —
the doc comment says so directly at `lib.rs:127-129`. Whether that pinned
certificate *should* be trusted is entirely the caller's decision.

**Key size.** This crate enforces **no** key-size minimum or maximum. The only
key check is the signature-length-equals-modulus-length agreement above, which
accepts any modulus the caller supplies. RSA-4096 is the *reseed* protocol's
convention, expressed in the consumer as
`ReseedSignatureType::RsaSha512_4096`
(`crates/i2pr-netdb/src/reseed.rs:65-86`), not a rule here — the in-crate test
even uses a 256-byte (2048-bit) modulus (`lib.rs:392`). Do not attribute a
4096-bit requirement to this crate.

**DER sign padding.** `unsigned_big_endian` (`lib.rs:178-185`) strips leading
zero bytes from the DER INTEGERs so the stored modulus length reflects the true
key size and the `SignatureLength` comparison lines up. This was the specific
fix recorded for the extraction in
[`plans/implementation/i2pcontrol-proposal-170/322-routerinfo-canonical-source-and-news-completion.md`](../../plans/implementation/i2pcontrol-proposal-170/322-routerinfo-canonical-source-and-news-completion.md).

### Bounded-allocation and decompression posture

- **No decompression happens in this crate.** No ZIP, GZIP, or XML code, so
  there is no expansion-ratio or zip-bomb check to point at — and none is
  needed at this layer, because the content returned here is still
  compressed. Reseed archive limits (`MAX_ARCHIVE_ENTRIES`, the
  `verify_su3_archive` scan) live in `i2pr-netdb`; NEWS GZIP/XML expansion
  limits live in `i2pr-daemon`. The one compression-adjacent operation is
  DER parsing of the caller's *pinned* certificate, which the caller has
  already bounded on its own read path (for example
  `MAX_NEWS_CERTIFICATE_BYTES` in `crates/i2pr-daemon/src/news.rs`).
- **No content copy.** The accessors borrow. Allocation is limited to the two
  bounded `String`s in `Su3Header` (version ≤ 64 bytes, signer id ≤ 256
  bytes) and the caller-owned key vectors.
- **Ceiling before structure, ceiling before allocation.** The file ceiling is
  the first check, and the content ceiling is applied to the *declared* length
  before any slice is taken.
- **Exact consumption.** `total_length != input.len()` (`lib.rs:264`) rejects
  trailing bytes, so a second container or a trailing payload cannot be smuggled
  past a caller that validates only the first framing.
- **No panic surface.** All offset arithmetic is `checked_add`/`checked_mul`
  and all slicing goes through `get(..)` with a typed error, so hostile length
  fields cannot panic this crate.

## Dependencies

`crates/i2pr-su3/Cargo.toml` — four production dependencies, **zero**
workspace (`i2pr-*`) dependencies, and no `[dev-dependencies]`:

| Dependency | Workspace pin | Why it is present |
| --- | --- | --- |
| `sad-rsa` | `0.10`, `default-features = false`, features `std`, `encoding`, `sha2` | RSA public-key construction, `BoxedUint` big-endian integer import, and PKCS#1 v1.5 `VerifyingKey`/`Signature` (`lib.rs:10-12`, `324-332`) |
| `sha2` | workspace `0.10` | SHA-2 family backing; note the source currently imports `Sha512` through `sad_rsa::sha2` (`lib.rs:11`) rather than naming this crate directly |
| `thiserror` | workspace | `Su3Error` derive (`lib.rs:13`, `89`) |
| `x509-parser` | `0.18`, `default-features = false`, feature `verify` | Minimal DER X.509 parse and `SubjectPublicKeyInfo` extraction (`lib.rs:142-161`); the `verify` feature is enabled centrally in the workspace manifest, while this crate itself uses only the parse/SPKI surface |

Because there is no `i2pr-*` edge, the entry in
[`scripts/check-dependency-direction.sh`](../../scripts/check-dependency-direction.sh) (line 29)
is an empty set — `"i2pr-su3": set()` — the tightest possible entry in the
allowlist. The published dependency row in
[`dependency-graph.md`](dependency-graph.md) matches:
`(no production crate) + sad-rsa, sha2, thiserror, x509-parser`.

**Policy context.** [`docs/security-model.md`](../../docs/security-model.md)
requires that production cryptography use reviewed implementations and that no
local primitive is written. ADR
[0005 — Reviewed cryptographic dependency selection](../../docs/adr/0005-crypto-dependency-selection.md)
is the repo's reviewed-crypto-dependency decision; it enumerates the
`i2pr-crypto` set and its Plan 329/336 amendments, and it does **not** name
`sad-rsa` or `x509-parser`, so treat it as the governing policy rather than as
an itemised review of this crate's two crypto-bearing dependencies. The
extraction itself is recorded in Plan 322 and closed in
[`plans/closure/i2pcontrol-proposal-170/322-status.md`](../../plans/closure/i2pcontrol-proposal-170/322-status.md);
licence and advisory coverage for the two crypto crates remains subject to
`cargo deny`, per ADR 0005's consequences.

## Consumers

Exactly two production crates depend on `i2pr-su3`:

- **`i2pr-netdb`** — reseed verification. `trust_signer_from_certificate`
  delegates the certificate lift and maps `CertificateParse`,
  `UnsupportedKeyType`, and `CertificateValidity` into its own
  `ReseedTrustError` (`crates/i2pr-netdb/src/reseed.rs:759-770`);
  `verify_rsa_sha512_signature` rebuilds an `RsaSha512Signer` from its
  configured trust set, cross-checks the signed/signature slices against its
  own expectation, and delegates the math here
  (`crates/i2pr-netdb/src/reseed.rs:911-931`). The reseed-specific trust
  allowlist, signature-type enum, ZIP entry limits, and RouterInfo validation
  stay in `i2pr-netdb`.
- **`i2pr-daemon`** — signed NEWS. Pins a signer certificate, builds the
  signer once (`crates/i2pr-daemon/src/news.rs:209`), verifies every cached
  and fetched record with explicit per-subsystem `Su3Limits`
  (`crates/i2pr-daemon/src/news.rs:257-268`), and applies the NEWS
  content/file-type gate and bounded XML/GZIP parsing *after* signature
  verification (`crates/i2pr-daemon/src/news.rs:270+`). Its config path also
  bounds the operator signer id with `MAX_SIGNER_ID_BYTES`
  (`crates/i2pr-daemon/src/config.rs:2813`).

## Tests

There is **no `crates/i2pr-su3/tests/` directory** and no fixture directory
owned by this crate. All coverage is a three-test `#[cfg(test)] mod tests` at
`lib.rs:336-402`, built on one local `fixture()` helper (`lib.rs:340-355`)
that assembles a hand-rolled SU3 container: version `20261004`, signer
`trusted`, content `payload`, signature type 6, and a **zero** signature
length.

| Test | Source | What it proves |
| --- | --- | --- |
| `validates_generic_framing_and_exposes_exact_content` | `lib.rs:357` | `file_type`/`content_type`/`signer_id` decode correctly and the three accessors return exactly the right ranges: `content` is `b"payload"`, `signed_bytes` is the entire buffer, `signature` is empty (the fixture's signature length is 0) |
| `rejects_content_over_limit_and_trailing_bytes` | `lib.rs:369` | `max_content_bytes: 6` against a 7-byte payload yields `ContentTooLarge`; one appended byte yields `InvalidLength` (exact-consumption enforcement) |
| `verifier_rejects_header_changed_after_framing_parse` | `lib.rs:385` | Flipping `input[18]` (file type) after a successful `parse` makes `verify_rsa_sha512` return `InputChanged`, proving the re-parse equality check closes the stale-header gap before any signature work |

Verified: `cargo test --locked -p i2pr-su3 --all-targets` → **3 passed, 0
failed, 0 ignored** (`sad-rsa 0.10.2`, `x509-parser 0.18.1`).

**Honest coverage gap.** No in-crate test exercises a *successful* signature
verification, a wrong-key rejection, an expired-signer rejection, an
unsupported-signature-type rejection, or the certificate-lift path — the
fixture has a zero-length signature, so `verify_rsa_sha512` can only be
driven into its pre-crypto checks. Those paths are covered downstream instead:
`i2pr-netdb`'s reseed suite (which generates real 2048-bit RSA keypairs and
signatures at `crates/i2pr-netdb/src/reseed.rs:1188-1211`) and
`i2pr-daemon`'s signed-NEWS tests (certificate pin, wrong signer, expired
pinned cert, invalid signature, wrong content/file type — recorded as evidence
in Plan 322).

## Distinctive design choices

1. **Zero workspace dependencies** — the smallest crate in the workspace, and
   the reason both `i2pr-netdb` and `i2pr-daemon` can depend on it without a
   layering argument; the checker allowlist entry is literally `set()`.
2. **Borrow, never copy, the payload** — the accessors return `&'a [u8]`, so a
   caller can verify and then feed the *same* bytes to its own parser with no
   second copy of up to 8 MiB of content.
3. **Ceiling before structure** — `max_file_bytes` is the first statement of
   `parse`, and `max_content_bytes` is checked against the *declared* length,
   so hostile size fields are rejected before any slice or allocation.
4. **Exact consumption, not a prefix match** — `total_length != input.len()`
   rejects trailing bytes, so no second container or trailing payload can hide
   behind a valid framing.
5. **Explicit-key trust, no discovery** — the key arrives as an argument;
   nothing in the container can nominate or upgrade the key that verifies it.
6. **A pinned certificate is the trust anchor** — the crate lifts an SPKI and
   a validity interval and stops; it builds no chain, validates no
   self-signature, and consults no trust store, because that judgement is the
   caller's.
7. **Fail-closed on algorithm** — signature type 6 only, checked before any
   key material is touched, with no default or fallback algorithm.
8. **Caller-supplied clock** — `now_seconds` is a parameter, keeping the crate
   free of any time source, I/O, or async runtime.
9. **Unforgeable headers** — the three offset fields are private, so a
   `Su3Header` can only come from `parse`; `verify_rsa_sha512` then re-parses
   and compares the entire struct, closing the parse-then-mutate gap with
   `InputChanged`.
10. **No local cryptography** — hash, RSA verification, and DER parsing are
    all delegated to reviewed third-party crates per
    [`docs/security-model.md`](../../docs/security-model.md), with only
    typed-bounds and fail-closed policy written locally.

## Cross-references

- [`overview.md`](overview.md) §4.4 — the crate index row and the one-line
  summary this deep dive expands.
- [`dependency-graph.md`](dependency-graph.md) — the allowlist mirror; the
  `i2pr-su3` row lists no production crate.
- [`tooling.md`](tooling.md) — the boundary and evidence scripts.
- [`i2pr-netdb.md`](i2pr-netdb.md) — the reseed consumer that retains
  archive and RouterInfo policy.
- [`i2pr-netdb-persist.md`](i2pr-netdb-persist.md) — persistence and the
  bounded reseed ingestion path.
- [`i2pr-crypto.md`](i2pr-crypto.md) — the router-identity crypto wrapper
  crate governed by ADR 0005; no overlap with this crate.
- [`i2pr-daemon.md`](i2pr-daemon.md) — the signed-NEWS owner that applies the
  XML/GZIP and cache policy after envelope verification.
- [ADR 0005 — Reviewed cryptographic dependency selection](../../docs/adr/0005-crypto-dependency-selection.md)
  — governing crypto-dependency policy.
- [`docs/security-model.md`](../../docs/security-model.md) — the
  reviewed-implementations and secret-handling rules.
- [`GUARDRAILS.md`](../../GUARDRAILS.md) — the CI-enforced boundaries this
  crate is written to respect.
- [`specs/support.toml`](../../specs/support.toml) — the machine-readable
  `netdb.su3-reseed` support entry.
- Plan 322 (extraction of this crate) —
  [`implementation plan`](../../plans/implementation/i2pcontrol-proposal-170/322-routerinfo-canonical-source-and-news-completion.md)
  and [`closure status`](../../plans/closure/i2pcontrol-proposal-170/322-status.md).
- [`scripts/check-dependency-direction.sh`](../../scripts/check-dependency-direction.sh) (line 29) —
  the `"i2pr-su3": set()` allowlist entry this crate must keep empty.
