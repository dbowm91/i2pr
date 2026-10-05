//! Centralized browser security response policy.
//!
//! One function builds every response the console emits, so no route can
//! ship a page without the hardening headers and no route can accidentally
//! introduce a permissive CORS header. The policy is intentionally strict
//! enough that the console needs no `unsafe-inline` allowance: the shell
//! ships no inline script or style, and any future inline *data* belongs in
//! a non-executable block rather than in a relaxed policy.

use axum::body::Body;
use axum::http::Response;
use axum::http::header::{self, HeaderName, HeaderValue};

/// The `Content-Security-Policy` applied to every console response.
///
/// - `default-src 'none'` denies everything not named below;
/// - `script-src 'self'` and `style-src 'self'` forbid remote and inline
///   code, which is what lets the policy omit `unsafe-inline`;
/// - `connect-src 'self'` confines the browser to this console's own JSON
///   endpoints, so a compromised page cannot beacon anywhere;
/// - `frame-ancestors 'none'` blocks clickjacking;
/// - `form-action 'self'` stops a console form posting off-origin;
/// - `base-uri 'none'` blocks base-tag redirection.
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; \
script-src 'self'; \
style-src 'self'; \
img-src 'self'; \
font-src 'self'; \
connect-src 'self'; \
form-action 'self'; \
frame-ancestors 'none'; \
base-uri 'none'; \
object-src 'none'";

/// `Referrer-Policy` for a local administrative surface.
///
/// `no-referrer` keeps the console URL — which can carry a login flow — out
/// of any `Referer` sent to a third party, and out of the local server's
/// access log for unrelated requests.
pub const REFERRER_POLICY: &str = "no-referrer";

/// `Cross-Origin-Opener-Policy` value.
pub const CROSS_ORIGIN_OPENER_POLICY: &str = "same-origin";

/// `Cross-Origin-Resource-Policy` value.
pub const CROSS_ORIGIN_RESOURCE_POLICY: &str = "same-origin";

/// `Permissions-Policy` value.
///
/// Every powerful browser feature is switched off; the console is a
/// documentation and read-only status surface, so none of them are needed.
pub const PERMISSIONS_POLICY: &str = "accelerometer=(), autoplay=(), camera=(), \
display-capture=(), encrypted-media=(), fullscreen=(self), geolocation=(), \
gyroscope=(), magnetometer=(), microphone=(), payment=(), publickey-credentials-get=(), \
screen-wake-lock=(), usb=(), xr-spatial-tracking=()";

/// The cookie name carrying the opaque console session.
pub const SESSION_COOKIE_NAME: &str = "i2pr_console_session";

/// The cookie name carrying the per-session CSRF token.
///
/// This is deliberately **not** `HttpOnly`: the browser must read it to
/// attach it to a state-changing request. That is safe because it is a
/// per-session nonce bound to the `HttpOnly` session cookie, and it is
/// useless without that cookie, which script running in the page cannot
/// read.
pub const CSRF_COOKIE_NAME: &str = "i2pr_console_csrf";

/// How a response may be cached.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CachePolicy {
    /// Administrative content: never stored, anywhere.
    NoStore,
    /// Immutable compiled assets: may be cached but must revalidate.
    ImmutableAsset,
}

/// Header names this module owns.
///
/// Declaring them once keeps the guard below honest: a response is judged
/// by looking for exactly these names.
static CORS_HEADER_NAMES: &[&str] = &[
    "access-control-allow-origin",
    "access-control-allow-credentials",
    "access-control-allow-methods",
    "access-control-allow-headers",
    "access-control-expose-headers",
    "access-control-max-age",
];

/// Applies the console security policy to `response`.
///
/// This is the only sanctioned way for a console handler to produce a
/// response body. It sets the hardening headers, applies the cache policy,
/// and then verifies that no CORS header is present, so a future handler
/// that tries to relax the policy fails loudly in tests rather than
/// shipping a cross-origin readable admin page.
pub fn apply_policy(response: &mut Response<Body>, content_type: &'static str, cache: CachePolicy) {
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static(REFERRER_POLICY),
    );
    headers.insert(
        HeaderName::from_static("cross-origin-opener-policy"),
        HeaderValue::from_static(CROSS_ORIGIN_OPENER_POLICY),
    );
    headers.insert(
        HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static(CROSS_ORIGIN_RESOURCE_POLICY),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static(PERMISSIONS_POLICY),
    );
    // Framing is denied by CSP `frame-ancestors`; the legacy header is kept
    // for user agents that predate CSP framing control.
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    let cache_value = match cache {
        CachePolicy::NoStore => "no-store",
        CachePolicy::ImmutableAsset => "no-cache",
    };
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache_value));

    assert!(
        !has_cors_header(response),
        "console responses must never carry a CORS header"
    );
}

/// Returns whether `response` carries any CORS header.
///
/// Used both as a post-condition in [`apply_policy`] and as an assertion in
/// tests over live responses.
pub fn has_cors_header(response: &Response<Body>) -> bool {
    response.headers().keys().any(|name| {
        let name = name.as_str();
        CORS_HEADER_NAMES
            .iter()
            .any(|forbidden| name.eq_ignore_ascii_case(forbidden))
    })
}

/// Builds a `Set-Cookie` value for the console session cookie.
///
/// The cookie is `HttpOnly` so page script can never read it,
/// `SameSite=Strict` so it is never attached to a cross-site request, and
/// scoped to `/` with **no** `Domain` attribute so it cannot widen to a
/// sibling host.
///
/// `Secure` is deliberately **not** set. The console speaks plain HTTP on
/// loopback by design and there is no TLS termination; browsers treat
/// `http://localhost` as a secure context, so the cookie would be dropped
/// on a loopback deployment if `Secure` were required. The consequence is
/// documented rather than papered over: the transport is readable by any
/// local process and by anything with access to loopback, and the console
/// is loopback-only in both modes. Authentication protects against a local
/// *user*, not against a local *root*.
pub fn session_cookie_header(token: &str, max_age_seconds: u64) -> String {
    format!(
        "{SESSION_COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age_seconds}"
    )
}

/// Builds a `Set-Cookie` value that clears the console session cookie.
///
/// The attributes must match [`session_cookie_header`] or the browser will
/// keep the original cookie.
pub fn clear_session_cookie_header() -> String {
    format!("{SESSION_COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0")
}

/// Builds a readable `Set-Cookie` value for the CSRF token.
///
/// The CSRF cookie is intentionally not `HttpOnly`: the page must attach it
/// to state-changing requests. `SameSite=Strict` still applies.
pub fn csrf_cookie_header(token: &str, max_age_seconds: u64) -> String {
    format!("{CSRF_COOKIE_NAME}={token}; Path=/; SameSite=Strict; Max-Age={max_age_seconds}")
}

/// Builds a readable `Set-Cookie` value that clears the CSRF cookie.
pub fn clear_csrf_cookie_header() -> String {
    format!("{CSRF_COOKIE_NAME}=; Path=/; SameSite=Strict; Max-Age=0")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_with_cache(cache: CachePolicy) -> Response<Body> {
        let mut response = Response::new(Body::from("x"));
        apply_policy(&mut response, "text/html; charset=utf-8", cache);
        response
    }

    #[test]
    fn every_response_carries_the_hardening_headers() {
        let response = response_with_cache(CachePolicy::NoStore);
        let headers = response.headers();
        assert_eq!(
            headers
                .get("content-security-policy")
                .and_then(|v| v.to_str().ok()),
            Some(CONTENT_SECURITY_POLICY)
        );
        assert_eq!(
            headers
                .get(header::X_CONTENT_TYPE_OPTIONS)
                .and_then(|v| v.to_str().ok()),
            Some("nosniff")
        );
        assert_eq!(
            headers
                .get(header::REFERRER_POLICY)
                .and_then(|v| v.to_str().ok()),
            Some("no-referrer")
        );
        assert_eq!(
            headers
                .get("cross-origin-opener-policy")
                .and_then(|v| v.to_str().ok()),
            Some("same-origin")
        );
        assert_eq!(
            headers
                .get("cross-origin-resource-policy")
                .and_then(|v| v.to_str().ok()),
            Some("same-origin")
        );
        assert!(headers.contains_key("permissions-policy"));
        assert_eq!(
            headers
                .get(header::X_FRAME_OPTIONS)
                .and_then(|v| v.to_str().ok()),
            Some("DENY")
        );
    }

    #[test]
    fn csp_denies_everything_and_never_allows_inline_or_remote_code() {
        assert!(CONTENT_SECURITY_POLICY.starts_with("default-src 'none'"));
        assert!(!CONTENT_SECURITY_POLICY.contains("unsafe-inline"));
        assert!(!CONTENT_SECURITY_POLICY.contains("unsafe-eval"));
        assert!(CONTENT_SECURITY_POLICY.contains("script-src 'self'"));
        assert!(CONTENT_SECURITY_POLICY.contains("style-src 'self'"));
        assert!(CONTENT_SECURITY_POLICY.contains("frame-ancestors 'none'"));
        assert!(CONTENT_SECURITY_POLICY.contains("form-action 'self'"));
        // No external origin may appear anywhere in the policy.
        assert!(!CONTENT_SECURITY_POLICY.contains("http:"));
        assert!(!CONTENT_SECURITY_POLICY.contains("https:"));
        assert!(!CONTENT_SECURITY_POLICY.contains("*"));
    }

    #[test]
    fn cache_policy_distinguishes_admin_content_from_assets() {
        assert_eq!(
            response_with_cache(CachePolicy::NoStore)
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some("no-store")
        );
        assert_eq!(
            response_with_cache(CachePolicy::ImmutableAsset)
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some("no-cache")
        );
    }

    #[test]
    fn responses_never_carry_cors_headers() {
        assert!(!has_cors_header(&response_with_cache(CachePolicy::NoStore)));
        let mut response = response_with_cache(CachePolicy::NoStore);
        response.headers_mut().insert(
            HeaderName::from_static("access-control-allow-origin"),
            HeaderValue::from_static("*"),
        );
        assert!(has_cors_header(&response));
    }

    #[test]
    fn session_cookie_is_httponly_samesite_strict_and_path_scoped() {
        let header = session_cookie_header("abc123", 900);
        assert!(header.contains("HttpOnly"));
        assert!(header.contains("SameSite=Strict"));
        assert!(header.contains("Path=/"));
        // No Domain attribute: a broad domain would expose the cookie to
        // unrelated hosts on the same machine.
        assert!(!header.to_lowercase().contains("domain="));
        assert!(header.starts_with("i2pr_console_session=abc123"));
    }

    #[test]
    fn session_cookie_clearing_matches_the_setting_attributes() {
        let set = session_cookie_header("abc", 60);
        let cleared = clear_session_cookie_header();
        for attribute in ["Path=/", "HttpOnly", "SameSite=Strict"] {
            assert!(set.contains(attribute));
            assert!(
                cleared.contains(attribute),
                "clearing must match {attribute}"
            );
        }
        assert!(cleared.contains("Max-Age=0"));
    }

    #[test]
    fn csrf_cookie_is_readable_by_design_but_still_samesite() {
        let header = csrf_cookie_header("csrf-token", 60);
        assert!(
            !header.contains("HttpOnly"),
            "the page must read the CSRF token"
        );
        assert!(header.contains("SameSite=Strict"));
        assert!(header.starts_with("i2pr_console_csrf=csrf-token"));
        assert!(clear_csrf_cookie_header().contains("Max-Age=0"));
    }

    #[test]
    fn console_and_csrf_cookies_never_collide() {
        assert_ne!(SESSION_COOKIE_NAME, CSRF_COOKIE_NAME);
    }
}
