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
//! profile.
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
//! 177 adds the runtime-neutral SOCKS5 module for `socks5-client`.
//! Plan 178 adds the runtime-neutral IRC module for `irc-client`.
//! No listener starts in this crate and no Tokio primitive exists
//! here.

#![forbid(unsafe_code)]

pub mod config;
pub mod destination;
pub mod errors;
pub mod events;
pub mod http;
pub mod irc;
pub mod socks5;

pub use config::{
    DestinationPolicy, LocalListenerSpec, MAX_ACTIVE_CONNECTIONS_AGGREGATE,
    MAX_ACTIVE_CONNECTIONS_PER_SERVICE, MAX_BUFFERED_BYTES_PER_DIRECTION, MAX_CONFIGURED_TARGETS,
    MAX_GROUP_ID_LEN, MAX_SERVICE_ID_LEN, MAX_SERVICE_TUNNELS, MAX_STATIC_ALIASES,
    MAX_UNIX_PATH_LEN, MIN_BUFFERED_BYTES_PER_DIRECTION, ServerTarget, ServiceClientGroupId,
    ServiceResourceLimits, ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet,
    ServiceTunnelSpec,
};
pub use destination::{DestinationRef, StaticAliasTable};
pub use errors::ServiceTunnelError;
pub use events::{ServiceTunnelEvent, ServiceTunnelSnapshot};
pub use http::{
    HeaderEntry, HeaderName, HttpClientOptions, HttpError, HttpErrorKind, HttpLimits,
    HttpRequestHead, ParseError, PrivacyPolicy, RequestLine, RequestTarget, TargetKind,
    TargetParseError, UserAgentPolicy, build_error_response, parse_authority_form,
    parse_request_head, parse_request_target, rewrite_headers,
};
pub use irc::{
    IrcClientOptions, IrcCommand, IrcCommandClass, IrcDropReason, IrcError, IrcErrorKind,
    IrcLimits, IrcLineParser, IrcTag, LineDirection, LineParserOutcome, ParsedLine,
    PingRewriteState, PrivacySubstitutions, ReasonRewritePolicy, TagsOutcome, TagsParser,
    classify_core as classify_irc_core, classify_post_tag_core,
    is_allowed as is_irc_command_allowed, is_command_allowed as is_irc_command_allowed_alias,
};
pub use socks5::{
    ConnectDestination, ConnectPortPolicy, GreetingOutcome, GreetingParser, RequestOutcome,
    RequestParser, Socks5ClientOptions, Socks5Error, Socks5ErrorKind, Socks5Limits,
    Socks5ReplyCode, build_reply as build_socks5_reply,
    build_reply_from_code as build_socks5_reply_from_code,
};
