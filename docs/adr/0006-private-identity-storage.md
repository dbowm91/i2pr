# ADR 0006: Versioned permission-hardened private identity storage

- Status: Accepted
- Date: 2026-07-15

## Context

The router identity is security-critical local state. It must survive restart
without silently regenerating after corruption, while the first milestone does
not yet have a passphrase lifecycle, key derivation policy, recovery UX, or
interactive secret provider.

## Decision

`i2pr-storage` owns a narrow version-1 private identity store. Its exact binary
format is 184 bytes:

| Region | Size | Contents |
| --- | ---: | --- |
| Header | 24 | `I2PRID\0\0`, version, reserved field, signing/crypto IDs, four explicit lengths |
| Payload | 128 | signing seed, X25519 seed, derived signing public key, derived X25519 public key |
| Integrity | 32 | SHA-256 of header and payload |

All integers are big-endian and fixed-width. Version 1 accepts only type 7 and
type 4, all four key lengths equal 32, reserved bits zero, exact total
consumption, and the public keys derived from the stored private seeds. The
parser reads at most `MAX_IDENTITY_FILE_SIZE` (4 KiB) and rejects truncation,
trailing bytes, unsupported versions/algorithms, checksum changes, and
derived-public-key mismatches. Version 2 requires a new decision and migration
policy; internal Rust layout and serde are not storage contracts.

The data directory must be a non-symlink directory with no group/world mode
bits on Unix. Newly created directories use mode 0700 and identity files use
mode 0600. Existing insecure paths are rejected. Writes use a same-directory
temporary file, write and `sync_all`, then an atomic no-replace hard-link
install followed by temporary-file removal and directory sync on Unix. The
no-replace install is deliberate: unlike a normal rename, it cannot overwrite
an identity created concurrently by another process. Directory sync is a
no-op on platforms where the standard library does not expose that operation.

The format is integrity-protected but not encrypted. An attacker who can write
the file or directory can replace both contents and checksum, so filesystem
ownership and permissions are part of the threat model. Passphrase encryption,
key derivation, backup recovery, and rotation remain separate decisions.

## Consequences

Corruption is fail-closed and never causes silent identity replacement. The
operator must protect backups as private key material and must explicitly
delete/rotate an identity through a future lifecycle operation. The no-replace
atomic install is slightly more specific than a portable rename policy, but it
gives the required crash-safe and concurrent create-only behavior on the
supported Unix path.

## Review triggers

Review before adding replacement/rotation, encryption at rest, migrations,
multi-process locking, Windows-specific durability guarantees, or public
RouterInfo/NetDB storage.
