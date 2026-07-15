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
`i2pr-testkit`, and lower-level crates must not depend on `i2pr-daemon`.

Use the local quality commands documented in `CONTRIBUTING.md`. Configuration
and protocol inputs are untrusted: keep parsing bounded, reject unknown
fields, avoid side effects during validation, and test negative paths. Do not
claim protocol support before interoperability evidence exists.

The `i2pr-proto` codec foundation uses borrowed cursors and caller-visible
maximums. New protocol decoders should use strict top-level consumption and
typed `CodecError` categories; do not add hidden unlimited defaults, runtime or
filesystem dependencies, or speculative universal codec traits.

Do not select a project license or copy implementation code from another router
without explicit owner review. Do not perform malformed-traffic or stress
testing against the public I2P network.
