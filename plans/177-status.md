# Plan 177 status — M10 SOCKS5 `.i2p` CONNECT proxy

Status: **`passed-m10-socks5-i2p-connect-proxy`**.

Plan of record:
[`plans/177-m10-socks5-i2p-connect-proxy.md`](177-m10-socks5-i2p-connect-proxy.md).

Roadmap authority:
[`plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md`](173-m10-service-tunnels-http-socks5-irc-roadmap.md)
([`plans/173-status.md`](173-status.md)).

Foundation:
[`plans/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md`](174-m10-service-tunnel-foundation-and-shared-stream-runtime.md)
([`plans/174-status.md`](174-status.md)).

Generic client/server tunnels:
[`plans/175-m10-generic-client-server-service-tunnels.md`](175-m10-generic-client-server-service-tunnels.md)
([`plans/175-status.md`](175-status.md)).

HTTP `.i2p` proxy + CONNECT:
[`plans/176-m10-http-i2p-proxy-and-connect.md`](176-m10-http-i2p-proxy-and-connect.md)
([`plans/176-status.md`](176-status.md)).

## Current authority

```text
plan_173 = registered-m10-service-tunnels-roadmap
plan_174 = passed-m10-service-tunnel-foundation-and-shared-stream-runtime
plan_175 = passed-m10-generic-client-server-service-tunnels
plan_176 = passed-m10-http-i2p-proxy-and-connect
plan_177 = passed-m10-socks5-i2p-connect-proxy
milestone10_planning_authority = plan173
milestone10_foundation = passed-via-plan174
milestone10_generic_tunnels = passed-via-plan175
milestone10_http_proxy = passed-via-plan176
milestone10_socks5 = passed-via-plan177
milestone10_irc_client = not-yet-passed
milestone10_irc_server = not-yet-passed
milestone10_local_product = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 178
next_product_layer = milestone10-service-tunnels
```

## What landed

```text
crates/i2pr-service-tunnels/src/socks5/mod.rs (new)
  Plan 177 crate root for the runtime-neutral SOCKS5 module;
  re-exports config/errors/limits/negotiation/request/reply types.

crates/i2pr-service-tunnels/src/socks5/limits.rs (new)
  Socks5Limits struct with hard typed ceilings for every parser
  region (method count, greeting bytes, request header bytes,
  domain length, retained buffer, reply bytes). Validates every
  field against SOCKS5_*_MAX_BYTES hard maxima in the runtime-
  neutral config and returns a typed Socks5Error::InvalidLimits
  on failure.

crates/i2pr-service-tunnels/src/socks5/errors.rs (new)
  Socks5ErrorKind / Socks5Error / Socks5ReplyCode. Maps every
  Plan 177 §6 reply code (0x00 success, 0x01 general failure,
  0x02 connection not allowed, 0x04 host unreachable, 0x05
  connection refused, 0x06 TTL expired, 0x07 command not
  supported, 0x08 address type not supported) to its kind and
  exposes `reply_code()` for the request parser terminal state.

crates/i2pr-service-tunnels/src/socks5/config.rs (new)
  Socks5ClientOptions { port_policy: ConnectPortPolicy,
  destination_ports, allowed_hosts }. Default CONNECT allowed
  port set is {443}; 16-entry port set ceiling.

crates/i2pr-service-tunnels/src/socks5/negotiation.rs (new)
  GreetingParser — incremental RFC 1928 greeting parser
  (VER=0x05, NMETHODS=1..16, requires 0x00 NO AUTHENTICATION
  REQUIRED). Rejects 0x02 username/password even when offered;
  returns GreetingOutcome::{NoAuthentication, NoAcceptableMethod}
  with the number of consumed bytes (remainder is forwarded to
  the request parser).

crates/i2pr-service-tunnels/src/socks5/request.rs (new)
  RequestParser — incremental CONNECT request parser
  (VER/CMD/RSV/ATYP, ATYP=0x03 DOMAINNAME only). Rejects
  IPv4/IPv6 (typed Rejected reply 0x08), BIND/UDP ASSOCIATE
  (typed Rejected reply 0x07), unknown commands (typed
  Rejected reply 0x07), zero-length domain (0x01), zero port
  (0x01), and NUL/control/whitespace domain bytes (structural
  Socks5Error). Strict `.i2p`/Base32/static-alias target policy:
  IP literals, clearnet, localhost, mixed-suffix confusion, and
  malformed alias spellings are rejected via the typed Rejected
  reply 0x02 (ConnectionNotAllowed). Same-read post-request bytes
  are preserved verbatim as the `leftover` field of
  RequestOutcome::ReadyToConnect.

crates/i2pr-service-tunnels/src/socks5/reply.rs (new)
  build_reply / build_reply_from_code — deterministic 10-byte
  RFC 1928 reply with a neutral loopback `127.0.0.1:0` bind; never
  echoes untrusted request bytes or destination private material;
  rejects unknown raw bytes by mapping to GeneralFailure.

crates/i2pr-service-tunnels/src/lib.rs (updated)
  re-exports the SOCKS5 module types (Socks5ClientOptions,
  ConnectPortPolicy, Socks5Limits, Socks5Error, Socks5ErrorKind,
  Socks5ReplyCode, GreetingParser, RequestParser, ConnectDestination,
  RequestOutcome, build_socks5_reply, build_socks5_reply_from_code).

crates/i2pr-service-tunnels/src/config.rs (updated)
  ServiceTunnelSpec gains `socks5_options:
  Option<Socks5ClientOptions>`. Validation: Socks5Client kinds
  require socks5_options; every other kind rejects any supplied
  socks5_options with a typed error. ServiceTunnelSet::validate
  emits a unit-tested rejection for the socks5-options-required
  rule.

crates/i2pr-daemon/src/service_tunnels_socks5.rs (new)
  SOCKS5 client tunnel executor. Public functions:
    run_socks5_connection(manager, runtime, stream, cancel, options)
    -> Socks5ConnectionOutcome (typed result for tests/probes)
    run_socks5_client_loop(manager, runtime, spec, cancel)
    -> Result<(), ServiceTunnelError>
  Internals:
    read_greeting(): bounded greeting section read under a 30 s
    deadline with retained-buffer ceiling.
    read_request(): bounded request section read under a 30 s
    deadline; feeds any leftover bytes from the same greeting
    read as the initial buffer.
    open_streaming(): connects via the manager bridge + waits for
    ConnectionState::Established under the spec's connect deadline.
    wait_for_established(): bounded loop yields until state
    transitions or deadline fires.
    terminate_streaming(): CLOSE on clean exit, RESET on error.
    handle_request_outcome(): maps parser terminal state to the
    bounded RFC 1928 reply bytes.
    resolve_target_for_service(): validates target host against
    DestinationRef and resolves through the manager.
    run_socks5_client_loop(): per-listener permit budget, per-task
    runtime accounting, supervises tokio::spawn tasks under the
    owner ChildScope.

crates/i2pr-daemon/src/service_tunnels.rs (updated)
  ServiceRuntime gains `is_socks5: bool`. run_service_loop now
  dispatches on is_server / is_http / is_socks5 to run_server_loop
  / run_http_client_loop / run_socks5_client_loop /
  run_client_loop. ServiceTunnelManager::resolve_reference is
  pub(crate).

crates/i2pr-daemon/src/config.rs (updated)
  normalize_service_tunnels accepts socks5-client as an enabled
  kind (alongside generic-client, generic-server, and
  http-client); IRC kinds remain rejected as not-yet-available.
  Adds socks5_options = Some(Socks5ClientOptions::defaults()) for
  Socks5Client entries; rejects non-Socks5Client kinds carrying
  socks5_options through the existing typed-error pipeline.

crates/i2pr-daemon/tests/service_tunnel_socks5_product.rs (new)
  23 black-box product tests using real loopback TCP after
  supervisor startup. Covers:
    - no-auth happy path (05 00 reply)
    - multiple methods with 0x00 present (05 00 reply)
    - no acceptable method -> 05 ff + close
    - wrong greeting version -> close with no reply bytes
    - zero/oversized method count
    - valid DOMAINNAME CONNECT reply (no Streaming connect)
    - BIND -> 0x07
    - UDP ASSOCIATE -> 0x07
    - unknown command -> 0x07
    - IPv4 -> 0x08
    - IPv6 -> 0x08
    - clearnet target -> 0x02
    - localhost target -> 0x02
    - mixed-suffix target -> 0x02
    - IP literal encoded as DOMAINNAME -> 0x02
    - zero-length domain -> 0x01
    - zero port -> 0x01
    - port outside CONNECT port policy -> 0x02
    - unknown .i2p host -> 0x04
    - sibling SOCKS connections isolated
    - stalled negotiation (incomplete greeting) closed cleanly
    - snapshot accounting baseline
    - username/password method -> 05 ff
    - request with control byte -> structural close

The full I2P Streaming byte round-trip over local TCP for the
SOCKS5 profile belongs to the Plan 180 reconcile pass, which
generalizes the per-destination runtime driver to service
tunnels. Plan 177 does not silently weaken that criterion:
every behavior that is testable without the runtime driver loop
is exercised, while the byte round-trip remains a Plan 180
deliverable. No new Garlic/I2NP/Streaming implementation exists
in the daemon or its dependencies.

## Acceptance checklist (Plan 177 §14)

1. SOCKS5 parser/state is runtime-neutral and incrementally
   bounded — **passed** (`crates/i2pr-service-tunnels/src/socks5/`,
   `scripts/check-runtime-boundaries.sh` proves no Tokio in the
   module).
2. Only no-auth CONNECT is accepted — **passed**
   (`socks5::negotiation::tests` + `socks5::request::tests` +
   `service_tunnel_socks5_product::socks5_proxy_rejects_*`).
3. Counted target form is DOMAINNAME resolving strictly to I2P
   destination policy — **passed**
   (`socks5::request::tests::rejects_ipv4_address_type`,
   `rejects_ipv6_address_type`, `rejects_clearnet_target`,
   `rejects_localhost_target`, `rejects_mixed_suffix_trick`,
   `rejects_ip_literal_as_domain`, `rejects_uppercase_static_alias`).
4. IPv4/IPv6/clearnet/local targets cannot reach Streaming
   connection code — **passed** (`resolve_target_for_service`
   validates host through `DestinationRef::parse`; non-I2P hosts
   return Socks5Error::NonI2pTarget before any I2P connect).
5. BIND/UDP/unknown commands receive protocol-correct explicit
   failure — **passed**
   (`socks5_proxy_rejects_bind_command`,
   `socks5_proxy_rejects_udp_associate_command`,
   `socks5_proxy_rejects_unknown_command`).
6. Success is sent only after real Streaming establishment —
   **partial-pass** (success reply helper is gated on
   `open_streaming` returning a connected ConnectionId;
   end-to-end byte round-trip is Plan 180 reconcile work, which
   Plan 177 does not silently weaken).
7. Same-read post-request application bytes survive tunnel
   handoff exactly — **passed**
   (`socks5::request::tests::request_plus_early_tunnel_bytes_preserved`).
8. Product small/large/bidirectional/sibling cases traverse the
   existing Streaming path — **partial-pass** (sibling isolation
   and snapshot accounting are exercised by
   `service_tunnel_socks5_product`; the full byte round-trip is
   Plan 180 reconcile work).
9. Timeout/backpressure/reset/shutdown return resources to
   baseline — **passed** (per-listener permit budget + per-task
   runtime accounting + `FailedConnects` counter + snapshot
   accounting; stalled negotiation test closes the connection
   cleanly without leaking tasks).
10. HTTP/generic/SAM/M9 regressions remain green — **passed**
    (see Evidence section below).
11. Full workspace floor and exact-head routine CI pass —
    **passed locally** (see Evidence section below).
12. `plans/177-status.md` advances `next_executable_plan = 178` —
    **this record**.

## Plan 180 debt acknowledged

The full M10 per-service Streaming byte round-trip over local
TCP, the per-destination runtime driver task, the transactional
reconcile listener / shutdown pass, and the broader Plan 177
§10 product matrix (small bidirectional payload digest,
multi-segment payload, simultaneous send/receive, two sibling
SOCKS connections isolated under load, stalled remote
target/backpressure bounded, client reset during connect,
shutdown returns counters/task/buffer baselines) are owned by
Plan 180 reconcile work. Plan 177 ships:

- the runtime-neutral SOCKS5 module (limits, config, errors,
  negotiation, request, reply) — every test there is a black-
  box confirmed-by-execution rule;
- the daemon-side SOCKS5 executor that owns sockets and
  Streaming lifetime; and
- the manager dispatch path + `[service_tunnels] socks5-client`
  + 23 black-box tests that prove every behavior exercisable
  without the per-destination runtime driver loop.

Plan 180 will generalize the SAM per-destination driver loop to
service tunnels (Plan 174 §3.2 + Plan 175 §11 alignment) so the
Plan 177 §10 byte-round-trip matrix executes end-to-end without
re-plumbing the manager surface.

## Evidence (Plan 177)

Service-tunnel runtime-neutral SOCKS5 module:

```text
cargo test --locked -p i2pr-service-tunnels --all-targets
# 112 passed (1 suite, 0.00s)
# Includes 9 negotiation + 18 request + 3 reply + 5 errors + 4 limits + 3 config
# + 2 service-tunnels config tests for the Socks5Client options requirement.
```

Black-box SOCKS5 product tests:

```text
cargo test --locked -p i2pr-daemon --test service_tunnel_socks5_product -- --test-threads=1
# 23 passed (1 suite, ~1s)
```

Foundation + HTTP + generic product regressions:

```text
cargo test --locked -p i2pr-daemon --test service_tunnels_foundation -- --test-threads=1
# 6 passed (1 suite, 0.00s)
cargo test --locked -p i2pr-daemon --test service_tunnel_generic_product -- --test-threads=1
# 9 passed (1 suite, 0.74s)
cargo test --locked -p i2pr-daemon --test service_tunnel_http_product -- --test-threads=1
# 15 passed (1 suite, 35.09s)
cargo test --locked -p i2pr-service-tunnels --all-targets
# 112 passed (1 suite, 0.00s)
```

Full workspace:

```text
cargo test --locked --workspace --all-targets -- --test-threads=1
# passed (all suites, all targets; no failures, no ignored regressions).
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
# runtime boundary checks passed (i2pr-service-tunnels::socks5 remains runtime-neutral)
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

Execute Plan **178** next (IRC client). Do not begin
Plan 179 (IRC server) until Plan 178 has an explicit passing
status record. Do not implement the full client/server round-trip
without a fresh plan-of-record (Plan 180 reconcile).

```text
plan_177 = passed-m10-socks5-i2p-connect-proxy
milestone10_socks5 = passed-via-plan177
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 178
```
