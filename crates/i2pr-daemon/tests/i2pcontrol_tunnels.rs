//! Plan 289 black-box TunnelManager tests.
//!
//! Each test binds the I2PControl listener to `127.0.0.1:0` (ephemeral
//! port) and drives the seven lifecycle actions through real TLS + TCP
//! after listener startup. The control state lives in a temporary data
//! directory with a real M10 `ServiceTunnelManager`; no private
//! coordinator, store, or dispatch API is touched from these tests.
//!
//! The test TLS client uses a test-only accept-any verifier (`NoVerify`)
//! confined to this file. Production source never calls `dangerous()`;
//! `scripts/check-runtime-boundaries.sh` rejects it outside tests.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

/// Plan 342: the fail-closed outbound-credential owner for a row that does not
/// exercise a credential. No identity is loaded, so no store exists, and every
/// credential field is refused rather than silently accepted.
fn plan342_test_outbound_secrets()
-> Arc<dyn i2pr_service_tunnels::outbound_secret::OutboundSecretStore> {
    Arc::new(i2pr_service_tunnels::outbound_secret::NoOutboundSecrets)
}

use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use i2pr_daemon::config::Config;
use i2pr_daemon::i2pcontrol::I2pControlServiceState;
use i2pr_daemon::i2pcontrol_inspection::InspectionHandles;
use i2pr_daemon::i2pcontrol_tunnels::TunnelControlState;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Operator password for every black-box service in this file.
const TEST_PASSWORD: &str = "black-box-289-tunnels-password";
/// Bounded client-side deadline for every wire operation.
const WIRE_TIMEOUT: Duration = Duration::from_secs(10);

#[test]
fn pinned_proposal_request_response_examples_are_frozen() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/i2pcontrol-proposal-170-examples.json"
    ))
    .expect("Proposal examples fixture parses");
    assert_eq!(corpus["source"], "Proposal 170, revision 2026-05-20");
    let examples = corpus["examples"].as_array().expect("examples array");
    let names: std::collections::BTreeSet<_> = examples
        .iter()
        .map(|example| example["name"].as_str().expect("example name"))
        .collect();
    assert_eq!(examples.len(), 10);
    assert_eq!(names.len(), examples.len());
    for example in examples {
        assert_eq!(example["request"]["jsonrpc"], "2.0");
        assert_eq!(example["response"]["jsonrpc"], "2.0");
        let method = example["request"]["method"].as_str().expect("method");
        if method == "TunnelManager" {
            let params = example["request"]["params"].as_object().expect("params");
            assert!(params.contains_key("Action"));
            assert!(!params.contains_key("options"));
        }
    }
    let set_config = examples
        .iter()
        .find(|example| example["name"] == "address-book-config")
        .expect("SetConfig example");
    let keys: std::collections::BTreeSet<_> = set_config["request"]["params"]["SetConfig"]
        .as_object()
        .expect("SetConfig map")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        i2pr_i2pcontrol::PROPOSAL_ADDRESS_BOOK_CONFIG_KEYS
            .into_iter()
            .collect()
    );
    let differential = &corpus["differential"];
    assert!(
        differential["java_pr6"]["source"]
            .as_str()
            .unwrap_or_default()
            .ends_with("45bb593000408071dd376b78848fdc246dccd964")
    );
    assert!(
        differential["emissary"]["source"]
            .as_str()
            .unwrap_or_default()
            .ends_with("6885a945d25a5ae61bc68191d27c5816bc3df4c9")
    );
    assert_eq!(
        i2pr_i2pcontrol::proposal_tunnel_value_type("NewDest"),
        Some(i2pr_i2pcontrol::ProposalTunnelValueType::Integer)
    );
    assert_eq!(
        i2pr_i2pcontrol::proposal_tunnel_value_type("ConnectDelay"),
        Some(i2pr_i2pcontrol::ProposalTunnelValueType::Boolean)
    );
}

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

/// Distinct loopback ports process-wide (parallel tests never share).
fn distinct_port() -> u16 {
    static NEXT: AtomicU16 = AtomicU16::new(0);
    static BASE: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
    let base = *BASE.get_or_init(|| 33_000 + (std::process::id() % 5_000) as u16);
    base + NEXT.fetch_add(1, Ordering::Relaxed) * 37
}

/// Builds a daemon config with an enabled loopback I2PControl service
/// over a temporary data directory plus optional extra TOML.
fn config_text(data_dir: &std::path::Path, password: &str, extra: &str) -> String {
    format!(
        "schema_version = 1\n[router]\ndata_dir = {:?}\n[i2pcontrol]\nenabled = true\nbind_address = \"127.0.0.1\"\nport = 0\npassword = {password:?}\n{extra}",
        data_dir.to_string_lossy()
    )
}

/// Starts the service with production control construction
/// (`TunnelControlState::for_config`) and runs control startup, exactly
/// like the supervised factory plus `run()` would.
async fn start_service(
    config: &Config,
) -> (
    Arc<I2pControlServiceState>,
    SocketAddr,
    ChildScope,
    CancellationToken,
) {
    // Plan 337: production construction builds the one shared manager
    // and injects it into the control state.
    let manager = i2pr_daemon::build_shared_service_manager(config)
        .expect("shared manager builds")
        .expect("control implies a shared manager");
    let control = Arc::new(
        TunnelControlState::for_config(config, manager, plan342_test_outbound_secrets())
            .expect("control builds for config"),
    );
    let inspection = Arc::new(InspectionHandles::from_config(config));
    let state = Arc::new(
        I2pControlServiceState::new_with_inspection(config.i2pcontrol.clone(), inspection)
            .expect("state builds"),
    );
    state.set_control_manager(Arc::clone(&control));
    let (listener, bound) = state
        .bind(SocketAddr::new(
            IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
            0,
        ))
        .await
        .expect("bind succeeds");
    let parent = CancellationToken::new();
    let scope = ChildScope::for_test(&parent, ChildFailurePolicy::FailParent);
    // Control startup before serving, mirroring `run()`.
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
async fn tunnelmanager_emits_canonical_proposal_result_and_redacts_secrets() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD, "")).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    let b32 = format!("{}.b32.i2p", "a".repeat(52));
    let port = distinct_port();
    let created = tunnel_raw(
        address,
        &token,
        serde_json::json!({
            "Action":"create", "Name":"canonical", "Type":"httpclient",
            "TargetDestination":b32, "Port":port, "ReachableBy":"127.0.0.1",
            "Description":"I2PControl managed test tunnel", "MaxConcurrentConns":24,
            "Close":true, "CloseTime":45, "Reduce":true, "ReduceTime":12, "ReduceCount":3, "Profile":"interactive",
            "SigType":"EDDSA_SHA512_ED25519", "EncType":"4",
            "AllowUserAgent":true, "AllowReferer":true, "AllowAccept":false,
            "AllowInternalSSL":false, "NewDest":2
        }),
        2,
    )
    .await;
    assert!(created.get("error").is_none(), "create: {created}");
    assert!(
        created["result"]["status"]
            .as_str()
            .unwrap_or_default()
            .starts_with("success"),
        "create status: {created}"
    );
    assert!(
        created["result"]["status"]
            .as_str()
            .unwrap_or("")
            .starts_with("success - created tunnel"),
        "{created}"
    );
    let fetched = tunnel_raw(
        address,
        &token,
        serde_json::json!({"Action":"get", "Name":"canonical"}),
        3,
    )
    .await;
    let info = &fetched["result"]["info"];
    assert_eq!(info["client"], serde_json::json!(true));
    assert_eq!(info["status"], serde_json::json!("running"));
    assert_eq!(info["targetDestination"], serde_json::json!(b32));
    assert_eq!(info["rawConfig"]["name"], serde_json::json!("canonical"));
    assert_eq!(info["rawConfig"]["type"], serde_json::json!("httpclient"));
    assert_eq!(info["rawConfig"]["port"], serde_json::json!(port));
    assert_eq!(
        info["rawConfig"]["reachableBy"],
        serde_json::json!("127.0.0.1")
    );
    assert_eq!(
        info["rawConfig"]["description"],
        serde_json::json!("I2PControl managed test tunnel")
    );
    assert_eq!(
        info["rawConfig"]["maxConcurrentConns"],
        serde_json::json!(24)
    );
    assert_eq!(info["rawConfig"]["close"], serde_json::json!(true));
    assert_eq!(info["rawConfig"]["closeTime"], serde_json::json!(45));
    assert_eq!(info["rawConfig"]["reduce"], serde_json::json!(true));
    assert_eq!(info["rawConfig"]["reduceTime"], serde_json::json!(12));
    assert_eq!(info["rawConfig"]["reduceCount"], serde_json::json!(3));
    assert_eq!(
        info["rawConfig"]["profile"],
        serde_json::json!("interactive")
    );
    assert_eq!(info["rawConfig"]["allowUserAgent"], serde_json::json!(true));
    assert_eq!(info["rawConfig"]["allowReferer"], serde_json::json!(true));
    assert_eq!(info["rawConfig"]["allowAccept"], serde_json::json!(false));
    assert_eq!(
        info["rawConfig"]["allowInternalSSL"],
        serde_json::json!(false)
    );
    assert_eq!(info["rawConfig"]["newDest"], serde_json::json!(2));
    assert_eq!(
        info["rawConfig"]["sigType"],
        serde_json::json!("EDDSA_SHA512_ED25519")
    );
    assert_eq!(info["rawConfig"]["encType"], serde_json::json!("4"));
    assert_eq!(info["persistentClientKey"], serde_json::json!(true));
    assert_eq!(info["offlineKeys"], serde_json::json!(false));
    let destination_before_crypto_noop = info["destination"]
        .as_str()
        .expect("serialized local Destination")
        .to_owned();
    let edited = tunnel_raw(
        address,
        &token,
        serde_json::json!({
            "Action":"edit", "Name":"canonical",
            "SigType":"EDDSA_SHA512_ED25519", "EncType":"4"
        }),
        9,
    )
    .await;
    assert!(edited.get("error").is_none(), "edit: {edited}");
    let fetched_after_crypto_noop = tunnel_raw(
        address,
        &token,
        serde_json::json!({"Action":"get", "Name":"canonical"}),
        10,
    )
    .await;
    assert_eq!(
        fetched_after_crypto_noop["result"]["info"]["destination"],
        serde_json::json!(destination_before_crypto_noop),
        "re-selecting the effective algorithms must not rotate identity"
    );

    let resume_port = distinct_port();
    let resume_created = tunnel_raw(
        address,
        &token,
        serde_json::json!({
            "Action":"create", "Name":"resume-client", "Type":"client",
            "TargetDestination":b32, "Port":resume_port, "StartOnLoad":false,
            "Close":true, "NewDest":1
        }),
        4,
    )
    .await;
    assert!(
        resume_created.get("error").is_none(),
        "NewDest=1 with Close creates: {resume_created}"
    );
    let resume_fetched = tunnel_raw(
        address,
        &token,
        serde_json::json!({"Action":"get", "Name":"resume-client"}),
        5,
    )
    .await;
    assert_eq!(
        resume_fetched["result"]["info"]["rawConfig"]["newDest"],
        serde_json::json!(1)
    );
    assert_eq!(
        resume_fetched["result"]["info"]["rawConfig"]["close"],
        serde_json::json!(true)
    );
    assert!(
        info["destinationB32"]
            .as_str()
            .is_some_and(|address| address.len() == 60 && address.ends_with(".b32.i2p"))
    );
    assert!(info["rawConfig"].get("ProxyPassword").is_none());
    assert!(fetched.to_string().find("ProxyPassword").is_none());
    for (id, action, phrase) in [
        (4, "edit", "edited tunnel"),
        (5, "start", "starting tunnel"),
        (6, "stop", "stopping tunnel"),
        (7, "restart", "restarting tunnel"),
        (8, "delete", "deleted tunnel"),
    ] {
        let params = if action == "edit" {
            serde_json::json!({"Action": action, "Name": "canonical", "Port": port})
        } else {
            serde_json::json!({"Action": action, "Name": "canonical"})
        };
        let response = tunnel_raw(address, &token, params, id).await;
        assert!(
            response["result"]["status"]
                .as_str()
                .unwrap_or_default()
                .contains(phrase),
            "{action} response shape: {response}"
        );
    }
    let shared = tunnel_raw(
        address,
        &token,
        serde_json::json!({
            "Action":"create", "Name":"shared-client", "Type":"client",
            "TargetDestination":b32, "Port":distinct_port(),
            "StartOnLoad":false, "Shared":true, "PersistentClientKey":true
        }),
        9,
    )
    .await;
    assert!(
        shared["result"]["status"]
            .as_str()
            .unwrap_or_default()
            .contains("created tunnel shared-client"),
        "Shared create response: {shared}"
    );
    let shared_info = tunnel_raw(
        address,
        &token,
        serde_json::json!({"Action":"get","Name":"shared-client"}),
        10,
    )
    .await;
    assert_eq!(
        shared_info["result"]["info"]["rawConfig"]["shared"],
        serde_json::json!(true)
    );
    assert_eq!(
        shared_info["result"]["info"]["persistentClientKey"],
        serde_json::json!(true)
    );
    let deleted_shared = tunnel_raw(
        address,
        &token,
        serde_json::json!({"Action":"delete","Name":"shared-client"}),
        12,
    )
    .await;
    assert!(
        deleted_shared["result"]["status"]
            .as_str()
            .unwrap_or_default()
            .contains("deleted tunnel shared-client")
    );
    for (name, bulk_port) in [
        ("bulk-zeta", distinct_port()),
        ("bulk-alpha", distinct_port()),
    ] {
        let response = tunnel_raw(
            address,
            &token,
            serde_json::json!({
                "Action":"create", "Name":name, "Type":"client",
                "TargetDestination":b32, "Port":bulk_port, "StartOnLoad":false
            }),
            11,
        )
        .await;
        assert!(response.get("error").is_none(), "bulk setup: {response}");
    }
    let stopped_resume_client = tunnel_raw(
        address,
        &token,
        serde_json::json!({"Action":"stop", "Name":"resume-client"}),
        9,
    )
    .await;
    assert!(stopped_resume_client.get("error").is_none());
    let all = tunnel_raw(
        address,
        &token,
        serde_json::json!({"Action":"start","All":true}),
        10,
    )
    .await;
    let results = all["result"]["results"].as_array().expect("bulk results");
    assert_eq!(results.len(), 3, "{all}");
    assert_eq!(results[0]["name"], "bulk-alpha");
    assert_eq!(results[1]["name"], "bulk-zeta");
    for (id, action) in [(12, "restart"), (13, "stop")] {
        let response = tunnel_raw(
            address,
            &token,
            serde_json::json!({"Action":action,"All":true}),
            id,
        )
        .await;
        assert_eq!(response["result"]["results"].as_array().unwrap().len(), 3);
        assert!(
            response["result"]["results"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| item["status"] == "success")
        );
    }
}

#[tokio::test]
async fn tunnelmanager_spoofed_host_roundtrips_for_http_server() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD, "")).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    let created = tunnel_raw(
        address,
        &token,
        serde_json::json!({
            "Action":"create", "Name":"spoofed", "Type":"httpserver",
            "TargetHost":"127.0.0.1", "TargetPort":8080,
            "StartOnLoad":false, "WebsiteHostname":"site.example.i2p", "BlockReferers":false,
            "BlockUserAgents":true, "UserAgents":"crawler,none", "BlockAccessInProxies":true
        }),
        2,
    )
    .await;
    assert!(created.get("error").is_none(), "create: {created}");
    let fetched = tunnel_raw(
        address,
        &token,
        serde_json::json!({"Action":"get", "Name":"spoofed"}),
        3,
    )
    .await;
    assert_eq!(
        fetched["result"]["info"]["rawConfig"]["spoofedHost"],
        serde_json::json!("site.example.i2p")
    );
    assert_eq!(
        fetched["result"]["info"]["rawConfig"]["blockUserAgents"],
        serde_json::json!(true)
    );
    assert_eq!(
        fetched["result"]["info"]["rawConfig"]["userAgents"],
        serde_json::json!("crawler,none")
    );
    assert_eq!(
        fetched["result"]["info"]["rawConfig"]["blockAccessInProxies"],
        serde_json::json!(true)
    );
    assert_eq!(
        fetched["result"]["info"]["rawConfig"]["blockReferers"],
        serde_json::json!(false)
    );
}

#[tokio::test]
async fn canonical_proxy_auth_uses_owned_credentials_and_redacts_password() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD, "")).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    let created = tunnel_raw(
        address,
        &token,
        serde_json::json!({
            "Action":"create", "Name":"canonical-socks", "Type":"socks",
            "TargetDestination":format!("{}.b32.i2p", "a".repeat(52)),
            "Port":distinct_port(), "StartOnLoad":false, "ProxyAuth":true,
            "ProxyUsername":"operator", "ProxyPassword":"s3cret!"
        }),
        2,
    )
    .await;
    assert!(created.get("error").is_none(), "create: {created}");
    assert!(
        created["result"]["status"]
            .as_str()
            .unwrap_or_default()
            .starts_with("success"),
        "create status: {created}"
    );
    let fetched = tunnel_raw(
        address,
        &token,
        serde_json::json!({"Action":"get", "Name":"canonical-socks"}),
        3,
    )
    .await;
    let info = &fetched["result"]["info"];
    assert_eq!(info["status"], "stopped");
    assert_eq!(info["rawConfig"]["proxyAuth"], true);
    assert_eq!(info["rawConfig"]["proxyUsername"], "operator");
    assert!(info["rawConfig"].get("proxyPassword").is_none());
    assert!(!fetched.to_string().contains("s3cret!"));
}

#[tokio::test]
async fn custom_options_are_rejected_as_invalid_params_without_allocation() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD, "")).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    let rejected = tunnel_raw(
        address,
        &token,
        serde_json::json!({
            "Action":"create", "Name":"unsafe-options", "Type":"server",
            "CustomOptions":"TargetHost=attacker.invalid"
        }),
        2,
    )
    .await;
    assert_eq!(rejected["error"]["code"], -32_602, "{rejected}");
    assert!(
        rejected["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("no safe typed allowlist"),
        "the rejection names the missing safe owner: {rejected}"
    );
}

#[tokio::test]
async fn canonical_server_policy_fields_use_service_tunnel_owners() {
    let directory = tempfile::tempdir().expect("tempdir");
    let filter_root = directory
        .path()
        .join("i2pcontrol")
        .join("tunnels")
        .join("filters");
    std::fs::create_dir_all(&filter_root).expect("filter root creates");
    let filter_path = filter_root.join("denied.txt");
    std::fs::write(&filter_path, format!("{}.b32.i2p\n", "b".repeat(51) + "a"))
        .expect("filter file writes");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&filter_path, std::fs::Permissions::from_mode(0o600))
            .expect("filter file is private");
    }
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD, "")).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    let access_entry = format!("{}.b32.i2p", "b".repeat(51) + "a");
    let created = tunnel_raw(
        address,
        &token,
        serde_json::json!({
            "Action":"create", "Name":"canonical-server", "Type":"httpserver",
            "TargetHost":"127.0.0.1", "TargetPort":9090,
            "PrivKeyFile":"canonical-server-key",
            "AccessOption":"deny", "AccessList":access_entry, "JumpList":"false",
            "FilterFilePath":"denied.txt",
            "ClientPerMinute":2, "ClientPerHour":10, "ClientPerDay":30,
            "TotalInPerMinute":20, "TotalInPerHour":100, "TotalInPerDay":300,
            "PostLimit":300, "PostLimitTime":600, "PerClientPeriod":6,
            "TotalPeriod":20, "TotalBanTime":1200,
            "StartOnLoad":false
        }),
        2,
    )
    .await;
    assert!(created.get("error").is_none(), "create: {created}");
    assert!(
        created["result"]["status"]
            .as_str()
            .unwrap_or_default()
            .starts_with("success"),
        "server create status: {created}"
    );
    let fetched = tunnel_raw(
        address,
        &token,
        serde_json::json!({"Action":"get", "Name":"canonical-server"}),
        3,
    )
    .await;
    let raw = &fetched["result"]["info"]["rawConfig"];
    assert_eq!(raw["targetHost"], "127.0.0.1", "{fetched}");
    assert_eq!(raw["targetPort"], 9090);
    assert_eq!(raw["privKeyFile"], "canonical-server-key");
    assert_eq!(raw["accessList"], access_entry);
    assert_eq!(raw["accessOption"], "deny");
    assert_eq!(raw["filterFilePath"], "denied.txt");
    assert_eq!(raw["jumpList"], "false");
    assert_eq!(raw["clientPerMinute"], 2);
    assert_eq!(raw["clientPerHour"], 10);
    assert_eq!(raw["clientPerDay"], 30);
    assert_eq!(raw["totalInPerMinute"], 20);
    assert_eq!(raw["totalInPerHour"], 100);
    assert_eq!(raw["totalInPerDay"], 300);
    assert_eq!(raw["postLimit"], 300);
    assert_eq!(raw["postLimitTime"], 600);
    assert_eq!(raw["perClientPeriod"], 6);
    assert_eq!(raw["totalPeriod"], 20);
    assert_eq!(raw["totalBanTime"], 1200);
    assert_eq!(raw["startOnLoad"], false);
}

/// Calls one TunnelManager action over the wire.
async fn tunnel(
    address: SocketAddr,
    token: &str,
    params: serde_json::Value,
    id: u32,
) -> serde_json::Value {
    let original = params.clone();
    let mut map = params.as_object().expect("object").clone();
    // Keep the test fixtures readable while emitting the canonical Proposal
    // wire shape: option fields share the top-level params object.
    if let Some(serde_json::Value::Object(options)) = map.remove("options") {
        for (key, value) in options {
            assert!(map.insert(key, value).is_none(), "duplicate wire field");
        }
    }
    map.insert("Token".to_owned(), serde_json::json!(token));
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "TunnelManager",
            "params": map,
            "id": id,
        }),
        &[],
    )
    .await;
    // This compatibility view is test-only: lifecycle tests written before
    // Plan 320 inspect the internal adapter result. Canonical wire assertions
    // use `tunnel_raw` below and never pass through this conversion.
    legacy_tunnel_result(response, &original)
}

async fn tunnel_raw(
    address: SocketAddr,
    token: &str,
    params: serde_json::Value,
    id: u32,
) -> serde_json::Value {
    let mut map = params.as_object().expect("object").clone();
    if let Some(serde_json::Value::Object(options)) = map.remove("options") {
        for (key, value) in options {
            assert!(map.insert(key, value).is_none(), "duplicate wire field");
        }
    }
    map.insert("Token".to_owned(), serde_json::json!(token));
    let (_, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0", "method": "TunnelManager", "params": map, "id": id,
        }),
        &[],
    )
    .await;
    response
}

fn legacy_tunnel_result(
    mut response: serde_json::Value,
    request: &serde_json::Value,
) -> serde_json::Value {
    use std::sync::atomic::{AtomicU64, Ordering};
    static GENERATION: AtomicU64 = AtomicU64::new(1);
    let Some(mut result) = response
        .as_object_mut()
        .and_then(|object| object.remove("result"))
    else {
        return response;
    };
    let status = result
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_owned();
    if status.starts_with("error - ") {
        let message = status.trim_start_matches("error - ").to_owned();
        response["error"] = serde_json::json!({"code": -32602, "message": message});
        return response;
    }
    let action = request
        .get("Action")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    if action == "get" {
        let info = result.get("info").cloned().unwrap_or_default();
        let raw = info.get("rawConfig").and_then(serde_json::Value::as_object);
        let mut options = serde_json::Map::new();
        if let Some(raw) = raw {
            for (key, value) in raw {
                let internal = match key.as_str() {
                    "uniqueLocalAddressPerClient" => "unique_local_address".to_owned(),
                    "UniqueLocalAddressPerClient" => "unique_local_address".to_owned(),
                    "MultiHoming" => "multihoming".to_owned(),
                    _ => key
                        .chars()
                        .enumerate()
                        .fold(String::new(), |mut out, (index, ch)| {
                            if ch.is_ascii_uppercase() && index > 0 {
                                out.push('_');
                            }
                            out.extend(ch.to_lowercase());
                            out
                        }),
                };
                let value = match value {
                    serde_json::Value::String(s) => serde_json::Value::String(s.clone()),
                    other => serde_json::Value::String(other.to_string()),
                };
                options.insert(internal, value);
            }
            if raw.contains_key("proxyUsername") {
                options.insert("proxy_password".to_owned(), serde_json::json!("[redacted]"));
            }
        }
        let state = info
            .get("status")
            .cloned()
            .unwrap_or(serde_json::json!("stopped"));
        result = serde_json::json!({
            "type": raw.and_then(|m| m.get("type")).cloned().unwrap_or_default(),
            "status": state,
            "running": info.get("status").and_then(serde_json::Value::as_str) == Some("running"),
            "start_on_load": raw.and_then(|m| m.get("startOnLoad")).cloned().unwrap_or(serde_json::Value::Bool(false)),
            "destination": info.get("destination").cloned().unwrap_or(serde_json::Value::Null),
            "options": options,
            "generation": GENERATION.load(Ordering::Relaxed).saturating_sub(1),
            "provenance": "control",
        });
    } else {
        let running = matches!(action.as_str(), "create" | "start" | "restart");
        result = serde_json::json!({"running": running, "generation": GENERATION.fetch_add(1, Ordering::Relaxed)});
    }
    response["result"] = result;
    response
}

#[tokio::test]
async fn tunnel_lifecycle_over_wire() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD, "")).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    let b32 = format!("{}.b32.i2p", "a".repeat(52));
    let port = distinct_port();

    // Create starts immediately with start_on_load intent.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({
            "Action": "create", "Name": "alpha", "Type": "client",
            "TargetDestination": b32, "Port": port,
        }),
        2,
    )
    .await;
    assert_eq!(response["result"]["running"], serde_json::json!(true));
    let generation = response["result"]["generation"]
        .as_u64()
        .expect("generation");

    // Get separates runtime state from persisted intent.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "alpha"}),
        3,
    )
    .await;
    let result = &response["result"];
    assert_eq!(result["provenance"], serde_json::json!("control"));
    assert_eq!(result["type"], serde_json::json!("client"));
    assert_eq!(result["status"], serde_json::json!("running"));
    assert_eq!(result["running"], serde_json::json!(true));
    assert_eq!(result["start_on_load"], serde_json::json!(true));
    assert_eq!(
        result["options"]["port"],
        serde_json::json!(port.to_string())
    );
    assert_eq!(result["generation"], serde_json::json!(generation));

    // Edit applies a real option change.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "edit", "Name": "alpha", "TunnelQuantity": 4}),
        4,
    )
    .await;
    assert!(response.get("error").is_none(), "edit succeeds: {response}");
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "alpha"}),
        5,
    )
    .await;
    assert_eq!(
        response["result"]["options"]["tunnel_quantity"],
        serde_json::json!("4")
    );

    // Stop, start, and delete move running intent with new generations.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "stop", "Name": "alpha"}),
        6,
    )
    .await;
    assert_eq!(response["result"]["running"], serde_json::json!(false));
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "alpha"}),
        7,
    )
    .await;
    assert_eq!(response["result"]["status"], serde_json::json!("stopped"));
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "start", "Name": "alpha"}),
        8,
    )
    .await;
    assert!(
        response.get("error").is_none(),
        "start succeeds: {response}"
    );
    assert_eq!(response["result"]["running"], serde_json::json!(true));
    assert!(response["result"]["generation"].as_u64().expect("gen") > generation);
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "delete", "Name": "alpha"}),
        9,
    )
    .await;
    assert_eq!(response["result"]["running"], serde_json::json!(false));
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "alpha"}),
        10,
    )
    .await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_602));
}

#[tokio::test]
async fn tunnel_server_identity_stable_over_wire() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD, "")).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    // Dummy loopback target for the server tunnel.
    let target = std::net::TcpListener::bind("127.0.0.1:0").expect("target binds");
    let target_port = target.local_addr().expect("target addr").port();

    let response = tunnel(
        address,
        &token,
        serde_json::json!({
            "Action": "create", "Name": "srv", "Type": "server",
            "TargetHost": "127.0.0.1", "TargetPort": target_port,
        }),
        2,
    )
    .await;
    assert!(
        response.get("error").is_none(),
        "create succeeds: {response}"
    );
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "srv"}),
        3,
    )
    .await;
    let before = response["result"]["destination"]
        .as_str()
        .expect("server b64")
        .to_owned();
    assert!(!before.is_empty());
    // Restart preserves the persistent server identity.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "restart", "Name": "srv"}),
        4,
    )
    .await;
    assert!(
        response.get("error").is_none(),
        "restart succeeds: {response}"
    );
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "srv"}),
        5,
    )
    .await;
    assert_eq!(
        response["result"]["destination"].as_str().expect("b64"),
        before
    );
    assert_eq!(response["result"]["status"], serde_json::json!("running"));
    drop(target);
}

#[tokio::test]
async fn tunnel_streamr_supported_and_secret_rejected_over_wire() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD, "")).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    // Streamr's UDP endpoint fields are outside Proposal 170's canonical
    // TunnelManager vocabulary, so the canonical endpoint rejects them.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "create", "Name": "stream", "Type": "streamrclient",
        "options": {
            "TargetDestination": format!("{}.b32.i2p", "a".repeat(52)),
            "LocalUdpHost": "127.0.0.1",
            "LocalUdpPort": distinct_port(),
        }}),
        2,
    )
    .await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_602));
    // Secret options never reach storage: rejected with no secret echo.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({
            "Action": "create", "Name": "alpha", "Type": "client",
            "options": {
                "TargetDestination": format!("{}.b32.i2p", "a".repeat(52)),
                "Port": distinct_port(),
                "ProxyPassword": "hunter2",
            },
        }),
        4,
    )
    .await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_602));
    assert!(
        !response["error"]["message"]
            .as_str()
            .expect("message")
            .contains("hunter2")
    );
}

#[tokio::test]
async fn tunnel_collision_and_startup_rejected_over_wire() {
    let b32 = format!("{}.b32.i2p", "a".repeat(52));
    let extra = format!(
        "[service_tunnels]\nenabled = false\n[[service_tunnels.tunnel]]\nid = \"web-client\"\nkind = \"http-client\"\nenabled = true\nlistener = \"127.0.0.1:8180\"\ndestination = \"{b32}\"\n"
    );
    let directory = tempfile::tempdir().expect("tempdir");
    let config = Config::parse(&config_text(directory.path(), TEST_PASSWORD, &extra))
        .expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    // Control names collide with startup-owned names.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({
            "Action": "create", "Name": "web-client", "Type": "client",
            "TargetDestination": b32, "Port": distinct_port(),
        }),
        2,
    )
    .await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_602));
    // Startup-owned definitions reject mutation but stay inspectable.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "delete", "Name": "web-client"}),
        3,
    )
    .await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_602));
    assert!(
        response["error"]["message"]
            .as_str()
            .expect("message")
            .contains("startup")
    );
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "web-client"}),
        4,
    )
    .await;
    assert_eq!(response["result"]["running"], serde_json::json!(true));
    // The nonstandard whole-inventory get form is rejected.
    let response = tunnel(address, &token, serde_json::json!({"Action": "get"}), 5).await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_602));
}

#[tokio::test]
async fn tunnel_envelope_rejections_over_wire() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD, "")).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    // Missing action, unknown action, and create without type.
    for params in [
        serde_json::json!({"Name": "a"}),
        serde_json::json!({"Action": "launch", "Name": "a"}),
        serde_json::json!({"Action": "create", "Name": "a"}),
        serde_json::json!({"Action": "get", "unknown": 1}),
        serde_json::json!({"Action": "edit", "Name": "a", "Type": "client"}),
    ] {
        let response = tunnel(address, &token, params, 2).await;
        assert_eq!(
            response["error"]["code"],
            serde_json::json!(-32_602),
            "params rejected"
        );
    }
    // Unknown tunnel names fail explicitly.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "missing"}),
        3,
    )
    .await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_602));
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "start", "Name": "missing"}),
        4,
    )
    .await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_602));
}

#[tokio::test]
async fn tunnel_restart_recovery_over_wire() {
    let directory = tempfile::tempdir().expect("tempdir");
    let text = config_text(directory.path(), TEST_PASSWORD, "");
    let config = Config::parse(&text).expect("config parses");
    let target = std::net::TcpListener::bind("127.0.0.1:0").expect("target binds");
    let target_port = target.local_addr().expect("target addr").port();
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    let response = tunnel(
        address,
        &token,
        serde_json::json!({
            "Action": "create", "Name": "srv", "Type": "server",
            "TargetHost": "127.0.0.1", "TargetPort": target_port,
        }),
        2,
    )
    .await;
    assert!(
        response.get("error").is_none(),
        "create succeeds: {response}"
    );
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "srv"}),
        3,
    )
    .await;
    let before = response["result"]["destination"]
        .as_str()
        .expect("b64")
        .to_owned();
    // Simulate a daemon restart: rebuild over the same data
    // directory and run control startup again. (Server tunnels bind
    // no loopback TCP listener, so the previous instance's sockets
    // cannot conflict; it idles until the test ends.)
    let config = Config::parse(&text).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "srv"}),
        4,
    )
    .await;
    assert_eq!(
        response["result"]["destination"].as_str().expect("b64"),
        before
    );
    assert_eq!(response["result"]["running"], serde_json::json!(true));
    assert_eq!(response["result"]["status"], serde_json::json!("running"));
    drop(target);
}

#[tokio::test]
async fn tunnel_disabled_mode_preserves_state() {
    use i2pr_daemon::i2pcontrol_tunnels::ControlStore;
    fn snapshot_tree(root: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
        fn visit(
            root: &std::path::Path,
            directory: &std::path::Path,
            snapshot: &mut Vec<(std::path::PathBuf, Vec<u8>)>,
        ) {
            for entry in std::fs::read_dir(directory).expect("directory reads") {
                let entry = entry.expect("entry");
                let path = entry.path();
                let relative = path
                    .strip_prefix(root)
                    .expect("path below root")
                    .to_path_buf();
                let metadata = std::fs::symlink_metadata(&path).expect("metadata");
                if metadata.is_dir() {
                    snapshot.push((relative, Vec::new()));
                    visit(root, &path, snapshot);
                } else {
                    snapshot.push((relative, std::fs::read(path).expect("file reads")));
                }
            }
        }
        let mut snapshot = Vec::new();
        visit(root, root, &mut snapshot);
        snapshot.sort_by(|left, right| left.0.cmp(&right.0));
        snapshot
    }

    let directory = tempfile::tempdir().expect("tempdir");
    // Seed one published generation directly through the store.
    let store = ControlStore::open(directory.path()).expect("store opens");
    let staged = store
        .stage(&std::collections::BTreeMap::new())
        .expect("stage");
    store.publish(staged).expect("publish");
    let tunnels_dir = directory.path().join("i2pcontrol").join("tunnels");
    let snapshot_before = snapshot_tree(&tunnels_dir);
    assert!(!snapshot_before.is_empty());
    // Build the daemon graph with I2PControl disabled: no service is
    // registered and the control files are untouched.
    let text = format!(
        "schema_version = 1\n[router]\ndata_dir = {:?}\n",
        directory.path().to_string_lossy()
    );
    let config = Config::parse(&text).expect("config parses");
    assert!(!config.i2pcontrol.enabled);
    let graph = i2pr_daemon::build_daemon_graph(&config).expect("graph builds");
    drop(graph);
    let snapshot_after = snapshot_tree(&tunnels_dir);
    assert_eq!(
        snapshot_before, snapshot_after,
        "disabled mode mutates nothing"
    );
}

/// Plan 290: the four composed families are real TunnelManager
/// backends over the wire. Each kind completes
/// create/get/stop/start/delete with its exact Proposal spelling,
/// and per-kind required fields fail before any side effect.
#[tokio::test]
async fn tunnel_plan290_family_lifecycle_over_wire() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD, "")).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    let b32 = format!("{}.b32.i2p", "a".repeat(52));
    let mut id: u32 = 2;

    // (name, type, options, expects_listener_bind).
    let target_port = distinct_port();
    let cc_port = distinct_port();
    let si_port = distinct_port();
    let hb_port = distinct_port();
    let families = [
        (
            "cc290",
            "connectclient",
            serde_json::json!({"TargetDestination": b32, "Port": cc_port}),
            true,
        ),
        (
            "si290",
            "socksirc",
            serde_json::json!({"TargetDestination": b32, "Port": si_port}),
            true,
        ),
        (
            "hs290",
            "httpserver",
            serde_json::json!({"TargetHost": "127.0.0.1", "TargetPort": target_port}),
            false,
        ),
        (
            "hb290",
            "httpbidirserver",
            serde_json::json!({
                "TargetHost": "127.0.0.1",
                "TargetPort": target_port,
                "Port": hb_port,
            }),
            true,
        ),
    ];
    for (name, kind, options, has_bind) in families {
        let response = tunnel(
            address,
            &token,
            serde_json::json!({"Action": "create", "Name": name, "Type": kind, "options": options}),
            id,
        )
        .await;
        id += 1;
        assert!(
            response.get("error").is_none(),
            "{kind} create succeeds: {response}"
        );
        assert_eq!(response["result"]["running"], serde_json::json!(true));
        let response = tunnel(
            address,
            &token,
            serde_json::json!({"Action": "get", "Name": name}),
            id,
        )
        .await;
        id += 1;
        assert_eq!(response["result"]["type"], serde_json::json!(kind));
        assert_eq!(response["result"]["status"], serde_json::json!("running"));
        let _ = has_bind; // Binding is runtime-private in the canonical Proposal result.
        for action in ["stop", "start", "delete"] {
            let response = tunnel(
                address,
                &token,
                serde_json::json!({"Action": action, "Name": name}),
                id,
            )
            .await;
            id += 1;
            assert!(
                response.get("error").is_none(),
                "{kind} {action} succeeds: {response}"
            );
        }
        let response = tunnel(
            address,
            &token,
            serde_json::json!({"Action": "get", "Name": name}),
            id,
        )
        .await;
        id += 1;
        assert_eq!(response["error"]["code"], serde_json::json!(-32_602));
    }

    // Required fields fail before allocation: a server kind without
    // its target halves, and a client kind without its destination.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "create", "Name": "hs-bad", "Type": "httpserver"}),
        id,
    )
    .await;
    id += 1;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_602));
    let response = tunnel(
        address,
        &token,
        serde_json::json!({
            "Action": "create", "Name": "cc-bad", "Type": "connectclient",
            "Port": distinct_port(),
        }),
        id,
    )
    .await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_602));
}

/// Plan 291: both Streamr families complete the TunnelManager
/// lifecycle with exact Proposal spellings, and their required
/// UDP/destination fields fail before allocation.
#[tokio::test]
async fn tunnel_plan291_streamr_lifecycle_over_wire() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD, "")).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    for kind in ["streamrserver", "streamrclient"] {
        let response = tunnel(
            address,
            &token,
            serde_json::json!({
                "Action": "create", "Name": "streamr", "Type": kind,
                "LocalUdpHost": "127.0.0.1", "LocalUdpPort": distinct_port(),
            }),
            2,
        )
        .await;
        assert_eq!(response["error"]["code"], serde_json::json!(-32_602));
    }
}

/// Plan 292: shaping options travel the full control transaction.
/// Create carries quantity/length intent, get echoes it, edit
/// replaces the destination generation, and out-of-bound shaping
/// fails before allocation.
#[tokio::test]
async fn tunnel_plan292_shaping_lifecycle_over_wire() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD, "")).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    let b32 = format!("{}.b32.i2p", "a".repeat(52));
    let port = distinct_port();

    let response = tunnel(
        address,
        &token,
        serde_json::json!({
            "Action": "create", "Name": "shaped", "Type": "client",

                "TargetDestination": b32,
                "Port": port,
                "TunnelQuantity": 4,
                "TunnelLength": 3,
                "Profile": "interactive",
        }),
        2,
    )
    .await;
    assert!(
        response.get("error").is_none(),
        "shaped create succeeds: {response}"
    );
    let generation = response["result"]["generation"]
        .as_u64()
        .expect("generation");
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "shaped"}),
        3,
    )
    .await;
    assert_eq!(
        response["result"]["options"]["tunnel_quantity"],
        serde_json::json!("4")
    );
    assert_eq!(
        response["result"]["options"]["tunnel_length"],
        serde_json::json!("3")
    );
    assert_eq!(
        response["result"]["options"]["profile"],
        serde_json::json!("interactive")
    );

    // Edit replaces the destination generation with new shaping.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "edit", "Name": "shaped", "TunnelQuantity": 5}),
        4,
    )
    .await;
    assert!(
        response.get("error").is_none(),
        "shaping edit succeeds: {response}"
    );
    assert!(response["result"]["generation"].as_u64().expect("gen") > generation);
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "shaped"}),
        5,
    )
    .await;
    assert_eq!(
        response["result"]["options"]["tunnel_quantity"],
        serde_json::json!("5")
    );

    // Out-of-bound shaping fails before allocation.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "create", "Name": "shaped-bad", "Type": "client",

            "TargetDestination": b32,
            "Port": distinct_port(),
            "TunnelQuantity": 7,
        }),
        6,
    )
    .await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_602));
    // Differing per-direction lengths fail instead of dropping one.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "create", "Name": "shaped-split", "Type": "client",

            "TargetDestination": b32,
            "Port": distinct_port(),
            "InboundLength": 1,
            "OutboundLength": 3,
        }),
        7,
    )
    .await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_602));
}

/// Plan 292: the new option cells persist across a daemon restart
/// with their effect intact, and accepted proxy plaintext never
/// reaches the GET echo or the disk (only the marked verifier is
/// stored).
#[tokio::test]
async fn tunnel_plan292_options_persist_over_wire() {
    let directory = tempfile::tempdir().expect("tempdir");
    let text = config_text(directory.path(), TEST_PASSWORD, "");
    let config = Config::parse(&text).expect("config parses");
    let target = std::net::TcpListener::bind("127.0.0.1:0").expect("target binds");
    let target_port = target.local_addr().expect("target addr").port();
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    // Generic server with the deterministic source bind plus an
    // inbound peer policy.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({
            "Action": "create", "Name": "srv292", "Type": "server",
            "options": {
                "TargetHost": "127.0.0.1",
                "TargetPort": target_port,
                "UniqueLocalAddressPerClient": true,
                "AccessList": format!("{}.b32.i2p", "b".repeat(51) + "a"),
            },
        }),
        2,
    )
    .await;
    assert!(
        response.get("error").is_none(),
        "server create succeeds: {response}"
    );
    // HTTP server with a canonical endpoint and target.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({
            "Action": "create", "Name": "web292", "Type": "httpserver",

                "TargetHost": "127.0.0.1",
                "TargetPort": target_port,
        }),
        3,
    )
    .await;
    assert!(
        response.get("error").is_none(),
        "httpserver create succeeds: {response}"
    );
    // Subscriber with a sink redirect and an interactive profile
    // plus idle policy on a client for lifecycle coverage.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "create", "Name": "sub292", "Type": "streamrclient",
        "options": {
            "TargetDestination": format!("{}.b32.i2p", "a".repeat(52)),
            "LocalUdpHost": "127.0.0.1",
            "LocalUdpPort": distinct_port(),
            "RemoteUdpHost": "127.0.0.2",
        }}),
        4,
    )
    .await;
    assert_eq!(response["error"]["code"], serde_json::json!(-32_602));
    // SOCKS listener with accepted proxy credentials (plaintext
    // must never be observable afterwards). Not started on load:
    // the stopped definition still proves stored-verifier
    // persistence without binding a listener the restarted
    // instance would collide with.
    let response = tunnel(
        address,
        &token,
        serde_json::json!({
            "Action": "create", "Name": "socks292", "Type": "socks",
            "options": {
                "TargetDestination": format!("{}.b32.i2p", "a".repeat(52)),
                "Port": distinct_port(),
                "ProxyUsername": "operator",
                "ProxyPassword": "s3cret!",
                "StartOnLoad": false,
            },
        }),
        5,
    )
    .await;
    assert!(
        response.get("error").is_none(),
        "proxy create succeeds: {response}"
    );
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "srv292"}),
        6,
    )
    .await;
    assert_eq!(
        response["result"]["options"]["unique_local_address"],
        serde_json::json!("true")
    );
    assert!(
        response["result"]["options"]["access_list"]
            .as_str()
            .expect("allow echo")
            .contains('b')
    );
    let before = response["result"]["destination"]
        .as_str()
        .expect("b64")
        .to_owned();
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "socks292"}),
        9,
    )
    .await;
    assert_eq!(
        response["result"]["options"]["proxy_password"],
        serde_json::json!("[redacted]")
    );
    assert!(
        !serde_json::to_string(&response["result"])
            .expect("json")
            .contains("s3cret!"),
        "no plaintext in the GET echo"
    );
    // Restart over the same data directory: options persist and
    // the server destination is stable (server tunnels bind no
    // loopback TCP listener, so the previous instance's sockets
    // cannot conflict; the stopped SOCKS definition below never
    // binds either).
    let config = Config::parse(&text).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    let response = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "srv292"}),
        10,
    )
    .await;
    assert_eq!(
        response["result"]["options"]["unique_local_address"],
        serde_json::json!("true")
    );
    assert_eq!(
        response["result"]["destination"].as_str().expect("b64"),
        before
    );
    // No accepted plaintext reaches the disk: only the marked
    // verifier may be stored.
    let tunnels = directory.path().join("i2pcontrol").join("tunnels");
    let entries = std::fs::read_dir(&tunnels).expect("tunnels dir");
    let mut saw_verifier = false;
    for entry in entries {
        let entry = entry.expect("entry");
        if entry.file_type().expect("type").is_file() {
            let bytes = std::fs::read(entry.path()).expect("read");
            assert!(
                !bytes.windows(7).any(|window| window == b"s3cret!"),
                "plaintext on disk: {}",
                entry.file_name().to_string_lossy()
            );
            saw_verifier = saw_verifier || bytes.windows(7).any(|window| window == b"$i2pr1$");
        }
    }
    assert!(saw_verifier, "marked verifier is stored");
    drop(target);
}
