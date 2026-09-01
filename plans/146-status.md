# Plan 146 status — SAM 3.1 private-destination bidirectional reference requalification

Status: **`passed-m7-sam31-private-destination-reference-requalification`**.

Registered: **2026-09-01**.

Plan of record:
[`plans/146-m7-sam31-private-destination-reference-requalification.md`](146-m7-sam31-private-destination-reference-requalification.md).

Source audit: Plan 145 (`active-m7-sam31-corrective-roadmap`),
[`plans/145-status.md`](145-status.md).

## Outcome

Plan 146 closes **Outcome B**: i2pr's previous `from_bytes` /
`from_base64` round-trip rejected reference `PrivateKeyFile` output
because the i2pr decoder enforced `encryption_public ==
X25519(static_secret)`, an invariant that the standard Java I2P
`PrivateKeyFile` (pinned at `2800040deee9bb376567b671ef2e9c34cf3e30b6`,
release 2.12.0) and i2pd's `IdentityEx`/`PrivateKeys`
(pinned at `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`, release 2.60.0)
both relax for destinations. The destination encryption field is
unused for end-to-end traffic; both reference implementations
populate it with random bytes and record an unrelated random 32-byte
`PrivateKey` slot in the PRIV suffix.

The fix is a relaxed import invariant:

- `DestinationIdentity::from_imported(destination, signing_seed,
  static_secret)` (new) preserves the destination bytes verbatim.
  It only enforces `signing_public == EdDSA(signing_seed)`; a
  mismatch returns
  `DestinationIdentityError::ImportSigningKeyMismatch`. The
  destination's encryption public field is **not** required to equal
  `X25519(static_secret)`.
- `SamPrivateDestination::from_bytes_array` (modified) still
  round-trips the structural decode (`decode` then
  `encode_to_vec`), so truncated / malformed / unknown-cert
  destinations are still rejected with `PublicPrivateMismatch`. The
  reconstructed identity is then derived through
  `DestinationIdentity::from_imported`, not the legacy
  `from_private_bytes`.
- `SamPrivateDestination::into_identity` (internal `identity_from_bytes`)
  (modified) routes through the same relaxed import constructor so
  the resulting `DestinationId` matches the embedded destination
  hash byte-for-byte.

After the fix, both directions of Plan 146 evidence pass end-to-end:

- **Direction A (reference generates, i2pr imports)**: Java I2P
  pinned `PrivateKeyFile` generates `priv_binary_len = 455`,
  `priv_base64_len = 608`, `pub_binary_len = 391`,
  `pub_base64_len = 524`, `private_key_field_is_256 = false`. i2pr's
  `SamPrivateDestination::from_bytes` succeeds and
  `into_identity().id()` returns the exact same 32-byte SHA-256
  destination hash the reference reports.
- **Direction B (i2pr generates, reference consumes)**: i2pr
  `DEST GENERATE SIGNATURE_TYPE=7` produces the canonical 524-char
  `PUB` and 608-char `PRIV` text; the pinned Java I2P
  `PrivateKeyFile` parser succeeds, reports
  `parsed_pub_binary_len = 391`, `parsed_pub_base64_len = 524`,
  `parsed_cert_signing_type = 7`, `parsed_cert_crypto_type = 4`,
  and re-emits a `PUB` Base64 that exactly equals the i2pr reply.
- **Real-listener smoke (Plan 146 §10)**: A real Tokio listener on
  loopback TCP accepts the same Java-generated `PRIV` through
  `SESSION CREATE STYLE=STREAM`, replies with `SESSION STATUS
  RESULT=OK`, and confirms the destination registry and streaming
  pool counts return to zero on listener shutdown.
- **Helper self-describes (Plan 146 §4)**: pinned reference and
  release identifiers match `tests/integration/ntcp2/manifest.toml`.
- **Negative path (Plan 146 §11)**: `+` and `/` characters
  embedded in `PUB` text are rejected, `pub_binary_len` mismatches
  are rejected, the i2pr codec's strict I2P Base64 bounds still
  reject truncated Base64, and the `from_bytes` API still rejects
  `+AAA`/`/AAA` characters.

## Pinned reference evidence

- Java I2P source revision:
  `2800040deee9bb376567b671ef2e9c34cf3e30b6` (release 2.12.0).
- i2pd source revision:
  `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e` (release 2.60.0).
- Cache key (matches `target/interop/cache/current-cache.json`):
  Java I2P `8ecafd4b1075610ead86a4d93974794ef4e82a224858d8d45ef83cf526770361`,
  i2pd `501439e8ca88f378756403d10827162ac55151a8fee69e4f88dfe2641a98e7be`.
- Lock digest: `943af1f7af3ba5f3df52c499cfd386be4b76cb2f650218c174981b114f4121ef`.

## Code changes

- `crates/i2pr-client/src/identity.rs` — new
  `DestinationIdentity::from_imported` constructor; new
  `DestinationIdentityError::ImportSigningKeyMismatch` variant.
- `crates/i2pr-api/src/sam/private_destination.rs` — module-level
  docstring records the Plan 146 reference-compatibility invariant;
  `from_bytes_array` and `identity_from_bytes` route through
  `DestinationIdentity::from_imported`.
- `crates/i2pr-daemon/tests/sam_plan146_reference.rs` (new) — five
  focused tests spanning Plan 146 §4, §5, §6, §10, §11.
- `tests/integration/sam/reference/Plan146ReferenceHelper.java`
  (new) — `generate`, `parse`, and `version` subcommands with
  never-logged raw `PRIV` and direct read-back of the encoded
  record.

## Acceptance criteria evidence

### §5/§6 bidirectional reference evidence

- `plan146_reference_helper_self_describes`:
  - reference=`java_i2p`, release=`2.12.0`,
    source_revision=`2800040deee9bb376567b671ef2e9c34cf3e30b6`,
    signature_type=`7`, crypto_type=`4`.
- `plan146_reference_generates_i2pr_imports_exact_destination`:
  - `priv_binary_len = 455`, `priv_base64_len = 608`,
    `pub_binary_len = 391`, `pub_base64_len = 524`,
    `private_key_field_is_256 = false`,
    `helper_self_round_trip_dest_equal = true`,
    `helper_self_round_trip_bytes_equal = true`,
    `dest_sha256` and i2pr `DestinationId` are byte-equal.
- `plan146_i2pr_generates_reference_consumes_exact_destination`:
  - `parsed_cert_type = KEY_CERT`,
    `parsed_cert_signing_type = 7`,
    `parsed_cert_crypto_type = 4`,
    i2pr `DEST REPLY PUB` byte-equals reference re-emitted `PUB`.

### §10 real-listener smoke

- `plan146_real_listener_smoke_returns_resource_baseline`:
  - Real Tokio listener on an ephemeral loopback port.
  - `HELLO VERSION MIN=3.1 MAX=3.1`, then
    `SESSION CREATE STYLE=STREAM ID=plan146-ref DESTINATION=<Java-generated-PRIV>`,
    expected `SESSION STATUS RESULT=OK`.
  - Listener shutdown returns destination registry and streaming
    pool counts to zero.

### §11 negative path

- `plan146_negative_path_lengths_and_alphabet`:
  - i2pr `from_base64` rejects `+`/ `/` characters.
  - i2pr `decode` rejects byte lengths that violate the i2pr
    decoder's strict alphabet.
  - `pub_binary_len` mismatch between i2pr `PUB` text and i2pr's
    codec-decoded length is detected.

### Plan-level tests, formatting, clippy, static boundaries

- `cargo test --locked --workspace` — 512 tests pass, 0 fail.
- `cargo fmt --all --check` — clean.
- `cargo clippy --locked --workspace --all-targets --all-features
  -- -D warnings` — clean.
- `cargo check --locked --workspace --all-targets` — clean.
- `bash scripts/check-dependency-direction.sh` — `dependency
  direction: ok`.
- `bash scripts/check-runtime-boundaries.sh` — `runtime boundary
  checks passed`.

## Documentation corrections

- `specs/references/sam31-private-destination.md` — bidirectional
  reference evidence section added; relaxed encryption-public-key
  invariant documented; pinned Java I2P source revision recorded.
- `specs/support.toml` — `sam31_private_destination_external_compatibility`
  restored to `reference-compatible`, with `plan146_java_i2p_280004…`
  and `plan146_i2pd_f618e…` evidence keys.
- `docs/protocol-support.md` — Plan 146 status supersedes Plan 142's
  "external compatibility not yet claimed" note.
- `plans/README.md` — Plan 146 row added with the plan-of-record and
  closure record pointers.
- `README.md`, `AGENTS.md`, skill `i2pr-local-dev` — Milestone 7
  product status updated to reflect closed Plan 146.

## What Plan 146 does **not** close

- Same-socket raw `STREAM CONNECT` / `STREAM ACCEPT` product bridge
  with live `TCP -> Streaming -> Delivery` byte flow remains Plan
  147.
- Two-independent-client final Milestone 7 closure remains
  Plan 148.
- No SAM feature is advertised.

## Handoff instruction

The next implementation model should read Plan 145 and execute
**Plan 147 only**. The Plan 146 private-destination bidirectional
reference sub-claim is closed and must not be re-opened without a
concrete finding against a new reference.
