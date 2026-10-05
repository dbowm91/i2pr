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
use i2pr_daemon::control_sources::ControlMetrics;
use i2pr_daemon::i2pcontrol::I2pControlServiceState;
use i2pr_daemon::i2pcontrol_inspection::{InspectionHandles, ServiceEndpoint};
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
    assert!(result["i2p.router.clockskew"].is_null());
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
async fn canonical_transport_totals_are_served_from_published_metrics() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let inspection = Arc::new(InspectionHandles::new(
        2,
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        Vec::new(),
    ));
    let metrics = Arc::new(ControlMetrics::new());
    metrics.observe_transport(12_345, 67_890, 12, 34);
    inspection.publish_metrics(metrics);
    let (_state, address, _scope, _parent) =
        start_service_with_inspection(config, inspection).await;
    let token = authenticate(address).await;
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {
                "Token": token,
                "i2p.router.net.total.received.bytes": null,
                "i2p.router.net.total.sent.bytes": null,
            },
            "id": 2,
        }),
        &[],
    )
    .await;
    assert_eq!(
        response["result"]["i2p.router.net.total.received.bytes"],
        12_345
    );
    assert_eq!(
        response["result"]["i2p.router.net.total.sent.bytes"],
        67_890
    );
}

#[tokio::test]
async fn router_info_transport_limits_follow_validated_config_over_wire() {
    let config = Config::parse(&format!(
        "{}\n[transport.ntcp2]\nmax_active_links = 17\n[ssu2]\nmax_active_sessions = 23\n",
        config_text(TEST_PASSWORD)
    ))
    .expect("config parses");
    let inspection = Arc::new(InspectionHandles::from_config(&config));
    let (_state, address, _scope, _parent) =
        start_service_with_inspection(config.i2pcontrol.clone(), inspection).await;
    let token = authenticate(address).await;
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {
                "Token": token,
                "i2p.router.netdb.ntcp.limit": null,
                "i2p.router.netdb.ssu.limit": null,
            },
            "id": 2,
        }),
        &[],
    )
    .await;

    assert_eq!(response["result"]["i2p.router.netdb.ntcp.limit"], 17);
    assert_eq!(response["result"]["i2p.router.netdb.ssu.limit"], 23);
}

#[tokio::test]
async fn base_router_info_peer_counts_keep_the_documented_integer_shape() {
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
    inspection
        .publish_netdb(
            vec!["known-a".to_owned(), "known-b".to_owned()],
            vec!["active-a".to_owned()],
            i2pr_daemon::i2pcontrol_inspection::FloodfillMode::Disabled,
        )
        .expect("bounded NetDB snapshot publishes");
    let (_state, address, _scope, _parent) =
        start_service_with_inspection(config, inspection).await;
    let token = authenticate(address).await;
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {
                "Token": token,
                "i2p.router.netdb.knownpeers": false,
                "i2p.router.netdb.activepeers": null,
            },
            "id": 2,
        }),
        &[],
    )
    .await;
    assert_eq!(response["result"]["i2p.router.netdb.knownpeers"], 2);
    assert_eq!(response["result"]["i2p.router.netdb.activepeers"], 1);
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
    // The Proposal permits null until identity publication.
    let (_, response) = post_json(address, &body, &[]).await;
    assert!(response["result"]["i2p.router.id"].is_null());
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
    for key in ["i2p.router.news", "i2p.router.logs.clear"] {
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

/// The three canonical selectors Plan 322 Group A left open.
const TRANSIT_VOLUME_KEYS: [&str; 3] = [
    "i2p.router.net.total.transit.bytes",
    "i2p.router.net.bw.transit.15s",
    "i2p.router.net.tunnels.shareratio",
];

/// Inspection handles that publish only what a transit-volume projection
/// may read. `participation` is published exactly as given, so a test can
/// leave it unpublished to exercise the fail-closed path.
fn transit_volume_handles(
    participation: Option<i2pr_daemon::transit_volume::TransitParticipation>,
    sent_total: Option<(u64, u64)>,
) -> Arc<InspectionHandles> {
    let inspection = Arc::new(InspectionHandles::new(
        2,
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        Vec::new(),
    ));
    if let Some(participation) = participation {
        inspection.publish_transit_participation(participation);
    }
    if let Some((received, sent)) = sent_total {
        let metrics = Arc::new(ControlMetrics::new());
        metrics.observe_transport(received, sent, 0, 0);
        inspection.publish_metrics(metrics);
    }
    inspection
}

/// Plan 340: a product router publishes the disabled posture, and all
/// three selectors answer with the truth about a router that relays
/// nothing. The zeros are not substituted for a missing measurement — the
/// disabled posture owns no counters at all.
#[tokio::test]
async fn proposal_transit_volume_reflects_the_published_posture_over_wire() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let handles = transit_volume_handles(
        Some(i2pr_daemon::transit_volume::TransitParticipation::Disabled),
        None,
    );
    let (_state, address, _scope, _parent) = start_service_with_inspection(config, handles).await;
    let token = authenticate(address).await;
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {
                "Token": token,
                "i2p.router.net.total.transit.bytes": null,
                "i2p.router.net.bw.transit.15s": null,
                "i2p.router.net.tunnels.shareratio": null,
            },
            "id": 7,
        }),
        &[],
    )
    .await;
    let result = &response["result"];
    assert_eq!(
        result["i2p.router.net.total.transit.bytes"],
        serde_json::json!(0)
    );
    assert_eq!(
        result["i2p.router.net.bw.transit.15s"],
        serde_json::json!(0)
    );
    assert_eq!(
        result["i2p.router.net.tunnels.shareratio"],
        serde_json::json!(0.0),
        "a router that relays nothing shares none of its bandwidth"
    );
}

/// Plan 340: a participating router reports the volume its forward path
/// actually measured, and the share ratio is measured against the attested
/// sent total rather than assumed.
#[tokio::test]
async fn proposal_transit_volume_reports_a_participating_router_measured_volume() {
    use std::sync::{Arc as StdArc, Mutex};

    use i2pr_daemon::transit_volume::{
        TRANSIT_CELL_ACCOUNTED_BYTES, TransitParticipation, TransitVolumeCounters,
    };

    // The projection reads the request's own clock, so the recorded
    // seconds must be the trailing fifteen real ones rather than a fixed
    // epoch the window could not contain.
    let now_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("wall clock")
        .as_secs();
    let counters = StdArc::new(Mutex::new(TransitVolumeCounters::default()));
    {
        let mut guard = counters.lock().expect("unpoisoned");
        for second in 0..15_u64 {
            guard.record_forward(now_seconds - (14 - second));
        }
    }
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let handles = transit_volume_handles(
        Some(TransitParticipation::Enabled(StdArc::clone(&counters))),
        // Ten times the relayed volume, so the measured share is 0.1.
        Some((0, 10 * TRANSIT_CELL_ACCOUNTED_BYTES * 15)),
    );
    let (_state, address, _scope, _parent) = start_service_with_inspection(config, handles).await;
    let token = authenticate(address).await;
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {
                "Token": token,
                "i2p.router.net.total.transit.bytes": null,
                "i2p.router.net.bw.transit.15s": null,
                "i2p.router.net.tunnels.shareratio": null,
            },
            "id": 8,
        }),
        &[],
    )
    .await;
    let result = &response["result"];
    assert_eq!(
        result["i2p.router.net.total.transit.bytes"],
        serde_json::json!(15 * TRANSIT_CELL_ACCOUNTED_BYTES),
        "cumulative volume must equal what the forward path recorded"
    );
    assert_eq!(
        result["i2p.router.net.bw.transit.15s"],
        serde_json::json!(TRANSIT_CELL_ACCOUNTED_BYTES),
        "one cell per second across the window averages to one cell per second"
    );
    let ratio = result["i2p.router.net.tunnels.shareratio"]
        .as_f64()
        .expect("double share ratio");
    assert!(
        (ratio - 0.1).abs() < f64::EPSILON,
        "measured share must track the attested denominator, got {ratio}"
    );
}

/// Plan 340: a ratio needs a denominator. While the router participates,
/// an unattested sent total is a gap, not a `0.0` — while the two byte
/// counters keep answering, because they do not need one.
#[tokio::test]
async fn proposal_transit_share_ratio_requires_an_attested_denominator_over_wire() {
    use std::sync::{Arc as StdArc, Mutex};

    use i2pr_daemon::transit_volume::{TransitParticipation, TransitVolumeCounters};

    let now_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("wall clock")
        .as_secs();
    let counters = StdArc::new(Mutex::new(TransitVolumeCounters::default()));
    counters
        .lock()
        .expect("unpoisoned")
        .record_forward(now_seconds);
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let handles = transit_volume_handles(
        Some(TransitParticipation::Enabled(StdArc::clone(&counters))),
        None,
    );
    let (_state, address, _scope, _parent) = start_service_with_inspection(config, handles).await;
    let token = authenticate(address).await;
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {
                "Token": token,
                "i2p.router.net.tunnels.shareratio": null,
            },
            "id": 9,
        }),
        &[],
    )
    .await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_603));
    assert!(
        response.get("result").is_none(),
        "no partial result when the ratio has no denominator"
    );
    let message = response["error"]["message"]
        .as_str()
        .expect("error message");
    assert!(message.contains("i2p.router.net.tunnels.shareratio"));
    assert!(
        message.contains("Plan 340"),
        "owner plan named in {message}"
    );

    // The same owner answers the two counters that need no denominator.
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {
                "Token": token,
                "i2p.router.net.total.transit.bytes": null,
                "i2p.router.net.bw.transit.15s": null,
            },
            "id": 10,
        }),
        &[],
    )
    .await;
    let result = &response["result"];
    assert_eq!(
        result["i2p.router.net.total.transit.bytes"],
        serde_json::json!(i2pr_daemon::transit_volume::TRANSIT_CELL_ACCOUNTED_BYTES)
    );
    // One cell inside the window floors to 68 bytes/second, which proves
    // the window reads real recorded volume rather than a default.
    assert_eq!(
        result["i2p.router.net.bw.transit.15s"],
        serde_json::json!(i2pr_daemon::transit_volume::TRANSIT_CELL_ACCOUNTED_BYTES / 15)
    );
}

/// The canonical fail-closed proof the source matrix's `Unavailable`
/// invariant names: with no published transit posture there is no owner to
/// answer from, and every selector fails closed naming its field and the
/// owning plan, with no partial RouterInfo result. An absent owner is
/// never read as a router relaying nothing.
#[tokio::test]
async fn proposal_unavailable_sources_fail_closed_over_wire() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let handles = transit_volume_handles(None, None);
    let (_state, address, _scope, _parent) = start_service_with_inspection(config, handles).await;
    let token = authenticate(address).await;
    for (id, key) in TRANSIT_VOLUME_KEYS.into_iter().enumerate() {
        let (_, response) = post_json(
            address,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "RouterInfo",
                "params": {"Token": token, key: null},
                "id": id,
            }),
            &[],
        )
        .await;
        assert_eq!(
            response["error"]["code"],
            serde_json::json!(-32_603),
            "{key}"
        );
        assert!(
            response.get("result").is_none(),
            "no partial result for {key}"
        );
        let message = response["error"]["message"]
            .as_str()
            .expect("error message");
        assert!(message.contains(key), "field named in {message}");
        assert!(
            message.contains("Plan 340"),
            "owner plan named in {message}"
        );
    }
}

/// The five canonical selectors Plan 170 adopts from i2pd.
const PER_FAMILY_KEYS: [&str; 5] = [
    "i2p.router.net.status.v6",
    "i2p.router.net.error",
    "i2p.router.net.error.v6",
    "i2p.router.net.testing",
    "i2p.router.net.testing.v6",
];

/// A bounded runtime SSU2 service that has registered with the inspection
/// plane but has never bound a socket, so both families are inert and no
/// reachability claim exists.
fn unregistered_family_ssu2_service() -> i2pr_runtime::Ssu2RuntimeService {
    let bundle = i2pr_crypto::RouterIdentityBundle::generate(&mut i2pr_crypto::OsRng)
        .expect("controlled identity bundle");
    let identity =
        i2pr_daemon::router_i2np::generate_controlled_identity(&bundle, "127.0.0.1", 44_001)
            .expect("controlled identity");
    i2pr_runtime::Ssu2RuntimeService::new(i2pr_runtime::Ssu2RuntimeConfig::default(), identity)
        .expect("valid runtime service")
}

/// Inspection handles with the per-family transport owner published, and the
/// NetDB attested either populated or empty.
fn per_family_handles(known_peers: Option<Vec<String>>) -> Arc<InspectionHandles> {
    let inspection = Arc::new(InspectionHandles::new(
        2,
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        Vec::new(),
    ));
    inspection.publish_ssu2(unregistered_family_ssu2_service());
    if let Some(known) = known_peers {
        inspection
            .publish_netdb(
                known,
                Vec::new(),
                i2pr_daemon::i2pcontrol_inspection::FloodfillMode::Disabled,
            )
            .expect("bounded NetDB snapshot publishes");
    }
    inspection
}

/// Plan 339: with a registered transport owner and an attested populated
/// NetDB, all five selectors return exact i2pd-enumeration integers. The
/// baseline is the honest one — no status claim, no error, and not testing —
/// because this router has never observed reachability.
#[tokio::test]
async fn proposal_per_family_network_condition_over_wire() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let (_state, address, _scope, _parent) =
        start_service_with_inspection(config, per_family_handles(Some(vec!["known-a".to_owned()])))
            .await;
    let token = authenticate(address).await;
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {
                "Token": token,
                "i2p.router.net.status.v6": null,
                "i2p.router.net.error": null,
                "i2p.router.net.error.v6": null,
                "i2p.router.net.testing": null,
                "i2p.router.net.testing.v6": null,
            },
            "id": 2,
        }),
        &[],
    )
    .await;
    let result = &response["result"];
    assert_eq!(
        result["i2p.router.net.status.v6"],
        serde_json::json!(2),
        "Unknown"
    );
    assert_eq!(result["i2p.router.net.error"], serde_json::json!(0), "None");
    assert_eq!(
        result["i2p.router.net.error.v6"],
        serde_json::json!(0),
        "None"
    );
    assert_eq!(result["i2p.router.net.testing"], serde_json::json!(0));
    assert_eq!(result["i2p.router.net.testing.v6"], serde_json::json!(0));

    // Every emitted value must sit inside the adopted i2pd enumeration.
    for key in PER_FAMILY_KEYS {
        let value = result[key]
            .as_i64()
            .unwrap_or_else(|| panic!("{key} is not an integer"));
        assert!(
            (0..=5).contains(&value),
            "{key} = {value} is outside the adopted enumeration"
        );
    }
}

/// Plan 339: an *attested* empty NetDB is `NoDescriptors` (5) on both family
/// selectors, while a populated one is `None` (0). The error rows therefore
/// read a real owner rather than a constant.
#[tokio::test]
async fn proposal_per_family_error_tracks_the_attested_netdb() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let (_state, address, _scope, _parent) =
        start_service_with_inspection(config, per_family_handles(Some(Vec::new()))).await;
    let token = authenticate(address).await;
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {
                "Token": token,
                "i2p.router.net.error": null,
                "i2p.router.net.error.v6": null,
            },
            "id": 2,
        }),
        &[],
    )
    .await;
    assert_eq!(
        response["result"]["i2p.router.net.error"],
        serde_json::json!(5)
    );
    assert_eq!(
        response["result"]["i2p.router.net.error.v6"],
        serde_json::json!(5)
    );
}

/// Plan 339: gating is per key. With no attested NetDB the two error rows
/// fail the request closed and name Plan 339, because "no descriptors" is a
/// claim about the NetDB — while `status.v6` and `testing.v6`, which do not
/// depend on it, still answer.
#[tokio::test]
async fn proposal_per_family_error_fails_closed_without_an_attested_netdb() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let (_state, address, _scope, _parent) =
        start_service_with_inspection(config, per_family_handles(None)).await;
    let token = authenticate(address).await;
    for (id, key) in ["i2p.router.net.error", "i2p.router.net.error.v6"]
        .into_iter()
        .enumerate()
    {
        let (_, response) = post_json(
            address,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "RouterInfo",
                "params": {"Token": token, key: null},
                "id": id,
            }),
            &[],
        )
        .await;
        assert_eq!(
            response["error"]["code"],
            serde_json::json!(-32_603),
            "{key}"
        );
        assert!(
            response.get("result").is_none(),
            "no partial result for {key}"
        );
        let message = response["error"]["message"]
            .as_str()
            .expect("error message");
        assert!(message.contains(key), "field named in {message}");
        assert!(
            message.contains("Plan 339"),
            "owner plan named in {message}"
        );
    }
    // The rows that do not depend on the NetDB still answer.
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {
                "Token": token,
                "i2p.router.net.status.v6": null,
                "i2p.router.net.testing": null,
                "i2p.router.net.testing.v6": null,
            },
            "id": 9,
        }),
        &[],
    )
    .await;
    assert_eq!(
        response["result"]["i2p.router.net.status.v6"],
        serde_json::json!(2)
    );
    assert_eq!(
        response["result"]["i2p.router.net.testing"],
        serde_json::json!(0)
    );
    assert_eq!(
        response["result"]["i2p.router.net.testing.v6"],
        serde_json::json!(0)
    );
}

/// Plan 339: with no transport owner registered at all, all five rows fail
/// closed and name Plan 339. The previous fail-closed behavior is preserved,
/// not weakened by the new owner.
#[tokio::test]
async fn proposal_per_family_condition_fails_closed_without_a_transport_owner() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let inspection = Arc::new(InspectionHandles::new(
        2,
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        Vec::new(),
    ));
    inspection
        .publish_netdb(
            vec!["known-a".to_owned()],
            Vec::new(),
            i2pr_daemon::i2pcontrol_inspection::FloodfillMode::Disabled,
        )
        .expect("bounded NetDB snapshot publishes");
    let (_state, address, _scope, _parent) =
        start_service_with_inspection(config, inspection).await;
    let token = authenticate(address).await;
    for (id, key) in PER_FAMILY_KEYS.into_iter().enumerate() {
        let (_, response) = post_json(
            address,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "RouterInfo",
                "params": {"Token": token, key: null},
                "id": id,
            }),
            &[],
        )
        .await;
        assert_eq!(
            response["error"]["code"],
            serde_json::json!(-32_603),
            "{key}"
        );
        assert!(
            response.get("result").is_none(),
            "no partial result for {key}"
        );
        let message = response["error"]["message"]
            .as_str()
            .expect("error message");
        assert!(message.contains(key), "field named in {message}");
        assert!(
            message.contains("Plan 339"),
            "owner plan named in {message}"
        );
    }
}

#[tokio::test]
async fn authenticated_router_info_logs_clear_clears_ring_and_returns_success() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let inspection = Arc::new(InspectionHandles::new(
        2,
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        Vec::new(),
    ));
    let ring = Arc::new(i2pr_daemon::control_sources::LogRing::new());
    ring.record("INFO", "daemon", "before clear");
    inspection.publish_log_ring(Arc::clone(&ring));
    let (_state, address, _scope, _parent) =
        start_service_with_inspection(config, inspection).await;
    let token = authenticate(address).await;
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {
                "Token": token,
                "i2p.router.logs": null,
                "i2p.router.logs.clear": null,
            },
            "id": 2,
        }),
        &[],
    )
    .await;
    assert_eq!(
        response["result"]["i2p.router.logs"],
        serde_json::json!(["INFO daemon: before clear"])
    );
    assert_eq!(response["result"]["i2p.router.logs.clear"], "success");
    assert!(ring.is_empty());
}

#[tokio::test]
async fn failed_mixed_router_info_selection_does_not_clear_logs() {
    let config = Config::parse(&config_text(TEST_PASSWORD))
        .expect("config parses")
        .i2pcontrol;
    let inspection = Arc::new(InspectionHandles::new(
        2,
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        Vec::new(),
    ));
    let ring = Arc::new(i2pr_daemon::control_sources::LogRing::new());
    ring.record("INFO", "daemon", "must remain on failed request");
    inspection.publish_log_ring(Arc::clone(&ring));
    let (_state, address, _scope, _parent) =
        start_service_with_inspection(config, inspection).await;
    let token = authenticate(address).await;
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": {
                "Token": token,
                "i2p.router.logs.clear": null,
                "i2p.router.news": null,
            },
            "id": 3,
        }),
        &[],
    )
    .await;
    assert_eq!(response["error"]["code"], -32603);
    assert_eq!(ring.snapshot().0.len(), 1);
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
    assert!(response["result"]["i2p.router.clockskew"].is_null());
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
    assert!(elements[0]["result"]["i2p.router.clockskew"].is_null());
    assert!(elements[1]["result"]["i2p.router.info"].is_null());
}
