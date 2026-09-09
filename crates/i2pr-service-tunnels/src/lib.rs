//! Plan 174 Milestone 10 service-tunnel foundation.
//!
//! Runtime-neutral service-tunnel configuration model, destination
//! reference policy, typed errors, typed events/snapshots, plus
//! the Plan 176 runtime-neutral HTTP/1.1 parser, hop-by-hop +
//! privacy rewrite, request-target validation, and bounded error
//! response generation.
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
//! 176 adds the runtime-neutral HTTP module for `http-client`; no
//! listener starts in this crate and no Tokio primitive exists here.

#![forbid(unsafe_code)]

pub mod config;
pub mod destination;
pub mod errors;
pub mod events;
pub mod http;

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
