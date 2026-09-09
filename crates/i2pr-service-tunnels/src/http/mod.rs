//! Plan 176 runtime-neutral HTTP/1.1 proxy module.
//!
//! Strict, bounded, smuggling-resistant HTTP/1.1 parser, header
//! rewrite policy, request-target validation, and bounded error
//! response generation for the M10 `.i2p` HTTP proxy.
//!
//! The module owns:
//! - parser bounds (line bytes, header bytes, header count, field
//!   name/value bytes, retained-bytes ceiling, generated error bytes);
//! - target validation (`.i2p`/Base32/static-alias only; clearnet,
//!   IP literals, `localhost`, mixed-suffix confusion, and
//!   non-`http` schemes hard-fail);
//! - hop-by-hop Connection header parsing + removal;
//! - privacy/hop-by-hop header rewrite policy (RFC 9110 + RFC 9112);
//! - bounded error response generation that never echoes untrusted
//!   request bytes.
//!
//! The module is runtime-neutral: no Tokio, no sockets, no timers,
//! no filesystem, no transport internals. Higher protocol/state
//! concerns (Streaming ownership, loopback TCP, per-connection
//! supervision, sibling-isolated byte pumps) belong to the daemon.
//!
//! ## Mandatory behavior
//!
//! - HTTP/1.1 only;
//! - parse absolute-form proxy requests, origin-form requests with
//!   a `.i2p` Host, and authority-form `CONNECT`;
//! - reject NUL/CR (other than CRLF), control bytes, obs-fold, and
//!   smuggling ambiguities (`Transfer-Encoding` plus
//!   `Content-Length`, conflicting `Content-Length`, malformed
//!   CRLF framing);
//! - reject non-`http` schemes in absolute-form;
//! - reject clearnet / IP literal / `localhost` / `.localhost`
//!   authorities;
//! - reject userinfo, empty CONNECT ports, overlong authorities,
//!   and overlong headers under deadline;
//! - normalize Host from the validated destination authority;
//! - force `Connection: close` for the first M10 profile;
//! - strip or normalize `Via`, `Forwarded`, `X-Forwarded-*`,
//!   `Proxy-Authorization`, `Proxy-Connection`, `Keep-Alive`,
//!   `TE`, `Trailer`, `Upgrade`, `Referer`, `From` per an explicit
//!   documented policy;
//! - never echo untrusted request bytes into error bodies.
//!
//! ## Reference profile (Plan 176 §2)
//!
//! Clean-room behavior from:
//!
//! - RFC 9110 (HTTP semantics);
//! - RFC 9112 (HTTP/1.1 message framing);
//! - current official I2P I2PTunnel HTTP documentation;
//! - Java I2P 2.13.0 (`9134f808337b401e8e53c73734c81fab04280c9d`)
//!   reference path
//!   `apps/i2ptunnel/java/src/net/i2p/i2ptunnel/I2PTunnelHTTPClient.java`.
//!
//! Do not copy Java implementation code.

#![forbid(unsafe_code)]

pub mod config;
pub mod error;
pub mod limits;
pub mod parser;
pub mod response;
pub mod rewrite;
pub mod target;

pub use config::{
    DEFAULT_USER_AGENT_VALUE, HTTP_REQUEST_LINE_MAX_BYTES, HttpClientOptions, PrivacyPolicy,
    UserAgentPolicy,
};
pub use error::{HttpError, HttpErrorKind};
pub use limits::HttpLimits;
pub use parser::{
    HeaderEntry, HeaderName, HttpRequestHead, ParseError, RequestLine, parse_request_head,
};
pub use response::{
    ERROR_BAD_GATEWAY_BYTES_LEN, ERROR_BAD_REQUEST_BYTES_LEN, ERROR_FORBIDDEN_BYTES_LEN,
    ERROR_GATEWAY_TIMEOUT_BYTES_LEN, build_error_response,
};
pub use rewrite::{rewrite_headers, validate_authority_host};
pub use target::{
    RequestTarget, TargetKind, TargetParseError, parse_authority_form, parse_request_target,
};
