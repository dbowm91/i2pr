# `i2pr-storage` — Deep Dive

Versioned, atomic, permission-hardened persistence for the router
identity and the NTCP2 transport static key. Two independent stores,
two independent files. No network I/O, no async.

Path: `crates/i2pr-storage/`

## Purpose

`i2pr-storage` persists two classes of local router secret material:

1. **Router identity** — Ed25519 signing seed + X25519 encryption seed,
   together with the derived public keys. Persisted as
   `<data_dir>/router.identity`.
2. **NTCP2 transport static key** — an X25519 keypair plus a published
   obfuscation IV. Persisted as `<data_dir>/ntcp2.static.key`, in a
   record **distinct** from the router identity.

Files are stored under a `0o700` directory with `0o600` file
permissions, each with its own magic, format-version header,
fixed-length payload, SHA-256 checksum, and zeroize-on-drop encoded
buffers. Files are **not** encrypted at rest — filesystem permissions
and operator backup handling are the Milestone 1 threat-model
boundary.

The NTCP2 static key is deliberately decoupled from the router
identity so a restart cannot silently change the `RouterAddress`
contract.

The crate owns no network I/O and no async. It is pure synchronous
filesystem code gated behind `#![forbid(unsafe_code)]`.

## Module layout

Single file crate — `src/lib.rs`.

## Public surface

### Constants

| Item | Line | Value |
| --- | --- | --- |
| `IDENTITY_FILE_NAME` | 25 | `"router.identity"` |
| `MAX_IDENTITY_FILE_SIZE` | 27 | `4096` |
| `IDENTITY_FORMAT_VERSION` | 29 | `1` |
| `NTCP2_TRANSPORT_KEY_FILE_NAME` | 32 | `"ntcp2.static.key"` |
| `MAX_NTCP2_TRANSPORT_KEY_FILE_SIZE` | 34 | `4096` |
| `NTCP2_TRANSPORT_KEY_FORMAT_VERSION` | 36 | `1` |

### Errors

- `enum StorageError` (line 60). Variants: `Io`, `UnsafePath`,
  `AlreadyExists`, `InsecurePermissions`, `TooLarge`, `Truncated`,
  `TrailingBytes`, `Malformed`, `UnsupportedVersion`,
  `UnsupportedAlgorithm`, `Integrity`, `Crypto` (transparent from
  `CryptoError`).

### Types

- `struct IdentityStore` (line 124) with:
  - `new`, `in_data_dir`, `path`, `prepare_directory`, `save_new`,
    `save`, `load`.
- `struct TransportStaticKeyMaterial` (line 228) with:
  - `generate`, `from_parts`, `key`, `iv`. **Not `Clone`**, no
    `Debug`.
- `struct TransportStaticKeyStore` (line 261) with:
  - `new`, `in_data_dir`, `path`, `generate_new`, `save_new`, `load`.
- `fn decode_transport_static_key` (line 360) — bounded entry point
  used by the isolated fuzz harness.

### Derived file-length constants
- `IDENTITY_FILE_LENGTH` = 184
- `NTCP2_FILE_LENGTH` = 132

## Record layouts

### `router.identity` (184 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 8 | Magic `b"I2PRID\0\0"` |
| 8 | 2 | Format version (`1`) |
| 10 | 2 | Reserved (`0`) |
| 12 | 2 | Signing algorithm (must equal `ROUTER_SIGNING_KEY_TYPE.code()`) |
| 14 | 2 | Encryption algorithm (must equal `ROUTER_CRYPTO_KEY_TYPE.code()`) |
| 16 | 2 | Signing private len (`32`) |
| 18 | 2 | Encryption private len (`32`) |
| 20 | 2 | Signing public len (`32`) |
| 22 | 2 | Encryption public len (`32`) |
| 24 | 32 | Signing private key (Ed25519 seed) |
| 56 | 32 | Encryption private key (X25519 seed) |
| 88 | 32 | Signing public key (derived) |
| 120 | 32 | Encryption public key (derived) |
| 152 | 32 | `SHA256(header ++ payload)` checksum |

### `ntcp2.static.key` (132 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 8 | Magic `b"I2PRN2K\0"` |
| 8 | 2 | Format version (`1`) |
| 10 | 2 | Reserved (`0`) |
| 12 | 2 | Algorithm (must equal `ROUTER_CRYPTO_KEY_TYPE.code()`) |
| 14 | 2 | Private key len (`32`) |
| 16 | 2 | Public key len (`32`) |
| 18 | 2 | IV len (`16`) |
| 20 | 32 | X25519 private seed |
| 52 | 32 | X25519 public key (derived) |
| 84 | 16 | Obfuscation IV |
| 100 | 32 | `SHA256(header ++ payload)` checksum |

Version migration policy: there is no migration path in this crate.
Version bumps land in a new plan milestone; the old reader stays
intact and a new branch is added. `RESERVED_HEADER` is currently
checked to be zero.

## Encoding boundaries

**Serialization** is manual big-endian (`push_u16` + `extend_from_slice`)
— no serde, no `bincode`. Output is wrapped in `Zeroizing<Vec<u8>>` so
the encoded buffer holding private bytes is wiped on drop.

**Deserialization**:

- Zero-copy `Reader` cursor over the input slice.
- `Reader::finish()` rejects trailing bytes.
- Bounds: `Truncated` for short input, `TooLarge` for > 4096 bytes.
- Magic, version, algorithm IDs, and length fields are validated
  against compile-time constants **before** any key material is
  extracted.
- The SHA-256 checksum covers `header ++ payload` (everything before
  the trailing 32 bytes); verified with `constant_time_eq`.
- After decoding, the derived public key is re-derived from the
  private key and compared against the stored public key with
  `constant_time_eq` — catches bit-flip corruption that would
  preserve the checksum.
- Unknown fields are structurally impossible: the exact byte count
  is enforced by the fixed-length layout; any extra bytes trigger
  `TrailingBytes`.

## Atomic write protocol

`save_new` and `TransportStaticKeyStore::save_new` follow the same
pattern:

1. Create a temporary file with `0o600` permissions.
2. Write + sync.
3. `hard_link` to the target (not `rename`). The pair of
   `reject_existing_target` + `hard_link` gives "no-replace"
   semantics and atomic one-winner creation.
4. Clean up the temp file on both success and error.
5. `sync_all` the parent directory on Unix to flush the link entry.

There is **no file locking** — correctness comes from the filesystem's
atomic link creation, verified by the
`concurrent_create_only_writes_have_one_winner` test (8 threads, one
winner).

## Path safety

- Symlinks are rejected at every level: the data directory, the
  identity file, the parent directory.
- On Unix, directory permissions must exclude group + other
  (`0o077` mask); files must be `0o600` (owner read-write only).
- Missing intermediate directories are **not** auto-created.

## Dependencies

| Dependency | Source | Purpose |
| --- | --- | --- |
| `i2pr-crypto` | path | `X25519PrivateKey`, `RouterIdentityBundle`, `TransportStaticKey`, algorithm constants, `sha256`, `constant_time_eq`, `CryptoError` |
| `rand_core` | workspace | `TryCryptoRng` |
| `thiserror` | workspace | `StorageError` derive |
| `zeroize` | workspace | `Zeroizing` wrappers |
| `rand_chacha` (dev) | — | Deterministic test seeds |
| `tempfile` (dev) | — | Filesystem tests |

Dependency chain: `i2pr-proto ← i2pr-crypto ← i2pr-storage`. Confirmed.

## Tests

Inline in `src/lib.rs:722-1039` and `tests/fixtures/ntcp2/crypto/storage-static-key.hex`
loaded by `include_str!`:

| Test | Line | Coverage |
| --- | --- | --- |
| `save_load_round_trip_preserves_public_identity` | 756 | encode → write → read → decode |
| `existing_identity_is_never_replaced` | 774 | `save_new` on existing file → `AlreadyExists` |
| `truncation_at_every_boundary_is_rejected` | 788 | Every prefix of a valid blob is rejected |
| `maximum_and_maximum_plus_one_are_bounded` | 799 | Zero-padded + max+1 byte files both rejected |
| `checksum_version_and_public_material_mutations_are_rejected` | 812 | Bit-flip, version bump, public-key mismatch with re-checksum |
| `generated_permissions_are_private_and_symlinks_are_rejected` (unix) | 840 | `0o600` file, `0o700` dir, symlink rejection |
| `new_directories_are_private_and_missing_intermediates_are_not_created` (unix) | 876 | Owner-only perms, no auto-create |
| `concurrent_create_only_writes_have_one_winner` | 905 | 8-thread race, one winner |
| `ntcp2_static_key_and_iv_round_trip_without_identity_coupling` | 946 | NTCP2 store independence |
| `committed_ntcp2_static_key_fixture_loads_strictly` | 961 | Hex fixture from `tests/fixtures/ntcp2/crypto/` |
| `ntcp2_static_key_rejects_mutations_and_replacement` | 980 | All mutation classes |
| `ntcp2_static_key_store_has_private_file_permissions` (unix) | 1023 | `0o600` |

## Distinctive design choices

- **Zeroize discipline**: decoded keys held in `Zeroizing<>`; encoded
  buffers wrapped in `Zeroizing<Vec<u8>>`; `TransportStaticKeyMaterial`
  is **not** `Clone` and has no `Debug`.
- **Atomic write via `hard_link`** rather than rename — combines atomic
  link creation with `AlreadyExists` to give no-replace semantics
  without file locks.
- **Path safety as a first-class concern**: symlinks rejected
  everywhere; perms enforced on the data dir and target files.
- **No encryption at rest** is an explicit, documented design choice
  (Milestone 1 threat model).
- **Re-derived public key compared against stored public key with
  constant-time eq** catches bit-flips that preserve the checksum.
- **`decode_transport_static_key`** is the only public decoder and is
  explicitly the fuzz-harness entry point.

## Cross-references

- [Overview](overview.md)
- [i2pr-crypto](i2pr-crypto.md) — owns key types and constants
- [i2pr-daemon](i2pr-daemon.md) — uses `IdentityStore` for the
  `identity generate`/`inspect` commands
- Plan-of-record: `plans/013-m1-identity-crypto-storage.md`
- Related closure: `plans/013-closure.md`
