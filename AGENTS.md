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
`i2pr-core <- i2pr-runtime <- i2pr-daemon` is the runtime composition path;
the daemon remains the process composition root. `i2pr-runtime` is the only
production crate allowed to depend on Tokio or `tokio-util`; protocol, crypto,
storage, and `i2pr-core` remain runtime-neutral.

Use the local quality commands documented in `CONTRIBUTING.md`. Configuration
and protocol inputs are untrusted: keep parsing bounded, reject unknown
fields, avoid side effects during validation, and test negative paths. Do not
claim protocol support before interoperability evidence exists.

Plan 021 supervision rules are mandatory: every long-lived task must be owned
by the supervisor or a service child scope, and every owned task must be
awaited, explicitly aborted after a recorded deadline, or transferred to a
documented owner. Discarded `JoinHandle`s and detached `tokio::spawn` calls are
not allowed. Service startup must validate the complete graph before spawning,
use explicit one-shot readiness, publish bounded latest-state health, and use
typed static failure categories. Runtime tests must use paused Tokio time or
explicit deterministic advancement; wall-clock sleeps are not acceptable.

Plan 022 communication rules are mandatory: every asynchronous queue must have
an explicit nonzero capacity below the infrastructure ceiling; sends and
receives must expose typed overload, closure, deadline, and cancellation
outcomes; service-to-service sends must not wait without a caller-visible
deadline or cancellation scope; and latest-state values must not imply
lossless history. Resource leases are non-cloneable, own one exact grant, and
release on drop, explicit consuming release, cancellation, timeout, panic, and
forced cleanup. Queue admission occurs before payload ownership enters a queue,
and an accepted queue item owns its charge until receiver handoff or drop.
Do not add unbounded Tokio channels, hidden retry loops, dynamic peer-derived
channel identifiers, or partial multi-class resource acquisition.

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

Plan 023 simulation rules are mandatory: use a documented root
`ReproducibilitySeed` plus a stable scenario identifier in every deterministic
failure report; derive independent component seeds rather than sharing mutable
RNG state across components. Replay records must contain only bounded seed,
scenario, sequence, fault-category, queue, timer, task, and resource metadata;
never include payloads, private keys, destinations, full RouterInfo values, or
real addresses. Keep stream and datagram links semantically distinct, admit
queue items before payload ownership, and enforce explicit pending-delivery,
buffer, duplicate, rule, peer, timer, and step limits. Fault scripts belong
only to `i2pr-testkit` and authorized isolated testnets; never run malformed,
stress, or fault-injection traffic against the public I2P network. A testkit
shutdown must purge queued work, wake waiters, and document any still-live
endpoint lease ownership; no detached simulation task is permitted.

The common and I2NP implementations expose grouped private leaf namespaces
through `crates/i2pr-proto/src/common/` and `src/i2np/`; preserve the crate-root
re-export façade and keep decode helpers private. `ReplySecret` is a
non-cloneable zeroizing wrapper for DatabaseLookup reply keys/tags and must not
be broadened into encrypted-reply semantics. Committed I2NP fixtures must use
the manifest schema in `tests/fixtures/i2np/manifest.tsv`, include provenance,
classification, deterministic inputs, hashes, and independence status, and be
consumed by tests rather than only hash-checked. New identity directories must
use creation-time restrictive modes; do not reintroduce create-then-chmod.
