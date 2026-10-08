//! Plan 381 §WP3 — the live **R-side** ELS2 driver against stock i2pd 2.61.0.
//!
//! WP1 proved the R-side *shape* over loopback I2PControl: a `.b33` target,
//! `DelayOpen`, and a credential arriving through the typed `CustomOptions`
//! namespace. WP2 proved the *reference* half: a stock i2pd client consumes a
//! stock i2pd publisher's blinded destination with a real payload crossing.
//! This driver joins the two, and it is the first row in Plan 381 that
//! involves **i2pr at all**.
//!
//! # Composition
//!
//! `ServiceProductSpec::reference` takes a [`ReferencePeer`] and, when set,
//! the product dials it, bootstraps the RouterInfo, and builds real tunnels.
//! The same `Arc<ServiceTunnelManager>` is then handed to both the product and
//! the I2PControl control state, so a service created over JSON-RPC lands in
//! the one manager that owns the runtime and is delivered by the same path as
//! a startup-configured one. That is the composition Plan 213 §C1 established
//! and this file reuses rather than re-derives.
//!
//! # What "public surfaces" means here, precisely
//!
//! The service is created by a real TLS listener carrying real JSON-RPC
//! `TunnelManager` calls, and the payload moves through a real local listener
//! and a real SSU2 session to a real reference. Nothing private is called and
//! no decoded LeaseSet is injected: the b33 goes in as a string and a
//! LeaseSet2 comes out of a network lookup.
//!
//! What is *not* claimed: the peer bootstrap and the router delivery backend
//! are attached by the controlled helper rather than by the shipped daemon,
//! because the shipped daemon has no configuration surface for learning one
//! named peer. A product-path lane would need that surface, which is outside
//! Plan 381's scope; it is recorded as a finding rather than assumed here.
//!
//! # Required environment
//!
//! - `I2PR_ELS2_REFERENCE_ROUTER_INFO` — the reference floodfill's `router.info`
//! - `I2PR_ELS2_REFERENCE_ENDPOINT` — `127.0.0.1:PORT`, its SSU2 endpoint
//! - `I2PR_ELS2_REFERENCE_DEST_B33_I2PD` — the publisher's blinded address in
//!   the spelling i2pd emits; i2pr decides blinded-ness by structure, so the
//!   `.b32.i2p` suffix is correct for both routers
//! - `I2PR_ELS2_SSU2_BIND` — a fixed loopback bind for R
//! - `I2PR_ELS2_EVIDENCE_DIR` — where the sanitized TSV is written
//!
//! Missing environment **fails**. Nothing here degrades into a skip.

#![forbid(unsafe_code)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::config::Config;
use i2pr_daemon::i2pcontrol::I2pControlServiceState;
use i2pr_daemon::i2pcontrol_inspection::InspectionHandles;
use i2pr_daemon::i2pcontrol_tunnels::TunnelControlState;
use i2pr_daemon::outbound_secret::RouterBoundOutboundSecrets;
use i2pr_daemon::service_product::{
    ReferencePeer, ServiceProduct, ServiceProductOptions, ServiceProductSpec,
};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::outbound_secret::RouterSecretOwner;
use i2pr_service_tunnels::{ServiceTunnelSet, StaticAliasTable};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const TEST_PASSWORD: &str = "plan381-els2-external";
const SERVICE_ID: &str = "plan381-els2-client";
/// The banner the reference's server tunnel terminates on. Seeing it proves
/// the bytes crossed I2P and reached the application, not merely that a
/// connection was accepted.
const EXPECTED_BANNER: &str = "ELS2-LANE-FIXTURE-OK";
// Plan 381: covers the 90s cold first-activation budget plus lookup,
// streaming handshake, and payload return with margin. Single attempt:
// no reconnect loop.
const READ_WINDOW_MS: u64 = 150_000;

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(
        std::env::var(name)
            .unwrap_or_else(|_| panic!("{name} is required; missing environment fails")),
    )
}

fn env_value(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is required; missing environment fails"))
}

fn append_evidence(dir: &std::path::Path, label: &str, value: &str) {
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("driver-evidence.tsv"))
        .expect("evidence file opens");
    writeln!(file, "{label}\t{value}").expect("evidence row writes");
}

fn secret_owner() -> Arc<dyn RouterSecretOwner> {
    let mut rng = rand_core::OsRng;
    let identity = RouterIdentityBundle::generate(&mut rng).expect("identity bundle");
    Arc::new(RouterBoundOutboundSecrets::from_router_identity(&identity).expect("owner"))
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
    let value = serde_json::from_slice(&raw[split + 4..]).unwrap_or_else(|_| serde_json::json!({}));
    (status, value)
}

async fn control(
    address: SocketAddr,
    method: &str,
    params: serde_json::Value,
    id: u32,
) -> serde_json::Value {
    let (status, response) = post_json(
        address,
        &serde_json::json!({"jsonrpc": "2.0", "method": method, "params": params, "id": id}),
    )
    .await;
    assert_eq!(status, 200, "{method} status");
    response
}

/// Reads the banner back through the service's own local listener.
///
/// A deadline-bounded search rather than a single `read_exact`: Streaming
/// delivers in packets whose boundaries are not the banner's, and a short read
/// would be a transport fact dressed up as a failure. The product's inbound
/// pump is selected alongside the read, because a control-created service is
/// delivered by the same poll loop that provisions it — reading without
/// pumping would wait on work this task is responsible for scheduling.
async fn read_banner(product: &mut ServiceProduct, port: u16) -> Option<String> {
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);
    let stream = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        TcpStream::connect(address),
    )
    .await
    .ok()?
    .ok()?;
    let (mut reader, mut writer) = stream.into_split();

    // Send something, so the row is a round trip rather than a receipt. The
    // reference fixture ignores input; the bytes exist to make the direction
    // real.
    let _ = writer
        .write_all(b"plan381-els2-ping\n")
        .await
        .map_err(|_| ())
        .ok();

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(READ_WINDOW_MS);
    let mut seen: Vec<u8> = Vec::new();
    while tokio::time::Instant::now() < deadline {
        let mut chunk = [0_u8; 1024];
        let read = tokio::select! {
            biased;
            result = reader.read(&mut chunk) => Some(result),
            _ = product.poll_inbound() => None,
        };
        let Some(read) = read else { continue };
        match read {
            Ok(n) if n > 0 => {
                seen.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&seen).into_owned();
                if text.contains(EXPECTED_BANNER) {
                    return Some(text);
                }
            }
            Ok(_) => return None,
            Err(_) => return None,
        }
    }
    None
}

#[tokio::test]
#[ignore = "requires the Plan 381 exact-pinned i2pd 2.61.0 ELS2 lane environment"]
async fn els2_i2pr_consumes_reference_published_els2() {
    let reference_ri = env_path("I2PR_ELS2_REFERENCE_ROUTER_INFO");
    let reference_endpoint: SocketAddr = env_value("I2PR_ELS2_REFERENCE_ENDPOINT")
        .parse()
        .expect("reference endpoint");
    // Plan 381: the qualified profile builds exact-three diverse
    // tunnels, so the lane bootstraps three family-distinct stock
    // references (f/c/n). Missing extra-peer environment fails like
    // every other lane input — never a silent smaller mesh.
    let peer2_ri = env_path("I2PR_ELS2_PEER2_ROUTER_INFO");
    let peer2_endpoint: SocketAddr = env_value("I2PR_ELS2_PEER2_ENDPOINT")
        .parse()
        .expect("peer2 endpoint");
    let peer3_ri = env_path("I2PR_ELS2_PEER3_ROUTER_INFO");
    let peer3_endpoint: SocketAddr = env_value("I2PR_ELS2_PEER3_ENDPOINT")
        .parse()
        .expect("peer3 endpoint");
    let extra_peers = vec![
        ReferencePeer {
            router_info_bytes: std::fs::read(&peer2_ri).expect("read peer2 router.info"),
            endpoint: peer2_endpoint,
        },
        ReferencePeer {
            router_info_bytes: std::fs::read(&peer3_ri).expect("read peer3 router.info"),
            endpoint: peer3_endpoint,
        },
    ];
    let destination = env_value("I2PR_ELS2_REFERENCE_DEST_B33_I2PD");
    let bind: SocketAddr = env_value("I2PR_ELS2_SSU2_BIND").parse().expect("bind");
    let evidence_dir = env_path("I2PR_ELS2_EVIDENCE_DIR");

    assert!(bind.ip().is_loopback(), "R bind must be loopback");
    assert!(
        reference_endpoint.ip().is_loopback(),
        "reference endpoint must be loopback"
    );
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");

    // The blinded address i2pd publishes is addressed to i2pr with the same
    // `.b32.i2p` suffix i2pd renders. i2pr decides blinded-ness by structure
    // before the b32 branch, so the suffix is a vocabulary choice on both
    // sides, not a kind marker.
    assert!(
        destination.ends_with(".b32.i2p"),
        "the lane must hand over the reference-facing spelling, got {destination}"
    );

    let data_dir = tempfile::tempdir().expect("data dir");
    let config_text = format!(
        "schema_version = 1\n[router]\ndata_dir = {:?}\n[i2pcontrol]\nenabled = true\n\
         bind_address = \"127.0.0.1\"\nport = 0\npassword = {TEST_PASSWORD:?}\n",
        data_dir.path().to_string_lossy()
    );
    let config = Config::parse(&config_text).expect("strict controlled profile");

    let manager = i2pr_daemon::build_shared_service_manager(&config)
        .expect("shared manager builds")
        .expect("control implies a shared manager");

    let mut rng = rand_core::OsRng;
    let bundle = Arc::new(RouterIdentityBundle::generate(&mut rng).expect("identity bundle"));

    let spec = ServiceProductSpec {
        data_dir: data_dir.path().to_path_buf(),
        ssu2_bind: bind,
        router_bundle: bundle,
        service_tunnels: Arc::new(ServiceTunnelSet {
            tunnels: Vec::new(),
        }),
        aliases: Arc::new(StaticAliasTable::new()),
        aggregate_connection_ceiling: 8,
        per_service_connection_ceiling: 4,
        reference: Some(ReferencePeer {
            router_info_bytes: std::fs::read(&reference_ri).expect("read reference router.info"),
            endpoint: reference_endpoint,
        }),
        extra_bootstrap_peers: extra_peers,
        options: ServiceProductOptions::default(),
        addressbook: i2pr_daemon::addressbook::SharedAddressBook::new(),
        shared_manager: Some(Arc::clone(&manager)),
    };

    let mut product = ServiceProduct::start(spec)
        .await
        .expect("service product starts against the reference peer");
    append_evidence(&evidence_dir, "reference-peer-started", "true");

    // The same manager, so the control-created service is owned by the product
    // runtime rather than by a parallel one.
    let control_state = Arc::new(
        TunnelControlState::for_config(&config, Arc::clone(&manager), secret_owner())
            .expect("control state for the product's manager"),
    );
    let inspection = Arc::new(InspectionHandles::from_config(&config));
    let state = Arc::new(
        I2pControlServiceState::new_with_inspection(config.i2pcontrol.clone(), inspection)
            .expect("control state builds"),
    );
    state.set_control_manager(Arc::clone(&control_state));
    let (listener, control_addr) = state
        .bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0))
        .await
        .expect("control bind");
    let parent = CancellationToken::new();
    let scope = ChildScope::for_test(&parent, ChildFailurePolicy::FailParent);
    let failures = control_state.startup(&scope, &parent).await;
    assert!(failures.is_empty(), "control startup clean: {failures:?}");
    let serve_state = Arc::clone(&state);
    let serve_token = parent.clone();
    let serve_scope = scope.clone();
    scope
        .spawn(move |_task| async move {
            let _ = serve_state.serve(listener, serve_scope, serve_token).await;
            Ok(())
        })
        .expect("control serve spawns");
    append_evidence(
        &evidence_dir,
        "i2pcontrol-listener-bound",
        &control_addr.to_string(),
    );

    let authenticated = control(
        control_addr,
        "Authenticate",
        serde_json::json!({"API": 1, "Password": TEST_PASSWORD}),
        1,
    )
    .await;
    let token = authenticated["result"]["Token"]
        .as_str()
        .expect("token minted")
        .to_owned();

    // The lane's consumer shape, created exactly as the plan requires: a
    // `.b33` target with DelayOpen, since `config.rs:1160-1176` refuses a
    // `.b33` service without it and the TOML path hardcodes it false.
    let create = serde_json::json!({
        "Token": token,
        "Action": "create",
        "Type": "client",
        "Name": SERVICE_ID,
        "TargetDestination": destination,
        "DelayOpen": true,
    });
    let created = control(control_addr, "TunnelManager", create, 2).await;
    append_evidence(
        &evidence_dir,
        "b33-client-created",
        &serde_json::json!({ "error": created["error"].is_null() }).to_string(),
    );
    assert!(
        created["error"].is_null(),
        "the .b33 client must be created over I2PControl: {created}"
    );

    // The resolver runs on the product's poll loop; drive it until the target
    // resolves and the local listener binds.
    let mut port: Option<u16> = None;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(180);
    while tokio::time::Instant::now() < deadline {
        let _ = product.poll_inbound().await;
        if let Some(bound) = product.client_listener_port(SERVICE_ID) {
            port = Some(bound);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    let port = port.expect("the .b33 client's local listener must bind");
    append_evidence(
        &evidence_dir,
        "b33-client-listener-bound",
        &port.to_string(),
    );

    // Record *why* the target is or is not resolved before asserting on the
    // payload. `EncryptedTargetStatus` is a closed enum with no ability to
    // carry a secret, so it is safe in evidence and it is the difference
    // between "the lookup did not happen" and "the lookup happened and the
    // record did not unwrap".
    let target_status = manager.encrypted_target_status(SERVICE_ID);
    append_evidence(
        &evidence_dir,
        "encrypted-target-status",
        &format!("{target_status:?}"),
    );
    if let Some(summary) = product.service_router_network_summary(SERVICE_ID) {
        append_evidence(
            &evidence_dir,
            "b33-router-network-summary",
            &format!("{summary:?}"),
        );
    }
    let counters = product.remote_counters().await;
    append_evidence(&evidence_dir, "remote-counters", &format!("{counters:?}"));
    let pre_failed = manager.failed_connects(SERVICE_ID);
    append_evidence(
        &evidence_dir,
        "pre-failed-connects",
        &pre_failed.to_string(),
    );
    let got = control(
        control_addr,
        "TunnelManager",
        serde_json::json!({"Token": token, "Action": "get", "Name": SERVICE_ID}),
        3,
    )
    .await;
    append_evidence(
        &evidence_dir,
        "control-get-pre",
        &serde_json::json!({ "error": got["error"].is_null(), "result": got["result"] })
            .to_string(),
    );

    // The three gates between "the listener bound" and "a lookup started".
    // None has been observed yet, so all three are read rather than assumed.
    // `spec_reference_for_service` and `project_remote_target` are the two
    // points `provision_all_service_router_material` consults before it ever
    // reaches the encrypted branch, and a `None` at either one is a `continue`
    // that leaves no status behind -- which is exactly what the evidence
    // currently shows.
    let reference_present = manager.spec_reference_for_service(SERVICE_ID).is_some();
    append_evidence(
        &evidence_dir,
        "spec-reference-present",
        &reference_present.to_string(),
    );
    let projection = manager
        .spec_reference_for_service(SERVICE_ID)
        .map(|reference| format!("{:?}", manager.project_remote_target(&reference)))
        .unwrap_or_else(|| "<no reference>".to_owned());
    append_evidence(&evidence_dir, "remote-target-projection", &projection);

    let banner = read_banner(&mut product, port).await;
    // Re-read after the connection attempt: the pre-connection snapshot above
    // is expected to be `None`/zero (provisioning runs on the data path via
    // `ensure_destination_active`), so only the post-attempt values
    // distinguish "activation never ran" from "lookup/stream failed".
    let post_status = manager.encrypted_target_status(SERVICE_ID);
    let post_counters = product.remote_counters().await;
    let post_reference_present = manager.spec_reference_for_service(SERVICE_ID).is_some();
    let post_projection = manager
        .spec_reference_for_service(SERVICE_ID)
        .map(|reference| format!("{:?}", manager.project_remote_target(&reference)))
        .unwrap_or_else(|| "<no reference>".to_owned());
    let post_generation = manager.committed_generation_id();
    let post_runtime_present = manager.service_runtime_for_spec(SERVICE_ID).is_some();
    append_evidence(
        &evidence_dir,
        "post-encrypted-target-status",
        &format!("{post_status:?}"),
    );
    append_evidence(
        &evidence_dir,
        "post-remote-counters",
        &format!("{post_counters:?}"),
    );
    append_evidence(
        &evidence_dir,
        "post-spec-reference-present",
        &post_reference_present.to_string(),
    );
    append_evidence(
        &evidence_dir,
        "post-remote-target-projection",
        &post_projection,
    );
    append_evidence(
        &evidence_dir,
        "post-committed-generation",
        &format!("{post_generation:?}"),
    );
    append_evidence(
        &evidence_dir,
        "post-service-runtime-present",
        &post_runtime_present.to_string(),
    );
    let post_failed = manager.failed_connects(SERVICE_ID);
    append_evidence(
        &evidence_dir,
        "post-failed-connects",
        &post_failed.to_string(),
    );
    let got_post = control(
        control_addr,
        "TunnelManager",
        serde_json::json!({"Token": token, "Action": "get", "Name": SERVICE_ID}),
        4,
    )
    .await;
    append_evidence(
        &evidence_dir,
        "control-get-post",
        &serde_json::json!({ "error": got_post["error"].is_null(), "result": got_post["result"] })
            .to_string(),
    );
    assert!(
        banner.is_some(),
        "the application payload must cross both ways: expected {EXPECTED_BANNER:?} back \
         through the reference's server tunnel. pre-status={target_status:?} \
         pre-counters={counters:?} post-status={post_status:?} \
         post-counters={post_counters:?} post-projection={post_projection}"
    );
    append_evidence(
        &evidence_dir,
        "application-payload-returned",
        EXPECTED_BANNER,
    );

    parent.cancel(i2pr_core::CancellationReason::TestHarnessTeardown);
    let _ = product.shutdown().await;
}
