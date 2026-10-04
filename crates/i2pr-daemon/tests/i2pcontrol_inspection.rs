//! Plan 288 black-box inspection-plane tests.
//!
//! Each test binds the I2PControl listener to `127.0.0.1:0` (ephemeral
//! port) and drives it through real TLS + TCP after listener startup.
//! Behavior is exercised only through the wire plus the public
//! inspection-handle publication API; no token table or dispatch
//! internals are touched from these tests.
//!
//! The test TLS client uses a test-only accept-any verifier (`NoVerify`)
//! confined to this file. Production source never calls `dangerous()`;
//! `scripts/check-runtime-boundaries.sh` rejects it outside tests.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use i2pr_daemon::config::Config;
use i2pr_daemon::i2pcontrol::I2pControlServiceState;
use i2pr_daemon::i2pcontrol_inspection::InspectionHandles;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Operator password for every black-box service in this file.
const TEST_PASSWORD: &str = "black-box-288-inspection-password";
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

/// Starts the service on an ephemeral loopback port with explicit
/// inspection handles.
async fn start_service_with_inspection(
    config: i2pr_daemon::config::I2pControlConfig,
    inspection: Arc<InspectionHandles>,
) -> (
    Arc<I2pControlServiceState>,
    SocketAddr,
    ChildScope,
    CancellationToken,
) {
    let state = Arc::new(
        I2pControlServiceState::new_with_inspection(config, inspection).expect("state builds"),
    );
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
async fn router_info_proposal_selection_over_wire() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let (_state, address, _scope, _parent) = start_service_with_inspection(
        config,
        Arc::new(InspectionHandles::new(
            2,
            i2pr_daemon::i2pcontrol_inspection::ServiceEndpoint {
                enabled: false,
                bind: None,
            },
            i2pr_daemon::i2pcontrol_inspection::ServiceEndpoint {
                enabled: false,
                bind: None,
            },
            Vec::new(),
        )),
    )
    .await;
    let token = authenticate(address).await;
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {
                "Token": token.clone(),
                "i2p.router.clockskew": "ignored",
            },
            "id": 2,
        }),
        &[],
    )
    .await;
    let result = &response["result"];
    assert_eq!(result["i2p.router.clockskew"], serde_json::json!(0));
    // Deterministic sorted key order regardless of request order
    // (`serde_json::Map` sorts keys; selection order is canonical).
    let keys: Vec<&str> = result
        .as_object()
        .expect("result object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, vec!["i2p.router.clockskew",]);

    let (_, base_response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {
                "Token": token,
                "i2p.router.version": null,
                "i2p.router.status": "select by presence",
                "i2p.router.uptime": "select by presence",
            },
            "id": 3,
        }),
        &[],
    )
    .await;
    assert_eq!(
        base_response["result"]["i2p.router.version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(base_response["result"]["i2p.router.status"], "running");
    assert!(base_response["result"]["i2p.router.uptime"].is_u64());
}

#[tokio::test]
async fn router_info_hash_gated_then_published_over_wire() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let inspection = Arc::new(InspectionHandles::new(
        2,
        i2pr_daemon::i2pcontrol_inspection::ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        i2pr_daemon::i2pcontrol_inspection::ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        Vec::new(),
    ));
    let (state, address, _scope, _parent) =
        start_service_with_inspection(config, Arc::clone(&inspection)).await;
    let token = authenticate(address).await;
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "RouterInfo",
        "params": {"Token": token, "i2p.router.id": null},
        "id": 2,
    });
    // Before identity publication the whole request fails explicitly.
    let (_, response) = post_json(address, &body, &[]).await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_603));
    assert!(
        response["error"]["message"]
            .as_str()
            .expect("message")
            .contains("Plan 288"),
        "message: {}",
        response["error"]["message"]
    );
    assert!(response.get("result").is_none(), "no partial response");
    // After publication the same request answers without a restart.
    let hash = format!("{}~", "b".repeat(43));
    state
        .inspection()
        .publish_router_hash(&hash)
        .expect("publishes");
    let (_, response) = post_json(address, &body, &[]).await;
    assert_eq!(response["result"]["i2p.router.id"], serde_json::json!(hash));
}

#[tokio::test]
async fn router_info_unavailable_fails_whole_without_partial() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let (_state, address, _scope, _parent) = start_service_with_inspection(
        config,
        Arc::new(InspectionHandles::new(
            2,
            i2pr_daemon::i2pcontrol_inspection::ServiceEndpoint {
                enabled: false,
                bind: None,
            },
            i2pr_daemon::i2pcontrol_inspection::ServiceEndpoint {
                enabled: false,
                bind: None,
            },
            Vec::new(),
        )),
    )
    .await;
    let token = authenticate(address).await;
    // One unavailable selector poisons the whole request even beside
    // available ones: no partial response is emitted.
    for key in ["i2p.router.info", "i2p.router.logs.clear"] {
        let (_, response) = post_json(
            address,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "RouterInfo",
                "params": {"Token": token, "i2p.router.clockskew": null, key: null},
                "id": 2,
            }),
            &[],
        )
        .await;
        assert_eq!(
            response["error"]["code"],
            serde_json::json!(-32_603),
            "key {key}"
        );
        assert!(response.get("result").is_none(), "no partial for {key}");
    }
}

#[tokio::test]
async fn router_info_unknown_and_base_keys_rejected() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let (_state, address, _scope, _parent) = start_service_with_inspection(
        config,
        Arc::new(InspectionHandles::new(
            2,
            i2pr_daemon::i2pcontrol_inspection::ServiceEndpoint {
                enabled: false,
                bind: None,
            },
            i2pr_daemon::i2pcontrol_inspection::ServiceEndpoint {
                enabled: false,
                bind: None,
            },
            Vec::new(),
        )),
    )
    .await;
    let token = authenticate(address).await;
    // Unknown keys, former normalized names, and case mismatches are
    // invalid params. Canonical selector values are presence-based.
    for key in [
        "bogus",
        "router.uptime",
        "router.version",
        "router.status",
        "Router.Version",
    ] {
        let (_, response) = post_json(
            address,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "RouterInfo",
                "params": {"Token": token, key: null},
                "id": 2,
            }),
            &[],
        )
        .await;
        assert_eq!(
            response["error"]["code"],
            serde_json::json!(-32_602),
            "key {key}"
        );
    }
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {"Token": token, "i2p.router.clockskew": true},
            "id": 3,
        }),
        &[],
    )
    .await;
    assert_eq!(
        response["result"]["i2p.router.clockskew"],
        serde_json::json!(0)
    );
}

#[tokio::test]
async fn router_info_notification_gets_no_body() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let (_state, address, _scope, _parent) = start_service_with_inspection(
        config,
        Arc::new(InspectionHandles::new(
            2,
            i2pr_daemon::i2pcontrol_inspection::ServiceEndpoint {
                enabled: false,
                bind: None,
            },
            i2pr_daemon::i2pcontrol_inspection::ServiceEndpoint {
                enabled: false,
                bind: None,
            },
            Vec::new(),
        )),
    )
    .await;
    let token = authenticate(address).await;
    // Notifications execute without a response body.
    let body = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "method": "RouterInfo",
        "params": {"Token": token, "router.version": null},
    }))
    .expect("body serializes");
    let head = format!(
        "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    let (status, response_body) = raw_request(address, &head, &body).await;
    assert_eq!(status, 204);
    assert!(response_body.is_empty());
}

#[tokio::test]
async fn client_services_all_six_over_wire() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let (_state, address, _scope, _parent) = start_service_with_inspection(
        config,
        Arc::new(InspectionHandles::new(
            2,
            i2pr_daemon::i2pcontrol_inspection::ServiceEndpoint {
                enabled: false,
                bind: None,
            },
            i2pr_daemon::i2pcontrol_inspection::ServiceEndpoint {
                enabled: false,
                bind: None,
            },
            Vec::new(),
        )),
    )
    .await;
    let token = authenticate(address).await;
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "ClientServicesInfo",
            "params": {
                "Token": token,
                "I2PTunnel": null,
                "HTTPProxy": null,
                "SOCKS": null,
                "SAM": null,
                "BOB": null,
                "I2CP": null,
            },
            "id": 2,
        }),
        &[],
    )
    .await;
    let result = &response["result"];
    assert_eq!(result["I2PTunnel"]["client"], serde_json::json!({}));
    assert_eq!(result["I2PTunnel"]["server"], serde_json::json!({}));
    assert_eq!(result["HTTPProxy"]["enabled"], serde_json::json!(false));
    assert_eq!(result["SOCKS"]["enabled"], serde_json::json!(false));
    assert_eq!(result["SAM"]["enabled"], serde_json::json!(false));
    assert_eq!(result["SAM"]["session_count"], serde_json::json!(0));
    assert_eq!(result["SAM"]["sessions"], serde_json::json!([]));
    // BOB is a deliberate constant, not a missing handler.
    assert_eq!(result["BOB"], serde_json::json!({"enabled": false}));
    assert_eq!(result["I2CP"]["enabled"], serde_json::json!(false));
    assert_eq!(result["I2CP"]["session_count"], serde_json::json!(0));
}

#[tokio::test]
async fn startup_inventory_visible_over_wire() {
    let b32 = format!("{}.b32.i2p", "a".repeat(52));
    let text = format!(
        "schema_version = 1\n[router]\ndata_dir = \"state\"\n[i2pcontrol]\nenabled = true\nbind_address = \"127.0.0.1\"\nport = 0\npassword = {TEST_PASSWORD:?}\n[service_tunnels]\nenabled = false\n[[service_tunnels.tunnel]]\nid = \"web-client\"\nkind = \"http-client\"\nenabled = true\nlistener = \"127.0.0.1:8180\"\ndestination = \"{b32}\"\n"
    );
    let config = Config::parse(&text).expect("config parses");
    let inspection = Arc::new(InspectionHandles::from_config(&config));
    let (_state, address, _scope, _parent) =
        start_service_with_inspection(config.i2pcontrol, Arc::clone(&inspection)).await;
    let token = authenticate(address).await;
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "ClientServicesInfo",
            "params": {"Token": token, "I2PTunnel": null, "HTTPProxy": null},
            "id": 2,
        }),
        &[],
    )
    .await;
    let result = &response["result"];
    assert_eq!(
        result["I2PTunnel"]["client"]["web-client"]["kind"],
        serde_json::json!("http-client")
    );
    assert_eq!(
        result["I2PTunnel"]["client"]["web-client"]["enabled"],
        serde_json::json!(true)
    );
    assert_eq!(
        result["I2PTunnel"]["client"]["web-client"]["running"],
        serde_json::json!(false)
    );
    assert_eq!(
        result["I2PTunnel"]["client"]["web-client"]["bind"],
        serde_json::json!("127.0.0.1:8180")
    );
    assert_eq!(result["HTTPProxy"]["enabled"], serde_json::json!(true));
    assert_eq!(
        result["HTTPProxy"]["bind"],
        serde_json::json!("127.0.0.1:8180")
    );
}

#[tokio::test]
async fn inspection_reads_do_not_mutate() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let (_state, address, _scope, _parent) = start_service_with_inspection(
        config,
        Arc::new(InspectionHandles::new(
            2,
            i2pr_daemon::i2pcontrol_inspection::ServiceEndpoint {
                enabled: false,
                bind: None,
            },
            i2pr_daemon::i2pcontrol_inspection::ServiceEndpoint {
                enabled: false,
                bind: None,
            },
            Vec::new(),
        )),
    )
    .await;
    let token = authenticate(address).await;
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "RouterInfo",
        "params": {"Token": token, "i2p.router.clockskew": null},
        "id": 2,
    });
    let (_, first) = post_json(address, &body, &[]).await;
    let (_, second) = post_json(address, &body, &[]).await;
    assert_eq!(first["result"], second["result"]);
}

#[tokio::test]
async fn batch_isolation_with_inspection() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let (_state, address, _scope, _parent) = start_service_with_inspection(
        config,
        Arc::new(InspectionHandles::new(
            2,
            i2pr_daemon::i2pcontrol_inspection::ServiceEndpoint {
                enabled: false,
                bind: None,
            },
            i2pr_daemon::i2pcontrol_inspection::ServiceEndpoint {
                enabled: false,
                bind: None,
            },
            Vec::new(),
        )),
    )
    .await;
    let token = authenticate(address).await;
    let (_, response) = post_json(
        address,
        &serde_json::json!([
            {"jsonrpc": "2.0", "method": "RouterInfo", "params": {"Token": token, "i2p.router.clockskew": null}, "id": "a"},
            {"jsonrpc": "2.0", "method": "RouterInfo", "params": {"Token": token, "i2p.router.info": null}, "id": "b"},
        ]),
        &[],
    )
    .await;
    let elements = response.as_array().expect("batch array");
    assert_eq!(elements.len(), 2);
    assert_eq!(
        elements[0]["result"]["i2p.router.clockskew"],
        serde_json::json!(0)
    );
    assert_eq!(elements[1]["error"]["code"], serde_json::json!(-32_603));
    assert!(elements[1].get("result").is_none(), "no partial in batch");
}
