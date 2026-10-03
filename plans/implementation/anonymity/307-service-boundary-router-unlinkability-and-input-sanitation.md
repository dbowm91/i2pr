# Plan 307 — Service-boundary router unlinkability and input-sanitation corrective

Status at registration: **registered-anonymity-router-unlinkability-and-input-sanitation**

Classification: invariant + corrective capability.

Hard dependencies: Plan 296 passed; ADR 0030 accepted. No M12 dependency.

## 1. Objective

Audit and correct every current client/server service-tunnel application boundary so router identity or router-local state cannot cross into remote service traffic or backend-facing synthesized metadata, and hostile application input cannot smuggle or reflect such data through parser/rewrite paths.

This is the immediate low-risk anonymity pass. It does not add Destination groups, multi-hop pool ownership, lifecycle smoothing, or Streaming tuning.

## 2. Why ready

Plan 296 removed the known HTTP User-Agent, local-alias Host, CONNECT Proxy-Agent, and IRC stable-token leaks. The remaining question is systematic coverage, especially server directions and input sanitation. Existing M10 service families are closed and stable enough to audit without changing product scope.

## 3. Current implementation evidence

- HTTP client rewriting canonicalizes remote Host and uses the common `MYOB/6.66 (AN/ON)` User-Agent.
- Generic, SOCKS, and CONNECT are intended to be opaque after setup.
- IRC client/server profiles parse and rewrite application lines.
- Service manager diagnostics and local error responses are daemon-owned.
- Future HTTP server support is not present and remains out of production scope.

## 4. Invariants

1. No network-visible synthesized service bytes contain router hash, RouterInfo, transport address, i2pr release/build identity, host path/name/IP, or user-local alias.
2. Remote peer-controlled input cannot inject CR/LF, delimiters, proxy-added headers, or diagnostic text that causes a trusted rewrite to emit unintended metadata.
3. Remote client Destination identity, where a future policy intentionally exposes it to a local backend, is distinct from router identity and must never be substituted by router data.
4. Opaque payload paths remain opaque; no TLS MITM.
5. Existing byte/count/deadline ceilings remain hard.
6. Errors exposed remotely are bounded protocol categories, not raw internal error strings.

## 5. Scope

In scope: HTTP client parser/rewrite, CONNECT setup response, SOCKS5 negotiation/replies, generic forwarding wrappers, IRC client and server filters, service-side application error construction, backend-facing synthesized values, and boundary checker coverage.

Explicitly out: future HTTP server implementation, Destination grouping, tunnel path selection, Streaming constants, browser/TLS fingerprint normalization, and global timing analysis.

## 6. Required production changes

1. Create a machine-readable service-boundary matrix covering each current service kind and direction, identifying parsed peer input, synthesized fields, pass-through regions, and forbidden router-local sources.
2. Trace all emitted application bytes from daemon/service crates and remove any remaining router/version/build/local-state token.
3. Harden HTTP field parsing/rewrite against CRLF, invalid field-name/value bytes, duplicate smuggling around hop-by-hop/proxy fields, and spoofed Forwarded/Via/X-Forwarded/proxy-auth metadata.
4. Verify SOCKS5 replies are fixed protocol bytes and never include implementation strings or local target text.
5. Verify IRC server/client rewritten host/location/reason fields derive only from allowed constants or authenticated remote-Destination facts, never router facts; reject control/delimiter injection.
6. Verify generic paths add no framing/application metadata.
7. Bound every remotely visible error reason to a closed vocabulary.
8. Extend `scripts/check-service-anonymity-boundaries.sh` or replace it with a structured checker that validates the complete matrix and includes seeded-negative cases for every forbidden class.

## 7. Ordered work packages

WP1 inventory and boundary matrix; WP2 parser/sanitizer hardening; WP3 server-direction/router-source tracing; WP4 negative transcript fixtures; WP5 checker and full regression floor.

## 8. Failure, cancellation, restart, and contention

No new long-lived owner is introduced. Existing service deadlines/cancellation remain authoritative. Parser rejection occurs before forwarding or allocation beyond existing ceilings. Sanitizer ambiguity fails closed with a typed local category.

## 9. Compatibility and migration

Valid existing traffic remains compatible. Malformed/spoofed inputs may now be rejected or stripped. No configuration migration or persistent format change.

## 10. Required tests

Seeded router hash/version/build/hostname/IP/path/alias leak attempts; CRLF and header-name/value injection; duplicate proxy/hop-by-hop field smuggling; IRC delimiter/control-character cases; SOCKS fixed-reply byte assertions; generic byte-preservation assertions; remote-error closed-vocabulary checks; existing Plan 296 regressions.

## 11. Exact verification commands

~~~bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-service-tunnels --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-service-anonymity-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-dependency-direction.sh
~~~

## 12. Documentation updates

Update the service-boundary matrix and anonymity roadmap. Keep `docs/security-model.md` claims narrow: this plan proves absence of registered router-local service-boundary leaks, not application anonymity.

## 13. Acceptance criteria

All current service directions have a closed boundary matrix; every forbidden router-local marker is rejected by seeded tests; malformed input cannot smuggle synthesized metadata; no valid existing service capability regresses; full workspace/security floor passes.

## 14. Stop conditions

Stop on a finding that requires a new protocol family, TLS interception, a Destination-group architecture change, or a router-wide logging redesign. Register a new corrective rather than widening this plan.

## 15. Closure evidence required

Implementation commits, matrix, seeded-negative transcript evidence, exact command results, finding severity table, and unblock audit for Plans 308/309.

## 16. Handoff

On pass, Plans 308 and 309 become dependency-ready and may proceed independently.
