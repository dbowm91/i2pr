//! Plan 290 runtime-neutral HTTP server profile filter.
//!
//! Java I2PTunnel's HTTP server mode ("Creates a destination to a
//! local HTTP server ip:port") filters inbound requests before they
//! reach the local webserver: header validation, header spoof
//! protection, header size checks, `Host` replacement, `X-I2P`
//! provenance, forced `Connection: close`, hop-by-hop stripping,
//! buffered headers, POST throttling, slowloris timeouts, response
//! privacy stripping, and bounded error responses.
//!
//! This module owns the runtime-neutral half of that contract:
//!
//! - request side: origin-form enforcement (the client proxy always
//!   rewrites to origin-form, so absolute/authority forms are a
//!   framing mismatch here, never silently reinterpreted), required
//!   `Host`, `Transfer-Encoding` rejection (this profile never
//!   forwards dechunked bodies), hop-by-hop + identifying-header
//!   stripping, `Host` replacement with the configured local target
//!   authority (spoof protection), forced `Connection: close`;
//! - response side: status-line validation, hop-by-hop +
//!   `Server`/`Via` stripping, framing preservation
//!   (`Content-Length` / `Transfer-Encoding` / `Content-Encoding`
//!   pass through so the transparent `x-i2p-gzip` cooperation and
//!   chunked framing keep working), forced `Connection: close`.
//!
//! Timeouts, slowloris bounds, POST chunk pacing, and socket
//! orchestration belong to the daemon: this module only validates
//! and rewrites bounded byte sections. Peer-identity headers are
//! deliberately NOT injected (privacy-strict deviation from Java:
//! the local webserver never learns the client's destination; see
//! the closure record).

#![forbid(unsafe_code)]

use super::error::{HttpError, HttpErrorKind};
use super::parser::{HeaderEntry, HeaderName, HttpRequestHead};
use super::target::parse_origin_form;

/// Maximum bytes for the validated local authority replacement
/// (`ip:port` text the daemon derives from the server target).
pub const SERVER_AUTHORITY_MAX_BYTES: usize = 256;

/// Filtered server-side request ready to forward to the local
/// target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilteredServerRequest {
    /// Serialized request head (`METHOD origin-form HTTP/1.1` +
    /// filtered headers + forced `Connection: close`).
    pub head_bytes: Vec<u8>,
    /// Declared `Content-Length`, if exactly one valid header was
    /// present. The daemon paces body forwarding against this
    /// bound.
    pub content_length: Option<u64>,
}

/// Filters one parsed inbound request head for the HTTP server
/// profile. `local_authority` is the daemon-derived `ip:port` text
/// of the loopback server target that replaces the inbound `Host`.
pub fn filter_server_request(
    head: &HttpRequestHead,
    local_authority: &str,
) -> Result<FilteredServerRequest, HttpError> {
    let rejected = |kind, reason| HttpError::new(kind, reason);
    if head.line.method == "CONNECT" {
        return Err(rejected(
            HttpErrorKind::MethodNotAllowed,
            "server profile rejects CONNECT",
        ));
    }
    // Origin-form only: the client proxy rewrites absolute-form to
    // origin-form, so any other form here is a framing mismatch.
    // The client absolute-form parser is not reused: proxy
    // request-target semantics (absolute URIs naming remote
    // destinations) must not be conflated with server semantics
    // (a bare path on the local target).
    if !head.line.target.starts_with('/') {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "server profile requires origin-form",
        ));
    }
    let (_path, _query) = parse_origin_form(&head.line.target).map_err(HttpError::from)?;
    validate_local_authority(local_authority)?;
    // HTTP/1.1 requires Host; the parser already rejects
    // duplicate/conflicting Host headers, so at most one survives.
    let host_count = head
        .headers
        .iter()
        .filter(|entry| entry.name_str() == "host")
        .count();
    if host_count == 0 {
        return Err(rejected(
            HttpErrorKind::MalformedHeaders,
            "server profile requires a Host header",
        ));
    }
    // This profile never forwards dechunked bodies: reject chunked
    // framing instead of stripping the header and corrupting the
    // body framing for the local target.
    if head
        .headers
        .iter()
        .any(|entry| entry.name_str() == "transfer-encoding")
    {
        return Err(rejected(
            HttpErrorKind::SmugglingAmbiguity,
            "server profile rejects Transfer-Encoding",
        ));
    }
    let mut connection_list: Vec<String> = Vec::new();
    for entry in &head.headers {
        if entry.name_str() == "connection" {
            for token in entry.value.split(',') {
                let trimmed = token.trim();
                if !trimmed.is_empty() {
                    connection_list.push(trimmed.to_ascii_lowercase());
                }
            }
        }
    }
    let mut content_lengths: Vec<u64> = Vec::new();
    let mut output: Vec<HeaderEntry> = Vec::with_capacity(head.headers.len());
    let mut host_inserted = false;
    for entry in &head.headers {
        let name = entry.name_str();
        if name == "connection" || connection_list.iter().any(|listed| listed == name) {
            continue;
        }
        if matches!(
            name,
            "proxy-connection" | "keep-alive" | "te" | "trailer" | "upgrade" | "transfer-encoding"
        ) {
            continue;
        }
        // Identifying-header strip (same set as the client
        // profile; the local target must not learn browser
        // provenance from a forwarded request).
        match name {
            "via"
            | "forwarded"
            | "x-forwarded-for"
            | "x-forwarded-host"
            | "x-forwarded-proto"
            | "proxy-authorization"
            | "referer"
            | "from" => continue,
            _ => {}
        }
        if name == "content-length" {
            let length: u64 = entry.value.trim().parse().map_err(|_| {
                rejected(
                    HttpErrorKind::MalformedHeaders,
                    "Content-Length is not a valid integer",
                )
            })?;
            content_lengths.push(length);
            continue;
        }
        if name == "host" {
            if !host_inserted {
                output.push(HeaderEntry {
                    name: HeaderName("host".to_owned()),
                    value: local_authority.to_owned(),
                });
                host_inserted = true;
            }
            continue;
        }
        output.push(entry.clone());
    }
    // Duplicate Content-Length values are ambiguous even when
    // identical; fail closed.
    if content_lengths.len() > 1 {
        return Err(rejected(
            HttpErrorKind::SmugglingAmbiguity,
            "duplicate Content-Length",
        ));
    }
    let content_length = content_lengths.into_iter().next();
    if let Some(length) = content_length {
        output.push(HeaderEntry {
            name: HeaderName("content-length".to_owned()),
            value: length.to_string(),
        });
    }
    output.push(HeaderEntry {
        name: HeaderName("connection".to_owned()),
        value: "close".to_owned(),
    });
    let mut head_bytes = Vec::with_capacity(512);
    head_bytes.extend_from_slice(head.line.method.as_bytes());
    head_bytes.push(b' ');
    head_bytes.extend_from_slice(head.line.target.as_bytes());
    head_bytes.extend_from_slice(b" HTTP/1.1\r\n");
    for entry in &output {
        head_bytes.extend_from_slice(entry.name_str().as_bytes());
        head_bytes.extend_from_slice(b": ");
        head_bytes.extend_from_slice(entry.value.as_bytes());
        head_bytes.extend_from_slice(b"\r\n");
    }
    head_bytes.extend_from_slice(b"\r\n");
    Ok(FilteredServerRequest {
        head_bytes,
        content_length,
    })
}

/// Validates the daemon-supplied local authority replacement.
fn validate_local_authority(authority: &str) -> Result<(), HttpError> {
    let rejected = |kind, reason| HttpError::new(kind, reason);
    if authority.is_empty() || authority.len() > SERVER_AUTHORITY_MAX_BYTES {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "local authority exceeds the ceiling",
        ));
    }
    if authority.bytes().any(|byte| byte <= 0x20 || byte == 0x7f) {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "local authority contains control or whitespace bytes",
        ));
    }
    // A colon may only appear as the single host/port separator;
    // anything else (userinfo `@`, second colon) is rejected.
    if authority.contains('@') || authority.bytes().filter(|byte| *byte == b':').count() != 1 {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "local authority must be host:port",
        ));
    }
    Ok(())
}

/// Filters one raw response head section captured from the local
/// target. Returns the serialized filtered head
/// (`status-line + headers + forced Connection: close`).
pub fn filter_server_response(head_bytes: &[u8]) -> Result<Vec<u8>, HttpError> {
    use super::config::HTTP_TOTAL_HEADER_MAX_BYTES;
    let rejected = |kind, reason| HttpError::new(kind, reason);
    if head_bytes.len() > HTTP_TOTAL_HEADER_MAX_BYTES {
        return Err(rejected(
            HttpErrorKind::BufferCeilingExceeded,
            "response head exceeds the ceiling",
        ));
    }
    let head_text = std::str::from_utf8(head_bytes).map_err(|_| {
        rejected(
            HttpErrorKind::MalformedHeaders,
            "response head is not valid UTF-8",
        )
    })?;
    let mut lines = head_text.split("\r\n");
    let status_line = lines.next().ok_or_else(|| {
        rejected(
            HttpErrorKind::MalformedHeaders,
            "response head has no status line",
        )
    })?;
    validate_status_line(status_line)?;
    let mut connection_list: Vec<String> = Vec::new();
    let mut parsed: Vec<(String, String)> = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        let Some(colon) = line.find(':') else {
            return Err(rejected(
                HttpErrorKind::MalformedHeaders,
                "response header has no colon",
            ));
        };
        let (name, value) = (&line[..colon], &line[colon + 1..]);
        validate_response_field_name(name)?;
        let value = value.trim();
        if value.bytes().any(|byte| byte < 0x20 || byte == 0x7f) {
            return Err(rejected(
                HttpErrorKind::MalformedHeaders,
                "response value contains control bytes",
            ));
        }
        let lower = name.to_ascii_lowercase();
        if lower == "connection" {
            for token in value.split(',') {
                let trimmed = token.trim();
                if !trimmed.is_empty() {
                    connection_list.push(trimmed.to_ascii_lowercase());
                }
            }
            continue;
        }
        parsed.push((lower, value.to_owned()));
    }
    let mut output: Vec<u8> = Vec::with_capacity(head_bytes.len());
    output.extend_from_slice(status_line.as_bytes());
    output.extend_from_slice(b"\r\n");
    for (name, value) in &parsed {
        if connection_list.iter().any(|listed| listed == name) {
            continue;
        }
        // Privacy-problematic response headers plus hop-by-hop
        // leftovers. Framing (`content-length`,
        // `transfer-encoding`) and content coding
        // (`content-encoding`, including `x-i2p-gzip`) pass
        // through untouched.
        if matches!(
            name.as_str(),
            "proxy-connection" | "keep-alive" | "te" | "trailer" | "upgrade" | "via" | "server"
        ) {
            continue;
        }
        output.extend_from_slice(name.as_bytes());
        output.extend_from_slice(b": ");
        output.extend_from_slice(value.as_bytes());
        output.extend_from_slice(b"\r\n");
    }
    output.extend_from_slice(b"connection: close\r\n\r\n");
    Ok(output)
}

/// Validates one response status line (`HTTP/1.0|HTTP/1.1 SP DDD SP reason`).
fn validate_status_line(line: &str) -> Result<(), HttpError> {
    let rejected = |kind, reason| HttpError::new(kind, reason);
    let (version, rest) = line.split_once(' ').ok_or_else(|| {
        rejected(
            HttpErrorKind::MalformedHeaders,
            "response status line is malformed",
        )
    })?;
    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return Err(rejected(
            HttpErrorKind::MalformedHeaders,
            "response version must be HTTP/1.x",
        ));
    }
    let (code, _) = rest.split_once(' ').ok_or_else(|| {
        rejected(
            HttpErrorKind::MalformedHeaders,
            "response status line is malformed",
        )
    })?;
    if code.len() != 3 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(rejected(
            HttpErrorKind::MalformedHeaders,
            "response status must be three digits",
        ));
    }
    Ok(())
}

/// Validates one response field name (visible ASCII token, no
/// spaces, no colon, no control bytes).
fn validate_response_field_name(name: &str) -> Result<(), HttpError> {
    let rejected = |kind, reason| HttpError::new(kind, reason);
    if name.is_empty() || name.len() > super::config::HTTP_FIELD_NAME_MAX_BYTES {
        return Err(rejected(
            HttpErrorKind::MalformedField,
            "response field name exceeds the ceiling",
        ));
    }
    if name
        .bytes()
        .any(|byte| byte <= 0x20 || byte == 0x7f || byte == b':' || byte == b'\r' || byte == b'\n')
    {
        return Err(rejected(
            HttpErrorKind::MalformedField,
            "response field name is not a valid token",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::parser::parse_request_head;
    use super::*;
    use crate::http::HttpLimits;

    fn parse_head(raw: &str) -> HttpRequestHead {
        parse_request_head(raw.as_bytes(), HttpLimits::defaults()).expect("head parses")
    }

    #[test]
    fn origin_form_request_is_filtered() {
        let head =
            parse_head("GET /path?q=1 HTTP/1.1\r\nHost: example.i2p\r\nVia: 1.1 proxy\r\n\r\n");
        let filtered = filter_server_request(&head, "127.0.0.1:8080").expect("filters");
        let text = std::str::from_utf8(&filtered.head_bytes).expect("utf-8");
        assert!(text.starts_with("GET /path?q=1 HTTP/1.1\r\n"));
        assert!(text.contains("host: 127.0.0.1:8080\r\n"));
        assert!(!text.contains("example.i2p"));
        assert!(!text.to_ascii_lowercase().contains("via:"));
        assert!(text.contains("connection: close\r\n"));
        assert_eq!(filtered.content_length, None);
    }

    #[test]
    fn absolute_form_is_rejected() {
        let head = parse_head("GET http://example.i2p/path HTTP/1.1\r\nHost: example.i2p\r\n\r\n");
        let error = filter_server_request(&head, "127.0.0.1:8080").expect_err("absolute");
        assert_eq!(error.kind, HttpErrorKind::MalformedTarget);
    }

    #[test]
    fn connect_method_is_rejected() {
        let head = parse_head("CONNECT example.i2p:443 HTTP/1.1\r\nHost: example.i2p\r\n\r\n");
        let error = filter_server_request(&head, "127.0.0.1:8080").expect_err("connect");
        assert_eq!(error.kind, HttpErrorKind::MethodNotAllowed);
    }

    #[test]
    fn missing_host_is_rejected() {
        let head = parse_head("GET /path HTTP/1.1\r\nX-Custom: yes\r\n\r\n");
        let error = filter_server_request(&head, "127.0.0.1:8080").expect_err("host");
        assert_eq!(error.kind, HttpErrorKind::MalformedHeaders);
    }

    #[test]
    fn transfer_encoding_is_rejected() {
        let head = parse_head(
            "POST /submit HTTP/1.1\r\nHost: example.i2p\r\nTransfer-Encoding: chunked\r\n\r\n",
        );
        let error = filter_server_request(&head, "127.0.0.1:8080").expect_err("te");
        assert_eq!(error.kind, HttpErrorKind::SmugglingAmbiguity);
    }

    #[test]
    fn duplicate_content_length_is_rejected() {
        let raw = "POST /submit HTTP/1.1\r\nHost: example.i2p\r\nContent-Length: 5\r\nContent-Length: 5\r\n\r\n";
        // Identical duplicates either fail at the shared parser
        // (conflicting-length rule) or reach the filter and fail
        // closed there; both outcomes are acceptable.
        match parse_request_head(raw.as_bytes(), HttpLimits::defaults()) {
            Err(_) => {}
            Ok(head) => {
                let result = filter_server_request(&head, "127.0.0.1:8080");
                assert!(
                    result.is_err(),
                    "duplicate Content-Length must fail, got {result:?}"
                );
            }
        }
    }

    #[test]
    fn content_length_is_reported() {
        let head =
            parse_head("POST /submit HTTP/1.1\r\nHost: example.i2p\r\nContent-Length: 11\r\n\r\n");
        let filtered = filter_server_request(&head, "127.0.0.1:8080").expect("filters");
        assert_eq!(filtered.content_length, Some(11));
        let text = std::str::from_utf8(&filtered.head_bytes).expect("utf-8");
        assert!(text.contains("content-length: 11\r\n"));
    }

    #[test]
    fn connection_listed_headers_are_dropped() {
        let head = parse_head(
            "GET / HTTP/1.1\r\nHost: example.i2p\r\nConnection: X-Custom\r\nX-Custom: evil\r\n\r\n",
        );
        let filtered = filter_server_request(&head, "127.0.0.1:8080").expect("filters");
        let text = std::str::from_utf8(&filtered.head_bytes).expect("utf-8");
        assert!(!text.to_ascii_lowercase().contains("x-custom"));
    }

    #[test]
    fn bad_local_authority_is_rejected() {
        let head = parse_head("GET / HTTP/1.1\r\nHost: example.i2p\r\n\r\n");
        assert!(filter_server_request(&head, "").is_err());
        assert!(filter_server_request(&head, "127.0.0.1:8080\r\nEvil: 1").is_err());
        assert!(filter_server_request(&head, "user@127.0.0.1:8080").is_err());
    }

    #[test]
    fn response_filters_privacy_headers() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nServer: secret/1.0\r\nVia: 1.1 proxy\r\nConnection: keep-alive\r\nContent-Length: 5\r\n\r\n";
        let filtered = filter_server_response(raw).expect("filters");
        let text = std::str::from_utf8(&filtered).expect("utf-8");
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("content-type: text/html\r\n"));
        assert!(text.contains("content-length: 5\r\n"));
        assert!(!text.to_ascii_lowercase().contains("server:"));
        assert!(!text.to_ascii_lowercase().contains("via:"));
        assert!(!text.contains("keep-alive"));
        assert!(text.contains("connection: close\r\n"));
    }

    #[test]
    fn response_preserves_framing_and_coding() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Encoding: x-i2p-gzip\r\n\r\n";
        let filtered = filter_server_response(raw).expect("filters");
        let text = std::str::from_utf8(&filtered).expect("utf-8");
        assert!(text.contains("transfer-encoding: chunked\r\n"));
        assert!(text.contains("content-encoding: x-i2p-gzip\r\n"));
    }

    #[test]
    fn malformed_response_status_is_rejected() {
        assert!(filter_server_response(b"NOT-HTTP 200 OK\r\n\r\n").is_err());
        assert!(filter_server_response(b"HTTP/1.1 99 OK\r\n\r\n").is_err());
        assert!(filter_server_response(b"HTTP/2 200 OK\r\n\r\n").is_err());
        assert!(filter_server_response(b"HTTP/1.1 200\r\n\r\n").is_err());
    }
}
