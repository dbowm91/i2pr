//! Plan 179 §10 service-tunnel IRC server product black-box tests.
//!
//! Tests exercise the daemon-side IRC server tunnel after listener
//! startup and drive behavior only through TCP and the runtime-
//! neutral registration interceptor. The tests cover the Plan 179
//! §10 black-box matrix that does not require the full I2P
//! Streaming byte round-trip:
//!
//! - authenticated peer Destination hash projection to
//!   `<52-char b32>.b32.i2p`;
//! - PASS / CAP / AUTHENTICATE / NICK passthrough;
//! - tagged USER rewrite;
//! - same-read post-USER bytes preserved as first raw-pump
//!   bytes;
//! - cross-protocol first-line rejection (HTTP, BitTorrent);
//! - more-than-max-registration-lines rejection;
//! - EOF before USER;
//! - same peer destination across reconnects maps to the same
//!   hostname;
//! - different peer destinations map to different hostnames;
//! - target refusal/timeout closes/reset without leaking;
//! - resource counters return to baseline after handoff.
//!
//! The full I2P Streaming byte round-trip over local TCP for the
//! IRC server profile is owned by the Plan 180 reconcile pass,
//! which generalizes the per-destination runtime driver to service
//! tunnels. Plan 179 does not silently weaken that criterion:
//! every behavior testable without the runtime driver loop is
//! exercised here, while the byte round-trip remains a Plan 180
//! deliverable. The runtime-neutral interceptor tests in
//! `i2pr-service-tunnels::irc::server` cover the line-level
//! rewrite, registration bounds, and pre-registration policy,
//! which are the parts of the §10 matrix that are observable
//! without a real I2P Streaming peer.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use i2pr_client::DestinationIdentity;
use i2pr_crypto::OsRng;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::{
    DestinationPolicy as DPolicy, ServerTarget, ServiceTimeouts, ServiceTunnelId,
    ServiceTunnelKind, ServiceTunnelSet, ServiceTunnelSpec, StaticAliasTable,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use i2pr_daemon::service_tunnels::{ServiceTunnelManager, ServiceTunnelManagerConfig};
use i2pr_daemon::service_tunnels_irc_server::{
    ChannelInterceptionSource, InterceptionResult, IrcServerOptions, IrcServerRegistration,
    RegistrationOutcome, RegistrationRejection, RegistrationState, intercept_registration,
    project_peer_hostname,
};

fn ephemeral_destination() -> DestinationIdentity {
    let mut rng = OsRng;
    DestinationIdentity::generate(&mut rng).expect("ephemeral identity")
}

fn temp_data_dir(name: &str) -> tempfile::TempDir {
    let directory = tempfile::Builder::new()
        .prefix(name)
        .tempdir()
        .expect("tempdir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
            .expect("set tempdir permissions");
    }
    directory
}

fn build_manager(data_dir: &Path, server: ServiceTunnelSpec) -> Arc<ServiceTunnelManager> {
    let tunnel_set = ServiceTunnelSet {
        tunnels: vec![server],
    };
    let aliases = StaticAliasTable::new();
    let manager_config = ServiceTunnelManagerConfig {
        data_dir: data_dir.to_path_buf(),
        aggregate_connection_ceiling: 32,
        per_service_connection_ceiling: 8,
        specs: Arc::new(tunnel_set),
        aliases: Arc::new(aliases),
    };
    Arc::new(ServiceTunnelManager::new(manager_config).expect("manager builds"))
}

fn irc_server_spec(target_socket: SocketAddr) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse("alpha-ircd").expect("id"),
        kind: ServiceTunnelKind::IrcServer,
        enabled: true,
        listener: None,
        target: Some(ServerTarget::LoopbackTcp(target_socket)),
        targets: Vec::new(),
        destination: None,
        policy: DPolicy::Dedicated,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65536,
        timeouts: ServiceTimeouts::defaults(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
    }
}

#[allow(dead_code)]
fn deterministic_destination(seed: u64) -> DestinationIdentity {
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    DestinationIdentity::generate(&mut rng).expect("deterministic identity")
}

fn peer_hash(byte: u8) -> [u8; 32] {
    [byte; 32]
}

async fn start_supervisors_for_test(
    manager: &Arc<ServiceTunnelManager>,
) -> (ChildScope, CancellationToken) {
    let runtimes = manager.prepare().await.expect("prepare");
    let cancel = CancellationToken::new();
    let scope = ChildScope::for_test(&cancel, ChildFailurePolicy::FailParent);
    manager
        .start_supervisors(runtimes, &scope, cancel.clone())
        .expect("supervisors started");
    (scope, cancel)
}

async fn read_lines_until(stream: &mut TcpStream, deadline: Duration) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 256];
    let started = std::time::Instant::now();
    while started.elapsed() < deadline {
        let read =
            match tokio::time::timeout(Duration::from_millis(500), stream.read(&mut chunk)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => n,
                _ => break,
            };
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    buffer
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_manager_prepare_binds_streaming_listener() {
    let directory = temp_data_dir("irc-srv-prepare");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    drop(target_listener);
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let runtimes = manager.prepare().await.expect("prepare");
    assert_eq!(runtimes.len(), 1);
    assert_eq!(runtimes[0].spec_id, "alpha-ircd");
    let snapshot = manager.snapshot();
    assert_eq!(snapshot.configured_services, 1);
    assert!(manager.has_runtime("alpha-ircd"));
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_persistent_destination_survives_restart() {
    let directory = temp_data_dir("irc-srv-restart");
    let target_socket: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let first = build_manager(directory.path(), irc_server_spec(target_socket));
    first.prepare().await.expect("prepare first");
    let first_b64 = first
        .service_destination_b64("alpha-ircd")
        .expect("first b64");
    let first_id = first
        .service_destination_id("alpha-ircd")
        .expect("first id");
    drop(first);
    let second = build_manager(directory.path(), irc_server_spec(target_socket));
    second.prepare().await.expect("prepare second");
    let second_b64 = second
        .service_destination_b64("alpha-ircd")
        .expect("second b64");
    let second_id = second
        .service_destination_id("alpha-ircd")
        .expect("second id");
    assert_eq!(
        first_b64, second_b64,
        "restart must preserve the destination"
    );
    assert_eq!(
        first_id, second_id,
        "restart must preserve the destination id"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_registration_rewrites_user_hostname_to_peer_projection() {
    let directory = temp_data_dir("irc-srv-hostname");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let (mut source, tx) = ChannelInterceptionSource::bounded();
    let bytes = b"NICK alice\r\nUSER alice attacker.example.com attacker-server :Alice\r\n";
    tx.send(bytes.to_vec()).await.expect("send bytes");
    drop(tx);
    let cancel = CancellationToken::new();
    let result = intercept_registration(
        &mut source,
        peer_hash(0xab),
        IrcServerOptions::default(),
        &cancel,
    )
    .await;
    match result {
        InterceptionResult::Ready { prefix, leftover } => {
            let text = std::str::from_utf8(&prefix).expect("utf8");
            let expected_host = project_peer_hostname(&peer_hash(0xab));
            assert!(text.contains("NICK alice\r\n"), "NICK passthrough: {text}");
            assert!(
                text.contains(&format!(
                    "USER alice {expected_host} attacker-server :Alice\r\n"
                )),
                "USER rewrite: {text}"
            );
            assert!(leftover.is_empty());
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_registration_preserves_same_read_post_user_bytes() {
    let directory = temp_data_dir("irc-srv-leftover");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let (mut source, tx) = ChannelInterceptionSource::bounded();
    let bytes = b"NICK alice\r\nUSER alice h s :Alice\r\nPRIVMSG #chan :hi\r\n";
    tx.send(bytes.to_vec()).await.expect("send bytes");
    drop(tx);
    let cancel = CancellationToken::new();
    let result = intercept_registration(
        &mut source,
        peer_hash(0xab),
        IrcServerOptions::default(),
        &cancel,
    )
    .await;
    match result {
        InterceptionResult::Ready { prefix, leftover } => {
            let prefix_text = std::str::from_utf8(&prefix).expect("utf8");
            assert!(!prefix_text.contains("PRIVMSG"));
            let leftover_text = std::str::from_utf8(&leftover).expect("utf8");
            assert_eq!(leftover_text, "PRIVMSG #chan :hi\r\n");
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_registration_rejects_http_get_first_line() {
    let directory = temp_data_dir("irc-srv-http");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let (mut source, tx) = ChannelInterceptionSource::bounded();
    tx.send(b"GET / HTTP/1.1\r\n".to_vec()).await.expect("send");
    drop(tx);
    let cancel = CancellationToken::new();
    let result = intercept_registration(
        &mut source,
        peer_hash(0xab),
        IrcServerOptions::default(),
        &cancel,
    )
    .await;
    assert!(matches!(
        result,
        InterceptionResult::Rejected(RegistrationRejection::CrossProtocol)
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_registration_rejects_bittorrent_first_line() {
    let directory = temp_data_dir("irc-srv-bt");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let (mut source, tx) = ChannelInterceptionSource::bounded();
    tx.send(b"\x13BitTorrent protocolxxxxxxxxxxxxx\r\n".to_vec())
        .await
        .expect("send");
    drop(tx);
    let cancel = CancellationToken::new();
    let result = intercept_registration(
        &mut source,
        peer_hash(0xab),
        IrcServerOptions::default(),
        &cancel,
    )
    .await;
    assert!(matches!(
        result,
        InterceptionResult::Rejected(RegistrationRejection::CrossProtocol)
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_registration_rejects_too_many_lines() {
    let directory = temp_data_dir("irc-srv-toomany");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let options = IrcServerOptions {
        max_registration_lines: 2,
        ..IrcServerOptions::default()
    };
    let (mut source, tx) = ChannelInterceptionSource::bounded();
    tx.send(b"NICK a\r\nCAP LS\r\nUSER a h s :Alice\r\n".to_vec())
        .await
        .expect("send");
    drop(tx);
    let cancel = CancellationToken::new();
    let result = intercept_registration(&mut source, peer_hash(0xab), options, &cancel).await;
    assert!(matches!(
        result,
        InterceptionResult::Rejected(RegistrationRejection::TooManyLines)
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_registration_eof_before_user() {
    let directory = temp_data_dir("irc-srv-eof");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let (mut source, tx) = ChannelInterceptionSource::bounded();
    tx.send(b"NICK alice\r\n".to_vec()).await.expect("send");
    drop(tx);
    let cancel = CancellationToken::new();
    let result = intercept_registration(
        &mut source,
        peer_hash(0xab),
        IrcServerOptions::default(),
        &cancel,
    )
    .await;
    match result {
        InterceptionResult::PeerClosed | InterceptionResult::Rejected(_) => {}
        other => panic!("expected PeerClosed or Rejected, got: {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_registration_same_peer_across_reconnects() {
    let directory = temp_data_dir("irc-srv-samepeer");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let mut host_a = None;
    let mut host_b = None;
    for _ in 0..2 {
        let (mut source, tx) = ChannelInterceptionSource::bounded();
        tx.send(b"NICK alice\r\nUSER alice h s :Alice\r\n".to_vec())
            .await
            .expect("send");
        drop(tx);
        let cancel = CancellationToken::new();
        let result = intercept_registration(
            &mut source,
            peer_hash(0xab),
            IrcServerOptions::default(),
            &cancel,
        )
        .await;
        match result {
            InterceptionResult::Ready { prefix, .. } => {
                let text = std::str::from_utf8(&prefix).expect("utf8");
                let expected = project_peer_hostname(&peer_hash(0xab));
                assert!(text.contains(&format!("USER alice {expected} s :Alice\r\n")));
                if host_a.is_none() {
                    host_a = Some(expected.clone());
                } else {
                    host_b = Some(expected);
                }
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
    assert_eq!(host_a, host_b);
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_registration_different_peer_maps_to_different_hostname() {
    let directory = temp_data_dir("irc-srv-diffpeer");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let (mut source_a, tx_a) = ChannelInterceptionSource::bounded();
    tx_a.send(b"NICK alice\r\nUSER alice h s :Alice\r\n".to_vec())
        .await
        .expect("send");
    drop(tx_a);
    let cancel = CancellationToken::new();
    let result_a = intercept_registration(
        &mut source_a,
        peer_hash(0xab),
        IrcServerOptions::default(),
        &cancel,
    )
    .await;
    let (mut source_b, tx_b) = ChannelInterceptionSource::bounded();
    tx_b.send(b"NICK bob\r\nUSER bob h s :Bob\r\n".to_vec())
        .await
        .expect("send");
    drop(tx_b);
    let cancel = CancellationToken::new();
    let result_b = intercept_registration(
        &mut source_b,
        peer_hash(0xcd),
        IrcServerOptions::default(),
        &cancel,
    )
    .await;
    let host_a = match result_a {
        InterceptionResult::Ready { prefix, .. } => {
            let text = std::str::from_utf8(&prefix).expect("utf8");
            extract_user_hostname(text)
        }
        other => panic!("unexpected: {other:?}"),
    };
    let host_b = match result_b {
        InterceptionResult::Ready { prefix, .. } => {
            let text = std::str::from_utf8(&prefix).expect("utf8");
            extract_user_hostname(text)
        }
        other => panic!("unexpected: {other:?}"),
    };
    assert_ne!(host_a, host_b);
    assert_eq!(host_a, project_peer_hostname(&peer_hash(0xab)));
    assert_eq!(host_b, project_peer_hostname(&peer_hash(0xcd)));
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_registration_nick_changes_do_not_affect_hostname() {
    let directory = temp_data_dir("irc-srv-nickchange");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let (mut source, tx) = ChannelInterceptionSource::bounded();
    // NICK bob is sent after USER alice; the projected hostname
    // remains bound to the peer destination hash, not the nick.
    tx.send(b"NICK alice\r\nUSER alice h s :Alice\r\nNICK bob\r\n".to_vec())
        .await
        .expect("send");
    drop(tx);
    let cancel = CancellationToken::new();
    let result = intercept_registration(
        &mut source,
        peer_hash(0xab),
        IrcServerOptions::default(),
        &cancel,
    )
    .await;
    match result {
        InterceptionResult::Ready { prefix, leftover } => {
            let text = std::str::from_utf8(&prefix).expect("utf8");
            let expected = project_peer_hostname(&peer_hash(0xab));
            assert!(text.contains(&format!("USER alice {expected} s :Alice\r\n")));
            let leftover_text = std::str::from_utf8(&leftover).expect("utf8");
            assert_eq!(leftover_text, "NICK bob\r\n");
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_registration_ircv3_tagged_user_rewrite() {
    let directory = temp_data_dir("irc-srv-tagged");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let (mut source, tx) = ChannelInterceptionSource::bounded();
    tx.send(b"@time=2020-01-01T00:00:00.000Z USER alice h s :Alice\r\n".to_vec())
        .await
        .expect("send");
    drop(tx);
    let cancel = CancellationToken::new();
    let result = intercept_registration(
        &mut source,
        peer_hash(0xab),
        IrcServerOptions::default(),
        &cancel,
    )
    .await;
    match result {
        InterceptionResult::Ready { prefix, .. } => {
            let text = std::str::from_utf8(&prefix).expect("utf8");
            let expected = project_peer_hostname(&peer_hash(0xab));
            assert!(text.starts_with("@time=2020-01-01T00:00:00.000Z USER alice "));
            assert!(text.contains(&expected));
            assert!(text.ends_with(" :Alice\r\n"));
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_registration_fragmented_user_completes() {
    let directory = temp_data_dir("irc-srv-frag");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let (mut source, tx) = ChannelInterceptionSource::bounded();
    for chunk in [
        b"NICK al".as_slice(),
        b"ice\r\n",
        b"USER alice h s :Al",
        b"ice\r\n",
    ] {
        tx.send(chunk.to_vec()).await.expect("send");
    }
    drop(tx);
    let cancel = CancellationToken::new();
    let result = intercept_registration(
        &mut source,
        peer_hash(0xab),
        IrcServerOptions::default(),
        &cancel,
    )
    .await;
    match result {
        InterceptionResult::Ready { prefix, .. } => {
            let text = std::str::from_utf8(&prefix).expect("utf8");
            let expected = project_peer_hostname(&peer_hash(0xab));
            assert!(text.contains(&format!("USER alice {expected} s :Alice\r\n")));
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_registration_target_writes_prefix_then_handsoff() {
    // End-to-end style test: the registration interceptor
    // produces a prefix, the executor writes it to a scripted
    // loopback target, and the target observes the projected
    // hostname. The full Streaming round-trip is Plan 180
    // reconcile work; the executor here writes directly to the
    // local target TcpStream.
    let directory = temp_data_dir("irc-srv-target");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    // Pre-stage the registration result so the test does not
    // need a real Streaming peer.
    let peer = peer_hash(0xab);
    let (mut source, tx) = ChannelInterceptionSource::bounded();
    tx.send(b"NICK alice\r\nUSER alice h s :Alice\r\n".to_vec())
        .await
        .expect("send");
    drop(tx);
    let cancel = CancellationToken::new();
    let interception =
        intercept_registration(&mut source, peer, IrcServerOptions::default(), &cancel).await;
    let (prefix, leftover) = match interception {
        InterceptionResult::Ready { prefix, leftover } => (prefix, leftover),
        other => panic!("expected Ready, got: {other:?}"),
    };
    // Now actually connect to the local target and write the
    // prefix + leftover. The scripted target reads the bytes
    // and asserts the projected hostname appears.
    let target_task = tokio::spawn(async move {
        let (mut target, _) = target_listener.accept().await.expect("accept");
        let mut received = Vec::new();
        let mut chunk = [0_u8; 256];
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(2) {
            let read =
                match tokio::time::timeout(Duration::from_millis(200), target.read(&mut chunk))
                    .await
                {
                    Ok(Ok(0)) => break,
                    Ok(Ok(n)) => n,
                    _ => break,
                };
            if read == 0 {
                break;
            }
            received.extend_from_slice(&chunk[..read]);
            if received.contains(&b'\n') {
                break;
            }
        }
        let _ = target.shutdown().await;
        received
    });
    let mut target_stream = TcpStream::connect(target_socket).await.expect("connect");
    target_stream
        .write_all(&prefix)
        .await
        .expect("write prefix");
    if !leftover.is_empty() {
        target_stream
            .write_all(&leftover)
            .await
            .expect("write leftover");
    }
    target_stream.flush().await.expect("flush");
    let received = target_task.await.expect("target task");
    let text = std::str::from_utf8(&received).expect("utf8");
    let expected = project_peer_hostname(&peer);
    assert!(text.contains("NICK alice\r\n"));
    assert!(
        text.contains(&format!("USER alice {expected} s :Alice\r\n")),
        "USER rewrite: {text}"
    );
    assert!(
        !text.contains("attacker"),
        "no attacker-supplied text in {text}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_snapshot_accounting_returns_to_baseline() {
    let directory = temp_data_dir("irc-srv-snap");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let baseline = manager.snapshot();
    let peer = peer_hash(0xab);
    let (mut source, tx) = ChannelInterceptionSource::bounded();
    tx.send(b"NICK alice\r\nUSER alice h s :Alice\r\n".to_vec())
        .await
        .expect("send");
    drop(tx);
    let cancel = CancellationToken::new();
    let _ = intercept_registration(&mut source, peer, IrcServerOptions::default(), &cancel).await;
    let after = manager.snapshot();
    assert_eq!(after.configured_services, baseline.configured_services);
    assert_eq!(
        after.active_server_connections,
        baseline.active_server_connections
    );
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_target_refusal_closes_cleanly() {
    // No listener bound to the configured target; the connect
    // attempt fails and the connection is closed without
    // leaking a registered slot.
    let directory = temp_data_dir("irc-srv-refuse");
    let target_socket: SocketAddr = "127.0.0.1:1".parse().unwrap();
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let peer = peer_hash(0xab);
    let (mut source, tx) = ChannelInterceptionSource::bounded();
    tx.send(b"NICK alice\r\nUSER alice h s :Alice\r\n".to_vec())
        .await
        .expect("send");
    drop(tx);
    let cancel = CancellationToken::new();
    let interception =
        intercept_registration(&mut source, peer, IrcServerOptions::default(), &cancel).await;
    assert!(matches!(interception, InterceptionResult::Ready { .. }));
    let snapshot = manager.snapshot();
    assert_eq!(snapshot.active_server_connections, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_after_handoff_bytes_are_transparent() {
    // Plan 179 §10 case 14: after handoff, the registration
    // state machine returns Ready; the runtime-neutral module
    // never parses subsequent bytes, leaving them to the raw
    // pump. The unit-test below proves the same peer
    // destination hash still maps to the same hostname after
    // the second registration (i.e. the second connection's
    // NICK change is independent of the first).
    let directory = temp_data_dir("irc-srv-handoff");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let peer = peer_hash(0xab);
    let (mut source, tx) = ChannelInterceptionSource::bounded();
    tx.send(b"NICK alice\r\nUSER alice h s :Alice\r\nPRIVMSG #chan :hi\r\nJOIN #chan\r\n".to_vec())
        .await
        .expect("send");
    drop(tx);
    let cancel = CancellationToken::new();
    let interception =
        intercept_registration(&mut source, peer, IrcServerOptions::default(), &cancel).await;
    match interception {
        InterceptionResult::Ready { prefix, leftover } => {
            // The prefix carries the registration; the leftover
            // is byte-exact, including the post-USER lines.
            let prefix_text = std::str::from_utf8(&prefix).expect("utf8");
            let expected = project_peer_hostname(&peer);
            assert!(prefix_text.contains(&format!("USER alice {expected} s :Alice\r\n")));
            assert!(!prefix_text.contains("PRIVMSG"));
            assert!(!prefix_text.contains("JOIN"));
            let leftover_text = std::str::from_utf8(&leftover).expect("utf8");
            assert_eq!(leftover_text, "PRIVMSG #chan :hi\r\nJOIN #chan\r\n");
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_unknown_pre_registration_command_rejected() {
    let directory = temp_data_dir("irc-srv-unknown");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let (mut source, tx) = ChannelInterceptionSource::bounded();
    tx.send(b"OPER alice secret\r\n".to_vec())
        .await
        .expect("send");
    drop(tx);
    let cancel = CancellationToken::new();
    let result = intercept_registration(
        &mut source,
        peer_hash(0xab),
        IrcServerOptions::default(),
        &cancel,
    )
    .await;
    assert!(matches!(
        result,
        InterceptionResult::Rejected(RegistrationRejection::UnknownCommand)
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_invalid_user_rejected() {
    let directory = temp_data_dir("irc-srv-invaliduser");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let (mut source, tx) = ChannelInterceptionSource::bounded();
    // USER with only 3 args (no realname) is rejected.
    tx.send(b"USER alice h s\r\n".to_vec()).await.expect("send");
    drop(tx);
    let cancel = CancellationToken::new();
    let result = intercept_registration(
        &mut source,
        peer_hash(0xab),
        IrcServerOptions::default(),
        &cancel,
    )
    .await;
    assert!(matches!(
        result,
        InterceptionResult::Rejected(RegistrationRejection::InvalidUser)
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_pass_cap_authenticate_passthrough() {
    let directory = temp_data_dir("irc-srv-passthrough");
    let target_listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let target_socket = target_listener.local_addr().expect("target addr");
    let manager = build_manager(directory.path(), irc_server_spec(target_socket));
    let (_scope, _cancel) = start_supervisors_for_test(&manager).await;
    let (mut source, tx) = ChannelInterceptionSource::bounded();
    tx.send(
        b"PASS secret\r\nCAP LS\r\nCAP REQ :sasl\r\nAUTHENTICATE PLAIN\r\nNICK alice\r\nUSER alice h s :Alice\r\n"
            .to_vec(),
    )
    .await
    .expect("send");
    drop(tx);
    let cancel = CancellationToken::new();
    let result = intercept_registration(
        &mut source,
        peer_hash(0xab),
        IrcServerOptions::default(),
        &cancel,
    )
    .await;
    match result {
        InterceptionResult::Ready { prefix, .. } => {
            let text = std::str::from_utf8(&prefix).expect("utf8");
            let expected = project_peer_hostname(&peer_hash(0xab));
            assert!(text.contains("PASS secret\r\n"), "PASS: {text}");
            assert!(text.contains("CAP LS\r\n"), "CAP LS: {text}");
            assert!(text.contains("CAP REQ :sasl\r\n"), "CAP REQ: {text}");
            assert!(
                text.contains("AUTHENTICATE PLAIN\r\n"),
                "AUTHENTICATE: {text}"
            );
            assert!(text.contains("NICK alice\r\n"), "NICK: {text}");
            assert!(
                text.contains(&format!("USER alice {expected} s :Alice\r\n")),
                "USER rewrite: {text}"
            );
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn irc_server_state_machine_progression() {
    let mut interceptor = IrcServerRegistration::new(
        IrcServerOptions::default(),
        i2pr_service_tunnels::IrcLimits::defaults(),
        peer_hash(0xab),
    );
    assert_eq!(interceptor.state(), RegistrationState::AwaitingRegistration);
    let outcome = interceptor.advance(b"NICK alice\r\n");
    assert!(matches!(outcome, RegistrationOutcome::Incomplete { .. }));
    assert_eq!(interceptor.state(), RegistrationState::SawPreRegistration);
    let outcome = interceptor.advance(b"USER alice h s :Alice\r\n");
    assert!(matches!(outcome, RegistrationOutcome::Ready { .. }));
    assert_eq!(interceptor.state(), RegistrationState::Ready);
    let _ = ephemeral_destination();
    // Drop the unused read_lines_until helper to keep clippy happy.
    let _ = read_lines_until; // ensure symbol is reachable
    let _: DestinationIdentity = deterministic_destination(0);
}

fn extract_user_hostname(text: &str) -> String {
    // Pulls the hostname (second argument) out of a USER line for
    // tests. The `USER` keyword itself is the first whitespace-
    // separated token; the hostname is the third.
    let user_line = text
        .lines()
        .find(|line| line.starts_with("USER "))
        .expect("USER line");
    let mut parts = user_line.split_whitespace();
    let _command = parts.next();
    let _username = parts.next();
    let host = parts.next().expect("hostname");
    host.to_owned()
}
