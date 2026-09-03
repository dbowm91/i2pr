# SSU2 v2 fixture corpus (Plan 155)

Spec-derived constructed vectors for the runtime-neutral SSU2 v2
foundation (`crates/i2pr-transport-ssu2`). Every file is listed in
`manifest.tsv` with a SHA-256 pin enforced by
`scripts/check-ssu2-vectors.sh`.

- `long-header.hex` — 32-byte SessionRequest long header fixture.
- `short-header-data.hex` — 16-byte Data short header fixture.
- `short-header-confirmed.hex` — 16-byte SessionConfirmed short
  header fixture (fragment 0 of 1).
- `blocks-positive.hex` — DateTime + ACK + Address + NewToken +
  Padding positive parse fixture.
- `blocks-malformed.hex` — duplicate-Padding negative fixture.

Provenance: constructed from the pinned SSU2 specification
(`specs/SOURCES.md`, website commit
`88596022920bdf99f27db27688faf4f204792fcd`). No private
keys, tokens, or operational secrets are committed; deterministic
test keys live inline in unit tests and are marked test-only.
