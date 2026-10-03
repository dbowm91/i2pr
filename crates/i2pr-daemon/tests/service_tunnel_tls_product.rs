//! Plan 297 `use_ssl` product tests.
//!
//! Black-box over loopback TCP only: a generic server with
//! `use_ssl` negotiates TLS to its loopback target under the
//! daemon's explicit pinned-identity policy through the real
//! server data path; a wrong pin fails the connection with no
//! plaintext fallback; `use_ssl` unset against a TLS target fails
//! instead of silently downgrading.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use i2pr_daemon::service_tunnels::{ServiceTunnelManager, ServiceTunnelManagerConfig};
use i2pr_daemon::service_tunnels_tls::ServiceTlsPolicy;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, HttpServerPolicy, LocalListenerSpec, ServerAccessPolicy,
    ServerTarget, ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet,
    ServiceTunnelSpec, StaticAliasTable,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn temp_data_dir(name: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(name)
        .tempdir()
        .expect("tempdir")
}

fn build_manager(
    data_dir: &Path,
    specs: Vec<ServiceTunnelSpec>,
    aliases: StaticAliasTable,
) -> Arc<ServiceTunnelManager> {
    let manager_config = ServiceTunnelManagerConfig {
        data_dir: data_dir.to_path_buf(),
        aggregate_connection_ceiling: 32,
        per_service_connection_ceiling: 8,
        specs: Arc::new(ServiceTunnelSet { tunnels: specs }),
        aliases: Arc::new(aliases),
    };
    Arc::new(ServiceTunnelManager::new(manager_config).expect("manager builds"))
}

fn client_spec(id: &str, server_b64: &str) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("id"),
        kind: ServiceTunnelKind::GenericClient,
        enabled: true,
        listener: Some(LocalListenerSpec::parse_socket("127.0.0.1:0").expect("listener")),
        target: None,
        targets: Vec::new(),
        destination: Some(DestinationRef::parse(server_b64).expect("destination")),
        policy: DestinationPolicy::Dedicated,
        inbound_port: None,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65536,
        timeouts: ServiceTimeouts::defaults(),
        shaping: i2pr_service_tunnels::TunnelShaping::balanced(),
        streaming_interactive: false,
        idle: i2pr_service_tunnels::IdlePolicy::disabled(),
        access: ServerAccessPolicy::default(),
        unique_local_address: false,
        multihoming: false,
        reply_bundling: false,
        use_ssl: false,
        http_policy: HttpServerPolicy::default(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
        connect_options: None,
        streamr_options: None,
    }
}

fn tls_server_spec(id: &str, target: SocketAddr, use_ssl: bool) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("id"),
        kind: ServiceTunnelKind::GenericServer,
        enabled: true,
        listener: None,
        target: Some(ServerTarget::LoopbackTcp(target)),
        targets: Vec::new(),
        destination: None,
        policy: DestinationPolicy::Dedicated,
        inbound_port: None,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65536,
        timeouts: ServiceTimeouts::defaults(),
        shaping: i2pr_service_tunnels::TunnelShaping::balanced(),
        streaming_interactive: false,
        idle: i2pr_service_tunnels::IdlePolicy::disabled(),
        access: ServerAccessPolicy::default(),
        unique_local_address: false,
        multihoming: false,
        reply_bundling: false,
        use_ssl,
        http_policy: HttpServerPolicy::default(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
        connect_options: None,
        streamr_options: None,
    }
}

async fn probe_server_b64(
    data_dir: &Path,
    spec: ServiceTunnelSpec,
    server_id: &str,
    policy: Option<Arc<ServiceTlsPolicy>>,
) -> String {
    let probe = build_manager(data_dir, vec![spec], StaticAliasTable::new());
    if let Some(policy) = policy {
        probe.set_service_tls_policy(policy);
    }
    probe.prepare().await.expect("server probe");
    probe
        .service_destination_b64(server_id)
        .expect("server b64")
}

async fn start_supervisors(manager: &Arc<ServiceTunnelManager>) -> (ChildScope, CancellationToken) {
    let runtimes = manager.prepare().await.expect("prepare");
    let cancel = CancellationToken::new();
    let scope = ChildScope::for_test(&cancel, ChildFailurePolicy::FailParent);
    manager
        .start_supervisors(runtimes, &scope, cancel.clone())
        .expect("supervisors started");
    (scope, cancel)
}

/// Loopback TLS echo fixture with a fresh self-signed identity.
/// Returns the socket address, the PEM certificate bytes (for
/// pinning), and an arrival channel.
async fn start_tls_echo(
    connections: usize,
) -> (
    SocketAddr,
    Vec<u8>,
    tokio::sync::mpsc::UnboundedReceiver<()>,
    tokio::task::JoinHandle<usize>,
) {
    let certified =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()]).expect("fixture cert");
    let cert_pem = certified.cert.pem().into_bytes();
    let cert_der = certified.cert.der().to_vec();
    let key_der = certified.key_pair.serialize_der();
    let server_config = std::sync::Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![rustls::pki_types::CertificateDer::from(cert_der.clone())],
                rustls::pki_types::PrivateKeyDer::Pkcs8(
                    rustls::pki_types::PrivatePkcs8KeyDer::from(key_der),
                ),
            )
            .expect("fixture server config"),
    );
    let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let (seen_tx, seen_rx) = tokio::sync::mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let mut total = 0_usize;
        for _ in 0..connections {
            let Ok(accepted) =
                tokio::time::timeout(Duration::from_secs(20), listener.accept()).await
            else {
                break;
            };
            let (stream, _) = accepted.expect("accept");
            let Ok(mut tls) =
                tokio::time::timeout(Duration::from_secs(20), acceptor.accept(stream))
                    .await
                    .map_err(|_| ())
                    .and_then(|inner| inner.map_err(|_| ()))
            else {
                break;
            };
            let _ = seen_tx.send(());
            let mut chunk = [0_u8; 4096];
            loop {
                match tokio::time::timeout(Duration::from_secs(10), tls.read(&mut chunk)).await {
                    Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
                    Ok(Ok(n)) => {
                        total += n;
                        if tls.write_all(&chunk[..n]).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
        total
    });
    (addr, cert_pem, seen_rx, task)
}

async fn roundtrip(listener: SocketAddr, payload: &[u8]) {
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream.write_all(payload).await.expect("write");
    let mut out = vec![0_u8; payload.len()];
    tokio::time::timeout(Duration::from_secs(20), stream.read_exact(&mut out))
        .await
        .expect("echo deadline")
        .expect("echo bytes");
    assert_eq!(out, payload);
    let _ = stream.shutdown().await;
}

async fn roundtrip_fails(listener: SocketAddr, payload: &[u8]) {
    // A failed server dial surfaces to the client as a broken
    // stream (never a hang, never plaintext): connect may succeed
    // at the loopback listener, but the payload never echoes.
    let outcome = tokio::time::timeout(Duration::from_secs(20), async {
        let mut stream = TcpStream::connect(listener).await?;
        stream.write_all(payload).await?;
        let mut out = vec![0_u8; payload.len()];
        stream.read_exact(&mut out).await?;
        Ok::<_, std::io::Error>(out)
    })
    .await;
    assert!(
        !matches!(outcome, Ok(Ok(_))),
        "roundtrip over a failed TLS dial must not succeed"
    );
}

/// With the target's identity pinned, `use_ssl` negotiates TLS
/// through the real server data path and the handshake is counted.
#[tokio::test(flavor = "current_thread")]
async fn use_ssl_negotiates_pinned_tls_to_loopback_target() {
    let directory = temp_data_dir("tls-pinned");
    let data_dir: PathBuf = directory.path().to_path_buf();
    let (target_addr, cert_pem, mut seen, _echo) = start_tls_echo(1).await;
    let policy = Arc::new(
        ServiceTlsPolicy::from_parts(None, Some(cert_pem), None).expect("pinned policy builds"),
    );
    let server_b64 = probe_server_b64(
        &data_dir,
        tls_server_spec("tls-server", target_addr, true),
        "tls-server",
        Some(Arc::clone(&policy)),
    )
    .await;
    let manager = build_manager(
        &data_dir,
        vec![
            client_spec("tls-client", &server_b64),
            tls_server_spec("tls-server", target_addr, true),
        ],
        StaticAliasTable::new(),
    );
    manager.set_service_tls_policy(policy);
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("tls-client")
        .expect("listener");
    roundtrip(listener, b"plan297-pinned-tls-001").await;
    tokio::time::timeout(Duration::from_secs(10), seen.recv())
        .await
        .expect("target arrival")
        .expect("arrival");
    assert_eq!(manager.tls_handshakes_for("tls-server"), (1, 0));
}

/// A wrong pin fails the handshake with no plaintext fallback and
/// counts the failure.
#[tokio::test(flavor = "current_thread")]
async fn wrong_pin_fails_closed_without_fallback() {
    let directory = temp_data_dir("tls-wrong-pin");
    let data_dir: PathBuf = directory.path().to_path_buf();
    let (target_addr, _cert_pem, _seen, _echo) = start_tls_echo(1).await;
    let (other_pem, _other_key) = {
        let certified =
            rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()]).expect("other cert");
        (
            certified.cert.pem().into_bytes(),
            certified.key_pair.serialize_pem().into_bytes(),
        )
    };
    let policy = Arc::new(
        ServiceTlsPolicy::from_parts(None, Some(other_pem), None).expect("wrong-pin policy builds"),
    );
    let server_b64 = probe_server_b64(
        &data_dir,
        tls_server_spec("tls-server", target_addr, true),
        "tls-server",
        Some(Arc::clone(&policy)),
    )
    .await;
    let manager = build_manager(
        &data_dir,
        vec![
            client_spec("tls-client", &server_b64),
            tls_server_spec("tls-server", target_addr, true),
        ],
        StaticAliasTable::new(),
    );
    manager.set_service_tls_policy(policy);
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("tls-client")
        .expect("listener");
    roundtrip_fails(listener, b"plan297-wrong-pin-001").await;
    assert_eq!(manager.tls_handshakes_for("tls-server"), (0, 1));
}

/// `use_ssl` unset against a TLS target fails instead of silently
/// downgrading to plaintext.
#[tokio::test(flavor = "current_thread")]
async fn plaintext_against_tls_target_fails() {
    let directory = temp_data_dir("tls-plaintext");
    let data_dir: PathBuf = directory.path().to_path_buf();
    let (target_addr, _cert_pem, _seen, _echo) = start_tls_echo(1).await;
    let server_b64 = probe_server_b64(
        &data_dir,
        tls_server_spec("tls-server", target_addr, false),
        "tls-server",
        None,
    )
    .await;
    let manager = build_manager(
        &data_dir,
        vec![
            client_spec("tls-client", &server_b64),
            tls_server_spec("tls-server", target_addr, false),
        ],
        StaticAliasTable::new(),
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("tls-client")
        .expect("listener");
    roundtrip_fails(listener, b"plan297-plaintext-001").await;
    assert_eq!(manager.tls_handshakes_for("tls-server"), (0, 0));
}

/// `use_ssl` without an installed policy fails before any
/// destination allocates.
#[tokio::test(flavor = "current_thread")]
async fn use_ssl_without_policy_fails_before_prepare() {
    let directory = temp_data_dir("tls-no-policy");
    let data_dir: PathBuf = directory.path().to_path_buf();
    let (target_addr, _cert_der, _seen, _echo) = start_tls_echo(0).await;
    let manager = build_manager(
        &data_dir,
        vec![tls_server_spec("tls-server", target_addr, true)],
        StaticAliasTable::new(),
    );
    assert!(
        manager.prepare().await.is_err(),
        "use_ssl without a policy must fail"
    );
}
