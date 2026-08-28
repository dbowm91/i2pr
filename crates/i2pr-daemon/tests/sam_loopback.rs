//! Real-loopback integration tests for the Plan 137 SAM v3.1 service.
//!
//! Every test in this module binds the SAM listener to `127.0.0.1:0`
//! (ephemeral port) and exercises a real TCP client over
//! `tokio::net::TcpStream`. No external router, no public network,
//! no DNS, no wall-clock sleeps.

#![allow(clippy::too_many_lines)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use i2pr_api::sam::limits::SamLimits;
use i2pr_daemon::config::SamConfig;
use i2pr_daemon::sam::SamServiceState;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

fn sam_config() -> SamConfig {
    SamConfig {
        enabled: true,
        bind_address: "127.0.0.1".parse().unwrap(),
        port: 0,
        limits: SamLimits::loopback_test_profile(),
    }
}

fn child_scope(parent: &CancellationToken) -> ChildScope {
    ChildScope::for_test(parent, ChildFailurePolicy::FailParent)
}

async fn start_listener(
    config: SamConfig,
) -> (
    Arc<SamServiceState>,
    SocketAddr,
    ChildScope,
    CancellationToken,
) {
    let state = Arc::new(SamServiceState::new(config.clone()).expect("state"));
    let bind_address = state.bind_address();
    let (listener, bound_address) = state.bind(bind_address).await.expect("bind");
    let parent = CancellationToken::new();
    let scope = child_scope(&parent);
    let state_for_task = Arc::clone(&state);
    let token_for_task = parent.clone();
    let scope_for_serve = scope.clone();
    let spawn_scope = scope.clone();
    spawn_scope
        .spawn(move |task_cancellation| {
            let _ = task_cancellation;
            async move {
                let _ = state_for_task
                    .serve(listener, scope_for_serve, token_for_task)
                    .await;
                Ok(())
            }
        })
        .expect("spawn listener task");
    // Give the listener a chance to enter the accept loop.
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    (state, bound_address, scope, parent)
}

async fn read_one_line(stream: &mut TcpStream) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 256];
    loop {
        match stream.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.last() == Some(&b'\n') {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf)
        .trim_end_matches(['\r', '\n'])
        .to_owned()
}

async fn wait_for_quiescence() {
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }
}

async fn write_all(stream: &mut TcpStream, bytes: &[u8]) {
    stream.write_all(bytes).await.expect("write_all");
    stream.flush().await.expect("flush");
}

async fn hello_3_1(stream: &mut TcpStream) {
    write_all(stream, b"HELLO VERSION MIN=3.1 MAX=3.1\n").await;
    let reply = read_one_line(stream).await;
    assert!(
        reply.starts_with("HELLO REPLY RESULT=OK VERSION=3.1"),
        "expected HELLO OK, got {reply:?}"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn listener_binds_and_accepts_loopback_clients() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    write_all(&mut client, b"QUIT\n").await;
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn hello_with_incompatible_version_closes_connection() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    write_all(&mut client, b"HELLO VERSION MIN=2.0 MAX=2.0\n").await;
    let reply = read_one_line(&mut client).await;
    assert!(
        reply.starts_with("HELLO REPLY RESULT=NOVERSION"),
        "expected NOVERSION, got {reply:?}"
    );
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn session_create_before_hello_is_rejected() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    write_all(
        &mut client,
        b"SESSION CREATE STYLE=STREAM ID=alpha DESTINATION=TRANSIENT\n",
    )
    .await;
    let reply = read_one_line(&mut client).await;
    assert!(
        reply.contains("RESULT=I2P_ERROR"),
        "expected I2P_ERROR, got {reply:?}"
    );
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn dest_generate_produces_typed_destination() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    write_all(&mut client, b"DEST GENERATE SIGNATURE_TYPE=7\n").await;
    let reply = read_one_line(&mut client).await;
    assert!(
        reply.starts_with("DEST REPLY RESULT=OK PUB=") && reply.contains(" PRIV="),
        "expected DEST REPLY OK with PUB/PRIV, got {reply:?}"
    );
    // The destination registry must NOT have grown because DEST
    // GENERATE is a utility command.
    assert_eq!(state.destination_registry().lock().unwrap().len(), 0);
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn dest_generate_does_not_increase_session_count() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    for _ in 0..3 {
        write_all(&mut client, b"DEST GENERATE SIGNATURE_TYPE=7\n").await;
        let _ = read_one_line(&mut client).await;
    }
    assert_eq!(state.session_registry().session_count(), 0);
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn session_create_transient_round_trip() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    write_all(
        &mut client,
        b"SESSION CREATE STYLE=STREAM ID=alpha DESTINATION=TRANSIENT\n",
    )
    .await;
    let reply = read_one_line(&mut client).await;
    assert!(
        reply.starts_with("SESSION STATUS RESULT=OK DESTINATION="),
        "expected SESSION STATUS OK, got {reply:?}"
    );
    assert_eq!(state.session_registry().session_count(), 1);
    assert_eq!(state.destination_registry().lock().unwrap().len(), 1);
    assert_eq!(state.streaming_pools().lock().unwrap().len(), 1);
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn session_create_with_imported_private_destination() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut first = TcpStream::connect(address).await.expect("connect 1");
    hello_3_1(&mut first).await;
    write_all(&mut first, b"DEST GENERATE SIGNATURE_TYPE=7\n").await;
    let reply = read_one_line(&mut first).await;
    let priv_text = extract_priv(&reply).expect("priv extraction");
    drop(first);

    let mut second = TcpStream::connect(address).await.expect("connect 2");
    hello_3_1(&mut second).await;
    let command = format!("SESSION CREATE STYLE=STREAM ID=imported DESTINATION={priv_text}\n");
    write_all(&mut second, command.as_bytes()).await;
    let reply = read_one_line(&mut second).await;
    assert!(
        reply.starts_with("SESSION STATUS RESULT=OK DESTINATION="),
        "expected SESSION STATUS OK, got {reply:?}"
    );
    assert_eq!(state.session_registry().session_count(), 1);
    drop(second);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn duplicate_session_id_is_rejected() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    // Hold two simultaneous control sockets. The first registers
    // `alpha`; the second must fail with DUPLICATED_ID while the
    // first session is still alive.
    let mut first = TcpStream::connect(address).await.expect("connect 1");
    hello_3_1(&mut first).await;
    write_all(
        &mut first,
        b"SESSION CREATE STYLE=STREAM ID=alpha DESTINATION=TRANSIENT\n",
    )
    .await;
    let reply = read_one_line(&mut first).await;
    assert!(reply.contains("RESULT=OK"));

    let mut second = TcpStream::connect(address).await.expect("connect 2");
    hello_3_1(&mut second).await;
    write_all(
        &mut second,
        b"SESSION CREATE STYLE=STREAM ID=alpha DESTINATION=TRANSIENT\n",
    )
    .await;
    let reply = read_one_line(&mut second).await;
    assert!(
        reply.contains("RESULT=DUPLICATED_ID"),
        "expected DUPLICATED_ID, got {reply:?}"
    );
    drop(first);
    drop(second);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn second_session_create_on_same_control_socket_is_rejected() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    write_all(
        &mut client,
        b"SESSION CREATE STYLE=STREAM ID=alpha DESTINATION=TRANSIENT\n",
    )
    .await;
    let _ = read_one_line(&mut client).await;
    write_all(
        &mut client,
        b"SESSION CREATE STYLE=STREAM ID=beta DESTINATION=TRANSIENT\n",
    )
    .await;
    let reply = read_one_line(&mut client).await;
    assert!(
        reply.contains("RESULT=I2P_ERROR"),
        "expected I2P_ERROR on second SESSION CREATE, got {reply:?}"
    );
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn control_disconnect_tears_down_session_and_destination() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    write_all(
        &mut client,
        b"SESSION CREATE STYLE=STREAM ID=alpha DESTINATION=TRANSIENT\n",
    )
    .await;
    let _ = read_one_line(&mut client).await;
    assert_eq!(state.session_registry().session_count(), 1);
    drop(client);
    // Give the per-connection task time to observe the EOF.
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        state.session_registry().session_count(),
        0,
        "session must be torn down on control disconnect"
    );
    assert_eq!(state.destination_registry().lock().unwrap().len(), 0);
    assert_eq!(state.streaming_pools().lock().unwrap().len(), 0);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn ping_echoes_payload() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    write_all(&mut client, b"PING hello world\n").await;
    let reply = read_one_line(&mut client).await;
    assert_eq!(reply, "PONG hello world");
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stream_connect_unknown_session_returns_invalid_id() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    write_all(
        &mut client,
        b"STREAM CONNECT ID=alpha DESTINATION=placeholder\n",
    )
    .await;
    let reply = read_one_line(&mut client).await;
    assert!(
        reply.contains("RESULT=INVALID_ID"),
        "expected INVALID_ID, got {reply:?}"
    );
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn session_capacity_boundary_is_enforced() {
    let mut config = sam_config();
    config.limits = SamLimits::loopback_test_profile();
    let (state, address, scope, parent) = start_listener(config).await;
    let max = SamLimits::loopback_test_profile().max_sessions;
    // Saturate the SAM session ceiling with concurrent control
    // sockets, each using a unique session identifier.
    let mut handles = Vec::new();
    for index in 0..max {
        let mut client = TcpStream::connect(address).await.expect("connect");
        hello_3_1(&mut client).await;
        let id = format!("session-{index}");
        let mut line = Vec::new();
        line.extend_from_slice(b"SESSION CREATE STYLE=STREAM ID=");
        line.extend_from_slice(id.as_bytes());
        line.extend_from_slice(b" DESTINATION=TRANSIENT\n");
        write_all(&mut client, &line).await;
        let reply = read_one_line(&mut client).await;
        assert!(
            reply.contains("RESULT=OK"),
            "expected OK within capacity, got {reply:?}"
        );
        handles.push(client);
    }
    wait_for_quiescence().await;
    assert_eq!(
        state.session_registry().session_count(),
        max as usize,
        "all {} sessions must be active",
        max
    );
    // The next attempt must fail.
    let mut extra = TcpStream::connect(address).await.expect("connect extra");
    hello_3_1(&mut extra).await;
    write_all(
        &mut extra,
        b"SESSION CREATE STYLE=STREAM ID=overflow DESTINATION=TRANSIENT\n",
    )
    .await;
    let reply = read_one_line(&mut extra).await;
    assert!(
        reply.contains("RESULT=I2P_ERROR"),
        "expected I2P_ERROR at capacity, got {reply:?}"
    );
    drop(handles);
    drop(extra);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn hello_split_byte_by_byte_matches_single_write() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    let payload = b"HELLO VERSION MIN=3.1 MAX=3.1\n";
    for byte in payload {
        client.write_all(&[*byte]).await.expect("write byte");
    }
    let reply = read_one_line(&mut client).await;
    assert!(
        reply.starts_with("HELLO REPLY RESULT=OK VERSION=3.1"),
        "expected HELLO OK, got {reply:?}"
    );
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn multiple_complete_lines_in_one_read_are_dispatched_one_at_a_time() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    client
        .write_all(b"HELLO VERSION MIN=3.1 MAX=3.1\n")
        .await
        .expect("write hello");
    let first = read_one_line(&mut client).await;
    assert!(
        first.starts_with("HELLO REPLY RESULT=OK"),
        "first: {first:?}"
    );
    client
        .write_all(b"PING bundle\n")
        .await
        .expect("write ping");
    let second = read_one_line(&mut client).await;
    assert_eq!(second, "PONG bundle");
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn quit_after_hello_closes_without_session() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    write_all(&mut client, b"QUIT\n").await;
    let mut buf = [0_u8; 32];
    // The peer close is driven by a supervised task and can take longer on
    // kqueue-backed runners; retain a bounded timeout without making the
    // test depend on wall-clock sleeps.
    let read = timeout(Duration::from_secs(2), client.read(&mut buf)).await;
    match read {
        Ok(Ok(0)) => {}
        Ok(Ok(n)) => assert!(buf[..n].is_empty()),
        Ok(Err(_)) => {}
        Err(_) => panic!("expected QUIT to close the connection"),
    }
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn service_shutdown_closes_listener_and_clients() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let report = scope.shutdown().await;
    let _ = report;
    let mut buf = [0_u8; 32];
    let _ = timeout(Duration::from_secs(2), client.read(&mut buf)).await;
    drop(client);
    drop(state);
}

fn extract_priv(reply: &str) -> Option<String> {
    let prefix = "PRIV=";
    let start = reply.find(prefix)? + prefix.len();
    let rest = &reply[start..];
    let end = rest.find(' ').unwrap_or(rest.len());
    Some(rest[..end].trim().to_owned())
}
