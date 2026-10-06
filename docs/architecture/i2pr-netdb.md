# `i2pr-netdb` — Deep Dive

Runtime-neutral NetDB record validation and bounded storage: RouterInfo,
classic LeaseSet, LeaseSet2, MetaLeaseSet, and Encrypted LeaseSet2
(DatabaseStore type 5) validation; bounded in-memory stores; SU3 reseed
verification; XOR-distance peer selection; local signed RouterInfo
construction; transport-neutral lookup and publication state machines;
and the M12 floodfill provenance / record / replication / resource layer.

Path: `crates/i2pr-netdb/`

> Status: experimental. Not production-ready. See `README.md` and
> `GUARDRAILS.md`. Nothing in this crate is advertised; see
> [Encrypted LeaseSet2 (type 5) and the floodfill floor](#encrypted-leaseset2-type-5-and-the-floodfill-floor)
> for the exact per-surface status.

## Purpose

`i2pr-netdb` is the first stateful router-information subsystem in
`i2pr`. It owns:

- cryptographic, freshness, and key-binding validation for `RouterInfo`
  records (`ValidatedRouterInfo`);
- validation and bounded storage for the supported record floor —
  classic LeaseSet (type 1, `lease_set`), LeaseSet2 (type 3,
  `lease_set2`), MetaLeaseSet (type 7) — plus the Plan 272 server-side
  variants;
- Encrypted LeaseSet2 (type 5) validation, layer cryptography, the
  per-UTC-day blinding lifecycle, and a bounded store (`els2`,
  `els2_auth`);
- bounded in-memory stores with deterministic
  replacement/conflict/expiry semantics (`RouterInfoStore`,
  `LeaseSet2Store`, `Els2Store`);
- the I2P Base64 codec used by reseed filenames (`base64`);
- SU3 reseed container parsing and RSA-SHA512-4096 signature
  verification over a bounded ZIP archive (`reseed`);
- pure routing primitives and the daily routing-key derivation
  (`routing`);
- local signed RouterInfo construction with a non-advertisement guard
  (`local`);
- the typed lookup identity, the exploratory-tunnel reply-path token,
  the waiter-set/coalescing primitives, the iterative `DatabaseLookup`
  state machine, and the bounded `DatabaseStore` decompressor
  (`lookup_id`, `lookup_action`, `lookup_policy`, `databaselookup`,
  `lookup_engine`);
- the local RouterInfo publication coordinator (`publication`) and the
  bounded unsolicited `DatabaseStore` handler (`store_message`);
- the M12 floodfill layer: provenance policy, server-authority record
  admission, the bounded DatabaseStore/DatabaseLookup service, direct
  replication planning, drop-released resource leases, and the
  role/advertisement vocabulary (`provenance`, `server_store`,
  `floodfill_service`, `replication`, `resource`, `floodfill_role`).

It must **not** own:

- **Filesystem I/O.** No `std::fs`, no file handles, no cache paths.
  Persistence, the reseed ingestor, and the Plan 276 floodfill envelope
  belong to [`i2pr-netdb-persist`](i2pr-netdb-persist.md), which composes
  the narrow APIs here. The raw-byte cache seam itself belongs to
  `i2pr-storage`.
- **Sockets, timers, channels, cancellation, DNS, or network
  acquisition.** This crate performs no download of any kind; the
  offline SU3 byte path is the only acquisition input. All Tokio,
  socket, and timer ownership is `i2pr-runtime`.
- **Unbounded growth.** Every store, buffer, waiter set, and resource
  budget is a caller-visible bound with a fail-closed outcome.

## Module layout

`crates/i2pr-netdb/src/` holds 23 modules plus `lib.rs`
(16 291 lines total; line counts from `wc -l`).

| Module | File | Lines | Responsibility | Key public types |
| --- | --- | ---: | --- | --- |
| `base64` | `base64.rs` | 315 | Strict bounded I2P Base64 alphabet codec (`-`/`~` in the `+`/`/` slots, `=` padding) for reseed filenames | `I2pBase64Error`, `MAX_DECODED_LEN`, `encode`, `decode`, `encode_filename_prefix` |
| `databaselookup` | `databaselookup.rs` | 188 | Standards-conformant `DatabaseLookupMessage` builder | `DatabaseLookupBuildError`, `build_databaselookup` |
| `els2` | `els2.rs` | 1868 | Plan 332 type-5 credential/subcredential derivation, two layer key derivations and ChaCha20 layers, encrypt/decrypt, signature/freshness/key-binding validation, bounded `Els2Store`, per-UTC-day `BlindingSchedule` | `ValidatedEncryptedLeaseSet2`, `Els2Store`, `Els2StoreConfig`, `Els2StoreStats`, `Els2InsertOutcome`, `BlindedStorageKey`, `BlindingIdentity`, `BlindingSchedule`, `DailyBlinding`, `OwnerBlinding`, `Els2Credentials`, `DecryptedEls2`, `Layer1Authorization`, `Els2ValidationPolicy` / `Els2ValidationContext` / `Els2ValidationError`, `Els2Error` |
| `els2_auth` | `els2_auth.rs` | 1125 | Plan 333 per-client ELS2 authorization: PSK and X25519 derivations, bounded `AuthBlock` codec, constant-time `recover_auth_cookie`, `Els2AuthorizationServerConfig`, `Els2ClientAuthSecret` lifecycle | `AuthBlock`, `AuthClientEntry`, `AuthClientMaterial`, `AuthClientPublicKey`, `AuthCookie`, `ClientName`, `PskClientKey`, `Els2ClientAuth`, `Els2AuthScheme`, `Els2AuthSecretRole`, `Els2ClientAuthSecret`, `Els2AuthorizationServerConfig`, `Els2AuthError` |
| `floodfill_role` | `floodfill_role.rs` | 301 | Daemon-owned floodfill eligibility and advertisement-authority vocabulary; qualified-SSU2 address predicate | `FloodfillRoleController`, `FloodfillRoleState`, `FloodfillRoleEffect`, `FloodfillEligibilitySnapshot`, `FloodfillAdvertisementPermit`, `LoopbackReachabilityProof`, `is_qualified_ssu2_address` |
| `floodfill_service` | `floodfill_service.rs` | 1601 | Plan 273/274 synchronous bounded DatabaseStore admission plus DatabaseLookup selection and one-shot supplied-key ECIES reply effects | `FloodfillStoreService`, `FloodfillStorePolicy`, `FloodfillStoreStats`, `FloodfillStoreEffect`, `FloodfillAck`, `FloodfillIngress`, `FloodfillLookupEffect`, `FloodfillReplyIntent`, `FloodfillRole`, `FloodfillTime`, `ReplyProtection`, `LookupFailure`, `ReplicationCandidate` |
| `lease_set` | `lease_set.rs` | 574 | Plan 272 validated classic LeaseSet and MetaLeaseSet plus their bounded stores | `ValidatedLeaseSet`, `ValidatedMetaLeaseSet`, `LeaseSetValidationContext`, `LeaseSetValidationError`, `LeaseSetStore`, `LeaseSetStoreConfig`, `LeaseSetInsertOutcome`, `MetaLeaseSetStore`, `MetaLeaseSetStoreConfig`, `MetaLeaseSetInsertOutcome` |
| `lease_set2` | `lease_set2.rs` | 1000 | Plan 119 Standard LeaseSet2 validation, freshness policy, disclosure block, and bounded store | `ValidatedLeaseSet2`, `DestinationHash`, `LeaseSet2ValidationPolicy` / `LeaseSet2ValidationContext` / `LeaseSet2ValidationError`, `LeaseSetDisclosureBlock`, `LeaseSet2Store`, `LeaseSet2StoreConfig`, `LeaseSet2StoreStats`, `LeaseSet2InsertOutcome` |
| `local` | `local.rs` | 703 | Local signed RouterInfo builder, controlled option declaration, self-validation, non-advertisement guard | `LocalRouterInfoBuilder`, `LocalRouterInfo`, `LocalRouterInfoError`, `controlled_router_options`, `CONTROLLED_ROUTER_VERSION`, `CONTROLLED_NET_ID` |
| `lookup_action` | `lookup_action.rs` | 476 | Typed action vocabulary emitted by the lookup machine; bounded gzip decompression; reply-path sink trait | `LookupAction`, `LookupOutcome`, `LookupFinalState`, `DecompressionError`, `ReplyPathSink`, `decompress_router_info`, `MAX_COMPRESSED_ROUTER_INFO_BYTES`, `MAX_DECOMPRESSED_ROUTER_INFO_BYTES`, `LOOKUP_EXCLUDED_PEER_BUDGET` |
| `lookup_engine` | `lookup_engine.rs` | 1364 | Iterative `RouterInfo`/`LeaseSet2` lookup state machine and bounded coalescing | `RouterInfoLookup`, `CoalescedRouterInfoLookup`, `LookupResult`, `LookupEngineError`, `LookupDiagnostics`, `StartOutcome`, `ResponseOutcome`, `DeliveryOutcome`, `handle_database_store`, `handle_database_store_lease_set2`, `handle_search_reply`, `handle_delivery_outcome`, `handle_databasestore_message`, `handle_searchreply_message` |
| `lookup_id` | `lookup_id.rs` | 357 | Typed lookup identity, exploratory reply-path token, bounded waiter/target sets, `ReplyPathProvider` | `LookupId`, `LookupKind`, `ReplyPath`, `ReplyPathError`, `ReplyPathProvider`, `WaiterSet`, `CoalescedTargets`, `router_hash_from_proto_hash`, `router_hash_from_destination`, `MAX_WAITERS_PER_LOOKUP`, `MAX_COALESCED_LOOKUPS` |
| `lookup_policy` | `lookup_policy.rs` | 468 | Bounded lookup budgets and nearest-N floodfill candidate selection | `LookupPolicy`, `LookupPolicyError`, `FloodfillSelection`, `select_floodfill_candidates`, `MAX_SUGGESTED_HASH_LIMIT`, `DEFAULT_*` budget constants |
| `provenance` | `provenance.rs` | 476 | Plan 271 bounded privacy-safe provenance metadata and the pure namespace/disclosure eligibility policy | `ProvenanceIndex`, `RecordProvenance`, `RecordId`, `NetDbNamespace`, `ClientNamespaceId`, `InboundProvenance`, `StorePurpose`, `ProvenanceLimits`, `Eligibility` (re-exported as `ProvenanceEligibility`) |
| `publication` | `publication.rs` | 696 | Local RouterInfo publication coordinator emitting one bounded `DatabaseStore` per nearest floodfill | `PublicationCoordinator`, `PublicationAttempt`, `PublicationAttemptRecord`, `PublicationAttemptState`, `PublicationCorrelation`, `PublicationSnapshot`, `PublicationError`, `MAX_PUBLICATION_ATTEMPTS` |
| `replication` | `replication.rs` | 308 | Plan 275 bounded source-excluding direct DatabaseStore action planning over current/next daily routing keys | `ReplicationPlanner`, `ReplicationPolicy`, `ReplicationPlan`, `ReplicationStats`, `ReplicationError`, `DirectFloodAction`, `FloodfillPeerView`, `RoutingKeyClass`, `nearest_peers` |
| `reseed` | `reseed.rs` | 1547 | SU3 framing, signer trust, RSA-SHA512-4096 verification, bounded ZIP ingestion, per-entry RouterInfo validation | `parse_su3`, `verify_su3`, `verify_su3_with_signers`, `verify_su3_archive`, `trust_signer_from_certificate`, `ReseedSignerTrustSet`, `TrustedSigner`, `ReseedSignerId`, `ReseedLimits`, `ReseedSignatureType`, `ReseedVerifyReport`, `ReseedVerifyOutcome`, `ReseedEntryReport`, `ReseedEntryState`, `ReseedVerifiedBundle`, `ReseedParseError`, `ReseedTrustError`, `ReseedEntryError`, `MAX_*` scan budgets |
| `resource` | `resource.rs` | 154 | Plan 276 shared floodfill admission budgets with drop-released leases | `FloodfillResourceBudget`, `FloodfillResourcePolicy`, `FloodfillResourceSnapshot`, `ResourceKind`, `ResourceLease` |
| `router_info` | `router_info.rs` | 598 | RouterInfo validation, `RouterHash` derivation, and the validated boundary | `ValidatedRouterInfo`, `RouterHash`, `router_hash`, `RouterInfoValidationPolicy` (also re-exported as `ValidationPolicy`), `ValidationContext`, `RouterInfoValidationError` |
| `routing` | `routing.rs` | 384 | XOR distance, daily routing-key derivation, `NearestSelection` ordering | `xor_distance`, `daily_routing_key`, `format_daily_key`, `NearestSelection`, `RoutingKeyError` |
| `server_store` | `server_store.rs` | 731 | Plan 272 main-router-only, explicit-provenance validated-record admission with aggregate count/byte caps and answer-safe getters | `ServerNetDb`, `ServerNetDbConfig`, `ValidatedNetDbRecord`, `ServerInsertOutcome`, `MaintenanceBatch`, `daily_rollover_due` |
| `store` | `store.rs` | 584 | Bounded in-memory `RouterInfoStore` with checked arithmetic and deterministic replacement semantics | `RouterInfoStore`, `RouterInfoStoreConfig`, `RouterInfoStoreStats`, `InsertOutcome` |
| `store_message` | `store_message.rs` | 318 | Plan 105 bounded ingestion handler for `DatabaseStore` arriving outside an active lookup | `handle_unsolicited_databasestore`, `UnsolicitedStorePolicy`, `UnsolicitedStoreOutcome`, `UnsolicitedStoreError` |
| *(root)* | `lib.rs` | 155 | `#![forbid(unsafe_code)]`, 23 private module declarations, and the crate's whole public surface | — (re-exports only) |

## Public surface

Every module is `mod`-private. `lib.rs` re-exports the entire public
surface; nothing is reachable by module path. The re-exports are:

```text
base64        I2pBase64Error, MAX_DECODED_LEN, decode, encode, encode_filename_prefix
databaselookup DatabaseLookupBuildError, build_databaselookup
els2          BlindedStorageKey, BlindingIdentity, BlindingSchedule, BlindingScheduleConfig,
              DailyBlinding, DecryptedEls2, ELS2_AUTH_CLIENT_LENGTH,
              ELS2_CREDENTIAL_PERSONALIZATION, ELS2_LAYER_KEY_MATERIAL_LENGTH,
              ELS2_LAYER1_FLAG_PER_CLIENT, ELS2_LAYER1_HKDF_INFO, ELS2_LAYER1_RESERVED_MASK,
              ELS2_LAYER1_SCHEME_DH, ELS2_LAYER1_SCHEME_MASK, ELS2_LAYER1_SCHEME_PSK,
              ELS2_LAYER1_SCHEME_SHIFT, ELS2_LAYER2_HKDF_INFO, ELS2_SALT_LENGTH,
              ELS2_SUBCREDENTIAL_PERSONALIZATION, Els2Credentials, Els2Error, Els2InsertOutcome,
              Els2Store, Els2StoreConfig, Els2StoreStats, Els2ValidationContext,
              Els2ValidationError, Els2ValidationPolicy, Layer1Authorization, LookupSecret,
              MAX_ELS2_AUTH_CLIENTS, MAX_ELS2_INNER_LEASE_SET_LENGTH,
              MAX_ELS2_OUTER_CIPHERTEXT_LENGTH, MAX_ELS2_RECORD_LENGTH, OwnerBlinding,
              ValidatedEncryptedLeaseSet2, day_bound_expiry_offset, decrypt_no_auth_outer_ciphertext,
              decrypt_outer_ciphertext, derive_els2_credentials, encrypt_no_auth_outer_ciphertext,
              encrypt_outer_ciphertext, next_utc_day_boundary_seconds,
              unblinded_scalar_from_ed25519_seed, utc_blinding_day
els2_auth     AuthBlock, AuthClientEntry, AuthClientMaterial, AuthClientPublicKey, AuthCookie,
              ClientName, ELS2_AUTH_CLIENT_ID_LENGTH, ELS2_AUTH_COOKIE_LENGTH, ELS2_AUTH_OKM_LENGTH,
              ELS2_DH_AUTH_HKDF_INFO, ELS2_DH_AUTH_KEY_TYPE_CODE, ELS2_PSK_AUTH_HKDF_INFO,
              Els2AuthError, Els2AuthScheme, Els2AuthSecretRole, Els2AuthorizationServerConfig,
              Els2ClientAuth, Els2ClientAuthSecret, MAX_ELS2_CLIENT_NAME_LENGTH, PskClientKey,
              build_dh_block, build_psk_block, dh_client_material, draw_generation_secrets,
              psk_client_material, recover_auth_cookie
floodfill_role      FloodfillAdvertisementPermit, FloodfillEligibilitySnapshot,
                    FloodfillRoleController, FloodfillRoleEffect, FloodfillRoleState,
                    LoopbackReachabilityProof, is_qualified_ssu2_address
floodfill_service   FloodfillAck, FloodfillIngress, FloodfillLookupEffect, FloodfillReplyIntent,
                    FloodfillRole, FloodfillStoreEffect, FloodfillStorePolicy, FloodfillStoreService,
                    FloodfillStoreStats, FloodfillTime, LookupFailure, ReplicationCandidate,
                    ReplyProtection
lease_set     LeaseSetInsertOutcome, LeaseSetStore, LeaseSetStoreConfig, LeaseSetValidationContext,
              LeaseSetValidationError, MetaLeaseSetInsertOutcome, MetaLeaseSetStore,
              MetaLeaseSetStoreConfig, ValidatedLeaseSet, ValidatedMetaLeaseSet
lease_set2    DestinationHash, LeaseSet2InsertOutcome, LeaseSet2Store, LeaseSet2StoreConfig,
              LeaseSet2StoreStats, LeaseSet2ValidationContext, LeaseSet2ValidationError,
              LeaseSet2ValidationPolicy, LeaseSetDisclosureBlock, ValidatedLeaseSet2
local         CONTROLLED_NET_ID, CONTROLLED_ROUTER_VERSION, LocalRouterInfo,
              LocalRouterInfoBuilder, LocalRouterInfoError, controlled_router_options
lookup_action DecompressionError, LOOKUP_EXCLUDED_PEER_BUDGET, LookupAction, LookupFinalState,
              LookupOutcome, MAX_COMPRESSED_ROUTER_INFO_BYTES,
              MAX_DECOMPRESSED_ROUTER_INFO_BYTES, ReplyPathSink, decompress_router_info
lookup_engine CoalescedRouterInfoLookup, DeliveryOutcome, LookupDiagnostics, LookupEngineError,
              LookupResult, ResponseOutcome, RouterInfoLookup, StartOutcome, handle_database_store,
              handle_database_store_lease_set2, handle_databasestore_message,
              handle_delivery_outcome, handle_search_reply, handle_searchreply_message
lookup_id     CoalescedTargets, LookupId, LookupKind, MAX_COALESCED_LOOKUPS, MAX_WAITERS_PER_LOOKUP,
              ReplyPath, ReplyPathError, ReplyPathProvider, WaiterSet,
              router_hash_from_destination, router_hash_from_proto_hash
lookup_policy DEFAULT_MAX_CANDIDATES_CONSIDERED, DEFAULT_MAX_PEERS_PER_LOOKUP,
              DEFAULT_MAX_SUGGESTED_HASHES, DEFAULT_PER_ATTEMPT_DEADLINE_MS,
              DEFAULT_SUGGESTED_HASH_LIMIT, DEFAULT_TOTAL_DEADLINE_MS, FloodfillSelection,
              LookupPolicy, LookupPolicyError, MAX_SUGGESTED_HASH_LIMIT, select_floodfill_candidates
provenance    ClientNamespaceId, Eligibility as ProvenanceEligibility, InboundProvenance,
              NetDbNamespace, ProvenanceIndex, ProvenanceLimits, RecordId, RecordProvenance,
              StorePurpose
publication   MAX_PUBLICATION_ATTEMPTS, PublicationAttempt, PublicationAttemptRecord,
              PublicationAttemptState, PublicationCoordinator, PublicationCorrelation,
              PublicationError, PublicationSnapshot
replication   DirectFloodAction, FloodfillPeerView, ReplicationError, ReplicationPlan,
              ReplicationPlanner, ReplicationPolicy, ReplicationStats, RoutingKeyClass,
              nearest_peers
reseed        ReseedEntryReport, ReseedEntryState, ReseedLimits, ReseedSignatureType,
              ReseedSignerId, ReseedSignerTrustSet, ReseedTrustError, ReseedVerifiedBundle,
              ReseedVerifyOutcome, ReseedVerifyReport, TrustedSigner, parse_su3,
              trust_signer_from_certificate, verify_su3, verify_su3_archive, verify_su3_with_signers
resource      FloodfillResourceBudget, FloodfillResourcePolicy, FloodfillResourceSnapshot,
              ResourceKind, ResourceLease
router_info   RouterHash, RouterInfoValidationError, RouterInfoValidationPolicy, ValidatedRouterInfo,
              ValidationContext, ValidationPolicy (alias), router_hash
routing       NearestSelection, RoutingKeyError, daily_routing_key, format_daily_key, xor_distance
server_store  MaintenanceBatch, ServerInsertOutcome, ServerNetDb, ServerNetDbConfig,
              ValidatedNetDbRecord, daily_rollover_due
store         InsertOutcome, RouterInfoStore, RouterInfoStoreConfig, RouterInfoStoreStats
store_message UnsolicitedStoreError, UnsolicitedStoreOutcome, UnsolicitedStorePolicy,
              handle_unsolicited_databasestore
```

Items that exist in a module but are deliberately **not** re-exported,
because they are module-internal helpers rather than surface:

- `routing::nearest` and `routing::nearest_floodfill` — present,
  `#[allow(dead_code)]`, and used by the in-module tests; not part of the
  public surface. Use `lookup_policy::select_floodfill_candidates` for
  bounded floodfill selection.
- `router_info::reencode_router_info`, `router_info::DEFAULT_MAX_AGE`,
  `DEFAULT_MAX_FUTURE_SKEW`, `DEFAULT_MAX_ENCODED_LEN` — module-visible
  policy defaults folded into `RouterInfoValidationPolicy::default_const`.
- `local::LocalRouterInfoSummary`, `local::assert_no_forbidden_caps`,
  `local::options_to_sorted_entries`, `local::options_to_btree`.
- `reseed::router_info_filename_hash`, and the `MAX_SU3_BYTES` /
  `MAX_ARCHIVE_*` / `MAX_ENTRY_UNCOMPRESSED_BYTES` / `MAX_SIGNER_ID_LEN` /
  `MAX_VERSION_LEN` / `RESEED_FILE_TYPE` / `RESEED_CONTENT_TYPE`
  constants, which are folded into `ReseedLimits::default`.
- `els2_auth::ELS2_AUTH_KEY_LENGTH`, `ELS2_AUTH_IV_LENGTH` and
  `els2::ELS2_LAYER1_FLAGS_NO_AUTH` — fixed by the specification and used
  by the codecs.
- `els2::ValidatedEncryptedLeaseSet2::validate` and
  `els2::Els2Store::insert` are public but reachable only through the
  re-exported type.

## Key contracts

### `ValidatedRouterInfo` — decodable vs. validated

`ValidatedRouterInfo::from_router_info` is the **only** constructor; the
type has private fields and there is no unchecked insertion path. It
enforces the Plan 103 §2.3 fail-closed order
(`router_info.rs:232-309`):

1. **Length** — `signed_bytes().len() + signature().as_bytes().len()`,
   computed with `checked_add`; over the cap is
   `EncodedTooLarge { actual, maximum }`, arithmetic overflow is
   `ArithmeticOverflow`.
2. **Key derivation** — `router_hash()` delegates to
   `i2pr_crypto::router_identity_hash`, i.e. SHA-256 over the **canonical
   encoded `RouterIdentity` only**. It is not the whole RouterInfo, not
   the signing key alone, and not a debug representation. A *decodable*
   `RouterInfo` from `i2pr-proto` therefore has no NetDB identity until
   this step runs.
3. **Expected-key check** — when the caller supplies `expected_key`
   (lookup answers, unsolicited stores, reseed entries) it must equal the
   derived `RouterHash`, else `KeyMismatch`.
4. **Algorithm support** — the identity's signing key must advertise a
   `public_key_len`; otherwise `UnsupportedAlgorithm { algorithm }`. The
   category is kept distinct from `InvalidSignature` so protocol drift is
   not masked.
5. **Signature verification** — `i2pr_crypto::verify_router_info` against
   the signing key **embedded in the record itself**, over the record's
   own retained signed bytes. This is a self-consistency check, not a
   trust decision: the key material is the attacker's own key, so a
   valid `ValidatedRouterInfo` proves only that the bytes are internally
   consistent. Trust for reseed bytes comes from `reseed`, and trust for
   a peer comes from transport/provenance.
6. **Freshness** — against caller-supplied `ValidationContext::now`; the
   validator never calls `SystemTime::now()`. Too old is `Stale`; too far
   ahead is `ExcessiveFuture`.
7. **Wrap.**

`RouterInfoValidationError` variants: `EncodedTooLarge { actual, maximum }`,
`UnsupportedAlgorithm { algorithm }`, `InvalidSignature`, `KeyMismatch`,
`Stale { age_secs, max_age_secs }`,
`ExcessiveFuture { skew_secs, max_skew_secs }`, `ArithmeticOverflow`,
`Crypto(CryptoError)`.

Defaults (`router_info.rs:20-30`): `DEFAULT_MAX_AGE` = 24 h,
`DEFAULT_MAX_FUTURE_SKEW` = 1 h, `DEFAULT_MAX_ENCODED_LEN` = 16 KiB.
`RouterInfoValidationPolicy::default_const()` is `const`-evaluable.

`advertises_floodfill()` reports the presence of the signed `f` letter in
`caps` and is documented as signed-but-self-asserted data: it never
implies honesty, health, or trust.

### Bounded stores: capacity, replacement, conflict, expiry

`RouterInfoStore::insert` is the only public entry point and takes a
`ValidatedRouterInfo` by value (`store.rs:118-174`):

| Incoming vs. existing | Outcome |
| --- | --- |
| newer `published` | `Replaced` (byte budget re-checked with `checked_add`) |
| older `published` | `StaleReplacement` |
| equal `published`, byte-identical record | `Idempotent` (no-op) |
| equal `published`, different signed bytes | `Conflict` (existing retained) |
| new key, over count or byte budget | `CapacityExceeded` (no mutation) |

Default `RouterInfoStoreConfig`: `max_records = 4_096`,
`max_total_encoded_bytes = 4 MiB`. A zero in either field is an explicit
reject-everything configuration. **There is no eviction** — capacity
exceeded is fail-closed, and reclamation is the caller's `remove` /
`prune(now, max_age_ms)` call.

All other stores mirror the same outcome vocabulary with their own
budgets:

| Store | `max_records` | `max_total_encoded_bytes` | Outcome enum |
| --- | ---: | ---: | --- |
| `RouterInfoStore` | 4 096 | 4 MiB | `InsertOutcome` |
| `LeaseSet2Store` | 4 096 | 4 MiB | `LeaseSet2InsertOutcome` |
| `Els2Store` | 4 096 | 8 MiB | `Els2InsertOutcome` |
| `LeaseSetStore` (type 1) | 2 048 | 2 MiB | `LeaseSetInsertOutcome` |
| `MetaLeaseSetStore` (type 7) | 1 024 | 2 MiB | `MetaLeaseSetInsertOutcome` |
| `ServerNetDb` | 8 192 | 16 MiB | `ServerInsertOutcome` |
| `ProvenanceIndex` | 8 192 | 512 KiB (64 B/entry) | `Eligibility::CapacityExceeded` |

`LeaseSet2Store` is keyed by `DestinationHash` and budgeted independently
of `RouterInfoStore`, so neither entry class can starve the other.

### I2P Base64 rules (`base64.rs`)

The I2P alphabet is
`ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~` —
slots 62/63 are `-` (0x2D) and `~` (0x7E) in place of RFC 4648's `+`
and `/`. Padding is `=`, **not** `~`; `~` is the value-63 digit. A
filename that embeds a RouterHash must use exactly this alphabet, because
the RFC 4648 alphabet encodes to a different hash. The frozen form is
Plan 142 (see [closure record](../../plans/closure/sam/142-status.md)).

The codec is strict and bounded (`base64.rs:132-186`):

- length must be a multiple of four → `InvalidLength { actual }`;
- any byte outside the alphabet, **including** `+`, `/`, and `?`, →
  `InvalidCharacter { index, byte }`;
- `=` is legal only at index 2 or 3 of a chunk, and never before a
  non-padding byte → `InvalidPadding { index }`;
- decoded length over `MAX_DECODED_LEN = 512` → `DecodedTooLarge { actual,
  maximum }`.

Output length is always `4 * ceil(n / 3)`. A 32-byte SHA-256 RouterHash
encodes to exactly 44 characters; `encode_filename_prefix` is the
reseed-filename form. `I2pBase64Error` variants: `InvalidLength`,
`InvalidCharacter`, `InvalidPadding`, `DecodedTooLarge`.

### SU3 reseed (`reseed.rs`)

`parse_su3` validates the container framing only; the archive is not
touched until trust has been established. `verify_su3` /
`verify_su3_with_signers` / `verify_su3_archive` are the full pipeline.
`i2pr-su3` owns the framing; `sad-rsa` (the Marvin-Attack-hardened
pure-Rust fork of `rsa`, which removes RUSTSEC-2023-0071) owns PKCS#1
v1.5 + SHA-512. This crate implements no RSA primitive.

`ReseedSignatureType` has exactly one variant, `RsaSha512_4096`, wire
code `6`, signature length `ceil(key_bits / 8)`. Any other SU3 signature
type is `UnsupportedSignatureType`; a header signature length that
disagrees with the certificate is `SignatureLengthMismatch`; a non-zero
reserved byte is `NonZeroReserved`.

`ReseedSignerTrustSet` maps `ReseedSignerId` to `TrustedSigner`; the
verifier requires an **exact** signer match (`UnknownSigner`) before any
signature work, and enforces the certificate's `NotBefore` / `NotAfter`
(`CertificateExpired`). `ReseedSignerId` rejects anything over
`MAX_SIGNER_ID_LEN = 256` UTF-8 bytes.

**ZIP posture.** The suite accepts only `Stored` and `Deflated` entries
(`UnsupportedCompressionMethod` otherwise). There is no explicit
expansion-*ratio* check; the defence is a set of independent absolute
ceilings, each enforced both against the declared ZIP size and again
against the bytes actually read, so a lying header cannot help:

| Bound | Value | Error |
| --- | ---: | --- |
| `MAX_SU3_BYTES` (single file) | 8 MiB | `LengthExceeded` |
| `MAX_ARCHIVE_ENTRIES` | 4 096 | `ArchiveEntriesExceeded` |
| `MAX_ENTRY_UNCOMPRESSED_BYTES` (per entry) | 256 KiB | `EntryTooLarge` |
| `MAX_ARCHIVE_UNCOMPRESSED_BYTES` (cumulative) | 64 MiB | `ArchiveUncompressedBytesExceeded` |
| `MAX_ARCHIVE_ROUTERINFO_BYTES` (accepted records) | 32 MiB | `ArchiveRouterInfoBytesExceeded` |
| per-record RouterInfo | `MAX_COMMON_STRUCTURE_SIZE` | `ReseedEntryError::RouterInfoInvalid` |

Entry names are checked for path validity (`InvalidPath`) and duplicates
(`DuplicateEntry`). Each entry's filename must carry the I2P Base64
RouterHash prefix and that hash must equal the hash derived from the
contained `RouterIdentity` (`RejectedFilename` /
`RouterHashMismatch` / `RejectedValidation`).

Failure policy (`reseed.rs:11-21`): a trust, framing, or aggregate-limit
failure accepts **zero** records and mutates nothing. A single bad
RouterInfo inside an otherwise authentic archive is counted and the scan
continues. Ingestion into a store is `i2pr-netdb-persist`'s job, not
this crate's.

`ReseedParseError` variants: `MagicMismatch`,
`UnsupportedFormatVersion { .. }`, `NonZeroReserved { .. }`,
`SignatureLengthMismatch { .. }`, `LengthExceeded { .. }`,
`Truncated { .. }`, `InvalidSignerId`, `InvalidVersion`,
`UnsupportedSignatureType { .. }`, `UnsupportedFileType`,
`UnsupportedContentType`, `ArchiveEntriesExceeded { .. }`,
`ArchiveUncompressedBytesExceeded { .. }`,
`ArchiveRouterInfoBytesExceeded { .. }`, `EntryTooLarge { .. }`,
`UnsupportedCompressionMethod { .. }`, `InvalidPath { .. }`,
`DuplicateEntry { .. }`, `ZipDecode`, `SignatureInvalid`, `UnknownSigner`,
`CertificateExpired`, `SignedRegionMismatch`.

`ReseedTrustError` variants: `SignerIdTooLong { .. }`,
`UnsupportedKeyType { .. }`, `CertificateNotValid { .. }`.
`ReseedEntryError` variants: `InvalidFilename { .. }`,
`RouterHashMismatch { .. }`, `RouterInfoInvalid { .. }`.
`ReseedEntryState`: `Accepted`, `RejectedFilename`, `RejectedDecode`,
`RejectedValidation`. `ReseedVerifyOutcome`: `Accepted { accepted }`,
`RejectedTrust { reason }`.

### XOR-distance peer selection (`routing.rs`)

- `xor_distance(left, right)` is a 32-byte bytewise XOR, allocation-free.
- The distance metric is compared **lexicographically as a big-endian
  256-bit integer**, the Kademlia convention.
- `NearestSelection` implements `Ord` as `distance` ascending, then
  **`RouterHash` ascending** as the deterministic tie-break
  (`routing.rs:132-140`). Two routers at the same XOR distance therefore
  always order identically across processes and runs.
- `daily_routing_key(search_key, now)` is
  `SHA256(search_key || UTC_yyyyMMdd[8])`. `format_daily_key` produces
  exactly 8 ASCII bytes via Howard Hinnant's proleptic-Gregorian
  conversion; dates outside year 0..=9999 are `RoutingKeyError::DateOutOfRange { days }`
  rather than silently truncated. The function never reads a clock and
  never widens to local time, so the same `Date` always yields the same
  key on any host.
- `lookup_policy::select_floodfill_candidates` applies the distance
  ordering under three simultaneous bounds — the caller's candidate
  ceiling, the policy peer budget, and the caller's exclusion set.

### Transport-neutral lookup and publication machines

`LookupKind` has two variants with I2NP lookup-type bits:
`RouterInfo` → `2`, `LeaseSet2` → `1`. `LookupId` is
`(request_id, kind, target RouterHash)`; the machine never aliases a
caller-supplied request id.

- `lookup_action::LookupAction` — `SendDatabaselookup { .. }` carrying the
  typed `DatabaseLookupMessage` to dispatch, `NeedExploratoryReplyPath`,
  `Complete { .. }`.
- The machine **refuses to emit a standards-conformant `DatabaseLookup`
  until a `ReplyPath` has been supplied** (`StartOutcome::NeedsReplyPath`).
  A direct peer link is not equivalent to a reply path.
- `databaselookup::build_databaselookup` puts the **raw `RouterHash`** on
  the wire as the key — not the daily routing key — takes `from` /
  reply-tunnel fields from the supplied reply path, and bounds excluded
  peers by `LOOKUP_EXCLUDED_PEER_BUDGET = 256`.
- `decompress_router_info` enforces `MAX_COMPRESSED_ROUTER_INFO_BYTES = 16 KiB`
  and `MAX_DECOMPRESSED_ROUTER_INFO_BYTES = 32 KiB` **before** allocating
  the output buffer.
- **Bounded duplicate suppression** is three separate ceilings:
  `MAX_WAITERS_PER_LOOKUP = 32` per active lookup (`WaiterSet`),
  `MAX_COALESCED_LOOKUPS = 8` distinct targets
  (`CoalescedRouterInfoLookup` / `CoalescedTargets`), and
  `MAX_SUGGESTED_HASH_LIMIT = 256` (ceiling) against
  `DEFAULT_SUGGESTED_HASH_LIMIT = 64` on the accepted `DatabaseSearchReply`
  suggestion set. Each `add` returns `bool` and is a no-op at capacity.
- `lookup_policy::LookupPolicy` defaults: `max_peers_per_lookup = 8`,
  `total_deadline_ms = 5_000`, `per_attempt_deadline_ms = 1_500`,
  `max_suggested_hashes = 128`, `max_candidates_considered = 32`. All
  are caller-overridable and validated at construction
  (`LookupPolicyError`).
- `PublicationCoordinator` emits one bounded `DatabaseStore` per nearest
  floodfill with `reply_token = 0` store-and-forget semantics, tracks
  acknowledgement tokens, **never re-signs the local RouterInfo** (a
  retry reuses the originally encoded bytes and the originally minted
  reply token), and surfaces `needs_verification_lookup()` for the
  verification path. `MAX_PUBLICATION_ATTEMPTS = 32` bounds concurrent
  attempts; `PublicationError` is `UnknownRequest { .. }`,
  `AlreadyTerminal { .. }`, `ReplyTokenReuse { .. }`.
  `PublicationAttemptRecord` (Plan 117 §5.2) carries the typed
  `DatabaseStoreMessage`; the runtime owns standard-header field
  assignment and the single canonical envelope encode.
- `store_message::handle_unsolicited_databasestore` refuses any payload
  other than `DatabaseStoreData::RouterInfoCompressed` — `LeaseSet`,
  `LeaseSet2`, `MetaLeaseSet`, `EncryptedLeaseSet`, and `Deferred` are
  all `UnsupportedPayload` — then decompresses under the same ceilings,
  binds the derived RouterHash to the message key (`KeyMismatch`), runs
  the full validator, and maps the `InsertOutcome` onto
  `UnsolicitedStoreOutcome`. `UnsolicitedStorePolicy::Reject` is
  `NotAllowed` before any parsing.

### `ValidatedLeaseSet2` and `LeaseSet2Store`

`ValidatedLeaseSet2::from_lease_set2` is the only constructor and
enforces the Plan 119 Phase H order
(`lease_set2.rs:217-315`): length → key derivation from the embedded
Destination → optional `expected_key` check (`DestinationMismatch`) →
**signature verification against the signing key embedded in the LS2's
own Destination, over the preimage `0x03 || LeaseSet2::signed_bytes()`**
(`InvalidSignature`) → disclosure policy → offline-delegation expiry →
encryption-key policy → freshness → wrap.

- **Encryption-key type and matching.** The record must yield exactly
  one usable X25519 key for ECIES-X25519-AEAD-Ratchet routing:
  none is `NoUsableX25519`, more than one is `DuplicateX25519`.
  `encryption_keys()` exposes the published public key sections
  (`i2pr_proto::LeaseSet2EncryptionKey`) without handing back the wrapped
  record, so a caller cannot bypass the validation gate.
- **Expiry.** `published` beyond
  `max_future_skew_seconds` (default 1 h) is `ExcessiveFuture`;
  `expires <= now` is `Expired`; every lease already past is
  `AllLeasesExpired`.
- **Disclosure.** `is_unpublished()` and
  `disclosure_block() -> Option<LeaseSetDisclosureBlock>` mark a record
  that must not be served (`LeaseSetDisclosureBlock::Unpublished` or
  `::BlindedPublicationDeferred`). A record that *requests*
  blinded-on-publication is rejected outright
  (`BlindedPublicationDeferred`) while type 5 remains the blinding
  mechanism. An expired offline Ed25519 delegation is
  `OfflineSignatureExpired`.
- `LeaseSet2ValidationError` variants: `EncodedTooLarge { actual, maximum }`,
  `InvalidSignature`, `DestinationMismatch`, `NoUsableX25519`,
  `DuplicateX25519`, `BlindedPublicationDeferred`, `OfflineSignatureExpired`,
  `AllLeasesExpired`, `ExcessiveFuture { skew_secs, max_skew_secs }`,
  `Expired { expires_secs, now_secs }`, `ArithmeticOverflow`,
  `Crypto(CryptoError)`.

Classic LeaseSet (type 1) and MetaLeaseSet (type 7) use
`LeaseSetValidationError`: `EncodedTooLarge`, `DestinationMismatch`,
`NoLeases`, `Expired`, `InvalidSignature`, `Crypto(CryptoError)`,
`Codec(CodecError)`. The classic store uses deterministic
**earliest-expiry** replacement; `ValidatedMetaLeaseSet::is_unpublished()`
and `is_blinded_on_publication()` gate MetaLeaseSet disclosure.

### Provenance (`provenance.rs`)

Provenance is deliberately **separate from validation**: a validated
record is not implicitly answerable, replicable, or persistable. Each
record carries `(NetDbNamespace, InboundProvenance, StorePurpose,
observed_at_ms)`.

- `NetDbNamespace` is `MainRouter` or `Client(ClientNamespaceId)`.
- `InboundProvenance` is `AuthenticatedDirectPeer`, `RouterTunnel`,
  `ClientTunnel`, `Local`.
- `StorePurpose` is `PublishedStore`, `LookupResponse`, `FloodReplica`,
  `LocalPublication`.

The four policy predicates return `Eligibility`:

| Predicate | Rule |
| --- | --- |
| `may_answer_router_lookup` | main-router namespace, router-bound inbound, `PublishedStore` |
| `may_answer_client_lookup` | client lookup-response metadata **never** grants router-answer eligibility |
| `may_replicate` | main-router namespace, **token-bearing** `PublishedStore`; a zero-token `FloodReplica` is `ReplicaCannotReflood` and is never re-flooded |
| `may_persist` | main-router namespace and an answerable purpose |

`Eligibility` variants: `Allowed`, `WrongNamespace`, `LookupResponseOnly`,
`ClientTunnelOnly`, `ReplicaCannotReflood`, `Expired`, `NotPublished`,
`CapacityExceeded`. A restored record's provenance is never a silent
default: `i2pr-netdb-persist` restores through mandatory revalidation and
narrows provenance to flood replica.

**Privacy.** `RecordId`'s `key` is rendered `"[redacted]"` in `Debug`, and
`ClientNamespaceId` renders as `ClientNamespaceId(..)`. This crate holds
no peer or namespace identity in any other serialized form.

### Replication (`replication.rs`)

`ReplicationPlanner` produces a bounded, source-excluding
`ReplicationPlan` of `DirectFloodAction`s over the current **and** next
daily routing keys, so a rollover is covered before it happens
(`RoutingKeyClass::Current` / `::Next`, `NEXT_KEY_TARGETS = 2`). Defaults:
`fanout = 3`, `max_candidates = 256`, `max_work = 4 096`,
`max_record_age_ms = 1 h`, `deadline_ms = 30 000`; rollover offsets are
`RI_ROLLOVER_MS = 45 min` and `LS_ROLLOVER_MS = 10 min`. The source peer
is always excluded, and a `DirectFloodAction` carries **no tunnel or
gateway route** — failed direct replication cannot silently name a
fallback. Only token-bearing publisher stores are planned; zero-token
flood replicas are never reflooded. `nearest_peers(peers, target, limit)`
applies the same XOR-distance-then-`RouterHash` ordering.

### Local RouterInfo builder and the non-advertisement guard (`local.rs`)

`LocalRouterInfoBuilder` borrows a `RouterIdentityBundle` for the length
of one call; it never clones private key material and never retains the
bundle. Every build path signs and then immediately self-validates the
record through `ValidatedRouterInfo::from_router_info` before returning
a `LocalRouterInfo`.

- `controlled_router_options()` declares `router.version =
  CONTROLLED_ROUTER_VERSION` (`"0.9.62"`) and `netId = CONTROLLED_NET_ID`
  (`"2"`). `router.version` is an I2NP feature/API level, not a release
  string, and is pinned by a test that fails if the declaration outruns
  the implemented I2NP surface. A controlled record omitting `netId` is
  marked unreachable by the reference, so it is always present.
- **The non-advertisement guard** is `validate_options`
  (`local.rs:268-282`): if a `caps` mapping is present, any of
  `f B K L M N P R S U X` is refused with
  `LocalRouterInfoError::InvalidMapping { context: "caps" }`. That covers
  floodfill (`f`), bandwidth tiering (`B`), and the transport-capability
  letters. The **normal** build path carries zero `RouterAddress` entries,
  and a supplied transport style is `ForbiddenTransport { style }`.
- `build_floodfill`, `build_floodfill_reachable`, and
  `build_floodfill_withdrawal` are the **controlled** paths, and each is
  permit-gated: they require a `FloodfillAdvertisementPermit`, and
  `build_floodfill_reachable` additionally requires a
  `LoopbackReachabilityProof`. A normal daemon config can never mint
  either (`LocalRouterInfoError::UnqualifiedFloodfillAddress` otherwise).
- `LocalRouterInfoError` variants: `ForbiddenTransport { style }`,
  `InvalidMapping { context }`, `SigningFailed`,
  `UnqualifiedFloodfillAddress`, `Validation(RouterInfoValidationError)`.

### Resource accounting owned here (`resource.rs`)

`FloodfillResourceBudget` is a `Clone`-able `Arc<Mutex<..>>` counter set
whose `reserve(kind)` returns `Option<ResourceLease>`; the lease releases
its slot in `Drop`, so every path — success, refusal, error, or teardown —
returns the budget. Defaults: `active_requests = 128`,
`queued_effects = 256`, `queued_bytes = 4 MiB`,
`crypto_validations = 1 024`, `direct_flood_attempts = 128`,
`maintenance_records = 256`. `ResourceKind`: `ActiveRequest`,
`QueuedEffect`, `QueuedBytes(usize)`, `CryptoValidation`,
`DirectFloodAttempt`, `MaintenanceRecord`. `queued_bytes` uses
`checked_add` and refuses rather than wrapping.

This is the **only** interior mutability in the crate; there is no
channel, no queue with an unbounded variant, and no `spawn`.

### Server store and DatabaseStore publication path

`ServerNetDb` is the main-router-only admission gate. It couples a
`ProvenanceIndex` with the validated record stores and **never treats the
client `RouterInfoStore` / `LeaseSet2Store` iterators as answerable
views**. `ValidatedNetDbRecord` is an enum over `RouterInfo`, `LeaseSet`,
`LeaseSet2`, `MetaLeaseSet`, `EncryptedLeaseSet2`. `insert` takes explicit
provenance and validates before any server-authority mutation;
`ServerInsertOutcome` is `Inserted`, `Replaced`, `Idempotent`, `Conflict`,
`Stale`, `CapacityExceeded`. The `*_for_answer` getters and
`database_store_for_answer` consult the provenance predicates first, so a
rejected record is unreachable by construction. `maintenance_batch` and
`daily_rollover_due(last_ms, now_ms)` bound periodic work.

`FloodfillStoreService` (Plans 273–274) accepts only explicit
authenticated ingress classifications (`FloodfillIngress`), validates
before mutation, and returns bounded typed effects
(`FloodfillStoreEffect::Stored { .. }`, `FloodfillAck`,
`ReplicationCandidate`). Zero-token replicas are stored without
acknowledgment and without re-flood eligibility. `handle_lookup` returns
`FloodfillLookupEffect::NoResponse(LookupFailure)` or
`::Reply(FloodfillReplyIntent::{Direct, Tunnel})` plus `ReplyProtection`,
using a **one-shot supplied-key** ECIES path; the service must not reach
for destination session state. `FloodfillStorePolicy` defaults:
`window_ms = 10_000`, `max_global_requests = 4_096`,
`max_source_requests = 64`, `max_key_requests = 32`,
`max_global_bytes = 4 MiB`, `max_source_bytes = 256 KiB`,
`max_compressed_bytes = 64 KiB`, `max_record_bytes = 64 KiB`,
`max_crypto_validations = 1 024`, `max_provenance_age_ms = 24 h`,
`lookup_reply_peer_count = 3`, `max_reply_bytes = MAX_I2NP_PAYLOAD_SIZE`,
`own_router_hash = None`. Internal trackers: `MAX_TRACKED_SOURCES = 1024`,
`MAX_TRACKED_KEYS = 2048`.

### Floodfill role and the advertisement gap

`FloodfillRoleController` is a **vocabulary and state machine**, not a
daemon service. `FloodfillEligibilitySnapshot::eligible()` is the
conjunction of nine daemon-owned signals (controlled qualification
permit, qualified SSU2 address, direct reachability, NetDB ready, storage
ready, maintenance ready, resource headroom, sane clock, healthy
supervision). `FloodfillRoleState` is `Disabled`, `Eligible`,
`Activating`, `Active`, `Draining`, `Failed`; losing eligibility from a
live state yields `FloodfillRoleEffect::WithdrawAdvertisementAndStopAdmission`,
and `accepts_new_work()` is true only in `Active`. The advertisement
token itself is a zero-sized `FloodfillAdvertisementPermit` that only
`complete_activation()` can mint.

**No floodfill advertisement is claimed by this crate.** It owns the
permit type and the eligibility vocabulary so the daemon has one
unambiguous authority surface; daemon role lifecycle, qualification
composition, and any advertisement emission live in `i2pr-daemon`. Per
`specs/support.toml`, daemon role lifecycle, qualification, and all
floodfill advertisement remain unimplemented/unclaimed.

### Encrypted LeaseSet2 (type 5) and the floodfill floor

`els2.rs` (Plan 332) and `els2_auth.rs` (Plan 333) implement type 5 as
real code, not a stub. What the source actually does:

- **Key binding.** A type-5 record lives at
  `SHA-256(sigtype || blindedPublicKey)`. `BlindedStorageKey` is that
  DHT storage key. `ValidatedEncryptedLeaseSet2::validate` re-derives the
  storage key from the record's own blinded public key and, when the
  caller supplies `expected_key`, requires a match
  (`StorageKeyMismatch`) — this is what stops a record minted under one
  day's blinded key from being accepted at a lookup for another.
- **Validation order** (`els2.rs:1427-1429`): length → blinded sigtype
  (`UnsupportedBlindedSigtype`) → blinded public-key decode
  (`InvalidBlindedPublicKey`) → storage key → offline delegation
  (`OfflineSignatureExpired`, `InvalidOfflineSignature`) → record
  signature (`InvalidSignature`, and the unblinded sigtype is checked
  first, `UnsupportedTransientSigtype`) → freshness
  (`ExcessiveFuture`, `Expired`) → wrap.
- **Two layers.** The `credential` and `subcredential` derivations
  (`ELS2_CREDENTIAL_PERSONALIZATION = b"credential"`,
  `ELS2_SUBCREDENTIAL_PERSONALIZATION = b"subcredential"`) bind the
  record to knowledge of the *unblinded* signing public key. Layer keys
  are HKDF-SHA256 over `ELS2_LAYER1_HKDF_INFO = b"ELS2_L1K"` and
  `ELS2_LAYER2_HKDF_INFO = b"ELS2_L2K"`, each
  `ELS2_LAYER_KEY_MATERIAL_LENGTH` bytes (32-byte ChaCha20 key + 12-byte
  nonce). The ChaCha20 stream starts at **block counter 1**, not 0.
  `encrypt_no_auth_outer_ciphertext` /
  `decrypt_no_auth_outer_ciphertext` are thin wrappers over the
  general `encrypt_outer_ciphertext` / `decrypt_outer_ciphertext` entry
  points that Plan 333 generalized to take an optional
  `Layer1Authorization`.
- **Layer-1 flags.** `ELS2_LAYER1_FLAG_PER_CLIENT = 0x01`,
  scheme bits `ELS2_LAYER1_SCHEME_MASK = 0x06` at
  `ELS2_LAYER1_SCHEME_SHIFT = 1`, with `ELS2_LAYER1_SCHEME_DH = 0x00` and
  `ELS2_LAYER1_SCHEME_PSK = 0x01`. Bits in
  `ELS2_LAYER1_RESERVED_MASK = 0xf0` are `Layer1ReservedFlags`; unknown
  bits are `UnrecognizedLayer1Flags`; a per-client flag with no
  authorization material is `InconsistentLayer1Authorization`.
- **Per-client authorization** (`els2_auth.rs`). One HKDF code path for
  both mechanisms: `HKDF(salt, authInput, label, ELS2_AUTH_OKM_LENGTH = 52)`
  sliced `0..32` (key) / `32..44` (IV) / `44..52`
  (`ELS2_AUTH_CLIENT_ID_LENGTH = 8`). PSK uses
  `ELS2_PSK_AUTH_HKDF_INFO = b"ELS2PSKA"`; DH uses
  `ELS2_DH_AUTH_HKDF_INFO = b"ELS2_XCA"` with
  `ELS2_DH_AUTH_KEY_TYPE_CODE = 4` and an ephemeral X25519 keypair per
  publication. The recovered `AuthCookie` is
  `ELS2_AUTH_COOKIE_LENGTH = 32` bytes and is prepended to the **layer-2**
  key input only, so a client that cannot recover it derives a wrong
  layer-2 key while layer-1 plaintext stays visible. `recover_auth_cookie`
  matches client identifiers in **constant time**
  (`i2pr_crypto::constant_time_eq`) and returns one
  `Els2AuthError::NotAuthorized` for both a wrong secret and an absent
  one, so the response is not an oracle. The bounded `AuthBlock` codec
  caps clients at `MAX_ELS2_AUTH_CLIENTS = 255`
  (`ELS2_AUTH_CLIENT_LENGTH = 40` per entry) and
  `MAX_ELS2_CLIENT_NAME_LENGTH = 64`. Secrets are
  `Zeroizing<[u8; 32]>` with no `Debug`/`Display`, no serde, and
  `Els2ClientAuthSecret` enforces the four `Els2AuthSecretRole`s so a PSK
  can never be used where a DH private key belongs
  (`WrongSecretRole`).
- **Blinding lifecycle.** `BlindingSchedule` owns the per-UTC-day
  material, refuses to mix an old `alpha` with a new signing key /
  storage key / address, supports bounded precomputation, and makes
  rollover atomic and restart-safe. `utc_blinding_day`,
  `day_bound_expiry_offset`, and `next_utc_day_boundary_seconds` are the
  clock helpers; the record's expiry is bound to the same UTC day.
- **Bounds.** `MAX_ELS2_INNER_LEASE_SET_LENGTH = 8 KiB`;
  `MAX_ELS2_OUTER_CIPHERTEXT_LENGTH = MAX_ELS2_INNER_LEASE_SET_LENGTH +
  2 * ELS2_SALT_LENGTH + 2` (8 258) — derived from the framing rather
  than asserted, after Plan 332 measured the real 66-byte ciphertext
  overhead in place of the 130 it first recorded; and
  `MAX_ELS2_RECORD_LENGTH = MAX_ELS2_OUTER_CIPHERTEXT_LENGTH + 128`
  (8 386), the default `Els2ValidationPolicy::max_encoded_len`.
  `Els2Store` is keyed by `BlindedStorageKey` with the same insertion
  vocabulary as the other stores.
- **Opaque serving.** `server_store::encrypted_lease_set2_for_answer` and
  `database_store_for_answer` return a type-5 record **opaquely**. A
  floodfill holds no unblinded public key, so it cannot decrypt what it
  stores, and the code does not try.

What is **not** claimed, stated explicitly:

- **Nothing is advertised.** `specs/support.toml` carries
  `common.leaseset2-family` with `advertised = false` and the note "No
  floodfill serving is claimed"; the type-5 surface is `status =
  "experimental"`.
- **No live interoperability yet, but the transcript is no longer i2pr-only.** Per
  ADR 0032 and Plan 346, the ELS2 network use of type 11 is a bounded
  compatibility profile: `els2_transcript` owns it, outbound records carry the
  deployed Java/i2pd transcript, and inbound records accept both that and the
  Proposal-146 strict transcript, reporting a typed four-state result and
  failing closed on an ambiguous match. Plan 347 owns the still-unexecuted live
  evidence. See
  [`332-status.md`](../../plans/closure/i2pcontrol-proposal-170/332-status.md)
  and
  [`333-status.md`](../../plans/closure/i2pcontrol-proposal-170/333-status.md):
  both "promote **no** capability advertisement and no live
  interoperability".
- **Plan 280 stopped, Plan 281 deferred — then the provider gap was
  closed in-tree.** `plans/closure/floodfill/280-status.md` records
  `stopped-no-acceptable-maintained-i2p-red25519-provider` after
  rejecting `reddsa`, `zakura-reddsa`, crates.io search, and the Go
  provider as out of boundary. `plans/closure/floodfill/281-status.md`
  narrowed the floor to types 0/1/3/7 and deferred type 5. Plans 330 and
  331 then closed that gap with an in-repo Red25519 implementation, and
  Plans 332/333 built on it. So the Plan 281 deferral was **superseded
  for the cryptographic provider**, while its downstream consequences
  still stand: `specs/support.toml`'s `m12_type11_red25519` row still
  reads "type5-deferred; no acceptable reviewed Rust provider; plan280
  stopped", and the same file's `m12_transit_tunnels` and
  `m12_floodfill_status` rows still describe type 5 as deferred. Those
  rows are **stale projections**; the `common.leaseset2-family` surface
  entry and the Plan 330–333 closure records are the later authority.

> **Boundary-checker note (resolved by Plan 364).**
> `scripts/check-m12-floodfill-boundaries.sh` used to enforce the Plan 281
> floor with `rg 'DatabaseStoreData::EncryptedLeaseSet|ValidatedEncryptedLeaseSet|ServerEncryptedLeaseSet'
> crates/i2pr-netdb/src`. Plans 332/333 added `els2.rs` / `els2_auth.rs`
> and the type-5 server representation to this crate, so that rule went stale
> and the script **exited 1** on the repository head. It was in neither the
> `AGENTS.md` routine floor nor `.github/workflows/ci.yml`, so the failure
> was silent. **Plan 364** replaced the stale rule with 9 positive assertions
> traced to the Plans 332/333/334/346 closure records, each negative-tested;
> `--self-test` drives 11 deliberate breaks and every one is caught on its own
> gate. The script now exits 0 and runs in **both** the routine floor and
> `ci.yml`. No source in this crate was changed to make it pass — the fix was
> to the rule, not the type-5 floor it policed.

### Every `MAX_*` / bound constant

| Constant | Value | Site |
| --- | ---: | --- |
| `MAX_DECODED_LEN` (I2P Base64) | 512 | `base64.rs:31` |
| `DEFAULT_MAX_AGE` | 24 h | `router_info.rs:20` |
| `DEFAULT_MAX_FUTURE_SKEW` | 1 h | `router_info.rs:24` |
| `DEFAULT_MAX_ENCODED_LEN` | 16 KiB | `router_info.rs:30` |
| `MAX_COMPRESSED_ROUTER_INFO_BYTES` | 16 KiB | `lookup_action.rs:24` |
| `MAX_DECOMPRESSED_ROUTER_INFO_BYTES` | 32 KiB | `lookup_action.rs:28` |
| `LOOKUP_EXCLUDED_PEER_BUDGET` | 256 | `lookup_action.rs:304` |
| `MAX_WAITERS_PER_LOOKUP` | 32 | `lookup_id.rs:75` |
| `MAX_COALESCED_LOOKUPS` | 8 | `lookup_id.rs:80` |
| `MAX_SUGGESTED_HASH_LIMIT` | 256 | `lookup_policy.rs:18` |
| `DEFAULT_SUGGESTED_HASH_LIMIT` | 64 | `lookup_policy.rs:22` |
| `DEFAULT_MAX_PEERS_PER_LOOKUP` | 8 | `lookup_policy.rs:24` |
| `DEFAULT_TOTAL_DEADLINE_MS` | 5 000 | `lookup_policy.rs:26` |
| `DEFAULT_PER_ATTEMPT_DEADLINE_MS` | 1 500 | `lookup_policy.rs:28` |
| `DEFAULT_MAX_SUGGESTED_HASHES` | 128 | `lookup_policy.rs:31` |
| `DEFAULT_MAX_CANDIDATES_CONSIDERED` | 32 | `lookup_policy.rs:35` |
| `MAX_PUBLICATION_ATTEMPTS` | 32 | `publication.rs:24` |
| `MAX_SU3_BYTES` | 8 MiB | `reseed.rs:33` |
| `MAX_ARCHIVE_ENTRIES` | 4 096 | `reseed.rs:37` |
| `MAX_ARCHIVE_ROUTERINFO_BYTES` | 32 MiB | `reseed.rs:41` |
| `MAX_ENTRY_UNCOMPRESSED_BYTES` | 256 KiB | `reseed.rs:45` |
| `MAX_ARCHIVE_UNCOMPRESSED_BYTES` | 64 MiB | `reseed.rs:49` |
| `MAX_SIGNER_ID_LEN` | 256 | `reseed.rs:52` |
| `MAX_VERSION_LEN` | 16 | `reseed.rs:55` |
| `RESEED_FILE_TYPE` / `RESEED_CONTENT_TYPE` | 2 / 0 | `reseed.rs:58,61` |
| `ELS2_SALT_LENGTH` | 32 | `els2.rs:60` |
| `ELS2_LAYER_KEY_MATERIAL_LENGTH` | 44 | `els2.rs:71` |
| `ELS2_LAYER1_FLAG_PER_CLIENT` | `0x01` | `els2.rs:73` |
| `ELS2_LAYER1_SCHEME_MASK` | `0x06` | `els2.rs:75` |
| `ELS2_LAYER1_SCHEME_SHIFT` | 1 | `els2.rs:77` |
| `ELS2_LAYER1_RESERVED_MASK` | `0xf0` | `els2.rs:79` |
| `ELS2_LAYER1_SCHEME_DH` / `_PSK` | `0x00` / `0x01` | `els2.rs:81,83` |
| `MAX_ELS2_INNER_LEASE_SET_LENGTH` | 8 KiB | `els2.rs:92` |
| `MAX_ELS2_OUTER_CIPHERTEXT_LENGTH` | inner + 2·salt + 2 = 8 258 | `els2.rs:106` |
| `MAX_ELS2_RECORD_LENGTH` | outer + 128 = 8 386 | `els2.rs:109` |
| `MAX_ELS2_AUTH_CLIENTS` | 255 | `els2.rs:112` |
| `ELS2_AUTH_CLIENT_LENGTH` | 40 | `els2.rs:114` |
| `ELS2_AUTH_OKM_LENGTH` | 52 | `els2_auth.rs:54` |
| `ELS2_AUTH_KEY_LENGTH` | 32 (module-private) | `els2_auth.rs:56` |
| `ELS2_AUTH_IV_LENGTH` | 12 (module-private) | `els2_auth.rs:58` |
| `ELS2_AUTH_CLIENT_ID_LENGTH` | 8 | `els2_auth.rs:60` |
| `ELS2_AUTH_COOKIE_LENGTH` | 32 | `els2_auth.rs:62` |
| `ELS2_DH_AUTH_HKDF_INFO` | `b"ELS2_XCA"` | `els2_auth.rs:64` |
| `ELS2_PSK_AUTH_HKDF_INFO` | `b"ELS2PSKA"` | `els2_auth.rs:66` |
| `ELS2_DH_AUTH_KEY_TYPE_CODE` | 4 | `els2_auth.rs:74` |
| `MAX_ELS2_CLIENT_NAME_LENGTH` | 64 | `els2_auth.rs:902` |
| `MAX_TRACKED_SOURCES` | 1 024 (module-private) | `floodfill_service.rs:24` |
| `MAX_TRACKED_KEYS` | 2 048 (module-private) | `floodfill_service.rs:25` |
| `DAY_MS` / `RI_ROLLOVER_MS` / `LS_ROLLOVER_MS` / `NEXT_KEY_TARGETS` | 86 400 000 / 45 min / 10 min / 2 | `replication.rs:12-15` |

## Dependencies

`crates/i2pr-netdb/Cargo.toml`:

**Production** — `base64ct`, `flate2`, `i2pr-crypto`, `i2pr-proto`,
`i2pr-su3`, `sha2`, `thiserror`, `zip`.

| Dependency | Why |
| --- | --- |
| `i2pr-proto` | Wire codecs and `Date`/`Hash`/`RouterInfo`/`LeaseSet2`/`EncryptedLeaseSet2` structures |
| `i2pr-crypto` | Signature verification, `router_identity_hash`, Red25519, HKDF, ChaCha20 layers, `constant_time_eq`, `Zeroizing` |
| `i2pr-su3` | SU3 container framing |
| `zip` | **Makes reseed ZIP parsing possible at all.** Resigning or vendoring a ZIP reader would mean patching an external archive format; `zip` is the bounded reader the reseed pipeline uses, with entry-count, per-entry, and cumulative-byte limits enforced by this crate on top of it. |
| `flate2` | Bounded gzip **de**compression of `DatabaseStore` `RouterInfo` payloads in `lookup_action::decompress_router_info` and the `store_message` path. Decompression ceilings are enforced before allocation. |
| `sha2` | The daily routing-key derivation `SHA256(search_key || yyyyMMdd)` in `routing` |
| `base64ct` | Constant-time base64 primitives backing the strict I2P Base64 codec |
| `thiserror` | Typed error enums — no `anyhow` in this crate |

**Dev** — `serde_json`, `rand_chacha` (0.10), `rand_chacha` 0.3,
`rand_core` (0.10), `rand_core` 0.6, `sad-rsa` (with `encoding`),
`sha2`, `zip`.

Checker allowlist (`scripts/check-dependency-direction.sh:30`):

```python
"i2pr-netdb": {"i2pr-crypto", "i2pr-proto", "i2pr-su3"},
```

The allowlist covers the three `i2pr-*` path dependencies exactly; the
remaining production dependencies are third-party crates the checker
governs separately. `i2pr-netdb-persist` is the only workspace crate
that may depend on `i2pr-netdb` for persistence composition, and
`i2pr-client`, `i2pr-daemon`, and `i2pr-i2cp`-adjacent composition read
the validated surface. There is no production dependency on
`i2pr-testkit`.

**Runtime-neutrality is verified, not assumed.** Greps over
`crates/i2pr-netdb/src/` find no `tokio`, no `tokio::*`, no `std::net`,
no `TcpStream`/`UdpSocket`/`SocketAddr`, no `std::fs`, no `async fn`, no
`.await`, no `spawn`, no `JoinHandle`, no `thread::`, and no unbounded
channel type. The only `write_all` hits are `std::io::Write` on in-memory
cursors and ZIP writers. `bash scripts/check-runtime-boundaries.sh`
passes on the current head. The one interior-mutability construct is
`Arc<Mutex<FloodfillResourceSnapshot>>` in `resource.rs`, which is
synchronous, bounded, and drop-released.

## Tests

243 tests, all local, all deterministic; no root, namespaces, Java I2P,
i2pd, or Internet connection. `cargo test --locked -p i2pr-netdb
--all-targets -- --test-threads=1` passes: **171 in-crate unit tests,
72 integration tests, 0 failed, 0 ignored.**

Integration files under `crates/i2pr-netdb/tests/` (4 files, 3 064 lines):

| File | Lines | Tests | Coverage |
| --- | ---: | ---: | --- |
| `els2_foundation.rs` | 1 418 | 25 | Type-5 foundation: distinct blinded/storage keys per UTC day, day-bound expiry, opaque floodfill store-and-serve, freshness and storage-key enforcement, an **independent Python derivation** agreeing on credentials and layer ciphertexts, counter-1 stream start, lookup-secret bound, non-Red25519 blinded sigtype rejection, offline key-block parsing and delegation verification, owner-schedule self-consistency, per-client layer-1 flags refused by the no-auth floor, bounded precomputation, reserved layer-1 flag rejection, atomic counted restart-safe rollover, size ceilings on both layers, store capacity without mutating existing state, newer-replaces / stale-rejects semantics, tampered ciphertext rejected by the record signature, tampered signature and header fields rejected, type-7 owner blinding, unsupported unblinded sigtype rejected before derivation, wrong published timestamp and wrong secret yielding no decryption |
| `els2_client_authorization.rs` | 1 061 | 30 | PSK and DH client authorization: both HKDF paths, bounded `AuthBlock` codec round-trips, client-count ceilings, wrong-role secret refusal, `AuthBlock` flag validation, `AuthClientMaterial` recovery, and the negative rows a tampered cookie yielding a *different* cookie rather than an error and a wrong PSK refused with the **same** error as an absent one (no oracle) |
| `els2_auth_emissary_differential.rs` | 384 | 9 | Differential against a pinned reference transcript; fixtures `tests/data/els2-auth-emissary-differential.json` and `tests/data/els2-independent-derivation.json` |
| `lease_set2_integration.rs` | 201 | 8 | `DestinationHash` indexing, the Plan 119 signature path, typed rejection of an invalid signature, a wrong expected key, and an expired LS2, deterministic replacement idempotency, swap on different bytes, capacity isolation from `RouterInfoStore`, `LeaseSet2StoreStats` contract |

In-crate `#[cfg(test)]` modules (171 tests) are distributed as:
`lookup_policy` 14, `floodfill_service` 14, `routing` 13,
`lookup_engine` 13, `local` 13, `lease_set2` 13, `store` 12,
`router_info` 11, `publication` 11, `lookup_action` 9, `reseed` 8,
`lookup_id` 8, `base64` 8, `store_message` 5, `databaselookup` 4,
`replication` 3, `provenance` 3, `lease_set` 3, `els2` 3, and 1 each in
`server_store`, `resource`, and `floodfill_role`. `els2_auth.rs` has no
in-crate test module; its coverage is entirely in
`tests/els2_client_authorization.rs` and
`tests/els2_auth_emissary_differential.rs`.

**Malformed / negative corpus coverage.** Every bounded surface has a
negative row rather than a happy-path-only row: RouterInfo rejects
expected-key mismatch *before* signature work, mutated signature bytes,
mutated signed bytes, stale and future-skewed publication; I2P Base64
rejects `+`, `/`, `?`, unknown bytes, wrong length, and over-padding, and
pins that `~` is value 63 rather than padding; the decompressor is tested
at, below, and above both ceilings; stores cover capacity, exact load,
over-capacity, idempotent, conflict, and stale paths; the SU3 suite
drives the full header + signature + ZIP + RouterInfo path with a
deterministic RSA test signer; `els2_foundation.rs` covers tampered
ciphertext, tampered signature, tampered header fields, and both
wrong-secret and wrong-timestamp decryption failures.

**Determinism.** All identities, RSA keys, and blindings are derived
from fixed seeds (`rand_chacha` `seed_from_u64`) or supplied `Date`
values; no test reads a wall clock or sleeps. Time is always injected
through `ValidationContext` / `*ValidationContext` / `FloodfillTime` /
`now_seconds`, which is why the crate needs `start_paused`-style
control to be exercised by its callers rather than internally.

## Distinctive design choices

1. **Validation is a type boundary, not a function result.** Every
   validated record (`ValidatedRouterInfo`, `ValidatedLeaseSet`,
   `ValidatedLeaseSet2`, `ValidatedMetaLeaseSet`,
   `ValidatedEncryptedLeaseSet2`) has private fields and exactly one
   constructor, so an unchecked record cannot enter a store.
2. **Every store fails closed and never evicts.** Capacity exceeded is a
   typed rejection with no state mutation; reclamation is an explicit
   caller `remove` / `prune`.
3. **Determinism is engineered, not hoped for.** XOR-distance selection
   breaks ties on `RouterHash`, daily routing keys are caller-`Date`-driven
   with no clock read, and all hashing is SHA-256 over canonical bytes.
4. **A valid signature is not a trust decision.** RouterInfo, LeaseSet2,
   and type-5 records are verified against key material the record itself
   carries; that proves internal consistency, and trust comes from
   `reseed` (signer certificates) or from transport plus provenance.
5. **Provenance is orthogonal to validation.** A validated record is not
   automatically answerable, replicable, or persistable; zero-token flood
   replicas are stored but never re-flooded.
6. **No-auth is the default, authorization is an explicit layer.** The
   ELS2 cookie gates **layer 2 only**, so a passive observer still sees
   the client-count in layer-1 plaintext — the privacy property the
   specification asks for, and the reason the PSK and DH paths share one
   code path.
7. **Authorization failure is uniform.** A wrong secret and an absent
   secret return the same typed error, and client identifiers are matched
   in constant time, so the control surface is not an oracle.
8. **Secrets are `Zeroizing` and role-typed.** PSK, cookie, and X25519
   private material have no `Debug`/`Display`/serde, and
   `Els2ClientAuthSecret` refuses to reinterpret a secret across
   `Els2AuthSecretRole`s.
9. **Time is always injected.** The crate never calls
   `SystemTime::now()`; every freshness decision takes a caller-supplied
   `Date` or `now_seconds`.
10. **Redaction is structural.** `RouterHash` and `DestinationHash`
    render as `X(..)` in `Debug`, and `RecordId`'s key renders as
    `"[redacted]"`, so a log line cannot leak identity.

## LeaseSet2-kind lookups against a non-destination key (Plan 351, ADR 0033)

`LookupKind::LeaseSet2` codes to `1`, and that is also what a reference client issues when
resolving an encrypted service — the record is filed under its **blinded storage key** rather than
a destination hash. Plan 351 relies on that being correct rather than inventing a lookup type: no
new wire type is introduced, so nothing a peer can observe changes.

The consequence inside this crate is that a `LeaseSet2`-kind lookup's `RouterHash` is no longer
always derivable from a `DestinationHash`. `handle_database_store` gained one arm:

- the key match at the top of the function is unchanged and is the only check that applies;
- a `DatabaseStoreData::EncryptedLeaseSet` record then produces
  `LookupResult::EncryptedLeaseSet2Success { lookup_id, message }` carrying the **raw** message.

That arm deliberately does **not** validate, decrypt, or install:

- the closed type-11 signature profile is ADR 0032's policy and stays with the ELS2 owner;
- unwrapping needs the daily blinding material and, for an authorized service, per-client key
  material that this crate has no business holding;
- the identity binding — that the unwrapped record signs with the unblinded public key the `.b33`
  names — cannot be checked here at all, because this crate never sees the address.

A caller that matches only `LeaseSet2Success` therefore treats a type-5 reply as a non-match and
keeps waiting, which is the correct fail-closed outcome for an ordinary consumer. No
`store.insert` happens on the encrypted arm: the `LeaseSet2Store` is keyed by destination hash and a
blinded storage key is not one. The unwrapped record is installed by the owner under the
unblinded destination hash the record itself carries.

`scripts/check-encrypted-service-consumer-caller.sh` fails if this arm ever starts validating,
unwrapping, or inserting.

## Cross-references

**ADRs**

- [ADR 0002 — Tokio runtime boundary](../../docs/adr/0002-tokio-runtime-boundary.md)
  — why this crate has no Tokio.
- [ADR 0003 — Bounded supervised services](../../docs/adr/0003-bounded-supervised-services.md)
  — the bound-everything posture.
- [ADR 0004 — Router identity algorithms](../../docs/adr/0004-router-identity-algorithms.md).
- [ADR 0005 — Crypto dependency selection](../../docs/adr/0005-crypto-dependency-selection.md)
  — external crypto only; amended by ADR 0032 for the ELS2 use of type 11.
- [ADR 0032 — The encrypted LeaseSet2 type-11 signature-profile
  boundary](../../docs/adr/0032-els2-type11-signature-profile-boundary.md) —
  the strict primitive stays byte-exact; the deployed Java/i2pd transcript is a
  separate, bounded, ELS2-owned profile with no generic dual-transcript verifier.
- [ADR 0010 — Transport contracts and crate boundaries](../../docs/adr/0010-transport-contracts-and-crate-boundaries.md).
- [ADR 0027 — Floodfill role, provenance, and advertisement](../../docs/adr/0027-floodfill-role-provenance-and-advertisement.md)
  — the type-5 deferral floor as originally written (later superseded
  for the provider; see above).
- [ADR 0028 — I2PControl Proposal 170 control plane](../../docs/adr/0028-i2pcontrol-proposal-170-control-plane.md)
  — the control surface that maps onto `Els2AuthorizationServerConfig`.

**NetDB subsystem closure records (Plans 102–106)**

- [`plans/closure/netdb/103-status.md`](../../plans/closure/netdb/103-status.md) —
  the validation floor and store semantics.
- [`plans/closure/netdb/104-status.md`](../../plans/closure/netdb/104-status.md) —
  SU3/reseed.
- [`plans/closure/netdb/105-status.md`](../../plans/closure/netdb/105-status.md) —
  routing keys, lookup identity, action vocabulary, decompressor bounds.
- [`plans/closure/netdb/106-status.md`](../../plans/closure/netdb/106-status.md) —
  the bootstrap handoff.

**M12 floodfill closure records**

- [270](../../plans/closure/floodfill/270-status.md) — architecture authority.
- [271](../../plans/closure/floodfill/271-status.md) — provenance segmentation.
- [272](../../plans/closure/floodfill/272-status.md) — record validation and storage (types 1/3/7).
- [273](../../plans/closure/floodfill/273-status.md) — DatabaseStore service.
- [274](../../plans/closure/floodfill/274-status.md) — DatabaseLookup service and reply protection.
- [275](../../plans/closure/floodfill/275-status.md) — direct replication and routing-key rollover.
- [276](../../plans/closure/floodfill/276-status.md) — persistence, maintenance, and resource governance.
- [277](../../plans/closure/floodfill/277-status.md) — daemon role lifecycle; **stopped with retained work**.
- [280](../../plans/closure/floodfill/280-status.md) — Red25519 provider qualification; **stopped, no acceptable provider**.
- [281](../../plans/closure/floodfill/281-status.md) — passed; deferred type 5 (provider gap later closed in-tree by Plans 330/331).
- [282](../../plans/closure/floodfill/282-status.md) — **stopped with retained work**; Store ack body-id and `DatabaseLookup.from` corrections.
- [283](../../plans/closure/floodfill/283-status.md) — passed; controlled activation and withdrawal composition.
- [332](../../plans/closure/i2pcontrol-proposal-170/332-status.md) and
  [333](../../plans/closure/i2pcontrol-proposal-170/333-status.md) —
  passed; type-5 foundation and per-client authorization, no advertisement
  and no interoperability.

**Other closure records cited for specific contracts**

- [`119-status.md`](../../plans/closure/destination-streaming/119-status.md) —
  the Standard LeaseSet2 carrier.
- [`117-status.md`](../../plans/closure/exploratory-tunnels/117-status.md) —
  the `PublicationAttemptRecord` and lookup-reply-path handoff.
- [`142-status.md`](../../plans/closure/sam/142-status.md) — the frozen I2P
  Base64 alphabet.

**Specs**

- [`specs/support.toml`](../../specs/support.toml) — machine-readable
  support inventory; `common.leaseset2-family` is `advertised = false`.
- [`specs/CONFORMANCE.md`](../../specs/CONFORMANCE.md) — what counts as
  evidence.

**Related deep dives**

- [i2pr-proto.md](i2pr-proto.md) — the wire codecs and structures this
  crate validates.
- [i2pr-crypto.md](i2pr-crypto.md) — signature verification, Red25519,
  HKDF, ChaCha20.
- [i2pr-su3.md](i2pr-su3.md) — SU3 framing and `sad-rsa`.
- [i2pr-netdb-persist.md](i2pr-netdb-persist.md) — the filesystem layer
  that composes this crate.
- [i2pr-tunnel.md](i2pr-tunnel.md) — the Plan 107
  `ExploratoryPoolReplyPathProvider` that feeds `ReplyPath`.
- [i2pr-client.md](i2pr-client.md) — destination lifecycle and ECIES
  routing.
- [i2pr-daemon.md](i2pr-daemon.md) — the composition root and the
  floodfill role owner.
- [i2pr-runtime.md](i2pr-runtime.md) — the sole owner of Tokio, sockets,
  and timers.
- [i2pr-storage.md](i2pr-storage.md) — identity and the raw-byte cache seam.
- [dependency-graph.md](dependency-graph.md) and
  [overview.md](overview.md) — crate index and allowlist.
