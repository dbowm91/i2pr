# Plan 297 — HTTP anonymity-profile convergence and differential qualification

Status: stopped-http-differential-capture-needs-controlled-three-router-topology

Classification: capability + evidence.

Hard dependencies: Plan 296 closed. Uses repository-pinned Java I2P 2.13.0 and i2pd 2.61.0 as exact behavioral references.

## Objective

Turn the HTTP client tunnel's privacy behavior from a collection of locally reasonable rewrites into one bundled, reference-derived default profile whose observable HTTP/1.x behavior does not create an obvious i2pr-only fingerprint.

## Why ready after Plan 296

Plan 296 establishes resolved-Destination Host authority and removes literal implementation branding. This plan can then compare remaining HTTP behavior without conflating direct leaks with profile-level differences.

## Current implementation evidence

The parser lowercases header names and the serializer emits rewritten headers deterministically. The current privacy policy strips several proxy/identity headers and normalizes User-Agent, but browser/locale/platform headers and aggregate order/casing/connection behavior have not been differentially qualified.

Likely classification dimensions include header casing/order, `Connection: close`, Accept-family fields, Client Hints, Fetch Metadata, Priority, upgrade hints, and application-specific headers. Treatment must be evidence-driven.

## Invariants

- Keep Plan 296 no-brand/no-local-alias guarantees.
- Do not randomize casing/order/tokens as a substitute for convergence.
- Do not MITM TLS or inspect CONNECT payloads.
- Do not weaken parser/header limits.
- Preserve ordinary I2P HTTP semantics.
- Raw/compatibility behavior is never described as anonymity-equivalent to the qualified profile.

## Scope

### In

Black-box HTTP capture through pinned Java/i2pd proxy references; a small named profile surface with one qualified default; evidence-driven normalization/stripping of identifying request fields; controllable request-line/header casing/order/connection behavior; differential fixtures and exact-pin evidence.

### Out

TLS/browser normalization through CONNECT; HTTP/2/3 termination; response-content modification; Streaming/tunnel timing; unrelated outproxy policy.

## Required production changes

### A. Reference capture first

Create an environment-gated harness that sends a fixed request matrix through exact Java/i2pd HTTP client proxy paths to a controlled remote I2P HTTP fixture and captures request-line/header metadata only.

Matrix includes simple GET/POST, local alias/B32 target, common Accept headers, Accept-Language/Encoding, Client Hints (`Sec-CH-UA*`), Fetch Metadata (`Sec-Fetch-*`), `Priority`, `Upgrade-Insecure-Requests`, Referer/From/Via/Forwarded/X-Forwarded-*, custom headers, and connection-related fields.

Sanitize values not required for comparison and source-lock reference versions.

### B. Freeze one coherent target profile

From observed behavior plus current I2P HTTP proxy documentation, define one `AnonymityCompatible` profile (name may vary). Prefer common deployed behavior. Where Java/i2pd differ, choose one coherent reference profile or documented common privacy rule; do not invent an arbitrary field-by-field hybrid that is more unique than either family.

Record each field disposition as pass/strip/replace/canonicalize with rationale.

### C. Bundle public configuration

Ordinary users select a small named privacy profile instead of independently composing every low-level behavior. Existing low-level controls may remain as explicit advanced compatibility options when API compatibility requires them.

Default maps to the qualified profile. Raw/keep requires explicit opt-in and documentation that it exposes application/browser identifiers.

### D. Wire-shape convergence

Where serializer behavior itself is a classifier, align request-line form, canonical header presentation/order, and connection semantics with the selected target, subject to correctness/safety.

Do not reproduce invalid reference behavior. Intentional divergence remains explicit in evidence.

### E. Differential checker

Add a deterministic comparator over sanitized captures with categories: exact/common match, semantically equivalent but wire-different, intentional safety divergence, unresolved fingerprint divergence. Closure must not collapse them.

## Ordered work packages

1. Build/source-lock reference capture lane.
2. Execute reference matrix and commit sanitized fixtures.
3. Freeze profile contract in runtime-neutral service-tunnel code/tests.
4. Refactor named profile configuration.
5. Align controllable wire-shape differences.
6. Run differential matrix against i2pr and both reference families.
7. Update docs/evidence.

## Failure/cancellation/restart/contention

External harness remains test-only. Missing reference artifacts fail the explicit lane and never create passing evidence. Production request handling keeps existing budgets/teardown; profile logic stays pure/bounded.

## Compatibility and migration

Preserve existing explicit privacy-policy parsing where practical, mapping it to advanced/custom mode. Default may become more privacy-preserving. Deprecate ambiguous combinations rather than silently remap them.

## Required tests

- literal profile inventory/default;
- explicit disposition for every registered identifying header class;
- custom/raw never selected implicitly;
- Java/i2pd/i2pr differential fixtures;
- casing/order assertions where target requires them;
- parser max/+1 bounds;
- GET/POST functional tests;
- CONNECT explicitly excluded and tested opaque.

## Exact verification commands

~~~text
cargo fmt --all --check
cargo test --locked -p i2pr-service-tunnels --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_local_roundtrip -- --test-threads=1
bash scripts/check-service-anonymity-boundaries.sh
bash tests/integration/service-tunnels/run-http-anonymity-reference.sh
bash scripts/check-http-anonymity-evidence.sh
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
~~~

Equivalent names matching repository conventions are acceptable if closure records them exactly.

## Documentation updates

Document named profiles, reference basis, raw/compatibility warning, and that CONNECT/TLS/browser fingerprints are not normalized.

## Acceptance criteria

Plan 297 passes only if:
1. exact-pinned Java/i2pd captures exist or the plan stops without claiming differential qualification;
2. default profile has a complete disposition matrix;
3. Plan 296 no-brand/no-alias properties remain;
4. i2pr matches selected target for every required dimension, with intentional divergences separately justified;
5. no randomization;
6. HTTP function/resource bounds remain green;
7. evidence checker and routine floor pass.

## Stop conditions

Stop if reference behavior cannot be reproduced on exact pins; selecting a profile would hide semantic incompatibility; or a proposed fix requires TLS interception.

## Closure evidence required

Retain pins, sanitized captures, disposition matrix, comparator summary, exact commands, migration notes, and unresolved divergences.

## Handoff notes

Plan 301 consumes this closure directly. Later HTTP feature work extends the matrix rather than adding ad-hoc default rewrites.
