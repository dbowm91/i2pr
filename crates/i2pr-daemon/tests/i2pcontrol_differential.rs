//! Plan 295 differential corpus: one bounded JSON-RPC request set run
//! against the production daemon composition (default) or, when the
//! `I2PR_I2PCONTROL_TARGET` / `I2PR_I2PCONTROL_PASSWORD` environment
//! pair provisions an external router, against that target.
//!
//! Local mode executes the current canonical disposition matrix (every answerable
//! row answers; every unowned row fails the whole request with its
//! owning-plan marker) and emits one sanitized `PLAN295-CORPUS` line
//! carrying counts plus a shape hash (tokens and passwords redacted
//! before hashing; no secret ever reaches the evidence). The
//! integration lane captures that line into the evidence file.
//!
//! External mode (`#[ignore]`-gated) runs the same corpus for
//! envelope validity and prints per-row shape hashes for manual
//! differential analysis; it never asserts i2pr's dispositions on a
//! foreign router. Missing target env in explicit mode fails loudly,
//! never silently passes.
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

/// Operator password for the locally composed service.
const TEST_PASSWORD: &str = "black-box-295-differential-password";
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

/// Builds an enabled loopback TOML config with an ephemeral port over
/// the given data directory.
fn config_text(data_dir: &std::path::Path, password: &str) -> String {
    format!(
        "schema_version = 1\n[router]\ndata_dir = {:?}\n[i2pcontrol]\nenabled = true\nbind_address = \"127.0.0.1\"\nport = 0\npassword = {password:?}\n",
        data_dir.to_string_lossy()
    )
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

/// Splits an HTTP response into status plus body.
fn parse_response(response: &[u8]) -> (u16, Vec<u8>) {
    let head_end = memchr_split(response);
    let head = std::str::from_utf8(&response[..head_end]).expect("head is ASCII");
    let status: u16 = head
        .lines()
        .next()
        .expect("status line")
        .split_whitespace()
        .nth(1)
        .expect("status code")
        .parse()
        .expect("status parses");
    (status, response[head_end..].to_vec())
}

/// Finds the end of the HTTP head (`\r\n\r\n` boundary).
fn memchr_split(response: &[u8]) -> usize {
    response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .expect("head boundary")
}

/// Sends one JSON-RPC body with compliant framing.
async fn post_json(address: SocketAddr, body: &serde_json::Value) -> (u16, serde_json::Value) {
    let bytes = serde_json::to_vec(body).expect("body serializes");
    let head = format!(
        "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        bytes.len()
    );
    let (status, response_body) = raw_request(address, &head, &bytes).await;
    assert_eq!(status, 200);
    let parsed: serde_json::Value =
        serde_json::from_slice(&response_body).expect("response parses");
    (status, parsed)
}

/// Authenticates and returns the minted token.
async fn authenticate(address: SocketAddr, password: &str) -> String {
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "Authenticate",
            "params": {"API": 1, "Password": password},
            "id": 1,
        }),
    )
    .await;
    response["result"]["Token"]
        .as_str()
        .expect("token minted")
        .to_owned()
}

/// Redacts secret-carrying values before evidence hashing.
fn sanitized(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(key, item)| {
                    if key == "Token" || key == "Password" {
                        (
                            key.clone(),
                            serde_json::Value::String("[redacted]".to_owned()),
                        )
                    } else {
                        (key.clone(), sanitized(item))
                    }
                })
                .collect(),
        ),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(sanitized).collect())
        }
        serde_json::Value::String(text) => {
            // Tokens minted by Authenticate are the only free-form
            // secret strings in responses; 40+ char opaque strings
            // are treated as secret-carrying.
            if text.len() >= 40 && text.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
                serde_json::Value::String("[redacted]".to_owned())
            } else {
                value.clone()
            }
        }
        _ => value.clone(),
    }
}

/// Short hex of the sanitized shape hash (evidence only, no secrets).
fn shape_hash(value: &serde_json::Value) -> String {
    let canonical = serde_json::to_string(&sanitized(value)).expect("canonicalizes");
    let digest = i2pr_crypto::sha256(canonical.as_bytes());
    hex_prefix(digest.as_bytes())
}

/// Lowercase hex of the first 8 digest bytes.
fn hex_prefix(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(16);
    for byte in bytes.iter().take(8) {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0F) as usize] as char);
    }
    out
}

/// Canonical selectors the default production composition answers (24 rows;
/// address-book rows gap while the subsystem is disabled, transport totals
/// until a source sample, and news never serves).
const ANSWERABLE: [&str; 24] = [
    "i2p.router.version",
    "i2p.router.status",
    "i2p.router.uptime",
    "i2p.router.id",
    "i2p.router.info",
    "i2p.router.net.tunnels.participating",
    "i2p.router.netdb.peers.list",
    "i2p.router.netdb.activepeers.info",
    "i2p.router.netdb.peers.info",
    "i2p.router.netdb.activepeers.stats",
    "i2p.router.netdb.bannedpeers",
    "i2p.router.netdb.peers",
    "i2p.router.netdb.activepeers.list",
    "i2p.router.net.tunnels.i2ptunnel",
    "i2p.router.net.tunnels.queue",
    "i2p.router.net.tunnels.tbmqueue",
    "i2p.router.net.tunnels.exploratory.inbound",
    "i2p.router.net.tunnels.exploratory.outbound",
    "i2p.router.net.tunnels.exploratory.info.list",
    "i2p.router.net.tunnels.client.inbound",
    "i2p.router.net.tunnels.client.outbound",
    "i2p.router.net.tunnels.client.info.list",
    "i2p.router.net.tunnels.participating.info",
    "i2p.router.clockskew",
];

/// Selectors that fail the whole request with an owning-plan marker
/// under the default composition (address book disabled, transport
/// totals and tunnel-build outcomes not yet sampled, news never served).
const GAPPED: [(&str, Option<&str>); 11] = [
    ("i2p.router.news", Some("295")),
    ("i2p.router.net.total.received.bytes", Some("322")),
    ("i2p.router.net.total.sent.bytes", Some("322")),
    ("i2p.router.net.tunnels.totalsuccessrate", Some("322")),
    ("i2p.router.net.tunnels.successrate", Some("322")),
    ("i2p.router.addressbook.private.list", None),
    ("i2p.router.addressbook.local.list", None),
    ("i2p.router.addressbook.router.list", None),
    ("i2p.router.addressbook.published.list", None),
    ("i2p.router.addressbook.subscriptions", None),
    ("i2p.router.addressbook.config", None),
];

/// Runs the corpus against one target and returns
/// `(answered, gapped, errors, shape_hash)`.
async fn run_corpus(
    address: SocketAddr,
    password: &str,
    strict: bool,
) -> (usize, usize, usize, String) {
    let token = authenticate(address, password).await;
    let mut answered = 0_usize;
    let mut gapped = 0_usize;
    let mut errors = 0_usize;
    let mut shapes = String::new();

    // One batched RouterInfo request over every answerable selector.
    let mut params = serde_json::Map::new();
    params.insert("Token".to_owned(), serde_json::Value::String(token.clone()));
    for selector in ANSWERABLE {
        params.insert(selector.to_owned(), serde_json::Value::Null);
    }
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "RouterInfo",
            "params": serde_json::Value::Object(params),
            "id": 10,
        }),
    )
    .await;
    let result = &response["result"];
    assert!(result.is_object(), "answerable subset answers: {response}");
    for selector in ANSWERABLE {
        assert!(
            result.get(selector).is_some(),
            "missing row {selector}: {result}"
        );
        answered += 1;
        if selector == "i2p.router.uptime" {
            // Control-plane uptime advances across runs; the shape
            // hash records the numeric type, never the instant.
            assert!(result[selector].is_number(), "uptime numeric: {result}");
            shapes.push_str("uptime:number");
        } else {
            shapes.push_str(&shape_hash(&result[selector]));
        }
    }

    // Each unowned selector fails its own request with its plan marker.
    for (selector, plan) in GAPPED {
        let (_, response) = post_json(
            address,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "RouterInfo",
                "params": {"Token": token, selector: null},
                "id": 11,
            }),
        )
        .await;
        let error = &response["error"];
        assert!(error.is_object(), "gap is an error: {response}");
        if strict {
            assert_eq!(error["code"], serde_json::json!(-32603));
            if let Some(plan) = plan {
                assert!(
                    error["message"].as_str().unwrap_or_default().contains(plan),
                    "gap names {plan}: {error}"
                );
            }
        }
        gapped += 1;
        shapes.push_str(&shape_hash(error));
    }

    // ClientServicesInfo answers every service (disabled is truthful).
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "ClientServicesInfo",
            "params": {
                "Token": token,
                "I2PTunnel": null, "HTTPProxy": null, "SOCKS": null,
                "SAM": null, "BOB": null, "I2CP": null,
            },
            "id": 12,
        }),
    )
    .await;
    assert!(
        response["result"].is_object(),
        "services answer: {response}"
    );
    answered += 6;
    shapes.push_str(&shape_hash(&response["result"]));

    // Disabled address book fails explicitly, never fabricates.
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "AddressBook",
            "params": {"Token": token, "Type": "private", "Hostname": "a.i2p"},
            "id": 13,
        }),
    )
    .await;
    assert!(response["error"].is_object(), "disabled errors: {response}");
    if strict {
        assert_eq!(response["error"]["code"], serde_json::json!(-32603));
        assert!(
            response["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .contains("not active"),
            "disabled marker: {response}"
        );
    }
    errors += 1;
    shapes.push_str(&shape_hash(&response["error"]));

    // Unknown tunnel reads fail with the unknown-tunnel marker.
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "TunnelManager",
            "params": {"Token": token, "Name": "no-such-tunnel", "Action": "get"},
            "id": 14,
        }),
    )
    .await;
    assert!(
        response["result"].is_object() || response["error"].is_object(),
        "tunnel envelope: {response}"
    );
    if strict {
        assert!(
            response["result"]["status"]
                .as_str()
                .unwrap_or_default()
                .starts_with("error - ")
        );
        assert!(
            response["result"]["status"]
                .as_str()
                .unwrap_or_default()
                .contains("unknown tunnel"),
            "unknown-tunnel marker: {response}"
        );
    }
    errors += 1;
    shapes.push_str(&shape_hash(&response["result"]));

    // Deep-option incompatibility carries the Plan 293 determination.
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "TunnelManager",
            "params": {
                "Token": token, "Name": "sig-probe", "Action": "create",
                "Type": "client", "SigType": "EDDSA_SHA512_ED25519",
            },
            "id": 15,
        }),
    )
    .await;
    assert!(
        response["result"].is_object() || response["error"].is_object(),
        "determination envelope: {response}"
    );
    if strict {
        let haystack = serde_json::to_string(&response).expect("serializes");
        assert!(
            haystack.contains("Plan 293 determination"),
            "determination marker: {response}"
        );
    }
    errors += 1;
    shapes.push_str(&shape_hash(&response["result"]));

    // Unknown method keeps its typed envelope.
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "NoSuchMethod",
            "params": {"Token": token},
            "id": 16,
        }),
    )
    .await;
    assert!(response["error"].is_object(), "method envelope: {response}");
    if strict {
        assert_eq!(response["error"]["code"], serde_json::json!(-32601));
    }
    errors += 1;
    shapes.push_str(&shape_hash(&response["error"]));

    let digest = i2pr_crypto::sha256(shapes.as_bytes());
    (answered, gapped, errors, hex_prefix(digest.as_bytes()))
}

/// Local differential corpus against the production composition.
///
/// Mirrors the supervised factory exactly (graph publishers, control
/// construction, address-book installation, control startup) without
/// running the supervisor: every constructor is the production one in
/// production order. Identity/address-book owners stay
/// unpublished/inactive by default, which the corpus asserts.
#[tokio::test]
async fn differential_corpus_against_production_composition() {
    let data = tempfile::tempdir().expect("temp data dir");
    let config = Config::parse(&config_text(data.path(), TEST_PASSWORD)).expect("config parses");
    // Production publishers, no services running: every attestation
    // the composition root installs is live.
    let (_graph, inspection) =
        i2pr_daemon::build_daemon_graph_with_inspection(&config).expect("graph builds");
    // Mirror the production i2pcontrol factory: control state plus
    // the disabled address-book manager, then control startup.
    let control = Arc::new(
        i2pr_daemon::i2pcontrol_tunnels::TunnelControlState::for_config(&config)
            .expect("control builds for config"),
    );
    let addressbook = Arc::new(i2pr_daemon::addressbook::AddressBookManager::activate(
        config.addressbook.clone(),
    ));
    let state = Arc::new(
        I2pControlServiceState::new_with_inspection(
            config.i2pcontrol.clone(),
            Arc::clone(&inspection),
        )
        .expect("state builds"),
    );
    state.set_control_manager(Arc::clone(&control));
    state.set_addressbook_manager(addressbook);
    let (listener, address) = state
        .bind(SocketAddr::new(
            IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
            0,
        ))
        .await
        .expect("bind succeeds");
    let parent = CancellationToken::new();
    let scope = ChildScope::for_test(&parent, ChildFailurePolicy::FailParent);
    let failures = control.startup(&scope, &parent).await;
    assert!(failures.is_empty(), "control startup clean: {failures:?}");
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
    let (answered, gapped, errors, shape) = run_corpus(address, TEST_PASSWORD, true).await;
    assert_eq!(answered, 30, "24 canonical rows + 6 services answer");
    assert_eq!(
        gapped, 11,
        "news + 2 unsampled transport totals + 2 unobserved build ratios + 6 address-book rows gap"
    );
    assert_eq!(
        errors, 4,
        "address-book, tunnel, determination, method errors"
    );
    println!(
        "PLAN295-CORPUS target=local answered={answered} gapped={gapped} errors={errors} shape={shape}"
    );
}

/// External differential corpus against a provisioned router.
///
/// Requires `I2PR_I2PCONTROL_TARGET=host:port` plus
/// `I2PR_I2PCONTROL_PASSWORD`. Missing env fails loudly (never a
/// silent pass); envelope validity is asserted, per-row dispositions
/// are recorded for manual analysis, never asserted.
#[tokio::test]
#[ignore = "requires a provisioned external I2PControl target"]
async fn differential_corpus_against_provisioned_target() {
    let target = std::env::var("I2PR_I2PCONTROL_TARGET").expect(
        "I2PR_I2PCONTROL_TARGET=host:port is required for the external differential corpus",
    );
    let password = std::env::var("I2PR_I2PCONTROL_PASSWORD")
        .expect("I2PR_I2PCONTROL_PASSWORD is required for the external differential corpus");
    let address: SocketAddr = target
        .parse()
        .expect("I2PR_I2PCONTROL_TARGET must be host:port");
    assert!(
        address.ip() != IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)) || address.port() != 0,
        "target must be explicit"
    );
    let (answered, gapped, errors, shape) = run_corpus(address, &password, false).await;
    println!(
        "PLAN295-CORPUS target=external answered={answered} gapped={gapped} errors={errors} shape={shape}"
    );
}
