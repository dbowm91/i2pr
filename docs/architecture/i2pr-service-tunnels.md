# `i2pr-service-tunnels` — Deep Dive

## Crate header

- **Crate:** `i2pr-service-tunnels`
- **Path:** `crates/i2pr-service-tunnels/`
- **One-line purpose:** Runtime-neutral Milestone 10 (and Proposal 170) service-tunnel
  configuration, destination-reference policy, per-profile protocol parsers, and typed
  errors/events — every byte of socket, task, timer, and listener ownership lives in
  `i2pr-daemon`.

Crate size: 37 Rust files, 14,620 lines (14 declared modules + `lib.rs`).
Test floor: 313 unit tests, all passing, all in-crate `#[cfg(test)]` modules.

## Purpose

This crate owns the *validated policy* half of every M10 service tunnel:

- typed service-tunnel kinds, ids, groups, listener specs, server targets, and
  destination references;
- central resource / deadline / tunnel-shaping / idle / rate-limit ceilings;
- the runtime-neutral HTTP/1.1 parser + privacy rewrite + request-target validator +
  bounded error-response generator;
- the runtime-neutral RFC 1928 SOCKS5 greeting/CONNECT parser + deterministic reply
  generator, plus the Plan 290 bounded SOCKS4a CONNECT parser;
- the runtime-neutral strict-CONNECT client option surface;
- the runtime-neutral IRC/IRCv3 line parser, tag framing, per-direction allowlist, and
  client-to-network privacy filter;
- the Plan 179 IRC server registration interceptor and authenticated peer hostname
  projection;
- the Plan 291 Streamr option surface;
- the Plan 292 listener proxy-credential verifiers, server inbound access policy, and
  pure idle-sweep decision;
- the Plan 341 outbound-proxy-secret policy seam (trait + framing + fail-closed default);
- the Plan 180 runtime-neutral `DiffClass` generation diff;
- typed errors and value-only typed events/snapshots.

### What it must NOT own

The crate doc comment (`src/lib.rs:18-22`) is explicit and the source agrees. This
crate owns **no sockets, no Tokio tasks, no timers, no filesystem access, no transport
internals, no NetDB mutation, and no Garlic/I2NP construction**. `i2pr-daemon` remains
the sole M10 socket/task/composition owner.

Two nuances worth recording precisely:

- **Address naming boundary.** This crate owns the *policy* for a destination
  reference — `DestinationRef::parse` is structural only (see
  [Key contracts](#key-contracts)). Canonical hostname ownership lives in
  `i2pr-addressbook`. The *lookup wiring* that turns a validated `StaticAlias` into a
  resolved destination is daemon-owned (`ServiceTunnelManager::resolve_reference` →
  `addressbook_lookup` → `resolve_addressbook_entry`, injected through
  `set_addressbook_handle`). This crate never performs that lookup.
- **`forbid(unsafe_code)`.** `src/lib.rs:48` sets the crate-level
  `#![forbid(unsafe_code)]`, and every module except `src/idle.rs` repeats it locally.
  `src/idle.rs` is covered by the crate-level attribute; it simply has no redundant
  per-module one. The effective invariant is what matters and it holds crate-wide.

## Module layout

Line counts are real `wc -l` output. Directory rows are the sum of their files.

| Module | File | Lines | Responsibility | Key public types |
| --- | --- | ---: | --- | --- |
| `lib` | `src/lib.rs` | 122 | Crate root: module declarations, crate-level `#![forbid(unsafe_code)]`, the full `pub use` re-export surface, architecture pointer | (re-exports only) |
| `config` | `src/config.rs` | 2563 | Typed kinds (12), ids/groups, listener + target shapes, `DestinationPolicy`, resource/deadline ceilings, `TunnelShaping`, `IdlePolicy`, `HttpServerPolicy`, destination group specs, `ServiceTunnelSet::validate` | `ServiceTunnelKind`, `ServiceTunnelId`, `ServiceClientGroupId`, `DestinationPolicy`, `LocalListenerSpec`, `ServerTarget`, `ServiceResourceLimits`, `ServiceTimeouts`, `ServiceTunnelSpec`, `ServiceTunnelSet`, `TunnelShaping`, `IdlePolicy`, `HttpServerPolicy`, `DestinationGroupSpec`, `DestinationGroupId`, `DestinationGroupKey`, `ServiceKeyReference`, `DestinationCryptoPolicy`, `DestinationSigningPolicy`, `DestinationLeaseSetEncryptionPolicy`, `multihoming_start_index` |
| `destination` | `src/destination.rs` | 464 | Structural Base32 / static-alias / configured-public-material reference policy and the bounded alias table; no DNS, filesystem, or network lookup, no clearnet fallback, IP literals rejected | `DestinationRef`, `StaticAliasTable`, `B32_SUFFIX`, `I2P_SUFFIX`, `B32_LABEL_LEN`, `MAX_STATIC_ALIAS_LEN`, `MAX_STATIC_ALIAS_LABEL_LEN`, `MAX_CONFIGURED_DESTINATION_LEN` |
| `errors` | `src/errors.rs` | 104 | The single typed structural error enum for the whole crate; carries bounded/truncated values and machine-readable reasons only | `ServiceTunnelError` |
| `events` | `src/events.rs` | 100 | Value-only lifecycle events and point-in-time accounting snapshots (no socket handles, no Tokio guards, no secrets) | `ServiceTunnelEvent`, `ServiceTunnelSnapshot` |
| `generation` | `src/generation.rs` | 555 | Plan 180 runtime-neutral generation diff model driving the daemon-side transactional reconcile | `DiffClass`, `ServiceDiff`, `diff_sets`, `diff_spec`, `kind_string` |
| `http` | `src/http/` | 3297 | Plan 176 HTTP/1.1 parser, header/limits config, request-target validator, hop-by-hop + privacy rewrite, bounded error responses, and the Plan 290/292 filtered-server + presentation policy (9 files: `mod`/`config`/`error`/`limits`/`parser`/`response`/`rewrite`/`server`/`target`) | `HttpLimits`, `HttpClientOptions`, `PrivacyPolicy`, `UserAgentPolicy`, `HttpRequestHead`, `RequestLine`, `HeaderEntry`, `HeaderName`, `RequestTarget`, `TargetKind`, `parse_request_head`, `parse_request_target`, `parse_authority_form`, `parse_origin_form`, `rewrite_headers`, `build_error_response`, `proxy_auth_required`, `HttpError`, `HttpErrorKind`, `ParseError`, `TargetParseError`, `HttpServerPolicy`, `HttpPostLimiter`, `HttpPostLimits`, `FilteredServerRequest`, `PresentationClass`, `classify_presentation`, `filter_server_request`, `filter_server_request_with_policy`, `filter_server_response` |
| `socks5` | `src/socks5/` | 2300 | Plan 177 RFC 1928 no-auth greeting + CONNECT parser, strict `.i2p` target policy, deterministic bounded reply generator, plus the Plan 290 bounded SOCKS4a CONNECT parser (7 files: `mod`/`config`/`errors`/`limits`/`negotiation`/`reply`/`request`/`socks4a`) | `Socks5Limits`, `Socks5ClientOptions`, `ConnectPortPolicy`, `GreetingParser`, `GreetingOutcome`, `RequestParser`, `RequestOutcome`, `ConnectDestination`, `Socks5Error`, `Socks5ErrorKind`, `Socks5ReplyCode`, `build_reply`, `build_reply_from_code`, `Socks4aRequestParser`, `Socks4aOutcome`, `build_socks4a_reply`, `SOCKS4A_REPLY_LEN`, `SOCKS4A_GRANTED`, `SOCKS4A_REJECTED` |
| `irc` | `src/irc/` | 3680 | Plan 178 IRC/IRCv3 line parser, tag framing, command classifier + per-direction allowlist, client-to-network privacy filter, and the Plan 179 server registration interceptor + peer hostname projection (10 files: `mod`/`client_filter`/`config`/`errors`/`limits`/`line`/`policy`/`server`/`tags`) | `IrcLimits`, `IrcClientOptions`, `IrcServerOptions`, `ReasonRewritePolicy`, `IrcCommand`, `IrcCommandClass`, `LineDirection`, `ParsedLine`, `FilterOutcome`, `IrcDropReason`, `IrcLineParser`, `LineParserOutcome`, `PingRewriteState`, `PrivacySubstitutions`, `IrcTag`, `TagsOutcome`, `TagsParser`, `IrcError`, `IrcErrorKind`, `IrcServerRegistration`, `RegistrationOutcome`, `RegistrationRejection`, `RegistrationState`, `project_peer_hostname`, `encode_b32_label`, `classify_core`, `classify_post_tag_core`, `is_allowed`, `is_command_allowed` |
| `access` | `src/access.rs` | 390 | Plan 292 server inbound peer allow/deny policy over canonical Base32 hashes, plus a bounded fixed-window authenticated connection-rate limiter | `ServerAccessPolicy`, `ServerConnectionRateLimits`, `ServerConnectionRateLimiter`, `MAX_ACCESS_LIST_ENTRIES`, `MAX_RATE_LIMIT_PEERS` |
| `auth` | `src/auth.rs` | 298 | Plan 292 per-realm SHA-256 `username:realm:password` verifiers, constant-time verify, redacted `Debug`, HTTP Basic decode, SOCKS RFC 1929 realm binding | `ProxyCredentials`, `decode_basic_credentials`, `PROXY_AUTH_REALM_HTTP`, `PROXY_AUTH_REALM_CONNECT`, `PROXY_AUTH_REALM_SOCKS`, `PROXY_VERIFIER_MARKER`, `MAX_PROXY_USERNAME_LEN`, `MAX_PROXY_PASSWORD_LEN` |
| `idle` | `src/idle.rs` | 172 | Plan 292 pure, timer-free, socket-free per-tunnel idle-sweep decision (exact-deadline fire, saturating arithmetic) | `IdleSweepAction`, `idle_decision` |
| `streamr` | `src/streamr.rs` | 187 | Plan 291 runtime-neutral Streamr profile options (loopback UDP endpoints, I2P port, refresh/expiry/subscriber/payload policy with freeze-derived defaults) | `StreamrOptions`, `DEFAULT_SUBSCRIBE_INTERVAL_MS`, `DEFAULT_SUBSCRIPTION_EXPIRY_MS`, `DEFAULT_MAX_SUBSCRIBERS`, `DEFAULT_PAYLOAD_LIMIT_BYTES`, `MAX_PAYLOAD_LIMIT_BYTES`, `MAX_SUBSCRIBER_CEILING`, `DEFAULT_STREAMR_I2P_PORT` |
| `outbound_secret` | `src/outbound_secret.rs` | 277 | Plan 341 runtime-neutral *policy* half of outbound proxy-secret ownership: the `OutboundSecretStore` capability, the sealed stored-form framing, and a fail-closed default. Holds no cryptography | `OutboundSecret`, `OutboundSecretStore`, `NoOutboundSecrets`, `validate_stored_form`, `OUTBOUND_SECRET_MARKER`, `MAX_OUTBOUND_SECRET_LEN`, `MAX_OUTBOUND_SECRET_STORED_LEN` |
| `connect` | `src/connect.rs` | 111 | Plan 290 strict HTTP CONNECT-only client option surface (bounded allowed-port set, default 443). Parsing/validation live in `http`; the executor lives in the daemon | `ConnectClientOptions`, `CONNECT_OPTIONS_MAX_PORTS`, `CONNECT_DEFAULT_PORT` |

## Public surface

The `pub use` re-exports from `src/lib.rs:65-122`, verbatim in shape:

```text
pub use access::{MAX_ACCESS_LIST_ENTRIES, MAX_RATE_LIMIT_PEERS, ServerAccessPolicy,
  ServerConnectionRateLimiter, ServerConnectionRateLimits};
pub use auth::{MAX_PROXY_PASSWORD_LEN, MAX_PROXY_USERNAME_LEN, PROXY_AUTH_REALM_CONNECT,
  PROXY_AUTH_REALM_HTTP, PROXY_AUTH_REALM_SOCKS, PROXY_VERIFIER_MARKER, ProxyCredentials,
  decode_basic_credentials};
pub use config::{DEFAULT_IDLE_TIMEOUT_MS, DEFAULT_STREAMING_CONNECT_DELAY_MS,
  DestinationCryptoPolicy, DestinationGroupId, DestinationGroupKey, DestinationGroupSpec,
  DestinationLeaseSetEncryptionPolicy, DestinationPolicy, DestinationSigningPolicy, IdlePolicy,
  LocalListenerSpec, MAX_ACTIVE_CONNECTIONS_AGGREGATE, MAX_ACTIVE_CONNECTIONS_PER_SERVICE,
  MAX_BUFFERED_BYTES_PER_DIRECTION, MAX_CONFIGURED_TARGETS, MAX_EFFECTIVE_DIRECTION_TUNNELS,
  MAX_GROUP_ID_LEN, MAX_IDLE_TIMEOUT_MS, MAX_SERVICE_ID_LEN, MAX_SERVICE_TUNNELS,
  MAX_STATIC_ALIASES, MAX_STREAMING_CONNECT_DELAY_MS, MAX_TUNNEL_BACKUP_QUANTITY,
  MAX_TUNNEL_LENGTH_HOPS, MAX_TUNNEL_LENGTH_VARIANCE, MAX_TUNNEL_QUANTITY, MAX_UNIX_PATH_LEN,
  MIN_BUFFERED_BYTES_PER_DIRECTION, MIN_IDLE_TIMEOUT_MS, ServerTarget, ServiceClientGroupId,
  ServiceKeyReference, ServiceResourceLimits, ServiceTimeouts, ServiceTunnelId,
  ServiceTunnelKind, ServiceTunnelSet, ServiceTunnelSpec, TunnelShaping,
  multihoming_start_index};
pub use connect::{CONNECT_DEFAULT_PORT, CONNECT_OPTIONS_MAX_PORTS, ConnectClientOptions};
pub use destination::{DestinationRef, StaticAliasTable};
pub use errors::ServiceTunnelError;
pub use events::{ServiceTunnelEvent, ServiceTunnelSnapshot};
pub use generation::{DiffClass, ServiceDiff, diff_sets, diff_spec, kind_string};
pub use http::{FilteredServerRequest, HeaderEntry, HeaderName, HttpClientOptions, HttpError,
  HttpErrorKind, HttpLimits, HttpPostLimiter, HttpPostLimits, HttpRequestHead, HttpServerPolicy,
  MAX_POST_LIMIT_PEERS, ParseError, PresentationClass, PrivacyPolicy, RequestLine,
  RequestTarget, TargetKind, TargetParseError, UserAgentPolicy, build_error_response,
  classify_presentation, filter_server_request, filter_server_request_with_policy,
  filter_server_response, parse_authority_form, parse_origin_form, parse_request_head,
  parse_request_target, proxy_auth_required, rewrite_headers};
pub use idle::{IdleSweepAction, idle_decision};
pub use irc::{IrcClientOptions, IrcCommand, IrcCommandClass, IrcDropReason, IrcError,
  IrcErrorKind, IrcLimits, IrcLineParser, IrcServerOptions, IrcServerRegistration, IrcTag,
  LineDirection, LineParserOutcome, ParsedLine, PingRewriteState, PrivacySubstitutions,
  ReasonRewritePolicy, RegistrationOutcome, RegistrationRejection, RegistrationState,
  TagsOutcome, TagsParser, classify_core as classify_irc_core, classify_post_tag_core,
  encode_b32_label, is_allowed as is_irc_command_allowed,
  is_command_allowed as is_irc_command_allowed_alias, project_peer_hostname};
pub use socks5::{ConnectDestination, ConnectPortPolicy, GreetingOutcome, GreetingParser,
  RequestOutcome, RequestParser, SOCKS4A_GRANTED, SOCKS4A_REJECTED, SOCKS4A_REPLY_LEN,
  Socks4aOutcome, Socks4aRequestParser, Socks5ClientOptions, Socks5Error, Socks5ErrorKind,
  Socks5Limits, Socks5ReplyCode, build_reply as build_socks5_reply,
  build_reply_from_code as build_socks5_reply_from_code, build_socks4a_reply};
pub use streamr::{DEFAULT_MAX_SUBSCRIBERS, DEFAULT_PAYLOAD_LIMIT_BYTES,
  DEFAULT_STREAMR_I2P_PORT, DEFAULT_SUBSCRIBE_INTERVAL_MS, DEFAULT_SUBSCRIPTION_EXPIRY_MS,
  MAX_PAYLOAD_LIMIT_BYTES, MAX_SUBSCRIBER_CEILING, StreamrOptions};
```

### `outbound_secret` is module-public but *not* re-exported at the crate root

`pub mod outbound_secret;` is declared at `src/lib.rs:61`, but there is deliberately **no**
`pub use outbound_secret::…`. Downstream crates reach these items through the module path
`i2pr_service_tunnels::outbound_secret::{OutboundSecret, OutboundSecretStore, …}`. The
private submodule constants (`OUTBOUND_SECRET_MARKER`, `MAX_OUTBOUND_SECRET_LEN`,
`MAX_OUTBOUND_SECRET_STORED_LEN`) and the `validate_stored_form` framing validator are
public within that module. This is the crate's only secret-bearing module.

## Key contracts

### Typed service-tunnel kinds

`ServiceTunnelKind` (`src/config.rs`) has exactly **12** variants, paired with their
`parse` / `as_str` spellings (`src/config.rs:230-263`):

| Variant | Spelling |
| --- | --- |
| `GenericClient` | `generic-client` |
| `GenericServer` | `generic-server` |
| `HttpClient` | `http-client` |
| `Socks5Client` | `socks5-client` |
| `IrcClient` | `irc-client` |
| `IrcServer` | `irc-server` |
| `ConnectClient` | `connect-client` (Plan 290) |
| `SocksIrc` | `socks-irc` (Plan 290) |
| `HttpServer` | `http-server` (Plan 290) |
| `HttpBidirServer` | `http-bidir-server` (Plan 290, deprecated) |
| `StreamrClient` | `streamr-client` (Plan 291) |
| `StreamrServer` | `streamr-server` (Plan 291) |

Kinds are typed enum values, never strings, after parsing. Per-kind option
applicability is gated structurally: `http_options`, `socks5_options`, `irc_options`,
`connect_options`, `streamr_options`, plus per-kind contradiction gates (shaping, profile,
credential, cadence, peer policy) are all rejected for the wrong kind.

### Destination references, static aliases, and the naming boundary

`DestinationRef` (`src/destination.rs:32-45`) has three variants, dispatched purely
structurally in `DestinationRef::parse`:

- `Base32Hash { label, hash }` — value ends with `.b32.i2p`; `label` is exactly
  `B32_LABEL_LEN = 52` lower-case characters and `hash` is the decoded 32-byte hash.
- `StaticAlias(String)` — value ends with `.i2p`; bounded lower-case label of at most
  `MAX_STATIC_ALIAS_LABEL_LEN = 63` (`MAX_STATIC_ALIAS_LEN = 67` including the suffix).
- `ConfiguredDestination(String)` — anything else that is bounded
  (`MAX_CONFIGURED_DESTINATION_LEN = 4096`) public material and is **not** an IP literal.

There is no DNS lookup, no filesystem lookup, no network lookup, and no implicit
clearnet fallback. Resolution of a validated reference to a LeaseSet or a remote runtime
remains a daemon/client composition operation. Canonical *naming* is owned by
`i2pr-addressbook`; the daemon owns the lookup wiring that injects the shared addressbook
into the manager. This crate owns the policy that decides what a syntactically acceptable
reference is.

### Resource and deadline ceilings (real values)

From `src/config.rs` unless noted:

| Constant | Value |
| --- | ---: |
| `MAX_SERVICE_TUNNELS` | 32 |
| `MAX_STATIC_ALIASES` | 64 |
| `MAX_SERVICE_ID_LEN` | 64 |
| `MAX_GROUP_ID_LEN` | 64 |
| `MAX_ACTIVE_CONNECTIONS_PER_SERVICE` | 128 |
| `MAX_ACTIVE_CONNECTIONS_AGGREGATE` | 1024 |
| `MIN_BUFFERED_BYTES_PER_DIRECTION` | 1_024 |
| `MAX_BUFFERED_BYTES_PER_DIRECTION` | 1_048_576 |
| `MAX_CONFIGURED_TARGETS` | 8 |
| `MAX_UNIX_PATH_LEN` | 255 |
| `MIN_CONNECT_TIMEOUT_MS` / `MAX_CONNECT_TIMEOUT_MS` | 1_000 / 120_000 |
| `MIN_READ_TIMEOUT_MS` / `MAX_READ_TIMEOUT_MS` | 1_000 / 600_000 |
| `MIN_WRITE_TIMEOUT_MS` / `MAX_WRITE_TIMEOUT_MS` | 1_000 / 600_000 |
| `MIN_SHUTDOWN_TIMEOUT_MS` / `MAX_SHUTDOWN_TIMEOUT_MS` | 1_000 / 30_000 |
| `DEFAULT_STREAMING_CONNECT_DELAY_MS` / `MAX_…_DELAY_MS` | 500 / 5_000 |
| `MAX_TUNNEL_QUANTITY` | 6 |
| `MAX_TUNNEL_LENGTH_HOPS` | 3 |
| `MAX_TUNNEL_BACKUP_QUANTITY` | 3 |
| `MAX_TUNNEL_LENGTH_VARIANCE` | 2 |
| `MAX_EFFECTIVE_DIRECTION_TUNNELS` | 8 |
| `MIN_IDLE_TIMEOUT_MS` / `MAX_IDLE_TIMEOUT_MS` / `DEFAULT_IDLE_TIMEOUT_MS` | 1_000 / 86_400_000 / 600_000 |

Profile ceilings:

- **HTTP** (`src/http/config.rs`): request line 8_192; total header 65_536; header count
  100; field name 256; field value 8_192; CONNECT authority 512; retained buffer 65_536;
  generated error 1_024; options ports 16; user-agent rules 32 × 256 bytes; host 256;
  path 4_096; query 4_096; server authority 256; spoofed host 253; `MAX_POST_LIMIT_PEERS`
  4_096; each bounded error body 256 bytes.
- **SOCKS5** (`src/socks5/config.rs`): method count 16; greeting 32; request header 32;
  domain 255; retained buffer 320; reply 64; options ports 16; `DEFAULT_CONNECT_PORT`
  443; `REPLY_LEN` 10; success bind `127.0.0.1` / port 0. **SOCKS4a**
  (`src/socks5/socks4a.rs`): version `0x04`; command `0x01`; fixed header 8; USERID max
  255; retained max 520; reply len 8; granted `90`; rejected `91`.
- **CONNECT** (`src/connect.rs`): `CONNECT_OPTIONS_MAX_PORTS` 16;
  `CONNECT_DEFAULT_PORT` 443.
- **IRC** (`src/irc/config.rs`): core line 512; tag envelope 8_191; client tag data 4_094;
  per-direction line buffer 8_192; generated line 8_192; tag count 128; tag key 64; CTCP
  counter ceilings 1_024; options allowed hosts 16. **IRC server**
  (`src/irc/server.rs`): pre-registration lines default 10, ceiling 64; cumulative
  pre-registration bytes default `IRC_LINE_BUFFER_MAX_BYTES` (8_192).
- **Access** (`src/access.rs`): `MAX_ACCESS_LIST_ENTRIES` 64;
  `MAX_RATE_LIMIT_PEERS` 4_096; rate windows `[60_000, 3_600_000, 86_400_000]` ms;
  per-limit maximum 100_000.
- **Auth** (`src/auth.rs`): `MAX_PROXY_USERNAME_LEN` 128;
  `MAX_PROXY_PASSWORD_LEN` 512.
- **Streamr** (`src/streamr.rs`): default subscribe interval 10_000 ms (fast start 2_000 ms
  for the first 5); default/max subscribe interval 30_000; default subscription expiry
  60_000; min/max expiry 10_000 / 300_000; default/max subscribers 10 / 64; default and
  hard payload limit 1_200; `DEFAULT_STREAMR_I2P_PORT` 0.
- **Outbound secret** (`src/outbound_secret.rs`): `MAX_OUTBOUND_SECRET_LEN` 512;
  `MAX_OUTBOUND_SECRET_STORED_LEN` = `2 * (512 + 64) + 16` = 1_168.

### Validated sets and listener/target shapes

- `ServiceTunnelSet::validate()` rejects duplicate ids, duplicate binds, contradictory
  options, and aggregate overflow **before** any daemon state change.
- Client kinds require a listener + destination and forbid server targets; server kinds
  require target(s), forbid a listener and a remote destination. Explicitly grouped
  server services require distinct, nonzero inbound I2P ports.
- `LocalListenerSpec` carries a `std::net::IpAddr` and is loopback-only;
  `ServerTarget` is `LoopbackTcp(std::net::SocketAddr)` or a bounded Unix path
  (`MAX_UNIX_PATH_LEN` 255). These are *value* types used for structural validation —
  this crate never opens one.
- `StaticAliasTable` rejects duplicates, conflicts, malformed targets, and ceiling
  overflow.

### Typed errors (`src/errors.rs`)

`ServiceTunnelError` is `Clone + Debug + Eq + PartialEq + Error` with exactly 11 variants:
`InvalidId`, `InvalidKind`, `InvalidGroup`, `InvalidDestinationRef`, `InvalidAlias`,
`InvalidListener`, `InvalidTarget`, `DuplicateId`, `DuplicateListener`, `DuplicateAlias`,
`ContradictoryOptions`, `ExceedsCeiling`. Values are bounded/truncated by callers and
`reason` fields are `&'static str`, so no runtime payload is carried.

Profile-specific typed errors live in their own modules and are re-exported at the root:

- `Socks5ErrorKind` (27 variants) — `Success` … `InvalidLimits`, `ConnectFailure`,
  including the negotiation/structural/`NonI2pTarget` split and the SOCKS4a
  `MalformedUserid` path.
- `IrcErrorKind` (13 variants) — `CoreLineTooLong`, `TagEnvelopeTooLong`,
  `TagDataTooLong`, `BufferCeiling`, `UnknownCommand`, `DisallowedDirection`, `InvalidTag`,
  `TagKeyTooLong`, `TooManyTags`, `RealnameTooLong`, `InvalidNumeric`, `InvalidLimits`,
  `PingTokenOverflow`.
- `HttpErrorKind` (17 variants) — `MalformedRequestLine`, `MalformedHeaders`,
  `MalformedField`, `MalformedTarget`, `UnsupportedScheme`, `NonI2pAuthority`,
  `UserinfoInAuthority`, `SmugglingAmbiguity`, `UnsupportedConnectPort`,
  `MethodNotAllowed` (405), `PresentationRefused` (403),
  `BufferCeilingExceeded`, `ResponseCeilingExceeded`, `InvalidLimits`, `BadGateway`,
  `GatewayTimeout`, `Other`.
- `RegistrationRejection` (8 variants) — `TooManyLines`, `BufferOverflow`,
  `CrossProtocol`, `UnknownCommand`, `InvalidLine`, `InvalidUser`, `InvalidServer`,
  `CoreLineTooLong`.

### Typed events and snapshots (`src/events.rs`)

`ServiceTunnelEvent` (5 variants): `SpecValidated { id, kind }`,
`SpecRejected { id, reason }`, `SetValidated { service_count }`,
`ConnectionAdmitted { id, active_for_service, active_aggregate }`,
`ConnectionClosed { id, active_for_service, active_aggregate }`.

`ServiceTunnelSnapshot` is a `Copy` point-in-time struct:
`configured_services`, `enabled_services`, `active_connections`, `rejected_connections`,
`destination_failures`. Snapshots are never memory-backed queues; saturation is signaled
by rejection, not buffer growth.

### `access.rs` and `auth.rs` — policy only, no listener, no connection

**What is modelled.** `ServerAccessPolicy { allow, deny, connection_rates }` where
`allow` is the union of `access_list` and `white_list` and `deny` is `black_list`.
Entries are canonical Base32 destination hashes (52 characters, with or without the
`.b32.i2p` suffix) decoded to `[u8; 32]`. Matching is exact on the 32-byte peer hash
observed at inbound accept: **deny always wins**; an empty allow set admits everyone not
denied; a non-empty allow set admits only its members. Values are never echoed.
`ServerConnectionRateLimiter` provides bounded fixed-window per-peer and aggregate
accounting over caller-supplied process-monotonic milliseconds; at table capacity it
reclaims expired records and fail-closed rejects an unseen peer.

**What is NOT owned — stated precisely.** This crate models access and auth *policy*
only. It owns **no listener, no connection, no accept loop, and no peer-hash source**.
The authenticated 32-byte peer hash arrives from the daemon's inbound Streaming accept;
this crate only decides what may be done with it. Likewise `ProxyCredentials` models the
*verifier* half: one SHA-256 over `username:realm:password` per tunnel, the plaintext
password dropped at construction, constant-time verification, and a hand-written
`Debug` that emits exactly `ProxyCredentials(<redacted>)`. The realm binds the verifier
to its listener family, so a verifier stolen from HTTP does not verify on SOCKS or
CONNECT. `decode_basic_credentials` is RFC 7617 Basic decode; SOCKS RFC 1929 shares the
same realm binding. Digest authentication is not implemented. **None of this crate's
code opens, accepts, or holds a listener or connection.**

### `outbound_secret.rs` — secret-handling invariants (verified)

`OutboundSecret` is the only type in the crate carrying outbound secret bytes. Verified
against `src/outbound_secret.rs`:

- **No `Debug`, no `Display`.** The struct declaration (`lines 64-67`) carries no
  `#[derive(...)]`; there is no `impl Debug` and no `impl Display` anywhere in the file.
  There is therefore no way to print it.
- **No `Clone`.** No `#[derive(Clone)]` and no manual `impl Clone`; the type is not
  copyable and cannot be duplicated.
- **Zeroized on drop.** The buffer is
  `Zeroizing<[u8; MAX_OUTBOUND_SECRET_LEN]>` (`line 65`), allocated via
  `Zeroizing::new([0_u8; 512])` (`line 89`) — a fixed-size stack buffer, not a `String`
  and not a `Vec`, so the crate does not need `zeroize/alloc` and the secret never
  sits in a heap allocation. `zeroize::Zeroizing` implements `Drop` and scrubs on scope
  exit. The `zeroize` dependency is present in `Cargo.toml` and used.
- **Bounded and NUL-free at construction.** `OutboundSecret::new` rejects empty input,
  input longer than 512 bytes, and any embedded NUL.
- **Borrow-only access.** `expose()` / `expose_str()` return borrows that cannot outlive
  the value; a caller builds its header and drops it.
- **Marker separation.** `OUTBOUND_SECRET_MARKER = "$i2pr1o$"` is intentionally
  distinct from the inbound verifier marker `PROXY_VERIFIER_MARKER = "$i2pr1$"`, so an
  inbound verifier can never be mistaken for an outbound sealed form.
  `validate_stored_form` enforces marker presence, non-empty lowercase-hex body of even
  length, and the stored-length ceiling — **before** any decode work.
- **Fail-closed default.** `NoOutboundSecrets` implements `OutboundSecretStore` by
  returning an error from `seal` and `open` and `false` from `is_available`. It is a
  refusal, not a stub that returns a default value, so "no owner installed" is a state
  the type carries.

The trait is the injection point: the daemon supplies the concrete router-bound
implementation, exactly as it already injects `RouterDeliveryService`. This module holds
no cryptography — it is only the policy half, which is why it can be runtime-neutral
with no AEAD dependency.

### `idle.rs` — pure idle-sweep decision

`idle_decision(policy, active_connections, streamr_subscribers, last_activity_ms,
now_ms) -> Option<IdleSweepAction>` is a pure function. There is no clock, no timer, and
no socket. It fires only when nothing is active (no open streams **and** no Streamr
subscribers) and the quiet interval reaches the deadline. Arithmetic is saturating, so
clock jumps never panic or wrap.

`IdleSweepAction` has 4 variants — `Close`, `RotateDestination`, `RebuildPools`,
`ReducePools` — and the fixed priority is **rotate > close > rebuild > reduce**
(`src/idle.rs:46-58`). `close_timeout_ms` and `reduce_timeout_ms` override
`timeout_ms`; `None` inherits `timeout_ms` and `Some(0)` fires as soon as the tunnel is
idle. `IdlePolicy::disabled()` and any inert policy decide nothing.

### `streamr.rs` — Streaming-repair profile options

`StreamrOptions` is `Copy` and shared by the subscriber (`streamr-client`) and publisher
(`streamr-server`) halves: `local_udp` (loopback media source for the server half / media
target for the client half), `remote_sink` (Plan 292 loopback-confined subscriber media
redirect — it *replaces* the media target, never duplicates it), `target_i2p_port`,
`subscribe_interval_ms`, `subscription_expiry_ms`, `max_subscribers`,
`payload_limit_bytes`. Defaults are freeze-derived from Java I2P's
`net.i2p.i2ptunnel.streamr` (`Pinger` cadence, `Subscriber::EXPIRATION`,
`MAX_SUBSCRIPTIONS`). Per-kind required fields are enforced by `ServiceTunnelSpec::validate`,
not here. No sockets, no timers, no Tokio — the daemon owns all UDP I/O.

### HTTP/1.1 parser, rewrite, target validation, bounded error responses

- **Parser** rejects NUL/CR/LF/obs-fold/CRLF ambiguities and host-smuggling shapes
  (conflicting `Content-Length`, `Transfer-Encoding` + `Content-Length`, GET/HEAD with
  framing, duplicate `Host` with a conflicting authority), overlong fields under deadline,
  methods outside uppercase ASCII, and non-`HTTP/1.1` versions.
- **Target validator** rejects non-`http` schemes, userinfo, IP literals, `localhost` /
  `.localhost`, mixed-suffix confusion (`*.i2p.example`), empty CONNECT ports, port 0,
  and malformed/empty authorities. `.b32.i2p` references and canonical `.i2p` aliases
  pass the strict static-alias grammar.
- **Rewrite** parses the `Connection` value list and removes every header it names
  (case-insensitive); removes `Proxy-Connection`, `Keep-Alive`, `TE`, `Trailer`,
  `Upgrade`, `Transfer-Encoding`; forces `Connection: close`; normalizes `Host` from the
  validated destination authority; strips `Via`, `Forwarded`, `X-Forwarded-{For,Host,Proto}`,
  `Proxy-Authorization`, optionally `Referer` and `From`; applies
  `User-Agent` keep/strip/replace with the stable `DEFAULT_USER_AGENT_VALUE`
  (`"MYOB/6.66 (AN/ON)"`).
- **Error responses** are bounded to 256 bytes each with a sanitized
  `X-HTTP-Proxy-Reason` header (CR/LF/control bytes stripped). Untrusted request bytes are
  never echoed.
- **Filtered server side** (Plan 290/292) enforces origin-form only, requires `Host` and
  replaces it with the loopback target, rejects `Transfer-Encoding`, strips
  hop-by-hop and identifying headers, forces close, and injects no peer identity.
  `classify_presentation` maps `/addresshelper` + `i2paddresshelper` to
  `PresentationClass::Helper` and `/jump` + `jump` to `PresentationClass::Jump`;
  `Ordinary` is always forwarded and a closed gate refuses 403 via
  `HttpErrorKind::PresentationRefused`.

### RFC 1928 SOCKS5 negotiation + CONNECT + bounded replies

- **Greeting parser**: `VER` must be `0x05`; `NMETHODS` must be
  `1..=SOCKS5_METHOD_COUNT_MAX`; the method list must contain
  `0x00 NO AUTHENTICATION REQUIRED`. `0x02 username/password` is never accepted, even
  when offered.
- **CONNECT request parser**: rejects BIND (`0x02`), UDP ASSOCIATE (`0x03`), and unknown
  commands; rejects IPv4 (`0x01`) and IPv6 (`0x04`) address types; rejects
  clearnet / IP literal / `localhost` / mixed-suffix / malformed alias targets; rejects
  NUL, control, and whitespace domain bytes; rejects zero-length domain and port 0.
- **Reply generator**: a deterministic `REPLY_LEN = 10` RFC 1928 reply with
  `BND.ADDR = 127.0.0.1` and `BND.PORT = 0` — a neutral loopback bind. Untrusted request
  bytes and destination private material are never echoed.
- **Port policy**: `ConnectPortPolicy::default()` permits only port 443
  (`DEFAULT_CONNECT_PORT`); configurable per service.
- **Allowed hosts**: `Socks5ClientOptions::allowed_hosts` may pin a bounded list of
  `.i2p` hosts, each validated by the strict static-alias grammar.
- **Plan 290 SOCKS4a parity** (`src/socks5/socks4a.rs`): version `0x04`, command
  `0x01` only, the `0.0.0.x` marker with a bounded USERID, an 8-byte grant/refuse reply
  (`90`/`91`), and the identical `.i2p`-only domain policy. **Plain SOCKS4 IPv4 literals
  fail closed** — only the domain-extension (4a) form is accepted.

**Explicit fail-closed non-support** (from `src/socks5/mod.rs` and
`specs/protocols/11-service-tunnels.md:256-260`): clearnet outproxy, DNS resolution of
SOCKS hostnames, IP-literal forwarding, SOCKS UDP, BIND, authentication as a *negotiated
SOCKS method*, Tor `RESOLVE` / `RESOLVE_PTR`, and arbitrary local/LAN target relay. The
SOCKS5 profile is strictly narrower than Java I2P's broad SOCKS/outproxy profile, and
that is a deliberate non-goal, not a gap. (The Plan 177 §11 list also named
"SOCKS4/4a" as unsupported; Plan 290 later added the *bounded* SOCKS4a CONNECT parser
above, so the current surface is narrower still: 4a domain-extension only, no SOCKS4
IPv4, no auth method.)

### IRC line parser, IRCv3 tags, classification, and privacy filter

- **Ceilings**: core line 512 bytes, tag envelope 8_191, client tag data 4_094,
  per-direction line buffer 8_192, generated line 8_192, tag count 128, tag key 64.
  Overlong lines are dropped without truncation and are never split into a
  syntactically different valid command.
- **Tag framing**: a structural IRCv3 envelope (`@tag …` plus a separating space) with
  opaque tag names/values, invalid escapes rejected, and per-key / count / data ceilings
  enforced. Tag presence never bypasses command classification, and core and tag limits
  are enforced separately.
- **Classification**: `IrcCommandClass::{Known(IrcCommand), Unknown}` with a typed
  `IrcCommand` set (29 spellings) and an explicit per-direction allowlist covering the
  Plan 178 §4 set. Unknown or unclassified commands are dropped, never passed.
- **Client-to-network privacy rewrites**: `USER` hostname/servername replaced with stable
  non-identifying placeholders (`DEFAULT_USER_HOSTNAME = "i2p"`,
  `DEFAULT_USER_SERVERNAME = "localhost"`); a location-bearing `PING` is rewritten with
  one bounded per-connection outstanding PONG token
  (`DEFAULT_PING_LOCATION = "i2p"`) where a new rewrite deterministically replaces the
  old; `QUIT`/`PART` reasons pass unchanged by default
  (`DEFAULT_QUIT_REASON = ""`, `ReasonRewritePolicy::Keep`) with a named opt-in stable
  replacement.
- **CTCP/DCC policy**: `ACTION` passes; malformed and multi-delimiter messages,
  address-bearing `DCC`, and other unsupported CTCP requests are dropped. There is no DCC
  helper tunnel.
- **Drop reasons** are typed: `UnknownCommand`, `DisallowedDirection`, `DccBlocked`,
  `UnsupportedCtcp`, `MalformedCtcp`, `RealnameTooLong`, `BadUserField`,
  `PingLocationOverflow`.
- **Options**: `IrcClientOptions { allowed_hosts, reason_rewrite, user_realname_max_bytes }`.
- Errors carry kinds plus machine-readable reasons only — no secrets, no nicknames, no
  message text.

**Explicit fail-closed non-support** (`specs/protocols/11-service-tunnels.md:334-338`,
Plan 178 §12): DCC tunnel support, WEBIRC, TLS termination, SASL credential management,
IRC bouncer functionality, a channel/user state database, and an arbitrary
protocol-aware server-side filter. The IRC profile is deliberately stricter than Java
I2P's broad IRC/DCC/operator profile.

### Plan 179 IRC-server registration interceptor and the 52-char projection

- **Ceilings**: pre-registration lines default `10` with a hard ceiling of `64`;
  cumulative pre-registration bytes default to the Plan 178 IRC line-buffer ceiling
  (8_192). The pre-registration allowlist (`PASS CAP AUTHENTICATE NICK`) rejects any line
  whose command name is empty, exceeds 16 bytes, or contains a non-uppercase-ASCII byte.
- **Cross-protocol detection**: the first observed line is checked against a small fixed
  list (`GET / POST / HEAD / PUT / DELETE / OPTIONS / CONNECT / TRACE / PATCH ` and the
  BitTorrent handshake magic); a non-empty match rejects the registration as
  `RegistrationRejection::CrossProtocol`.
- **The 52-character rule, and where it is enforced.** `project_peer_hostname` in
  `src/irc/server.rs:120-125` returns `encode_b32_label(&peer_destination_hash)` followed
  by `".b32.i2p"`. `encode_b32_label` (`src/irc/server.rs:131-…`) encodes a 32-byte
  (256-bit) hash over the I2P Base32 alphabet `a-z` + `2-7`, producing **52**
  characters with 4 bits of zero padding on the final character. The length is enforced
  at three places: `B32_LABEL_LEN = 52` in `src/destination.rs:17` (checked at
  `src/destination.rs:187` when parsing a reference label), the `String::with_capacity(52)`
  preallocation in `encode_b32_label`, and the
  `String::with_capacity(52 + ".b32.i2p".len())` preallocation in
  `project_peer_hostname`. The projection is computed once at interceptor construction
  time from the 32-byte Streaming peer destination hash, and it is the only acceptable
  source for the post-rewrite `USER` hostname argument. Note the source of the hash: the
  daemon takes it from the *authenticated* established Streaming connection, so
  application bytes can never influence the projected identity.
- **`USER` rewriting**: the registered `USER` line has its second argument replaced with
  the projected hostname; username, servername, realname, and (for RFC 1459) the mode
  parameter are preserved within the 512-byte core ceiling. A rewrite that would push
  the line past the ceiling is rejected as `RegistrationRejection::CoreLineTooLong`,
  never truncated.
- **Tag preservation**: IRCv3 tag envelopes are preserved verbatim on the rewritten
  `USER` (and `SERVER`) line, so the local IRC target observes the original tagged
  message.
- **`SERVER`** (server-to-server IRC) is accepted as a handoff line and passed verbatim;
  a connecting server name is not a per-user identity.
- **Buffering and handoff**: pre-registration commands are buffered per interceptor and
  concatenated with the rewritten `USER` line at handoff time. Same-read post-USER bytes
  (anything following the terminating CRLF in the same read) are preserved verbatim as
  the first raw-pump bytes.
- **Typed outcome**: `RegistrationOutcome::{Incomplete { retained }, Ready { prefix,
  leftover }, Rejected(reason), Eof}`, so the daemon executor can drive a one-shot prefix
  + leftover handoff to the loopback target.

### Remaining fail-closed non-goals for M10

None of these are silently bridged; each is a deliberate boundary:

- no clearnet outproxy;
- no SOCKS UDP ASSOCIATE or BIND;
- no SOCKS4 IPv4 relay, and no SOCKS *auth method* negotiation;
- no transparent proxying (client listeners are loopback-only by construction);
- no HTTP/2+ termination (HTTP/1.1 only);
- no TLS interception;
- no IRC DCC tunnelling or WEBIRC.

## Dependencies

From `crates/i2pr-service-tunnels/Cargo.toml` — production dependencies only, **no
`[dev-dependencies]` section and no build dependencies**:

| Dependency | Why |
| --- | --- |
| `base64ct` (workspace) | RFC 7617 Basic credential decode in `auth.rs` |
| `i2pr-proto` (path) | Bounded wire types shared with the rest of the workspace |
| `sha2` (workspace) | SHA-256 credential verifiers (`auth.rs`) |
| `subtle` (workspace) | Constant-time verifier comparison (`auth.rs`) |
| `thiserror` (workspace) | `ServiceTunnelError` derive |
| `zeroize` (workspace) | `Zeroizing` buffer in `outbound_secret.rs` |

From `scripts/check-dependency-direction.sh`, the allowlist entry is
`"i2pr-service-tunnels": {"i2pr-client", "i2pr-proto"}`. The checker only fails on
`direct - allowed` (forbidden edges), so an allowed-but-unused edge is not a violation.
**Precise statement: the allowlist permits `i2pr-client`, but this crate's manifest does
not depend on it.** Of the two allowed workspace edges, only `i2pr-proto` is actually
used. `src/outbound_secret.rs:14-16` states the reason directly: the crate is permitted
only `i2pr-client` and `i2pr-proto` internally, so it cannot depend on an AEAD — which
is precisely why that module is the policy half of outbound-secret ownership and holds no
cryptography.

`scripts/check-service-tunnel-boundaries.sh` additionally greps `Cargo.toml` for
`i2pr-transport|i2pr-tunnel|i2pr-runtime|i2pr-daemon|i2pr-testkit` and rejects any match.
None is present.

Reverse direction: `i2pr-daemon` depends on `i2pr-service-tunnels` (line 45 of the
checker's `i2pr-daemon` allowlist) for the typed spec surface.

## Tests

There is **no `crates/i2pr-service-tunnels/tests/` directory**. All 313 tests are
in-crate `#[cfg(test)]` modules, which is the honest coverage picture for a
runtime-neutral crate: the daemon and runtime integration suites are what exercise the
sockets, and they live elsewhere.

Verified count: `cargo test -p i2pr-service-tunnels --all-targets` → **313 passed; 0
failed; 0 ignored**.

`#[cfg(test)]` module and test counts per file:

| File | Tests | File | Tests |
| --- | ---: | --- | ---: |
| `config.rs` | 28 | `irc/policy.rs` | 22 |
| `irc/server.rs` | 24 | `http/server.rs` | 20 |
| `socks5/request.rs` | 20 | `generation.rs` | 18 |
| `irc/client_filter.rs` | 17 | `http/target.rs` | 16 |
| `http/parser.rs` | 16 | `irc/line.rs` | 15 |
| `irc/tags.rs` | 11 | `http/rewrite.rs` | 10 |
| `destination.rs` | 10 | `socks5/negotiation.rs` | 10 |
| `socks5/socks4a.rs` | 13 | `access.rs` | 6 |
| `http/config.rs` | 5 | `auth.rs` | 5 |
| `irc/config.rs` | 5 | `irc/errors.rs` | 5 |
| `http/response.rs` | 5 | `connect.rs` | 3 |
| `socks5/reply.rs` | 3 | `streamr.rs` | 3 |
| `outbound_secret.rs` | 4 | `socks5/config.rs` | 4 |
| `idle.rs` | 7 | `events.rs` | 2 |
| `http/limits.rs` | 2 | `socks5/errors.rs` | 2 |
| `socks5/limits.rs` | 2 | | |
| No test module: `errors.rs`, `http/error.rs`, `http/mod.rs`, `irc/limits.rs`, `irc/mod.rs`, `lib.rs`, `socks5/mod.rs` | | | |

### Bounded negative paths

- **Malformed HTTP**: NUL/CR/LF/obs-fold, lone LF, bare CR, conflicting `Content-Length`,
  `Transfer-Encoding` + `Content-Length`, GET with framing, duplicate `Host`, control-byte
  values, request-line and header-count ceilings, non-`http` schemes, userinfo,
  clearnet/IP/`localhost`/mixed-suffix, CONNECT-with-zero-port, overlong authority.
- **Bad SOCKS5**: wrong version, zero methods, too many methods, no acceptable method,
  BIND / UDP ASSOCIATE / unknown command, IPv4 / IPv6 address type, clearnet / IP literal
  / `localhost` / mixed-suffix, zero-domain / zero-port, request-with-control-byte,
  port-policy rejection; plus the SOCKS4a wrong version, plain-SOCKS4-IPv4 rejection,
  USERID control byte, overlong domain, zero port, and bounded neutral reply shape.
- **Malformed IRC**: exact core/tag/data ceilings and their `+1` rejection, tag envelope
  and escape grammar, tag-bypass attempts, full command-classification set, per-direction
  allowlist, incremental one-byte fragmentation, coalesced lines, USER rewrite, PING
  rewrite + PONG token replacement, QUIT/PART policy, CTCP `ACTION` pass vs
  DCC/`VERSION`/malformed drops.
- **Access / auth / idle / outbound secret**: rate limiter is bounded and fails closed at
  peer-table capacity; post-limiter bounds peer tracking and fails closed; verifier
  round-trips, realm separation, and Basic decode; idle exact-deadline fire, priority
  ordering, activity suppression, and clock-jump saturation; outbound-secret bounding,
  NUL rejection, marker interchangeability refusal, oversize-frame rejection without
  decode work, and the fail-closed default store.

## Boundary enforcement

Both scripts were run from the repository root. **Both passed.**

| Script | Exit code | Output |
| --- | ---: | --- |
| `bash scripts/check-service-tunnel-boundaries.sh` | `0` | `service-tunnel boundary checks passed` |
| `bash scripts/check-runtime-boundaries.sh` | `0` | `runtime boundary checks passed` |

### What `scripts/check-service-tunnel-boundaries.sh` enforces (Plan 180 §15)

Eight invariants, in order:

1. `crates/i2pr-service-tunnels/src` contains none of
   `tokio::|TcpListener|TcpStream|UdpSocket|UnixListener|UnixStream|tokio::net|tokio::spawn|tokio::time|tokio::sync`
   — no Tokio, sockets, listeners, tasks, or timers.
2. The crate's `Cargo.toml` contains none of
   `i2pr-transport|i2pr-tunnel|i2pr-runtime|i2pr-daemon|i2pr-testkit`.
3. No Garlic/I2NP construction: none of
   `GarlicClove|GarlicMessage|i2np::|i2np_message|build_short_tunnel|build_short_request|build_short_reply`.
4. No transport internals: none of
   `i2pr_transport_ssu2|i2pr_transport_ntcp2|build_short_request|build_short_reply`.
5. The production daemon config accepts no non-loopback `bind_address`.
6. **A single streaming pump**: `fn run_stream_pump\b` is defined at most once across
   `crates/i2pr-daemon/src` — a service-specific duplicate raw byte pump fails.
7. **No unbounded channels**: no `unbounded_channel|unbounded::<|UnboundedSender|UnboundedReceiver`
   in the five M10 daemon service-tunnel modules.
8. **A single manager entry point**: `pub fn register_service_tunnel_manager` must appear
   exactly once across `crates/i2pr-daemon/src` — never one supervisor per spec.

### Manual verification (in addition to the scripts)

Confirmed absent from `crates/i2pr-service-tunnels/src`: `tokio` (0 matches),
`std::fs` (0), `async fn` / `async move` / `async {` (0), `spawn` (0), and any of
`TcpListener` / `TcpStream` / `UdpSocket` / `mpsc::` / `broadcast::` / `channel(` (0).

`std::net` **does** appear, 11 times, but only as the value types
`std::net::IpAddr` and `std::net::SocketAddr` in `config.rs`, `destination.rs`,
`http/target.rs`, `socks5/request.rs`, and `streamr.rs` — used to *validate and reject*
non-loopback addresses and to *reject* IP literals as targets. No I/O type or operation
appears. This is consistent with the boundary scripts, neither of which greps for
`std::net` at all.

**Grouped-`use` blind spot: not present in this crate.** A sibling agent reported that
the runtime checker's literal `std::net` grep can miss an import hidden inside a grouped
`use std::{…}` block. I checked explicitly: there are **zero** matches for
`use std::{` in this crate. All nine `use std` imports are single-line and fully visible
(`std::collections::{BTreeMap, BTreeSet, HashMap}` spelled out individually across
`access.rs`, `socks5/config.rs`, `generation.rs`, `connect.rs`, `irc/config.rs`,
`irc/server.rs`, `http/config.rs`, `http/server.rs`, and `streamr.rs`). No
`std::net` usage is hidden behind a grouped import, and no violation was found.

## Distinctive design choices

1. Every service tunnel is validated as *policy* before the daemon allocates anything, so
   a bad config fails with a typed error rather than a half-built listener.
2. Kinds are typed enum values with paired `parse` / `as_str` spellings, never loose
   strings after parsing.
3. `DestinationRef::parse` is strictly structural — no DNS, filesystem, network, or
   clearnet fallback — and IP literals are rejected outright.
4. Static aliases are lower-case and bounded, and a Base32 spelling is never an alias;
   canonical naming stays in `i2pr-addressbook` while this crate owns only the reference
   *policy*.
5. `ServerTarget` distinguishes loopback TCP from a bounded Unix path, and a Unix target
   is an explicit `not-yet-supported` rather than a silently ignored field.
6. `outbound_secret` is the only secret-bearing module, and it is deliberately
   *un-re-exported* at the crate root: no `Debug`, no `Display`, no `Clone`, a fixed-size
   `Zeroizing` stack buffer, and a fail-closed default store instead of a stub.
7. The inbound verifier and the outbound sealed form carry deliberately distinct markers
   (`$i2pr1$` vs `$i2pr1o$`) so one can never be replayed as the other.
8. `idle_decision` is a pure function with no clock, so every deadline edge — including
   the exact-deadline fire, the `Some(0)` case, and clock-step saturation — is unit
   testable without a timer.
9. `access.rs` and `auth.rs` model policy only; the authenticated peer hash and the
   accepted connection both arrive from the daemon, so this crate never owns a listener.
10. Parser, rewrite, and reply *grammars* are shared verbatim between the daemon executor
    and the runtime-neutral unit tests, so the executor owns sockets and Streaming lifetime
    but never re-implements a grammar.

## Status and plan authority

M10 is closed. Per `plans/registry.md:35` the Service tunnels subsystem is `closed`, with
**Plan 215 as product authority and Plan 204 superseded by Plan 248**.

| Plan | Scope | Result |
| --- | --- | --- |
| 173 | Planning authority | — |
| 174 | Foundation | — |
| 175 | Generic client/server tunnels | — |
| 176 | HTTP | — |
| 177 | SOCKS5 | — |
| 178 | IRC client | — |
| 179 | IRC server | — |
| 180 | Composition / reconcile / hardening | — |
| 182 | Local-delivery corrective | — |
| 181 | Independent local acceptance | 29/29 local rows passed |
| 213 | Router-backed generic A/B external qualification | `P213-N-passed`, hosted double-pass |
| 214 | Product-only remote HTTP + IRC application closure | `passed-m10-product-only-remote-http-and-irc-application-closure` |
| 215 | Hosted Plan 214 tunnel-config-generation corrective + exact-head reverification | `passed-m10-hosted-plan214-tunnel-config-generation-corrective-and-exact-head-reverification` |

The hosted double-pass landed first on `1992d67` (runs `35309158441` +
`35309655867`, 73/73 rows green) and was then re-proved on the post-dependabot-merge head
`a71e0193c420c8d8464fd05ff67d5323515ed4dd` (runs `35347780246` + `35349313549`, both
again `P214-N-passed` with `P213-N-passed` and 73/73 rows green).

**Authority note.** `plans/closure/*/*-status.md` wins over `plans/registry.md`. The
authoritative `milestone10_final_acceptance` value per
`plans/closure/service-tunnels/215-status.md` is
**`closed-on-a71e0193-pending-plan204-convergence`** (it advanced from
`closed-on-1992d67-pending-plan204-convergence` on the re-closure pass). The registry
records that the Plan 204 convergence record is retained as
`retained-convergence-record-superseded-by-plan248-policy-reconciliation`, with M10
authority remaining Plan 215, and the M6 Java second-family row (Plan 201 / Plan 247)
stays pending. This crate's local composition does not qualify external reachability or
any anonymity property.

## Cross-references

**ADRs**

- [0001 — Modular monolith](../adr/0001-modular-monolith.md)
- [0002 — Tokio runtime boundary](../adr/0002-tokio-runtime-boundary.md)
- [0003 — Bounded supervised services](../adr/0003-bounded-supervised-services.md)
- [0006 — Private identity storage](../adr/0006-private-identity-storage.md)
- [0010 — Transport contracts and crate boundaries](../adr/0010-transport-contracts-and-crate-boundaries.md)
- [0028 — i2pcontrol / Proposal 170 control plane](../adr/0028-i2pcontrol-proposal-170-control-plane.md)
- [0030 — Destination linkability domains / service lifecycle](../adr/0030-destination-linkability-domains-service-lifecycle-and-i2pd-streaming.md)
- [0031 — One shared service tunnel manager](../adr/0031-one-shared-service-tunnel-manager.md)

**Plans and closure records**

- `plans/subsystems/service-tunnels-roadmap.md` (M10 roadmap)
- `plans/closure/service-tunnels/213-status.md`
- `plans/closure/service-tunnels/214-status.md`
- `plans/closure/service-tunnels/215-status.md`
- `plans/closure/mixed-router-interop/248-status.md` (Plan 248, supersedes Plan 204)
- `plans/registry.md` (subsystem status and M10 sequence)

**Specifications and reference notes**

- `specs/protocols/11-service-tunnels.md` (M10 dossier; the authoritative source for the
  Plan 177 §11 and Plan 178 §12 fail-closed non-support lists)
- `specs/protocols/12-repliable-datagrams-streamr.md` (frozen Streamr behavior reference)
- `specs/CONFORMANCE.md` (what counts as evidence)
- `specs/references/proposal-170-outbound-secret-owner.md` — directly relevant: it defines
  the `$i2pr1$` / `$i2pr1o$` marker separation that `src/outbound_secret.rs` implements
- `specs/references/proposal-170-transit-volume-and-share.md` — the same Proposal 170
  tunnel-shaping family that `TunnelShaping` and `IdlePolicy` model

**Related deep dives**

- [i2pr-daemon.md](i2pr-daemon.md) — the sole M10 socket, task, listener, and composition
  owner, plus the five `service_tunnels_*` executors
- [i2pr-client.md](i2pr-client.md) — destination lifecycle, ECIES session/routing,
  Streaming; the allowlisted-but-unused edge for this crate
- [i2pr-proto.md](i2pr-proto.md) — the one workspace crate this manifest actually depends on
- [i2pr-addressbook.md](i2pr-addressbook.md) — canonical destination naming, the other
  half of the naming boundary
- [i2pr-storage.md](i2pr-storage.md) — the versioned, atomic, secret-safe persistent
  service-destination store used by server kinds
- [i2pr-runtime.md](i2pr-runtime.md) — the runtime ownership this crate must not take
- [i2pr-i2pcontrol.md](i2pr-i2pcontrol.md) — the Proposal 170 control-plane surface
  (ADR 0028)
- [dependency-graph.md](dependency-graph.md) — the allowlist mirroring
  `scripts/check-dependency-direction.sh`
- [overview.md](overview.md) — crate index and data flow
- [tooling.md](tooling.md) — scripts, fixtures, lanes, CI
