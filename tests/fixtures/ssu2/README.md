# SSU2 v2 fixture corpus (Plans 155-156)

Spec-derived constructed vectors for the runtime-neutral SSU2 v2
foundation (`crates/i2pr-transport-ssu2`), plus Plan 156
locally-authored deterministic handshake vectors. Every file is
listed in `manifest.tsv` with a SHA-256 pin enforced by
`scripts/check-ssu2-vectors.sh`.

Plan 155 (spec-derived constructed vectors):

- `long-header.hex` — 32-byte SessionRequest long header fixture.
- `short-header-data.hex` — 16-byte Data short header fixture.
- `short-header-confirmed.hex` — 16-byte SessionConfirmed short
  header fixture (fragment 0 of 1).
- `blocks-positive.hex` — DateTime + ACK + Address + NewToken +
  Padding positive parse fixture.
- `blocks-malformed.hex` — duplicate-Padding negative fixture.

Plan 156 (locally-authored deterministic vectors; fixed test-only
keys/inputs documented in
`crates/i2pr-transport-ssu2/tests/handshake.rs`, which re-derives
every vector through the implementation and through an independent
raw-primitive path):

- `handshake-initial.hex` — initial transcript hash (32 bytes) plus
  initial chaining key (32 bytes) for responder static key
  `0x01..0x20`.
- `header-protection-request.hex` — protected SessionRequest-shaped
  datagram (fixed header, ephemeral `0x21*32`, intro `0x42*32`).
- `token-request.hex` — protected TokenRequest datagram (fixed
  connection IDs, packet number 7, timestamp 1700000000).
- `token-retry.hex` — protected Retry datagram (fixed token
  `0x0102030405060708`, timestamp 1700000000).
- `session-created-full.hex` — protected SessionCreated datagram
  from a full fixed-secret transcript (pins KDF/AEAD/protection).
- `session-confirmed-frag.hex` — protected SessionConfirmed
  fragment from the same fixed-secret chain (pins `se`/split input).

Provenance: constructed from the pinned SSU2 specification
(`specs/SOURCES.md`, website commit
`88596022920bdf99f27db27688faf4f204792fcd`). No private
keys, tokens, or operational secrets are committed; deterministic
test keys live inline in unit tests and are marked test-only.
