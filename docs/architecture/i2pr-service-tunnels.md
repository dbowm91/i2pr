# `i2pr-service-tunnels` — Deep Dive

Runtime-neutral Milestone 10 service-tunnel configuration,
destination reference policy, typed errors, typed events/snapshots, plus
the Plan 175 generic client/server tunnel composition surface owned by
the daemon, the Plan 176 HTTP/1.1 proxy parser/rewrite/target-validation
surface, the Plan 177 SOCKS5 no-auth CONNECT proxy
negotiation/request/reply surface, the Plan 178 IRC
client line-parser/tag/classifier/filter surface, the Plan 179
IRC server registration interceptor plus the authenticated peer
Destination hash projection, and the Plan 180 generation diff
classification surface.

Path:
- `crates/i2pr-service-tunnels/` (configuration + reference parsing + HTTP + SOCKS5 + IRC client/server).
- `crates/i2pr-storage/src/service_destination.rs` (persistent server destinations).
- `crates/i2pr-daemon/src/service_tunnels.rs` (manager + listener runtime).
- `crates/i2pr-daemon/src/service_tunnels_http.rs` (HTTP proxy executor).
- `crates/i2pr-daemon/src/service_tunnels_socks5.rs` (SOCKS5 proxy executor).
- `crates/i2pr-daemon/src/service_tunnels_irc_client.rs` (IRC client tunnel executor).
- `crates/i2pr-daemon/src/service_tunnels_irc_server.rs` (IRC server tunnel executor).

## Purpose

Plan 175 lands the first complete Milestone 10 application service
product; Plan 176 adds the first M10 application profile; Plan 177 adds
the second; Plan 178 adds the third; Plan 179 adds the
fourth; Plan 180 adds the runtime-neutral diff classification
that backs the daemon-side transactional reconcile. The crate
builds on the Plan 174 foundation
(`ServiceTunnelSet`, `LocalListenerSpec`, `ServerTarget`, bounded
resource/timeouts) and adds:

- bounded typed identifiers and service kinds;
- strict destination reference policy (Base32 / static alias /
  configured public material);
- loopback-only listener/target shapes;
- central resource and deadline ceilings;
- validated service-tunnel sets;
- typed errors and events carrying no sockets or secrets;
- the Plan 176 HTTP/1.1 parser, hop-by-hop/privacy rewrite,
  `.i2p`-only target validation, and bounded error-response
  surface;
- the Plan 177 RFC 1928 SOCKS5 greeting + CONNECT request
  parser, `.i2p`/DOMAINNAME-only target policy, deterministic
  RFC 1928 reply generator with neutral loopback bind, and
  bounded typed errors;
- the Plan 178 IRC/IRCv3 line parser, message-tag framing,
  typed command classifier with per-direction allowlist,
  client-to-network privacy rewrites, and CTCP/DCC policy;
- the Plan 179 IRC server registration interceptor, bounded
  pre-registration line / byte ceilings, cross-protocol
  detection, the authenticated peer Destination hash projection
  to a 52-character `.b32.i2p` hostname, IRCv3 tagged USER
  rewrite, and the typed `RegistrationOutcome` handoff contract;
- a versioned, atomic, secret-safe persistent service-destination
  storage seam (Plan 175);
- a daemon-owned manager that wires loopback TCP, I2P Streaming, and
  the existing Plan 149 local destination product path together
  (Plan 175 + Plan 176 + Plan 177 + Plan 178 + Plan 179);
- the runtime-neutral `DiffClass` typed generation diff classification
  (`Unchanged`, `MutableInPlace`, `ReplaceListener`,
  `ReplaceDestination`, `Remove`, `Add`) and the `diff_sets` /
  `diff_spec` helpers used by the daemon-side reconcile algorithm
  (Plan 180).

It must not own:

- Tokio, sockets, listeners, tasks, timers;
- filesystem access (lives in `i2pr-storage`);
- transport or tunnel-build internals;
- NetDB mutation;
- Garlic/I2NP construction;
- SAM or I2CP protocol parsing.

The daemon remains the sole M10 socket/task/composition owner.
Milestone 10 independent acceptance is owned by Plan 181 (29 local
rows passed; 2 remote rows blocked on the retained M6
mixed-router Streaming debt; Plan 183 owns the corrective
program). Plans 180-and-182 jointly close the M10 local product +
round-trip layer (reconcile model + local-delivery driver).

## Module layout

| Module | File | Responsibility | Key public types |
| --- | --- | --- | --- |
| `lib` | `crates/i2pr-service-tunnels/src/lib.rs` | Crate root, re-exports, architecture pointer | `ServiceTunnelId`, `DestinationRef`, `ServiceTunnelError`, HTTP re-exports |
| `config` | `crates/i2pr-service-tunnels/src/config.rs` | Typed kinds, policy, listener/target, limits, timeouts, set validation | `ServiceTunnelKind`, `DestinationPolicy`, `LocalListenerSpec`, `ServerTarget`, `ServiceResourceLimits`, `ServiceTimeouts`, `ServiceTunnelSpec`, `ServiceTunnelSet` |
| `generation` | `crates/i2pr-service-tunnels/src/generation.rs` | Plan 180 runtime-neutral generation diff model | `DiffClass`, `ServiceDiff`, `diff_sets`, `diff_spec`, `kind_string` |
| `destination` | `crates/i2pr-service-tunnels/src/destination.rs` | Base32/alias/configured parsing, alias table | `DestinationRef`, `StaticAliasTable` |
| `errors` | `crates/i2pr-service-tunnels/src/errors.rs` | Typed structural errors, no secrets | `ServiceTunnelError` |
| `events` | `crates/i2pr-service-tunnels/src/events.rs` | Value-only lifecycle events and snapshots | `ServiceTunnelEvent`, `ServiceTunnelSnapshot` |
| `http` | `crates/i2pr-service-tunnels/src/http/` | Plan 176 runtime-neutral HTTP/1.1 parser, target validator, hop-by-hop/privacy rewrite, bounded error response | `HttpLimits`, `HttpClientOptions`, `PrivacyPolicy`, `UserAgentPolicy`, `HttpRequestHead`, `RequestTarget`, `parse_request_head`, `rewrite_headers`, `build_error_response` |
| `socks5` | `crates/i2pr-service-tunnels/src/socks5/` | Plan 177 runtime-neutral RFC 1928 no-auth greeting + CONNECT request parser, `.i2p`/DOMAINNAME-only target policy, deterministic reply generator | `Socks5Limits`, `Socks5ClientOptions`, `ConnectPortPolicy`, `GreetingParser`, `RequestParser`, `ConnectDestination`, `Socks5Error`, `Socks5ErrorKind`, `Socks5ReplyCode`, `build_socks5_reply` |
| `irc` | `crates/i2pr-service-tunnels/src/irc/` | Plan 178 runtime-neutral IRC/IRCv3 line parser, tag framing, command classifier + per-direction allowlist, USER/PING/QUIT/PART rewrites, CTCP/DCC policy; plus the Plan 179 server registration interceptor, authenticated peer Destination hash projection, and the typed `RegistrationOutcome` handoff contract | `IrcLimits`, `IrcClientOptions`, `ReasonRewritePolicy`, `IrcCommand`, `IrcCommandClass`, `LineDirection`, `ParsedLine`, `FilterOutcome`, `IrcDropReason`, `IrcLineParser`, `LineParserOutcome`, `PingRewriteState`, `TagsParser`, `IrcError`, `IrcErrorKind`, `IrcServerOptions`, `IrcServerRegistration`, `RegistrationOutcome`, `RegistrationRejection`, `RegistrationState`, `project_peer_hostname` |
| `service_destination` | `crates/i2pr-storage/src/service_destination.rs` | Versioned, atomic, secret-safe persistent service destination storage | `ServiceDestinationStore`, `ServiceDestinationRecord`, `ServiceDestinationStorageError` |
| `service_tunnels` | `crates/i2pr-daemon/src/service_tunnels.rs` | Generic client/server tunnel composition root + Plan 180 generation/reconcile/draining | `ServiceTunnelManager`, `ServiceTunnelManagerConfig`, `ServiceRuntime`, `ServiceTunnelSnapshot`, `ClientTarget`, `DestinationFailure`, `ReconcileOutcome`, `ReapReport`, `GenerationSnapshot`, `StagedRuntime` |
| `service_generation` | `crates/i2pr-daemon/src/service_generation.rs` | Plan 180 committed-generation bookkeeping | `ServiceTunnelGeneration`, `DrainingGeneration`, `GenerationCounters`, `GenerationIdAllocator`, `DestinationResolution` |
| `service_tunnels_http` | `crates/i2pr-daemon/src/service_tunnels_http.rs` | Plan 176 HTTP client tunnel executor (supervisor + per-connection handler) | `HttpConnectionOutcome`, `run_http_connection`, `run_http_client_loop` |
| `service_tunnels_socks5` | `crates/i2pr-daemon/src/service_tunnels_socks5.rs` | Plan 177 SOCKS5 client tunnel executor (supervisor + per-connection handler) | `Socks5ConnectionOutcome`, `run_socks5_connection`, `run_socks5_client_loop` |
| `service_tunnels_irc_client` | `crates/i2pr-daemon/src/service_tunnels_irc_client.rs` | Plan 178 IRC client tunnel executor (supervisor + per-connection handler) | `IrcConnectionOutcome`, `run_irc_connection`, `run_irc_client_loop` |
| `service_tunnels_irc_server` | `crates/i2pr-daemon/src/service_tunnels_irc_server.rs` | Plan 179 IRC server tunnel executor (supervisor + per-connection handler with registration interception, target connect, and raw pump handoff) | `IrcServerConnectionOutcome`, `InterceptionResult`, `InterceptionSource`, `StreamingInterceptionSource`, `ChannelInterceptionSource`, `run_irc_server_loop`, `intercept_registration` |

Line counts (approximate at Plan 179 close): see files.

## Public surface

```text
i2pr_service_tunnels:
  pub use config::{ServiceTunnelId, ServiceClientGroupId, ServiceTunnelKind,
    DestinationPolicy, LocalListenerSpec, ServerTarget, ServiceResourceLimits,
    ServiceTimeouts, ServiceTunnelSpec, ServiceTunnelSet, MAX_*};
  pub use destination::{DestinationRef, StaticAliasTable};
  pub use errors::ServiceTunnelError;
  pub use events::{ServiceTunnelEvent, ServiceTunnelSnapshot};
  pub use generation::{DiffClass, ServiceDiff, diff_sets, diff_spec, kind_string};
  pub use http::{HeaderEntry, HeaderName, HttpClientOptions, HttpError,
    HttpErrorKind, HttpLimits, HttpRequestHead, ParseError, PrivacyPolicy,
    RequestLine, RequestTarget, TargetKind, TargetParseError, UserAgentPolicy,
    build_error_response, parse_authority_form, parse_request_head,
    parse_request_target, rewrite_headers};
  pub use socks5::{ConnectDestination, ConnectPortPolicy, GreetingOutcome,
    GreetingParser, RequestOutcome, RequestParser, Socks5ClientOptions,
    Socks5Error, Socks5ErrorKind, Socks5Limits, Socks5ReplyCode,
    build_socks5_reply, build_socks5_reply_from_code};
  pub use irc::{IrcClientOptions, IrcCommand, IrcCommandClass,
    IrcDropReason, IrcError, IrcErrorKind, IrcLimits, IrcLineParser,
    IrcServerOptions, IrcServerRegistration, IrcTag, LineDirection,
    LineParserOutcome, ParsedLine, PingRewriteState, PrivacySubstitutions,
    ReasonRewritePolicy, RegistrationOutcome, RegistrationRejection,
    RegistrationState, TagsOutcome, TagsParser, classify_irc_core,
    classify_post_tag_core, encode_b32_label, is_irc_command_allowed,
    is_irc_command_allowed_alias, project_peer_hostname};

i2pr_storage:
  pub ServiceDestinationStore, ServiceDestinationRecord,
  ServiceDestinationStorageError, decode_service_destination_bytes,
  MAX_SERVICE_DESTINATION_FILE_SIZE,
  SERVICE_DESTINATION_FILE_NAME, SERVICE_DESTINATIONS_SUBDIR,
  SERVICE_DESTINATION_FORMAT_VERSION.

i2pr_daemon::service_tunnels:
  pub ServiceTunnelManager, ServiceTunnelManagerConfig, ServiceRuntime,
  ServiceTunnelSnapshot, ClientTarget, DestinationFailure,
  ReconcileOutcome, ReapReport, GenerationSnapshot, StagedRuntime,
  register_service_tunnel_manager.

i2pr_daemon::service_generation:
  pub ServiceTunnelGeneration, DrainingGeneration, GenerationCounters,
  GenerationIdAllocator, DestinationResolution.

i2pr_daemon::service_tunnels_http:
  pub HttpConnectionOutcome, run_http_connection, run_http_client_loop.

i2pr_daemon::service_tunnels_socks5:
  pub Socks5ConnectionOutcome, run_socks5_connection, run_socks5_client_loop.

i2pr_daemon::service_tunnels_irc_client:
  pub IrcConnectionOutcome, run_irc_connection, run_irc_client_loop.

i2pr_daemon::service_tunnels_irc_server:
  pub IrcServerConnectionOutcome, InterceptionResult, InterceptionSource,
  StreamingInterceptionSource, ChannelInterceptionSource,
  IrcServerOptions, IrcServerRegistration, RegistrationOutcome,
  RegistrationRejection, RegistrationState, intercept_registration,
  project_peer_hostname, run_irc_server_loop.
```

## Key contracts

### `i2pr-service-tunnels` (Plan 174 foundation, retained)

- `#![forbid(unsafe_code)]` in every module.
- Every count/length/deadline has a hard typed ceiling:
  32 services, 64 aliases, 64-byte IDs, 128 conns per service,
  1024 aggregate, 1024-1048576 buffered bytes per direction,
  8 configured targets, connect 1-120 s, read/write 1-600 s,
  shutdown 1-30 s, 52-char Base32, 67-byte alias, 255-byte Unix path.
- `ServiceTunnelSet::validate()` rejects duplicate IDs, duplicate
  binds, contradictory options, and aggregate overflow before daemon
  state changes.
- `DestinationRef::parse()` is structural only: no DNS, filesystem,
  network, or clearnet fallback; IP literals rejected.
- `StaticAliasTable` rejects duplicates, conflicts, malformed
  targets, and ceiling overflow.
- Client kinds require listener + destination and forbid server
  targets; server kinds require target(s), forbid listener and remote
  destination, and require dedicated policy.
- `ServiceTunnelSpec::http_options` is mandatory for `HttpClient`
  and rejected for every other kind.
- `ServiceTunnelSpec::socks5_options` is mandatory for
  `Socks5Client` and rejected for every other kind.
- `ServiceTunnelSpec::irc_options` is mandatory for `IrcClient`
  and rejected for every other kind.
- `ServiceTunnelSpec.kind == IrcServer` reuses the Plan 175
  persistent server destination storage so restart preserves the
  public service Destination.

### Plan 176 HTTP runtime-neutral module

- Hard ceilings: request-line 8192 bytes, total header bytes
  65536, header count 100, field name 256 bytes, field value
  8192 bytes, CONNECT authority 512 bytes, retained buffer 65536,
  generated error 1024 bytes.
- Parser rejects: NUL/CR/LF/obs-fold/CRLF ambiguities, host
  smuggling (conflicting `Content-Length`, `Transfer-Encoding` +
  `Content-Length`, GET/HEAD with framing, duplicate `Host` with
  conflicting authority), overlong fields under deadline, methods
  outside uppercase ASCII, non-`HTTP/1.1` versions.
- Target validator rejects: non-`http` schemes, userinfo, IP
  literals, `localhost`/`.localhost`, mixed-suffix confusion
  (`*.i2p.example`), empty CONNECT ports, port 0, malformed/empty
  authority. `.b32.i2p` references pass; canonical `.i2p` aliases
  pass the strict static-alias grammar.
- Rewrite policy: parses the `Connection` value list and removes
  every header named by it (case-insensitive); removes
  `Proxy-Connection`, `Keep-Alive`, `TE`, `Trailer`, `Upgrade`,
  `Transfer-Encoding`; forces `Connection: close`; normalizes
  `Host` from the target authority; strips `Via`, `Forwarded`,
  `X-Forwarded-{For,Host,Proto}`, `Proxy-Authorization`,
  optionally `Referer`, `From`; `User-Agent` keep/strip/replace
  with the stable `i2pr/0.1` value.
- Error response: bounded bytes, sanitized `X-HTTP-Proxy-Reason`
  header (CR/LF/control bytes stripped), never echoes request
  bytes.

### Plan 175 generic client/server execution

Plan 175 enables `enabled = true` for `generic-client` and
`generic-server`. Plan 176 adds `http-client`. Plan 177 adds
`socks5-client`. Plan 178 adds `irc-client`. Plan 179 adds
`irc-server`. Every current kind is accepted after its plan
lands; there is no remaining not-yet-available gate (a
hypothetical future kind remains rejected until its plan lands).

### Plan 177 SOCKS5 runtime-neutral module

- Hard ceilings: method count 16, greeting bytes 32, request
  header bytes 32, domain length 255, retained buffer 320,
  generated reply bytes 64.
- Greeting parser: VER must be `0x05`; `NMETHODS` must be
  `1..=method_count_max`; method list must contain `0x00 NO
  AUTHENTICATION REQUIRED`; `0x02 username/password` is never
  accepted even when offered.
- Request parser: rejects NUL/control/whitespace in domain
  during accumulation (structural failure), BIND/UDP
  ASSOCIATE/unknown commands (`0x07`), IPv4/IPv6 (`0x08`),
  clearnet/IP literal/localhost/mixed-suffix/malformed alias
  (`0x02`), zero-length domain and zero port (`0x01`).
- Reply: deterministic 10-byte RFC 1928 reply with
  `BND.ADDR=127.0.0.1`, `BND.PORT=0`; never echoes untrusted
  request bytes or destination private material.
- CONNECT port policy: `ConnectPortPolicy::default()` permits
  only port 443; configurable per-service.
- Allowed-host policy: `Socks5ClientOptions::allowed_hosts` may
  pin a bounded list of `.i2p` hosts (validated by the strict
  static-alias grammar).

### Plan 178 IRC runtime-neutral module

- Hard ceilings: core line 512 bytes, tag envelope 8191 bytes,
  client tag data 4094 bytes, per-direction line buffer 8192
  bytes, generated line 8192 bytes, tag count 128, tag key 64
  bytes. Overlong lines are dropped without truncation, never
  split into a syntactically different valid command.
- Tag framing: structural IRCv3 envelope (`@tag …` + separating
  space) with opaque tag names/values, invalid escapes
  rejected, per-key/count/data ceilings enforced; tag presence
  never bypasses command classification and core/tag limits are
  enforced separately.
- Command classifier: explicit per-direction allowlist covering
  `PASS CAP AUTHENTICATE NICK USER PING PONG JOIN PART QUIT
  PRIVMSG NOTICE MODE TOPIC AWAY NAMES LIST WHO WHOIS WHOWAS
  ISON INVITE KICK USERHOST SERVER`, numeric replies, and
  documented server-originated commands (`PING MODE JOIN NICK
  QUIT PART KICK TOPIC CAP AUTHENTICATE ACCOUNT CHGHOST
  ERROR`). Unknown/unclassified commands are dropped, never
  passed.
- Privacy rewrites: `USER` hostname/servername replaced with
  stable non-identifying placeholders; location-bearing `PING`
  rewritten with one bounded per-connection outstanding PONG
  token (a new rewrite deterministically replaces the old
  one); `QUIT`/`PART` reasons pass unchanged by default with a
  named opt-in stable replacement.
- CTCP/DCC policy: `ACTION` passes; malformed/multi-delimiter
  messages, address-bearing `DCC`, and unsupported CTCP are
  dropped. No DCC helper tunnels.
- Options: `IrcClientOptions { allowed_hosts,
  reason_rewrite, user_realname_max_bytes }`; reason rewrite
  defaults to `Keep`.
- Errors carry kinds + machine-readable reasons only; no
  secrets, no nicknames, no message text.

### Plan 179 IRC server runtime-neutral module

- Hard ceilings: pre-registration lines default 10 with a hard
  maximum of 64; cumulative pre-registration bytes default
  to the Plan 178 IRC line-buffer ceiling (8192); the
  pre-registration command allowlist (`PASS CAP AUTHENTICATE
  NICK`) rejects any line whose command name contains
  non-uppercase ASCII, is empty, or exceeds 16 bytes.
- Cross-protocol rejection: the first observed line is checked
  against a small fixed list (`GET / POST / HEAD / PUT /
  DELETE / OPTIONS / CONNECT / TRACE / PATCH ` and the
  BitTorrent handshake magic); non-empty matches reject the
  registration as `CrossProtocol`.
- Authenticated peer hostname projection:
  `project_peer_hostname(&peer_destination_hash)` returns a
  52-character lower-case I2P Base32 label plus `.b32.i2p`,
  computed once at construction time from the 32-byte
  Streaming peer destination hash. The projection is the
  only acceptable source for the post-rewrite USER hostname
  argument.
- `USER` rewriting: the registered `USER` line is rewritten so
  the second argument is replaced with the projected hostname;
  the username, servername, realname, and (for RFC 1459) mode
  parameter are preserved within the 512-byte core ceiling. A
  rewrite that would push the projected line past the ceiling
  is rejected as `CoreLineTooLong`, never truncated.
- IRCv3 tag envelopes are preserved verbatim on the rewritten
  `USER` (and `SERVER`) line so the local IRC target observes
  the original tagged message.
- `SERVER` (server-to-server IRC) is accepted as a handoff
  line; the line is passed through verbatim (the connecting
  server name is not a per-user identity).
- Pre-registration commands are passed through verbatim to the
  local target; they accumulate in a per-interceptor buffer
  that is concatenated with the rewritten `USER` line at
  handoff time.
- Same-read post-USER bytes (any bytes that follow the
  terminating CRLF in the same read after the `USER` line)
  are preserved verbatim as the first raw-pump bytes.
- Typed `RegistrationOutcome::{Incomplete { retained },
  Ready { prefix, leftover }, Rejected(reason), Eof}` so the
  daemon executor can drive one-shot prefix + leftover
  handoff to the loopback target.
- Typed rejection reasons: `TooManyLines`, `BufferOverflow`,
  `CrossProtocol`, `UnknownCommand`, `InvalidLine`,
  `InvalidUser`, `InvalidServer`, `CoreLineTooLong`.

### Persistent server destinations (`i2pr-storage`)

The Plan 175 persistent service destination format is independent of
Rust layout and serde:

| Region | Size | Contents |
| --- | ---: | --- |
| Magic | 8 | `I2PRSD\0\0` |
| Header | 16 | version (1), reserved (0), signing algorithm (7 = Ed25519), static algorithm (4 = X25519) |
| Payload | 448 | Ed25519 seed, X25519 static secret, derived signing public key, derived X25519 public key, destination padding |
| Integrity | 32 | SHA-256 over header+payload |

Version 1 accepts only Ed25519 signing (type 7) and X25519 static
keys (type 4). The format:

- is integrity-protected by SHA-256;
- never accepts truncation, trailing bytes, unsupported versions,
  checksum changes, or derived public-key mismatches;
- is permission-hardened (file `0o600`, directory `0o700`);
- is atomic and no-replace (same-directory temporary + `hard_link`
  install; `AlreadyExists` is fail-closed);
- secret material is non-`Clone`, redacted `Debug`, and zeroized on
  drop.

A reload/reconcile never generates a new identity because a
component restart occurred. Corruption or wrong-version files fail
closed; identity rotation is never a side effect of reload.

### Service tunnel manager (`i2pr-daemon`)

- `ServiceTunnelManager::new(config)` builds a manager from a
  validated `ServiceTunnelSet` and `StaticAliasTable` plus the router
  data directory.
- `prepare()` builds each enabled service's destination runtime,
  binds loopback TCP listeners for client tunnels, and installs the
  per-service Streaming listener for server tunnels. The first call
  seeds the authoritative committed generation
  (`ServiceTunnelGeneration`).
- `start_supervisors(runtimes, children, cancellation)` spawns the
  per-service supervisor loops under the daemon's child scope,
  plus one supervised per-destination local-delivery driver per
  runtime (Plan 182; fail-closed when the child scope rejects
  the spawn).
- `shutdown()` cancels every per-service supervisor token and
  every delivery-driver token (counter entries retained).
- Plan 182 delivery substrate: per-destination outbound `Notify`
  signals (`outbound_signal` / `notify_outbound_signal`),
  cumulative `DeliverySweepCounters` (`delivery_counters`),
  and `deliver_outbound` sweeping the manager-level
  `sam_destinations` mirror through the public Plan 129
  `bridge_to_peer` seam (sender LeaseSet2 install, peer
  inbound-tunnel build, typed counters, failed-delivery
  termination). Stale drivers are cancelled on reconcile;
  the sweep covers both canonical and receiver-mirror queues.
- Plan 182 Streaming conventions: server tunnels listen on
  wildcard port 0 (SAM convention; clients connect `(0, 0)`),
  accepts use the connection's real authenticated peer/ports
  with the SYN response queued for the driver, and
  `ServicePumpEndpoint::try_send` branches on direction with
  typed backpressure matching (never Display-string matching).
- `lookup_local_service_destination(hash)` resolves a Base32 hash to
  a [`ClientTarget`] when the hash matches a service destination
  registered with the manager (cross-tunnel local delivery).
- `service_destination_b64(service_id)` returns the canonical SAM
  private-destination wrapper base64 for one service.
- `snapshot()` returns a sanitized accounting view (counts and
  identifiers only; no secrets).
- Plan 180 transactional reconcile: `reconcile(candidate,
  drain_deadline) -> ReconcileOutcome { generation_id, diff,
  draining_ids, drain_deadline }` validates the candidate,
  computes the typed `DiffClass` diff against the committed
  generation, stages every `Add` / `Replace*` entry, then
  atomically publishes the new generation and pushes only
  replaced/removed old runtimes onto the draining list under a
  hard deadline. `committed_generation_id`,
  `draining_generation_count`, `reap_expired_drains -> ReapReport`,
  and `generation_snapshot -> GenerationSnapshot` complete the
  unified cross-service resource accounting surface.

The manager never logs private destination material, signing seeds,
static secrets, raw payloads, or base64 of the private destination.

### Plan 176 HTTP proxy executor (`i2pr-daemon`)

- Per-connection loop: read HTTP/1.1 header section under a
  30 s deadline; parse; dispatch.
- CONNECT: validate the port against `HttpClientOptions::privacy
  .connect_allowed_ports` (default `{443}`), resolve the
  authority via the manager, open I2P Streaming, write a 2xx
  response with no `Content-Length`/`Transfer-Encoding`, then run
  the shared Plan 174 byte pump in blind bidirectional mode.
- Ordinary proxy: validate the absolute-form target host is `.i2p`,
  resolve via the manager, open I2P Streaming, write the rewritten
  request-line + headers (with the original same-read body bytes
  preserved as `initial_body`), then run the shared pump in
  body/response mode.
- Cleanup: Streaming CLOSE on a clean exit, RESET on pump error,
  `Connection: close` plus half-close shutdown on the local socket
  when the proxy emits a bounded error response.
- Headers/state: same per-listener `permit` budget and
  `ActiveConnections` / `FailedConnects` accounting as the generic
  client tunnels.

### Plan 177 SOCKS5 proxy executor (`i2pr-daemon`)

- Per-connection loop: read greeting under a 30 s deadline,
  negotiate no-auth, read CONNECT request under a 30 s deadline.
- Validation: CONNECT port against
  `Socks5ClientOptions::port_policy.connect_allowed_ports` (default
  `{443}`); reject with `0x02` (ConnectionNotAllowed) when outside
  policy.
- Resolution: `DestinationRef::parse` of the `.i2p` host plus
  `ServiceTunnelManager::resolve_reference` for Base32 / alias /
  local-delivery path.
- Streaming: open I2P Streaming, wait for `Established`, send the
  SOCKS5 success reply (`0x00`, `BND.ADDR=127.0.0.1`,
  `BND.PORT=0`), then run the shared Plan 174 byte pump in opaque
  tunnel mode with same-read post-request bytes preserved as the
  first tunnel bytes.
- Cleanup: Streaming CLOSE on a clean exit, RESET on pump error,
  `Connection: close` plus half-close shutdown on the local socket
  when the proxy emits a bounded SOCKS5 reply.
- State: same per-listener `permit` budget and `ActiveConnections`
  / `FailedConnects` accounting as the generic client tunnels.

### Plan 178 IRC client executor (`i2pr-daemon`)

- Per-connection loop: read IRC lines under a 30 s deadline,
  run the per-direction incremental line parser (one bounded
  partial-line buffer per side), consume runtime-neutral
  `Allow` / `Rewrite` / `Drop` filter decisions.
- Privacy: `USER`/`PING`/`QUIT`/`PART` rewrites and CTCP/DCC
  policy are owned by the runtime-neutral filter; the executor
  never duplicates command policy inline and never logs
  usernames, realnames, nicknames, or message text.
- Streaming: open I2P Streaming to the configured fixed I2P
  IRC destination after the filter pass, then run the shared
  Plan 174 byte pump in line-aware mode. Target selection never
  depends on IRC command contents.
- Cleanup: Streaming CLOSE on a clean exit, RESET on pump
  error, filter buffers released after EOF/cancel/remote
  close; sibling connections keep independent PONG/filter
  state.
- State: same per-listener `permit` budget and
  `ActiveConnections` / `FailedConnects` accounting as the
  generic client tunnels.

### Plan 179 IRC server executor (`i2pr-daemon`)

- Per-connection loop: the supervisor accepts inbound Streaming
  SYNs through the Plan 175 persistent server destination
  listener, waits up to 15 s for the connection to reach
  `Established`, captures the peer destination hash from
  authenticated Streaming metadata, then drives the
  registration interception phase under a 30 s total deadline
  with a 20 ms poll cadence.
- Authentication: the projected hostname is taken from
  `peer_destination_hash()` on the established streaming
  connection; the runtime-neutral interceptor rejects the
  registration if no peer hash is available, so application
  bytes never influence the projected identity.
- Target connect: connects to the configured loopback TCP
  target under a 10 s deadline; on failure emits at most one
  bounded IRC-style failure to the remote peer where safe
  and closes/reset without leaking.
- Handoff: writes the registration prefix + leftover exactly
  once to the loopback target, then switches permanently to
  the shared Plan 174 byte pump in opaque mode for the
  post-registration stream. After handoff, the post-handler
  is byte-transparent; no second ongoing IRC filter is
  layered on the server tunnel.
- Cleanup: Streaming CLOSE on a clean exit, RESET on pump
  error, registration buffers released after EOF/cancel/
  remote close; sibling connections keep independent
  per-connection state.
- State: same per-listener `permit` budget and
  `ActiveConnections` / `FailedConnects` accounting as the
  generic client/server tunnels. `irc-server` services use
  the Plan 175 persistent server destination storage so
  restart preserves the projected hostname algorithm.

## Dependencies

From `Cargo.toml`:
- `i2pr-service-tunnels` depends on `i2pr-proto` + `thiserror`.
- `i2pr-daemon` depends on `i2pr-service-tunnels` for the typed spec
  surface and on the SAM bridge infrastructure for destination
  composition.
- `i2pr-storage` adds `service_destination` module depending on
  `i2pr-crypto` (keys, hashing).

From `scripts/check-dependency-direction.sh`: the workspace graph is
unchanged. From `scripts/check-runtime-boundaries.sh`:
`i2pr-service-tunnels` (including the Plan 176 `http`, Plan 177
`socks5`, Plan 178 `irc` client, and Plan 179 `irc::server`
sub-modules) is runtime-neutral (`#![forbid(unsafe_code)]`, no
Tokio, sockets, listeners, tasks, or timers); the daemon owns
all M10 sockets and tasks.

## Tests

- `crates/i2pr-service-tunnels/src/http/parser.rs` unit tests:
  minimal GET round-trip, malformed obs-fold, lone LF, bare CR,
  conflicting `Content-Length`, `Transfer-Encoding` +
  `Content-Length`, GET with framing, duplicate Host, control-byte
  values, request-line length ceiling, header-count ceiling.
- `crates/i2pr-service-tunnels/src/http/target.rs` unit tests:
  absolute-form with/without port, non-`http` schemes, userinfo,
  clearnet/IP/localhost/mixed-suffix rejection, CONNECT
  authority-form with/without port, CONNECT-with-zero-port,
  CONNECT-overlong authority, canonical authority + origin-form
  rendering.
- `crates/i2pr-service-tunnels/src/http/rewrite.rs` unit tests:
  connection-nominated removal, hop-by-hop removal, host
  normalization, forced `Connection: close`, privacy headers
  stripped, `User-Agent` keep/strip/replace-stable, duplicate
  Host uniqueness.
- `crates/i2pr-service-tunnels/src/http/response.rs` unit tests:
  400/403 bounded response bytes, smuggling `400` without
  diagnostic header, sanitization of CR/LF/control bytes.
- `crates/i2pr-service-tunnels/src/http/config.rs` + `limits.rs`
  unit tests: `UserAgentPolicy` parse, default privacy policy
  (HTTPS-only), empty-port rejection, allowed-hosts malformed
  alias rejection, default limits validation.
- `crates/i2pr-service-tunnels/src/config.rs` unit tests:
  `http-client` requires `http_options`, `socks5-client`
  requires `socks5_options`, `irc-client` requires
  `irc_options`; non-profile kinds reject any options.
- `crates/i2pr-service-tunnels/src/irc/` unit tests: exact
  core/tag/data ceilings and `+1` rejection, tag envelope and
  escape grammar, command classification for the full §4 set,
  per-direction allowlist, tag-bypass attempts, USER rewrite,
  PING rewrite + PONG token replacement, QUIT/PART policy,
  CTCP ACTION pass vs DCC/VERSION/malformed drops,
  incremental fragmentation, coalesced lines, limits
  validation, options validation.
- Daemon-side: `crates/i2pr-daemon/src/config.rs` unit tests
  (disabled-by-default, unknown fields, loopback, enabled
  acceptance for `generic-client`/`generic-server`/`http-client`/
  `socks5-client`/`irc-client`, `irc-server` still rejected,
  duplicates, bounds) plus
  `crates/i2pr-daemon/tests/service_tunnels_foundation.rs`
  (Plan 174 black-box config/graph tests, updated for the Plan
  178 `irc-client` acceptance rule),
  `crates/i2pr-daemon/tests/service_tunnel_generic_product.rs`
  (Plan 175 manager-level tests),
  `crates/i2pr-daemon/tests/service_tunnel_http_product.rs`
  (Plan 176 black-box tests: clearnet/IP/localhost/mixed-suffix
  rejection, non-`http`/userinfo/smuggling/HTTP/1.0 rejection,
  unknown `.i2p` 502, CONNECT port-policy enforcement, sibling
  isolation, slow-incomplete-header timeout, snapshot
  accounting), and
  `crates/i2pr-daemon/tests/service_tunnel_socks5_product.rs`
  (Plan 177 black-box tests: no-auth happy path, multiple-method
  negotiation, no-acceptable-method, wrong version, zero/oversized
  methods, BIND/UDP ASSOCIATE/unknown command rejection, IPv4/IPv6
  rejection, clearnet/IP literal/localhost/mixed-suffix
  rejection, zero-domain/zero-port rejection, port-policy
  rejection, unknown `.i2p` host unreachable, same-read post-
  request bytes preserved, sibling isolation, snapshot
  accounting, username/password method rejection, oversized method
  rejection, request-with-control-byte rejection), and
  `crates/i2pr-daemon/tests/service_tunnel_irc_client_product.rs`
  (Plan 178 black-box tests: listener accept, unknown-command
  drop, sibling isolation, snapshot accounting, overlong core/tag
  rejection, fragmented/coalesced lines, CTCP ACTION pass, CTCP
  DCC drop, USER rewrite path, tagged message path,
  registration/CAP-SASL/JOIN/PRIVMSG/NOTICE path, slowloris
  boundedness, aggregate ceiling).
- Pump-side: `crates/i2pr-daemon/src/destination_streaming.rs` (5
  deterministic pump tests, retained from Plan 174; Plan 182
  adds the default-no-op `shutdown_write()` hook with CLOSE
  linger, byte-identical for the SAM endpoint).
- Plan 182 local-delivery tests:
  `crates/i2pr-daemon/tests/service_tunnels_local_roundtrip.rs`
  (9 tests: generic small/large/sibling echo digests, framed
  reverse, half-close EOF propagation, HTTP GET 200 + digest,
  SOCKS5 CONNECT + echo, IRC register/message/projection,
  resource baseline with zero `unknown_peer`/`missing_factory`)
  and
  `crates/i2pr-daemon/tests/service_tunnels_independent_application_clients.rs`
  (6 wire-surface tests incl. restart-stable server identity).
- Plan 181 external lane:
  `tests/integration/service-tunnels/run-independent.sh` (31
  command-derived rows),
  `scripts/check-service-tunnel-acceptance-evidence.sh`
  (routine-CI static checker),
  `scripts/interop/fetch-service-tunnel-clients.sh`
  (exact-pin jaraco/irc fetch),
  `.github/workflows/service-tunnels-external.yml` (manual
  full/local-only lane), and the ignored-by-default
  `service_tunnels_remote_qualification.rs` driver asserting
  the §6.3 stop condition (local rows passed, remote rows
  `blocked` on retained M6 debt).

## Distinctive design choices

1. Runtime-neutral configuration by construction; the boundary
   script proves no Tokio or listener ownership in
   `i2pr-service-tunnels` (including the Plan 176 `http`, Plan
   177 `socks5`, and Plan 178 `irc` sub-modules).
2. Kinds are typed enum values, not strings, after parsing.
3. Destination references never touch the network during validation.
4. Static aliases are lower-case and bounded; b32 spellings are
   never aliases.
5. Server targets distinguish loopback TCP from Unix path values;
   Unix targets are explicit `not-yet-supported` rather than silently
   ignored.
6. Shared client destinations require an explicit group; sharing is
   never implicit.
7. Contradictory client/server options fail before daemon mutation.
8. Persistent service destinations follow the same versioned,
   permission-hardened, atomic, no-replace contract as the router
   identity (see ADR 0006).
9. `generic-client`, `generic-server`, `http-client`,
   `socks5-client`, `irc-client`, and `irc-server` may be
   `enabled = true` after their plans land; there is no
   remaining not-yet-available gate for the current kinds.
10. The manager exposes only the public Destination b64; raw secrets
    stay inside `ServiceDestinationRecord` and `DestinationIdentity`.
11. Errors are typed and carry truncated values only, never secrets.
12. Snapshots are point-in-time counters, never memory-backed queues.
13. The cross-tunnel local-delivery path resolves through the manager
    itself so client tunnels do not need an external LeaseSet
    lookup to talk to a server tunnel owned by the same router.
14. The HTTP parser/rewrite target validator is shared by the
    daemon-owned executor and the runtime-neutral unit tests; the
    executor owns sockets and Streaming lifetime but never the
    parser grammar.
15. The HTTP executor's per-connection state, header deadlines, and
    error responses stay bounded; no per-connection memory grows
    with request body size.
16. The SOCKS5 greeting + CONNECT parser is shared by the daemon-
    owned executor and the runtime-neutral unit tests; the executor
    owns sockets and Streaming lifetime but never the parser
    grammar or the RFC 1928 reply generator.
17. The SOCKS5 executor's per-connection state, greeting/request
    deadlines, and reply bytes stay bounded; no per-connection
    memory grows with the negotiated CONNECT request size.
18. The IRC line parser/filter is shared by the daemon-owned
    executor and the runtime-neutral unit tests; the executor
    owns sockets and Streaming lifetime but never the filter
    grammar, the allowlist, or the rewrite policy.
19. The IRC executor's per-connection state (one partial-line
    buffer per side, one outstanding PONG token), line
    deadlines, and drop counters stay bounded; no
    per-connection memory grows with stream length, and
    sibling connections keep independent filter state.
20. Plan 180 transactional reconcile: the runtime-neutral
    `DiffClass` typed classification drives a single-stage
    generation swap. `Unchanged` / `MutableInPlace` entries copy
    the existing committed runtime + identity (preserving the
    persistent server destination across no-op reconciles);
    `ReplaceListener` / `ReplaceDestination` / `Remove` entries
    push the old runtime onto the draining list under a hard
    deadline; `Add` entries install fresh destinations.
    Stable server identities survive a no-op or
    target-only reconcile; forced-drain closes are bounded by
    a single `AtomicU64` per generation; the per-service
    runtime carries no Garlic/I2NP/Streaming implementation.
21. Plan 180 static boundary checker
    (`scripts/check-service-tunnel-boundaries.sh`) is enforced
    in routine CI and rejects the same runtime-neutral
    invariants `check-runtime-boundaries.sh` enforces plus the
    no-Glob/I2NP-construction, no-duplicate-byte-pump,
    no-unbounded-Tokio-channel, and exactly-one-entry-point
    Plan 180 §15 invariants.

## Cross-references

- Plans 173 (roadmap authority), 174 (foundation),
  175 (generic tunnels), 176 (HTTP), 177 (SOCKS5), 178 (IRC
  client), 179 (IRC server), 180 (composition / reconcile /
  hardening), 182 (local-delivery corrective), 181 (independent
  acceptance, blocked on retained M6 debt), 183 (M6 mixed-router
  program, registered).
- `docs/architecture/i2pr-daemon.md` (manager + runtime surface).
- `docs/architecture/i2pr-storage.md` (persistent destination storage).
- `specs/protocols/11-service-tunnels.md` (M10 dossier).
- ADRs 0001 (modular monolith), 0002 (Tokio boundary), 0006 (private
  identity storage), 0010 (transport contracts).