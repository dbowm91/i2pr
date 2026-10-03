# Plan 287 status — secure base I2PControl JSON-RPC, authentication, and TLS server

Status: **`passed-prop170-secure-base-i2pcontrol-jsonrpc-auth-tls`**.

Plan of record: [`287-secure-base-i2pcontrol-jsonrpc-auth-tls.md`](../../implementation/i2pcontrol-proposal-170/287-secure-base-i2pcontrol-jsonrpc-auth-tls.md).

Hard dependency Plan 286 is closed (`passed-prop170-parallel-authority-provenance-and-contract-foundation`).

## Implementation commits

- `3b87662` — `plan(287): secure base I2PControl JSON-RPC auth and TLS server`
  (`[i2pcontrol]` config, `i2pcontrol.rs` service, graph registration,
  `i2pcontrol_base.rs` black-box suite, config tests, `dangerous()`
  boundary guard, architecture-doc updates, workspace TLS/JSON deps).

This closure commit (closure record + registry + roadmap) lands on top
of `3b87662` with no production-code change.

## Requirement-to-evidence matrix

| Plan 287 requirement | Evidence |
|---|---|
| Daemon owns listener/sockets/TLS/tokens/throttle/permits/deadlines/dispatch | `crates/i2pr-daemon/src/i2pcontrol.rs`: `I2pControlServiceState::run/bind/serve/handle_connection`; `i2pr-i2pcontrol` untouched except two root re-exports |
| `[i2pcontrol]` block, disabled + loopback default, no insecure password default | `config.rs`: `RawI2pControlConfig`/`I2pControlConfig`/`normalize_i2pcontrol`; default `127.0.0.1:7650`, `enabled = false`, empty password rejected when enabled |
| Managed loopback TLS; explicit pair required off-loopback; no fallbacks | `build_tls_config`: rcgen ephemeral self-signed (`localhost`, `127.0.0.1`, `::1`) for loopback; PEM pair required otherwise; half-config and non-loopback-without-pair fail in `normalize_i2pcontrol` + construction |
| API-1 `Authenticate` + six errors exact | `process_authenticate`: -32001..-32006; `all_six_auth_errors_are_exact` incl. expired→unknown transition at the deterministic instant |
| Donor bounded behavior (32 B, 1 day, 1024 FIFO, 256 cap, ct-compare, source-IP throttle, bounded delay, memory-only) | `mint_token`, `TokenTable`, `ct_password_eq` (fixed 1024-step), `AuthThrottle` (source-IP key, 1024 fixed, ≤ 2000 ms delay), restart test proves memory-only |
| JSON-RPC 2.0 exact; ids; notifications; named params; header compat + agreement | `dispatch_body`/`process_element`/`check_token` over the Plan 286 decoders; `X-I2PControl-Token` accepted, disagreement → -32602 |
| Single + non-empty batches; empty→one invalid; per-element isolation; all-notification→no body; order; no sibling propagation | `mixed_batches_preserve_order_and_isolate_invalid`, `batch_authenticate_does_not_propagate_to_siblings`, `notification_suppresses_response_but_executes`; over-ceiling batch → single -32600 |
| Initial ceilings (1 MiB, 32, 64, bounded connections, one permit, per-request deadline, no fanout) | `MAX_HTTP_BODY_BYTES` honored via `max_body_bytes ≤ 1 MiB`; `MAX_BATCH_ELEMENTS`; 64-permit semaphore held across sequential batch; `max_connections ≤ 256`; `request_deadline` on TLS/read/write; sequential loop, no spawn per element |
| Dispatch floor (Authenticate executes; known→typed capability error; unknown→method-not-found; no fabrication) | `protected_known_method_reaches_typed_dispatch` (per-method Plan markers, -32603; `GetRate` → -32601) |
| Supervised lifecycle; shutdown cancels under deadlines; permits release; disabled allocates nothing; real-socket source IP | `register_i2pcontrol_service` (Optional, 1 s start bound); cancel→`ParentScope`; `graceful_and_forced_shutdown_release_resources` (64/64 permits); `disabled_config_allocates_nothing`; `peer.ip()` only, forwarding headers never read |
| No plaintext listener | No non-TLS path exists; every connection goes through `TlsAcceptor` |

## Dependency review (new workspace deps)

| Crate | Version | Purpose | Maintainer/health | Transitive impact | `unsafe` | Features | License | Untrusted input |
|---|---|---|---|---|---|---|---|---|
| `serde_json` | 1.0 | JSON-RPC envelope parse/emit | serde-rs, maintained | `itoa`, `ryu`, `serde` only | none in enabled set | `default-features = false`, `std` | MIT/Apache-2.0 | yes — only through bounded decoders + daemon body ceiling |
| `rustls` | 0.23 | TLS 1.2+ server | rustls team, maintained | `ring`, `rustls-pki-types`, etc. | via `ring` asm (reviewed, standard) | `std`, `tls12`, `ring` (no client auth, no AWS-LC) | MIT/Apache-2.0/ISC | yes — handshake + records, memory-safe |
| `tokio-rustls` | 0.26 | Tokio↔rustls bridge | maintained | `rustls`, `tokio` | none | `tls12` | MIT/Apache-2.0 | same as rustls |
| `rustls-pki-types` | 1.12 | PEM/DER certificate + key types + `PemObject` parsing | rustls team, maintained | minimal | none | `std` | MIT/Apache-2.0 | yes — explicit PEM files only, bounded reads |
| `rcgen` | 0.13 | Managed loopback self-signed generation | maintained | `ring`, `pem`, `x509-parser` | via `ring` | `crypto`, `pem`, `ring` | MIT/Apache-2.0 | no — generates, never parses peer input |
| `subtle` | 2.6 (already centralized) | constant-time password compare | RustCrypto, maintained | none | none | `default-features = false` | BSD-3-Clause | compares secrets, never logs |

`rustls-pemfile` was adopted first, then removed before closure: it
carries RUSTSEC-2025-0134 (unmaintained) and `cargo deny` failed. The
PEM code moved to `rustls-pki-types::pem::PemObject` with no behavior
change; `cargo deny check advisories bans sources` is green. Workspace
versions stay centralized with narrow default features throughout.

## Tests and guards run (local; macOS host)

```text
cargo fmt --all --check                                                    OK
cargo check --locked --workspace --all-targets                             OK
cargo test --locked --workspace --all-targets -- --test-threads=1          3273 passed, 0 failed
  (delta over Plan 286 floor: +16 i2pcontrol unit, +10 i2pcontrol_base wire, +4 i2pcontrol config)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps       OK
cargo test --locked --workspace --doc                                      0 doc tests, 0 failed
cargo test --locked -p i2pr-daemon --lib i2pcontrol                       20 passed
cargo test --locked -p i2pr-daemon --test i2pcontrol_base                 10 passed (0.22 s)
cargo test --locked -p i2pr-daemon --lib config                            69 passed
bash scripts/check-dependency-direction.sh                                 dependency direction: ok
bash scripts/check-runtime-boundaries.sh                                   runtime boundary checks passed (incl. new dangerous-TLS guard)
bash scripts/check-service-tunnel-boundaries.sh                            service-tunnel boundary checks passed
bash scripts/check-ntcp2-interoperability.sh                               OK
bash scripts/check-constrained-host-lane-boundary.sh                      OK
bash scripts/check-m11-transit-boundaries.sh                               passed
bash scripts/check-m11-transit-qualification-evidence.sh                   175 guarded rows green
bash scripts/check-sam-acceptance-evidence.sh                              22 rows green
bash scripts/check-ssu2-acceptance-evidence.sh                             15 rows green
bash scripts/check-i2cp-acceptance-evidence.sh                             24 rows green
bash scripts/check-exploratory-tunnel-evidence.sh                          12 labels green
bash scripts/check-netdb-tunnel-evidence.sh                                12 labels green
bash scripts/check-destination-tunnel-evidence.sh                          21 labels green
bash scripts/check-m6-mixed-router-acceptance-evidence.sh                  11 labels green
python3 -m unittest discover -s tests/integration/ntcp2/harness            18 tests OK
cargo deny check advisories bans sources                                   advisories ok, bans ok, sources ok
```

Not run locally (pre-existing macOS environment limit, identical on
unmodified `main`): `check-fixture-manifest.sh`,
`check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`,
`check-i2cp-vectors.sh`, `check-service-tunnel-acceptance-evidence.sh`,
`check-streaming-tunnel-evidence.sh` require bash 4+ associative arrays
(`declare -A`); this host ships bash 3.2.57 only. None of their areas
are touched by this plan; Linux CI is authoritative for those six.

## Acceptance criteria (independent test client)

An independent client (the black-box suite acts as one: fresh TLS per
request, test-only verifier, no shared state with the service) proves:

1. loopback TLS establishes (`tls_connect` handshake green);
2. `Authenticate` with API 1 succeeds;
3. an opaque 64-hex-char token is received;
4. a protected known-method request (`RouterInfo` + token) reaches typed
   dispatch (`-32603` with the Plan 288 marker);
5. exact JSON-RPC/authentication failure behavior is observed
   (-32700/-32600/-32601/-32602/-32603 plus all six -32001..-32006);
6. after restart the old token is invalid (`-32003`).

Routine workspace checks and the focused adversarial server tests
(16 unit + 10 wire + 4 config) pass.

## Defects found during implementation (all corrected)

- `read_http_request` consumed already-buffered body bytes with the head
  and then waited for them again (5 s stall → client EOF). Fixed by
  splitting buffered body from the head and reading only the remainder;
  regression-covered by every wire test.
- Server dropped TCP without `close_notify` (client `UnexpectedEof`).
  Fixed with graceful `shutdown()` after every response; regression-
  covered by every wire test.
- `rustls-pemfile` carries RUSTSEC-2025-0134 → replaced by
  `rustls-pki-types::pem::PemObject`; deny green.

## Migration / compatibility

Additive only: new `[i2pcontrol]` block (absent = disabled defaults),
new service module, new workspace deps. No existing config, wire, or
storage surface changes. `Cargo.lock` gains the rustls/rcgen/serde_json
subtrees; `rustls-pemfile` was removed again before closure.

## Security review

- Passwords/tokens never logged: `I2pControlPassword` + `ServicePassword`
  redact `Debug`; state implements no `Debug`; responses never echo the
  password; error variants carry no secrets (redaction tests green).
- Constant-time bounded password comparison (`subtle`, fixed 1024 steps).
- Tokens: 32 OS-random bytes, hex-opaque, memory-only, 1-day monotonic
  lifetime, 1024 FIFO, 256-byte presented cap, expiry→unknown transition.
- Throttle: source-IP (never port) keying, 1024 fixed entries, 60 s
  window, ≤ 2 s bounded delay; 24-thread atomicity proven, 8-client wire
  atomicity proven.
- TLS: loopback managed cert covers exactly `localhost`/`127.0.0.1`/`::1`;
  non-loopback/wildcard without a full explicit pair fails before bind;
  half-configured and unparsable material fail before bind or side
  effects; no plaintext path exists; test-only `dangerous()` is confined
  to `tests/` and rejected in `src/` by the boundary script.
- Every body/batch/string/collection/token/throttle/connection/request
  has an explicit ceiling; max/max+1 tests cover each.

## Documentation / operational evidence

- `docs/architecture/i2pr-daemon.md`: `i2pcontrol.rs` module row +
  `I2pControlConfig`/`I2pControlPassword` config row.
- `docs/architecture/dependency-graph.md`: `i2pr-i2pcontrol` consumer +
  new external deps with review pointer.
- Crate rustdoc on the service module (ownership, contract, ceilings);
  `cargo doc` warning-free.

## Known limitations

- Dispatch floor only: `RouterInfo`/`AddressBook`/`TunnelManager`/
  `ClientServicesInfo` authenticate, then answer typed not-yet-available
  (`-32603` with the owning-plan marker). Substantive behavior belongs
  to Plans 288/289/294. No Proposal 170 capability is claimed.
- One request per connection (`Connection: close`); no HTTP keep-alive
  (bounded-simplicity choice, documented in-module).
- The six bash-4-only checkers are unverifiable on this macOS host
  (environment debt, not a plan defect); CI covers them.

## Findings by severity

- Critical: none. High: none. Medium: none. Low: the six bash-4-only
  checkers (environment debt, unchanged from Plan 286 closure).

## Roadmap disposition

Plan 287 is **closed** (`passed-*`). Plans 288, 289, and 294 become
dependency-ready in parallel; see the unblock audit below. No other
Proposal 170 capability may be claimed yet.

## Unblock audit (required)

Audited `plans/registry.md` blocked work + the Proposal 170 roadmap §6
graph against the just-closed milestone:

- Plan 288 (hard dep: Plan 287) — all hard deps now closed → moves to
  `ready` in this commit.
- Plan 289 (hard dep: Plan 287; M10 authority reused, closed) — all hard
  deps now closed → moves to `ready` in this commit.
- Plan 294 (hard dep: Plan 287) — all hard deps now closed → moves to
  `ready` in this commit. May execute in parallel with Plans 288/289.
- Plan 290 (hard dep: Plan 289, still open) — remains blocked.
- Plan 291 (hard dep: Plan 289, still open) — remains blocked.
- Plan 292 (hard deps: Plans 290 + 291) — remains blocked.
- Plan 293 (hard dep: Plan 292) — remains blocked.
- Plan 295 (hard deps: Plans 288 + 293 + 294) — remains blocked.
- M12/mainline plans — unaffected (parallel workstream per ADR 0028).

No corrective pass is required: every Plan 287 acceptance criterion
passes with executed evidence above. Plans 288, 289, 294 are eligible
but out of scope for this handoff, which was bounded to Plans 286–287.
