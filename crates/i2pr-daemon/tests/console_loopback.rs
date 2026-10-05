//! Real-socket proof that the EggServe-to-Axum console adapter works.
//!
//! This test exists so an EggServe or Axum upgrade fails at the actual
//! adapter boundary rather than at a type check. It binds `127.0.0.1:0`,
//! drives HTTP/1 over a real TCP socket, and asserts on the bytes that come
//! back. Nothing here is a mock.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use i2pr_daemon::config::ConsoleConfig;
use i2pr_daemon::console::ConsoleServiceState;
use i2pr_runtime::CancellationToken;

/// Inspection handles for the console's read-only control client.
fn inspection() -> Arc<i2pr_daemon::InspectionHandles> {
    Arc::new(i2pr_daemon::InspectionHandles::from_config(
        &i2pr_daemon::config::Config::parse(
            "schema_version = 1\n[router]\ndata_dir = \"./state\"\n",
        )
        .expect("fixture config parses"),
    ))
}

/// Issues one minimal HTTP/1 GET over loopback and returns the raw response.
///
/// The `Host` header names the listener's real port, because the console
/// validates the effective authority before routing and refuses anything
/// else. A probe that sent a bare `localhost` would be testing the rebinding
/// defence rather than the page.
async fn probe_loopback_get(address: std::net::SocketAddr, path: &str) -> std::io::Result<String> {
    let mut stream = tokio::net::TcpStream::connect(address).await?;
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\n\r\n",
        port = address.port()
    );
    tokio::io::AsyncWriteExt::write_all(&mut stream, request.as_bytes()).await?;
    let mut response = String::new();
    tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut response).await?;
    Ok(response)
}

fn config() -> ConsoleConfig {
    ConsoleConfig {
        enabled: true,
        bind_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        theme: i2pr_console::DEFAULT_THEME_NAME.to_string(),
        max_connections: 8,
        auth: false,
        password_hash: None,
        max_sessions: 8,
        session_idle_secs: 900,
        session_absolute_secs: 28_800,
        login_max_failures: 5,
        login_window_secs: 300,
    }
}

/// Same listener, with the browser password enabled.
fn authenticated_config() -> ConsoleConfig {
    let mut config = config();
    config.auth = true;
    let derived = i2pr_console::security::auth::derive_from_password(
        i2pr_console::ConsoleSecret::new("correct horse"),
    )
    .expect("verifier derives");
    config.password_hash = Some(i2pr_daemon::config::ConsolePasswordHash::new(
        derived.verifier().hash_for_audit().to_string(),
    ));
    config
}

/// Starts the console on an ephemeral loopback port and returns its address.
async fn start_console() -> (std::net::SocketAddr, CancellationToken) {
    let state = ConsoleServiceState::new(config(), inspection()).expect("console state builds");
    let (listener, address) = state
        .bind("127.0.0.1:0".parse().unwrap())
        .await
        .expect("loopback bind succeeds");
    let cancellation = CancellationToken::new();
    let token = cancellation.clone();
    let task_state = Arc::clone(&state);
    tokio::spawn(async move {
        let _ = task_state.serve(listener, address, token).await;
    });
    // The listener is already bound before `serve` is polled, so the
    // address is immediately connectable; give the accept loop one poll.
    tokio::task::yield_now().await;
    (address, cancellation)
}

#[tokio::test]
async fn console_serves_the_shell_over_a_real_socket() {
    let (address, cancellation) = start_console().await;

    let response = probe_loopback_get(address, "/")
        .await
        .expect("loopback GET succeeds");

    assert!(
        response.starts_with("HTTP/1.1 200"),
        "expected 200 response head, got: {}",
        &response[..response.len().min(120)]
    );
    assert!(
        response.contains("text/html"),
        "shell must be served as HTML: {response}"
    );
    assert!(
        response.contains("data-theme=\"i2pr-default\""),
        "shell must carry the configured theme"
    );

    cancellation.cancel(i2pr_core::CancellationReason::TestHarnessTeardown);
}

#[tokio::test]
async fn console_serves_compiled_assets_over_a_real_socket() {
    let (address, cancellation) = start_console().await;

    let css = probe_loopback_get(address, "/assets/console.css")
        .await
        .expect("asset GET succeeds");
    assert!(css.starts_with("HTTP/1.1 200"));
    assert!(css.contains("text/css"));
    assert!(
        css.contains("--console-"),
        "stylesheet must reach the client"
    );

    let theme = probe_loopback_get(address, "/theme.css")
        .await
        .expect("theme GET succeeds");
    assert!(theme.starts_with("HTTP/1.1 200"));
    assert!(
        theme.contains(":root{"),
        "generated theme must reach the client"
    );

    cancellation.cancel(i2pr_core::CancellationReason::TestHarnessTeardown);
}

#[tokio::test]
async fn unknown_paths_and_traversal_attempts_are_refused_over_http() {
    let (address, cancellation) = start_console().await;

    for path in ["/nope", "/assets/unknown.css", "/assets/../console.css"] {
        let response = probe_loopback_get(address, path)
            .await
            .expect("loopback GET succeeds");
        assert!(
            response.starts_with("HTTP/1.1 404"),
            "{path} must be refused, got: {}",
            &response[..response.len().min(120)]
        );
    }

    cancellation.cancel(i2pr_core::CancellationReason::TestHarnessTeardown);
}

#[tokio::test]
async fn listener_shuts_down_and_stops_accepting() {
    let state = ConsoleServiceState::new(config(), inspection()).expect("console state builds");
    let (listener, address) = state
        .bind("127.0.0.1:0".parse().unwrap())
        .await
        .expect("loopback bind succeeds");
    assert!(state.bound_address().is_some());

    let cancellation = CancellationToken::new();
    let token = cancellation.clone();
    let serve_state = Arc::clone(&state);
    let server = tokio::spawn(async move { serve_state.serve(listener, address, token).await });

    tokio::task::yield_now().await;
    assert!(state.is_serving(), "service reports serving after start");

    cancellation.cancel(i2pr_core::CancellationReason::TestHarnessTeardown);
    let outcome = tokio::time::timeout(Duration::from_secs(10), server)
        .await
        .expect("shutdown completes within the bounded timeout")
        .expect("serve task joins");
    assert!(outcome.is_ok(), "serve returned {outcome:?}");
    assert!(
        !state.is_serving(),
        "service reports stopped after shutdown"
    );
}
#[tokio::test]
async fn authenticated_console_serves_a_live_authority_policy() {
    let state = ConsoleServiceState::new(authenticated_config(), inspection())
        .expect("console state builds");
    let (listener, address) = state
        .bind("127.0.0.1:0".parse().unwrap())
        .await
        .expect("loopback bind succeeds");
    let cancellation = CancellationToken::new();
    let token = cancellation.clone();
    let serve_state = Arc::clone(&state);
    tokio::spawn(async move {
        let _ = serve_state.serve(listener, address, token).await;
    });
    tokio::task::yield_now().await;

    // The shell redirects to the login page when unauthenticated.
    let response = probe_loopback_get(address, "/")
        .await
        .expect("loopback GET succeeds");
    assert!(
        response.starts_with("HTTP/1.1 307") || response.starts_with("HTTP/1.1 303"),
        "expected a redirect to login, got: {}",
        &response[..response.len().min(120)]
    );

    // A foreign authority is refused at the front door even on loopback.
    let mut stream = tokio::net::TcpStream::connect(address)
        .await
        .expect("connect");
    tokio::io::AsyncWriteExt::write_all(
        &mut stream,
        b"GET / HTTP/1.1\r\nHost: evil.test\r\nConnection: close\r\n\r\n",
    )
    .await
    .expect("write");
    let mut refused = String::new();
    tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut refused)
        .await
        .expect("read");
    assert!(
        refused.starts_with("HTTP/1.1 403"),
        "foreign authority must be refused, got: {}",
        &refused[..refused.len().min(120)]
    );

    // Every response carries the centralized hardening headers. HTTP/1.1
    // header names arrive lowercase on the wire, so compare case-insensitively.
    let lower = response.to_ascii_lowercase();
    assert!(
        lower.contains("content-security-policy:"),
        "the redirect must carry a CSP over the real socket"
    );
    assert!(lower.contains("x-content-type-options: nosniff"));
    // And the policy itself must forbid inline and remote code.
    assert!(lower.contains("default-src 'none'"));
    assert!(!lower.contains("unsafe-inline"));

    cancellation.cancel(i2pr_core::CancellationReason::TestHarnessTeardown);
}

#[tokio::test]
async fn console_serves_real_control_data_to_the_browser() {
    let state = ConsoleServiceState::new(config(), inspection()).expect("console state builds");
    let (listener, address) = state
        .bind("127.0.0.1:0".parse().unwrap())
        .await
        .expect("loopback bind succeeds");
    let cancellation = CancellationToken::new();
    let token = cancellation.clone();
    let serve_state = Arc::clone(&state);
    tokio::spawn(async move {
        let _ = serve_state.serve(listener, address, token).await;
    });
    tokio::task::yield_now().await;

    // The read-only overview endpoint answers with canonical control data.
    let api = probe_loopback_get(address, "/api/overview")
        .await
        .expect("loopback GET succeeds");
    assert!(
        api.starts_with("HTTP/1.1 200"),
        "overview must answer 200, got: {}",
        &api[..api.len().min(160)]
    );
    assert!(api.contains("application/json"));
    // The router version is static truth, so it must appear verbatim.
    assert!(
        api.contains("\"label\":\"Version\",\"value\":\"0.1.0\""),
        "overview must carry the canonical version value: {api}"
    );
    assert!(
        api.contains("\"routerInfo\":\"returned\""),
        "the RouterInfo answer must be reported as returned: {api}"
    );
    // Selectors whose source this router does not publish must be marked
    // unavailable rather than reported as zero.
    assert!(
        api.contains("\"availability\":\"unavailable\""),
        "unpublished selectors must be marked unavailable: {api}"
    );
    // A truthful zero (a freshly started router's uptime) is allowed; a
    // metric with no source must be null, never zero. The assertion is
    // scoped to the `metrics` array on purpose: service rows carry
    // `state`, not `value`, and an unavailable service row is equally
    // required to render `null`.
    let (metrics_section, services_section) = api
        .split_once("\"services\":")
        .expect("overview separates metrics from services");
    for row in metrics_section
        .split("\"availability\":\"unavailable\"")
        .skip(1)
    {
        assert!(
            row.contains("\"value\":null"),
            "an unavailable metric must be null: {row}"
        );
    }
    for row in services_section
        .split("\"availability\":\"unavailable\"")
        .skip(1)
    {
        assert!(
            row.contains("\"state\":null"),
            "an unavailable service row must be null: {row}"
        );
    }
    // Every canonical client service is reported.
    for service in ["I2PTunnel", "HTTPProxy", "SOCKS", "SAM", "BOB", "I2CP"] {
        assert!(
            api.contains(&format!("\"{service}\"")),
            "{service} must appear in the overview: {api}"
        );
    }

    // The server-rendered shell carries the same point-in-time snapshot.
    let shell = probe_loopback_get(address, "/")
        .await
        .expect("loopback GET succeeds");
    assert!(shell.contains("data-console-refresh=\"overview\""));
    assert!(shell.contains("data-availability=\"returned\""));
    assert!(shell.contains("data-availability=\"unavailable\""));

    cancellation.cancel(i2pr_core::CancellationReason::TestHarnessTeardown);
}
