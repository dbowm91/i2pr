# `i2pr-service-tunnels` — Deep Dive

Runtime-neutral Milestone 10 service-tunnel configuration,
destination references, policy, the Plan 175 generic
client/server tunnel composition surface owned by the daemon, and
the Plan 176 HTTP/1.1 proxy parser/rewrite/target-validation
surface.

Path:
- `crates/i2pr-service-tunnels/` (configuration + reference parsing + HTTP).
- `crates/i2pr-storage/src/service_destination.rs` (persistent server destinations).
- `crates/i2pr-daemon/src/service_tunnels.rs` (manager + listener runtime).
- `crates/i2pr-daemon/src/service_tunnels_http.rs` (HTTP proxy executor).

## Purpose

Plan 175 lands the first complete Milestone 10 application service
product; Plan 176 adds the first M10 application profile on top.
The crate builds on the Plan 174 foundation (`ServiceTunnelSet`,
`LocalListenerSpec`, `ServerTarget`, bounded resource/timeouts) and
adds:

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
- a versioned, atomic, secret-safe persistent service-destination
  storage seam (Plan 175);
- a daemon-owned manager that wires loopback TCP, I2P Streaming, and
  the existing Plan 149 local destination product path together
  (Plan 175 + Plan 176).

It must not own:

- Tokio, sockets, listeners, tasks, timers;
- filesystem access (lives in `i2pr-storage`);
- transport or tunnel-build internals;
- NetDB mutation;
- Garlic/I2NP construction;
- SAM or I2CP protocol parsing;
- SOCKS5 or IRC parsers (Plans 177-179).

The daemon remains the sole M10 socket/task/composition owner.
SOCKS5 / IRC profiles belong to later plans.

## Module layout

| Module | File | Responsibility | Key public types |
| --- | --- | --- | --- |
| `lib` | `crates/i2pr-service-tunnels/src/lib.rs` | Crate root, re-exports, architecture pointer | `ServiceTunnelId`, `DestinationRef`, `ServiceTunnelError`, HTTP re-exports |
| `config` | `crates/i2pr-service-tunnels/src/config.rs` | Typed kinds, policy, listener/target, limits, timeouts, set validation | `ServiceTunnelKind`, `DestinationPolicy`, `LocalListenerSpec`, `ServerTarget`, `ServiceResourceLimits`, `ServiceTimeouts`, `ServiceTunnelSpec`, `ServiceTunnelSet` |
| `destination` | `crates/i2pr-service-tunnels/src/destination.rs` | Base32/alias/configured parsing, alias table | `DestinationRef`, `StaticAliasTable` |
| `errors` | `crates/i2pr-service-tunnels/src/errors.rs` | Typed structural errors, no secrets | `ServiceTunnelError` |
| `events` | `crates/i2pr-service-tunnels/src/events.rs` | Value-only lifecycle events and snapshots | `ServiceTunnelEvent`, `ServiceTunnelSnapshot` |
| `http` | `crates/i2pr-service-tunnels/src/http/` | Plan 176 runtime-neutral HTTP/1.1 parser, target validator, hop-by-hop/privacy rewrite, bounded error response | `HttpLimits`, `HttpClientOptions`, `PrivacyPolicy`, `UserAgentPolicy`, `HttpRequestHead`, `RequestTarget`, `parse_request_head`, `rewrite_headers`, `build_error_response` |
| `service_destination` | `crates/i2pr-storage/src/service_destination.rs` | Versioned, atomic, secret-safe persistent service destination storage | `ServiceDestinationStore`, `ServiceDestinationRecord`, `ServiceDestinationStorageError` |
| `service_tunnels` | `crates/i2pr-daemon/src/service_tunnels.rs` | Generic client/server tunnel composition root | `ServiceTunnelManager`, `ServiceTunnelManagerConfig`, `ServiceRuntime`, `ServiceTunnelSnapshot`, `ClientTarget`, `DestinationFailure` |
| `service_tunnels_http` | `crates/i2pr-daemon/src/service_tunnels_http.rs` | Plan 176 HTTP client tunnel executor (supervisor + per-connection handler) | `HttpConnectionOutcome`, `run_http_connection`, `run_http_client_loop` |

Line counts (approximate at Plan 176 close): see files.

## Public surface

```text
i2pr_service_tunnels:
  pub use config::{ServiceTunnelId, ServiceClientGroupId, ServiceTunnelKind,
    DestinationPolicy, LocalListenerSpec, ServerTarget, ServiceResourceLimits,
    ServiceTimeouts, ServiceTunnelSpec, ServiceTunnelSet, MAX_*};
  pub use destination::{DestinationRef, StaticAliasTable};
  pub use errors::ServiceTunnelError;
  pub use events::{ServiceTunnelEvent, ServiceTunnelSnapshot};
  pub use http::{HeaderEntry, HeaderName, HttpClientOptions, HttpError,
    HttpErrorKind, HttpLimits, HttpRequestHead, ParseError, PrivacyPolicy,
    RequestLine, RequestTarget, TargetKind, TargetParseError, UserAgentPolicy,
    build_error_response, parse_authority_form, parse_request_head,
    parse_request_target, rewrite_headers};

i2pr_storage:
  pub ServiceDestinationStore, ServiceDestinationRecord,
  ServiceDestinationStorageError, decode_service_destination_bytes,
  MAX_SERVICE_DESTINATION_FILE_SIZE,
  SERVICE_DESTINATION_FILE_NAME, SERVICE_DESTINATIONS_SUBDIR,
  SERVICE_DESTINATION_FORMAT_VERSION.

i2pr_daemon::service_tunnels:
  pub ServiceTunnelManager, ServiceTunnelManagerConfig, ServiceRuntime,
  ServiceTunnelSnapshot, ClientTarget, DestinationFailure,
  register_service_tunnel_manager.

i2pr_daemon::service_tunnels_http:
  pub HttpConnectionOutcome, run_http_connection, run_http_client_loop.
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
`generic-server`. Plan 176 adds `http-client`. `socks5-client`,
`irc-client`, and `irc-server` remain rejected as not-yet-available
until their own plans.

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
  per-service Streaming listener for server tunnels.
- `start_supervisors(runtimes, children, cancellation)` spawns the
  per-service supervisor loops under the daemon's child scope.
- `shutdown()` cancels every per-service supervisor token.
- `lookup_local_service_destination(hash)` resolves a Base32 hash to
  a [`ClientTarget`] when the hash matches a service destination
  registered with the manager (cross-tunnel local delivery).
- `service_destination_b64(service_id)` returns the canonical SAM
  private-destination wrapper base64 for one service.
- `snapshot()` returns a sanitized accounting view (counts and
  identifiers only; no secrets).

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
`i2pr-service-tunnels` (including the Plan 176 `http` sub-module)
is runtime-neutral (`#![forbid(unsafe_code)]`, no Tokio, sockets,
listeners, tasks, or timers); the daemon owns all M10 sockets and
tasks.

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
  `http-client` requires `http_options`, non-HTTP kinds reject
  any options.
- Daemon-side: `crates/i2pr-daemon/src/config.rs` unit tests
  (disabled-by-default, unknown fields, loopback, enabled
  acceptance for `generic-client`/`generic-server`/`http-client`,
  SOCKS/IRC still rejected, duplicates, bounds) plus
  `crates/i2pr-daemon/tests/service_tunnels_foundation.rs`
  (Plan 174 black-box config/graph tests),
  `crates/i2pr-daemon/tests/service_tunnel_generic_product.rs`
  (Plan 175 manager-level tests), and
  `crates/i2pr-daemon/tests/service_tunnel_http_product.rs`
  (Plan 176 black-box tests: clearnet/IP/localhost/mixed-suffix
  rejection, non-`http`/userinfo/smuggling/HTTP/1.0 rejection,
  unknown `.i2p` 502, CONNECT port-policy enforcement, sibling
  isolation, slow-incomplete-header timeout, snapshot
  accounting).
- Pump-side: `crates/i2pr-daemon/src/destination_streaming.rs` (5
  deterministic pump tests, retained from Plan 174).

## Distinctive design choices

1. Runtime-neutral configuration by construction; the boundary
   script proves no Tokio or listener ownership in
   `i2pr-service-tunnels` (including the Plan 176 `http` sub-module).
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
9. `generic-client`, `generic-server`, and `http-client` may be
   `enabled = true` in this plan; SOCKS/IRC kinds remain rejected
   with an explicit field-level message.
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

## Cross-references

- Plans 173 (roadmap authority), 174 (foundation),
  175 (generic tunnels), 176 (this plan).
- `docs/architecture/i2pr-daemon.md` (manager + runtime surface).
- `docs/architecture/i2pr-storage.md` (persistent destination storage).
- `specs/protocols/11-service-tunnels.md` (M10 dossier).
- ADRs 0001 (modular monolith), 0002 (Tokio boundary), 0006 (private
  identity storage), 0010 (transport contracts).