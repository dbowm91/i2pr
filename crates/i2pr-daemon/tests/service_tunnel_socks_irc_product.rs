//! Plan 290 §socksirc black-box product tests.
//!
//! The SOCKS IRC profile composes SOCKS negotiation (SOCKS5 or
//! SOCKS4a) with the IRC client privacy filter: after CONNECT
//! succeeds every byte in both directions passes the filter, with
//! no raw bypass mode. Tests drive behavior only through TCP after
//! listener startup against a scripted IRC server fixture behind a
//! paired generic server:
//!
//! - SOCKS5 negotiate + IRC register (NICK/USER with hostname
//!   rewrite) + `PRIVMSG ACTION` round-trip with digest equality;
//! - SOCKS4a negotiate + register parity;
//! - `DCC SEND` dropped (fixture observes nothing);
//! - raw non-IRC bytes dropped without breaking the connection
//!   (proves no raw bypass: a later valid line still arrives);
//! - server-to-client numeric passes, unknown server command
//!   dropped;
//! - disallowed CONNECT port rejected with the version-appropriate
//!   reply (`0x02` / 91);
//! - sibling connections are isolated.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, IrcClientOptions, LocalListenerSpec, ServerTarget,
    ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet, ServiceTunnelSpec,
    Socks5ClientOptions, StaticAliasTable,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use i2pr_daemon::service_tunnels::{ServiceTunnelManager, ServiceTunnelManagerConfig};

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

fn socks_irc_spec(destination: DestinationRef) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse("alpha-socksirc").expect("id"),
        kind: ServiceTunnelKind::SocksIrc,
        enabled: true,
        listener: Some(LocalListenerSpec::parse_socket("127.0.0.1:0").expect("listener")),
        target: None,
        targets: Vec::new(),
        destination: Some(destination),
        policy: DestinationPolicy::Dedicated,
        max_connections: 8,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: ServiceTimeouts::defaults(),
        http_options: None,
        socks5_options: Some(Socks5ClientOptions::defaults()),
        irc_options: Some(IrcClientOptions::default()),
        connect_options: None,
    }
}

fn server_spec(id: &str, target: SocketAddr) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("id"),
        kind: ServiceTunnelKind::GenericServer,
        enabled: true,
        listener: None,
        target: Some(ServerTarget::LoopbackTcp(target)),
        targets: Vec::new(),
        destination: None,
        policy: DestinationPolicy::Dedicated,
        max_connections: 8,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: ServiceTimeouts::defaults(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
        connect_options: None,
    }
}

fn build_manager(
    data_dir: &Path,
    specs: Vec<ServiceTunnelSpec>,
    aliases: StaticAliasTable,
) -> Arc<ServiceTunnelManager> {
    let manager_config = ServiceTunnelManagerConfig {
        data_dir: data_dir.to_path_buf(),
        aggregate_connection_ceiling: 64,
        per_service_connection_ceiling: 16,
        specs: Arc::new(ServiceTunnelSet { tunnels: specs }),
        aliases: Arc::new(aliases),
    };
    Arc::new(ServiceTunnelManager::new(manager_config).expect("manager builds"))
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

/// Scripted IRC server fixture: records every received line on the
/// channel and emits one scripted server-to-client line per
/// received line. Accepts connections in a loop for sibling tests.
fn start_irc_fixture(
    script: Vec<Vec<u8>>,
) -> (
    SocketAddr,
    mpsc::UnboundedReceiver<String>,
    tokio::task::JoinHandle<()>,
) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.set_nonblocking(true).expect("nonblocking");
    let addr = listener.local_addr().expect("addr");
    let listener = TcpListener::from_std(listener).expect("tokio listener");
    let (observed_tx, observed_rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let observed = observed_tx.clone();
            let script = script.clone();
            tokio::spawn(async move {
                let mut buffer = Vec::new();
                let mut chunk = [0_u8; 1024];
                let mut scripted = script.into_iter();
                loop {
                    match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
                        .await
                    {
                        Ok(Ok(0)) | Err(_) => break,
                        Ok(Ok(n)) => {
                            buffer.extend_from_slice(&chunk[..n]);
                            while let Some(end) =
                                buffer.windows(2).position(|window| window == b"\r\n")
                            {
                                let line = String::from_utf8_lossy(&buffer[..end]).into_owned();
                                buffer.drain(..end + 2);
                                let _ = observed.send(line);
                                if let Some(reply) = scripted.next()
                                    && stream.write_all(&reply).await.is_err()
                                {
                                    break;
                                }
                            }
                        }
                        Ok(Err(_)) => break,
                    }
                }
            });
        }
    });
    (addr, observed_rx, task)
}

/// Builds a paired manager whose `alias` resolves to the generic
/// server destination in front of the IRC fixture.
async fn build_paired_manager(
    data_dir: &Path,
    alias: &str,
    fixture_addr: SocketAddr,
) -> Arc<ServiceTunnelManager> {
    let probe = build_manager(
        data_dir,
        vec![server_spec("alpha-server", fixture_addr)],
        StaticAliasTable::new(),
    );
    probe.prepare().await.expect("probe prepare");
    let server_b64 = probe
        .service_destination_b64("alpha-server")
        .expect("server b64");
    drop(probe);
    let destination = DestinationRef::ConfiguredDestination(server_b64);
    let mut aliases = StaticAliasTable::new();
    aliases
        .insert(alias, destination.clone())
        .expect("alias insert");
    build_manager(
        data_dir,
        vec![
            socks_irc_spec(destination),
            server_spec("alpha-server", fixture_addr),
        ],
        aliases,
    )
}

fn build_socks5_greeting() -> Vec<u8> {
    vec![0x05, 0x01, 0x00]
}

fn build_socks5_connect(host: &str, port: u16) -> Vec<u8> {
    let mut bytes = vec![0x05, 0x01, 0x00, 0x03];
    bytes.push(host.len() as u8);
    bytes.extend_from_slice(host.as_bytes());
    bytes.extend_from_slice(&port.to_be_bytes());
    bytes
}

fn build_socks4a_connect(host: &str, port: u16) -> Vec<u8> {
    let mut bytes = vec![0x04, 0x01];
    bytes.extend_from_slice(&port.to_be_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 1]);
    bytes.extend_from_slice(b"socksirc");
    bytes.push(0);
    bytes.extend_from_slice(host.as_bytes());
    bytes.push(0);
    bytes
}

async fn read_exact_bounded(stream: &mut TcpStream, expected: usize) -> Vec<u8> {
    let mut out = vec![0_u8; expected];
    let mut filled = 0_usize;
    let started = std::time::Instant::now();
    while filled < expected {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "timed out reading {expected} bytes (got {filled})"
        );
        match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut out[filled..])).await {
            Ok(Ok(0)) => panic!("EOF after {filled}/{expected} bytes"),
            Ok(Ok(n)) => filled += n,
            Ok(Err(error)) => panic!("read error: {error}"),
            Err(_) => panic!("read timed out"),
        }
    }
    out
}

async fn read_line_bounded(stream: &mut TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 256];
    let started = std::time::Instant::now();
    loop {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "timed out reading a line"
        );
        match tokio::time::timeout(Duration::from_secs(2), stream.read(&mut chunk)).await {
            Ok(Ok(0)) => panic!("EOF while reading a line"),
            Ok(Ok(n)) => {
                buffer.extend_from_slice(&chunk[..n]);
                if let Some(end) = buffer.windows(2).position(|w| w == b"\r\n") {
                    return String::from_utf8_lossy(&buffer[..end]).into_owned();
                }
            }
            _ => panic!("read failed while reading a line"),
        }
    }
}

async fn next_observed(rx: &mut mpsc::UnboundedReceiver<String>) -> String {
    tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("line arrives")
        .expect("channel open")
}

async fn no_observed_within(rx: &mut mpsc::UnboundedReceiver<String>) {
    assert!(
        tokio::time::timeout(Duration::from_millis(800), rx.recv())
            .await
            .is_err(),
        "no line must arrive within the quiet window"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn socks_irc_register_and_privmsg_roundtrip() {
    let directory = temp_data_dir("socksirc-roundtrip");
    let script = vec![b":irc.test 001 alice :welcome\r\n".to_vec()];
    let (fixture_addr, mut observed, _fixture) = start_irc_fixture(script);
    let manager = build_paired_manager(directory.path(), "irc-target.i2p", fixture_addr).await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socksirc")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&build_socks5_greeting())
        .await
        .expect("greeting");
    let greeting_reply = read_exact_bounded(&mut stream, 2).await;
    assert_eq!(greeting_reply, vec![0x05, 0x00]);
    stream
        .write_all(&build_socks5_connect("irc-target.i2p", 443))
        .await
        .expect("connect");
    let reply = read_exact_bounded(&mut stream, 10).await;
    assert_eq!(reply[0], 0x05, "success version: {reply:?}");
    assert_eq!(reply[1], 0x00, "success code: {reply:?}");
    // IRC register: NICK passes, USER hostname/servername are
    // rewritten to stable placeholders (leaked values absent).
    stream
        .write_all(b"NICK alice\r\nUSER alice leaked-host.example leaked-server :Alice Real\r\n")
        .await
        .expect("register");
    assert_eq!(next_observed(&mut observed).await, "NICK alice");
    let user = next_observed(&mut observed).await;
    assert_eq!(user, "USER alice i2p localhost :Alice Real");
    assert!(!user.contains("leaked"));
    // Server-to-client numeric passes the inbound filter.
    let welcome = read_line_bounded(&mut stream).await;
    assert_eq!(welcome, ":irc.test 001 alice :welcome");
    // CTCP ACTION passes with digest equality.
    let action = "PRIVMSG #chan :\x01ACTION waves\x01\r\n";
    stream.write_all(action.as_bytes()).await.expect("action");
    assert_eq!(
        next_observed(&mut observed).await,
        "PRIVMSG #chan :\x01ACTION waves\x01"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn socks_irc_dcc_is_dropped() {
    let directory = temp_data_dir("socksirc-dcc");
    let (fixture_addr, mut observed, _fixture) = start_irc_fixture(Vec::new());
    let manager = build_paired_manager(directory.path(), "irc-target.i2p", fixture_addr).await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socksirc")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&build_socks5_greeting())
        .await
        .expect("greeting");
    assert_eq!(read_exact_bounded(&mut stream, 2).await, vec![0x05, 0x00]);
    stream
        .write_all(&build_socks5_connect("irc-target.i2p", 443))
        .await
        .expect("connect");
    let reply = read_exact_bounded(&mut stream, 10).await;
    assert_eq!(reply[1], 0x00);
    // Address-bearing DCC is dropped: the fixture observes nothing.
    stream
        .write_all(b"PRIVMSG victim :\x01DCC SEND secret.txt 3232235777 0 1234\x01\r\n")
        .await
        .expect("dcc");
    no_observed_within(&mut observed).await;
}

#[tokio::test(flavor = "current_thread")]
async fn socks_irc_raw_bypass_is_impossible() {
    let directory = temp_data_dir("socksirc-bypass");
    let (fixture_addr, mut observed, _fixture) = start_irc_fixture(Vec::new());
    let manager = build_paired_manager(directory.path(), "irc-target.i2p", fixture_addr).await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socksirc")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&build_socks5_greeting())
        .await
        .expect("greeting");
    assert_eq!(read_exact_bounded(&mut stream, 2).await, vec![0x05, 0x00]);
    stream
        .write_all(&build_socks5_connect("irc-target.i2p", 443))
        .await
        .expect("connect");
    let reply = read_exact_bounded(&mut stream, 10).await;
    assert_eq!(reply[1], 0x00);
    // Raw non-IRC bytes after CONNECT are dropped, not tunneled.
    stream
        .write_all(b"GET / HTTP/1.0\r\nHost: x\r\n\r\n")
        .await
        .expect("raw");
    no_observed_within(&mut observed).await;
    // The connection is still usable for real IRC: no bypass, no breakage.
    stream.write_all(b"JOIN #chan\r\n").await.expect("join");
    assert_eq!(next_observed(&mut observed).await, "JOIN #chan");
}

#[tokio::test(flavor = "current_thread")]
async fn socks_irc_unknown_server_command_is_dropped() {
    let directory = temp_data_dir("socksirc-inbound");
    let script = vec![
        b":irc.test 001 alice :welcome\r\n".to_vec(),
        b"EVILCOMMAND some-arg\r\n".to_vec(),
        b":irc.test 002 alice :again\r\n".to_vec(),
    ];
    let (fixture_addr, mut observed, _fixture) = start_irc_fixture(script);
    let manager = build_paired_manager(directory.path(), "irc-target.i2p", fixture_addr).await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socksirc")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&build_socks5_greeting())
        .await
        .expect("greeting");
    assert_eq!(read_exact_bounded(&mut stream, 2).await, vec![0x05, 0x00]);
    stream
        .write_all(&build_socks5_connect("irc-target.i2p", 443))
        .await
        .expect("connect");
    assert_eq!(read_exact_bounded(&mut stream, 10).await[1], 0x00);
    // Trigger three fixture lines with three filter-passing
    // client lines; the unknown server command in the middle must
    // not arrive. (One PING rewrite token is outstanding at a
    // time, so the triggers use NICK/JOIN/plain-PRIVMSG rather
    // than stacked PINGs.)
    stream
        .write_all(b"NICK alice\r\nJOIN #chan\r\nPRIVMSG #chan :hello\r\n")
        .await
        .expect("trigger");
    assert_eq!(next_observed(&mut observed).await, "NICK alice");
    assert_eq!(next_observed(&mut observed).await, "JOIN #chan");
    assert_eq!(next_observed(&mut observed).await, "PRIVMSG #chan :hello");
    assert_eq!(
        read_line_bounded(&mut stream).await,
        ":irc.test 001 alice :welcome"
    );
    assert_eq!(
        read_line_bounded(&mut stream).await,
        ":irc.test 002 alice :again"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn socks_irc_socks4a_negotiation_parity() {
    let directory = temp_data_dir("socksirc-4a");
    let (fixture_addr, mut observed, _fixture) = start_irc_fixture(Vec::new());
    let manager = build_paired_manager(directory.path(), "irc-target.i2p", fixture_addr).await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socksirc")
        .expect("listener");
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    // No greeting on the 4a path: the request starts immediately.
    stream
        .write_all(&build_socks4a_connect("irc-target.i2p", 443))
        .await
        .expect("4a connect");
    let reply = read_exact_bounded(&mut stream, 8).await;
    assert_eq!(reply[0], 0x00, "4a reply version: {reply:?}");
    assert_eq!(reply[1], 90, "4a granted: {reply:?}");
    stream
        .write_all(b"NICK bob\r\nUSER bob h s :Bob\r\n")
        .await
        .expect("register");
    assert_eq!(next_observed(&mut observed).await, "NICK bob");
    let user = next_observed(&mut observed).await;
    assert_eq!(user, "USER bob i2p localhost :Bob");
}

#[tokio::test(flavor = "current_thread")]
async fn socks_irc_disallowed_port_replies_per_version() {
    let directory = temp_data_dir("socksirc-port");
    let (fixture_addr, _observed, _fixture) = start_irc_fixture(Vec::new());
    let manager = build_paired_manager(directory.path(), "irc-target.i2p", fixture_addr).await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socksirc")
        .expect("listener");
    // SOCKS5 framing: 0x02 connection-not-allowed.
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&build_socks5_greeting())
        .await
        .expect("greeting");
    assert_eq!(read_exact_bounded(&mut stream, 2).await, vec![0x05, 0x00]);
    stream
        .write_all(&build_socks5_connect("irc-target.i2p", 80))
        .await
        .expect("connect");
    let reply = read_exact_bounded(&mut stream, 10).await;
    assert_eq!(reply[1], 0x02, "port policy rejects: {reply:?}");
    // SOCKS4a framing: 91 rejected.
    let mut stream = TcpStream::connect(listener).await.expect("connect");
    stream
        .write_all(&build_socks4a_connect("irc-target.i2p", 80))
        .await
        .expect("4a connect");
    let reply = read_exact_bounded(&mut stream, 8).await;
    assert_eq!(reply, vec![0x00, 91, 0, 0, 0, 0, 0, 0]);
}

#[tokio::test(flavor = "current_thread")]
async fn socks_irc_siblings_are_isolated() {
    let directory = temp_data_dir("socksirc-siblings");
    let (fixture_addr, mut observed, _fixture) = start_irc_fixture(Vec::new());
    let manager = build_paired_manager(directory.path(), "irc-target.i2p", fixture_addr).await;
    let (_scope, _cancel) = start_supervisors(&manager).await;
    let listener = manager
        .client_listener_address("alpha-socksirc")
        .expect("listener");
    let mut first = TcpStream::connect(listener).await.expect("first");
    let mut second = TcpStream::connect(listener).await.expect("second");
    for stream in [&mut first, &mut second] {
        stream
            .write_all(&build_socks5_greeting())
            .await
            .expect("greeting");
        assert_eq!(read_exact_bounded(stream, 2).await, vec![0x05, 0x00]);
        stream
            .write_all(&build_socks5_connect("irc-target.i2p", 443))
            .await
            .expect("connect");
        assert_eq!(read_exact_bounded(stream, 10).await[1], 0x00);
    }
    first.write_all(b"NICK first\r\n").await.expect("nick");
    second.write_all(b"NICK second\r\n").await.expect("nick");
    let mut seen = vec![
        next_observed(&mut observed).await,
        next_observed(&mut observed).await,
    ];
    seen.sort();
    assert_eq!(seen, vec!["NICK first", "NICK second"]);
}
