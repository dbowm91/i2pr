# Repository agent instructions

These instructions supplement the environment-provided RTK command guidance.

Before changing code, read `README.md`, `GUARDRAILS.md`, the applicable plan in
`plans/`, and any relevant ADR in `docs/adr/`. Protocol work also requires the
matching dossier under `specs/protocols/` and `specs/CONFORMANCE.md`.

Milestone closure work must leave an explicit closure record with the changed
files, deviations, dependency and security decisions, quality-command results,
CI evidence, and known limitations. Keep `specs/support.toml` synchronized
with `docs/protocol-support.md`; code or namespace presence is not protocol
support evidence.

Keep changes plan-first and bounded. Preserve the dependency direction shown in
`docs/architecture.md`; do not add future transport, NetDB, tunnel, client, or
plugin APIs without a detailed plan. Production crates must not depend on
`i2pr-testkit`, and lower-level crates must not depend on `i2pr-daemon`. The
current direction is `i2pr-proto <- i2pr-crypto <- i2pr-storage`, with
`i2pr-daemon` as the composition root over those crates and `i2pr-core`.

Use the local quality commands documented in `CONTRIBUTING.md`. Configuration
and protocol inputs are untrusted: keep parsing bounded, reject unknown
fields, avoid side effects during validation, and test negative paths. Do not
claim protocol support before interoperability evidence exists.

The `i2pr-proto` codec foundation uses borrowed cursors and caller-visible
maximums. New protocol decoders should use strict top-level consumption and
typed `CodecError` categories; do not add hidden unlimited defaults, runtime or
filesystem dependencies, or speculative universal codec traits.

The Plan 014 I2NP module is structural only: standard, obsolete-SSU, and
NTCP2/SSU2 short headers, checksum/length validation, typed dispatch, and
bounded selected bodies/framing are allowed. Expiration policy, duplicate
suppression, routing, transport authentication, NetDB actions, tunnel crypto
or reassembly, garlic decryption, and capability advertisement remain outside
`i2pr-proto`. Deferred/opaque bodies must be named explicitly and redact raw
bytes in `Debug` output.

The maintained fuzz workspace lives under `fuzz/`, is not a production
workspace member, and uses nightly-only `cargo-fuzz` with bounded inputs. Seed
corpora must be locally authored or provenance-recorded, sanitized, hashed,
and free of private keys, live peer captures, addresses, and destinations.
Run `bash scripts/check-fixture-manifest.sh` when fixture bytes change and use
`bash scripts/fuzz-smoke.sh` for the opt-in short fuzz lane.

The common-structure model in `crates/i2pr-proto/src/common/` preserves exact
signed byte regions, uses immutable sorted mappings, and treats algorithm
identifiers and lengths as explicit typed data. It is structural only: do not
add signing, encryption, freshness policy, transport interpretation, or
capability advertisement there. Plan 013's type-7 Ed25519/type-4 X25519
execution belongs in `i2pr-crypto`; versioned private identity persistence
belongs in `i2pr-storage`. Secret wrappers must remain non-debuggable,
non-cloneable where practical, and zeroizing. Keep `specs/support.toml` and
`docs/protocol-support.md` aligned with the evidence available for each exact
surface.

The explicit identity commands are intentionally narrow: generation is
create-only, inspection never prints private material, dry-run never mutates
identity state, and corrupt identity files must fail closed rather than trigger
silent regeneration. The storage format, Unix permissions, atomic install,
checksum, and at-rest threat model are recorded in ADR 0006.

Do not select a project license or copy implementation code from another router
without explicit owner review. Do not perform malformed-traffic or stress
testing against the public I2P network.

The common and I2NP implementations expose grouped private leaf namespaces
through `crates/i2pr-proto/src/common/` and `src/i2np/`; preserve the crate-root
re-export façade and keep decode helpers private. `ReplySecret` is a
non-cloneable zeroizing wrapper for DatabaseLookup reply keys/tags and must not
be broadened into encrypted-reply semantics. Committed I2NP fixtures must use
the manifest schema in `tests/fixtures/i2np/manifest.tsv`, include provenance,
classification, deterministic inputs, hashes, and independence status, and be
consumed by tests rather than only hash-checked. New identity directories must
use creation-time restrictive modes; do not reintroduce create-then-chmod.
