# Plan 182 status — M10 local-delivery corrective

Status: **`passed-m10-local-delivery-corrective`**.

Plan of record:
[`plans/182-m10-local-delivery-corrective.md`](182-m10-local-delivery-corrective.md).

## Current authority

```text
plan_182 = passed-m10-local-delivery-corrective
milestone10_local_product = passed-via-plan180-and-plan182
milestone10_local_roundtrip = passed-via-plan182
milestone10_final_acceptance = not-yet-closed (plan181 §6 pending retained M6 debt)
next_executable_plan = 181 (local matrix green; remote rows blocked, see plans/181-status.md)
```

## Defect provenance (Plan 180 head `8c450cb`)

Executed probe before the fix: `service_tunnels_loopback_listener`
example + `GET http://alpha-test.i2p/` against the HTTP client
tunnel with the alias resolving to the co-owned generic server
destination. Result: `HTTP/1.1 502 Bad Gateway /
X-HTTP-Proxy-Reason: deadline reached` — the Streaming SYN never
completed. Seven cooperating gaps were identified, each
sufficient to break establishment or data flow (see the plan of
record §2 for file:line provenance):

1. no inbound-tunnel factory installed on service bridges
   (`missing_factory` on every sweep);
2. no delivery driver draining outbound queues to co-owned peers
   (SYN never left the client bridge);
3. server `listen(1)` vs client connect `(0, 0)` port mismatch
   (`NoMatchingListener` even with delivery);
4. generic accept dropped the SYN response and faked the peer;
5. IRC server accept never answered the SYN at all;
6. server-direction pump sends always failed with
   `UnknownConnection` (wrong manager + missing peer);
7. IRC client executor was a skeleton (immediate shutdown).

Two further defects surfaced while proving the fix:

8. stringly-typed backpressure detection in
   `ServicePumpEndpoint::try_send` (`contains("SendWindowFull")`
   / `contains("Congestion")`) never matched the lowercase
   Display strings, so the first window-full event killed the
   pump instead of backpressuring (96 KiB transfer died after
   ~8 KiB with `streaming congestion control rejection`);
9. the shared pump exited on local EOF after a single drain, so
   in-flight responses never reached half-closed sockets and no
   orderly CLOSE propagated (Plan 181 §5.1 half-close row);
10. aggregate semaphore permits were bound outside the spawned
    tasks, so the ceiling never engaged (all five client loops
    plus the generic server path);
11. the IRC client filter loop stripped CRLF on `Allow` and never
    re-appended it, fusing lines and stalling the server
    registration interceptor forever.

## What landed

```text
crates/i2pr-daemon/src/service_tunnels.rs
  Plan 182 §3 items 1-4, 6: per-destination outbound Notify
  signals, cumulative DeliverySweepCounters, per-destination
  supervised delivery drivers (yield/notify-or-50 ms tick,
  double sweep with timer poll, typed counter record),
  deliver_outbound sweep over the manager-level sam_destinations
  mirror reusing bridge_to_peer (no fault-profile hook),
  terminate_failed_delivery, driver wake-ups after every SYN
  queue and every pump admission, inbound factory install at
  staged build, wildcard Streaming port 0 for servers (SAM
  convention; do-not-revert note), SAM-parity generic accept
  (real peer/ports, queued SYN response, peer threaded into the
  server pump), direction-branched try_send with typed
  backpressure matching, orderly shutdown_write (CLOSE), stale
  driver cancellation on reconcile, driver cancellation on
  shutdown, permit-for-task-lifetime capture in both loops.

crates/i2pr-daemon/src/service_tunnels_irc_server.rs
  Plan 182 §3 items 5-6: SAM-parity accept step before the
  Established wait (real peer/ports, queued SYN response,
  driver wake), wildcard-port fallback 0, full peer threaded
  through established-wait/connection/raw-pump, milestone
  debug logs (SYN answered, registration outcome, wait state).

crates/i2pr-daemon/src/service_tunnels_irc_client.rs
  Plan 182 §3 item 9: complete run_irc_connection (resolve ->
  connect (0,0) -> bounded Established wait -> bounded
  line-oriented filter loop both directions with
  decide_client_to_server/decide_server_to_client, CRLF
  re-appended on Allow, 50 ms poll reads so draining never
  stalls, orderly shutdown_write on EOF, bounded backpressure
  retries).

crates/i2pr-daemon/src/service_tunnels_http.rs,
crates/i2pr-daemon/src/service_tunnels_socks5.rs
  Plan 182 driver wake-up after SYN queue; permit-for-task
  lifetime capture.

crates/i2pr-daemon/src/destination_streaming.rs
  Plan 182 §3 item 9 pump half: default-no-op shutdown_write()
  trait hook (SAM endpoint keeps the default: exit-on-EOF
  behavior byte-identical), CLOSE linger (drain-only until
  terminal or 15 s cap), CLOSE-response on terminal exit.
  No signature change; MockEndpoint and SamPumpEndpoint
  untouched.

crates/i2pr-daemon/tests/service_tunnels_local_roundtrip.rs (new)
  9 tests: generic small/large/sibling echo digests, framed
  reverse, half-close EOF propagation, HTTP GET 200 + digest,
  SOCKS5 CONNECT + echo, IRC register/message/projection,
  resource baseline incl. zero unknown_peer/missing_factory.

crates/i2pr-daemon/tests/service_tunnels_independent_application_clients.rs (new)
  6 tests: HTTP bounded-response wire surface, SOCKS5 greeting
  + IPv4-fallback rejection, IRC accept/close accounting,
  generic accept, restart-stable server identity.

crates/i2pr-daemon/tests/service_tunnels_remote_qualification.rs (new)
  Ignored-by-default §6 qualification driver (explicit
  --ignored --exact selection, fail-closed without
  I2PD_PEER_PUB_B64, asserts the §6.3 stop condition).

crates/i2pr-daemon/examples/service_tunnels_loopback_listener.rs (new)
  Two-phase harness listener (transient server-only prepare
  captures persistent b64; durable manager wires clients via
  full public material + static aliases), single-JSON port
  document, --data-dir/--generic-server-target/--irc-server-target,
  INFO logging to stderr, loopback-only.
```

## Evidence (Plan 182)

New suites (exact commands):

```text
cargo test --locked -p i2pr-daemon --test service_tunnels_local_roundtrip -- --test-threads=1
# 9 passed
cargo test --locked -p i2pr-daemon --test service_tunnels_independent_application_clients -- --test-threads=1
# 6 passed
```

Retained regressions (same implementation revision):

```text
cargo test --locked -p i2pr-daemon --test service_tunnels_foundation \
  --test service_tunnel_generic_product --test service_tunnels_final_acceptance \
  --test service_tunnels_adversarial_matrix -- --test-threads=1
# 43 passed (4 suites)
cargo test --locked -p i2pr-daemon --test service_tunnel_socks5_product \
  --test service_tunnel_irc_server_product -- --test-threads=1
# 44 passed (2 suites)
cargo test --locked -p i2pr-daemon --test service_tunnel_http_product -- --test-threads=1
# 15 passed
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_client_product -- --test-threads=1
# 18 passed
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
# 4 passed (shared-pump compatibility)
cargo test --locked -p i2pr-service-tunnels --all-targets
# 222 passed
cargo test --locked -p i2pr-daemon --lib
# 112 passed
cargo fmt --all --check
# ok
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
# No issues found
```

Full workspace floor: see `plans/181-status.md` (run on the
closing head together with the Plan 181 lane).

## Stop conditions

None of the Plan 182 §5 stop conditions fired:

- no wire-format change (Streaming/garlic/I2NP untouched);
- no M6 mixed-router work (all rows loopback-local);
- no test weakened to accommodate the implementation (the
  half-close row was reframed from echo-after-EOF to
  EOF-propagation because CLOSE ends the send direction by
  protocol contract; reply-after-EOF is out of profile and
  documented as such);
- the `ClosingRemote`-send restriction in
  `StreamingManager::send_data` is retained M6 behavior, not
  altered here;
- the SAM fault-profile hook was deliberately not ported (it
  stays a SAM-test seam).

## Handoff

Plan 181 local matrix (§4/§5) is executable on this
revision. Plan 181 §6 remote rows remain classified
`m6-mixed-router-streaming-blocker` (retained M6 debt, not
this plan); Milestone 10 final acceptance stays open.

```text
plan_182 = passed-m10-local-delivery-corrective
milestone10_local_product = passed-via-plan180-and-plan182
milestone10_local_roundtrip = passed-via-plan182
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 181
```
