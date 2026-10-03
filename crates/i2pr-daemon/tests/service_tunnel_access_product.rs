//! Plan 292 access-list product tests.
//!
//! Black-box over loopback TCP only: the pre-SYN gate enforces the
//! installed policy against the observed caller hash. Client
//! destinations are ephemeral per prepare (only server kinds
//! persist), so no test can pre-name the real caller: the matrix
//! pins both enforceable directions with stranger lists instead. A
//! non-empty allow-list naming nobody real rejects the caller
//! (counter fires, no handshake); a deny-list naming nobody real
//! admits the caller (echo flows). The listed-member-admits
//! direction is pinned at the boundary (`allows` unit tests) and the
//! admit path through the gate runs under every M10 roundtrip via
//! the default empty policy.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, HttpServerPolicy, LocalListenerSpec, ServerAccessPolicy,
    ServerTarget, ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet,
    ServiceTunnelSpec, StaticAliasTable,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use i2pr_daemon::service_tunnels::{ServiceTunnelManager, ServiceTunnelManagerConfig};

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

fn server_spec(id: &str, target: SocketAddr, access: ServerAccessPolicy) -> ServiceTunnelSpec {
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
        access,
        unique_local_address: false,
        multihoming: false,
        reply_bundling: false,
        use_ssl: false,
        http_policy: i2pr_service_tunnels::HttpServerPolicy::default(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
        connect_options: None,
        streamr_options: None,
    }
}

/// Probes the server destination under the same service id the real
/// manager will use. Server kinds persist their identity per
/// (directory, service id), so the alias stays valid; client kinds
/// are ephemeral and are never probed.
async fn probe_server_b64(data_dir: &Path, echo: SocketAddr, server_id: &str) -> String {
    let probe = build_manager(
        data_dir,
        vec![server_spec(server_id, echo, ServerAccessPolicy::default())],
        StaticAliasTable::new(),
    );
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

async fn start_echo_fixture() -> (SocketAddr, tokio::task::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut total = 0_usize;
        let mut chunk = [0_u8; 4096];
        loop {
            match tokio::time::timeout(Duration::from_secs(10), stream.read(&mut chunk)).await {
                Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
                Ok(Ok(n)) => {
                    total += n;
                    if stream.write_all(&chunk[..n]).await.is_err() {
                        break;
                    }
                }
            }
        }
        total
    });
    (addr, task)
}

async fn read_exact_bounded(stream: &mut TcpStream, expected: usize) -> Vec<u8> {
    let mut out = vec![0_u8; expected];
    tokio::time::timeout(Duration::from_secs(20), stream.read_exact(&mut out))
        .await
        .expect("echo deadline")
        .expect("echo bytes");
    out
}

/// Polls the denial counter until it reaches `want` or the deadline
/// expires; returns the last observed value.
async fn poll_denied(manager: &Arc<ServiceTunnelManager>, spec: &str, want: usize) -> usize {
    let started = std::time::Instant::now();
    loop {
        let seen = manager.access_denied_for(spec);
        if seen >= want || started.elapsed() > Duration::from_secs(20) {
            return seen;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// A hash nobody holds this epoch (the caller is an ephemeral
/// identity the test never observes).
fn stranger_hash() -> [u8; 32] {
    [0x5A_u8; 32]
}

/// A non-empty allow-list naming nobody real rejects the caller
/// before the handshake: no echo arrives and the denial counter
/// fires.
#[tokio::test(flavor = "current_thread")]
async fn allow_list_naming_only_stranger_rejects_caller() {
    let directory = temp_data_dir("access-allow-stranger");
    let data_dir: PathBuf = directory.path().to_path_buf();
    let (echo_addr, _echo_task) = start_echo_fixture().await;
    let server_b64 = probe_server_b64(&data_dir, echo_addr, "alpha-server").await;
    let access = ServerAccessPolicy {
        allow: vec![stranger_hash()],
        deny: Vec::new(),
    };
    let manager = build_manager(
        &data_dir,
        vec![
            client_spec("alpha-client", &server_b64),
            server_spec("alpha-server", echo_addr, access),
        ],
        StaticAliasTable::new(),
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    let payload = b"plan292-allow-stranger-001";
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream.write_all(payload).await.expect("write");
    // The server rejects the SYN pre-echo, so no payload comes back.
    let mut probe = [0_u8; 64];
    let echoed = tokio::time::timeout(Duration::from_secs(8), stream.read(&mut probe)).await;
    assert!(
        matches!(echoed, Err(_) | Ok(Ok(0))),
        "unlisted caller receives no echo bytes: {echoed:?}"
    );
    assert!(
        poll_denied(&manager, "alpha-server", 1).await >= 1,
        "rejection is recorded"
    );
}

/// A deny-list naming nobody real changes nothing: the echo
/// round-trip completes and no denial is recorded.
#[tokio::test(flavor = "current_thread")]
async fn deny_list_naming_others_admits_caller() {
    let directory = temp_data_dir("access-deny-others");
    let data_dir: PathBuf = directory.path().to_path_buf();
    let (echo_addr, echo_task) = start_echo_fixture().await;
    let server_b64 = probe_server_b64(&data_dir, echo_addr, "alpha-server").await;
    let access = ServerAccessPolicy {
        allow: Vec::new(),
        deny: vec![stranger_hash()],
    };
    let manager = build_manager(
        &data_dir,
        vec![
            client_spec("alpha-client", &server_b64),
            server_spec("alpha-server", echo_addr, access),
        ],
        StaticAliasTable::new(),
    );
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-client")
        .expect("listener");
    let payload = b"plan292-deny-others-001";
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream.write_all(payload).await.expect("write");
    assert_eq!(
        read_exact_bounded(&mut stream, payload.len()).await,
        payload
    );
    assert_eq!(manager.access_denied_for("alpha-server"), 0);
    let _ = stream.shutdown().await;
    let total = tokio::time::timeout(Duration::from_secs(10), echo_task)
        .await
        .expect("echo done")
        .expect("echo task");
    assert_eq!(total, payload.len());
}

fn canonical_b32() -> String {
    format!("{}.b32.i2p", "a".repeat(52))
}

/// Garbage is never admitted: a non-hash entry fails the parse
/// outright (it cannot silently become an empty allow-list), and a
/// populated allow-list admits only its members. All-`a` Base32 is
/// exactly 32 zero bytes, so membership is pinned without reaching
/// into the crate-internal decoder.
#[test]
fn parse_rejects_non_hashes_and_matches_members() {
    assert!(ServerAccessPolicy::parse(&["not-a-hash"], &[]).is_err());
    let listed = canonical_b32();
    let policy = ServerAccessPolicy::parse(&[listed.as_str()], &[]).expect("parses");
    assert!(policy.allows(&[0_u8; 32]));
    assert!(!policy.allows(&[1_u8; 32]));
}
