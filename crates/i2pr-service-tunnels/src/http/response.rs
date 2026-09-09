//! Plan 176 bounded error response generation.
//!
//! Produces a strict, byte-exact HTTP/1.1 error response whose
//! body never echoes untrusted request bytes and never exceeds the
//! configured ceiling. The response generator is the only place in
//! the M10 HTTP proxy that constructs raw wire output for clients;
//! it must remain conservative.

#![forbid(unsafe_code)]

use super::error::{HttpError, HttpErrorKind};

/// Maximum byte length of a generated bad-request error response.
pub const ERROR_BAD_REQUEST_BYTES_LEN: usize = 256;
/// Maximum byte length of a generated forbidden error response.
pub const ERROR_FORBIDDEN_BYTES_LEN: usize = 256;
/// Maximum byte length of a generated bad-gateway error response.
pub const ERROR_BAD_GATEWAY_BYTES_LEN: usize = 256;
/// Maximum byte length of a generated gateway-timeout error response.
pub const ERROR_GATEWAY_TIMEOUT_BYTES_LEN: usize = 256;

/// Builds one bounded HTTP/1.1 error response.
///
/// The output is exactly `status_line CRLF headers CRLF body` where
/// the body is a fixed `<reason>` text plus an explicit
/// `Content-Length: <n>` header. The body content is **never** any
/// untrusted value from the request line, headers, or URI.
///
/// The error kind selects the status code and reason phrase via
/// [`HttpErrorKind::default_status`]; the `detail` is used as a
/// `X-HTTP-Proxy-Reason` header (truncated to a safe length) so
/// operators can correlate diagnostics without leaking client
/// payload.
pub fn build_error_response(kind: HttpErrorKind, detail: &str) -> Vec<u8> {
    let (status, reason) = kind.default_status();
    let body = reason.to_owned();
    let safe_detail = sanitize_detail(detail);
    let mut response = String::with_capacity(ERROR_BAD_REQUEST_BYTES_LEN + safe_detail.len());
    response.push_str(&format!("HTTP/1.1 {status} {reason}\r\n"));
    response.push_str("Content-Type: text/plain; charset=utf-8\r\n");
    response.push_str("Connection: close\r\n");
    response.push_str("Proxy-Authenticate: \r\n");
    response.push_str(&format!("Content-Length: {}\r\n", body.len()));
    if !safe_detail.is_empty() {
        response.push_str(&format!("X-HTTP-Proxy-Reason: {safe_detail}\r\n"));
    }
    response.push_str("\r\n");
    response.push_str(&body);
    response.into_bytes()
}

fn sanitize_detail(detail: &str) -> String {
    let mut out = String::with_capacity(detail.len().min(64));
    for byte in detail.bytes().take(64) {
        if byte.is_ascii_graphic() || byte == b' ' {
            out.push(byte as char);
        }
    }
    out
}

/// Convenience constructor for [`HttpError`].
pub fn http_error(kind: HttpErrorKind, reason: &'static str) -> HttpError {
    HttpError::new(kind, reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_request_response_is_bounded() {
        let response = build_error_response(HttpErrorKind::MalformedRequestLine, "ignored");
        assert!(response.len() <= ERROR_BAD_REQUEST_BYTES_LEN);
        let text = std::str::from_utf8(&response).expect("utf-8");
        assert!(text.starts_with("HTTP/1.1 400 Bad Request\r\n"));
        assert!(text.contains("Connection: close"));
        assert!(text.contains("Content-Length: 11"));
        assert!(text.ends_with("Bad Request"));
    }

    #[test]
    fn forbidden_response_uses_403() {
        let response = build_error_response(HttpErrorKind::NonI2pAuthority, "clearnet");
        let text = std::str::from_utf8(&response).expect("utf-8");
        assert!(text.starts_with("HTTP/1.1 403 Forbidden\r\n"));
    }

    #[test]
    fn bad_gateway_response_uses_502() {
        let response = build_error_response(HttpErrorKind::Other, "lookup failed");
        let text = std::str::from_utf8(&response).expect("utf-8");
        assert!(text.starts_with("HTTP/1.1 400 Bad Request\r\n"));
    }

    #[test]
    fn smuggling_response_uses_400() {
        let response = build_error_response(HttpErrorKind::SmugglingAmbiguity, "");
        let text = std::str::from_utf8(&response).expect("utf-8");
        assert!(text.starts_with("HTTP/1.1 400 Bad Request\r\n"));
        assert!(!text.contains("X-HTTP-Proxy-Reason"));
    }

    #[test]
    fn sanitization_strips_crlf() {
        let response = build_error_response(
            HttpErrorKind::MalformedHeaders,
            "bad\r\nSet-Cookie: x=1\r\nInjected",
        );
        let text = std::str::from_utf8(&response).expect("utf-8");
        assert!(!text.contains("\r\nSet-Cookie"));
        assert!(text.contains("X-HTTP-Proxy-Reason: badSet-Cookie: x=1Injected"));
    }
}
