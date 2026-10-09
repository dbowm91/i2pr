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
//! - `I2PR_ELS2_AUTH_MODE` — `none` (default), `psk`, or `dh`: the mode the
//!   reference publisher uses. Anything else fails.
//! - `I2PR_ELS2_CLIENT_CREDENTIAL` — `psk:<hex>` / `dh:<hex>` when the mode
//!   is authorized, empty otherwise. It travels by environment into the
//!   `CustomOptions` `{"i2pr": {"LeasesetClientCredential": ...}}` seam (Plan
//!   380) and is never written to evidence — only its presence is recorded.
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
const REVERSE_SERVER_ID: &str = "plan400-i2pr-els2-server";
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

fn env_optional(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
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
async fn read_banner(
    product: &mut ServiceProduct,
    port: u16,
    manager: &i2pr_daemon::service_tunnels::ServiceTunnelManager,
    service_id: &str,
    active_connections_peak: &mut usize,
) -> Option<String> {
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
        *active_connections_peak =
            (*active_connections_peak).max(manager.active_connections(service_id));
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

async fn read_sam_line_pumping(
    product: &mut ServiceProduct,
    stream: &mut TcpStream,
    timeout: std::time::Duration,
) -> Result<String, String> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut line = Vec::new();
    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err("SAM reply deadline reached".to_owned());
        }
        let mut byte = [0_u8; 1];
        tokio::select! {
            result = stream.read(&mut byte) => match result {
                Ok(0) => return Err("SAM socket closed before reply".to_owned()),
                Ok(_) if byte[0] == b'\n' => {
                    return Ok(String::from_utf8_lossy(&line).trim_end_matches('\r').to_owned());
                }
                Ok(_) if line.len() < 4096 => line.push(byte[0]),
                Ok(_) => return Err("SAM reply exceeded line bound".to_owned()),
                Err(error) => return Err(format!("SAM socket read failed: {error}")),
            },
            _ = product.poll_inbound() => {}
            _ = tokio::time::sleep_until(deadline) => return Err("SAM reply deadline reached".to_owned()),
        }
    }
}

async fn sam_expect_ok(
    product: &mut ServiceProduct,
    stream: &mut TcpStream,
    command: &str,
    timeout: std::time::Duration,
) -> Result<String, String> {
    stream
        .write_all(format!("{command}\n").as_bytes())
        .await
        .map_err(|error| format!("SAM command write failed: {error}"))?;
    let reply = read_sam_line_pumping(product, stream, timeout).await?;
    if reply.contains("RESULT=OK") {
        Ok(reply)
    } else {
        Err(format!("SAM command returned a non-OK status: {reply}"))
    }
}

fn decode_key_hex(value: &str) -> [u8; 32] {
    let bytes = value.as_bytes();
    assert_eq!(bytes.len(), 64, "authorization key has fixed length");
    let mut output = [0_u8; 32];
    for (index, pair) in bytes.chunks_exact(2).enumerate() {
        let high = (pair[0] as char).to_digit(16).expect("hex digit");
        let low = (pair[1] as char).to_digit(16).expect("hex digit");
        output[index] = ((high << 4) | low) as u8;
    }
    output
}

fn encode_key_hex(value: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in value {
        encoded.push(HEX[usize::from(byte >> 4)] as char);
        encoded.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    encoded
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
    // Plan 384: the stock reference's ordinary (type-3) publisher is the
    // authority control for the i2pr-side post-start provisioning row.
    let authority_destination = env_value("I2PR_ELS2_REFERENCE_DEST_B32_STD");
    let bind: SocketAddr = env_value("I2PR_ELS2_SSU2_BIND").parse().expect("bind");
    let evidence_dir = env_path("I2PR_ELS2_EVIDENCE_DIR");
    // Plan 381 WP4: the publisher auth mode and, when it is authorized, the
    // client credential. The value is used once, in the create below, and is
    // never written to evidence — only the mode and its presence are.
    // A credential with mode `none` (or a mode without its credential) fails
    // here rather than driving the lane with a mismatched pair.
    let auth_mode = env_optional("I2PR_ELS2_AUTH_MODE");
    let auth_mode = if auth_mode.is_empty() {
        "none".to_owned()
    } else {
        auth_mode
    };
    let credential = env_optional("I2PR_ELS2_CLIENT_CREDENTIAL");
    match auth_mode.as_str() {
        "none" => assert!(
            credential.is_empty(),
            "mode none must not carry a client credential"
        ),
        "psk" | "dh" => assert!(
            !credential.is_empty(),
            "mode {auth_mode} requires I2PR_ELS2_CLIENT_CREDENTIAL"
        ),
        other => panic!("I2PR_ELS2_AUTH_MODE must be one of none|psk|dh, got {other}"),
    }
    append_evidence(&evidence_dir, "auth-mode", &auth_mode);
    append_evidence(
        &evidence_dir,
        "credential-present",
        &(!credential.is_empty()).to_string(),
    );
    // Plan 381 WP4 negatives. `I2PR_ELS2_NEGATIVE` is empty for the positive
    // rows, `wrong-credential` (PSK lane: the true credential is ignored and
    // a random one is presented), or `wrong-secret` (a well-formed but wrong
    // `leaseset_password` is configured, so the derived storage key misses
    // the published record). Anything else fails here, not in the lane.
    let negative = env_optional("I2PR_ELS2_NEGATIVE");
    match negative.as_str() {
        "" | "wrong-credential" | "wrong-secret" => {}
        other => {
            panic!("I2PR_ELS2_NEGATIVE must be one of wrong-credential|wrong-secret, got {other}")
        }
    }
    if negative == "wrong-credential" {
        assert_eq!(
            auth_mode, "psk",
            "wrong-credential runs on the PSK lane with a minted random key"
        );
    }
    append_evidence(
        &evidence_dir,
        "negative",
        if negative.is_empty() {
            "none"
        } else {
            negative.as_str()
        },
    );
    // A wrong credential is minted locally, never read from the lane: it
    // must not authorize, and it must not equal the true credential by
    // construction (fresh randomness, not a mutation of the real key).
    let presented_credential = if negative == "wrong-credential" {
        let mut bytes = [0_u8; 32];
        {
            use rand_core::TryRngCore as _;
            rand_core::OsRng
                .try_fill_bytes(&mut bytes)
                .expect("random wrong credential");
        }
        let mut hex = String::with_capacity(64);
        for byte in bytes {
            hex.push_str(&format!("{byte:02x}"));
        }
        format!("psk:{hex}")
    } else {
        credential.clone()
    };

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
    // In authorized modes the Plan 380 credential rides the typed
    // `CustomOptions` seam; the lane runner minted it against the same
    // publisher key the reference is publishing with.
    let mut create = serde_json::json!({
        "Token": token,
        "Action": "create",
        "Type": "client",
        "Name": SERVICE_ID,
        "TargetDestination": destination,
        "DelayOpen": true,
    });
    if !presented_credential.is_empty() {
        create["CustomOptions"] =
            serde_json::json!({"i2pr": {"LeasesetClientCredential": presented_credential}});
    }
    // A wrong lookup secret is a well-formed value the publisher never used.
    // `OptionalLookup` is the Proposal wire name the request layer maps to
    // `leaseset_password` (`tunnel_request.rs:186`).
    if negative == "wrong-secret" {
        create["OptionalLookup"] = serde_json::json!("plan381-wrong-lookup-secret");
    }
    let created = control(control_addr, "TunnelManager", create, 2).await;
    append_evidence(
        &evidence_dir,
        "b33-client-created",
        &serde_json::json!({ "error": created["error"].is_null() }).to_string(),
    );
    // Assert on the RESULT status, not just the envelope: a rejected
    // candidate returns `"status": "error - ..."` with a null JSON-RPC
    // error, and an envelope-only assert passes a service that was never
    // built. (Plan 381 WP4: this exact weakness hid a duplicate-listener
    // refusal of the second client for a full lane cycle.)
    let created_status = created["result"]["status"].as_str().unwrap_or_default();
    assert!(
        created["error"].is_null() && created_status.starts_with("success"),
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
    append_evidence(
        &evidence_dir,
        "consumer-credential-sealed",
        &manager
            .encrypted_target_credential(SERVICE_ID)
            .is_some()
            .to_string(),
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
    append_evidence(
        &evidence_dir,
        "pre-failure-stage",
        &format!("{:?}", manager.deferred_connect_failure_stage(SERVICE_ID)),
    );
    append_evidence(
        &evidence_dir,
        "pre-activation-pending",
        &format!("{:?}", product.destination_activation_pending(SERVICE_ID)),
    );
    append_evidence(
        &evidence_dir,
        "pre-activation-failure-present",
        &product
            .deferred_activation_failure(SERVICE_ID)
            .is_some()
            .to_string(),
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

    let mut consumer_active_connections_peak = 0;
    let banner = read_banner(
        &mut product,
        port,
        &manager,
        SERVICE_ID,
        &mut consumer_active_connections_peak,
    )
    .await;
    append_evidence(
        &evidence_dir,
        "b33-active-connections-peak",
        &consumer_active_connections_peak.to_string(),
    );
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
    append_evidence(
        &evidence_dir,
        "post-failure-stage",
        &format!("{:?}", manager.deferred_connect_failure_stage(SERVICE_ID)),
    );
    append_evidence(
        &evidence_dir,
        "post-activation-pending",
        &format!("{:?}", product.destination_activation_pending(SERVICE_ID)),
    );
    append_evidence(
        &evidence_dir,
        "post-activation-failure-present",
        &product
            .deferred_activation_failure(SERVICE_ID)
            .is_some()
            .to_string(),
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
    if negative.is_empty() {
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
    } else {
        // The negative rows: no payload may cross, and the refusal must be
        // the specific typed status — not a timeout, not a crash, not a
        // silent stall. A wrong credential dies in layer-1 authorization;
        // a wrong lookup secret derives a storage key no record answers to.
        //
        // The minted wrong credential authorizes nothing, but it is still
        // key-shaped material and must not reach evidence: self-scrub the
        // file before asserting on it.
        if negative == "wrong-credential" {
            let wrong_hex = presented_credential
                .strip_prefix("psk:")
                .expect("minted wrong credential keeps the psk: spelling");
            let evidence_text = std::fs::read_to_string(evidence_dir.join("driver-evidence.tsv"))
                .expect("evidence reads back");
            assert!(
                !evidence_text.contains(wrong_hex),
                "the minted wrong credential must not reach evidence"
            );
            append_evidence(&evidence_dir, "wrong-credential-scrub", "passed");
        }
        let expected = if negative == "wrong-credential" {
            "ClientCredentialRejected"
        } else {
            "LookupExhausted"
        };
        let observed = format!("{post_status:?}");
        append_evidence(&evidence_dir, "negative-expected", expected);
        append_evidence(&evidence_dir, "negative-observed", &observed);
        assert!(
            banner.is_none(),
            "negative {negative}: no payload may cross, but the banner came back"
        );
        assert!(
            observed.contains(expected),
            "negative {negative}: expected refusal {expected}, observed {observed}"
        );
    }

    // Plan 384 WP3: create an ordinary, non-encrypted client *after* the
    // product and its startup provisioning pass are already running. DelayOpen
    // routes its first connection through the product-owned bounded
    // activation request, which must observe the manager's committed control
    // generation and provision the new Destination before opening the stream.
    // This is deliberately a regular .b32 target, with no ELS2 credential.
    let authority_id = "plan384-post-start-authority";
    let authority_generation_before = manager.committed_generation_id();
    let authority_create = control(
        control_addr,
        "TunnelManager",
        serde_json::json!({
            "Token": token,
            "Action": "create",
            "Type": "client",
            "Name": authority_id,
            "TargetDestination": authority_destination,
            "DelayOpen": true,
        }),
        5,
    )
    .await;
    let authority_status = authority_create["result"]["status"]
        .as_str()
        .unwrap_or_default();
    assert!(
        authority_create["error"].is_null() && authority_status.starts_with("success"),
        "post-start ordinary authority client must commit over I2PControl: {authority_create}"
    );
    append_evidence(&evidence_dir, "authority-create", "committed");
    let committed_generation = manager.committed_generation_id();
    let generation_advanced = matches!(
        (authority_generation_before, committed_generation),
        (Some(before), Some(after)) if after > before
    );
    append_evidence(
        &evidence_dir,
        "authority-generation-advanced",
        &generation_advanced.to_string(),
    );
    assert!(
        generation_advanced,
        "post-start control create must publish a new shared-manager generation"
    );
    append_evidence(
        &evidence_dir,
        "authority-committed-generation",
        &format!("{}", committed_generation.is_some()),
    );
    let authority_projection = manager
        .spec_reference_for_service(authority_id)
        .map(|reference| format!("{:?}", manager.project_remote_target(&reference)));
    append_evidence(
        &evidence_dir,
        "authority-remote-target-projection",
        authority_projection.as_deref().unwrap_or("<missing>"),
    );
    assert!(
        authority_projection.is_some(),
        "the committed product generation must expose the control-created ordinary target"
    );
    let authority_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(180);
    let authority_port = loop {
        let _ = product.poll_inbound().await;
        if let Some(port) = product.client_listener_port(authority_id) {
            break port;
        }
        assert!(
            tokio::time::Instant::now() < authority_deadline,
            "post-start ordinary client did not bind after committed-generation provisioning"
        );
    };
    append_evidence(&evidence_dir, "authority-client-listener-bound", "true");
    let mut authority_active_connections_peak = 0;
    let authority_banner = read_banner(
        &mut product,
        authority_port,
        &manager,
        authority_id,
        &mut authority_active_connections_peak,
    )
    .await;
    append_evidence(
        &evidence_dir,
        "authority-active-connections-peak",
        &authority_active_connections_peak.to_string(),
    );
    let authority_counters = product.remote_counters().await;
    append_evidence(
        &evidence_dir,
        "authority-remote-counters",
        &format!("{authority_counters:?}"),
    );
    let authority_activation_failure = product.deferred_activation_failure(authority_id);
    let authority_activation_pending = product.destination_activation_pending(authority_id);
    let authority_failed_connects = manager.failed_connects(authority_id);
    let authority_failure_stage = manager.deferred_connect_failure_stage(authority_id);
    append_evidence(
        &evidence_dir,
        "authority-activation-failure-present",
        &authority_activation_failure.is_some().to_string(),
    );
    append_evidence(
        &evidence_dir,
        "authority-activation-pending",
        &format!("{authority_activation_pending:?}"),
    );
    append_evidence(
        &evidence_dir,
        "authority-failed-connects",
        &authority_failed_connects.to_string(),
    );
    append_evidence(
        &evidence_dir,
        "authority-failure-stage",
        authority_failure_stage.unwrap_or("none"),
    );
    assert!(
        authority_banner.is_some(),
        "post-start ordinary client did not resolve the reference's standard LS2 and carry payload; activation_pending={authority_activation_pending:?}; activation_failure={authority_activation_failure:?}; failure_stage={authority_failure_stage:?}; failed_connects={authority_failed_connects}; counters={authority_counters:?}"
    );
    append_evidence(&evidence_dir, "authority-b32-payload-returned", "true");

    // Plan 396 reverse direction: control-create a real type-5 server on the
    // running i2pr product, then consume it through the stock reference's
    // loopback SAM and the same local fixture. The i2pd session receives only
    // the mode-specific client key; evidence records no credential or address.
    let publication_baseline = product.lease_publication_snapshot();
    let fixture_port: u16 = env_value("I2PR_ELS2_FIXTURE_PORT")
        .parse()
        .expect("fixture port");
    let mut reverse_create_params = serde_json::json!({
        "Token": token,
        "Action": "create",
        "Type": "server",
        "Name": REVERSE_SERVER_ID,
        "TargetHost": "127.0.0.1",
        "TargetPort": fixture_port,
        "EncryptLeaseSet": match auth_mode.as_str() {
            "none" => "blinded",
            "psk" => "encrypted with per-user key (psk)",
            "dh" => "encrypted with per-user key (dh)",
            _ => unreachable!("auth mode validated above"),
        },
    });
    let (reverse_auth_public, reverse_consumer_key) = if auth_mode == "none" {
        (None, None)
    } else {
        let raw = credential
            .strip_prefix(if auth_mode == "psk" { "psk:" } else { "dh:" })
            .expect("validated credential scheme");
        let key = decode_key_hex(raw);
        if auth_mode == "psk" {
            (Some(key), Some(key))
        } else {
            let private = i2pr_crypto::X25519PrivateKey::from_bytes(key);
            (Some(private.public_bytes()), Some(key))
        }
    };
    if let Some(client_key) = reverse_auth_public.as_ref() {
        reverse_create_params["LeaseSetClientAuths"] = serde_json::json!([{
            "Name": "client0",
            "Key": encode_key_hex(client_key),
        }]);
    }
    let reverse_created = control(control_addr, "TunnelManager", reverse_create_params, 5).await;
    let reverse_status = reverse_created["result"]["status"]
        .as_str()
        .unwrap_or_default();
    assert!(
        reverse_created["error"].is_null() && reverse_status.starts_with("success"),
        "control-created reverse ELS2 server must commit: status={reverse_status:?}, error={}",
        reverse_created["error"]
    );
    append_evidence(&evidence_dir, "reverse-server-create", "committed");

    // A committed control generation is not itself publication evidence.
    // Drive the real product until its outbound DatabaseStore cells have
    // been admitted, or fail within a bounded deadline before asking i2pd
    // to consume the b33. This distinguishes publication absence from DHT
    // visibility while retaining the one-attempt remote lookup gate.
    let publication_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(180);
    let publication = loop {
        let current = product.lease_publication_snapshot();
        if current.accepted > publication_baseline.accepted {
            break current;
        }
        if tokio::time::Instant::now() >= publication_deadline {
            let provisioning = product.destination_provisioning_snapshot(REVERSE_SERVER_ID);
            append_evidence(&evidence_dir, "reverse-publication-handoff", "failed");
            append_evidence(
                &evidence_dir,
                "reverse-publication-failure-stage",
                current
                    .last_failure_stage
                    .map(|stage| stage.label())
                    .unwrap_or("none"),
            );
            append_evidence(
                &evidence_dir,
                "reverse-publication-counts",
                &format!(
                    "attempts={},accepted={},failed={},pending={},begin_capacity={},begin_no_floodfill={},begin_invalid_record={},begin_other={},coordinator_pending={}",
                    current.attempts,
                    current.accepted,
                    current.failed,
                    current.pending,
                    current.begin_rejected_capacity,
                    current.begin_rejected_no_floodfill,
                    current.begin_rejected_invalid_record,
                    current.begin_rejected_other,
                    current.coordinator_pending_publications
                ),
            );
            append_evidence(
                &evidence_dir,
                "reverse-destination-provisioning",
                &format!("{provisioning:?}"),
            );
            panic!(
                "control-created server did not reach the local DatabaseStore delivery boundary: {current:?}"
            );
        }
        product
            .poll_inbound()
            .await
            .expect("product advances while awaiting ELS2 publication");
    };
    append_evidence(&evidence_dir, "reverse-publication-handoff", "accepted");
    append_evidence(
        &evidence_dir,
        "reverse-publication-failure-stage",
        publication
            .last_failure_stage
            .map(|stage| stage.label())
            .unwrap_or("none"),
    );
    append_evidence(
        &evidence_dir,
        "reverse-publication-counts",
        &format!(
            "attempts={},accepted={},failed={},pending={},begin_capacity={},begin_no_floodfill={},begin_invalid_record={},begin_other={},coordinator_pending={}",
            publication.attempts,
            publication.accepted,
            publication.failed,
            publication.pending,
            publication.begin_rejected_capacity,
            publication.begin_rejected_no_floodfill,
            publication.begin_rejected_invalid_record,
            publication.begin_rejected_other,
            publication.coordinator_pending_publications
        ),
    );

    let reverse_get = control(
        control_addr,
        "TunnelManager",
        serde_json::json!({"Token": token, "Action": "get", "Name": REVERSE_SERVER_ID}),
        5,
    )
    .await;
    let reverse_address = reverse_get["result"]["info"]["encryptedAddress"]
        .as_str()
        .expect("type-5 server reports encryptedAddress")
        .to_owned();
    assert!(
        reverse_address.ends_with(".b32.i2p"),
        "reference SAM requires the encrypted address's b32 spelling"
    );
    let sam_port: u16 = env_value("I2PR_ELS2_REFERENCE_CONSUMER_SAM_PORT")
        .parse()
        .expect("reference consumer SAM port");
    let sam_endpoint = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), sam_port);
    let mut session = TcpStream::connect(sam_endpoint)
        .await
        .expect("reference SAM session connection");
    sam_expect_ok(
        &mut product,
        &mut session,
        "HELLO VERSION MIN=3.1 MAX=3.1",
        std::time::Duration::from_secs(30),
    )
    .await
    .expect("reference SAM hello");
    let mut create_session = [
        "SESSION CREATE STYLE=STREAM ID=plan400reverse",
        "DESTINATION=TRANSIENT SIGNATURE_TYPE=7",
    ]
    .join(" ");
    if let Some(client_key) = reverse_consumer_key.as_ref() {
        let client_key = i2pr_api::sam::base64::encode(client_key);
        create_session.push_str(" i2cp.leaseSetPrivKey=");
        create_session.push_str(&client_key);
    }
    sam_expect_ok(
        &mut product,
        &mut session,
        &create_session,
        std::time::Duration::from_secs(240),
    )
    .await
    .expect("reference SAM transient session with ELS2 authorization");
    append_evidence(&evidence_dir, "reverse-reference-signature-type", "7");
    let mut connect = TcpStream::connect(sam_endpoint)
        .await
        .expect("reference SAM connect socket");
    sam_expect_ok(
        &mut product,
        &mut connect,
        "HELLO VERSION MIN=3.1 MAX=3.1",
        std::time::Duration::from_secs(30),
    )
    .await
    .expect("reference SAM connect hello");
    let reverse_orphans_before_connect = product.inbound_orphan_receives();
    let reverse_inbound_traffic_before_connect = product.inbound_traffic_snapshot();
    append_evidence(
        &evidence_dir,
        "reverse-inbound-orphan-receives-before-connect",
        &reverse_orphans_before_connect.to_string(),
    );
    append_evidence(
        &evidence_dir,
        "reverse-router-inbound-traffic-before-connect",
        &format!("{reverse_inbound_traffic_before_connect:?}"),
    );
    sam_expect_ok(
        &mut product,
        &mut connect,
        &format!("STREAM CONNECT ID=plan400reverse DESTINATION={reverse_address} PORT=0"),
        std::time::Duration::from_secs(180),
    )
    .await
    .expect("reference consumes i2pr type-5 LeaseSet and establishes Streaming");
    append_evidence(
        &evidence_dir,
        "reverse-service-connections-after-connect",
        &format!("{:?}", product.service_tunnel_snapshot()),
    );
    append_evidence(
        &evidence_dir,
        "reverse-inbound-orphan-receives-after-connect",
        &product.inbound_orphan_receives().to_string(),
    );
    connect
        .write_all(b"plan400-reverse-ping\n")
        .await
        .expect("reverse fixture request writes");
    let reverse_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut reverse_payload = Vec::new();
    while tokio::time::Instant::now() < reverse_deadline {
        let mut chunk = [0_u8; 1024];
        tokio::select! {
            result = connect.read(&mut chunk) => match result {
                Ok(0) => break,
                Ok(length) => {
                    reverse_payload.extend_from_slice(&chunk[..length]);
                    if String::from_utf8_lossy(&reverse_payload).contains(EXPECTED_BANNER) {
                        break;
                    }
                }
                Err(_) => break,
            },
            _ = product.poll_inbound() => {}
            _ = tokio::time::sleep_until(reverse_deadline) => break,
        }
    }
    let reverse_payload_returned =
        String::from_utf8_lossy(&reverse_payload).contains(EXPECTED_BANNER);
    append_evidence(
        &evidence_dir,
        "reverse-payload-returned",
        if reverse_payload_returned {
            "true"
        } else {
            "false"
        },
    );
    append_evidence(
        &evidence_dir,
        "reverse-publication-post-read",
        &format!("{:?}", product.lease_publication_snapshot()),
    );
    append_evidence(
        &evidence_dir,
        "reverse-service-connections-post-read",
        &format!("{:?}", product.service_tunnel_snapshot()),
    );
    append_evidence(
        &evidence_dir,
        "reverse-routing-counters-post-read",
        &format!("{:?}", product.remote_counters().await),
    );
    append_evidence(
        &evidence_dir,
        "reverse-inbound-orphan-receives-post-read",
        &product.inbound_orphan_receives().to_string(),
    );
    append_evidence(
        &evidence_dir,
        "reverse-inbound-orphan-receives-delta",
        &product
            .inbound_orphan_receives()
            .saturating_sub(reverse_orphans_before_connect)
            .to_string(),
    );
    let reverse_inbound_traffic_after_read = product.inbound_traffic_snapshot();
    append_evidence(
        &evidence_dir,
        "reverse-router-inbound-traffic-delta",
        &format!(
            "{:?}",
            reverse_inbound_traffic_after_read.delta_since(reverse_inbound_traffic_before_connect)
        ),
    );
    append_evidence(
        &evidence_dir,
        "reverse-destination-provisioning-post-read",
        &format!(
            "{:?}",
            product.destination_provisioning_snapshot(REVERSE_SERVER_ID)
        ),
    );
    assert!(
        reverse_payload_returned,
        "stock reference SAM client did not receive the i2pr-published ELS2 fixture banner"
    );

    append_evidence(
        &evidence_dir,
        "authority-b32",
        "post-start ordinary payload returned",
    );
    append_evidence(
        &evidence_dir,
        "reverse-direction",
        "stock i2pd consumed the control-created i2pr ELS2 server through SAM",
    );

    parent.cancel(i2pr_core::CancellationReason::TestHarnessTeardown);
    let _ = product.shutdown().await;
}
