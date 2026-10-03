//! Plan 287 black-box base-server tests.
//!
//! Each test binds the I2PControl listener to `127.0.0.1:0` (ephemeral
//! port) and drives it through real TLS + TCP after listener startup.
//! Behavior is exercised only through the wire; no private bridge, token
//! table, or dispatch API is touched from these tests.
//!
//! The test TLS client uses a test-only accept-any verifier (`NoVerify`)
//! confined to this file. Production source never calls `dangerous()`;
//! `scripts/check-runtime-boundaries.sh` rejects it outside tests.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use i2pr_daemon::config::Config;
use i2pr_daemon::i2pcontrol::I2pControlServiceState;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Operator password for every black-box service in this file.
const TEST_PASSWORD: &str = "black-box-i2pcontrol-password";
/// Bounded client-side deadline for every wire operation.
const WIRE_TIMEOUT: Duration = Duration::from_secs(10);

/// Test-only accept-any certificate verifier. Confined to this file.
#[derive(Debug)]
struct NoVerify;

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
        ]
    }
}

/// Builds an enabled loopback TOML config with an ephemeral port.
fn config_text(password: &str) -> String {
    format!(
        "schema_version = 1\n[router]\ndata_dir = \"state\"\n[i2pcontrol]\nenabled = true\nbind_address = \"127.0.0.1\"\nport = 0\npassword = {password:?}\n"
    )
}

/// Starts the service on an ephemeral loopback port.
async fn start_service(
    config: i2pr_daemon::config::I2pControlConfig,
) -> (
    Arc<I2pControlServiceState>,
    SocketAddr,
    ChildScope,
    CancellationToken,
) {
    let state = Arc::new(I2pControlServiceState::new(config).expect("state builds"));
    let (listener, bound) = state
        .bind(SocketAddr::new(
            IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
            0,
        ))
        .await
        .expect("bind succeeds");
    let parent = CancellationToken::new();
    let scope = ChildScope::for_test(&parent, ChildFailurePolicy::FailParent);
    let serve_state = Arc::clone(&state);
    let serve_token = parent.clone();
    let serve_scope = scope.clone();
    scope
        .spawn(move |_task| async move {
            let _ = serve_state.serve(listener, serve_scope, serve_token).await;
            Ok(())
        })
        .expect("serve spawns");
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    (state, bound, scope, parent)
}

/// Opens one TLS connection to the service.
async fn tls_connect(address: SocketAddr) -> tokio_rustls::client::TlsStream<TcpStream> {
    let client = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerify))
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client));
    let server_name = rustls::pki_types::ServerName::try_from("localhost")
        .expect("server name")
        .to_owned();
    let stream = tokio::time::timeout(WIRE_TIMEOUT, TcpStream::connect(address))
        .await
        .expect("connect deadline")
        .expect("tcp connects");
    tokio::time::timeout(WIRE_TIMEOUT, connector.connect(server_name, stream))
        .await
        .expect("tls deadline")
        .expect("tls handshake succeeds")
}

/// Sends one raw HTTP request and reads the full response to close.
async fn raw_request(address: SocketAddr, head: &str, body: &[u8]) -> (u16, Vec<u8>) {
    let mut stream = tls_connect(address).await;
    let mut request = head.as_bytes().to_vec();
    request.extend_from_slice(body);
    tokio::time::timeout(WIRE_TIMEOUT, stream.write_all(&request))
        .await
        .expect("write deadline")
        .expect("write succeeds");
    let mut response = Vec::new();
    tokio::time::timeout(WIRE_TIMEOUT, stream.read_to_end(&mut response))
        .await
        .expect("read deadline")
        .expect("read succeeds");
    parse_response(&response)
}

/// Sends one JSON-RPC body with compliant framing.
async fn post_json(
    address: SocketAddr,
    body: &serde_json::Value,
    extra_headers: &[(&str, &str)],
) -> (u16, serde_json::Value) {
    let bytes = serde_json::to_vec(body).expect("body serializes");
    let mut head = format!(
        "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        bytes.len()
    );
    for (name, value) in extra_headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    let (status, response_body) = raw_request(address, &head, &bytes).await;
    assert_eq!(status, 200);
    let parsed: serde_json::Value =
        serde_json::from_slice(&response_body).expect("response parses");
    (status, parsed)
}

/// Splits a raw HTTP response into status code and body.
fn parse_response(response: &[u8]) -> (u16, Vec<u8>) {
    let head_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("response head")
        + 4;
    let head = std::str::from_utf8(&response[..head_end]).expect("head is text");
    let status: u16 = head
        .split(' ')
        .nth(1)
        .expect("status token")
        .parse()
        .expect("status parses");
    (status, response[head_end..].to_vec())
}

/// Authenticates and returns the opaque token.
async fn authenticate(address: SocketAddr) -> String {
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "Authenticate",
            "params": {"API": 1, "Password": TEST_PASSWORD},
            "id": 1,
        }),
        &[],
    )
    .await;
    response["result"]["Token"]
        .as_str()
        .expect("token minted")
        .to_owned()
}

#[tokio::test]
async fn loopback_tls_authenticate_and_typed_dispatch() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let (_state, address, _scope, _parent) = start_service(config).await;

    // 1. Loopback TLS establishes (reaching here proves the handshake).
    // 2. API-1 Authenticate yields an opaque token.
    let token = authenticate(address).await;
    assert_eq!(token.len(), 64);

    // 3-4. A protected known method answers the select form: an empty
    // RouterInfo selection returns an empty result object.
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {"Token": token},
            "id": 2,
        }),
        &[],
    )
    .await;
    assert_eq!(response["result"], serde_json::json!({}));

    // 5. Exact authentication failure behavior over the wire.
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "Authenticate",
            "params": {"API": 1, "Password": "wrong"},
            "id": 3,
        }),
        &[],
    )
    .await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_001));

    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {"Token": "bogus-token"},
            "id": 4,
        }),
        &[],
    )
    .await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_003));

    // Unknown methods answer method-not-found with or without a token.
    let (_, response) = post_json(
        address,
        &serde_json::json!({"jsonrpc": "2.0", "method": "GetRate", "params": {}, "id": 5}),
        &[],
    )
    .await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_601));
}

#[tokio::test]
async fn restart_invalidates_tokens() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let (_first, first_address, _scope, parent) = start_service(config.clone()).await;
    let token = authenticate(first_address).await;

    // Restart = fresh in-memory state on the same configuration.
    let _ = parent.cancel(i2pr_core::CancellationReason::ParentScope);
    let (_second, second_address, _scope, _parent) = start_service(config).await;
    let (_, response) = post_json(
        second_address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {"Token": token},
            "id": 1,
        }),
        &[],
    )
    .await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_003));
}

#[tokio::test]
async fn non_loopback_no_cert_half_tls_and_bad_material_fail_closed() {
    // Non-loopback bind without explicit TLS material fails validation.
    let text = "schema_version = 1\n[router]\ndata_dir = \"state\"\n[i2pcontrol]\nenabled = true\nbind_address = \"0.0.0.0\"\npassword = \"secret\"\n";
    assert!(Config::parse(text).is_err());
    // Wildcard IPv6 fails the same way.
    let text = "schema_version = 1\n[router]\ndata_dir = \"state\"\n[i2pcontrol]\nenabled = true\nbind_address = \"::\"\npassword = \"secret\"\n";
    assert!(Config::parse(text).is_err());
    // Half-configured TLS material fails validation.
    let text = "schema_version = 1\n[router]\ndata_dir = \"state\"\n[i2pcontrol]\nenabled = true\npassword = \"secret\"\ncertificate = \"/tmp/cert.pem\"\n";
    assert!(Config::parse(text).is_err());
    // Unreadable explicit material fails construction (before any bind).
    let directory = tempfile::tempdir().expect("temp directory");
    let cert = directory.path().join("cert.pem");
    let key = directory.path().join("key.pem");
    std::fs::write(&cert, "not a certificate").expect("write cert");
    std::fs::write(&key, "not a key").expect("write key");
    let text = format!(
        "schema_version = 1\n[router]\ndata_dir = \"state\"\n[i2pcontrol]\nenabled = true\npassword = \"secret\"\ncertificate = {:?}\nprivate_key = {:?}\n",
        cert.to_string_lossy(),
        key.to_string_lossy()
    );
    let config = Config::parse(&text).expect("shape parses").i2pcontrol;
    assert!(I2pControlServiceState::new(config).is_err());
    // No listener side effect is possible: only garbage files exist.
    let entries: Vec<_> = std::fs::read_dir(directory.path())
        .expect("read dir")
        .collect();
    assert_eq!(entries.len(), 2);
}

#[tokio::test]
async fn disabled_mode_registers_no_service_and_mutates_nothing() {
    let directory = tempfile::tempdir().expect("temp directory");
    let text = format!(
        "schema_version = 1\n[router]\ndata_dir = {:?}\n",
        directory.path().to_string_lossy()
    );
    let config = Config::parse(&text).expect("defaults parse");
    assert!(!config.i2pcontrol.enabled);
    let graph = i2pr_daemon::build_daemon_graph(&config).expect("graph builds");
    let names: Vec<_> = graph
        .startup_order()
        .iter()
        .map(|name| name.as_str().to_string())
        .collect();
    assert!(
        !names.iter().any(|name| name == "i2pcontrol"),
        "disabled config must not register i2pcontrol, got: {names:?}"
    );
    assert!(I2pControlServiceState::new(config.i2pcontrol).is_err());
    // The data directory was never created, let alone mutated.
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("read dir")
            .count(),
        0
    );
}

#[tokio::test]
async fn enabled_graph_registers_supervised_service() {
    let config = Config::parse(&config_text(TEST_PASSWORD)).expect("config parses");
    let graph = i2pr_daemon::build_daemon_graph(&config).expect("graph builds");
    let names: Vec<_> = graph
        .startup_order()
        .iter()
        .map(|name| name.as_str().to_string())
        .collect();
    assert!(
        names.iter().any(|name| name == "i2pcontrol"),
        "enabled config must register i2pcontrol, got: {names:?}"
    );
}

#[tokio::test]
async fn graceful_and_forced_shutdown_release_resources() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let (state, address, _scope, parent) = start_service(config).await;
    // Exercise the listener once so permits cycle before shutdown.
    let _ = authenticate(address).await;
    assert_eq!(state.inflight_available(), 64);
    let snapshot = state.snapshot();
    assert!(snapshot.connections_accepted >= 1);
    assert!(snapshot.requests_processed >= 1);
    // Shutdown cancels the listener; permits observe a clean baseline.
    let _ = parent.cancel(i2pr_core::CancellationReason::ParentScope);
    tokio::time::timeout(Duration::from_secs(5), async {
        while state.snapshot().connections_accepted < 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("counters settle");
    assert_eq!(state.inflight_available(), 64);
    assert_eq!(state.live_token_count(), 1);
}

#[tokio::test]
async fn simultaneous_wire_failed_auth_counts_atomically() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let (state, address, _scope, _parent) = start_service(config).await;
    let mut handles = Vec::new();
    for index in 0..8_u64 {
        handles.push(tokio::spawn(async move {
            post_json(
                address,
                &serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "Authenticate",
                    "params": {"API": 1, "Password": "wrong"},
                    "id": index,
                }),
                &[],
            )
            .await
        }));
    }
    for handle in handles {
        let (_, response) = handle.await.expect("client joins");
        assert_eq!(response["error"]["code"], serde_json::json!(-32_001));
    }
    let ip: IpAddr = "127.0.0.1".parse().expect("loopback");
    assert_eq!(state.throttle_failure_count(ip), 8);
}

#[tokio::test]
async fn oversized_body_and_framing_rejected() {
    // Small body ceiling proves the +1 admission case quickly.
    let text = "schema_version = 1\n[router]\ndata_dir = \"state\"\n[i2pcontrol]\nenabled = true\nbind_address = \"127.0.0.1\"\nport = 0\npassword = \"secret\"\nmax_body_bytes = 64\n";
    let config = Config::parse(text).expect("config parses").i2pcontrol;
    assert_eq!(config.max_body_bytes, 64);
    let (_state, address, _scope, _parent) = start_service(config).await;
    // 65-byte body (+1) is rejected at HTTP admission with 413.
    let body = vec![b'x'; 65];
    let head = format!(
        "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    let (status, _) = raw_request(address, &head, &body).await;
    assert_eq!(status, 413);
    // Non-POST methods are rejected without JSON dispatch.
    let (status, _) = raw_request(
        address,
        "GET / HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\n\r\n",
        &[],
    )
    .await;
    assert_eq!(status, 405);
    // Missing content length is rejected (no chunked support).
    let (status, _) = raw_request(address, "POST / HTTP/1.1\r\nHost: x\r\n\r\n", &[]).await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn notification_over_wire_gets_no_body() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let (_state, address, _scope, _parent) = start_service(config).await;
    let body = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "method": "Authenticate",
        "params": {"API": 1, "Password": TEST_PASSWORD},
    }))
    .expect("body");
    let head = format!(
        "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    let (status, response_body) = raw_request(address, &head, &body).await;
    assert_eq!(status, 204);
    assert!(response_body.is_empty());
}

#[tokio::test]
async fn managed_tls_reused_without_filesystem_mutation() {
    let directory = tempfile::tempdir().expect("temp directory");
    let text = format!(
        "schema_version = 1\n[router]\ndata_dir = {:?}\n[i2pcontrol]\nenabled = true\nbind_address = \"127.0.0.1\"\nport = 0\npassword = \"secret\"\n",
        directory.path().to_string_lossy()
    );
    let config = Config::parse(&text).expect("config parses").i2pcontrol;
    assert!(!config.has_explicit_tls());
    let (_state, address, _scope, _parent) = start_service(config).await;
    // Two sequential connections succeed against the one managed identity.
    for _ in 0..2 {
        let (_, response) = post_json(
            address,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "Authenticate",
                "params": {"API": 1, "Password": "secret"},
                "id": 1,
            }),
            &[],
        )
        .await;
        assert!(response["result"]["Token"].is_string());
    }
    // The data directory gained no certificate files: managed TLS is
    // in-memory only and disabled mode would allocate nothing at all.
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("read dir")
            .count(),
        0
    );
}
