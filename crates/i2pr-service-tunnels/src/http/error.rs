//! Plan 176 typed HTTP errors.
//!
//! All errors are structural and carry no socket handles, no private
//! application bytes, and no headers. They are emitted as typed
//! failure modes for the daemon to map to bounded error responses
//! (see [`crate::http::response::build_error_response`]).

#![forbid(unsafe_code)]

use thiserror::Error;

/// One typed HTTP failure category. Used by the error response
/// generator to choose a status code (see Plan 176 §5).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum HttpErrorKind {
    /// Request line framing is malformed or unreadable.
    MalformedRequestLine,
    /// Header section framing is malformed (CRLF, obs-fold, count,
    /// length, control bytes).
    MalformedHeaders,
    /// Field name or value is malformed (empty name, NUL, control,
    /// obs-fold, overlong).
    MalformedField,
    /// Request-target URI is malformed (URI/authority/port/path).
    MalformedTarget,
    /// Request scheme is unsupported (e.g. `https`, `ftp`, `ws`).
    UnsupportedScheme,
    /// Request authority is non-`.i2p` (clearnet, IP literal,
    /// `localhost`, mixed-suffix confusion).
    NonI2pAuthority,
    /// Request authority contains userinfo.
    UserinfoInAuthority,
    /// `Content-Length` values conflict, or `Transfer-Encoding`
    /// is ambiguous with `Content-Length`.
    SmugglingAmbiguity,
    /// CONNECT authority port is empty or unsupported.
    UnsupportedConnectPort,
    /// Total bytes buffered exceed the configured ceiling.
    BufferCeilingExceeded,
    /// Generated error response exceeds the configured ceiling.
    ResponseCeilingExceeded,
    /// Limits were misconfigured.
    InvalidLimits,
    /// Streaming connect, lookup, or remote failure (502 Bad Gateway).
    BadGateway,
    /// Connect or read deadline expired (504 Gateway Timeout).
    GatewayTimeout,
    /// Catch-all malformed input.
    Other,
}

impl HttpErrorKind {
    /// Maps an [`HttpErrorKind`] to the status code and reason
    /// phrase the error-response generator emits by default.
    pub fn default_status(self) -> (u16, &'static str) {
        match self {
            HttpErrorKind::MalformedRequestLine => (400, "Bad Request"),
            HttpErrorKind::MalformedHeaders => (400, "Bad Request"),
            HttpErrorKind::MalformedField => (400, "Bad Request"),
            HttpErrorKind::MalformedTarget => (400, "Bad Request"),
            HttpErrorKind::UnsupportedScheme => (400, "Bad Request"),
            HttpErrorKind::NonI2pAuthority => (403, "Forbidden"),
            HttpErrorKind::UserinfoInAuthority => (400, "Bad Request"),
            HttpErrorKind::SmugglingAmbiguity => (400, "Bad Request"),
            HttpErrorKind::UnsupportedConnectPort => (403, "Forbidden"),
            HttpErrorKind::BufferCeilingExceeded => (400, "Bad Request"),
            HttpErrorKind::ResponseCeilingExceeded => (500, "Internal Server Error"),
            HttpErrorKind::InvalidLimits => (500, "Internal Server Error"),
            HttpErrorKind::BadGateway => (502, "Bad Gateway"),
            HttpErrorKind::GatewayTimeout => (504, "Gateway Timeout"),
            HttpErrorKind::Other => (400, "Bad Request"),
        }
    }
}

/// Typed HTTP error carrying a kind plus a machine-readable reason.
#[derive(Clone, Debug, Eq, PartialEq, Error)]
#[error("http: {kind:?}: {reason}")]
pub struct HttpError {
    /// Failure category.
    pub kind: HttpErrorKind,
    /// Short machine-readable reason.
    pub reason: &'static str,
}

impl HttpError {
    /// Builds one error with a static reason string.
    pub fn new(kind: HttpErrorKind, reason: &'static str) -> Self {
        Self { kind, reason }
    }
}
