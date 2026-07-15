# i2pr-proto fuzzing

This is an independent cargo-fuzz workspace. Its runtime dependencies are
local protocol, crypto, storage, and NTCP2 crates; `libfuzzer-sys` is
intentionally confined here and is used by nightly-only cargo-fuzz builds.

Each target rejects input above its caller-visible maximum before invoking a
decoder. Harnesses perform only pure parsing: they do not open sockets, read or
write files, use the clock, or access global router state.

The `date`, `date32`, and `hash` targets cover the fixed-width public
primitive decoders. `i2np_bodies` selects each independently complex I2NP body
type and constructs a bounded standard envelope around arbitrary body bytes,
so body parsers are fuzzed independently without duplicating a corpus for
every message identifier. The remaining I2NP targets exercise each top-level
header entry point directly.

Run a short local smoke pass from the repository root with:

```text
bash scripts/fuzz-smoke.sh
```

For an individual campaign, install `cargo-fuzz` and run, for example,
`cargo fuzz run --manifest-path fuzz/Cargo.toml router_info -- -runs=1000`.
Long campaigns should retain minimized regressions in the matching corpus.

The seed files are deliberately small, locally authored malformed inputs. See
`corpus/metadata.toml` for their provenance; they are not network captures,
peer identities, or copied implementation fixtures.

The `ntcp2_transcript` and `ntcp2_storage` targets cover bounded synthetic
transcript sequencing and exact-format transport-static-key record decoding;
they never use operational keys or filesystem access.

Plan 034 adds `ntcp2_blocks` for authenticated-plaintext block and ordering
parsing and `ntcp2_frames` for bounded length/ciphertext and counter-state
commands using fixed test-only keys. Both targets are pure, bounded, and
payload-redacted; unauthenticated input never yields application blocks.

Plan 036's local validation runs the complete smoke list at 32 deterministic
runs per target and runs `ntcp2_handshake`, `ntcp2_blocks`, `ntcp2_frames`, and
`ntcp2_transcript` at 1,000 runs each with fixed seed `36`. In managed ptrace
environments, set `LSAN_OPTIONS=detect_leaks=0`; this disables only the runner's
known LeakSanitizer shutdown incompatibility and does not alter the fuzz
target. These campaigns are pure local evidence and do not replace the
required Java I2P/i2pd controlled interoperability lane.
