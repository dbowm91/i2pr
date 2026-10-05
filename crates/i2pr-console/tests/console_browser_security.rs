//! End-to-end browser-security behaviour through the assembled router.
//!
//! These tests drive the real hardened router with real headers and real
//! cookie flows. The guard stack is exercised as middleware, not as a
//! directly-called function, so a change that bypasses the layer in the
//! router fails here.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures_executor::block_on;
use tower::util::ServiceExt;

use i2pr_console::security::headers::{CSRF_COOKIE_NAME, SESSION_COOKIE_NAME};
use i2pr_console::{
    ConsoleConfig, ConsoleSecret, ConsoleState, DEFAULT_THEME_NAME, SecurityPolicy, SessionLimits,
    ThrottleLimits, router,
};

const HOST: &str = "localhost:7070";

fn bound() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7070)
}

fn open_console() -> axum::Router {
    let policy = SecurityPolicy::unauthenticated(
        bound(),
        SessionLimits::default(),
        ThrottleLimits::default(),
    )
    .expect("policy builds");
    router(Arc::new(ConsoleState::new(
        ConsoleConfig::new(DEFAULT_THEME_NAME),
        policy,
    )))
}

fn locked_console() -> axum::Router {
    let policy = SecurityPolicy::authenticated(
        bound(),
        ConsoleSecret::new("correct horse"),
        SessionLimits::default(),
        ThrottleLimits::default(),
    )
    .expect("policy builds");
    router(Arc::new(ConsoleState::new(
        ConsoleConfig::new(DEFAULT_THEME_NAME),
        policy,
    )))
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: String,
}

fn send(router: axum::Router, request: Request<Body>) -> Reply {
    let response = block_on(router.oneshot(request)).expect("router responds");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes =
        block_on(axum::body::to_bytes(response.into_body(), 1024 * 1024)).expect("body collects");
    Reply {
        status,
        headers,
        body: String::from_utf8(bytes.to_vec()).expect("body is utf-8"),
    }
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::HOST, HOST)
        .body(Body::empty())
        .expect("request builds")
}

fn post(uri: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::HOST, HOST)
        .header(header::ORIGIN, format!("http://{HOST}"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("password=correct+horse"))
        .expect("request builds")
}

/// Extracts the value of a named cookie from a `Set-Cookie` header list.
fn set_cookie(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| {
            let (pair, _) = value.split_once(';')?;
            let (key, val) = pair.split_once('=')?;
            (key.trim() == name).then(|| val.trim().to_string())
        })
}

// ---------------------------------------------------------------------------
// Authority validation
// ---------------------------------------------------------------------------

#[test]
fn requests_with_a_foreign_authority_are_refused() {
    for host in [
        "evil.test:7070",
        "localhost.evil.test:7070",
        "127.0.0.1:9999",
        "10.0.0.1:7070",
    ] {
        let request = Request::builder()
            .method("GET")
            .uri("/")
            .header(header::HOST, host)
            .body(Body::empty())
            .expect("request builds");
        let reply = send(open_console(), request);
        assert_eq!(
            reply.status,
            StatusCode::FORBIDDEN,
            "{host} must be refused"
        );
        assert!(
            !reply.body.contains("<!DOCTYPE"),
            "{host} must not be served the shell"
        );
    }
}

#[test]
fn requests_without_an_authority_are_refused() {
    let request = Request::builder()
        .method("GET")
        .uri("/")
        .body(Body::empty())
        .expect("request builds");
    let reply = send(open_console(), request);
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
}

#[test]
fn authority_validation_applies_in_authenticated_mode_too() {
    let request = Request::builder()
        .method("GET")
        .uri("/")
        .header(header::HOST, "evil.test:7070")
        .body(Body::empty())
        .expect("request builds");
    // Authentication must never become permission to skip authority checks.
    assert_eq!(
        send(locked_console(), request).status,
        StatusCode::FORBIDDEN
    );
}

#[test]
fn accepted_authority_is_served() {
    let reply = send(open_console(), get("/"));
    assert_eq!(reply.status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Cross-origin policy
// ---------------------------------------------------------------------------

#[test]
fn cross_origin_unsafe_requests_are_refused_before_the_handler() {
    let request = Request::builder()
        .method("POST")
        .uri("/api/fixture/mark")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, "http://evil.test")
        .body(Body::empty())
        .expect("request builds");
    let reply = send(open_console(), request);
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
}

#[test]
fn unsafe_requests_without_origin_or_referer_are_refused_by_default() {
    let request = Request::builder()
        .method("POST")
        .uri("/api/fixture/mark")
        .header(header::HOST, HOST)
        .body(Body::empty())
        .expect("request builds");
    assert_eq!(send(open_console(), request).status, StatusCode::FORBIDDEN);
}

#[test]
fn no_response_ever_carries_a_cors_header() {
    for uri in ["/", "/theme.css", "/assets/console.css", "/nope", "/login"] {
        let reply = send(open_console(), get(uri));
        for name in reply.headers.keys() {
            let name = name.as_str().to_ascii_lowercase();
            assert!(
                !name.starts_with("access-control-"),
                "{uri} leaked a CORS header: {name}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Security headers
// ---------------------------------------------------------------------------

#[test]
fn every_response_carries_the_hardening_headers() {
    for uri in ["/", "/theme.css", "/assets/console.css", "/nope", "/login"] {
        let reply = send(open_console(), get(uri));
        assert_eq!(
            reply
                .headers
                .get("content-security-policy")
                .and_then(|v| v.to_str().ok()),
            Some(i2pr_console::security::headers::CONTENT_SECURITY_POLICY),
            "{uri} must carry the CSP"
        );
        assert_eq!(
            reply
                .headers
                .get(header::X_CONTENT_TYPE_OPTIONS)
                .and_then(|v| v.to_str().ok()),
            Some("nosniff"),
            "{uri} must carry nosniff"
        );
        // Administrative content is never stored; compiled assets use the
        // narrower revalidating rule the plan permits. Neither is storable
        // as a durable authenticated page.
        let expected_cache = if uri.starts_with("/assets/") || uri == "/theme.css" {
            "no-cache"
        } else {
            "no-store"
        };
        assert_eq!(
            reply
                .headers
                .get(header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some(expected_cache),
            "{uri} cache policy"
        );
        assert!(reply.headers.contains_key("cross-origin-opener-policy"));
    }
}

#[test]
fn the_csp_forbids_inline_and_remote_code() {
    let reply = send(open_console(), get("/"));
    let csp = reply
        .headers
        .get("content-security-policy")
        .and_then(|v| v.to_str().ok())
        .expect("csp present")
        .to_string();
    assert!(!csp.contains("unsafe-inline"));
    assert!(!csp.contains("unsafe-eval"));
    assert!(csp.contains("frame-ancestors 'none'"));
    assert!(csp.contains("default-src 'none'"));
}

// ---------------------------------------------------------------------------
// Authentication, sessions, and CSRF
// ---------------------------------------------------------------------------

#[test]
fn authenticated_console_redirects_to_login_without_a_session() {
    let reply = send(locked_console(), get("/"));
    // A GET redirect must preserve the method, so 307 is the correct code.
    assert_eq!(reply.status, StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        reply
            .headers
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some("/login")
    );
}

#[test]
fn wrong_password_is_refused_and_the_failure_is_recorded() {
    let request = Request::builder()
        .method("POST")
        .uri("/login")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, format!("http://{HOST}"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("password=wrong"))
        .expect("request builds");
    let reply = send(locked_console(), request);
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert!(reply.body.contains("Incorrect password"));
    // No session cookie may be issued on a failed login.
    assert!(set_cookie(&reply.headers, SESSION_COOKIE_NAME).is_none());
}

#[test]
fn successful_login_issues_both_cookies_with_the_required_attributes() {
    let reply = send(locked_console(), post("/login"));
    assert_eq!(reply.status, StatusCode::SEE_OTHER);

    let session = set_cookie(&reply.headers, SESSION_COOKIE_NAME).expect("session cookie");
    assert_eq!(session.len(), 64);
    let raw = reply
        .headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(SESSION_COOKIE_NAME))
        .expect("raw session cookie header")
        .to_string();
    assert!(raw.contains("HttpOnly"));
    assert!(raw.contains("SameSite=Strict"));
    assert!(raw.contains("Path=/"));
    assert!(!raw.to_lowercase().contains("domain="));

    // The CSRF cookie is readable by the page, but still same-origin scoped.
    let csrf = set_cookie(&reply.headers, CSRF_COOKIE_NAME).expect("csrf cookie");
    assert_eq!(csrf.len(), 64);
}

#[test]
fn login_throttle_blocks_repeated_failures() {
    // One console instance: the throttle table is per-listener state, so a
    // fresh router per request would reset it and prove nothing.
    let console = router(Arc::new(ConsoleState::new(
        ConsoleConfig::new(DEFAULT_THEME_NAME),
        SecurityPolicy::authenticated(
            bound(),
            ConsoleSecret::new("correct horse"),
            SessionLimits::default(),
            ThrottleLimits {
                max_failures: 3,
                window_secs: 300,
                max_peers: 8,
            },
        )
        .expect("policy builds"),
    )));

    for _ in 0..3 {
        let request = Request::builder()
            .method("POST")
            .uri("/login")
            .header(header::HOST, HOST)
            .header(header::ORIGIN, format!("http://{HOST}"))
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("password=wrong"))
            .expect("request builds");
        assert_eq!(
            send(console.clone(), request).status,
            StatusCode::UNAUTHORIZED
        );
    }

    // The fourth attempt is blocked before the password is even verified,
    // and it blocks the *correct* password too, which is the documented
    // trade-off of a console-wide throttle.
    let reply = send(console.clone(), post("/login"));
    assert_eq!(reply.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(reply.body.contains("Too many attempts"));
}

/// Signs in on `console` and returns `(session cookie, csrf cookie)`.
///
/// The caller must reuse the same router for the action it is testing: the
/// session table belongs to the console instance, not to a request.
fn sign_in(console: &axum::Router) -> (String, String) {
    let reply = send(console.clone(), post("/login"));
    assert_eq!(reply.status, StatusCode::SEE_OTHER, "sign-in must succeed");
    (
        set_cookie(&reply.headers, SESSION_COOKIE_NAME).expect("session cookie"),
        set_cookie(&reply.headers, CSRF_COOKIE_NAME).expect("csrf cookie"),
    )
}

fn fixture_mark_request(session: Option<&str>, csrf: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/fixture/mark")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, format!("http://{HOST}"));
    let cookies = match (session, csrf) {
        (Some(session), Some(csrf)) => {
            format!("{SESSION_COOKIE_NAME}={session}; {CSRF_COOKIE_NAME}={csrf}")
        }
        (Some(session), None) => format!("{SESSION_COOKIE_NAME}={session}"),
        (None, Some(csrf)) => format!("{CSRF_COOKIE_NAME}={csrf}"),
        (None, None) => String::new(),
    };
    if !cookies.is_empty() {
        builder = builder.header(header::COOKIE, cookies);
    }
    builder.body(Body::empty()).expect("request builds")
}

#[test]
fn a_protected_api_route_refuses_an_unauthenticated_caller() {
    let reply = send(locked_console(), fixture_mark_request(None, None));
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
}

#[test]
fn a_valid_session_plus_csrf_token_permits_the_mutation() {
    let console = locked_console();
    let (session, csrf) = sign_in(&console);
    let reply = send(console, fixture_mark_request(Some(&session), Some(&csrf)));
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply
            .headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/json; charset=utf-8")
    );
    assert!(reply.body.contains("\"marks\":1"));
}

#[test]
fn a_valid_session_without_the_csrf_token_is_refused() {
    let console = locked_console();
    let (session, _csrf) = sign_in(&console);
    // SameSite=Strict is a useful default but not a guarantee; the guard
    // checks the token explicitly.
    let reply = send(console, fixture_mark_request(Some(&session), None));
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
}

#[test]
fn a_wrong_csrf_token_is_refused() {
    let console = locked_console();
    let (session, _csrf) = sign_in(&console);
    let forged = "0".repeat(64);
    let reply = send(console, fixture_mark_request(Some(&session), Some(&forged)));
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
}

#[test]
fn a_forged_session_cookie_is_refused() {
    let console = locked_console();
    let (_session, csrf) = sign_in(&console);
    let forged = "a".repeat(64);
    let reply = send(console, fixture_mark_request(Some(&forged), Some(&csrf)));
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
}

#[test]
fn logout_revokes_the_session_so_it_cannot_be_replayed() {
    let console = locked_console();
    let (session, csrf) = sign_in(&console);

    // Logout carries its CSRF token as a form field, so the script-free
    // form works; the handler still verifies it.
    let logout = Request::builder()
        .method("POST")
        .uri("/logout")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, format!("http://{HOST}"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, format!("{SESSION_COOKIE_NAME}={session}"))
        .body(Body::from(format!("csrf={csrf}")))
        .expect("request builds");
    let reply = send(console.clone(), logout);
    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    let cleared: Vec<String> = reply
        .headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(str::to_string)
        .collect();
    assert!(
        cleared
            .iter()
            .any(|value| value.starts_with(SESSION_COOKIE_NAME) && value.contains("Max-Age=0"))
    );
    assert!(
        cleared
            .iter()
            .any(|value| value.starts_with(CSRF_COOKIE_NAME) && value.contains("Max-Age=0"))
    );

    // The revoked session can no longer act, even with the right token.
    let replay = send(console, fixture_mark_request(Some(&session), Some(&csrf)));
    assert_eq!(
        replay.status,
        StatusCode::FORBIDDEN,
        "a revoked session must not be reusable"
    );
}

#[test]
fn logout_without_a_csrf_token_is_refused() {
    let console = locked_console();
    let (session, _csrf) = sign_in(&console);
    let logout = Request::builder()
        .method("POST")
        .uri("/logout")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, format!("http://{HOST}"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, format!("{SESSION_COOKIE_NAME}={session}"))
        .body(Body::empty())
        .expect("request builds");
    assert_eq!(send(console, logout).status, StatusCode::FORBIDDEN);
}

#[test]
fn logout_with_a_wrong_csrf_token_is_refused_and_keeps_the_session() {
    let console = locked_console();
    let (session, csrf) = sign_in(&console);
    let logout = Request::builder()
        .method("POST")
        .uri("/logout")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, format!("http://{HOST}"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, format!("{SESSION_COOKIE_NAME}={session}"))
        .body(Body::from(format!("csrf={}", "0".repeat(64))))
        .expect("request builds");
    assert_eq!(send(console.clone(), logout).status, StatusCode::FORBIDDEN);
    // A refused logout must not silently sign the operator out.
    let still_valid = send(console, fixture_mark_request(Some(&session), Some(&csrf)));
    assert_eq!(still_valid.status, StatusCode::OK);
}

#[test]
fn an_unauthenticated_console_offers_no_login_form() {
    let reply = send(open_console(), get("/login"));
    assert_eq!(reply.status, StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        reply
            .headers
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some("/")
    );
}

#[test]
fn the_unauthenticated_console_serves_its_api_route_directly() {
    let reply = send(open_console(), fixture_mark_request(None, None));
    assert_eq!(reply.status, StatusCode::OK);
}
