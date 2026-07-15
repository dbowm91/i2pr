# Milestone 1 Plan A closure: bounded codec foundation

## Scope and outcome

This record closes `plans/011-m1-codec-foundation.md`. The change is limited
to reusable primitive codec mechanics in `i2pr-proto`; it does not implement
RouterIdentity, Destination, RouterInfo, LeaseSet, I2NP, networking, runtime
services, persistence, or capability advertisement.

## Changed files and public API

- `crates/i2pr-proto/src/codec.rs` — borrowed `DecodeCursor`, bounded
  `EncodeBuffer`, `CodecError`, strict `decode_exact`, `encode_to_vec`, and
  unit tests for truncation, bounds, canonical output mechanics, UTF-8, and
  error classification.
- `crates/i2pr-proto/src/lib.rs` — exports the narrow codec API while retaining
  the existing namespace vocabulary.
- `AGENTS.md`, `README.md`, and `docs/architecture.md` — document the codec
  boundary, explicit limits, ownership, exact consumption, and non-claiming
  status.
- `plans/011-closure.md` — this durable handoff and closure record.

The public API is intentionally function- and cursor-based:

- `DecodeCursor<'a>` borrows input and exposes checked fixed-width reads,
  bounded `u8`/`u16`/`u32` length-prefixed bytes and UTF-8, and `finish()`.
- `decode_exact(input, maximum, decoder)` enforces a top-level input maximum
  and rejects trailing bytes.
- `EncodeBuffer<'a>` appends deterministic big-endian values and bounded
  length-prefixed bytes/UTF-8 to a caller-provided `Vec<u8>`.
- `encode_to_vec(maximum, encoder)` creates a bounded output vector.
- `CodecError` distinguishes all categories required by Plan 011 without
  retaining attacker-controlled payloads.

## Limits and security decisions

- Every top-level decode and fresh encode receives an explicit maximum.
- Every length-prefixed read checks the declared length before taking bytes;
  encoders check caller limits and protocol prefix widths before emission.
- Cursor offsets and output lengths use checked arithmetic.
- Decoded byte fields are borrowed; no production helper allocates an owned
  field as part of primitive decoding.
- Errors retain only static context, offsets, and bounded numeric metadata.
- `i2pr-proto` remains `unsafe`-free and has no new dependency. It has no
  runtime, filesystem, CLI, tracing, or async writer abstraction.
- Test-only truncation, trailing-byte, and bit-mutation helpers are private to
  the crate test configuration. No production crate depends on `i2pr-testkit`.

## Tests and quality results

The codec unit suite covers empty input, every truncation prefix of a compound
fixture, offset overflow near `usize::MAX`, length maximum and maximum-plus-one
behavior, invalid UTF-8, strict trailing bytes, deterministic fixed-byte
encoding, exact output lengths, safe error display, and bounded primitive
round-trips. No external or copied test vectors were introduced; fixed bytes
are locally authored primitive expectations.

The final local validation run produced the following results:

| Command | Result |
| --- | --- |
| `rtk cargo fmt --all --check` | passed |
| `rtk cargo check --workspace` | passed |
| `rtk cargo check --workspace --all-targets` | passed |
| `rtk cargo test --workspace` | passed — 41 tests |
| `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings` | passed |
| `RUSTDOCFLAGS="-D warnings" rtk cargo doc --workspace --no-deps` | passed |
| `rtk bash scripts/check-dependency-direction.sh` | passed |
| `rtk cargo deny check advisories bans sources` | passed |
| `rtk rustup run 1.85.0 cargo check --workspace --all-targets` | passed |

No fuzz target was added because the repository has no established fuzz
workspace. The bounded cursor and its test harness are documented here for
Plan 014 to promote into maintained fuzz infrastructure.

## CI evidence and known limitations

The local quality matrix is complete. The pushed commit `d55b582` passed
[GitHub Actions CI run 29387346895](https://github.com/dbowm91/i2pr/actions/runs/29387346895).
The run passed dependency policy, Ubuntu and macOS quality, and Ubuntu MSRV
jobs. GitHub reported only non-blocking Node.js 20 deprecation annotations for
`actions/checkout@v4`; no job failed.

Known limitations at this handoff are intentional:

- No maintained fuzz target or seed corpus exists yet; Plan 014 owns that
  infrastructure.
- Fixed primitive bytes are locally authored expectations, not independent
  router interoperability vectors.
- No common I2P structure, cryptographic primitive, RouterInfo, identity,
  network transport, persistence, or capability advertisement is implemented.
- Allocation-failure behavior is bounded by pre-emission length checks, but no
  process-level out-of-memory simulation is attempted.

## Deviations, support status, and handoff

The plan's optional generic codec traits were intentionally not introduced.
Concrete cursor and encoder functions are sufficient for Plan 012 and keep
limits visible at call sites. A `u32` length-prefixed helper was included
alongside the required `u8` and `u16` forms because later common structures
need a checked wider declaration without introducing a generic parser layer.

`specs/support.toml` and `docs/protocol-support.md` remain unchanged: the
primitive mechanics are not a complete common-structure implementation and do
not constitute protocol interoperability evidence or a capability claim.

The next plan may consume this API for common structures. It must add its own
protocol-specific limits, canonicalization rules, vectors with provenance,
and negative tests; it must not treat this foundation's mechanics as evidence
that RouterIdentity, Destination, RouterInfo, or other structures are
supported.
