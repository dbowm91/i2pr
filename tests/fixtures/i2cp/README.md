# I2CP wire fixture corpus (Plan 164)

Committed vectors for the runtime-neutral I2CP foundation
(`crates/i2pr-api/src/i2cp/`). Every file is listed in
`manifest.tsv` with a SHA-256 pin enforced by
`scripts/check-i2cp-vectors.sh`.

All vectors are locally-authored deterministic fixtures generated
through the implemented codecs with fixed test-only inputs:

- destination keys `0x11` (X25519 public), `0x22` (Ed25519 signing
  public), padding `0x33`;
- SessionConfig/LeaseSet2 signatures are dummy `0x44`/`0x77` bytes
  (structural only; no signature is verified in Plan 164);
- LeaseSet2 decryption private key `0x99 * 32` is test-only material;
- clock values are fixed (`1786000000000` ms);
- payload `hello-i2cp`.

No private keys, live identities, tokens, or operational secrets are
present. Positive vectors decode to pinned field values asserted in
`crates/i2pr-api/tests/i2cp_vectors.rs`; malformed vectors assert
typed rejection (`UnknownMessageType`, `DeprecatedMessageType`,
`BodyTooLarge`, `Incomplete`, `Malformed`).

Oversized bodies above the 64 KiB ceiling are covered by generated
(max/max+1) unit tests rather than committed 64 KiB files; the
`malformed-oversize-length` vector pins the 5-byte header case that
must fail before allocation.
