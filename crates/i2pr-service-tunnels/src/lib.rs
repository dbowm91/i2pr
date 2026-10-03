//! Plan 174 Milestone 10 service-tunnel foundation.
//!
//! Runtime-neutral service-tunnel configuration model, destination
//! reference policy, typed errors, typed events/snapshots, plus
//! the Plan 176 runtime-neutral HTTP/1.1 parser, hop-by-hop +
//! privacy rewrite, request-target validation, and bounded error
//! response generation, plus the Plan 177 runtime-neutral SOCKS5
//! no-authentication negotiation, CONNECT request parser, and
//! bounded reply generator for the M10 `socks5-client` profile,
//! plus the Plan 178 runtime-neutral IRC line parser, IRCv3
//! message-tag framing, command classification + per-direction
//! allowlist, and client-to-network privacy filter (USER/PING/
//! QUIT/PART rewrites + CTCP/DCC policy) for the M10 `irc-client`
//! profile, plus the Plan 179 runtime-neutral IRC server
//! registration interceptor and authenticated peer Destination
//! hostname projection for the M10 `irc-server` profile.
//!
//! This crate owns no sockets, no Tokio tasks, no timers, no
//! filesystem access, no transport internals, no NetDB mutation, and
//! no Garlic/I2NP construction. The daemon remains the sole M10
//! socket/task/composition owner; this crate only validates
//! configuration and policy structurally.
//!
//! ```text
//! i2pr-client (destination/Streaming-facing types, future)
//!     ^
//!     |
//! i2pr-service-tunnels (policy/protocol only)
//!     ^
//!     |
//! i2pr-daemon (only socket/task/composition owner)
//! ```
//!
//! Plan 174/175 enabled `generic-client` / `generic-server`. Plan
//! 176 adds the runtime-neutral HTTP module for `http-client`. Plan
//! 177 adds the runtime-neutral SOCKS5 module for `socks5-client`
//! (plus Plan 290 bounded SOCKS4a CONNECT parity for the pinned
//! historical SOCKS 4/4a/5 profile). Plan 178 adds the
//! runtime-neutral IRC client module for `irc-client`. Plan 179
//! adds the runtime-neutral IRC server registration interceptor for
//! `irc-server`. Plan 290 adds the strict CONNECT-only client
//! profile (`connect-client`), the SOCKS+IRC composition
//! (`socks-irc`), the filtered HTTP server (`http-server`), and
//! the deprecated bidirectional HTTP server (`http-bidir-server`)
//! over the same shared primitives. No listener starts in
//! this crate and no Tokio primitive exists here.

#![forbid(unsafe_code)]

pub mod config;
pub mod connect;
pub mod destination;
pub mod errors;
pub mod events;
pub mod generation;
pub mod http;
pub mod idle;
pub mod irc;
pub mod socks5;
pub mod streamr;

pub use config::{
    DEFAULT_IDLE_TIMEOUT_MS, DestinationPolicy, IdlePolicy, LocalListenerSpec,
    MAX_ACTIVE_CONNECTIONS_AGGREGATE, MAX_ACTIVE_CONNECTIONS_PER_SERVICE,
    MAX_BUFFERED_BYTES_PER_DIRECTION, MAX_CONFIGURED_TARGETS, MAX_GROUP_ID_LEN,
    MAX_IDLE_TIMEOUT_MS, MAX_SERVICE_ID_LEN, MAX_SERVICE_TUNNELS, MAX_STATIC_ALIASES,
    MAX_TUNNEL_LENGTH_HOPS, MAX_TUNNEL_QUANTITY, MAX_UNIX_PATH_LEN,
    MIN_BUFFERED_BYTES_PER_DIRECTION, MIN_IDLE_TIMEOUT_MS, ServerTarget, ServiceClientGroupId,
    ServiceResourceLimits, ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet,
    ServiceTunnelSpec, TunnelShaping,
};
pub use connect::{CONNECT_DEFAULT_PORT, CONNECT_OPTIONS_MAX_PORTS, ConnectClientOptions};
pub use destination::{DestinationRef, StaticAliasTable};
pub use errors::ServiceTunnelError;
pub use events::{ServiceTunnelEvent, ServiceTunnelSnapshot};
pub use generation::{DiffClass, ServiceDiff, diff_sets, diff_spec, kind_string};
pub use http::{
    FilteredServerRequest, HeaderEntry, HeaderName, HttpClientOptions, HttpError, HttpErrorKind,
    HttpLimits, HttpRequestHead, ParseError, PrivacyPolicy, RequestLine, RequestTarget, TargetKind,
    TargetParseError, UserAgentPolicy, build_error_response, filter_server_request,
    filter_server_response, parse_authority_form, parse_request_head, parse_request_target,
    rewrite_headers,
};
pub use idle::{IdleSweepAction, idle_decision};
pub use irc::{
    IrcClientOptions, IrcCommand, IrcCommandClass, IrcDropReason, IrcError, IrcErrorKind,
    IrcLimits, IrcLineParser, IrcServerOptions, IrcServerRegistration, IrcTag, LineDirection,
    LineParserOutcome, ParsedLine, PingRewriteState, PrivacySubstitutions, ReasonRewritePolicy,
    RegistrationOutcome, RegistrationRejection, RegistrationState, TagsOutcome, TagsParser,
    classify_core as classify_irc_core, classify_post_tag_core, encode_b32_label,
    is_allowed as is_irc_command_allowed, is_command_allowed as is_irc_command_allowed_alias,
    project_peer_hostname,
};
pub use socks5::{
    ConnectDestination, ConnectPortPolicy, GreetingOutcome, GreetingParser, RequestOutcome,
    RequestParser, SOCKS4A_GRANTED, SOCKS4A_REJECTED, SOCKS4A_REPLY_LEN, Socks4aOutcome,
    Socks4aRequestParser, Socks5ClientOptions, Socks5Error, Socks5ErrorKind, Socks5Limits,
    Socks5ReplyCode, build_reply as build_socks5_reply,
    build_reply_from_code as build_socks5_reply_from_code, build_socks4a_reply,
};
pub use streamr::{
    DEFAULT_MAX_SUBSCRIBERS, DEFAULT_PAYLOAD_LIMIT_BYTES, DEFAULT_STREAMR_I2P_PORT,
    DEFAULT_SUBSCRIBE_INTERVAL_MS, DEFAULT_SUBSCRIPTION_EXPIRY_MS, MAX_PAYLOAD_LIMIT_BYTES,
    MAX_SUBSCRIBER_CEILING, StreamrOptions,
};
