# Plan 332 status — passed: Encrypted LeaseSet2 (DatabaseStore type 5) foundation

- Plan: [`plans/implementation/i2pcontrol-proposal-170/332-encrypted-leaseset2-foundation.md`](../../implementation/i2pcontrol-proposal-170/332-encrypted-leaseset2-foundation.md)
- Status: **`passed-encrypted-leaseset2-foundation-without-per-client-authorization`**
- Decision date: 2026-10-04
- Classification: cryptographic protocol implementation plus NetDB record and client lifecycle
  integration. This plan promotes **no** capability advertisement and no live interoperability.
- Freeze commit: **`4e691bd`** (implementation frozen before any external oracle was invoked)
- Worksheet: [`specs/references/red25519-algorithm-worksheet.md`](../../../specs/references/red25519-algorithm-worksheet.md) §10–§14
- Conformance: [`specs/CONFORMANCE.md`](../../../specs/CONFORMANCE.md) §"Encrypted LeaseSet2 (DatabaseStore type 5) status"

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| First-class DatabaseStore type-5 protocol structure, not a deferral | `crates/i2pr-proto/src/common/els2.rs`; `DatabaseStoreData::EncryptedLeaseSet` in `src/i2np/netdb.rs` and `src/i2np/message.rs`; `crates/i2pr-proto/tests/lease_set2_fixture.rs` | PASS. Type 5 replaced the `Deferred` pass-through: a body is framed, reserved flag bits are rejected, the exact signature preimage is retained, and a short or malformed body is a typed `CodecError` instead of an accepted blob. The superseded deferral test was rewritten, not deleted, and names the supersession. |
| Type 5 must not reuse the ordinary LeaseSet2 header shape | `common/els2.rs` framing table; `common/lease2.rs` untouched | PASS. The type-5 layer 0 carries no Destination, no properties mapping, and no encryption keys. `i2pr-netdb/src/els2.rs` decrypts layer 2 into an ordinary `LeaseSet2` only after decryption. |
| RFC 8439 ChaCha20 pinned to the layer semantics, owned by one crypto owner | `crates/i2pr-crypto/src/chacha.rs`; RFC 8439 §2.4.2 vector; `rfc8439_section_2_3_2_block_vector_pins_the_initial_counter` and `published_vector_is_unreachable_at_block_counter_zero` | PASS. 12-byte nonce and initial block counter 1 are enforced inside the wrapper, not at each call site. The published §2.4.2 vector is unreachable at counter 0, and a counter-0 control row pins the divergence. `LayerCipherKey` erases on drop and has no `Clone`/`Debug`/serde. |
| Credential and subcredential derivation bound to the unblinded public key | `derive_els2_credentials`; `credential_binds_to_the_unblinded_key_and_the_blinded_key` | PASS. Credential depends on the unblinded key and its sigtype; subcredential adds the blinded key; the personalization keeps the credential from colliding with a DHT lookup hash. Only sigtypes 7 and 11 are accepted for the unblinded key. |
| Two HKDF layer derivations, no 2-byte key or 8-byte IV | `derive_layer_keys`; the independent re-derivation fixture | PASS. `HKDF(salt, subcredential‖published, "ELS2_L1K"/"ELS2_L2K", 44)` sliced `okm[0..32]`/`okm[32..44]`, matching worksheet §14.1. |
| No-client-authorization encryption and decryption | `encrypt_no_auth_outer_ciphertext`, `decrypt_no_auth_outer_ciphertext`; `per_client_layer_one_flags_are_refused_by_the_no_auth_floor` | PASS. A layer-1 per-client flag byte is a typed `ClientAuthorizationRequired`, never silently decrypted. Plan 333 owns the authorized form. |
| Inner timestamp and expiration cross-check | `decrypt_no_auth_outer_ciphertext`; `wrong_published_timestamp_yields_no_decryption` | PASS. `published` participates in both layer key inputs, so a wrong timestamp yields a wrong key; the inner record's own timestamps are then required to match the outer. Both store types (3 and 7) are covered. |
| Exact size ceilings, caller-visible | `MAX_ELS2_INNER_LEASE_SET_LENGTH`, `MAX_ELS2_OUTER_CIPHERTEXT_LENGTH`, `MAX_ELS2_RECORD_LENGTH`; `size_ceilings_are_enforced_on_both_layers` | PASS after correction — see finding 2. The outer ceiling is derived from the framing so the two bounds cannot drift. |
| Canonical encrypted-service B32/B33 encode/decode, no secret in the address | `crates/i2pr-proto/src/common/base32.rs`; 13 unit rows including a full single-bit sweep | PASS. Ordinary 52-character B32 is untouched. Reserved flag bits, sigtype-width mismatch, unsupported sigtypes, missing suffix, foreign characters, and non-canonical trailing bits are each rejected with a distinct typed error. Flags can *declare* a secret requirement; no secret is ever encoded. |
| Blinding lifecycle: one owner, atomic rollover, restart-safe, bounded precomputation | `BlindingIdentity`, `BlindingSchedule`, `BlindingScheduleConfig`, `OwnerBlinding`; `rollover_is_atomic_counted_once_and_restart_safe`, `precomputation_is_bounded_and_never_changes_the_current_day` | PASS. Every accessor returns one value carrying its own day, blinded public key, and storage key together, so a mixed result is unrepresentable. Rollover is counted exactly once per day change; nothing is persisted, so a restart recomputes identical material. Precomputation never moves the current day and the cache is capped. |
| Lookup-secret bound documented, no silent fallback | `i2pr_crypto::red25519::LookupSecret`; `lookup_secret_bound_is_documented_and_enforced` | PASS. The 256-byte bound is applied at construction, an over-long secret is a distinct `LookupSecretRejected` configuration error rather than a blinding failure, and an absent secret and an empty secret are the same derivation input. |
| Red25519 record signature, offline-key delegation, freshness, key binding | `ValidatedEncryptedLeaseSet2::validate`; `offline_key_block_is_parsed_and_its_delegation_is_verified`, `freshness_and_storage_key_checks_are_enforced`, `tampered_signature_and_header_fields_are_rejected` | PASS. Fail-closed order: length → blinded sigtype → blinded key decode → storage key → offline delegation → record signature → freshness. The offline block is verified with the *blinded* key; the record signature is verified with the transient key when offline keys are present. |
| NetDB store/serves type 5 opaquely; type-5 is not a RouterInfo payload | `Els2Store`; `ValidatedNetDbRecord::EncryptedLeaseSet2`; `server_store.rs`; `floodfill_service.rs`, `lookup_engine.rs`, `store_message.rs`; `floodfill_stores_and_serves_type_five_records_opaquely` | PASS. A floodfill validates the Red25519 signature and the freshness window, then serves the stored bytes. It never derives the subcredential, because it never learns the unblinded public key. `store_message.rs` and `lookup_engine.rs` treat type 5 as a non-RouterInfo payload. |
| Bounded store with replacement, conflict, and capacity rules | `Els2Store`; `store_replaces_newer_rejects_stale_and_never_conflicts_with_itself`, `store_capacity_is_enforced_without_mutating_existing_state` | PASS. Strictly newer replaces; equal-and-identical is idempotent; equal-and-different is a conflict that preserves the stored record; older is stale. A rejected insert never evicts. Byte accounting is released on every drop path. |
| Local destinations can construct, sign, publish, resolve, and decrypt | `crates/i2pr-client/src/encrypted_leaseset.rs`; `crates/i2pr-client/tests/els2_publish_resolve.rs` (7 rows) | PASS. The publisher builds a `DatabaseStoreMessage` the daemon can send through the existing path; the resolver derives the day's blinded key from the address alone and returns the address, the daily material, and the inner `LeaseSet2` as one value. The decrypted inner record is validated with the ordinary LeaseSet2 rules before it is handed on. |
| A lookup-only schedule cannot publish, and a resolver cannot sign | `EncryptedLeaseSet2Publisher`, `BlindingSchedule::owner_blinding`; `a_resolver_cannot_sign_and_a_publisher_needs_owner_material` | PASS. A lookup-only schedule returns `NotAnOwner` instead of an unsigned record. The resolver is a separate type holding only a public identity. |
| One destination owner; no second destination stack | crate boundaries; `docs/architecture/dependency-graph.md` | PASS. Layer 2 is decrypted into the existing `LeaseSet2` type and fed to the existing destination path. The only new dependency is `chacha20` in `i2pr-crypto`, already locked and already used by `i2pr-transport-ssu2` and `i2pr-tunnel`. |
| Regression and security floor | see Verification | PASS. 3,860 tests across 139 suites before the differential rows; full floor green. |

## Independent and reference evidence

| Lane | Source | Result |
|---|---|---|
| Official ELS2 vectors | — | **None exist.** Worksheet §14.7. No published vector covers the credential, the subcredential, the layer derivations, or the layer ciphertexts, so no official-vector claim is made. |
| Independent re-derivation | `tools/generate-els2-independent-fixture.py` → `crates/i2pr-netdb/tests/data/els2-independent-derivation.json` (SHA-256 `ed1200d104f7a6aeb1115f99f5e3b1469aa8ef143380c82592460a9f55c1da44`) | PASS. Pure Python from the frozen specification text — standard-library SHA-256 and HMAC-SHA256 plus an RFC 8439 ChaCha20 written from the RFC — reproduces the credential, the subcredential, and the complete outer ciphertext byte-for-byte across 5 cases spanning both unblinded sigtypes, both inner store types, a single-byte inner, a 733-byte inner, a 64-byte block boundary, zero salts, and `published = 0` and `2^31-1`. |
| Post-freeze Emissary black-box differential | `eggstack/emissary@6885a945d25a5ae61bc68191d27c5816bc3df4c9`, public API only; `crates/i2pr-crypto/tests/data/els2-emissary-differential.json` (SHA-256 `cedec8c2a42b056eb53a66cba84343223eb89af87d92bccc62cd37e2877216a2`); `tests/els2_emissary_differential.rs` | PASS, bidirectional. The implementation was frozen at `4e691bd` **before** the oracle ran. Forward: credential, subcredential, the complete 609-byte outer ciphertext, and the encrypted-service address text are all **byte-identical**; the oracle decodes the i2pr address, recovers the exact unblinded public key, and reads the secret-required flag. Reverse: i2pr decrypts the oracle-produced outer ciphertext and recovers the 543-byte inner LeaseSet2 byte-for-byte. No Emissary source was read and the driver lives outside this repository. |
| Java I2P differential | `i2p/i2p.i2p@93eef5d` | **Not executed.** No runnable Java I2P build was provisioned, and the type-11 signature divergence already recorded in Plans 331/336 makes the lane moot for a signed record. Named as a Plan 335 live-lane obligation; not treated as passing. |
| i2pd differential | `PurpleI2P/i2pd@2c694149` | **Not executed for ELS2.** i2pd was read for the layer framing and the initial-counter layout only. The type-11 signature divergence means an i2pr-signed record is unverifiable by i2pd regardless, so a layer differential would not change the interoperability conclusion. |

## Six implementation-discovered findings, all fixed in code

Recorded as worksheet §14.10–§14.15. None was resolved by relaxing a vector or a reference behavior.

1. **The layer keystream's initial block counter is 1, and it is observable.** The specification and the
   pinned i2pd reference agree; the `chacha20` crate's constructor starts at 0, so the wrapper seeks.
   A counter-0 control row makes a regression fail loudly.
2. **ELS2 layer framing overhead is 66 bytes, not 130.** The inner salt lives *inside* the layer-1
   plaintext rather than in the clear beside it. A first-pass resource bound over-counted it. The
   bound is now derived from the framing so the two cannot drift. Over-large would have been
   safety-only; under-large would have rejected valid records at the boundary.
3. **The `expires` offset is bounded by two rules that do not contain each other.** A record published
   early in a UTC day needs nearly 86 400 s to reach midnight, which does not fit two bytes.
   Clamping to midnight and then saturating would have emitted a record expiring minutes before
   midnight while the caller believed it lived until midnight. Frozen as
   `min(requested, until_midnight, 65535)`, checked at every point in the day.
4. **The b33 CRC-32 is a typo detector, not a MAC.** The checksum is folded into the leading three
   bytes, which are therefore a function of the sigtypes and the key, so a bit flip *inside the folded
   prefix* decodes to a different valid address instead of failing. Canonical form is enforced by
   re-encoding, giving one text form per address; the residue is documented rather than hidden.
5. **The lookup secret protects discovery, not content.** It enters `GENERATE_ALPHA`, so it changes
   the blinded key and the storage key; it does not enter the credential or subcredential. A party
   with the address and the record can decrypt them. Building the differential surfaced a live
   hazard here: the resolver initially held a *second* identity built without the secret, which
   defeated the storage-key gate. The resolver now holds exactly one identity.
6. **ELS2 layers are malleable by construction.** A layer is a raw stream with no authenticator, so a
   flipped ciphertext byte corrupts the plaintext and may still parse. Integrity is asserted at the
   record signature and at the inner LeaseSet2's own signature, and the tamper test says so rather
   than claiming the layer detects tampering.

## Verification

Local commands and outcomes (2026-10-04, Rust 1.95.0, Linux x86_64):

- `cargo fmt --all --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` — passed: **3,860 passed, 0 failed,
  35 ignored across 139 suites** (baseline before this plan: 3,796 across 137). The Plan 332
  contribution is 25 NetDB rows, 7 client rows, and 6 crypto rows.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — passed after
  fixing six lints (`should_implement_trait` on `LookupSecret::from_str`, `repeat_once`,
  `bool_comparison`, `redundant_pattern_matching`, two unused-import sets).
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` — passed.
- `cargo test --locked --workspace --doc` — passed.
- `bash scripts/check-dependency-direction.sh` — passed.
- `bash scripts/check-runtime-boundaries.sh` — passed.
- `bash scripts/check-service-tunnel-boundaries.sh` — passed.
- `bash scripts/check-fixture-manifest.sh` — passed. The manifest covers `tests/fixtures/i2np` only and
  does not cover the new `crates/*/tests/data` fixtures, which is recorded rather than assumed.
- `bash scripts/check-constrained-host-lane-boundary.sh` — passed.
- `bash scripts/check-m11-transit-boundaries.sh` — passed.
- `python3 scripts/check-global-plan-number-uniqueness.py` — passed.
- `python3 -m unittest discover -s tests/planning -p 'test_*.py'` — passed.
- `bash scripts/check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`, `check-i2cp-vectors.sh` — passed.
- `bash scripts/check-m11-transit-qualification-evidence.sh`, `check-sam-acceptance-evidence.sh`,
  `check-ssu2-acceptance-evidence.sh`, `check-i2cp-acceptance-evidence.sh`,
  `check-i2pcontrol-acceptance-evidence.sh`, `check-service-tunnel-acceptance-evidence.sh`,
  `check-exploratory-tunnel-evidence.sh`, `check-netdb-tunnel-evidence.sh`,
  `check-destination-tunnel-evidence.sh`, `check-streaming-tunnel-evidence.sh`,
  `check-m6-mixed-router-acceptance-evidence.sh` — all passed.
- `bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test` — passed.
- `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` — passed.
- `cargo deny check advisories bans sources` — advisories ok, bans ok, sources ok. Pre-existing
  duplicate-version warnings are unchanged.
- Emissary differential lane (post-freeze, outside CI): `cargo build -p emissary-core --lib` in
  `eggstack/emissary@6885a945`, then the two external driver crates under `/tmp/ref/`, neither
  committed.

These are local results, not CI claims. The external lanes were not re-run for this plan; the
Java I2P differential remains unexecuted and is a Plan 335 obligation.

## Security review

- **No secret is formatted, cloned, serialized, or logged.** `LayerCipherKey`, `LookupSecret`,
  `BlindingIdentity`, `OwnerBlinding`, and the Red25519 scalar owners erase on drop; `LookupSecret`
  and `BlindingIdentity` are deliberately not `Clone`; `BlindingDay` gained `Ord`/`Hash` so the
  schedule can bound its cache, which changes no key material.
- **The lookup secret never reaches the address, the log, or the record.** A `Debug` implementation
  on `LookupSecret` prints its length only. `ResolvedEncryptedService` has a `Display` that prints
  the address and day and deliberately omits the inner LeaseSet2, because a `Display` impl is one of
  the easiest ways to put routing keys in a log line.
- **A floodfill cannot decrypt what it stores.** It validates the Red25519 signature over the blinded
  key and the freshness window, and it never receives the unblinded public key, so the subcredential
  is not derivable for it. Storing unvalidated records is impossible: `Els2Store` accepts only
  `ValidatedEncryptedLeaseSet2`.
- **A type-5 record cannot be confused with a RouterInfo payload.** `store_message.rs` and
  `lookup_engine.rs` treat it as a non-RouterInfo record, so the maintenance sweep and the
  unsolicited-store path neither index it as a router nor feed it to the Explorer.
- **Blinding cannot be downgraded.** The resolver checks the record's storage key against the key
  derived from the address *before* decrypting, so a record fetched under the wrong day or secret is a
  key mismatch rather than a garbage plaintext.
- **No advertisement.** `specs/support.toml` is unchanged. No capability, version, or RouterInfo
  string changed. No daemon configuration exposes an encrypted-LeaseSet mode; that is Plan 334.

## Compatibility, migration, and failure behavior

- **No migration.** Type 5 is new; no existing record type, stored file, or configuration key changes
  meaning. Ordinary LeaseSet2, LeaseSet, RouterInfo, SAM, I2CP, and Streaming are untouched apart from
  one match arm each in the NetDB dispatch.
- **Existing behavior change, deliberate:** a type-5 `DatabaseStore` body that is not a well-formed
  layer-0 record is now rejected where it was previously accepted as an opaque payload. This is the
  fail-closed behavior a floodfill needs before it can store the record.
- **Failure modes are typed and distinguishable:** framing errors are `CodecError`; validation errors
  are `Els2ValidationError`; layer errors are `Els2Error`; the store reports
  `Inserted`/`Idempotent`/`Replaced`/`Conflict`/`StaleReplacement`/`CapacityExceeded`. A capacity
  rejection never mutates existing state.
- **New dependency:** `chacha20 0.9.1` in `i2pr-crypto`. It was already in the workspace table and
  already locked through `i2pr-transport-ssu2` and `i2pr-tunnel`, so the license (MIT OR Apache-2.0),
  the advisory set, and the `unsafe` surface are unchanged. `default-features = false` and no
  additional features were enabled. `docs/architecture/dependency-graph.md` was amended to list it.

## Limitations and what this plan does not claim

1. **Per-client authorization is not implemented.** PSK and DH are Plan 333. A record with a
   per-client layer-1 flag is refused, not approximated.
2. **Offline (transient) key blocks are parsed and verified, not delivered.** The specification states
   there is no file format and no I2CP enhancement for delivering pre-generated daily keys, so i2pr
   does not claim key batching.
3. **Nothing is published or fetched on a live network.** The client builds a `DatabaseStoreMessage`;
   no daemon driver sends it. Live interoperability is Plan 335 and is expected to be blocked on the
   type-11 signature divergence.
4. **The type-7 path is covered but not recommended.** It works — the converted Ed25519 scalar blinds
   correctly and the credential carries `stA = 7` — but the specification recommends type 11 for new
   encrypted destinations, and i2pr does not claim otherwise.
5. **b33 emission is narrow-form only.** Both forms parse; the 37-byte/two-byte-sigtype form is
   accepted for future use and is never emitted, because both supported sigtypes fit in one byte.
6. **MetaLeaseSet as the inner record is decrypted and structurally validated, with its timestamps
   cross-checked through its header.** Its inner routing semantics are not exercised by a Plan 332
   row; the MetaLeaseSet path is the same code path as LeaseSet2 plus a different store type.

## Unblock audit

| Plan | Pre-closure state | Decision |
|---|---|---|
| 333 PSK + DH client authorization | blocked on 332 | **Unblocked → ready.** Its only hard dependency was this plan. The layer-1 flag layout, the `authCookie` input position in the layer-2 derivation, the `ELS2_XCA`/`ELS2PSKA` labels, and the 52-byte authorization OKM are already frozen and exported as constants, so Plan 333 extends an existing surface rather than creating one. |
| 334 Proposal 170 encrypted-LeaseSet mode mapping | blocked on 333 | Stays blocked behind 333. |
| 335 live ELS2 interoperability | blocked on 334 | Stays blocked behind 334. The Java lane is still unexecuted and the type-11 signature divergence is still decisive; both are named obligations for that plan, not resolved here. |
| 326 encrypted/blinded LeaseSet | historical blocked | Stays blocked; the successor reclosure still belongs to Plan 335. |
| 327, 328, 322 | blocked on router owners | Unchanged. |

Roadmap: `plans/subsystems/red25519-encrypted-leaseset-roadmap.md` §6 records 332 `passed`, 333
`ready`, 334–335 blocked. The branch completion boundary in §7 is now reachable in principle through
the foundation: the record is structurally first-class, the layer cryptography is byte-identical to
an independent implementation in both directions, and a floodfill can store and serve it without
being able to read it.

## Handoff notes for Plan 333

1. `i2pr_netdb::encrypt_no_auth_outer_ciphertext` is the no-auth floor; the authorized form needs the
   same two derivations with `authCookie` in front of the subcredential in the **layer-2** input and
   the per-client block in **layer-1** plaintext. Both input shapes are already derived in the Plan 332
   tests, so the authorized form is a small, well-specified delta.
2. The layer-1 flag layout is exported: `ELS2_LAYER1_FLAG_PER_CLIENT`, `ELS2_LAYER1_SCHEME_DH`,
   `ELS2_LAYER1_SCHEME_PSK`, `ELS2_LAYER1_SCHEME_MASK`, `ELS2_LAYER1_SCHEME_SHIFT`,
   `ELS2_LAYER1_RESERVED_MASK`, `ELS2_AUTH_CLIENTS`, `ELS2_AUTH_CLIENT_LENGTH`.
3. The HKDF info labels for the authorized path are **not** yet exported — `ELS2_XCA` and
   `ELS2PSKA` are 52-byte-OKM derivations and belong with that code. Do not add them preemptively.
4. `decrypt_no_auth_outer_ciphertext` returns `ClientAuthorizationRequired` on any per-client flag;
   Plan 333 replaces that with the real path rather than relaxing the check.
5. Do not weaken the storage-key gate to make an authorized client work. The lookup secret's role is
   documented at `i2pr-client/src/encrypted_leaseset.rs` and worksheet §14.14.
