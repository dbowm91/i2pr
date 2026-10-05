# Portable service-tunnel core contract v1

Status: Plan 349 architecture contract; supported Rust exports and package compatibility are finalized by Plan 350.

## Ownership

`i2pr-service-tunnels` owns transport-independent service-tunnel specifications, validation, destination/linkability policy, access and rate decisions, resource/idle policy, and bounded HTTP/SOCKS/IRC/CONNECT filtering. Native i2pr and external transports are adapters. This contract does not make the crate a router capability or product by itself.

The core may consume or produce bounded values and pure decisions. It does not own sockets, SAM or Streaming sessions, async runtimes, timers, DNS, filesystem or key persistence, process lifecycle, NetDB, tunnel pools, or router-global state. The adapter owns those resources, connection pumps, reconnect, monotonic clock source, admission/release, local listener/target descriptions, and transport-specific errors.

## Adapter inputs and outputs

An adapter constructs a validated `ServiceTunnelSpec`/`ServiceTunnelSet` using bounded identifiers, profile options, targets, timeouts, and resource limits. Local listener and target descriptions are policy values only; binding and connecting remain adapter work. Outbound destination references are structurally validated by the core; name resolution and router lookup remain adapter/composition responsibilities.

Destination group identifiers represent explicit linkability domains. Equal configured group keys mean intentional sharing; distinct/dedicated group keys remain separate even when service kind, port, or target is equal. Key references describe ownership/policy and do not load or persist keys. The adapter maps each group to its own transport identity/session owner while preserving intentional sharing.

For peer-dependent access, rate, or IRC projection, the adapter supplies the canonical peer hash only after authenticating the remote I2P Destination through its transport. Nicknames, hostnames, local socket addresses, SAM session IDs, and unauthenticated datagram sources are not substitutes. If authenticated identity is unavailable, peer-dependent policy fails closed or the profile is rejected.

Where policy depends on direction or ports, the adapter supplies normalized stream direction and local/remote port metadata. Rate and idle decisions use a caller-supplied monotonic timestamp/duration; the core does not read a clock. Generation/diff results describe policy changes; the adapter orders start/stop/replace, owns rollback and restart, and reports failures without mutating core policy invisibly.

The core's bounded errors/events classify policy and parsing outcomes. Adapters may map them to their own transport errors, but must redact identity, credentials, and payload material from logs and metrics. Resource leases/counters and their release-on-close/cancel semantics remain with the runtime owner unless an explicitly documented core policy object owns a bounded counter.

## Not part of this contract

SAM command parsing, version/capability negotiation, session management, `PRIMARY`/`MASTER` behavior, STREAM/DATAGRAM wire behavior, naming, reconnect/backoff, socket ownership, async traits, TLS, DNS, configuration-file syntax, persistence, key generation/storage, process lifecycle, FFI, and router probing are not core responsibilities.

## Module and export inventory (Plan 349)

The crate root currently exposes these module families. Plan 350 may narrow/stabilize this inventory; this table classifies intent, not a guarantee that every current symbol remains supported.

| Module | Current root exports | Reusable policy classification | Adapter-only / defer to Plan 350 |
|---|---|---|---|
| `access` | `MAX_ACCESS_LIST_ENTRIES`, `MAX_RATE_LIMIT_PEERS`, `ServerAccessPolicy`, `ServerConnectionRateLimiter`, `ServerConnectionRateLimits` | bounded allow/deny and peer rate policy | mutable limiter ownership/concurrency details |
| `auth` | `MAX_PROXY_PASSWORD_LEN`, `MAX_PROXY_USERNAME_LEN`, `PROXY_AUTH_REALM_CONNECT`, `PROXY_AUTH_REALM_HTTP`, `PROXY_AUTH_REALM_SOCKS`, `PROXY_VERIFIER_MARKER`, `ProxyCredentials`, `decode_basic_credentials` | bounded credential parsing and verifier policy | secret lifecycle; never expose stored secret internals |
| `config` | `DEFAULT_IDLE_TIMEOUT_MS`, `DEFAULT_STREAMING_CONNECT_DELAY_MS`, `DestinationCryptoPolicy`, `DestinationGroupId`, `DestinationGroupKey`, `DestinationGroupSpec`, `DestinationLeaseSetEncryptionPolicy`, `DestinationPolicy`, `DestinationSigningPolicy`, `IdlePolicy`, `LocalListenerSpec`, `MAX_ACTIVE_CONNECTIONS_AGGREGATE`, `MAX_ACTIVE_CONNECTIONS_PER_SERVICE`, `MAX_BUFFERED_BYTES_PER_DIRECTION`, `MAX_CONFIGURED_TARGETS`, `MAX_EFFECTIVE_DIRECTION_TUNNELS`, `MAX_GROUP_ID_LEN`, `MAX_IDLE_TIMEOUT_MS`, `MAX_SERVICE_ID_LEN`, `MAX_SERVICE_TUNNELS`, `MAX_STATIC_ALIASES`, `MAX_STREAMING_CONNECT_DELAY_MS`, `MAX_TUNNEL_BACKUP_QUANTITY`, `MAX_TUNNEL_LENGTH_HOPS`, `MAX_TUNNEL_LENGTH_VARIANCE`, `MAX_TUNNEL_QUANTITY`, `MAX_UNIX_PATH_LEN`, `MIN_BUFFERED_BYTES_PER_DIRECTION`, `MIN_IDLE_TIMEOUT_MS`, `MIN_STREAMING_CONNECT_DELAY_MS`, `ServerTarget`, `ServiceClientGroupId`, `ServiceKeyReference`, `ServiceResourceLimits`, `ServiceTimeouts`, `ServiceTunnelId`, `ServiceTunnelKind`, `ServiceTunnelSet`, `ServiceTunnelSpec`, `TunnelShaping`, `multihoming_start_index` | validated service specs, targets, limits, destination groups | local endpoint execution and key storage |
| `connect` | `CONNECT_DEFAULT_PORT`, `CONNECT_OPTIONS_MAX_PORTS`, `ConnectClientOptions` | CONNECT profile policy/options | socket connection |
| `destination` | `DestinationRef`, `StaticAliasTable` | structural destination references and alias validation | naming resolution / addressbook access |
| `errors` | `ServiceTunnelError` | bounded typed policy outcomes | logging/export transport |
| `events` | `ServiceTunnelEvent`, `ServiceTunnelSnapshot` | bounded policy snapshots | logging/export transport |
| `generation` | `DiffClass`, `ServiceDiff`, `diff_sets`, `diff_spec`, `kind_string` | deterministic set/spec diff classification | task/listener lifecycle application |
| `http` | `FilteredServerRequest`, `HeaderEntry`, `HeaderName`, `HttpClientOptions`, `HttpError`, `HttpErrorKind`, `HttpLimits`, `HttpPostLimiter`, `HttpPostLimits`, `HttpRequestHead`, `HttpServerPolicy`, `MAX_POST_LIMIT_PEERS`, `ParseError`, `PresentationClass`, `PrivacyPolicy`, `RequestLine`, `RequestTarget`, `TargetKind`, `TargetParseError`, `UserAgentPolicy`, `build_error_response`, `classify_presentation`, `filter_server_request`, `filter_server_request_with_policy`, `filter_server_response`, `parse_authority_form`, `parse_origin_form`, `parse_request_head`, `parse_request_target`, `proxy_auth_required`, `rewrite_headers` | bounded parsing, privacy rewrites, client/server policy filters | byte pumps and local target I/O |
| `idle` | `IdleSweepAction`, `idle_decision` | pure deadline/idle decision | timer ownership |
| `irc` | `IrcClientOptions`, `IrcCommand`, `IrcCommandClass`, `IrcDropReason`, `IrcError`, `IrcErrorKind`, `IrcLimits`, `IrcLineParser`, `IrcServerOptions`, `IrcServerRegistration`, `IrcTag`, `LineDirection`, `LineParserOutcome`, `ParsedLine`, `PingRewriteState`, `PrivacySubstitutions`, `ReasonRewritePolicy`, `RegistrationOutcome`, `RegistrationRejection`, `RegistrationState`, `TagsOutcome`, `TagsParser`, `classify_irc_core`, `classify_post_tag_core`, `encode_b32_label`, `is_irc_command_allowed`, `is_irc_command_allowed_alias`, `project_peer_hostname` | bounded parse, privacy policy, registration projection from authenticated identity | network session and stream ownership |
| `outbound_secret` | none (module is public, stored secret is not root-re-exported) | no root re-export; capability contract remains policy-owned | persistence/crypto key lifecycle; internal surface needs separate review |
| `outproxy` | `DEFAULT_OUTPROXY_ATTEMPTS`, `DEFAULT_OUTPROXY_CONNECT_TIMEOUT_MS`, `MAX_OUTPROXY_ATTEMPTS`, `MAX_OUTPROXY_AUTH_HEADER_LEN`, `MAX_OUTPROXY_BACKOFF_MS`, `MAX_OUTPROXY_CONNECT_TIMEOUT_MS`, `MAX_OUTPROXY_HOST_LABEL_LEN`, `MAX_OUTPROXY_HOST_LEN`, `MAX_OUTPROXY_LIST_ENTRIES`, `MAX_OUTPROXY_LIST_LEN`, `MAX_OUTPROXY_USERNAME_LEN`, `NoOutproxyProvider`, `OutproxyAuthHeader`, `OutproxyConfig`, `OutproxyEndpoint`, `OutproxyError`, `OutproxyFailure`, `OutproxyList`, `OutproxyPolicy`, `OutproxyProvider`, `OutproxyRoute`, `OutproxyTarget`, `OutproxyType` | typed endpoint/provider policy and bounded auth-header construction | route execution; root exports require security review |
| `socks5` | `ConnectDestination`, `ConnectPortPolicy`, `GreetingOutcome`, `GreetingParser`, `RequestOutcome`, `RequestParser`, `SOCKS4A_GRANTED`, `SOCKS4A_REJECTED`, `SOCKS4A_REPLY_LEN`, `Socks4aOutcome`, `Socks4aRequestParser`, `Socks5ClientOptions`, `Socks5Error`, `Socks5ErrorKind`, `Socks5Limits`, `Socks5ReplyCode`, `build_socks5_reply`, `build_socks5_reply_from_code`, `build_socks4a_reply` | bounded SOCKS request/reply policy | socket negotiation/forwarding |
| `streamr` | `DEFAULT_MAX_SUBSCRIBERS`, `DEFAULT_PAYLOAD_LIMIT_BYTES`, `DEFAULT_STREAMR_I2P_PORT`, `DEFAULT_SUBSCRIBE_INTERVAL_MS`, `DEFAULT_SUBSCRIPTION_EXPIRY_MS`, `MAX_PAYLOAD_LIMIT_BYTES`, `MAX_SUBSCRIBER_CEILING`, `StreamrOptions` | bounded profile values | SAM/Streaming session implementation |

The `lib.rs` root currently has wildcard-public modules and re-exports config, policy, parser, error, and value types from these modules. No public type is assigned transport authority. `std::net::IpAddr`/`SocketAddr` uses are structural/loopback policy values; no socket operation is present.

## Dependency and enforcement baseline

At Plan 349 the crate's production dependencies were `base64ct`, `i2pr-proto`, `sha2`, `subtle`, `thiserror`, and `zeroize`. Plan 350's full source/tree audit found no `i2pr_proto` reference outside the manifest and this module's stale architecture comment, removed the unused path dependency, and left the external production dependencies `base64ct`, `sha2`, `subtle`, `thiserror`, and `zeroize`. The crate now has no direct i2pr workspace dependency. The boundary checker rejects direct workspace dependencies, Tokio, socket/listener types, process/filesystem/DNS ownership, and Garlic/I2NP construction. Its negative checks have positive controls.

## Security and compatibility invariants

- Inputs remain bounded and fail closed; no implicit clearnet or DNS fallback.
- Authenticated peer identity is provenance-bearing input, never inferred from a display name or local endpoint.
- Explicit destination groups preserve identity/linkability policy.
- Secret-bearing types stay non-`Debug`, non-`Display`, and non-`Clone` where specified; no credential or payload logging.
- Native M10 behavior and `specs/support.toml` are unchanged by this contract.
- Public API/semver decisions are governed by Plan 350; this v1 document does not claim current export stability.
