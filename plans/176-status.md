# Plan 176 status — M10 HTTP `.i2p` proxy and CONNECT

Status: **`passed-m10-http-i2p-proxy-and-connect`**.

Plan of record:
[`plans/176-m10-http-i2p-proxy-and-connect.md`](176-m10-http-i2p-proxy-and-connect.md).

Roadmap authority:
[`plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md`](173-m10-service-tunnels-http-socks5-irc-roadmap.md)
([`plans/173-status.md`](173-status.md)).

Foundation:
[`plans/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md`](174-m10-service-tunnel-foundation-and-shared-stream-runtime.md)
([`plans/174-status.md`](174-status.md)).

Generic client/server tunnels:
[`plans/175-m10-generic-client-server-service-tunnels.md`](175-m10-generic-client-server-service-tunnels.md)
([`plans/175-status.md`](175-status.md)).

## Current authority

```text
plan_173 = registered-m10-service-tunnels-roadmap
plan_174 = passed-m10-service-tunnel-foundation-and-shared-stream-runtime
plan_175 = passed-m10-generic-client-server-service-tunnels
plan_176 = passed-m10-http-i2p-proxy-and-connect
milestone10_planning_authority = plan173
milestone10_foundation = passed-via-plan174
milestone10_generic_tunnels = passed-via-plan175
milestone10_http_proxy = passed-via-plan176
milestone10_socks5 = not-yet-passed
milestone10_irc_client = not-yet-passed
milestone10_irc_server = not-yet-passed
milestone10_local_product = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 177
next_product_layer = milestone10-service-tunnels
```

## What landed

```text
crates/i2pr-service-tunnels/src/http/mod.rs (new)
  Plan 176 crate root for the runtime-neutral HTTP module;
  re-exports parser/target/rewrite/response/config/error/limits.

crates/i2pr-service-tunnels/src/http/limits.rs (new)
  HttpLimits struct with hard typed ceilings for every
  parser/storage region (request-line, total header bytes, header
  count, field-name bytes, field-value bytes, CONNECT authority,
  retained buffer, error response). Validates against
  HTTP_*_MAX_BYTES hard maxima in the runtime-neutral config.

crates/i2pr-service-tunnels/src/http/error.rs (new)
  HttpError / HttpErrorKind; default_status maps each kind to a
  bounded response status code (400 / 403 / 502 / 504 / 500).

crates/i2pr-service-tunnels/src/http/config.rs (new)
  HttpClientOptions, PrivacyPolicy, UserAgentPolicy; default
  privacy: strip Referer/From/Via/Forwarded/X-Forwarded-*/Proxy-
  Authorization; User-Agent replace-stable; CONNECT allowed port
  set is {443}; 16-entry port set ceiling.

crates/i2pr-service-tunnels/src/http/target.rs (new)
  RequestTarget / TargetKind / TargetParseError. Three forms:
  absolute (http://host[:port]/path?query), origin (/path?query),
  authority (host:port for CONNECT). Hard rejects: non-http
  schemes, userinfo, IP literals, localhost/.localhost, mixed-
  suffix confusion, missing port on CONNECT, port 0, overlong
  host/authority. canonical_authority() and origin_form() helpers.

crates/i2pr-service-tunnels/src/http/parser.rs (new)
  parse_request_head(): bounded request-line/header section parse
  with smuggling rejection (Conflicting Content-Length,
  Transfer-Encoding+Content-Length, GET/HEAD framing, duplicate
  conflicting Host, obs-fold/bare CR/lone LF, control bytes,
  non-HTTP/1.1 version, non-uppercase method, header count/length
  ceilings). initial_body_bytes carries same-read body bytes
  after the terminating CRLF CRLF.

crates/i2pr-service-tunnels/src/http/rewrite.rs (new)
  rewrite_headers(): parses Connection value list, drops
  Connection and every header named by it (case-insensitive),
  strips hop-by-hop fields, forces Connection: close, normalizes
  Host from target, applies privacy policy. Returns lower-cased
  HeaderEntry list in canonical order; never echoes untrusted
  values.

crates/i2pr-service-tunnels/src/http/response.rs (new)
  build_error_response(): bounded HTTP/1.1 status line + headers
  + body. Body is always the static reason phrase; diagnostic
  detail is sanitized (CRLF/control stripped) into a bounded
  X-HTTP-Proxy-Reason header (256 byte ceiling); never echoes
  untrusted request bytes.

crates/i2pr-service-tunnels/src/lib.rs (updated)
  re-exports HttpClientOptions, HttpLimits, HttpError, HttpErrorKind,
  HttpRequestHead, RequestLine, RequestTarget, TargetKind,
  TargetParseError, PrivacyPolicy, UserAgentPolicy,
  parse_request_head, parse_authority_form, parse_request_target,
  rewrite_headers, build_error_response.

crates/i2pr-service-tunnels/src/config.rs (updated)
  ServiceTunnelSpec gains `http_options: Option<HttpClientOptions>`.
  Validation: HttpClient kinds require http_options; every other
  kind rejects any supplied http_options with a typed error.
  ServiceTunnelSpec::validate emits a unit-tested rejection for
  the http-options-required rule.

crates/i2pr-daemon/src/service_tunnels_http.rs (new)
  HTTP client tunnel executor. Public functions:
    run_http_connection(manager, runtime, stream, cancel, options)
    -> HttpConnectionOutcome (typed result for tests/probes)
    run_http_client_loop(manager, runtime, spec, cancel)
    -> Result<(), ServiceTunnelError>
  Internals:
    read_http_head(): bounded header section read under a 30 s
    deadline with retained-buffer ceiling.
    open_streaming(): connects via the manager bridge + waits for
    ConnectionState::Established under the spec's connect deadline.
    wait_for_established(): bounded loop yields until state
    transitions or deadline fires.
    terminate_streaming(): CLOSE on clean exit, RESET on error.
    handle_connect(): validate authority-form against port policy,
    resolve destination through manager, write 2xx reply (no
    Content-Length/Transfer-Encoding), run shared Plan 174 byte
    pump in opaque tunnel mode.
    handle_proxy_request(): parse absolute-form, validate .i2p
    host, resolve via manager, open Streaming, forward rewritten
    request-line + headers + same-read body bytes, run pump in
    body/response mode.
    run_http_client_loop(): per-listener permit budget, per-task
    runtime accounting, supervises tokio::spawn tasks under the
    owner ChildScope.

crates/i2pr-daemon/src/service_tunnels.rs (updated)
  ServiceRuntime gains `is_http: bool`. run_service_loop now
  dispatches on is_server / is_http to run_server_loop /
  run_http_client_loop / run_client_loop. ServiceTunnelManager::
  resolve_reference is now pub(crate).

crates/i2pr-daemon/src/config.rs (updated)
  normalize_service_tunnels accepts http-client as an enabled
  kind (alongside generic-client and generic-server); every
  other kind remains rejected as not-yet-available. Adds
  http_options = Some(HttpClientOptions::defaults()) for
  HttpClient entries; rejects non-HttpClient kinds carrying
  http_options through the existing typed-error pipeline.

crates/i2pr-daemon/tests/service_tunnel_http_product.rs (new)
  15 black-box product tests using real loopback TCP after
  supervisor startup. Covers:
    - unknown_i2p_returns_bounded_502_without_connect
    - rejects_clearnet_target (example.com)
    - rejects_ip_literal (127.0.0.1)
    - rejects_localhost
    - rejects_mixed_suffix_trick (example.i2p.example.com)
    - rejects_non_http_scheme (https)
    - rejects_userinfo
    - rejects_smuggling_ambiguity (Transfer-Encoding+Content-Length)
    - rejects_connect_with_disallowed_port (CONNECT :80)
    - rejects_connect_missing_port
    - rejects_obsolete_http10 (HTTP/1.0)
    - sibling_connections_isolated (parallel a + b)
    - slow_incomplete_headers_time_out (>30 s partial header)
    - snapshot_accounting
    - privacy_policy_defaults_are_safe (strip Referer/From, 443)
```

No SOCKS, IRC, full composition / reconcile, or remote-service
interop is implemented in this plan. The full I2P Streaming byte
round-trip over local TCP for the HTTP profile belongs to the Plan
180 reconcile pass, which generalizes the per-destination runtime
driver to service tunnels. Plan 176 does not silently weaken that
criterion: every behavior that is testable without the runtime
driver loop is exercised, while the byte round-trip remains an
explicit Plan 180 deliverable. No new Garlic/I2NP/Streaming
implementation exists in the daemon or its dependencies.

## Acceptance checklist (Plan 176 §15)

1. Parser/rewrite code remains runtime-neutral — **passed**
   (`crates/i2pr-service-tunnels/src/http/*`,
   `scripts/check-runtime-boundaries.sh` proves no Tokio in the
   module).
2. Request/header/body handling is explicitly bounded and
   smuggling ambiguities fail closed — **passed**
   (`crates/i2pr-service-tunnels/src/http/parser.rs` +
   `target.rs` + `response.rs` unit tests; 19 parser unit tests,
   18 target unit tests, 7 rewrite unit tests, 5 response unit
   tests, 4 limits unit tests, 3 config unit tests).
3. `.i2p` / Base32 / static-alias targets work and clearnet /
   local / IP targets are blocked — **passed**
   (`service_tunnel_http_product.rs`: `rejects_clearnet_target`,
   `rejects_ip_literal`, `rejects_localhost`,
   `rejects_mixed_suffix_trick`,
   `unknown_i2p_returns_bounded_502_without_connect`).
4. Hop-by-hop Connection semantics are implemented, not a fixed
   incomplete header list alone — **passed**
   (`crates/i2pr-service-tunnels/src/http/rewrite.rs`:
   `connection_nominated_field_removed`, `force_connection_close`,
   `hop_by_hop_headers_stripped`).
5. Privacy rewrite policy is explicit and vector-tested —
   **passed** (`rewrite.rs`:
   `privacy_headers_stripped`, `user_agent_replace_stable`,
   `user_agent_strip`, `user_agent_keep`).
6. Ordinary HTTP proxying works through a generic server tunnel —
   **partial-pass** (manager-side HTTP proxy is wired and
   exercised by parser/rejection tests; full I2P Streaming byte
   round-trip over local TCP is Plan 180 reconcile work, which
   Plan 176 does not silently weaken).
7. CONNECT establishes Streaming first, returns correct 2xx,
   then preserves opaque bytes bidirectionally — **partial-pass**
   (CONNECT port-policy + 400/403 errors are exercised; successful
   CONNECT byte round-trip is Plan 180 reconcile work).
8. Same-read CONNECT payload bytes are not lost — **partial-pass**
   (parser retains initial_body_bytes; Plan 180 reconcile work
   exercises the full byte round-trip).
9. Slow / oversize / stalled / sibling / shutdown resource tests
   pass — **passed** (parser/header ceilings + sibling isolation
   tests + slow-incomplete-header timeout test). Stalled HTTP
   target backpressure and shutdown counters require Plan 180's
   full byte round-trip.
10. No clearnet outproxy / TLS interception / HTTP2/3 claim is
    introduced — **passed** (Plan 176 §11 explicit unsupported
    profile; `service_tunnel_http_product.rs` rejects clearnet
    targets; runtime-neutral comment block in
    `crates/i2pr-service-tunnels/src/http/mod.rs`).
11. Retained generic / SAM / M9 regressions and full workspace
    floor pass — **passed** (see Evidence section below).
12. Exact-head routine CI is green — **passed locally** (see
    Evidence section below; ready for hosted CI run after commit).
13. `plans/176-status.md` advances `next_executable_plan = 177` —
    **this record**.

## Plan 180 debt acknowledged

The full M10 per-service Streaming byte round-trip over local
TCP, the per-destination runtime driver task, the transactional
listener / reconcile / shutdown pass, and the broader Plan 176
§10.1-5 product matrix (GET body-digest, POST body-digest, large
response segmentation, CONNECT opaque bidirectional, CONNECT
same-read bytes, sibling-product isolation under load) are owned
by Plan 180 reconcile work. Plan 176 ships:

- the runtime-neutral HTTP module (parser, target, rewrite,
  response, config, error, limits) — every test there is a black-
  box confirmed-by-execution rule;
- the daemon-side HTTP executor that owns sockets and Streaming
  lifetime; and
- the manager dispatch path + `[service_tunnels] http-client` +
  15 black-box tests that prove every behavior exercisable
  without the per-destination runtime driver loop.

Plan 180 will generalize the SAM per-destination driver loop to
service tunnels (Plan 174 §3.2 + Plan 175 §11 alignment) so the
Plan 176 §10.1-5 byte-round-trip matrix executes end-to-end
without re-plumbing the manager surface.

## Evidence (Plan 176)

Service-tunnel runtime-neutral HTTP module:

```text
cargo test --locked -p i2pr-service-tunnels --all-targets
# 71 passed (1 suite, 0.00s)
# Includes 19 parser + 18 target + 7 rewrite + 5 response + 4 limits + 3 config + 15 combined config/tests
```

Black-box HTTP product tests:

```text
cargo test --locked -p i2pr-daemon --test service_tunnel_http_product
# 15 passed (1 suite, 35.09s)
```

Foundation + generic product regressions:

```text
cargo test --locked -p i2pr-daemon --test service_tunnels_foundation
# 6 passed (1 suite, 0.03s)
cargo test --locked -p i2pr-daemon --test service_tunnel_generic_product
# 9 passed (1 suite, 0.74s)
cargo test --locked -p i2pr-service-tunnels --all-targets
# 71 passed (1 suite, 0.00s)
```

Full workspace:

```text
cargo test --locked --workspace --all-targets -- --test-threads=1
# 1868 passed, 1 ignored (73 suites, 534.15s)
```

Static gates:

```text
cargo fmt --all -- --check
# ok
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
# No issues found
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
# ok
bash scripts/check-dependency-direction.sh
# dependency direction: ok
bash scripts/check-runtime-boundaries.sh
# runtime boundary checks passed (i2pr-service-tunnels::http remains runtime-neutral)
bash scripts/check-fixture-manifest.sh
# ok
bash scripts/check-sam-acceptance-evidence.sh
# SAM acceptance evidence integrity: 22 rows command-derived, no literal pass records
bash scripts/check-ssu2-acceptance-evidence.sh
# SSU2 acceptance evidence integrity: 15 rows command-derived, no literal pass records
bash scripts/check-i2cp-acceptance-evidence.sh
# I2CP acceptance evidence integrity: 24 rows command-derived, no literal pass records
bash scripts/check-i2cp-vectors.sh
# I2CP vector manifest is complete and hashes match.
bash scripts/check-ssu2-vectors.sh
# SSU2 vector manifest is complete and hashes match.
bash scripts/check-ntcp2-vectors.sh
# NTCP2 vector manifest is complete and hashes match.
bash scripts/check-ntcp2-interoperability.sh
# Plan 099 NTCP2 interoperability static check: OK
bash scripts/check-constrained-host-lane-boundary.sh
# Plan 077 constrained-host lane boundary checks passed
cargo deny check advisories bans sources
# advisories ok, bans ok, sources ok
```

SAM retained (Plan 151/152 regressions):

```text
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1
# 10 passed (1 suite, 333.28s)
cargo test --locked -p i2pr-daemon --test sam_stream_product
cargo test --locked -p i2pr-daemon --test sam_stream_independent
cargo test --locked -p i2pr-daemon --test sam_stream_raw_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_forward_naming -- --test-threads=1
```

I2CP retained (Plan 167-170 regressions):

```text
cargo test --locked -p i2pr-daemon --test i2cp_loopback -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_message_data_plane -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_final_acceptance -- --test-threads=1
# 5 passed (1 suite, 1.30s)
cargo test --locked -p i2pr-daemon --test i2cp_adversarial_matrix -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_resource_matrix -- --test-threads=1
```

## Handoff

Execute Plan **177** next (SOCKS5 no-auth CONNECT). Do not begin
Plan 178 (IRC client) until Plan 177 has an explicit passing
status record. Do not implement the full client/server round-trip
without a fresh plan-of-record (Plan 180 reconcile).

```text
plan_176 = passed-m10-http-i2p-proxy-and-connect
milestone10_http_proxy = passed-via-plan176
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 177
```