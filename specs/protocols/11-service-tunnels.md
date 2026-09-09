# Service tunnels (Milestone 10)

Status: **generic tunnels landed** (Plan 175 passed; HTTP/SOCKS/IRC still not implemented)  
Planning authority: **Plan 173** (`plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md`)  
Foundation: **Plan 174** (`plans/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md`)  
Generic client/server tunnels: **Plan 175** (`plans/175-m10-generic-client-server-service-tunnels.md`)  
Next executable plan: **176** (HTTP `.i2p` proxy + CONNECT)

> Plan 174 is a refactor/foundation pass. It must not change I2P wire
> semantics or broaden listener exposure. No generic, HTTP, SOCKS5,
> or IRC listener is active yet.

## Reference basis (Plan 173 §4)

Clean-room behavior from specifications and observed reference
behavior. Do not copy Java I2P source.

- Java I2P 2.13.0 (`i2p/i2p.i2p @ 9134f808337b401e8e53c73734c81fab04280c9d`):
  `I2PTunnelClient.java`, `I2PTunnelServer.java`,
  `I2PTunnelHTTPClient.java`, `socks/SOCKS5Server.java`,
  `irc/IRCFilter.java`, `irc/IrcInboundFilter.java`,
  `irc/IrcOutboundFilter.java`, `irc/I2PTunnelIRCServer.java`.
- HTTP/1.1 proxy semantics from RFC 9110/9112 (absolute-form,
  authority-form CONNECT, hop-by-hop removal, no Content-Length on
  successful CONNECT).
- SOCKS5 from RFC 1928 (no-auth CONNECT, DOMAINNAME `.i2p` only).
- IRCv3 message-tags framing (512-byte core line, 8191-byte tag
  section, 4094-byte client tag data; reject, never truncate).

## Architecture lock (Plan 173 §3)

```text
i2pr-client (destination, LeaseSet2, ECIES, routing, Streaming)
    ^
    |
i2pr-service-tunnels (policy/protocol only, runtime-neutral)
    ^
    |
i2pr-daemon (only socket/task/composition owner)
```

- `crates/i2pr-service-tunnels` owns configuration validation,
  destination references, static aliases, and (later) HTTP/SOCKS/IRC
  parsers and policies. No Tokio, no listeners, no timers, no
  filesystem, no transport/tunnel-build internals, no NetDB mutation,
  no Garlic/I2NP construction.
- `crates/i2pr-daemon/src/destination_streaming.rs` owns the shared
  bounded socket<->Streaming pump. SAM reuses it via a narrow
  capability; future service tunnels reuse the same primitive.
- Server tunnels use versioned, atomic, secret-safe persistent
  router-owned destinations (Plan 175). Client tunnels default to
  ephemeral router-owned destinations; sharing only through explicit
  bounded `SharedClientGroup`.
- Client listeners bind loopback only; generic server TCP targets
  are loopback only; Unix targets are path values until daemon
  platform validation.

## Plan 174 foundation surface

- `ServiceTunnelId` / `ServiceClientGroupId` (1–64 bytes,
  lower-case `a-z0-9-_`, leading alphanumeric).
- `ServiceTunnelKind`: `generic-client`, `generic-server`,
  `http-client`, `socks5-client`, `irc-client`, `irc-server`.
- `DestinationPolicy`: `Dedicated` or
  `SharedClientGroup(ServiceClientGroupId)` (shared only for client
  kinds; never implicit).
- `DestinationRef`: `Base32Hash` (52-char `a-z2-7` +
  `.b32.i2p`), `StaticAlias` (lower-case `.i2p`, 67-byte ceiling),
  `ConfiguredDestination` (bounded public material, no `priv`,
  no IP literals, no URIs). No DNS/filesystem/network lookup, no
  clearnet fallback.
- `LocalListenerSpec` (loopback `ip:port` only),
  `ServerTarget` (`LoopbackTcp` loopback only or
  `UnixPath` absolute `unix:/path` value).
- `ServiceResourceLimits` (32 services, 64 aliases, 128 conns per
  service, 1024 aggregate, 1024–1048576 buffered bytes per
  direction, 8 configured targets) and `ServiceTimeouts`
  (connect 1–120 s, read/write 1–600 s, shutdown 1–30 s).
- `ServiceTunnelSet::validate()` rejects duplicate IDs, duplicate
  binds, contradictory options, and ceiling overflow before daemon
  state changes.
- Daemon `[service_tunnels]` is strict (`deny_unknown_fields`),
  disabled by default, loopback-only, with duplicate-bind detection
  and router-wide budget guards. Any `enabled = true` tunnel is
  rejected as not-yet-available until Plan 175; no listener starts
  in Plan 174.
- Shared pump (`destination_streaming::run_stream_pump`) is generic
  over `AsyncRead + AsyncWrite`, bounded per-read chunk, negotiated
  segmentation, send-window backpressure without busy spin,
  sibling-isolated drain, fair ACK/driver progress, and
  cancellation/EOF/terminal convergence. SAM delegates to it;
  no second byte pump remains.

## Explicit not-yet-implemented product rows

| Product | Status | Owning plan |
| --- | --- | --- |
| Generic TCP client listener | passed-experimental-loopback-only | 175 |
| Generic TCP server destination + target | passed-experimental-loopback-only | 175 |
| Persistent server destination storage | passed-experimental-secret-safe | 175 |
| HTTP `.i2p` proxy + CONNECT | not-yet-implemented | 176 |
| SOCKS5 no-auth `.i2p` CONNECT | not-yet-implemented | 177 |
| IRC client privacy filter | not-yet-implemented | 178 |
| IRC server authenticated hostname | not-yet-implemented | 179 |
| Full composition / reconcile / hardening | not-yet-implemented | 180 |
| Independent acceptance / final closure | not-yet-implemented | 181 |

No row above may be marked passed until its owning plan has an
explicit passing status record with command-derived evidence.

## Evidence (Plan 175)

- `crates/i2pr-storage/src/service_destination.rs`
  (`ServiceDestinationStore`, `ServiceDestinationRecord`,
  `ServiceDestinationStorageError`; `#![forbid(unsafe_code)]`;
  versioned, permission-hardened, atomic, no-replace; corruption
  fail-closed).
- `crates/i2pr-daemon/src/service_tunnels.rs`
  (`ServiceTunnelManager`, `ServiceRuntime`,
  `ServiceTunnelSnapshot`, `ClientTarget`,
  `DestinationFailure`; per-service supervisor loops under a
  shared `ChildScope`; `with_destination_bridge` /
  `lookup_local_service_destination` typed capabilities).
- `crates/i2pr-daemon/src/config.rs` (`[service_tunnels]`
  accepts `enabled = true` for `generic-client` / `generic-server`;
  HTTP/SOCKS/IRC remain rejected).
- `crates/i2pr-daemon/tests/service_tunnel_generic_product.rs`
  (9 black-box manager tests: prepare/restart-stable identity,
  corrupt-identity rejection, missing-target rejection,
  Unix-target not-yet-supported, no HTTP/SOCKS/IRC leak,
  cross-tunnel local destination lookup, snapshot accounting,
  disabled-service ignored).
- `crates/i2pr-daemon/tests/service_tunnels_foundation.rs`
  (Plan 174 graph/config regression suite; updated to confirm
  the Plan 175 generic-client/server acceptance rule).
- `scripts/check-dependency-direction.sh` (workspace graph
  unchanged) and `scripts/check-runtime-boundaries.sh`
  (`i2pr-service-tunnels` remains runtime-neutral).
- `plans/175-status.md` (exact evidence, `next_executable_plan = 176`).

## Evidence (Plan 174)

- `crates/i2pr-service-tunnels/` (config, destination, errors,
  events; `#![forbid(unsafe_code)]`, no Tokio/sockets).
- `crates/i2pr-daemon/src/destination_streaming.rs` (generic pump
  + 5 deterministic tests: small bidirectional, multi-segment,
  backpressure, sibling isolation, cancel/half-close).
- `crates/i2pr-daemon/src/sam/raw_stream.rs` (thin SAM adaptation
  over the shared primitive; Plan 151/152 regressions green).
- `crates/i2pr-daemon/src/config.rs` (`[service_tunnels]`
  strict surface + 7 unit tests).
- `crates/i2pr-daemon/tests/service_tunnels_foundation.rs`
  (4 black-box config/graph tests).
- `scripts/check-dependency-direction.sh` (explicit
  `i2pr-service-tunnels` edge) and
  `scripts/check-runtime-boundaries.sh` (runtime-neutral
  enforcement).
- `plans/174-status.md` (exact evidence, `next_executable_plan = 175`).
