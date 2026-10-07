//! Plan 381 §WP1 — the **R-side** consumer service, created the way the live
//! ELS2 lane creates it.
//!
//! Plan 380 proved that a consumer credential can arrive over I2PControl and be
//! sealed. It did **not** prove that the shape the external lane needs is
//! constructible at all, and that shape has three constraints the lane cannot
//! choose its way around:
//!
//!   1. the target is a **`.b33`**, not a `.b32.i2p`;
//!   2. `i2pr-service-tunnels/src/config.rs:1160-1176` refuses a `.b33`
//!      service target unless `DelayOpen` is set, and the daemon's TOML path
//!      hardcodes `delay_open: false` (`i2pr-daemon/src/config.rs:2208`), so the
//!      **only** way to create this service is over I2PControl — this is Plan
//!      351 Gate 2 and it is unchanged;
//!   3. the `R` role is a **non-floodfill** identity, so the service must be
//!      constructible without a floodfill anywhere in the composition.
//!
//! Every row here goes through a real TLS listener and a real JSON-RPC
//! `TunnelManager` call. Nothing private is touched.
//!
//! **What these rows do not establish.** They are the R-side *shape*, not a
//! live fetch. No row here reaches the network, resolves a destination, or
//! moves an application payload; a row that did would need the mesh that
//! `tests/integration/els2/run-i2pd-els2.sh` provides, and that is WP2/WP3.
//! Nothing in this file is offered in place of Plan 374's rows.

#![forbid(unsafe_code)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::config::Config;
use i2pr_daemon::i2pcontrol::I2pControlServiceState;
use i2pr_daemon::i2pcontrol_inspection::InspectionHandles;
use i2pr_daemon::i2pcontrol_tunnels::TunnelControlState;
use i2pr_daemon::outbound_secret::RouterBoundOutboundSecrets;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::outbound_secret::RouterSecretOwner;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const TEST_PASSWORD: &str = "plan381-lane-consumer";

fn plan381_secret_owner() -> Arc<dyn RouterSecretOwner> {
    let mut rng = rand_core::OsRng;
    let identity = RouterIdentityBundle::generate(&mut rng).expect("identity bundle");
    Arc::new(RouterBoundOutboundSecrets::from_router_identity(&identity).expect("owner"))
}

/// A real `.b33.i2p` built the way a publisher builds one.
///
/// `authorized` selects between the two shapes the lane exercises: `true` is the
/// per-client-authorized b33 that requires a credential, `false` the
/// ordinary one that must keep working without one (Plan 380 invariant 1).
fn plan381_b33(authorized: bool) -> String {
    use i2pr_netdb::BlindingIdentity;
    use i2pr_proto::{B32_BLINDED_SIGTYPE, B32_UNBLINDED_SIGTYPE_RED25519, SigningKeyType};
    let mut rng = rand_core::OsRng;
    let private: i2pr_crypto::red25519::Red25519PrivateScalar =
        i2pr_crypto::red25519::generate_private(&mut rng).expect("key");
    let public = i2pr_crypto::red25519::derive_public_key(&private);
    let identity = BlindingIdentity::new(
        public,
        SigningKeyType::RedDsaSha512Ed25519,
        Some("plan381-wire-lookup-secret"),
    )
    .expect("blinding identity");
    let schedule =
        i2pr_netdb::BlindingSchedule::new(identity, i2pr_netdb::BlindingScheduleConfig::default());
    let publisher = i2pr_client::encrypted_leaseset::EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher.authorized_address(authorized).expect("address");
    assert_eq!(address.blinded_sigtype(), B32_BLINDED_SIGTYPE);
    assert_eq!(address.unblinded_sigtype(), B32_UNBLINDED_SIGTYPE_RED25519);
    let text = address.to_text().expect("address text");
    // The suffix is `.b32.i2p` even for a blinded address, and deliberately so:
    // i2pd emits exactly that too (`libi2pd/Blinding.h:28` reached from
    // `daemon/HTTPServer.cpp:494`, `blinded.ToB33() << ".b32.i2p"`). Which
    // kind an address *is* is decided by structure in
    // `i2pr-service-tunnels/src/destination.rs:112-114`, before the b32 branch,
    // so the suffix carries no kind information and asserting on it would be
    // asserting on cosmetics.
    assert!(
        text.ends_with(".b32.i2p"),
        "the lane's address keeps the reference's suffix convention: {text}"
    );
    assert_eq!(
        text.split('.').next().map(str::len),
        Some(56),
        "a b33 body is 56 base32 characters -- 3 header bytes plus a 32-byte \
         key is 35 bytes, and 35 is a multiple of 5, so base32 emits 56 with no \
         padding: {text}"
    );
    text
}

/// The PSK and DH credentials in the grammar `EncryptedTargetCredential::parse`
/// accepts: `psk:` / `dh:` plus 64 lowercase hex.
fn plan381_psk_credential() -> String {
    format!("psk:{}", "3a".repeat(32))
}

/// The DH credential as the control surface takes it.
///
/// Plan 380's file uses a fixed literal for the same reason this does: the row
/// is about the create *shape*, and any 32 bytes that satisfy
/// `EncryptedTargetCredential::parse` exercise it. Deriving a real keypair
/// here would be a statement about byte order that no row in this file can
/// check, because nothing here consumes the key.
fn plan381_dh_credential() -> String {
    format!("dh:{}", "5c".repeat(32))
}

#[derive(Debug)]
struct NoVerify;

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls_pki_types::CertificateDer<'_>,
        _intermediates: &[rustls_pki_types::CertificateDer<'_>],
        _server_name: &rustls_pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls_pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls_pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls_pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn config_text(data_dir: &std::path::Path, password: &str) -> String {
    format!(
        "schema_version = 1\n[router]\ndata_dir = {:?}\n[i2pcontrol]\nenabled = true\n\
         bind_address = \"127.0.0.1\"\nport = 0\npassword = {password:?}\n",
        data_dir.to_string_lossy()
    )
}

/// Starts the I2PControl service over loopback with a real secret owner.
///
/// There is no floodfill anywhere in this composition, which is the point of
/// row `plan381_the_lane_consumer_needs_no_floodfill`.
async fn start_service(config: &Config) -> (SocketAddr, ChildScope, CancellationToken) {
    let owner = plan381_secret_owner();
    let manager = i2pr_daemon::build_shared_service_manager(config)
        .expect("shared manager builds")
        .expect("control implies a shared manager");
    let control = Arc::new(
        TunnelControlState::for_config(config, manager, owner).expect("control builds for config"),
    );
    let inspection = Arc::new(InspectionHandles::from_config(config));
    let state = Arc::new(
        I2pControlServiceState::new_with_inspection(config.i2pcontrol.clone(), inspection)
            .expect("state builds"),
    );
    state.set_control_manager(Arc::clone(&control));
    let (listener, bound) = state
        .bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0))
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
    (bound, scope, parent)
}

async fn tls_connect(address: SocketAddr) -> tokio_rustls::client::TlsStream<TcpStream> {
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerify))
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let stream = TcpStream::connect(address).await.expect("tcp connect");
    connector
        .connect(
            rustls_pki_types::ServerName::try_from("localhost").expect("server name"),
            stream,
        )
        .await
        .expect("tls handshake")
}

async fn post_json(address: SocketAddr, request: &serde_json::Value) -> (u16, serde_json::Value) {
    let mut stream = tls_connect(address).await;
    let body = serde_json::to_vec(request).expect("request serializes");
    let head = format!(
        "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await.expect("head");
    stream.write_all(&body).await.expect("body");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("response");
    parse_response(&raw)
}

fn parse_response(raw: &[u8]) -> (u16, serde_json::Value) {
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("header terminator");
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .expect("status line");
    let body = &raw[split + 4..];
    let value = serde_json::from_slice(body).unwrap_or_else(|_| serde_json::json!({}));
    (status, value)
}

async fn authenticate(address: SocketAddr) -> String {
    let (status, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0", "method": "Authenticate",
            "params": {"API": 1, "Password": TEST_PASSWORD},
            "id": 1,
        }),
    )
    .await;
    assert_eq!(status, 200, "authenticate status");
    response["result"]["Token"]
        .as_str()
        .expect("token minted")
        .to_owned()
}

async fn tunnel(
    address: SocketAddr,
    token: &str,
    params: serde_json::Value,
    id: u32,
) -> serde_json::Value {
    let mut map = params.as_object().expect("object").clone();
    map.insert("Token".to_owned(), serde_json::json!(token));
    let (status, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0", "method": "TunnelManager", "params": map, "id": id,
        }),
    )
    .await;
    assert_eq!(status, 200, "tunnel status");
    response
}

/// The lane's consumer shape: a `.b33` target, `DelayOpen`, and an optional
/// credential delivered through i2pr's typed `CustomOptions` namespace.
fn lane_client_create(name: &str, target: &str, credential: Option<&str>) -> serde_json::Value {
    let mut params = serde_json::json!({
        "Action": "create", "Type": "client", "Name": name,
        "TargetDestination": target,
        "DelayOpen": true,
    });
    if let Some(credential) = credential {
        params["CustomOptions"] = serde_json::json!({
            "i2pr": {"LeasesetClientCredential": credential},
        });
    }
    params
}

fn ok(response: &serde_json::Value) -> bool {
    response.get("error").is_none()
        && response["result"]["status"]
            .as_str()
            .unwrap_or_default()
            .starts_with("success")
}

fn refused(response: &serde_json::Value) -> bool {
    !ok(response)
}

/// The lane's PSK consumer: the row that makes the external lane's PSK half
/// constructible before any mesh exists.
#[tokio::test(flavor = "current_thread")]
async fn plan381_the_lane_consumer_creates_with_a_psk_credential() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let (address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;

    let created = tunnel(
        address,
        &token,
        lane_client_create(
            "lane-psk",
            &plan381_b33(true),
            Some(&plan381_psk_credential()),
        ),
        2,
    )
    .await;
    assert!(
        ok(&created),
        "the lane's PSK consumer must create: {created}"
    );

    let got = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "lane-psk"}),
        3,
    )
    .await;
    assert!(ok(&got), "get: {got}");
    assert_eq!(
        got["result"]["info"]["lease_set_security"]["clientCredentialConfigured"], true,
        "the PSK consumer must report a configured credential"
    );
}

/// The same row for the DH half.
#[tokio::test(flavor = "current_thread")]
async fn plan381_the_lane_consumer_creates_with_a_dh_credential() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let (address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;

    let created = tunnel(
        address,
        &token,
        lane_client_create(
            "lane-dh",
            &plan381_b33(true),
            Some(&plan381_dh_credential()),
        ),
        2,
    )
    .await;
    assert!(
        ok(&created),
        "the lane's DH consumer must create: {created}"
    );

    let got = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "lane-dh"}),
        3,
    )
    .await;
    assert!(ok(&got), "get: {got}");
    assert_eq!(
        got["result"]["info"]["lease_set_security"]["clientCredentialConfigured"], true,
        "the DH consumer must report a configured credential"
    );
}

/// **Plan 380 invariant 1, on the lane's own shape.** `NONE` needs no credential
/// and must keep working. A change that made the credential mandatory would
/// break the lane's unauthenticated row while every credentialed row stayed
/// green, which is why this is pinned separately rather than folded in.
#[tokio::test(flavor = "current_thread")]
async fn plan381_the_lane_consumer_creates_with_no_credential_for_auth_none() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let (address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;

    let created = tunnel(
        address,
        &token,
        lane_client_create("lane-none", &plan381_b33(false), None),
        2,
    )
    .await;
    assert!(
        ok(&created),
        "an unauthorized .b33 consumer needs no credential: {created}"
    );

    let got = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "lane-none"}),
        3,
    )
    .await;
    assert!(ok(&got), "get: {got}");
    assert_eq!(
        got["result"]["info"]["lease_set_security"]["clientCredentialConfigured"], false,
        "an unauthorized consumer must not claim a credential"
    );
}

/// `DelayOpen` is not a nicety. `i2pr-service-tunnels/src/config.rs:1160-1176`
/// refuses a `.b33` service target without it, and the daemon's TOML path
/// hardcodes `delay_open: false`, so this row is what proves the lane cannot be
/// built any other way.
///
/// The row is written as a **toggle**, not as a single refusal, and that is
/// deliberate. The wire does not project the specific reason — the daemon
/// collapses `ContradictoryOptions` to `"spec options contradict the kind"` —
/// so an assertion on the message text would be asserting on a string the
/// control surface deliberately does not carry. Issuing the identical request
/// twice with only `DelayOpen` changed is what makes the refusal attributable
/// to the flag; a lone refusal would also pass if the request were malformed
/// for some unrelated reason.
#[tokio::test(flavor = "current_thread")]
async fn plan381_a_b33_consumer_is_refused_exactly_when_delay_open_is_absent() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let (address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    let target = plan381_b33(true);
    let credential = plan381_psk_credential();

    let with_delay_open = tunnel(
        address,
        &token,
        serde_json::json!({
            "Action": "create", "Type": "client", "Name": "lane-toggle-on",
            "TargetDestination": target,
            "DelayOpen": true,
            "CustomOptions": {"i2pr": {"LeasesetClientCredential": credential}},
        }),
        2,
    )
    .await;
    assert!(
        ok(&with_delay_open),
        "the identical request with DelayOpen must create: {with_delay_open}"
    );

    let without_delay_open = tunnel(
        address,
        &token,
        serde_json::json!({
            "Action": "create", "Type": "client", "Name": "lane-toggle-off",
            "TargetDestination": target,
            "DelayOpen": false,
            "CustomOptions": {"i2pr": {"LeasesetClientCredential": credential}},
        }),
        3,
    )
    .await;
    assert!(
        refused(&without_delay_open),
        "the same request without DelayOpen must be refused: {without_delay_open}"
    );
}

/// The `R` role is a non-floodfill identity. This composition contains no
/// floodfill service at all, so the row is really the statement that nothing on
/// the create path requires one — which WP2 depends on when it makes `R`
/// deliberately not a floodfill.
#[tokio::test(flavor = "current_thread")]
async fn plan381_the_lane_consumer_needs_no_floodfill() {
    let directory = tempfile::tempdir().expect("tempdir");
    // No [i2cp], no [sam], no floodfill: only I2PControl on loopback, which is
    // exactly the composition the lane's non-floodfill `R` role uses.
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    assert!(
        config.i2pcontrol.enabled,
        "the lane's control surface is on"
    );
    assert_eq!(
        config.i2pcontrol.bind_address.to_string(),
        "127.0.0.1",
        "and it is loopback-only"
    );

    let (address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;
    let created = tunnel(
        address,
        &token,
        lane_client_create(
            "lane-nofloodfill",
            &plan381_b33(true),
            Some(&plan381_psk_credential()),
        ),
        2,
    )
    .await;
    assert!(
        ok(&created),
        "a non-floodfill composition must create the consumer: {created}"
    );
}

/// A malformed credential is refused at create time, so the lane cannot be
/// brought up with a key i2pd will never accept.
#[tokio::test(flavor = "current_thread")]
async fn plan381_the_lane_consumer_refuses_a_malformed_credential() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let (address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;

    for (label, credential) in [
        ("unknown scheme", format!("pskk:{}", "3a".repeat(32))),
        ("short key", "psk:3a3a".to_owned()),
        ("uppercase hex", format!("psk:{}", "3A".repeat(32))),
        ("empty", "psk:".to_owned()),
    ] {
        let created = tunnel(
            address,
            &token,
            lane_client_create("lane-bad", &plan381_b33(true), Some(&credential)),
            2,
        )
        .await;
        assert!(refused(&created), "{label} must be refused: {created}");
    }
}
