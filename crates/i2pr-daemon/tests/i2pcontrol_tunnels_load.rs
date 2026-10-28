//! Plan 510: the load-sensitive black-box lane for the managed-tunnel
//! staging path.
//!
//! Plan 491's F4 recorded an intermittent `i2pcontrol_tunnels`
//! managed-tunnel staging failure — *"runtime transition failed:
//! manager configuration rejected"* — that never reproduced again, in
//! three isolated runs, in the `--no-fail-fast` workspace sweep, or in
//! two later floor runs. The residual risk was named as *"a
//! load-sensitive race in the managed-service staging path"*, and the
//! failure was undiagnosable because every manager rejection collapsed
//! into one static label.
//!
//! This lane is the plan's durable contribution: it drives the staging
//! path under **real** load — N concurrent managed-tunnel creates against
//! one real daemon, each generating a destination identity, building a
//! lease set and binding a loopback listener — and it drives the
//! **reject** path so the rejection is exercised rather than assumed.
//!
//! The lane is deterministic by construction:
//!
//! * the load is **real work**, never a sleep or a simulated delay;
//! * every wait is an **explicit bounded deadline**;
//! * the concurrency is **bounded**;
//! * every concurrent create owns a distinct name, a distinct loopback
//!   port and a distinct target destination, so a failure is
//!   attributable rather than a collision.
//!
//! The setup machinery is copied from `tests/i2pcontrol_tunnels.rs`
//! (Plan 289's black-box lane) so that lane stays byte-identical: this
//! file **adds** a lane, it does not change one.
//!
//! The test TLS client uses a test-only accept-any verifier (`NoVerify`)
//! confined to this file. Production source never calls `dangerous()`;
//! `scripts/check-runtime-boundaries.sh` rejects it outside tests.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use i2pr_daemon::config::Config;
use i2pr_daemon::i2pcontrol::I2pControlServiceState;
use i2pr_daemon::i2pcontrol_inspection::InspectionHandles;
use i2pr_daemon::i2pcontrol_tunnels::TunnelControlState;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Plan 342: the fail-closed outbound-credential owner for a row that does not
/// exercise a credential. No identity is loaded, so no store exists, and every
/// credential field is refused rather than silently accepted.
fn plan342_test_outbound_secrets()
-> Arc<dyn i2pr_service_tunnels::outbound_secret::RouterSecretOwner> {
    Arc::new(i2pr_service_tunnels::outbound_secret::NoOutboundSecrets)
}

/// Operator password for every black-box service in this file.
const TEST_PASSWORD: &str = "black-box-510-load-password";

/// Bounded client-side deadline for every wire operation.
///
/// The load lane's deadline is wider than Plan 289's: N concurrent creates
/// each perform real destination-identity and lease-set work, and the
/// deadline must bound the whole lane rather than punish one slow create.
/// It is still an explicit deadline — never a sleep.
const WIRE_TIMEOUT: Duration = Duration::from_secs(60);

/// The number of managed-tunnel creates driven **concurrently**.
///
/// Bounded on purpose: the lane measures the staging path under
/// contention, it does not try to exhaust the host. Eight concurrent
/// creates already serialize eight real staging transactions against one
/// shared manager.
const LOAD_CONCURRENCY: usize = 8;

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
///
/// The base band is deliberately different from Plan 289's lane
/// (`33_000 + pid % 5_000`): the two lanes run as separate test binaries
/// and may run concurrently, and a shared band would let one lane's
/// listener steal the other's port.
///
/// The band also sits **below** the operating system's ephemeral range
/// (`32768–60999` on Linux). A fixed port inside the ephemeral range can
/// be handed out to an unrelated outbound connection on the same host,
/// and the control's `await_port_release` would then wait up to two
/// seconds for it before the staging binds. That wait is bounded, so the
/// lane still passes — but it makes the lane's *duration* depend on
/// unrelated host traffic. Keeping the band below the ephemeral range
/// removes the dependency.
fn distinct_port() -> u16 {
    static NEXT: AtomicU16 = AtomicU16::new(0);
    static BASE: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
    let base = *BASE.get_or_init(|| 20_000 + (std::process::id() % 5_000) as u16);
    base + NEXT.fetch_add(1, Ordering::Relaxed) * 37
}

/// A distinct, syntactically valid base32 destination body.
///
/// The load lane gives every concurrent create its own target so that a
/// failure is attributable to the staging path rather than to two creates
/// sharing one destination.
///
/// The body is 52 characters of the canonical lower-case `a-z2-7`
/// alphabet, and the **last** character is pinned to `a`: 52 Base32
/// characters carry 260 bits, of which only 256 belong to a Destination,
/// so the trailing four bits must be zero or the label does not decode.
/// The distinctness lives in the first two characters.
fn target_body(index: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
    const LABEL_LEN: usize = 52;
    let mut body = String::with_capacity(LABEL_LEN);
    for position in 0..LABEL_LEN {
        let selector = if position == 0 {
            index % ALPHABET.len()
        } else if position == 1 {
            (index / ALPHABET.len()) % ALPHABET.len()
        } else if position + 1 == LABEL_LEN {
            // The trailing four bits must be zero.
            0
        } else {
            (index + position) % ALPHABET.len()
        };
        body.push(ALPHABET[selector] as char);
    }
    body
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
///
/// The token is service-wide and not connection-bound, so one token
/// serves the whole concurrent lane.
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

/// Calls one TunnelManager action over the wire, returning the raw
/// canonical Proposal envelope (no legacy conversion).
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

/// The `result.status` string of one TunnelManager reply.
fn status_of(response: &serde_json::Value) -> String {
    response["result"]["status"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// Plan 510: the managed-tunnel staging path survives **concurrent**
/// creates against one real daemon.
///
/// This is the lane Plan 491's F4 needed and did not have. Every create
/// is a real staging transaction — a destination identity is generated, a
/// lease set is built and validated, and a loopback listener is bound —
/// and all N of them are in flight at once against one shared manager.
///
/// The load is real work, never a sleep: there is no wall-clock delay
/// anywhere in this test, only explicit bounded deadlines.
#[tokio::test]
async fn concurrent_managed_tunnel_creates_all_succeed_under_load() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD, "")).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;

    // Every create owns a distinct name, a distinct loopback port and a
    // distinct target destination, so a failure is attributable to the
    // staging path rather than to a collision between two creates.
    let mut plans = Vec::with_capacity(LOAD_CONCURRENCY);
    for index in 0..LOAD_CONCURRENCY {
        plans.push((
            format!("load-{index}"),
            distinct_port(),
            format!("{}.b32.i2p", target_body(index)),
        ));
    }
    let names: Vec<String> = plans.iter().map(|(name, _, _)| name.clone()).collect();
    let ports: Vec<u16> = plans.iter().map(|(_, port, _)| *port).collect();
    let targets: Vec<String> = plans.iter().map(|(_, _, target)| target.clone()).collect();

    // Drive the creates concurrently: each opens its own TLS connection
    // and posts its own request, so all N are in flight at once.
    let mut handles = Vec::with_capacity(LOAD_CONCURRENCY);
    for (index, (name, port, target)) in plans.into_iter().enumerate() {
        let token = token.clone();
        handles.push(tokio::spawn(async move {
            let response = tunnel_raw(
                address,
                &token,
                serde_json::json!({
                    "Action": "create", "Name": name, "Type": "httpclient",
                    "TargetDestination": target, "Port": port,
                    "ReachableBy": "127.0.0.1",
                    "SigType": "EDDSA_SHA512_ED25519", "EncType": "4",
                }),
                u32::try_from(index).expect("id fits"),
            )
            .await;
            (name, port, target, response)
        }));
    }

    // Every create must succeed. A failure here is the lane's finding:
    // the staging path could not carry the concurrent load.
    for handle in handles {
        let (name, port, target, response) = handle.await.expect("create task joins");
        assert!(
            response.get("error").is_none(),
            "create {name} carried a JSON-RPC error: {response}"
        );
        let status = status_of(&response);
        assert!(
            status.starts_with("success - created tunnel"),
            "create {name} (port {port}, target {target}) did not succeed: {status}"
        );
    }

    // Every staged tunnel is present and running afterwards. A create
    // that reported success but left nothing behind would be a staging
    // defect the status string alone would hide.
    for (index, name) in names.iter().enumerate() {
        let fetched = tunnel_raw(
            address,
            &token,
            serde_json::json!({"Action": "get", "Name": name}),
            u32::try_from(index).expect("id fits") + 1_000,
        )
        .await;
        let info = &fetched["result"]["info"];
        assert_eq!(
            info["status"],
            serde_json::json!("running"),
            "staged tunnel {name} is not running: {fetched}"
        );
        assert_eq!(
            info["rawConfig"]["port"],
            serde_json::json!(ports[index]),
            "staged tunnel {name} carries the wrong port: {fetched}"
        );
        assert_eq!(
            info["targetDestination"],
            serde_json::json!(targets[index]),
            "staged tunnel {name} carries the wrong target: {fetched}"
        );
    }
}

/// Plan 510: a contradictory managed-tunnel configuration is rejected,
/// and the reason the daemon returns names a **class** and carries no
/// value.
///
/// The rejection is a *staging* rejection, not an envelope rejection:
/// `UseSSL` on a server kind with no daemon TLS policy is admitted by
/// the request envelope and by the control spec build, and is refused
/// later, while the shared manager stages the runtime. That is the
/// exact path Plan 491's F4 failed on, so the lane exercises it rather
/// than assuming it.
#[tokio::test]
async fn a_contradictory_configuration_is_rejected_with_a_value_free_reason() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD, "")).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;

    let name = "ssl-without-policy";
    let port = distinct_port();
    let rejected = tunnel_raw(
        address,
        &token,
        serde_json::json!({
            "Action": "create", "Name": name, "Type": "httpserver",
            "TargetHost": "127.0.0.1", "TargetPort": port,
            "UseSSL": true,
        }),
        2,
    )
    .await;

    // The control rejection rides in `result.status`, not in a JSON-RPC
    // error member: the TunnelManager envelope reports a rejected
    // lifecycle action as a status the caller must read.
    let status = status_of(&rejected);
    assert!(
        status.starts_with("error - "),
        "the contradictory configuration was not rejected: {status}"
    );
    assert!(
        status.contains("runtime transition failed: server TLS policy missing"),
        "the rejection did not name its class: {status}"
    );

    // The reason names a class and carries **no value**: no service id,
    // no port, no address, no destination, no detail.
    assert!(
        !status.contains(name),
        "the reason echoed the service id: {status}"
    );
    assert!(
        !status.contains(&port.to_string()),
        "the reason echoed the port: {status}"
    );
    assert!(
        !status.contains("127.0.0.1"),
        "the reason echoed an address: {status}"
    );
    assert!(
        !status.contains("use_ssl requires"),
        "the reason echoed the raw detail: {status}"
    );
}

/// Plan 510: a rejected staging transaction leaves the shared manager
/// usable.
///
/// The load lane's contribution is a path that carries concurrent
/// staging work. A rejection that poisoned the shared manager would
/// turn every later create into the same undiagnosable failure Plan 491
/// recorded, so the lane proves the path recovers: one rejected create,
/// then a successful one, on the same daemon.
#[tokio::test]
async fn the_staging_path_recovers_after_a_rejected_create() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD, "")).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;

    let rejected = tunnel_raw(
        address,
        &token,
        serde_json::json!({
            "Action": "create", "Name": "ssl-without-policy", "Type": "httpserver",
            "TargetHost": "127.0.0.1", "TargetPort": distinct_port(),
            "UseSSL": true,
        }),
        2,
    )
    .await;
    assert!(
        status_of(&rejected).starts_with("error - "),
        "the contradictory configuration was not rejected: {}",
        status_of(&rejected)
    );

    // The shared manager must still stage a runtime afterwards.
    let name = "after-rejection";
    let port = distinct_port();
    let target = format!("{}.b32.i2p", target_body(LOAD_CONCURRENCY));
    let created = tunnel_raw(
        address,
        &token,
        serde_json::json!({
            "Action": "create", "Name": name, "Type": "httpclient",
            "TargetDestination": target, "Port": port,
            "ReachableBy": "127.0.0.1",
            "SigType": "EDDSA_SHA512_ED25519", "EncType": "4",
        }),
        3,
    )
    .await;
    assert!(
        status_of(&created).starts_with("success - created tunnel"),
        "the staging path did not recover after a rejected create: {}",
        status_of(&created)
    );
}
