# Plan 182 — M10 local-delivery corrective (per-destination driver, wildcard ports, accept-path and pump repairs)

Status: **open corrective; blocks Plan 181 §5 until passed**.

## 1. Goal

Repair the M10 service-tunnel local byte round-trip so an i2pr
client tunnel can establish Streaming to an i2pr server tunnel
owned by the same manager and move application bytes in both
directions through the existing Plan 174 shared pump. No wire
change, no new protocol, no M6 mixed-router work.

## 2. Defect provenance (all observed on Plan 180 head `8c450cb`)

Executed probe: `service_tunnels_loopback_listener` example +
`GET http://alpha-test.i2p/` against the HTTP client tunnel with
the alias resolving to the co-owned generic server destination.
Result: `HTTP/1.1 502 Bad Gateway / X-HTTP-Proxy-Reason: deadline
reached`. The Streaming SYN never completes. Code inspection
shows seven cooperating gaps, each sufficient to break
establishment or data flow:

1. **No inbound-tunnel factory on service bridges.**
   `ServiceTunnelManager::create_bridge_for_spec`
   (`crates/i2pr-daemon/src/service_tunnels.rs`) carries
   `BridgeData.inbound_tunnel_factory` but never calls
   `SamDestinationHandle::install_inbound_tunnel_factory`, so the
   Plan 129 local seam would count `missing_factory` for every
   service delivery.
2. **No delivery driver.** Nothing drains
   `StreamingManager::drain_outbound` on service bridges or
   routes `TransportSendRequest`s to the peer bridge. SAM's
   `run_destination_driver` + `deliver_outbound`
   (`crates/i2pr-daemon/src/sam.rs`,
   `crates/i2pr-daemon/src/sam/raw_stream.rs`) has no service
   equivalent; `ServicePumpEndpoint::notify_outbound` only polls
   timers.
3. **Streaming port mismatch.** Service servers
   `listen(1)`/`accept(1)` while every client path connects with
   `local_port = 0, remote_port = 0`. The proven SAM convention
   is connect `(0, 0)` against a wildcard `listen(0)` listener
   (`sam.rs:2131-2132`, `sam.rs:2364`, `sam.rs:2596`). A SYN with
   wire `destination_port = 0` matches no port-1 listener, so
   `handle_inbound_syn` fails with `NoMatchingListener` even if
   delivery ran.
4. **Generic server accept drops the SYN response.**
   `handle_server_syn` (`service_tunnels.rs`) discards the
   `accept_inbound_syn` `TransportSendRequest`
   (`let _accept_outcome`), passes a fake peer (zero hash + own
   signing key) and hardcoded `(0, 0)` ports instead of the
   connection's authenticated peer/ports (contrast SAM
   `sam.rs:2503-2534`), and never queues the response.
5. **IRC server accept never answers the SYN.**
   `run_irc_server_loop` /
   `run_irc_server_connection_established`
   (`crates/i2pr-daemon/src/service_tunnels_irc_server.rs`)
   waits for `Established` without ever calling
   `accept_inbound_syn`, so IRC server connections cannot
   establish.
6. **Server-direction pump can never send.**
   `ServicePumpEndpoint::try_send` uses the canonical
   `streaming()` manager and requires `self.remote`, but server
   endpoints are built with `remote = None` while their
   connections live on `receiver_streaming()`. Every
   server-to-client send fails with `UnknownConnection`
   (contrast `raw_stream.rs:565-605` direction branching).
7. **IRC client executor is a skeleton.**
   `run_irc_connection`
   (`crates/i2pr-daemon/src/service_tunnels_irc_client.rs`)
   shuts the TCP stream down immediately; it never resolves,
   connects, filters, or pumps.

## 3. Narrow corrective (production)

All changes stay inside `crates/i2pr-daemon/src/` (plus tests):

1. Install the fabric inbound-tunnel factory into every staged
   service bridge at build time.
2. Add manager-owned per-destination outbound `Notify` signals,
   `DeliverySweepCounters` accounting (reuse
   `crate::sam::fabric::DeliverySweepCounters`), and a
   `deliver_outbound` sweep over the manager-level
   `sam_destinations` mirror reusing the public
   `crate::sam::streams::bridge_to_peer` seam (no fault-profile
   hook; that stays a SAM-test seam).
3. Spawn one supervised per-destination delivery driver per
   runtime in `start_supervisors` (mirror
   `run_destination_driver`: yield, notify-or-50 ms tick,
   double sweep with timer poll between, counter record,
   established wake via existing polling).
4. Notify the driver after every SYN queue (all four client
   kinds), after every admitted pump segment (existing
   `notify_outbound` hook), and after every queued SYN
   response.
5. Switch service server Streaming listeners to wildcard port 0
   (`listen` + both server `accept` loops), keeping client
   connect `(0, 0)`. Document the SAM-convention rationale at
   the change site; do not revert to port 1 without a passing
   round-trip test behind the revert.
6. Repair `handle_server_syn` to SAM parity: real authenticated
   peer (hash + signing + static from the connection, zero
   fallback only for the static key exactly as SAM does), real
   connection ports, queue the SYN response, notify the
   driver, and carry the peer into `run_server_connection` /
   the server pump endpoint.
7. Branch `ServicePumpEndpoint::try_send` on direction (server
   sends through `receiver_streaming_mut`) and thread the
   server peer through `new_server` (+ the IRC raw-pump call
   site).
8. Add the IRC-server accept step (real peer/ports, queue SYN
   response, notify) before the Established wait.
9. Implement `run_irc_connection`: resolve → connect `(0, 0)`
   → bounded Established wait → bounded line-oriented
   client-to-server filter loop using the runtime-neutral
   `decide_client_to_server` API with the module ceilings →
   admitted bytes through the owning endpoint → delivered
   bytes back to TCP → EOF/terminal/cancel convergence. The
   shared opaque `run_stream_pump` is untouched (Plan 180
   single-pump invariant holds).

Explicitly out of scope: wire-format changes, M6
mixed-router/tunnel work, remote independent-router rows
(Plan 181 §6 stays blocked on the retained M6 debt), new
dependencies, unbounded channels.

## 4. Tests (all command-derived, loopback-only)

New focused suite
`crates/i2pr-daemon/tests/service_tunnels_local_roundtrip.rs`:

- generic echo small-payload digest equality;
- generic echo `>= 32` KiB multi-segment digest equality;
- generic reverse-bytes through a reversing fixture;
- generic half-close/EOF propagation;
- two sibling connections isolated;
- HTTP GET 200 + body digest through a loopback HTTP fixture;
- HTTP POST body digest;
- SOCKS5 DOMAINNAME CONNECT + echo bytes (raw RFC 1928
  handshake in-test; no DNS);
- IRC client → server registration + PRIVMSG round-trip +
  ACTION through a loopback IRC fixture, asserting the
  fixture-observed USER hostname equals the authenticated
  peer-Destination projection;
- server destination restart stability (already covered;
  retained as a row);
- post-run snapshot/resource baseline (connections,
  draining, failed-connects) returns to zero.

Every test binds `127.0.0.1:0`, uses bounded deadlines, and
drives bytes only through TCP. No private key material or raw
payloads in assertions beyond digest equality.

## 5. Acceptance criteria

1. The seven §2 gaps are each closed by a named code change;
   no gap is closed by weakening a test.
2. New round-trip suite passes (11+ tests).
3. All retained Plan 174–180 suites stay green.
4. Full workspace floor + clippy + fmt + static checkers pass.
5. `plans/182-status.md` records the defect provenance, the
   fix list, and command-derived evidence.
6. Plan 181 §5 matrix is then executable; Plan 181 §6 remote
   rows remain classified `m6-mixed-router-streaming-blocker`
   (separate retained debt, not this plan).

## 6. Handoff

On success:

```text
plan_182 = passed-m10-local-delivery-corrective
milestone10_local_product = passed-via-plan180-and-plan182
milestone10_local_roundtrip = passed-via-plan182
milestone10_final_acceptance = not-yet-closed (plan181 §6 pending M6)
next_executable_plan = 181 (local matrix) then M6 corrective
```

Do not claim remote independent-router interop from this plan.
