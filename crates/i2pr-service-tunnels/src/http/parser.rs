//! Plan 176 HTTP/1.1 request-head parser.
//!
//! Reads a complete request-line plus header section from a bounded
//! byte buffer. Every length, count, and value has a hard ceiling
//! from [`crate::http::limits::HttpLimits`].
//!
//! The parser:
//!
//! - rejects CR without LF, lone LF without CR, bare CR, and any
//!   byte outside the printable ASCII / whitespace range in the
//!   header section;
//! - rejects NUL and any control bytes in field names/values;
//! - rejects obs-fold (line continuation with leading whitespace)
//!   rather than normalize it (Plan 176 §3.1 explicit-reject rule);
//! - rejects field names that are empty, contain `:`, or carry
//!   trailing whitespace;
//! - rejects duplicate `Host` authorities that disagree;
//! - rejects `Content-Length` value conflicts and `Transfer-Encoding`
//!   + `Content-Length` smuggling ambiguities;
//! - emits a typed [`HttpRequestHead`] with parsed method,
//!   request-target, and headers (each header is a name/value pair;
//!   values are single-line and never folded);
//!
//! The parser does not buffer HTTP bodies. The body is moved
//! through the same stream via the shared Plan 174 byte pump; the
//! caller is responsible for transferring any same-read body bytes
//! (returned as [`HttpRequestHead::initial_body_bytes`]) to the
//! pump.

#![forbid(unsafe_code)]

use super::config::HTTP_HEADER_COUNT_MAX;
use super::error::{HttpError, HttpErrorKind};
use super::limits::HttpLimits;

/// Owned, lower-cased header name.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct HeaderName(pub String);

impl HeaderName {
    /// Returns the canonical lower-case spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One parsed header line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeaderEntry {
    /// Lower-case header name.
    pub name: HeaderName,
    /// Header value (single line, trimmed of trailing whitespace,
    /// no folding).
    pub value: String,
}

impl HeaderEntry {
    /// Returns the canonical lower-case name string.
    pub fn name_str(&self) -> &str {
        self.name.as_str()
    }
}

/// Parsed HTTP request-line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestLine {
    /// HTTP method (uppercase ASCII).
    pub method: String,
    /// Raw request-target bytes (URI form preserved).
    pub target: String,
    /// HTTP version string; only `HTTP/1.1` is accepted.
    pub version: String,
}

/// Parsed HTTP request head.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpRequestHead {
    /// Parsed request line.
    pub line: RequestLine,
    /// Parsed headers in arrival order.
    pub headers: Vec<HeaderEntry>,
    /// First bytes of body that were already buffered after the
    /// terminating CRLF on the same read. Always empty in the
    /// runtime-neutral parser (the caller forwards bytes directly
    /// to the pump) but the field exists so the daemon can carry
    /// same-read bytes across the command->raw transition.
    pub initial_body_bytes: Vec<u8>,
}

/// Typed parser failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    /// Underlying typed HTTP error.
    pub error: HttpError,
}

impl From<HttpError> for ParseError {
    fn from(error: HttpError) -> Self {
        Self { error }
    }
}

impl ParseError {
    /// Constructs one error from an [`HttpErrorKind`] and reason.
    pub fn new(kind: HttpErrorKind, reason: &'static str) -> Self {
        Self {
            error: HttpError::new(kind, reason),
        }
    }
}

/// Parses a complete HTTP request head from the supplied buffer.
/// The buffer must contain at least the request line and the
/// terminating CRLF CRLF. If extra bytes follow the terminating
/// CRLF CRLF they are returned verbatim in
/// [`HttpRequestHead::initial_body_bytes`].
pub fn parse_request_head(
    buffer: &[u8],
    limits: HttpLimits,
) -> Result<HttpRequestHead, ParseError> {
    let rejected = |kind, reason| ParseError::new(kind, reason);
    if buffer.len() > limits.retained_buffer_max_bytes {
        return Err(rejected(
            HttpErrorKind::BufferCeilingExceeded,
            "input buffer exceeds retained ceiling",
        ));
    }
    let (head, body_offset) = find_header_end(buffer).ok_or_else(|| {
        rejected(
            HttpErrorKind::MalformedHeaders,
            "header section terminator not found within retained buffer",
        )
    })?;
    let head_str = std::str::from_utf8(head).map_err(|_| {
        rejected(
            HttpErrorKind::MalformedHeaders,
            "header bytes are not valid UTF-8",
        )
    })?;
    let mut lines = head_str.split("\r\n");
    let request_line = lines.next().ok_or_else(|| {
        rejected(
            HttpErrorKind::MalformedRequestLine,
            "request line is missing",
        )
    })?;
    let line = parse_request_line(request_line, limits.request_line_max_bytes)?;
    let mut headers: Vec<HeaderEntry> = Vec::new();
    for raw in lines {
        if raw.is_empty() {
            break;
        }
        if headers.len() >= limits.header_count_max {
            return Err(rejected(
                HttpErrorKind::MalformedHeaders,
                "header count exceeds ceiling",
            ));
        }
        if raw.len() > limits.total_header_bytes_max {
            return Err(rejected(
                HttpErrorKind::MalformedHeaders,
                "header line exceeds the ceiling",
            ));
        }
        if raw.starts_with(' ') || raw.starts_with('\t') {
            return Err(rejected(
                HttpErrorKind::MalformedField,
                "obs-fold is rejected",
            ));
        }
        let entry = parse_header_line(
            raw,
            limits.field_name_max_bytes,
            limits.field_value_max_bytes,
        )?;
        headers.push(entry);
    }
    enforce_no_smuggling(&headers, &line.method)?;
    enforce_host_uniqueness(&headers)?;
    Ok(HttpRequestHead {
        line,
        headers,
        initial_body_bytes: buffer[body_offset..].to_vec(),
    })
}

fn find_header_end(buffer: &[u8]) -> Option<(&[u8], usize)> {
    let mut index = 0;
    while index + 1 < buffer.len() {
        if buffer[index] == b'\r' && buffer[index + 1] == b'\n' {
            // Detect the empty line (CRLF CRLF) that ends the
            // header section.
            if index + 3 < buffer.len() && buffer[index + 2] == b'\r' && buffer[index + 3] == b'\n'
            {
                let end = index + 4;
                return Some((&buffer[..end], end));
            }
        }
        index += 1;
    }
    None
}

fn parse_request_line(input: &str, max_bytes: usize) -> Result<RequestLine, ParseError> {
    let rejected = |kind, reason| ParseError::new(kind, reason);
    if input.is_empty() {
        return Err(rejected(
            HttpErrorKind::MalformedRequestLine,
            "request line is empty",
        ));
    }
    if input.len() > max_bytes {
        return Err(rejected(
            HttpErrorKind::MalformedRequestLine,
            "request line exceeds ceiling",
        ));
    }
    if input.bytes().any(|b| b <= 0x1f || b == 0x7f) {
        return Err(rejected(
            HttpErrorKind::MalformedRequestLine,
            "request line contains control bytes",
        ));
    }
    let mut parts = input.split(' ');
    let method = parts.next().ok_or_else(|| {
        rejected(
            HttpErrorKind::MalformedRequestLine,
            "request line missing method",
        )
    })?;
    let target = parts.next().ok_or_else(|| {
        rejected(
            HttpErrorKind::MalformedRequestLine,
            "request line missing target",
        )
    })?;
    let version = parts.next().ok_or_else(|| {
        rejected(
            HttpErrorKind::MalformedRequestLine,
            "request line missing version",
        )
    })?;
    if parts.next().is_some() {
        return Err(rejected(
            HttpErrorKind::MalformedRequestLine,
            "request line has too many spaces",
        ));
    }
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(rejected(
            HttpErrorKind::MalformedRequestLine,
            "method must be uppercase ASCII",
        ));
    }
    if method.len() > HTTP_HEADER_COUNT_MAX {
        return Err(rejected(
            HttpErrorKind::MalformedRequestLine,
            "method exceeds the ceiling",
        ));
    }
    if version != "HTTP/1.1" {
        return Err(rejected(
            HttpErrorKind::MalformedRequestLine,
            "version must be HTTP/1.1",
        ));
    }
    Ok(RequestLine {
        method: method.to_owned(),
        target: target.to_owned(),
        version: version.to_owned(),
    })
}

fn parse_header_line(
    input: &str,
    name_max_bytes: usize,
    value_max_bytes: usize,
) -> Result<HeaderEntry, ParseError> {
    let rejected = |kind, reason| ParseError::new(kind, reason);
    let (raw_name, raw_value) = match input.split_once(':') {
        Some(value) => value,
        None => {
            return Err(rejected(
                HttpErrorKind::MalformedField,
                "header line missing colon",
            ));
        }
    };
    if raw_name.is_empty() {
        return Err(rejected(
            HttpErrorKind::MalformedField,
            "header name is empty",
        ));
    }
    if raw_name.len() > name_max_bytes {
        return Err(rejected(
            HttpErrorKind::MalformedField,
            "header name exceeds the ceiling",
        ));
    }
    let mut name_chars = raw_name.chars();
    let first = name_chars.next().expect("non-empty");
    if !(first.is_ascii_alphanumeric() || first == '!') {
        return Err(rejected(
            HttpErrorKind::MalformedField,
            "header name has invalid first byte",
        ));
    }
    for byte in raw_name.bytes() {
        let ok = byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'!' | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'-'
                    | b'.'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'|'
                    | b'~'
            );
        if !ok {
            return Err(rejected(
                HttpErrorKind::MalformedField,
                "header name contains forbidden character",
            ));
        }
    }
    let value = raw_value.trim();
    if value.is_empty() {
        return Err(rejected(
            HttpErrorKind::MalformedField,
            "header value is empty",
        ));
    }
    if value.len() > value_max_bytes {
        return Err(rejected(
            HttpErrorKind::MalformedField,
            "header value exceeds the ceiling",
        ));
    }
    if value
        .bytes()
        .any(|b| b == 0 || (b < 0x20 && b != b'\t') || b == 0x7f)
    {
        return Err(rejected(
            HttpErrorKind::MalformedField,
            "header value contains control bytes",
        ));
    }
    Ok(HeaderEntry {
        name: HeaderName(raw_name.to_ascii_lowercase()),
        value: value.to_owned(),
    })
}

fn enforce_no_smuggling(headers: &[HeaderEntry], method: &str) -> Result<(), ParseError> {
    let rejected = |kind, reason| ParseError::new(kind, reason);
    let mut content_length_values: Vec<&str> = Vec::new();
    let mut transfer_encoding_present = false;
    for entry in headers {
        match entry.name.as_str() {
            "content-length" => content_length_values.push(entry.value.as_str()),
            "transfer-encoding" => transfer_encoding_present = true,
            _ => {}
        }
    }
    if content_length_values.len() > 1 {
        let mut iter = content_length_values.iter();
        let first = iter.next().expect("at least two");
        for value in iter {
            if *value != *first {
                return Err(rejected(
                    HttpErrorKind::SmugglingAmbiguity,
                    "conflicting Content-Length values",
                ));
            }
        }
    }
    if transfer_encoding_present && !content_length_values.is_empty() {
        return Err(rejected(
            HttpErrorKind::SmugglingAmbiguity,
            "Transfer-Encoding with Content-Length is rejected",
        ));
    }
    // Methods without a body (GET, HEAD) must not carry Content-Length or
    // Transfer-Encoding; ordinary HTTP/1.1 forbids them.
    if (method == "GET" || method == "HEAD")
        && (!content_length_values.is_empty() || transfer_encoding_present)
    {
        return Err(rejected(
            HttpErrorKind::SmugglingAmbiguity,
            "GET/HEAD must not carry framing headers",
        ));
    }
    Ok(())
}

fn enforce_host_uniqueness(headers: &[HeaderEntry]) -> Result<(), ParseError> {
    let rejected = |kind, reason| ParseError::new(kind, reason);
    let mut host_values: Vec<&str> = Vec::new();
    for entry in headers {
        if entry.name.as_str() == "host" {
            host_values.push(entry.value.as_str());
        }
    }
    if host_values.len() > 1 {
        let mut iter = host_values.iter();
        let first = iter.next().expect("at least two");
        for value in iter {
            if *value != *first {
                return Err(rejected(
                    HttpErrorKind::MalformedHeaders,
                    "duplicate Host authority does not match",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> HttpLimits {
        HttpLimits::defaults()
    }

    fn parse(bytes: &[u8]) -> Result<HttpRequestHead, ParseError> {
        parse_request_head(bytes, limits())
    }

    #[test]
    fn parses_minimal_get() {
        let head = parse(b"GET http://example.i2p/path HTTP/1.1\r\nHost: example.i2p\r\n\r\n")
            .expect("parse");
        assert_eq!(head.line.method, "GET");
        assert_eq!(head.line.target, "http://example.i2p/path");
        assert_eq!(head.line.version, "HTTP/1.1");
        assert_eq!(head.headers.len(), 1);
        assert_eq!(head.headers[0].name_str(), "host");
        assert_eq!(head.headers[0].value, "example.i2p");
        assert!(head.initial_body_bytes.is_empty());
    }

    #[test]
    fn parses_initial_body_bytes() {
        let buffer = b"GET http://example.i2p/path HTTP/1.1\r\nHost: example.i2p\r\n\r\nPAYLOAD";
        let head = parse(buffer).expect("parse");
        assert_eq!(head.initial_body_bytes, b"PAYLOAD");
    }

    #[test]
    fn rejects_obs_fold() {
        let bytes = b"GET http://example.i2p/path HTTP/1.1\r\nHost: example.i2p\r\n extra\r\n\r\n";
        assert!(parse(bytes).is_err());
    }

    #[test]
    fn rejects_non_http11() {
        let bytes = b"GET http://example.i2p/ HTTP/1.0\r\nHost: example.i2p\r\n\r\n";
        assert!(parse(bytes).is_err());
    }

    #[test]
    fn rejects_lowercase_method() {
        let bytes = b"get http://example.i2p/ HTTP/1.1\r\nHost: example.i2p\r\n\r\n";
        assert!(parse(bytes).is_err());
    }

    #[test]
    fn rejects_lone_lf() {
        let bytes = b"GET http://example.i2p/path HTTP/1.1\nHost: example.i2p\n\n";
        assert!(parse(bytes).is_err());
    }

    #[test]
    fn rejects_bare_cr() {
        let bytes = b"GET http://example.i2p/path HTTP/1.1\rHost: example.i2p\r\r";
        assert!(parse(bytes).is_err());
    }

    #[test]
    fn rejects_conflicting_content_length() {
        let bytes = b"POST http://example.i2p/ HTTP/1.1\r\nHost: example.i2p\r\nContent-Length: 5\r\nContent-Length: 6\r\n\r\n";
        assert!(parse(bytes).is_err());
    }

    #[test]
    fn rejects_transfer_encoding_plus_content_length() {
        let bytes = b"POST http://example.i2p/ HTTP/1.1\r\nHost: example.i2p\r\nTransfer-Encoding: chunked\r\nContent-Length: 0\r\n\r\n";
        assert!(parse(bytes).is_err());
    }

    #[test]
    fn rejects_get_with_framing() {
        let bytes =
            b"GET http://example.i2p/ HTTP/1.1\r\nHost: example.i2p\r\nContent-Length: 0\r\n\r\n";
        assert!(parse(bytes).is_err());
    }

    #[test]
    fn rejects_duplicate_host() {
        let bytes =
            b"GET http://example.i2p/ HTTP/1.1\r\nHost: example.i2p\r\nHost: other.i2p\r\n\r\n";
        assert!(parse(bytes).is_err());
    }

    #[test]
    fn rejects_empty_header_name() {
        let bytes = b"GET http://example.i2p/ HTTP/1.1\r\nHost: example.i2p\r\n: value\r\n\r\n";
        assert!(parse(bytes).is_err());
    }

    #[test]
    fn rejects_control_in_header_value() {
        let bytes = b"GET http://example.i2p/ HTTP/1.1\r\nHost: example.i2p\r\nX-Test: bad\x01value\r\n\r\n";
        assert!(parse(bytes).is_err());
    }

    #[test]
    fn rejects_request_line_too_long() {
        let long_target = format!("http://example.i2p/{}", "a".repeat(9000));
        let bytes = format!("GET {long_target} HTTP/1.1\r\nHost: example.i2p\r\n\r\n");
        assert!(parse(bytes.as_bytes()).is_err());
    }

    #[test]
    fn rejects_header_count_over_ceiling() {
        let mut bytes = Vec::from(&b"GET http://example.i2p/ HTTP/1.1\r\n"[..]);
        for i in 0..(HTTP_HEADER_COUNT_MAX + 5) {
            bytes.extend_from_slice(format!("X-Header-{i}: v\r\n").as_bytes());
        }
        bytes.extend_from_slice(b"\r\n");
        assert!(parse(&bytes).is_err());
    }
}
