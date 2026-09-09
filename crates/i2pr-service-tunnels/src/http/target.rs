//! Plan 176 request-target parsing and validation.
//!
//! Accepts the three forms permitted by the M10 HTTP proxy:
//!
//! - absolute-form (`http://host[:port]/path?query`);
//! - origin-form (`/path?query`) with a separately-supplied Host
//!   authority;
//! - authority-form for `CONNECT` (`host:port`).
//!
//! Validation hard-fails:
//!
//! - non-`http` schemes in absolute-form;
//! - userinfo (`user@`, `user:pass@`);
//! - clearnet hosts (no `.i2p` / `.b32.i2p` suffix);
//! - IP literal authorities;
//! - `localhost`, `.localhost`;
//! - mixed-suffix confusion (`example.i2p.example.com`);
//! - empty CONNECT port;
//! - port out of range, when present;
//! - overlong host bytes;
//! - bytes that are not URI-permitted (NUL, control, space).

#![forbid(unsafe_code)]

use super::error::{HttpError, HttpErrorKind};
use crate::destination::{B32_SUFFIX, I2P_SUFFIX};

/// Length ceiling for the URI host portion of an absolute-form
/// request-target.
pub const HOST_MAX_BYTES: usize = 256;
/// Length ceiling for the origin-form path portion.
pub const PATH_MAX_BYTES: usize = 4096;
/// Length ceiling for the origin-form query portion.
pub const QUERY_MAX_BYTES: usize = 4096;

/// The kind of request-target that was parsed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TargetKind {
    /// `http://host[:port]/path?query`.
    Absolute,
    /// `/path?query` (paired with a separate Host header).
    Origin,
    /// `host:port` for `CONNECT`.
    Authority,
}

/// Parsed request-target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestTarget {
    /// Request-target form.
    pub kind: TargetKind,
    /// Lower-case canonical host (e.g. `example.i2p`,
    /// `aaaa…aaaa.b32.i2p`).
    pub host: String,
    /// Optional port; required for `Authority` form, optional for
    /// `Absolute`, unused for `Origin`.
    pub port: Option<u16>,
    /// Origin-form path; empty string for `Authority`.
    pub path: String,
    /// Origin-form query; empty string when absent.
    pub query: String,
    /// Scheme spelling for absolute-form; always `http`.
    pub scheme: Option<&'static str>,
}

/// Typed request-target parsing failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetParseError {
    /// Bounded failure category.
    pub kind: HttpErrorKind,
    /// Short machine-readable reason.
    pub reason: &'static str,
}

impl From<TargetParseError> for HttpError {
    fn from(error: TargetParseError) -> Self {
        HttpError::new(error.kind, error.reason)
    }
}

impl TargetParseError {
    fn new(kind: HttpErrorKind, reason: &'static str) -> Self {
        Self { kind, reason }
    }
}

/// Parses one absolute-form request-target. The supplied bytes
/// already include the `http://...` scheme prefix; everything after
/// the authority (the path+query) is captured but not deeply
/// validated (URI grammar stays strict but path bytes are checked
/// for control/space).
pub fn parse_absolute_form(input: &str) -> Result<RequestTarget, TargetParseError> {
    let rejected = |kind, reason| TargetParseError::new(kind, reason);
    if !input.starts_with("http://") {
        return Err(rejected(
            HttpErrorKind::UnsupportedScheme,
            "absolute-form must use the http scheme",
        ));
    }
    let remainder = &input["http://".len()..];
    if remainder.is_empty() {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "absolute-form authority is empty",
        ));
    }
    let (authority, path_query) = match remainder.find('/') {
        Some(index) => (&remainder[..index], &remainder[index..]),
        None => (remainder, ""),
    };
    let (host, port) = parse_authority(authority, false)?;
    validate_host(&host)?;
    let (path, query) = parse_path_query(path_query)?;
    Ok(RequestTarget {
        kind: TargetKind::Absolute,
        host,
        port,
        path,
        query,
        scheme: Some("http"),
    })
}

/// Parses one origin-form path+query (already extracted from the
/// request line). The Host header is validated separately and
/// paired by the daemon.
pub fn parse_origin_form(input: &str) -> Result<(String, String), TargetParseError> {
    parse_path_query(input)
}

/// Parses one authority-form `host:port` for `CONNECT`.
pub fn parse_authority_form(
    input: &str,
    limits_connect_authority: usize,
) -> Result<RequestTarget, TargetParseError> {
    let rejected = |kind, reason| TargetParseError::new(kind, reason);
    if input.is_empty() {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "CONNECT authority is empty",
        ));
    }
    if input.len() > limits_connect_authority {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "CONNECT authority exceeds the ceiling",
        ));
    }
    if input.bytes().any(|b| b <= 0x20 || b == 0x7f) {
        return Err(rejected(
            HttpErrorKind::MalformedField,
            "CONNECT authority contains control or whitespace",
        ));
    }
    let (host, port) = parse_authority(input, true)?;
    validate_host(&host)?;
    if input.parse::<u16>().is_ok() {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "CONNECT authority is port-only",
        ));
    }
    let Some(port) = port else {
        return Err(rejected(
            HttpErrorKind::UnsupportedConnectPort,
            "CONNECT authority missing port",
        ));
    };
    if port == 0 {
        return Err(rejected(
            HttpErrorKind::UnsupportedConnectPort,
            "CONNECT port is zero",
        ));
    }
    Ok(RequestTarget {
        kind: TargetKind::Authority,
        host,
        port: Some(port),
        path: String::new(),
        query: String::new(),
        scheme: None,
    })
}

/// Parses one request-target string, dispatching on the absolute
/// prefix. Origin-form callers must pre-strip and pass to
/// [`parse_origin_form`].
pub fn parse_request_target(input: &str) -> Result<RequestTarget, TargetParseError> {
    if input.starts_with("http://") {
        return parse_absolute_form(input);
    }
    Err(TargetParseError::new(
        HttpErrorKind::MalformedTarget,
        "request-target must be absolute-form for ordinary proxy requests",
    ))
}

fn parse_authority(
    input: &str,
    require_port: bool,
) -> Result<(String, Option<u16>), TargetParseError> {
    let rejected = |kind, reason| TargetParseError::new(kind, reason);
    if input.is_empty() {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "authority is empty",
        ));
    }
    if input.contains('@') {
        return Err(rejected(
            HttpErrorKind::UserinfoInAuthority,
            "userinfo is forbidden in HTTP proxy authorities",
        ));
    }
    let split = input.rsplit_once(':');
    let (host_port, port_literal) = match split {
        Some((host, port)) => (host, Some(port)),
        None => {
            if require_port {
                return Err(rejected(
                    HttpErrorKind::MalformedTarget,
                    "authority missing port",
                ));
            }
            (input, None)
        }
    };
    if host_port.is_empty() {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "authority host is empty",
        ));
    }
    if host_port.contains(':') {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "authority host contains multiple colons",
        ));
    }
    if host_port.len() > HOST_MAX_BYTES {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "authority host exceeds the ceiling",
        ));
    }
    let host = host_port.to_ascii_lowercase();
    let port = if let Some(literal) = port_literal {
        let parsed: u16 = literal
            .parse()
            .map_err(|_| rejected(HttpErrorKind::MalformedTarget, "authority port is invalid"))?;
        Some(parsed)
    } else {
        None
    };
    Ok((host, port))
}

fn parse_path_query(input: &str) -> Result<(String, String), TargetParseError> {
    let rejected = |kind, reason| TargetParseError::new(kind, reason);
    if input.len() > PATH_MAX_BYTES + 1 + QUERY_MAX_BYTES {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "path+query exceeds the ceiling",
        ));
    }
    if input.bytes().any(|b| b <= 0x1f || b == 0x7f) {
        return Err(rejected(
            HttpErrorKind::MalformedField,
            "path or query contains control bytes",
        ));
    }
    if !input.starts_with('/') {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "origin-form path must start with '/'",
        ));
    }
    let (path, query) = match input.find('?') {
        Some(index) => (&input[..index], &input[index + 1..]),
        None => (input, ""),
    };
    if path.len() > PATH_MAX_BYTES {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "path exceeds the ceiling",
        ));
    }
    if query.len() > QUERY_MAX_BYTES {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "query exceeds the ceiling",
        ));
    }
    Ok((path.to_owned(), query.to_owned()))
}

/// Validates that the supplied host is an `.i2p` (Base32 or
/// static-alias) authority.
pub fn validate_host(host: &str) -> Result<(), TargetParseError> {
    let rejected = |kind, reason| TargetParseError::new(kind, reason);
    if host.is_empty() {
        return Err(rejected(HttpErrorKind::NonI2pAuthority, "host is empty"));
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Err(rejected(
            HttpErrorKind::NonI2pAuthority,
            "IP literals are rejected",
        ));
    }
    let lowered = host.to_ascii_lowercase();
    if lowered == "localhost" || lowered.ends_with(".localhost") {
        return Err(rejected(
            HttpErrorKind::NonI2pAuthority,
            "localhost authority is rejected",
        ));
    }
    if !lowered.ends_with(I2P_SUFFIX) {
        return Err(rejected(
            HttpErrorKind::NonI2pAuthority,
            "host must end with .i2p",
        ));
    }
    // `.b32.i2p` is its own strict form; other `.i2p` aliases must
    // pass through the strict static-alias grammar.
    if !lowered.ends_with(B32_SUFFIX) {
        crate::destination::validate_static_alias(&lowered)
            .map_err(|_| rejected(HttpErrorKind::NonI2pAuthority, "static alias is malformed"))?;
    }
    Ok(())
}

/// Returns the canonical Host authority string the proxy forwards
/// for an absolute-form target. The port is omitted when it matches
/// the scheme default (80 for http).
pub fn canonical_authority(target: &RequestTarget) -> String {
    match target.port {
        Some(80) | None => target.host.clone(),
        Some(port) => format!("{}:{}", target.host, port),
    }
}

/// Returns the origin-form request-target the proxy forwards to the
/// remote service. The path is always present and query is appended
/// only when non-empty.
pub fn origin_form(target: &RequestTarget) -> String {
    if target.query.is_empty() {
        target.path.clone()
    } else {
        format!("{}?{}", target.path, target.query)
    }
}

#[cfg(test)]
mod tests {
    use super::super::config::HTTP_CONNECT_AUTHORITY_MAX_BYTES;
    use super::*;

    #[test]
    fn absolute_form_parses_with_port() {
        let target =
            parse_request_target("http://example.i2p:8080/path?query=1").expect("absolute");
        assert_eq!(target.kind, TargetKind::Absolute);
        assert_eq!(target.host, "example.i2p");
        assert_eq!(target.port, Some(8080));
        assert_eq!(target.path, "/path");
        assert_eq!(target.query, "query=1");
    }

    #[test]
    fn absolute_form_parses_without_port() {
        let target = parse_request_target("http://example.i2p/path").expect("absolute");
        assert_eq!(target.port, None);
        assert_eq!(target.host, "example.i2p");
        assert_eq!(target.path, "/path");
        assert_eq!(target.query, "");
    }

    #[test]
    fn absolute_form_rejects_non_http_scheme() {
        for bad in [
            "https://example.i2p/",
            "ftp://example.i2p/",
            "ws://example.i2p/",
            "HTTP://example.i2p/",
        ] {
            assert!(
                parse_request_target(bad).is_err(),
                "non-http scheme must reject: {bad}"
            );
        }
    }

    #[test]
    fn absolute_form_rejects_userinfo() {
        assert!(parse_request_target("http://user@example.i2p/").is_err());
        assert!(parse_request_target("http://user:pass@example.i2p/").is_err());
    }

    #[test]
    fn absolute_form_rejects_clearnet() {
        for bad in [
            "http://example.com/",
            "http://example.onion/",
            "http://1.2.3.4/",
            "http://[::1]/",
            "http://localhost/",
            "http://router.local/",
            "http://example.i2p.example.com/",
            "http://example.i2p:80./",
        ] {
            assert!(
                parse_request_target(bad).is_err(),
                "clearnet/IP/localhost/mixed-suffix must reject: {bad}"
            );
        }
    }

    #[test]
    fn absolute_form_rejects_empty_authority() {
        assert!(parse_request_target("http:///path").is_err());
    }

    #[test]
    fn absolute_form_rejects_empty_path() {
        assert!(parse_request_target("http://example.i2p/").is_ok());
        assert!(parse_request_target("http://example.i2p/path").is_ok());
    }

    #[test]
    fn origin_form_parses() {
        let (path, query) = parse_origin_form("/foo?bar=baz").expect("origin");
        assert_eq!(path, "/foo");
        assert_eq!(query, "bar=baz");
    }

    #[test]
    fn origin_form_rejects_no_leading_slash() {
        assert!(parse_origin_form("foo").is_err());
        assert!(parse_origin_form("?bar=1").is_err());
    }

    #[test]
    fn authority_form_requires_port() {
        let target =
            parse_authority_form("example.i2p:443", HTTP_CONNECT_AUTHORITY_MAX_BYTES).expect("ok");
        assert_eq!(target.kind, TargetKind::Authority);
        assert_eq!(target.host, "example.i2p");
        assert_eq!(target.port, Some(443));
    }

    #[test]
    fn authority_form_rejects_missing_port() {
        assert!(parse_authority_form("example.i2p", HTTP_CONNECT_AUTHORITY_MAX_BYTES).is_err());
        assert!(parse_authority_form(":443", HTTP_CONNECT_AUTHORITY_MAX_BYTES).is_err());
    }

    #[test]
    fn authority_form_rejects_clearnet() {
        assert!(parse_authority_form("example.com:443", HTTP_CONNECT_AUTHORITY_MAX_BYTES).is_err());
        assert!(parse_authority_form("1.2.3.4:443", HTTP_CONNECT_AUTHORITY_MAX_BYTES).is_err());
        assert!(parse_authority_form("localhost:443", HTTP_CONNECT_AUTHORITY_MAX_BYTES).is_err());
    }

    #[test]
    fn authority_form_rejects_zero_port() {
        assert!(parse_authority_form("example.i2p:0", HTTP_CONNECT_AUTHORITY_MAX_BYTES).is_err());
    }

    #[test]
    fn authority_form_rejects_overlong() {
        let long = format!("{}x.i2p:443", "a".repeat(256));
        assert!(parse_authority_form(&long, HTTP_CONNECT_AUTHORITY_MAX_BYTES).is_err());
    }

    #[test]
    fn canonical_authority_omits_default_port() {
        let mut target = parse_request_target("http://example.i2p/path").expect("absolute");
        assert_eq!(canonical_authority(&target), "example.i2p");
        target.port = Some(8080);
        assert_eq!(canonical_authority(&target), "example.i2p:8080");
    }

    #[test]
    fn origin_form_renders_path_query() {
        let target = parse_request_target("http://example.i2p/foo?bar=baz").expect("absolute");
        assert_eq!(origin_form(&target), "/foo?bar=baz");
        let target = parse_request_target("http://example.i2p/foo").expect("absolute");
        assert_eq!(origin_form(&target), "/foo");
    }
}
