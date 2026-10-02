# Plan 296 — Service-boundary implementation neutrality and leak regression

Status: registered-anonymity-service-boundary-implementation-neutrality

Classification: invariant + corrective capability.

Hard dependencies: M10 service-tunnel product authority (Plan 215), existing Destination resolution/Streaming composition, and accepted ADR 0029. No M12 dependency.

## Objective

Remove direct i2pr implementation branding and user-local naming leakage from service-tunnel application boundaries, and add fail-closed regression tests that prevent equivalent leaks from returning.

## Why this plan is ready

The affected behavior is already implemented and locally testable. The audit localized concrete defects to service-tunnel policy/rewrite/composition code; no new router protocol capability is required.

## Current implementation evidence

At registration:
- `crates/i2pr-service-tunnels/src/http/config.rs` defines `DEFAULT_USER_AGENT_VALUE = "i2pr/0.1"` and default replacement behavior.
- `http/target.rs` retains caller-visible host and `http/rewrite.rs` synthesizes Host from that target rather than resolved Destination.
- `crates/i2pr-daemon/src/service_tunnels_http.rs` emits `Proxy-Agent: i2pr` in CONNECT success.
- `crates/i2pr-service-tunnels/src/irc/config.rs` defines optional stable reason replacement as `i2pr`.
- Generic/SOCKS/CONNECT data paths otherwise behave as opaque forwarding and should be protected from future synthesized branding.

## Invariants that must not regress

1. Remote application-visible router-synthesized bytes contain no i2pr product name, package/release version, build/git identifier, local hostname/IP, filesystem path, or caller-local Destination alias.
2. HTTP remote naming derives from resolved Destination and canonical B32, not local address-book spelling.
3. Default client Destination isolation stays dedicated.
4. No TLS MITM or arbitrary payload inspection.
5. Existing loopback/default-off/resource/cancellation/dependency boundaries remain.
6. Protocol-required RouterInfo metadata is outside this application-boundary rewrite.

## Scope

### In

HTTP User-Agent default correction; resolved-Destination B32 Host synthesis; removal of CONNECT `Proxy-Agent: i2pr`; removal of router-branded IRC stable-reason defaults/presets; transcript-level negative leak tests for application/control boundaries; a static boundary checker; focused documentation of opaque-forwarding limitations.

### Out

Broad browser-header normalization (Plan 297), Streaming behavior (Plans 298–299), tunnel path/hop diversity (Plan 300), arbitrary-application anonymity, RouterInfo version policy.

## Required production changes

### A. HTTP User-Agent convergence floor

Change the qualified default away from `i2pr/0.1` to the exact implementation-neutral stable User-Agent shared by pinned Java/i2pd HTTP proxy behavior at execution time. The audit baseline is `MYOB/6.66 (AN/ON)`; source-lock exact pinned references before landing the value.

Do not replace it with another i2pr-specific token. Explicit expert override may remain only as a documented non-qualified compatibility path.

### B. Resolved B32 Host authority

Refactor HTTP rewrite/composition so Host synthesis has the resolved `Destination` or a narrow canonical B32 derived from it:

1. parse/validate caller target;
2. resolve to Destination;
3. derive canonical lowercase B32 host from Destination hash;
4. synthesize remote Host plus semantically required non-default port;
5. never transmit local alias by default.

Do not duplicate resolver/address-book ownership.

### C. Local CONNECT branding

Return minimal successful CONNECT response without `Proxy-Agent: i2pr`. No replacement product token.

### D. IRC reason behavior

Default QUIT/PART privacy rewriting must not insert `i2pr`. Prefer stripping optional reason text where protocol-compatible. If stable replacement remains, it is protocol-generic, unbranded, and non-default.

### E. Boundary regression harness

Add tests over bytes actually emitted. Reject seeded case-insensitive `i2pr`, router-origin semantic-version/build markers, configured local aliases, local hostname/IP sentinels, build SHA sentinels, and filesystem sentinels.

Add `scripts/check-service-anonymity-boundaries.sh` (or equivalent existing-style name). It scans test transcripts/static service-boundary constants, not the whole repository.

## Ordered work packages

1. Freeze exact reference User-Agent/Host behavior.
2. Add failing tests for the four concrete findings.
3. Refactor Host rewrite around resolved Destination.
4. Remove/replace direct branding defaults.
5. Add negative transcript/static checker.
6. Run focused/workspace verification.
7. Update documentation/status after evidence.

## Failure, cancellation, restart, and contention semantics

No new long-lived tasks or persistent state. Resolver failure occurs before remote application bytes are emitted. Failed canonical-B32 derivation is typed local failure, never alias fallback. Existing stream teardown remains unchanged.

## Compatibility and migration

Explicit raw/keep User-Agent config may remain. Any behavior that intentionally exposed a local alias in remote Host is changed as a privacy/security correction. Destination key persistence/service names do not change.

## Required tests

- default HTTP emits pinned neutral UA and no `i2pr`;
- local alias resolves to canonical B32 remote Host and alias is absent;
- explicit B32 is canonical/idempotent;
- non-default port handling;
- CONNECT success has no Proxy-Agent/product token;
- IRC default/replace path has no router brand;
- generic/SOCKS paths add no synthetic product marker;
- existing max/+1 header/target bounds;
- negative scanner catches seeded violations.

## Exact verification commands

~~~text
cargo fmt --all --check
cargo test --locked -p i2pr-service-tunnels --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_local_roundtrip -- --test-threads=1
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-service-anonymity-boundaries.sh
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
~~~

## Documentation updates

Update service-tunnel docs and security model only to record the new invariant/current non-claim. Register exact reference pins used for UA/Host expectations.

## Acceptance criteria

Plan 296 passes only if:
1. no qualified default service-tunnel transcript contains i2pr product/version token;
2. HTTP local aliases are absent remotely and Host is canonical resolved B32;
3. CONNECT/IRC branding findings are closed;
4. opaque forwarding is regression-protected against router-added branding;
5. checker fails on seeded leak fixtures;
6. routine/focused verification is green;
7. no broader anonymity claim is added.

## Stop conditions

Stop/register a corrective if pinned Java/i2pd disagree on the supposed common UA/B32 behavior, Host canonicalization would duplicate resolver state, or tests require inspecting/rewriting opaque TLS payloads.

## Closure evidence required

Record implementation commit, exact reference pins, sanitized before/after transcript summaries or hashes, checker/focused/full-floor results, and compatibility deviations.

## Handoff notes

This is the only dependency-ready anonymity plan at registration. On closure, Plans 297, 298, and 300 become independently ready.
