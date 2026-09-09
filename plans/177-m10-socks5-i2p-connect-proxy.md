# Plan 177 — Milestone 10 SOCKS5 `.i2p` CONNECT proxy

Status: **blocked until Plan 176 passes**.

## 1. Goal

Add the M10 SOCKS5 client-proxy profile on top of the shared service-tunnel destination/Streaming runtime:

```text
ordinary SOCKS5 client
  -> loopback i2pr SOCKS5 listener
  -> bounded RFC 1928 negotiation/request parser
  -> .i2p-only destination resolution
  -> existing Streaming client path
  -> remote I2P service / generic server tunnel
```

The M10 profile is intentionally small:

```text
SOCKS version        = 5 only
authentication       = NO AUTHENTICATION REQUIRED (0x00) only
command              = CONNECT (0x01) only
counted address type = DOMAINNAME (0x03) containing an I2P name
```

No clearnet outproxy, UDP ASSOCIATE, BIND, SOCKS4/4a, username/password authentication, or Tor SOCKS extensions belong in this plan.

## 2. Reference basis

Use clean-room behavior from:

- RFC 1928;
- current official I2P SOCKS/I2PTunnel documentation;
- exact-pinned Java I2P 2.13.0 commit `9134f808337b401e8e53c73734c81fab04280c9d`, especially `apps/i2ptunnel/java/src/net/i2p/i2ptunnel/socks/SOCKS5Server.java` as a behavior reference only.

The Java reference supports substantially more behavior than M10. Do not copy its auth, UDP, Tor-resolve, IP/outproxy, or compatibility extensions merely because they exist.

## 3. Runtime-neutral SOCKS5 state machine

Add a runtime-neutral module under `i2pr-service-tunnels`, e.g.:

```text
src/socks5/
  mod.rs
  negotiation.rs
  request.rs
  reply.rs
```

No Tokio, sockets, timers, DNS, NetDB mutation, or Streaming ownership in this module.

Required typed states:

```text
AwaitGreetingHeader
AwaitMethods
MethodSelected
AwaitRequestHeader
AwaitDomain
AwaitPort
ReadyToConnect
TunnelMode
Rejected
```

The parser must be incremental and bounded; it may receive one byte at a time or greeting+request+early payload in one read.

## 4. Method negotiation

Accept:

```text
VER = 0x05
NMETHODS = 1..MAX_SOCKS_METHODS
METHODS contains 0x00
```

Reply:

```text
05 00
```

If `0x00` is not offered, reply `05 ff` and close after the reply is flushed.

Reject/fail closed on:

- wrong version;
- zero methods;
- method list exceeding the hard ceiling;
- incomplete method list past handshake deadline;
- retained input exceeding handshake-byte ceiling.

Do not implement username/password merely to accommodate a client configuration error.

## 5. CONNECT request profile

Accept only:

```text
VER  = 0x05
CMD  = 0x01 CONNECT
RSV  = 0x00
ATYP = 0x03 DOMAINNAME
DST.PORT = nonzero u16
```

`DST.ADDR` must parse through the same M10 `DestinationRef`/static-alias policy used by HTTP and generic tunnels.

Mandatory accepted targets:

- canonical `.b32.i2p` destination hash;
- configured bounded static `.i2p` alias.

Unknown human-readable `.i2p` names fail explicitly unless present in the configured alias resolver.

Hard reject before Streaming/network action:

- IPv4 (`ATYP=0x01`);
- IPv6 (`ATYP=0x04`);
- clearnet hostname;
- localhost/local aliases;
- suffix-confusion forms such as `x.i2p.example.com`;
- zero-length hostname;
- control/NUL/whitespace hostname bytes;
- port zero;
- unsupported command.

This is stricter than Java I2P's broad SOCKS/outproxy profile by design.

## 6. Required reply mapping

Produce deterministic RFC 1928 replies. At minimum:

```text
success                         -> 0x00
policy/non-I2P target rejected  -> 0x02
unresolved I2P name/no route    -> 0x04
connection refused              -> 0x05
TTL/deadline style timeout      -> 0x06 where semantically appropriate
command unsupported             -> 0x07
address type unsupported        -> 0x08
other bounded connect failure   -> 0x01
```

Successful reply may use a neutral loopback bind address/port; it must not expose destination private material, local router identities, or unrelated listener addresses.

Do not leak untrusted hostname contents into error logs.

## 7. Daemon composition

Activate `socks5-client` in the Plan 175 service manager.

Listener rules:

- loopback only;
- disabled by default;
- tests use ephemeral ports;
- one admission permit before retained handshake state;
- bounded handshake deadline;
- bounded connection lifetime/resources through existing service limits.

Connection sequence:

1. negotiate no-auth;
2. parse and validate CONNECT;
3. resolve I2P destination;
4. establish a real Streaming connection under connect deadline;
5. send SOCKS success only after Streaming reaches `Established`;
6. enter opaque tunnel mode and transfer ownership to the Plan 174 shared raw byte pump;
7. preserve any bytes already read beyond the end of the CONNECT request as first tunnel bytes;
8. terminal local/remote/cancel state releases permits/tasks/buffers.

If Streaming establishment fails, return the mapped SOCKS error and never enter tunnel mode.

## 8. Bounds

Central hard ceilings must include:

- method count;
- retained greeting/request bytes;
- domain length (RFC field is u8; local policy may be lower but must be explicit);
- handshake duration;
- connect duration;
- pre-tunnel early-payload bytes retained from the same read;
- per-listener/aggregate active connections through shared M10 limits.

Do not allocate proportional to attacker-provided counts before validating against the ceiling.

## 9. Runtime-neutral tests

Cover at least:

1. no-auth happy path;
2. multiple methods with `0x00` present;
3. no acceptable method -> `0xff`;
4. wrong greeting version;
5. zero/oversized method count;
6. valid DOMAINNAME CONNECT;
7. BIND -> `0x07`;
8. UDP ASSOCIATE -> `0x07`;
9. unknown command -> `0x07`;
10. IPv4/IPv6 -> `0x08` or policy rejection as documented;
11. zero-length domain;
12. malformed `.i2p` suffix;
13. IP literal encoded as DOMAINNAME rejected;
14. zero port;
15. incremental one-byte parsing;
16. greeting+request in one buffer;
17. request+early tunnel bytes preserved exactly;
18. every error reply is bounded and deterministic.

## 10. Black-box product tests

Add a real TCP suite, e.g.:

```text
crates/i2pr-daemon/tests/service_tunnel_socks5_product.rs
```

Canonical topology:

```text
SOCKS5 client socket
  -> i2pr SOCKS listener
  -> I2P Streaming
  -> i2pr generic server tunnel
  -> loopback scripted/echo target
```

After startup, move application bytes only through OS sockets.

Required cases:

- small bidirectional payload digest;
- multi-segment payload;
- simultaneous send/receive;
- two sibling SOCKS connections isolated;
- same-read CONNECT + first application bytes preserved;
- unresolved `.i2p` -> failure, no server target connection;
- IPv4/IPv6/clearnet target -> rejection before Streaming connect;
- BIND/UDP -> command-not-supported;
- stalled negotiation timeout;
- stalled remote target/backpressure remains bounded;
- client reset during connect;
- shutdown returns counters/task/buffer baselines.

If `curl` is available, `curl --socks5-hostname` may be a non-counted smoke test here. Plan 181 owns independent-client evidence.

## 11. Security/non-goals

Do not add:

- DNS resolution of SOCKS hostnames;
- IP-literal forwarding;
- clearnet outproxy;
- SOCKS UDP;
- BIND;
- SOCKS4/4a;
- authentication;
- Tor RESOLVE/RESOLVE_PTR;
- arbitrary local/LAN target relay.

The SOCKS parser must remain policy/protocol-only; all socket/task ownership remains daemon-side.

## 12. Documentation/support

Update at passing closure:

- `specs/protocols/11-service-tunnels.md` SOCKS5 profile table;
- `docs/architecture/i2pr-service-tunnels.md` parser/runtime boundary;
- sample disabled SOCKS config;
- `specs/support.toml` experimental loopback `.i2p` CONNECT row;
- `plans/177-status.md`.

## 13. Validation floor

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-service-tunnels --all-targets
cargo test --locked -p i2pr-daemon --test service_tunnel_socks5_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_http_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_generic_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
cargo deny check advisories bans sources
```

## 14. Acceptance criteria

Plan 177 passes only when:

1. SOCKS5 parser/state is runtime-neutral and incrementally bounded.
2. Only no-auth CONNECT is accepted.
3. Counted target form is DOMAINNAME resolving strictly to I2P destination policy.
4. IPv4/IPv6/clearnet/local targets cannot reach Streaming connection code.
5. BIND/UDP/unknown commands receive protocol-correct explicit failure.
6. Success is sent only after real Streaming establishment.
7. Same-read post-request application bytes survive tunnel handoff exactly.
8. Product small/large/bidirectional/sibling cases traverse the existing Streaming path.
9. timeout/backpressure/reset/shutdown return resources to baseline.
10. HTTP/generic/SAM/M9 regressions remain green.
11. Full workspace floor and exact-head routine CI pass.
12. `plans/177-status.md` advances `next_executable_plan = 178`.

## 15. Stop conditions

Write a narrow corrective instead of weakening this plan if:

- the implementation requires DNS/IP forwarding to satisfy normal SOCKS clients;
- success must be sent before Streaming establishment;
- same-read payload preservation cannot be achieved without unbounded buffering;
- a shared Streaming/runtime defect is exposed;
- SOCKS parsing is moved into daemon socket code instead of the runtime-neutral policy crate.

## 16. Handoff

Expected transition:

```text
plan_177 = passed-m10-socks5-i2p-connect-proxy
milestone10_socks5 = passed-via-plan177
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 178
```
