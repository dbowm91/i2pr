# Plan 307 status — service-boundary router unlinkability and input sanitation

Status: `passed-service-boundary-router-unlinkability-and-input-sanitation`

Implementation commits: `06ee4bf` (`fix(privacy): close service boundary input leaks (Plan 307)`) and `c4a55c3` (`test(privacy): cover equal duplicate Host rejection (Plan 307)`).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Inventory every current application-facing service direction | `specs/service-boundary-matrix.toml` covers HTTP client, HTTP CONNECT, SOCKS5 client, generic client/server, and IRC client/server. The checker requires the complete seven-row set and all input/synthesis/pass-through/forbidden fields. | Passed |
| Reject unsafe HTTP authority and header ambiguity | HTTP parsing now rejects every duplicate `Host`, including identical duplicates. Existing parser tests cover CRLF/control injection, framing ambiguity, and malformed field names/values. | Passed |
| Remove proxy and forwarding metadata from rewritten HTTP | Rewrite strips `Via`, `Forwarded`, all `X-Forwarded-*`, `X-Real-IP`, `X-Client-IP`, `Proxy-Authorization`, and `Proxy-Authenticate`; unit cases cover representative members and the full prefix rule. | Passed |
| Keep SOCKS5 replies fixed and opaque streams unmodified | Existing SOCKS5 protocol-byte and generic stream boundary coverage; `check-service-tunnel-boundaries.sh`. | Passed |
| Keep IRC rewrites bounded and independent of router-local state | Existing IRC parser/filter/server tests and source boundary scan; authenticated peer-Destination projection remains distinct from router identity. | Passed |
| Bound remotely visible errors and router-local leak markers | HTTP response generation uses closed status/reason categories; SOCKS5 has fixed replies; daemon IRC outcomes expose closed local categories. The seeded boundary checker exercises product/build identity, RouterHash/RouterInfo, transport address, host/IP, path, alias, raw-error, and proxy-agent markers. | Passed |
| Preserve resource and architecture boundaries | No new owner, dependency, wire format, storage format, listener, or persistent state. Runtime and dependency-direction guards remain green. | Passed |

## Commands and outcomes (local)

- `cargo fmt --all --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked -p i2pr-service-tunnels --all-targets -- --test-threads=1` — 222 passed on the production implementation; rerun after the test-only follow-up commit `c4a55c3` — 223 passed.
- `cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1` — 1,345 passed, 33 ignored, 60 suites.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` — 3,271 passed, 34 ignored, 110 suites.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — passed with no issues.
- `RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps` — passed.
- `cargo test --locked --workspace --doc` — passed, 16 suites.
- `bash scripts/check-service-anonymity-boundaries.sh` — passed, including matrix completeness and seeded-negative checks.
- `bash scripts/check-service-tunnel-boundaries.sh` — passed.
- `bash scripts/check-runtime-boundaries.sh` — passed.
- `bash scripts/check-dependency-direction.sh` — passed.
- `git diff --check` — passed before the implementation commit.

## Compatibility, security, and limitations

Repeated `Host` request fields are now rejected even when values match. HTTP requests drop the full `X-Forwarded-*` family and proxy authentication metadata before forwarding. Valid ordinary requests and opaque body/stream payloads retain their existing behavior. No configuration migration is required. This evidence establishes the registered service-boundary invariant only; it makes no general anonymity claim and does not qualify HTTP fingerprints, Destination-group isolation, tunnel-path length, or Streaming behavior.

| Severity | Finding |
|---|---|
| Critical | None. |
| High | None. |
| Medium | Plans 308–313 still contain separate HTTP-profile, group-ownership, multi-hop, lifecycle, and Streaming qualification requirements. |
| Low | None. |

## Unblock audit and roadmap disposition

- **Plan 308:** ready. Plan 307 is closed; retained Plan 304 pin/topology/corpus artifacts and ADR 0030 satisfy its other prerequisites. It can proceed independently of Plan 309.
- **Plan 309:** ready. Plan 307 is closed; retained Plan 305 source/owner audit and ADR 0030 satisfy its other prerequisites. It can proceed independently of Plan 308.
- **Plan 310:** remains blocked on Plan 309. No interface or group-owner contract was added by Plan 307.
- **Plans 311 and 312:** remain blocked on Plan 310.
- **Plan 313:** remains blocked on Plan 312.
- Plans 297–305 retain their historical stopped statuses. No M12/mainline status or security-model claim changes.

Disposition: passed. The next execution sequence is Plan 308 followed by Plan 309; the latter's result controls readiness for Plan 310.
