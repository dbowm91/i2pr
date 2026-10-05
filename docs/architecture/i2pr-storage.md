# `i2pr-storage` — Deep Dive

Versioned, atomic, permission-hardened synchronous filesystem persistence for
router-owned local state: the router identity, the NTCP2 transport static key,
persistent service destinations, and three byte-level opaque/adapter stores.

Path: `crates/i2pr-storage/`

## Purpose

`i2pr-storage` owns on-disk bytes and nothing else. It provides six store
surfaces, in two categories:

**Versioned secret records** (magic + version + length + SHA-256, decoded
against compile-time constants):

1. **Router identity** — Ed25519 signing seed + X25519 encryption seed, the
   derived public keys, and the exact identity padding.
   `<data_dir>/router.identity`, format version 2.
2. **NTCP2 transport static key** — an X25519 keypair plus the published
   obfuscation IV, in a record **distinct** from the router identity.
   `<data_dir>/ntcp2.static.key`, format version 1.
3. **Service destination identity** — one stable router-owned destination per
   enabled server-tunnel service.
   `<data_dir>/service_destinations/<id>/destination.identity`,
   format version 2 (v1 still decodes; see [Key contracts](#key-contracts)).

**Opaque byte-level stores** (no magic, no version, no checksum — the store
never parses or trusts the bytes):

4. **`cache_seam`** — permission-hardened raw-byte cache used by the Plan 104
   NetDB composition owner (`i2pr-netdb-persist`).
5. **`address_book_generation`** — Plan 294 opaque current/backup generation
   pair; `i2pr-addressbook` owns serialization and validation.
6. **`verified_content_cache`** — Plan 322 opaque current/backup record store
   for SU3-verified NEWS content; the daemon owns verification and decoding.

What the crate must **not** own:

- No network I/O, no sockets, no `tokio`, no `async fn`, no `.await` (verified:
  zero matches in `src/`).
- No decoding of RouterInfo, ZIP, SU3, address-book JSON, or NEWS XML. The
  store surfaces bytes and the composition owner re-validates before use.
- No decision about **where** the data directory lives. Real paths, CLI
  arguments, and process configuration are owned by `i2pr-daemon`; this crate
  is handed a `&Path`.
- No encryption at rest. Filesystem permissions plus operator backup handling
  are the Milestone 1 threat-model boundary (ADR 0006).

`std::fs` use here is legitimate and expected: the runtime-neutral `std::fs`
prohibition applies to the transport, API, and service crates, not to the
storage crate. `#![forbid(unsafe_code)]` is declared in `lib.rs` and in each
submodule.

## Module layout

Line counts recomputed with `wc -l`.

| Module | File | Lines | Responsibility | Key public types |
| --- | --- | ---: | --- | --- |
| crate root | `src/lib.rs` | 1481 | Identity + NTCP2 static-key records; shared private path/permission helpers (`create_temporary_file`, `ensure_secure_directory`, `validate_existing_directory`, `sync_directory`) reused by all sibling modules | `StorageError`, `IdentityStore`, `TransportStaticKeyMaterial`, `TransportStaticKeyStore`, `decode_transport_static_key` |
| `cache_seam` | `src/lib.rs` (inline, 425–802) | — | Plan 104 raw-byte NetDB cache seam; layout, filename hygiene, scan budgets, atomic write/replace | `ByteCache`, `CacheError` |
| `service_destination` | `src/service_destination.rs` | 1232 | Plan 175 per-service destination identity records; v1/v2 versioned formats with explicit migration | `ServiceDestinationStore`, `ServiceDestinationRecord`, `ServiceDestinationStorageError`, `decode_service_destination_bytes` |
| `address_book_generation` | `src/address_book_generation.rs` | 305 | Plan 294 opaque address-book generation current/backup pair | `AddressBookGenerationStore`, `AddressBookGenerationStorageError` |
| `verified_content_cache` | `src/verified_content_cache.rs` | 201 | Plan 322 opaque SU3-verified NEWS content current/backup pair | `VerifiedContentCacheStore`, `VerifiedContentCacheError` |

`cache_seam` is an inline `pub mod` inside `lib.rs`, not a separate file. The
other three modules are separate files and reuse the same private
directory/permission/temporary-file helpers from the crate root, so the
permission and symlink policy is defined once.

## Public surface

The crate root re-exports (`lib.rs:32–48`):

```rust
pub mod address_book_generation;
pub use address_book_generation::{
    ADDRESSBOOK_BACKUP_FILE_NAME, ADDRESSBOOK_CURRENT_FILE_NAME, ADDRESSBOOK_STATE_SUBDIR,
    AddressBookGenerationStorageError, AddressBookGenerationStore,
    MAX_ADDRESSBOOK_GENERATION_FILE_SIZE,
};
pub mod verified_content_cache;
pub use verified_content_cache::{
    MAX_VERIFIED_CONTENT_CACHE_BYTES, VERIFIED_CONTENT_CACHE_BACKUP, VERIFIED_CONTENT_CACHE_DIR,
    VERIFIED_CONTENT_CACHE_FILE, VerifiedContentCacheError, VerifiedContentCacheStore,
};
pub mod service_destination;
pub use service_destination::{
    MAX_SERVICE_DESTINATION_FILE_SIZE, SERVICE_DESTINATION_FILE_NAME,
    SERVICE_DESTINATION_FORMAT_VERSION, SERVICE_DESTINATIONS_SUBDIR, ServiceDestinationRecord,
    ServiceDestinationStorageError, ServiceDestinationStore, decode_service_destination_bytes,
};
```

Note the asymmetry: the re-export list is a curated subset. Several
`pub const`s in `service_destination` are reachable only through the module
path (`i2pr_storage::service_destination::SERVICE_DESTINATION_FORMAT_VERSION_V1`,
`…_V2`, `…_LEGACY_FILLER_LENGTH`, `…_V2_PADDING_LENGTH`).

### Constants declared at the crate root

| Item | Source line | Value |
| --- | ---: | --- |
| `IDENTITY_FILE_NAME` | 51 | `"router.identity"` |
| `MAX_IDENTITY_FILE_SIZE` | 53 | `4096` |
| `IDENTITY_FORMAT_VERSION` | 55 | `2` (current; not `1`) |
| `NTCP2_TRANSPORT_KEY_FILE_NAME` | 58 | `"ntcp2.static.key"` |
| `MAX_NTCP2_TRANSPORT_KEY_FILE_SIZE` | 60 | `4096` |
| `NTCP2_TRANSPORT_KEY_FORMAT_VERSION` | 62 | `1` |

### Types and functions

| Item | Source line | Notes |
| --- | ---: | --- |
| `enum StorageError` | 89 | 12 variants; see below |
| `struct IdentityStore` | 152 | `new`, `in_data_dir`, `path`, `prepare_directory`, `save_new`, `save`, `load`. Derives `Clone, Debug, Eq, PartialEq` |
| `struct TransportStaticKeyMaterial` | 256 | `generate`, `from_parts`, `key`, `iv`, `into_parts`. **Not `Clone`**, no `Debug` |
| `struct TransportStaticKeyStore` | 298 | `new`, `in_data_dir`, `path`, `generate_new`, `save_new`, `load`. Derives `Clone, Debug, Eq, PartialEq` |
| `fn decode_transport_static_key` | 397 | bounded pure decoder; the fuzz-harness entry point |

`StorageError` variants: `Io { operation, source }`, `UnsafePath`,
`AlreadyExists`, `InsecurePermissions`, `TooLarge { actual, maximum }`,
`Truncated`, `TrailingBytes`, `Malformed { context }`,
`UnsupportedVersion { actual }`, `UnsupportedAlgorithm { algorithm, context }`,
`Integrity`, `Crypto` (transparent from `CryptoError`). There is **no**
`StorageError::Cache` variant; `cache_seam::CacheError` converts in through
`From`.

### `cache_seam` (`lib.rs:425–802`)

| Item | Source line | Value / shape |
| --- | ---: | --- |
| `ROUTERS_SUBDIR` | 433 | `"netdb/routers"` |
| `PENDING_SUBDIR` | 436 | `"netdb/routers/.pending"` |
| `MAX_CACHE_FILE_BYTES` | 443 | `64 * 1024` (64 KiB) |
| `MAX_CACHE_SCAN_BYTES` | 448 | `32 * 1024 * 1024` (32 MiB, `u64`) |
| `MAX_CACHE_SCAN_ENTRIES` | 453 | `16 * 1024` (16384) |
| `enum CacheError` | 457 | `Io`, `UnsafePath`, `InvalidFilename { name }`, `FileTooLarge { path, maximum }`, `ScanBudgetExceeded { maximum }`, `ScanEntriesExceeded { maximum }`, `EmptyPayload`; `From<CacheError> for StorageError` at 505 |
| `struct ByteCache` | 524 | Derives `Clone, Debug, Eq, PartialEq`; `in_data_dir`, `root`, `pending_dir`, `exists`, `prepare`, `validate_name`, `path_for`, `write` (insert-only), `replace` (atomic replacing), `remove`, `read`, `scan` |

`validate_name` requires **exactly 64 lowercase hex characters**
(`lib.rs:564–581`) — not the percent-encoded or single-segment forms the
`cache_seam` module doc mentions. `write` is insert-only via `hard_link`;
`replace` is the only path that uses `fs::rename` over an existing entry.

### `address_book_generation` (`address_book_generation.rs`)

| Item | Source line | Value |
| --- | ---: | --- |
| `ADDRESSBOOK_STATE_SUBDIR` | 36 | `"addressbook"` |
| `ADDRESSBOOK_CURRENT_FILE_NAME` | 38 | `"addressbook.current.json"` |
| `ADDRESSBOOK_BACKUP_FILE_NAME` | 40 | `"addressbook.backup.json"` |
| `MAX_ADDRESSBOOK_GENERATION_FILE_SIZE` | 43 | `32_000_000` |
| `enum AddressBookGenerationStorageError` | 47 | `Io`, `UnsafePath`, `InsecurePermissions`, `TooLarge { actual, maximum }` |
| `struct AddressBookGenerationStore` | 79 | Derives `Clone, Debug` (no `Eq`/`PartialEq`); `new(dir)`, `prepare`, `publish`, `load_current`, `load_backup` |

There is **no `in_data_dir` constructor** — the caller composes
`data_dir.join(ADDRESSBOOK_STATE_SUBDIR)` and passes it to `new`. Filenames are
fixed module constants; no caller path input reaches the store.

### `verified_content_cache` (`verified_content_cache.rs`)

| Item | Source line | Value |
| --- | ---: | --- |
| `VERIFIED_CONTENT_CACHE_DIR` | 16 | `"news"` |
| `VERIFIED_CONTENT_CACHE_FILE` | 17 | `"verified-content.cache"` |
| `VERIFIED_CONTENT_CACHE_BACKUP` | 18 | `"verified-content.backup"` |
| `MAX_VERIFIED_CONTENT_CACHE_BYTES` | 19 | `8 * 1024 * 1024 + 16 * 1024` (8 MiB + 16 KiB) |
| `enum VerifiedContentCacheError` | 22 | `Io`, `UnsafePath`, `InsecurePermissions`, `TooLarge { actual, maximum }` |
| `struct VerifiedContentCacheStore` | 42 | Derives `Clone, Debug, Eq, PartialEq`; `in_data_dir`, `prepare`, `publish`, `load_current`, `load_backup` |

This store has **no magic, version, or checksum of its own** — by design. The
record is opaque authenticated-content bytes; SU3 verification, signature
checking, and record decoding stay with the daemon, which must re-verify a
loaded record before publishing it to a live consumer.

### `service_destination` (`service_destination.rs`)

| Item | Source line | Value |
| --- | ---: | --- |
| `SERVICE_DESTINATION_FILE_NAME` | 51 | `"destination.identity"` |
| `SERVICE_DESTINATIONS_SUBDIR` | 53 | `"service_destinations"` |
| `MAX_SERVICE_DESTINATION_FILE_SIZE` | 55 | `4096` |
| `SERVICE_DESTINATION_FORMAT_VERSION` | 64 | `2` |
| `SERVICE_DESTINATION_FORMAT_VERSION_V1` | 66 | `1` (not re-exported at crate root) |
| `SERVICE_DESTINATION_FORMAT_VERSION_V2` | 68 | `2` (not re-exported at crate root) |
| `SERVICE_DESTINATION_LEGACY_FILLER_LENGTH` | 70 | `256` |
| `SERVICE_DESTINATION_V2_PADDING_LENGTH` | 72 | `96` (`384 - 256 - 32`) |
| `enum ServiceDestinationStorageError` | 99 | Same 12 variants as `StorageError` plus `InvalidId { value, reason }` |
| `struct ServiceDestinationRecord` | 175 | `signing_seed`, `static_secret`, `padding`, `legacy_filler`, `is_v2`; hand-written `Debug` at 186 |
| `struct ServiceDestinationStore` | 230 | `for_service`, `for_group`, `for_key_reference`, `new`, `path`, `parent`, `exists`, `prepare_directory`, `save_new`, `load`, `generate_new`, `migrate_from`, `remove` |
| `fn decode_service_destination_bytes` | 888 | bounded pure decoder |

`SERVICE_DESTINATION_MAGIC` is `b"I2PRSD\0\0"` (line 74, private).

## Key contracts

### Versioned record format

All versioned records are manual big-endian (`push_u16` + `extend_from_slice`),
Rust-layout independent, serde-free, and terminated by a SHA-256 checksum over
`header ++ payload` (everything before the trailing 32 bytes), verified with
`constant_time_eq`. The exact byte count is enforced, so unknown trailing
fields are structurally impossible — extras yield `TrailingBytes`.

#### `router.identity` — 504 bytes, version 2

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 8 | Magic `b"I2PRID\0\0"` |
| 8 | 2 | Format version (`2`) |
| 10 | 2 | Reserved (`0`, `RESERVED_HEADER`) |
| 12 | 2 | Signing algorithm — must equal `ROUTER_SIGNING_KEY_TYPE.code()` |
| 14 | 2 | Encryption algorithm — must equal `ROUTER_CRYPTO_KEY_TYPE.code()` |
| 16 | 2 | Signing private length (`32`) |
| 18 | 2 | Encryption private length (`32`) |
| 20 | 2 | Signing public length (`32`) |
| 22 | 2 | Encryption public length (`32`) |
| 24 | 32 | Signing private key (Ed25519 seed) |
| 56 | 32 | Encryption private key (X25519 seed) |
| 88 | 32 | Signing public key (derived) |
| 120 | 32 | Encryption public key (derived) |
| 152 | 320 | Identity padding (`IDENTITY_PADDING_LENGTH`) |
| 472 | 32 | `SHA256(header ++ payload)` |

`IDENTITY_FILE_LENGTH` = 24 + 128 + 320 + 32 = **504**, where
`IDENTITY_PADDING_LENGTH = 384 - 2 * PRIVATE_KEY_LENGTH` (`lib.rs:69–71`).
Version 2 added the 320-byte padding region, moving the record from 184 to 504
bytes. The padding is opaque to the consumer but fixed-length so the layout
stays deterministic.

#### `ntcp2.static.key` — 132 bytes, version 1

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 8 | Magic `b"I2PRN2K\0"` |
| 8 | 2 | Format version (`1`) |
| 10 | 2 | Reserved (`0`) |
| 12 | 2 | Algorithm — must equal `ROUTER_CRYPTO_KEY_TYPE.code()` |
| 14 | 2 | Private key length (`32`) |
| 16 | 2 | Public key length (`32`) |
| 18 | 2 | IV length (`16`) |
| 20 | 32 | X25519 private seed |
| 52 | 32 | X25519 public key (derived) |
| 84 | 16 | Obfuscation IV |
| 100 | 32 | `SHA256(header ++ payload)` |

`NTCP2_FILE_LENGTH` = 20 + 32 + 32 + 16 + 32 = **132** (`lib.rs:80–84`).

#### `destination.identity` — 528 bytes (v2), 496 bytes (v1)

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 8 | Magic `b"I2PRSD\0\0"` |
| 8 | 2 | Format version (`1` or `2`) |
| 10 | 2 | Reserved (`0`) |
| 12 | 2 | Signing algorithm — `ROUTER_SIGNING_KEY_TYPE.code()` |
| 14 | 2 | Crypto algorithm — `ROUTER_CRYPTO_KEY_TYPE.code()` |
| 16 | 32 | Ed25519 signing seed |
| 48 | 32 | X25519 static inbound secret |
| 80 | 32 | Signing public key (derived) |
| 112 | 32 | X25519 public key (derived) |
| 144 | 256 | Legacy ElGamal public filler — v2 only |
| 400 | 96 | Destination padding — v2 only |
| 496 | 32 | `SHA256(header ++ payload)` |

v2 payload = 32·4 + 256 + 96 = 480, so `SERVICE_DESTINATION_FILE_LENGTH` = 16 +
480 + 32 = **528**. v1 reuses the crate's `IDENTITY_PADDING_LENGTH` (320) in
place of filler+padding, so `SERVICE_DESTINATION_V1_FILE_LENGTH` = 16 + 448 +
32 = **496**. Plan 223 introduced v2 (Java-compatible legacy ElGamal
Destination shape, reconstructs ElGamal/type-0 Destinations); v1 files carry the
pre-corrective X25519 Destination shape and are still decoded byte-identically.
Both lengths are accepted on load (`service_destination.rs:589–592`); new
generations always write v2 (`encode` at 540), and `migrate_from` is the
explicit upgrade operation.

### Atomic create-only semantics (identity and NTCP2 stores)

`IdentityStore::save_new` (`lib.rs:178`) and
`TransportStaticKeyStore::save_new` (`lib.rs:329`) follow one protocol:

1. `ensure_secure_directory` on the parent, then `reject_existing_target`,
   which returns `UnsafePath` for a symlink and `AlreadyExists` for any other
   existing entry.
2. Create a same-directory temporary file with `0o600` (`create_temporary_file`,
   `lib.rs:1012`).
3. `write_all` + `sync_all`, then `drop` the handle.
4. `fs::hard_link` to the target — **not** `rename`. `io::ErrorKind::AlreadyExists`
   from the link is mapped back to `StorageError::AlreadyExists`, so the
   create-only check still holds under concurrency.
5. Remove the temporary on both the success and error paths.
6. `sync_directory` the parent on Unix to flush the link entry.

There is **no file locking**. Correctness comes from the filesystem's atomic
link creation, exercised by `concurrent_create_only_writes_have_one_winner`
(8 threads, exactly one winner, `lib.rs:1347`).

The rotating opaque stores take a different, also atomic path: `publish` writes
and syncs a temporary, `fs::rename`s the previous current over the backup, then
`fs::rename`s the temporary over current, then syncs the directory. A failed
publication leaves the prior current file intact.

### Permission and path invariants

- **Directories** are created with mode `0o700` (`DirBuilderExt::mode`,
  `lib.rs:1054`) and validated with the mask `mode & 0o077 != 0` →
  `InsecurePermissions` (`lib.rs:1079–1088`). On non-Unix the check is a no-op.
- **Files** are created with mode `0o600` (`OpenOptionsExt::mode`,
  `lib.rs:1021`) and validated with `mode & 0o077 != 0 || mode & 0o400 == 0`
  (`lib.rs:1090–1103`) — i.e. no group/other access and owner-readable. The
  same mask is applied to address-book generations
  (`address_book_generation.rs:192`) and verified-content records
  (`verified_content_cache.rs:145`).
- **Symlinks and non-regular files** are rejected at every level: the data
  directory, the parent directory, the target file, and cache entries
  (`UnsafePath`).
- **Missing intermediate directories are not auto-created.**
  `ensure_secure_directory` creates only the final component and fails when the
  grandparent is absent (`lib.rs:1043–1058`; test
  `new_directories_are_private_and_missing_intermediates_are_not_created`).

### Explicit generate / load / rotate operations

There is no implicit bootstrap anywhere in this crate. Every write is an
explicit call:

- `IdentityStore::prepare_directory` creates or validates the directory
  *without* creating identity state.
- `TransportStaticKeyStore::generate_new` generates, then saves
  create-only; `save_new` persists supplied material; `load` revalidates.
- `ServiceDestinationStore::generate_new` generates a destination;
  `migrate_from` is a separate explicit v1→v2 operation.
- Opaque stores expose `publish` plus `load_current` / `load_backup`;
  a never-published slot loads as `Ok(None)`, and a missing directory loads as
  absent rather than erroring.

**Identity generate is the only writer.** `IdentityStore::save` (`lib.rs:215`)
is a compatibility spelling that delegates to `save_new`, so no code path can
replace an existing identity, and no path regenerates identity as a side effect
of a reload, restart, or configuration re-parse (ADR 0007). A corrupt or
wrong-version file fails closed with a typed error.

### Opaque store boundaries

- `cache_seam` never parses RouterInfo, ZIP, SU3, or NetDB records. Filenames
  are opaque 64-char lowercase hex. `i2pr-netdb-persist` decodes and validates
  through `i2pr-netdb`, then delegates atomic write/replace/remove/read/scan.
- `address_book_generation` never parses JSON, hostnames, or destinations.
  `i2pr-addressbook` owns `encode_generation` / `decode_generation`; the
  daemon composes the halves and decides activation.
- `verified_content_cache` never verifies SU3 or decodes NEWS. A loaded record
  must be re-verified by the daemon before it reaches a live consumer.

## Dependencies

`crates/i2pr-storage/Cargo.toml` — production:

| Dependency | Source | Purpose |
| --- | --- | --- |
| `i2pr-crypto` | path `../i2pr-crypto` | `X25519PrivateKey`, `RouterIdentityBundle`, `TransportStaticKey`, `PRIVATE_KEY_LENGTH`, `X25519_KEY_LENGTH`, algorithm codes, `sha256`, `constant_time_eq`, `CryptoError` |
| `rand_core` | workspace | `TryCryptoRng` for injected generation |
| `thiserror` | workspace | `StorageError` and the four sibling error enums |
| `zeroize` | workspace | `Zeroizing` wrappers for secret buffers |

Dev-only:

| Dependency | Source | Purpose |
| --- | --- | --- |
| `rand_chacha` | workspace | Deterministic `ChaCha8Rng` test seeds |
| `rand_core` | workspace (also a production dep) | `SeedableRng` in tests |
| `tempfile` | `"3.14"`, pinned in this crate's manifest — **not** a workspace dep | `tempdir()` for filesystem tests |

`tempfile` is a dev-dependency only; this crate has no third-party
production filesystem dependency, and the daemon owns the real data-directory
paths. The dependency-direction allowlist entry is
`"i2pr-storage": {"i2pr-crypto"}` (`scripts/check-dependency-direction.sh:28`),
so `i2pr-crypto` is the only permitted workspace dependency. Chain:
`i2pr-proto ← i2pr-crypto ← i2pr-storage`.

## Tests

All tests are in-crate `#[cfg(test)] mod tests` blocks. **There is no
`crates/i2pr-storage/tests/` directory** — the crate ships no integration-test
harness. A repo-level fixture at
`tests/fixtures/ntcp2/crypto/storage-static-key.hex` is pulled into the lib test
with `include_str!("../../../tests/fixtures/ntcp2/crypto/storage-static-key.hex")`
(`lib.rs:1406`); it is a fixture, not a test binary.

| Module | Test block | `#[test]` count |
| --- | ---: | ---: |
| `lib.rs` | 1164–1481 | 12 |
| `service_destination.rs` | 894–1232 | 10 |
| `address_book_generation.rs` | 222–305 | 3 |
| `verified_content_cache.rs` | 168–201 | 2 |

`lib.rs` coverage: round-trip preserving public identity; existing identity
never replaced; truncation at every boundary; max and max+1 bounding
(`TrailingBytes` then `TooLarge`); checksum, version, and public-material
mutations rejected; `0o600` file and private directory plus symlink rejection
(unix); new directories private and missing intermediates not created (unix);
8-thread concurrent create-only with one winner; NTCP2 key/IV round trip without
identity coupling; committed NTCP2 hex fixture loads strictly with the expected
IV; NTCP2 mutation and replacement rejection; NTCP2 store private file
permissions (unix).

Opaque-store coverage: `publish_load_and_rotation_round_trip`,
`symlink_and_special_files_are_rejected`, `oversized_files_fail_before_read`
(address book); `publishes_and_rotates_opaque_records`,
`rejects_oversized_records_and_symlink_loads` (verified content).

## Distinctive design choices

- **Byte-level, not domain-level, seams**: the cache, address-book, and
  verified-content stores never parse what they persist, so this crate cannot
  drift from the codecs that own those formats.
- **`hard_link` instead of `rename`** for create-only secret writes, giving
  atomic no-replace semantics with no file locking.
- **`reject_existing_target` plus `AlreadyExists` remapping** makes "never
  silently replaces" hold under races, not just sequential calls.
- **Permissions are validated by mask, not by exact mode**: `0o700`/`0o600` are
  what the crate *creates*; acceptance is "`no group/other bits` and, for files,
  `owner-readable`".
- **Zeroize discipline**: encoded buffers and decoded key/seed arrays are
  wrapped in `Zeroizing`, and `TransportStaticKeyMaterial` is neither `Clone`
  nor `Debug`; `ServiceDestinationRecord` has a hand-written `Debug` instead of
  a derived one.
- **Post-checksum re-derivation**: the public key is re-derived from the private
  key and compared with `constant_time_eq`, catching bit flips that a
  recomputed checksum would otherwise accept.
- **One shared path-safety core**: the private directory/permission/temporary
  helpers live in `lib.rs` and are reused by all three sibling modules, so the
  symlink and mode policy cannot diverge between stores.
- **v1 and v2 service-destination records both decode**, with no silent hash
  change; upgrade is an explicit `migrate_from` call.
- **Asymmetric rotation primitives**: `ByteCache::write` is insert-only,
  `ByteCache::replace` is the single rename-over-existing path, and the two
  opaque stores rotate current→backup on every publish.
- **No encryption at rest** is a documented Milestone 1 decision (ADR 0006),
  not an oversight.

## Cross-references

ADRs:

- [ADR 0006 — Versioned permission-hardened private identity storage](../adr/0006-private-identity-storage.md)
- [ADR 0007 — Explicit router identity first-run policy](../adr/0007-explicit-identity-first-run.md)
- [ADR 0011 — NTCP2 cryptographic composition and static-key persistence](../adr/0011-ntcp2-crypto-and-static-key-storage.md)

Plans and closure records (all verified to exist):

- Plan 013 — `plans/implementation/workspace-foundation/013-m1-identity-crypto-storage.md`,
  closure `plans/closure/workspace-foundation/013-closure.md` (identity + NTCP2 static-key storage)
- Plan 104 — `plans/closure/netdb/104-status.md` (`cache_seam` composition owner)
- Plan 175 — `plans/closure/service-tunnels/175-status.md` (service destinations)
- Plan 223 — v1→v2 service-destination format
- Plan 276 — `plans/closure/floodfill/276-status.md` (NetDB composition)
- Plan 294 — `plans/closure/i2pcontrol-proposal-170/294-status.md` (address-book generation adapter)
- Plan 322 — `plans/closure/i2pcontrol-proposal-170/322-status.md` (verified-content cache)

Related deep dives:

- [Overview](overview.md)
- [i2pr-crypto](i2pr-crypto.md) — owns the key types, `sha256`, `constant_time_eq`, and algorithm codes used by every record here
- [i2pr-netdb-persist](i2pr-netdb-persist.md) — the `cache_seam` composition owner that decodes and validates before delegating
- [i2pr-netdb](i2pr-netdb.md) — validates the RouterInfo bytes the seam stores
- [i2pr-addressbook](i2pr-addressbook.md) — canonical naming and generation serialization/validation owner
- [i2pr-daemon](i2pr-daemon.md) — owns the data directory, the `identity` CLI lifecycle, and NEWS verification
- [i2pr-runtime](i2pr-runtime.md) — owns Tokio, timers, and cancellation; this crate is synchronous
