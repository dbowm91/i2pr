# Plan 333 status — passed: Encrypted LeaseSet2 PSK and DH client authorization

- Plan: [`plans/implementation/i2pcontrol-proposal-170/333-encrypted-leaseset-client-authorization.md`](../../implementation/i2pcontrol-proposal-170/333-encrypted-leaseset-client-authorization.md)
- Status: **`passed-encrypted-leaseset-client-authorization-psk-and-dh`**
- Decision date: 2026-10-04
- Classification: cryptographic protocol implementation plus secret lifecycle. This plan promotes
  **no** capability advertisement and no live interoperability.
- Freeze commit: **`f525578`** (implementation frozen before any external oracle was invoked)
- Worksheet: [`specs/references/red25519-algorithm-worksheet.md`](../../../specs/references/red25519-algorithm-worksheet.md) §11, §14.16–§14.23
- Conformance: [`specs/CONFORMANCE.md`](../../../specs/CONFORMANCE.md) §"Encrypted LeaseSet2 per-client authorization status"

## Requirement-to-evidence matrix

The plan's evidence list asks for twelve rows "for PSK and DH separately". The last one —
cross-implementation publication/decryption against Java and i2pd "where supported" — was **not
executed** and is recorded as such below rather than counted as passing. Everything else is covered
by `crates/i2pr-netdb/tests/els2_client_authorization.rs` (30 rows) and
`crates/i2pr-client/tests/els2_authorized_publish_resolve.rs` (16 rows).

| Requirement | Evidence | Result |
|---|---|---|
| Exact PSK layer-1 construction: bounded non-empty key set | `Els2AuthorizationServerConfig::psk`; `an_empty_client_set_is_refused`, `the_maximum_client_count_is_accepted_and_one_more_is_refused` | PASS. An empty set is a construction error, not a record nobody can open. Capacity 255 is exercised and 256 is refused with `TooManyClients`. |
| Correct client identifier derivation | `psk_client_material`; `the_derivation_is_deterministic_and_domain_separated` | PASS. 52-byte OKM sliced `okm[0..32]`/`okm[32..44]`/`okm[44..52]`, worksheet §14.16. A different key, salt, subcredential, or `published` each yield a different identifier. |
| Per-generation fresh auth cookie and salt | `draw_generation_secrets`, `build_psk_block`; `each_generation_draws_a_fresh_salt_and_cookie` | PASS. 16 generations yield 16 distinct salts and 16 distinct cookies. A cookie from another generation does not open the record. |
| Exact HKDF domain labels | `ELS2_PSK_AUTH_HKDF_INFO` = `ELS2PSKA`, `ELS2_DH_AUTH_HKDF_INFO` = `ELS2_XCA`; the byte-identity differential | PASS. Labels are exported constants and agree with the oracle byte-for-byte in both directions. |
| Encrypted cookie record framing | `AuthBlock::encode_to_vec` / `decode`; `a_block_round_trips_through_its_codec`, `a_declared_client_count_that_disagrees_with_the_bytes_is_refused` | PASS. `salt(32) || count(2 BE) || N × 40`. A count that disagrees with the byte count, a truncated block, and trailing bytes are all refused — trailing bytes matter because they would otherwise be read as the start of the inner ciphertext. |
| Randomized emitted entry order where recommended | `shuffle`; `entry_order_does_not_decide_authorization` | PASS. 16 builds of the same key set produce more than one distinct order, and every client is authorized regardless of where its entry landed. Entry order carries no meaning. |
| Constant-time matching where applicable | `recover_auth_cookie`, `AuthClientMaterial::recover_cookie`; worksheet §14.21 | PASS. Every entry is examined, the identifier comparison is constant-time, and the first match wins. The compiler is not trusted to constant-time a slice comparison here. |
| Decryption using an authorized PSK only | `a_wrong_psk_is_refused_with_the_same_error_as_an_absent_one`, `a_psk_client_is_refused_against_a_dh_block_and_vice_versa` | PASS. A wrong key, an unconfigured key, and a key for the other scheme all produce the identical `NotAuthorized` and the identical message. |
| Per-client X25519 public keys; generation-local ephemeral keypair | `build_dh_block`, `generate_client_dh_keypair`; `one_dh_client_publishes_and_recovers_its_own_cookie` | PASS. The ephemeral is generation-local, so revoking a client stops it deriving a key for subsequent records. |
| Shared-secret derivation with all-zero rejection | `X25519PrivateKey::diffie_hellman`; worksheet note in `dh_client_material` | PASS, by reuse. The reviewed `x25519-dalek` wrapper already rejects an all-zero shared secret; this plan does not duplicate the primitive, and the doc comment says so. |
| Client identifier and encrypted cookie construction | `dh_client_material`; `the_dh_derivation_is_deterministic_and_binds_the_client_public_key` | PASS. The client's own public key is inside the derivation, so substituting a different one derives a different identifier — a client cannot claim a co-client's authorization (worksheet §14.17). |
| Exact layer framing and HKDF domains | `Layer1Authorization`, `encrypt_outer_ciphertext`; the Emissary differential | PASS, byte-identical in both directions. |
| Bounded client count and aggregate DH work | `MAX_ELS2_AUTH_CLIENTS = 255`; the max/max+1 row | PASS. The server performs N+1 DH operations for N clients, bounded by the same 255 ceiling. |
| Authorized-client decrypt path using the client private key | `resolve_with_auth`; `every_authorized_dh_client_reads_the_same_published_record` | PASS. Three configured clients all read the same published record. |
| Use the existing reviewed X25519 wrapper | `i2pr_crypto::X25519PrivateKey` throughout | PASS. No new X25519 primitive, no new curve dependency, no `unsafe`. |
| Secrets are typed zeroizing values, never emitted by I2CP/Get/rawConfig/logging | `PskClientKey`, `AuthCookie`, `Els2ClientAuthSecret`; `secrets_never_render_their_bytes` | PASS. All three are zeroizing, not `Clone`, and have redacted `Debug`. `Els2ClientAuth::Dh` redacts the private half while naming the public one. No control surface exists yet that could leak them — that is Plan 334. |
| One narrow secret owner distinguishing four roles | `Els2ClientAuthSecret`, `Els2AuthSecretRole`; `a_secret_reports_its_role_and_refuses_to_change_sides` | PASS. Server PSK, server-side DH client public key, client PSK, and client DH private key are all 32 bytes and are not interchangeable, so each secret carries an explicit role and refuses conversion across it. |
| Persistence: explicit owner-restricted policy, restart-safe, no one-way verifiers, atomic with the destination generation | `to_persisted_bytes` / `from_persisted_bytes`; `a_client_recovers_after_a_restart_from_persisted_material` | PASS for the parts this plan owns, and explicitly **not claimed** for the rest. The encoding is reversible and role-tagged because the protocol needs the secret on every publication, so a one-way verifier would be useless — which is what the plan requires. A client rebuilds from persisted bytes and the address text alone and opens a record published before the restart. At-rest encryption remains the storage layer's responsibility and is not implemented here. |
| Names associated with client keys are metadata only | `ClientName` | PASS. The type has no accessor returning anything a derivation could consume, and no name is ever placed in a cryptographic input. |
| Control-neutral API consumed by Plan 334, not Proposal 170 parsing | `Els2AuthorizationServerConfig` | PASS. The module knows nothing about I2CP or Proposal 170 field names; it is the protocol owner that Plan 334 maps onto them. |
| One authorized client (both schemes) | `one_psk_client_publishes_and_recovers_its_own_cookie`, `one_dh_client_publishes_and_recovers_its_own_cookie`, `every_authorized_psk_client_reads_the_same_published_record`, `every_authorized_dh_client_reads_the_same_published_record` | PASS. |
| Multiple authorized clients | the same four rows, plus `every_configured_psk_client_recovers_its_own_cookie` and its DH counterpart | PASS. Three clients read one record. |
| Wrong key | `a_wrong_psk_is_refused_with_the_same_error_as_an_absent_one`, `a_dh_client_whose_key_is_not_configured_is_refused` | PASS. |
| Missing key | `an_unauthorized_psk_client_is_refused_with_a_typed_error`, `an_unauthorized_dh_client_is_refused` | PASS, and deliberately a *different* error from a wrong key: only the missing-credential case is fixable by presenting some authorized key. |
| Duplicate identifier collision handling | `duplicate_client_identifiers_are_rejected_at_build_time`, `a_duplicate_identifier_is_rejected_for_dh_too`; worksheet §14.18 | PASS. Rejected at build, not merged. A merge would silently drop a client and make the cleartext count disagree with the real one. |
| Shuffled record order | `entry_order_does_not_decide_authorization` | PASS. |
| Max and max+1 clients | `the_maximum_client_count_is_accepted_and_one_more_is_refused` | PASS, at capacity 255 and 256. |
| Tampered cookie / salt / ephemeral key | `a_tampered_salt_or_ephemeral_key_defeats_authorization`, `a_tampered_cookie_yields_a_different_cookie_rather_than_an_error`, `tampering_with_the_published_ciphertext_is_refused_end_to_end` | PASS, with a correction recorded below. |
| Restart recovery | `a_client_recovers_after_a_restart_from_persisted_material` | PASS. |
| Daily rollover | `a_record_from_the_previous_day_is_not_authorized_today`, `the_expiry_is_still_clamped_to_the_day_boundary_with_authorization` | PASS. Yesterday's record is refused today as a storage-key mismatch and still opens at its own timestamp. Authorization does not change publication semantics. |
| Wrong lookup secret combined with auth | `a_wrong_lookup_secret_plus_authorization_fails_at_the_storage_key` | PASS. The right secret plus the right key opens; the wrong secret fails at the storage key *before* the credential is consulted, because the secret is a discovery control and the credential is a reading control. |
| Cross-implementation publication/decryption against Java and i2pd | — | **NOT EXECUTED.** See "Independent and reference evidence". |

## Independent and reference evidence

| Lane | Source | Result |
|---|---|---|
| Official ELS2 authorization vectors | — | **None exist.** The specification publishes no vectors for the authorization derivations, the block, or the encrypted cookie, so no official-vector claim is made. |
| Independent re-derivation | — | **Not extended.** The Plan 332 pure-Python re-derivation covers the credential, the subcredential, and the layer ciphertexts, none of which this plan changes. The authorization HKDF was instead compared against a live second implementation, which is stronger evidence for this particular surface. |
| Post-freeze Emissary black-box differential | `eggstack/emissary@6885a945d25a5ae61bc68191d27c5816bc3df4c9`, public API only; `crates/i2pr-netdb/tests/data/els2-auth-emissary-differential.json` (SHA-256 `8cf031ce16272d9dc203173a6fdde2adc725c5215f0a44fbaeb028642d77056d`); `tests/els2_auth_emissary_differential.rs` (9 rows) | **PASS, bidirectional, byte-identical, both schemes.** The implementation was frozen at `f525578` **before** the oracle ran. Driven from byte-identical inputs — same subcredential, same outer and inner salts, same auth cookie, same client keys, same ephemeral key — i2pr's pre-shared-key publication and the oracle's are the **same 440 bytes**, and the Diffie-Hellman publications are the **same 440 bytes**. In reverse, the oracle decrypts both i2pr records to the exact 300-byte inner payload. The oracle's refusals agree with i2pr's on all three cross-credential cases. No Emissary source was read, and both driver crates live outside this repository. |
| Java I2P differential | `i2p/i2p.i2p@93eef5d` | **NOT EXECUTED.** No runnable Java I2P build was provisioned, and the type-11 signature divergence recorded in Plans 331/336 makes the lane moot for a signed record regardless. The plan's evidence list says "where supported"; this is not claimed as supported. |
| i2pd differential | `PurpleI2P/i2pd@2c694149` | **NOT EXECUTED for authorization.** i2pd was read for the layer framing and the initial-counter layout only, never for the authorization block. The type-11 signature divergence means an i2pr-signed record is unverifiable there regardless. |

The differential driver's first run disagreed on the Diffie-Hellman record. The cause was in the
driver, not the implementation: the exporter emitted the ephemeral *public* key where the oracle's
`encrypt_dh_with_ephemeral` takes the *private* key, so the two sides derived different `epk`
values. Fixing the driver produced byte-identity. This is recorded because a differential that
"passes" for the wrong reason is worth more to a future reader than one that is not mentioned.

## Three implementation-discovered findings, and one correction

Recorded as worksheet §14.16–§14.23. None was resolved by relaxing a vector or a reference behavior.

1. **The layer-2 ciphertext offset double-counted the layer-1 flags byte — a regression this plan
   introduced and its own evidence caught.** Generalizing Plan 332's encrypt/decrypt into one
   authorization-aware path re-derived the inner-ciphertext offset as `1 + (1 + auth_len)` instead of
   `1 + auth_len`, so every authorized record decrypted to garbage. Plan 332's seven previously green
   client rows failed immediately, which is the only reason this was caught before a freeze. The
   offset is now computed once, from the same value that was parsed, so the two cannot disagree. The
   lesson recorded for later: a refactor that generalizes a frozen path must re-run the frozen path's
   own tests, which here was the whole point of keeping them.
2. **An authorized service's address never declared that it required a client key.** The layer-1 flag
   byte already refuses a record whose credential is absent, but that check happens *after* a fetch.
   `publisher.address()` hardcoded `requires_client_key = false` and the strict resolver constructor
   refused such addresses, so `resolve_with_auth` was unreachable through the public API — the feature
   existed and could not be called. Both are fixed (worksheet §14.20). Recorded as a finding rather
   than a routine change because "implemented but unreachable" is the failure mode a green test suite
   most easily hides.
3. **The client-count ceiling is a policy choice, and the two references disagree** (worksheet
   §14.23). The wire format's 2-byte count permits 65 535 entries; i2pr bounds at 255, Emissary at 99.
   Neither comes from the specification, and they are not compatible at the top of the range: a record
   i2pr considers publishable at 200 clients is one Emissary refuses to parse. i2pr's looser bound is
   the safer direction for a publisher and the more permissive one for a receiver, so it is not
   lowered to match a reference. Recorded so that reconciling the two is a deliberate decision rather
   than an accident.
4. **Correction to this plan's own initial reasoning.** The layer-1 block carries no tag — ChaCha20 is
   unauthenticated — and a first draft of the tamper evidence treated that as a live integrity gap.
   It is not. The block lives inside the outer ciphertext, which the Red25519 signature covers, and a
   receiver validates that signature before decrypting, so a tamper is rejected as `InvalidSignature`
   rather than as an authorization failure. The test now asserts exactly that. Worksheet §14.15
   already recorded the general principle; the reasoning error was in not applying it to the new
   block.

## Verification

Local commands and outcomes (2026-10-04, Rust 1.95.0, Linux x86_64):

- `cargo fmt --all --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` — passed: **3,920 passed, 0
  failed, 35 ignored across 143 suites** (Plan 332 baseline: 3,860 across 139). This plan contributes
  30 NetDB authorization rows, 16 client rows, and 9 differential rows.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — passed after
  fixing four findings: `too_many_arguments` on `encrypt_outer_ciphertext` (see below), an unused
  `dh_keys` helper, a `single_match`, and a `mem::drop` on a `Copy` value.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` — passed.
- `cargo test --locked --workspace --doc` — passed (0 doctests in every crate).
- `bash scripts/check-dependency-direction.sh` — passed.
- `bash scripts/check-runtime-boundaries.sh` — passed.
- `bash scripts/check-service-tunnel-boundaries.sh` — passed.
- `bash scripts/check-fixture-manifest.sh` — passed. As in Plan 332, the manifest covers
  `tests/fixtures/i2np` only and does not cover `crates/*/tests/data`; that is recorded, not assumed.
- `bash scripts/check-constrained-host-lane-boundary.sh`, `check-m11-transit-boundaries.sh` — passed.
- `bash scripts/check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`, `check-i2cp-vectors.sh`,
  `check-ntcp2-interoperability.sh` — passed.
- `bash scripts/check-m11-transit-qualification-evidence.sh`, `check-sam-acceptance-evidence.sh`,
  `check-ssu2-acceptance-evidence.sh`, `check-i2cp-acceptance-evidence.sh`,
  `check-i2pcontrol-acceptance-evidence.sh`, `check-service-tunnel-acceptance-evidence.sh`,
  `check-exploratory-tunnel-evidence.sh`, `check-netdb-tunnel-evidence.sh`,
  `check-destination-tunnel-evidence.sh`, `check-streaming-tunnel-evidence.sh`,
  `check-m6-mixed-router-acceptance-evidence.sh` — all passed.
- `bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test` — passed.
- `python3 scripts/check-global-plan-number-uniqueness.py` — passed.
- `python3 -m unittest discover -s tests/planning -p 'test_*.py'` — passed (6 tests).
- `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` —
  passed (18 tests).
- `cargo deny check advisories bans sources` — advisories ok, bans ok, sources ok. Pre-existing
  duplicate-version warnings are unchanged.
- Emissary differential lane (post-freeze, outside CI): the two external driver crates under
  `/tmp/ref/`, neither committed.

These are local results, not CI claims. The Java I2P and i2pd authorization lanes remain unexecuted
and are Plan 335 obligations.

## Security review

- **No secret is formatted, cloned, serialized, or logged.** `PskClientKey`, `AuthCookie`, and
  `Els2ClientAuthSecret` are zeroizing, deliberately not `Clone`, and have redacted `Debug` that
  prints a role or a length and never a byte. `Els2ClientAuth::Dh` redacts the private half while
  naming the public half. `secrets_never_render_their_bytes` asserts this by scanning the formatted
  output for the secret's own byte pattern.
- **The four roles cannot be crossed.** `as_psk`, `as_dh_private`, and `as_dh_public` each refuse a
  secret carrying the wrong role, so a persistence layer that mixed a server key and a client key
  fails at the boundary rather than at the first publication.
- **Identifier matching is constant-time and error text is uninformative.** A wrong key, an
  unconfigured key, and a key for the other scheme produce the same `NotAuthorized` and the same
  message, so an attacker learns nothing about how close a guess was. The *missing*-credential error
  is deliberately different and deliberately more informative, because it is fixable.
- **The resolver is still lookup-only.** A client that holds a key which opens the record still holds
  no signing material; publication requires an owner schedule, which requires the service's private
  blinded key. The asymmetry is structural, not a convention.
- **A floodfill still learns nothing.** An authorized record is stored and served exactly as an
  unauthorized one is: the floodfill validates the signature and freshness and never derives the
  subcredential, because it never learns the unblinded public key. The client count in the
  authorization block is inside the layer-1 plaintext, so a *network* observer learns how many
  clients are subscribed, which is the privacy property the specification asks for, and a floodfill
  learns nothing beyond what the record already exposed.
- **No advertisement.** `specs/support.toml` is unchanged. No capability, version, or RouterInfo
  string changed. No daemon configuration exposes an encrypted-LeaseSet mode; that is Plan 334.
- **The `too_many_arguments` lint was answered by design, not by an allow.** `encrypt_outer_ciphertext`
  took nine positional parameters including an `Option` and a raw buffer that had to agree with the
  flags byte. Those three are now `Layer1Authorization`, which validates their agreement once at
  construction. Suppressing the lint would have left a call site where the block and the cookie could
  be transposed.

## Compatibility, migration, and failure behavior

- **No migration.** Authorization is a new capability on a new record type. No existing record type,
  stored file, or configuration key changes meaning.
- **Authorization is additive and cannot be turned on retroactively.** A record published *without* a
  block does not become authorized because a credential is later presented, and a record published
  *with* a block is not openable by presenting a credential to the unauthenticated path
  (`a_record_published_without_authorization_does_not_become_authorized`). The Plan 332 no-authorization
  path delegates to the same general functions, so its differential fixture remains valid unchanged.
- **Failure modes are typed and distinguishable:** `Els2AuthError` for derivation, block, and
  authorization failures; `Els2Error::Auth` wrapping it through the layer path; `Layer1Authorization::new`
  for a caller that disagrees with itself; `EncryptedLeaseSetError::NotAuthorized` (wrong or
  cross-scheme credential) versus `CredentialNotSupplied` (none) at the client boundary.
- **No new dependency.** `i2pr-netdb` gained no `zeroize` and no `rand_core` dependency; `i2pr-crypto`
  re-exports `TryCryptoRng` and `Zeroizing` so the crypto-owning crate below can use them without
  widening the dependency allowlist. `docs/architecture/dependency-graph.md` is unchanged.

## One pre-existing defect observed, root-caused, and deliberately not fixed here

During a full floor run, `i2pr-daemon`'s `service_tunnel_unique_local_product::unique_local_dial_uses_deterministic_source`
failed once with `InvalidDestinationRef { reason: "configured destinations must not carry private
destination material" }`. It passed 8/8 in isolation and passed in every other full run.

**Root cause.** `i2pr_service_tunnels::destination::parse_configured_destination` rejects any
configured destination whose lowercased text contains the substring `priv`:

```rust
if value.to_ascii_lowercase().contains("priv") {
    return Err(rejected("configured destinations must not carry private destination material"));
}
```

The test mints a real base64 I2P destination from a live manager and feeds it back through
`DestinationRef::parse`. A base64 destination is random-looking text over `[A-Za-z0-9-~]`, so it
spells `priv` by chance at roughly **1.8 × 10⁻⁴ per destination** (measured over 200 000 random
draws of a representative length) — about one draw in 5 500. A workspace run that mints thousands of
destinations is therefore expected to hit it occasionally, which is exactly what was observed.

The heuristic is trying to catch an operator pasting *private* destination material into a config
file. It works for its intent and fails closed for it, but it also refuses a **legitimate public**
destination whose encoding happens to contain those four letters. That is a real availability bug in
the service-tunnel configuration path, not in anything this plan touched.

**Not fixed here, on purpose.** The defect is in `i2pr-service-tunnels`, a different subsystem with
its own Plan-of-record, and nothing in this plan's diff reaches it — the failing test references no
`i2pr-netdb`, `i2pr-crypto`, or encrypted-LeaseSet2 surface at all. Fixing it inside a Plan 333
closure would be an unfocused change to code this plan has no evidence obligations in. It is recorded
here so the next service-tunnel plan can pick it up with a root cause rather than rediscovering it
from a CI log. The correct fix is to stop matching a substring in opaque encoded material — for
example by decoding first and checking the *structure*, or by matching only against an explicit
`priv:` scheme prefix.

## Limitations and what this plan does not claim

1. **Java I2P and i2pd authorization lanes were not executed.** No Java I2P build was provisioned,
   and the type-11 signature divergence blocks a meaningful signed-record comparison either way.
   The plan's evidence list qualifies that row with "where supported"; it is not claimed as supported.
2. **The differential covers one authorized client per scheme.** The multi-client case is covered by
   i2pr's own rows but was not run against the oracle, so byte-identity is not claimed above one
   client.
3. **Nothing is published or fetched on a live network.** `i2pr-client` builds a `DatabaseStoreMessage`
   and a resolver; no daemon driver sends or fetches it. No `specs/support.toml` entry exists.
4. **At-rest encryption of authorization secrets is not implemented.** The persistence encoding is
   reversible and role-tagged, and its doc comment says plainly that the owner-restricted, encrypted
   store is the storage layer's responsibility. Restart safety is proven; secret protection at rest is
   not claimed.
5. **A tampered authorization cookie is not detected by the block itself.** ChaCha20 is
   unauthenticated, so a flipped bit yields a different cookie rather than an error. This is not a
   gap — the identifier is checked first in constant time, the wrong cookie yields a wrong layer-2 key,
   and the outer signature covers the block — but the block does not detect tampering on its own and
   the test says so rather than implying otherwise.
6. **A failing random source during the optional entry shuffle leaves the order as-is** rather than
   aborting an otherwise valid publication. The order is a privacy hint, not a correctness
   requirement. A failure that prevents drawing the cookie, salt, or ephemeral *is* fatal.

## Unblock audit

| Plan | Pre-closure state | Decision |
|---|---|---|
| 334 Proposal 170 encrypted-LeaseSet mode mapping | blocked on 333 | **Unblocked → ready.** Its hard dependency is this plan, and the surface it needs now exists: `Els2AuthorizationServerConfig` is the protocol-side owner of the client set and scheme, `Els2AuthScheme` is the mode discriminator, `PskClientKey` and `AuthClientPublicKey` are the two credential types, `ClientName` is the metadata-only label, and `build_authorized_database_store` is the publication entry point. Nothing in `els2_auth.rs` knows a Proposal 170 field name, which is exactly the separation Plan 334 needs. |
| 335 live ELS2 interoperability | blocked on 334 | Stays blocked behind 334. Both named obligations are unchanged: the Java lane is still unexecuted, and the type-11 signature divergence is still decisive. This plan adds a second one — the client-count ceiling divergence in §14.23 is a live interop question for a large authorized client set. |
| 326 encrypted/blinded LeaseSet | historical blocked | Stays blocked; the successor reclosure still belongs to Plan 335. |
| 327, 328, 322 | blocked on router owners | Unchanged. |

Roadmap: `plans/subsystems/red25519-encrypted-leaseset-roadmap.md` §6 records 333 `passed` and 334
`ready`, with 335 blocked. The branch completion boundary in §7 is closer to reachable: an
authorized service can now be published and read by exactly its configured clients, with the
construction byte-identical to an independent implementation in both directions, while a floodfill
stores and serves the record without being able to read it.

## Handoff notes for Plan 334

1. `Els2AuthorizationServerConfig` is the type to map Proposal 170's `LeaseSetClientAuths` onto. It
   distinguishes `Psk(Vec<PskClientKey>)` from `Dh(Vec<AuthClientPublicKey>)` at construction, so a
   control surface cannot build a mixed or half-configured set.
2. `ClientName` is the metadata-only label type. It is `Copy`, has no accessor that returns anything
   a derivation could consume, and is not accepted by any function in `els2_auth.rs`. Attaching a
   name to a configured key is a control-surface concern and needs no protocol support.
3. `EncryptedLeaseSet2Publisher::authorized_address` sets `B32_FLAG_REQUIRES_CLIENT_KEY`. A control
   surface that publishes an authorized service must use this rather than `address`, and a client
   configuration surface must build its resolver with `EncryptedLeaseSet2Resolver::new_authorized`
   rather than `new`, which refuses such an address by design.
4. Secrets arrive and leave through `Els2ClientAuthSecret`, never as bare byte strings. A control
   surface must persist the role-tagged encoding and must not substitute a one-way verifier: the
   protocol needs the secret on every publication.
5. The scheme discriminator for a control surface is `authorization_scheme(&config)`, and
   `ELS2_DH_AUTH_KEY_TYPE_CODE = 4` exists only for control surfaces — the DH derivation itself takes
   no key-type bytes.
