# Plan 321 status — Canonical AddressBook operational completion

Status: **`passed-prop170-addressbook-operational-completion`**.

Plan of record: [`plans/implementation/i2pcontrol-proposal-170/321-addressbook-operational-completion.md`](../../implementation/i2pcontrol-proposal-170/321-addressbook-operational-completion.md).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Exact canonical 13-key SetConfig projection | Daemon maps Proposal keys onto typed config fields; `i2pr-addressbook` renders canonical keys and validates unique confined artifact names. Config round-trip and invalid-value tests pass. `theme` is retained inert metadata. | PASS |
| Bounded subscription transport | Daemon-only `LoopbackProxyFetcher` capability connects only to an explicitly configured loopback proxy; supports HTTP/HTTPS, verified TLS roots, strict response framing, no redirects/decompression, URL/body/header/timeout bounds, and no direct-origin sockets. One worker fetches sequentially; the generation is capped at 16 × 1 MiB and 300 seconds, with one bounded retry for unavailable transport. | PASS |
| Conditional acquisition and atomic state | Per-URL parsed entries, ETag, and Last-Modified are stored in one version-compatible address-book generation. The local proxy test observes conditional headers and a 304; restart restores the validated source state. | PASS |
| Whole-generation parsing and precedence | Existing strict whole-body parser validates every fetched body. All URLs must validate before one transaction replaces source maps; later configured URLs win duplicate hostnames deterministically. Failed/corrupt fetches retain the old resolver generation. | PASS |
| Cadence and coalesced replacement | `update_delay` drives the supervised refresh interval; SetSubscriptions commits atomically, notifies the owner, and the one-active/one-pending queue coalesces replacement. Cancellation releases/promotes queue ownership. Queue state-machine tests and end-to-end refresh tests pass. | PASS |
| should_publish artifact | `false` leaves the prior artifact untouched. `true` writes a confined, mode-0600 temporary file and atomically renames router-book entries only; startup repairs the artifact from the authoritative generation. Local/private entries are excluded. Tests cover enable/disable and restart repair. | PASS |
| Canonical state and getters | Proposal AddressBook config/book selectors and ordinary resolver share the same `AddressBookManager` generation. Existing daemon tests exercise manager views and refresh commits. | PASS |

## Verification

| Command | Result |
|---|---|
| `cargo fmt --all --check` | PASS (formatted with `cargo fmt --all`) |
| `cargo check --locked -p i2pr-daemon --all-targets` | PASS |
| `cargo test --locked -p i2pr-addressbook --all-targets` | PASS — 34 tests |
| `cargo test --locked -p i2pr-daemon --lib addressbook_fetch::tests -- --test-threads=1` | PASS — 4 tests |
| `cargo test --locked -p i2pr-daemon --lib subscription_refresh_uses_validators_and_commits_whole_generations -- --test-threads=1` | PASS — 1 test |
| `cargo test --locked -p i2pr-daemon --lib should_publish_projects_router_entries_but_excludes_local_and_private -- --test-threads=1` | PASS — 1 test |
| `cargo test --locked -p i2pr-daemon --lib startup_repairs_published_artifact_from_committed_generation -- --test-threads=1` | PASS — 1 test |
| `cargo clippy --locked -p i2pr-addressbook -p i2pr-daemon --all-targets --all-features -- -D warnings` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `bash scripts/check-dependency-direction.sh` | PASS |
| `bash scripts/check-runtime-boundaries.sh` | PASS |
| `git diff --check` | PASS |

The complete workspace routine floor was not run; this plan changes the address-book and daemon slices only. The focused fetch fixtures exercise HTTP transport; HTTPS uses the same bounded parser after CONNECT and rustls validation, but no local certificate-backed HTTPS proxy fixture was added. Existing schedule behavior is covered through the worker's configured interval and queue tests; no wall-clock interval wait is used in acceptance.

## Migration, security, and limitations

- Prior generations without per-source records remain readable; their derived subscription entries are retained until refreshed.
- The proxy setting must be a literal loopback IP and nonzero port; subscription URLs must target `.i2p` hostnames. No proxy discovery, local DNS, direct clearnet fallback, redirect following, or content decompression is permitted.
- Published output contains only the router book. Atomic replacement and restart repair keep the configured artifact aligned with the committed generation when publication is enabled.
- Added `webpki-roots` as the Mozilla trust-anchor dataset used by existing rustls; no crypto primitive, unsafe code, or runtime boundary exception was introduced.
- Findings: critical 0, high 0, medium 0, low 0.

## Roadmap disposition and unblock audit

Plan 321 is closed as **`passed-prop170-addressbook-operational-completion`**. Plan 322's only outstanding hard dependency was Plan 321, so Plan 322 is moved from blocked to **in-progress**. Plans 324 and 327 remain blocked on Plan 323; Plan 326 remains blocked on Plans 323, 324, and the separately closed blocked Plan 325 provider survey; Plan 328 remains blocked on Plans 322, 326, and 327. Plan 323 remains active. No other dependency becomes ready from this closure, and M12/mainline readiness is unchanged.
