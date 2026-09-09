# Service tunnels (Milestone 10)

Status: **IRC `.i2p` server profile landed** (Plan 179 passed; full client/server byte round-trip integration still on Plan 180 reconcile roadmap)  
Planning authority: **Plan 173** (`plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md`)  
Foundation: **Plan 174** (`plans/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md`)  
Generic client/server tunnels: **Plan 175** (`plans/175-m10-generic-client-server-service-tunnels.md`)  
HTTP `.i2p` proxy + CONNECT: **Plan 176** (`plans/176-m10-http-i2p-proxy-and-connect.md`)  
SOCKS5 `.i2p` CONNECT: **Plan 177** (`plans/177-m10-socks5-i2p-connect-proxy.md`)  
IRC `.i2p` client profile + privacy filter: **Plan 178** (`plans/178-m10-irc-client-profile-and-privacy-filtering.md`)  
IRC `.i2p` server profile + authenticated peer hostname: **Plan 179** (`plans/179-m10-irc-server-profile-and-authenticated-peer-hostname.md`)  
Next executable plan: **180** (composition, reconcile, hardening)

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
  destination references, static aliases, the Plan 176 HTTP/1.1
  parser/rewrite/target-validation/error-response surface, the
  Plan 177 SOCKS5 negotiation/request/reply surface, and the
  Plan 178 IRC line-parser/tag/classifier/filter surface. No
  Tokio, no listeners, no timers, no filesystem, no
  transport/tunnel-build internals, no NetDB mutation, no
  Garlic/I2NP construction.
- `crates/i2pr-daemon/src/destination_streaming.rs` owns the shared
  bounded socket<->Streaming pump. SAM reuses it via a narrow
  capability; service tunnels reuse the same primitive.
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
  rejected as not-yet-available until its owning plan lands; no
  listener starts in Plan 174.
- Shared pump (`destination_streaming::run_stream_pump`) is generic
  over `AsyncRead + AsyncWrite`, bounded per-read chunk, negotiated
  segmentation, send-window backpressure without busy spin,
  sibling-isolated drain, fair ACK/driver progress, and
  cancellation/EOF/terminal convergence. SAM delegates to it;
  no second byte pump remains.

## Plan 176 HTTP surface (added)

The `http-client` service kind is enabled alongside `generic-client`
and `generic-server` in Plan 176. Its runtime-neutral surface lives
under `crates/i2pr-service-tunnels/src/http/`:

- `parser` — strict HTTP/1.1 request-line + header parser with hard
  ceilings (request-line 8192, header total 65536, header count
  100, name 256, value 8192, retained 65536). Rejects NUL/control
  bytes, bare CR, lone LF, obs-fold, conflicting `Content-Length`,
  `Transfer-Encoding`+`Content-Length`, duplicate/conflicting Host,
  overlong fields, smuggling ambiguities. Headers are emitted as
  single-line entries (no folding).
- `target` — `RequestTarget` parser for absolute-form / origin-form
  / authority-form. Validates `.i2p` / `.b32.i2p` authorities only;
  rejects IP literals, `localhost`, mixed-suffix confusion
  (`foo.i2p.example`), userinfo, malformed/empty port, overlong
  hosts. `origin_form()` produces the rewritten request-target;
  `canonical_authority()` returns the normalized `Host`.
- `rewrite` — `Connection` header parse + iterate-and-drop, drop
  known hop-by-hops (`Proxy-Connection`, `Keep-Alive`, `TE`,
  `Trailer`, `Upgrade`, `Transfer-Encoding`), force
  `Connection: close`, normalize `Host`, apply the privacy policy
  (`Via`, `Forwarded`, `X-Forwarded-*`, `Proxy-Authorization`,
  `Referer`, `From` strip; `User-Agent` keep/strip/replace-stable).
- `response` — bounded HTTP/1.1 error response builder. Body is
  always the static reason phrase; diagnostic detail is bounded
  to a sanitized `X-HTTP-Proxy-Reason` header (no CR/LF/control
  byte echo).
- `config` — `HttpClientOptions { privacy: PrivacyPolicy, ...
  }`. Default `User-AgentPolicy` is `ReplaceStable` with value
  `i2pr/0.1`. Default CONNECT-allowed port is `{ 443 }`; the set
  is bounded to `HTTP_OPTIONS_MAX_PORTS` entries (16).

Daemon-side composition:
- `crates/i2pr-daemon/src/service_tunnels_http.rs` owns the
  per-connection HTTP executor. Reads headers under a 30 s
  deadline, dispatches CONNECT to a 2xx reply followed by the
  shared Plan 174 byte pump in tunnel mode, or rewrites + forwards
  ordinary requests and runs the shared pump in body/response
  mode. Termination emits Streaming CLOSE on success and RESET on
  error. No new Garlic/I2NP/Streaming implementation is introduced.
- `crates/i2pr-daemon/src/service_tunnels.rs` adds `is_http` on
  `ServiceRuntime` and dispatches `http-client` services to the new
  HTTP supervisor loop. The pre-existing `service_tunnels_http`
  module owns the listener/connection/sibling-isolation logic.

The full I2P Streaming byte round-trip over local TCP for the
HTTP profile is owned by Plan 180 reconcile work, which
generalizes the per-destination runtime driver to service tunnels.
Plan 176 does not silently weaken that criterion: every behavior
that is testable without the runtime driver loop is exercised,
while the byte round-trip remains a Plan 180 deliverable.

## Plan 177 SOCKS5 surface (added)

The `socks5-client` service kind is enabled alongside
`generic-client`, `generic-server`, and `http-client` in
Plan 177. Its runtime-neutral surface lives under
`crates/i2pr-service-tunnels/src/socks5/`:

- `negotiation` — incremental RFC 1928 greeting parser
  (`VER=0x05`, `NMETHODS=1..16`, requires `0x00 NO AUTHENTICATION
  REQUIRED`); rejects `0x02` username/password even when offered;
  replies `05 00` on success and `05 ff` on no acceptable method.
- `request` — incremental CONNECT request parser
  (`VER/CMD/RSV/ATYP`, `ATYP=0x03 DOMAINNAME` only); rejects
  `IPv4`/`IPv6`, `BIND`, `UDP ASSOCIATE`, unknown commands,
  zero-length domain, zero port, and NUL/control/whitespace
  domain bytes; preserves same-read post-request bytes verbatim.
- `reply` — deterministic 10-byte RFC 1928 reply generator with
  a neutral loopback `127.0.0.1:0` bind; never echoes untrusted
  request bytes or destination private material.
- `errors` — typed `Socks5ErrorKind` mapped to the eight RFC 1928
  reply codes (`0x00` success, `0x01` general failure, `0x02`
  connection not allowed, `0x04` host unreachable, `0x05`
  connection refused, `0x06` TTL expired, `0x07` command not
  supported, `0x08` address type not supported).
- `config` — `Socks5ClientOptions { port_policy: ConnectPortPolicy,
  destination_ports, allowed_hosts }`. Default CONNECT allowed
  ports: `{443}`; bounded to `SOCKS5_OPTIONS_MAX_PORTS` (16).
- `limits` — `Socks5Limits` with hard typed ceilings for method
  count (16), greeting bytes (32), request header bytes (32),
  domain length (255), retained buffer (320), and reply bytes
  (64). Every ceiling is also a hard maximum validated against
  the typed maximum at startup.

Daemon-side composition:

- `crates/i2pr-daemon/src/service_tunnels_socks5.rs` owns the
  per-connection SOCKS5 executor. Reads greeting + CONNECT under
  bounded deadlines (30 s each), validates the CONNECT port
  against the per-service `ConnectPortPolicy`, resolves the
  destination through the manager (Base32/alias/local-delivery
  path), opens I2P Streaming, sends the success reply only after
  Streaming reaches `Established`, then runs the shared Plan 174
  byte pump in opaque tunnel mode with same-read post-request
  bytes preserved as first tunnel bytes. Termination emits
  Streaming CLOSE on success and RESET on pump error.
- `crates/i2pr-daemon/src/service_tunnels.rs` adds `is_socks5`
  on `ServiceRuntime` and dispatches `socks5-client` services to
  the new SOCKS5 supervisor loop.
- `crates/i2pr-daemon/src/config.rs` (`[service_tunnels]`)
  accepts `enabled = true` for `generic-client`,
  `generic-server`, `http-client`, and `socks5-client`; IRC
  remains rejected as not-yet-available.

Explicit non-support (Plan 177 §11): clearnet outproxy, DNS
resolution of SOCKS hostnames, IP-literal forwarding, SOCKS UDP,
BIND, SOCKS4/4a, authentication, Tor RESOLVE/RESOLVE_PTR, and
arbitrary local/LAN target relay. SOCKS5 is strictly stricter
than Java I2P's broad SOCKS/outproxy profile.

The full I2P Streaming byte round-trip over local TCP for the
SOCKS5 profile is the same Plan 180 reconcile pass that
generalizes the per-destination runtime driver to service
tunnels; Plan 177 does not silently weaken that criterion.

## Plan 178 IRC client surface (added)

The `irc-client` service kind is enabled alongside
`generic-client`, `generic-server`, `http-client`, and
`socks5-client` in Plan 178. Its runtime-neutral surface lives
under `crates/i2pr-service-tunnels/src/irc/`:

- `line` — incremental CRLF line parser: one bounded
  partial-line buffer per direction, multiple lines per read,
  one line fragmented across many reads, IRCv3 tagged-line
  fragmentation, accepted line followed by partial next line.
  Overlong lines are dropped without truncation, never split
  into a syntactically different valid command.
- `tags` — structural IRCv3 message-tag framing: opaque tag
  names/values, leading-`@` envelope with separating space,
  per-key / per-count / cumulative tag-data ceilings, invalid
  escapes rejected. Tag presence never bypasses command
  filtering; core/tag size limits are enforced separately.
- `policy` — typed command classifier with an explicit
  per-direction allowlist. Client-to-server admits the full
  client set: `PASS CAP AUTHENTICATE NICK USER PING PONG
  JOIN PART QUIT PRIVMSG NOTICE MODE TOPIC AWAY NAMES LIST
  WHO WHOIS WHOWAS ISON INVITE KICK USERHOST`.
  Server-to-client admits numeric replies (`001`–`999`),
  the documented server commands (`ACCOUNT CHGHOST ERROR`),
  and the relayed channel/messaging set (`PING PONG PRIVMSG
  NOTICE MODE JOIN NICK QUIT PART KICK TOPIC CAP
  AUTHENTICATE`); registration-only commands are rejected
  inbound. Unknown/unclassified commands are dropped, never
  passed.
- `client_filter` — client-to-network privacy rewrites:
  `USER` hostname/servername replaced with stable
  non-identifying placeholders (username + realname preserved
  within bounds); location-bearing `PING` rewritten with one
  bounded per-connection outstanding rewrite token so the
  expected `PONG` shape can be presented back; `QUIT`/`PART`
  reasons pass unchanged by default with a named opt-in stable
  replacement policy. `PRIVMSG`/`NOTICE` CTCP policy allows
  `ACTION`, drops malformed/multi-delimiter messages, drops
  address-bearing `DCC`, and drops unsupported CTCP by default.
- `config` — `IrcClientOptions { allowed_hosts,
  reason_rewrite, user_realname_max_bytes }`. Reason rewrite
  defaults to `Keep`.
- `errors`/`limits` — bounded typed errors (no secrets, no
  message text in diagnostics) and hard ceilings: core 512,
  tag envelope 8191, client tag data 4094, line buffer 8192,
  tag count 128, tag key 64.

Daemon-side composition:

- `crates/i2pr-daemon/src/service_tunnels_irc_client.rs` owns
  the per-connection IRC client executor. Binds one loopback
  listener per `irc-client` spec, runs the per-direction line
  parser under bounded deadlines with one partial-line buffer
  per side, consumes runtime-neutral filter decisions, and
  reuses the Plan 149 destination product path + Plan 174
  shared byte pump. No new Garlic/I2NP/Streaming implementation
  is introduced.
- `crates/i2pr-daemon/src/service_tunnels.rs` adds `is_irc`
  on `ServiceRuntime` and dispatches `irc-client` services to
  the new IRC supervisor loop.
- `crates/i2pr-daemon/src/config.rs` (`[service_tunnels]`)
  accepts `enabled = true` for `generic-client`,
  `generic-server`, `http-client`, `socks5-client`, and now
  `irc-client`; `irc-server` remains rejected as
  not-yet-available.

Explicit non-support (Plan 178 §12): DCC tunnel support,
WEBIRC, TLS termination, SASL credential management, IRC
bouncer functionality, channel/user state database, and an
arbitrary protocol-aware server-side filter. The IRC profile
is strictly stricter than Java I2P's broad IRC/DCC/operator
profile.

Tag-size semantics (recorded explicitly per Plan 178 §13):
the 512-byte ceiling applies to the post-tag core portion
including CRLF; the 8191-byte ceiling applies to the tag
envelope (leading `@` plus tags plus separating space); the
4094-byte ceiling applies to cumulative un-escaped client tag
values; the 8192-byte per-direction buffer is the combined
documented maximum plus terminator allowance, not unbounded
`read_line` growth.

The full I2P Streaming byte round-trip over local TCP for the
IRC client profile is the same Plan 180 reconcile pass that
generalizes the per-destination runtime driver to service
tunnels; Plan 178 does not silently weaken that criterion.

## Explicit not-yet-implemented product rows

| Product | Status | Owning plan |
| --- | --- | --- |
| Generic TCP client listener | passed-experimental-loopback-only | 175 |
| Generic TCP server destination + target | passed-experimental-loopback-only | 175 |
| Persistent server destination storage | passed-experimental-secret-safe | 175 |
| HTTP `.i2p` proxy + CONNECT | passed-experimental-loopback-only | 176 |
| SOCKS5 no-auth `.i2p` CONNECT | passed-experimental-loopback-only | 177 |
| IRC client privacy filter | passed-experimental-loopback-only | 178 |
| IRC server authenticated hostname | passed-experimental-loopback-only | 179 |
| Full composition / reconcile / hardening | not-yet-implemented | 180 |
| Independent acceptance / final closure | not-yet-implemented | 181 |

No row above may be marked passed until its owning plan has an
explicit passing status record with command-derived evidence.

## Evidence (Plan 178)

- `crates/i2pr-service-tunnels/src/irc/` (line, tags, policy,
  client_filter, config, errors, limits; `#![forbid(unsafe_code)]`,
  runtime-neutral; unit tests covering exact-maximum and `+1`
  ceilings, tag/count/key/data bounds, command classification,
  per-direction allowlist, tag-bypass attempts, USER rewrite,
  PING rewrite + PONG token accounting, QUIT/PART policy, CTCP
  ACTION/DCC/VERSION/malformed cases, incremental fragmentation,
  coalesced lines, sibling-state isolation).
- `crates/i2pr-service-tunnels/src/lib.rs` re-exports the
  IRC module types (`IrcClientOptions`, `IrcCommand`,
  `IrcCommandClass`, `IrcDropReason`, `IrcError`, `IrcLimits`,
  `IrcLineParser`, `LineDirection`, `LineParserOutcome`,
  `ParsedLine`, `PingRewriteState`, `ReasonRewritePolicy`,
  `TagsParser`).
- `crates/i2pr-service-tunnels/src/config.rs` accepts
  `ServiceTunnelSpec.irc_options`; only `IrcClient` carries
  options and the unit test confirms non-IRC kinds reject any
  options.
- `crates/i2pr-daemon/src/service_tunnels_irc_client.rs` (IRC
  supervisor + per-connection executor; per-listener `permit`
  budget; bounded line read deadlines; bounded cleanup on
  EOF/reset/cancel).
- `crates/i2pr-daemon/src/service_tunnels.rs`
  (`is_irc` flag on `ServiceRuntime`; IRC-specific dispatch in
  `run_service_loop`).
- `crates/i2pr-daemon/src/config.rs` (`[service_tunnels]`
  accepts `enabled = true` for `generic-client`,
  `generic-server`, `http-client`, `socks5-client`, and now
  `irc-client`; `irc-server` remains rejected as
  not-yet-available).
- `crates/i2pr-daemon/tests/service_tunnel_irc_client_product.rs`
  (18 black-box tests: listener accept, unknown-command drop,
  sibling isolation, snapshot accounting, overlong core/tag
  rejection, fragmented/coalesced lines, CTCP ACTION pass,
  CTCP DCC drop, USER rewrite path, tagged message path,
  registration/CAP-SASL/JOIN/PRIVMSG/NOTICE path, slowloris
  boundedness, aggregate ceiling).
- `crates/i2pr-daemon/tests/service_tunnels_foundation.rs`
  (Plan 174 black-box config/graph tests, updated to confirm
  the Plan 178 `irc-client` acceptance rule and the remaining
  `irc-server` rejection).
- `scripts/check-dependency-direction.sh` (workspace graph
  unchanged) and `scripts/check-runtime-boundaries.sh`
  (`i2pr-service-tunnels::irc` remains runtime-neutral).
- `plans/178-status.md` (exact evidence,
  `next_executable_plan = 179`).

## Evidence (Plan 176)

- `crates/i2pr-service-tunnels/src/http/` (parser, target,
  rewrite, response, config, error, limits; `#![forbid(unsafe_code)]`,
  runtime-neutral; 41 runtime-neutral unit tests covering parse
  round-trip, target validation, hop-by-hop removal, privacy
  rewrite, error response generation, and limits).
- `crates/i2pr-service-tunnels/src/lib.rs` re-exports the
  HTTP module types (`HttpClientOptions`, `PrivacyPolicy`,
  `UserAgentPolicy`, `parse_request_head`, `rewrite_headers`,
  `build_error_response`, `parse_request_target`,
  `parse_authority_form`, `HttpLimits`, `HttpError`).
- `crates/i2pr-service-tunnels/src/config.rs` accepts
  `ServiceTunnelSpec.http_options`; only `HttpClient` carries
  options and the unit test confirms non-HTTP kinds reject any
  options.
- `crates/i2pr-daemon/src/service_tunnels_http.rs` (HTTP
  supervisor + per-connection executor; per-listener
  `permit` budget; bounded header read deadline; bounded CONNECT
  port policy; typed error responses on 400/403/502; bounded
  cleanup on EOF/reset/cancel).
- `crates/i2pr-daemon/src/service_tunnels.rs`
  (`is_http` flag on `ServiceRuntime`; HTTP-specific dispatch in
  `run_service_loop`).
- `crates/i2pr-daemon/src/config.rs` (`[service_tunnels]`
  accepts `enabled = true` for `generic-client`,
  `generic-server`, and `http-client`; SOCKS/IRC remain rejected
  as not-yet-available).
- `crates/i2pr-daemon/tests/service_tunnel_http_product.rs`
  (15 black-box tests: clearnet/IP/localhost/mixed-suffix
  rejection, non-`http` scheme/userinfo/smuggling rejection,
  unknown `.i2p` 502, CONNECT-with-disallowed-port 403,
  CONNECT-without-port 400, HTTP/1.0 rejection, sibling isolation,
  slow-incomplete-header timeout, snapshot accounting, default
  privacy-policy port policy).
- `crates/i2pr-daemon/tests/service_tunnels_foundation.rs`
  (Plan 174 + 175 graph/config regression updated to confirm the
  Plan 176 `http-client` acceptance rule).
- `scripts/check-dependency-direction.sh` (workspace graph
  unchanged) and `scripts/check-runtime-boundaries.sh`
  (`i2pr-service-tunnels::http` remains runtime-neutral).
- `plans/176-status.md` (exact evidence,
  `next_executable_plan = 177`).

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
  SOCKS/IRC remain rejected).
- `crates/i2pr-daemon/tests/service_tunnel_generic_product.rs`
  (9 black-box manager tests: prepare/restart-stable identity,
  corrupt-identity rejection, missing-target rejection,
  Unix-target not-yet-supported, no HTTP/SOCKS/IRC leak,
  cross-tunnel local destination lookup, snapshot accounting,
  disabled-service ignored).
- `crates/i2pr-daemon/tests/service_tunnels_foundation.rs`
  (Plan 174 graph/config regression suite).
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
