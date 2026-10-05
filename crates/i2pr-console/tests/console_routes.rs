//! Black-box route behavior for the console.
//!
//! Every assertion drives the assembled router through the tower service
//! interface, so these tests exercise routing, extraction, and response
//! construction exactly as the server adapter does, without binding a
//! socket. Socket ownership and the EggServe adapter are covered by the
//! daemon-side consumer test.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures_executor::block_on;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tower::util::ServiceExt;

use i2pr_console::{
    ConsoleConfig, ConsoleState, SecurityPolicy, SessionLimits, ThrottleLimits, router,
};

/// The authority the test policies were built for.
const HOST: &str = "localhost:7070";

fn bound() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7070)
}

fn open_policy() -> SecurityPolicy {
    SecurityPolicy::unauthenticated(bound(), SessionLimits::default(), ThrottleLimits::default())
        .expect("unauthenticated policy builds")
}

fn service() -> axum::Router {
    let config = ConsoleConfig::new(i2pr_console::DEFAULT_THEME_NAME);
    router(Arc::new(ConsoleState::new(config, open_policy())))
}

async fn get(router: axum::Router, uri: &str) -> (StatusCode, String, String) {
    let response = router
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(header::HOST, HOST)
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body collects");
    (
        status,
        content_type,
        String::from_utf8(bytes.to_vec()).expect("body is utf-8"),
    )
}

#[test]
fn shell_renders_as_html_and_names_its_theme() {
    block_on(async {
        let (status, content_type, body) = get(service(), "/").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(content_type, "text/html; charset=utf-8");
        assert!(body.starts_with("<!DOCTYPE html>"));
        assert!(body.contains("data-theme=\"i2pr-default\""));
        assert!(body.contains("<main id=\"main\""));
        // The shell links its assets by absolute path.
        assert!(body.contains("href=\"/assets/console.css\""));
        assert!(body.contains("href=\"/theme.css\""));
        assert!(body.contains("src=\"/assets/console.js\""));
    });
}

#[test]
fn shell_is_csp_ready_and_carries_no_inline_code() {
    block_on(async {
        let (_, _, body) = get(service(), "/").await;
        // A strict `script-src 'self'; style-src 'self'` policy forbids inline
        // code; the shell must therefore contain none.
        assert!(!body.contains("<style"));
        assert!(!body.contains("style="), "inline style attribute found");
        assert!(!body.contains("<script>"));
        assert!(!body.contains("onclick"));
        assert!(!body.contains("onload"));
        // It must not pull a remote origin.
        assert!(!body.contains("http://"));
        assert!(!body.contains("https://"));
    });
}

#[test]
fn theme_stylesheet_is_pure_custom_properties() {
    block_on(async {
        let (status, content_type, body) = get(service(), "/theme.css").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(content_type, "text/css; charset=utf-8");

        let rule = body
            .lines()
            .nth(1)
            .expect("stylesheet carries a :root rule");
        assert!(rule.starts_with(":root{"));
        assert!(rule.ends_with('}'));

        let declarations: Vec<&str> = rule
            .trim_start_matches(":root{")
            .trim_end_matches('}')
            .split(';')
            .filter(|part| !part.is_empty())
            .collect();
        assert!(
            !declarations.is_empty(),
            "theme must declare custom properties"
        );
        for declaration in declarations {
            let (name, value) = declaration
                .split_once(':')
                .unwrap_or_else(|| panic!("declaration is not name:value: {declaration}"));
            assert!(
                name.starts_with("--console-"),
                "unexpected property {name} in generated theme css"
            );
            assert_eq!(value.len(), 7, "value must be #rrggbb: {value}");
            assert!(value.starts_with('#'));
            assert!(value[1..].bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
    });
}

#[test]
fn compiled_assets_are_served_with_declared_types() {
    block_on(async {
        let cases = [
            ("/assets/console.css", "text/css; charset=utf-8"),
            ("/assets/console.js", "text/javascript; charset=utf-8"),
            ("/assets/logo.svg", "image/svg+xml"),
        ];
        for (path, expected) in cases {
            let (status, content_type, body) = get(service(), path).await;
            assert_eq!(status, StatusCode::OK, "{path} should be served");
            assert_eq!(content_type, expected, "{path} content type");
            assert!(!body.is_empty(), "{path} must have a body");
        }
    });
}

#[test]
fn asset_lookup_rejects_traversal_and_unknown_names() {
    block_on(async {
        for hostile in [
            "/assets/unknown.css",
            "/assets/CONSOLE.CSS",
            "/assets/../console.css",
            "/assets/%2e%2e%2fconsole.css",
            "/assets/subdir/console.css",
            "/assets/console.css/",
        ] {
            let (status, _, body) = get(service(), hostile).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{hostile} must not resolve");
            // The miss body must not echo what was requested.
            assert_eq!(body, "not found\n");
        }
    });
}

#[test]
fn unknown_routes_return_a_fixed_body() {
    block_on(async {
        let (status, content_type, body) = get(service(), "/does-not-exist").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(content_type, "text/plain; charset=utf-8");
        assert_eq!(body, "not found\n");
        assert!(!body.contains("does-not-exist"));
    });
}

#[test]
fn every_route_declares_no_store_caching() {
    block_on(async {
        for path in ["/", "/theme.css", "/assets/console.css", "/nope"] {
            let response = service()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .body(Body::empty())
                        .expect("request builds"),
                )
                .await
                .expect("router responds");
            let cache = response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
                .unwrap_or_default();
            assert_eq!(cache, "no-store", "{path} must not be cached");
        }
    });
}

#[test]
fn malformed_theme_configuration_falls_back_to_the_default_palette() {
    block_on(async {
        let config = ConsoleConfig::new("this-theme-does-not-exist");
        let router = router(Arc::new(ConsoleState::new(config, open_policy())));
        let (status, _, body) = get(router, "/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            body.contains("data-theme=\"i2pr-default\""),
            "an unusable configured theme must degrade to the default"
        );
        assert!(body.contains("data-availability=\"unavailable\""));
    });
}

#[test]
fn rendered_text_is_escaped() {
    block_on(async {
        // A title is operator-supplied and reaches the document body; it must
        // not be able to introduce markup.
        let rendered = i2pr_console::routes::render_shell(
            &i2pr_console::ThemePalette::resolve_or_default(i2pr_console::DEFAULT_THEME_NAME),
            "evil\"><script>alert(1)</script>",
            None,
            &i2pr_console::Overview::build(
                &i2pr_console::ControlReply::unavailable("none"),
                &i2pr_console::ControlReply::unavailable("none"),
            ),
        );
        assert!(!rendered.contains("<script>"));
        assert!(rendered.contains("&lt;script&gt;"));
    });
}
