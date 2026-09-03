//! Plan 149 §10 — self-composed SAM 3.1 black-box product test.
//!
//! This is the canonical Milestone 7 self-composition evidence. The
//! test drives the SAM listener through TCP and SAM protocol commands
//! alone. After service startup, the only Rust APIs the test invokes
//! are read/write on a `tokio::net::TcpStream` plus helpers that
//! parse the resulting reply lines. The test never calls:
//!
//! - `build_sam_destination_bridge`
//! - `SamDestinations::install`
//! - `SamDestinationBridge::install_inbound_tunnel_factory`
//! - `DestinationRuntime::new` / `with_shared_identity`
//! - `install_remote_lease_set2`
//! - `install_inbound_tunnel_factory`
//! - `spawn_destination_driver`
//! - `bridge_to_peer`
//! - `send_data_segment`
//! - `deliver_outbound`
//!
//! If any of those private APIs ever creep into the canonical
//! product evidence, this test must fail. That is the point: a real
//! external SAM client cannot reach them, so neither can this test.

#![allow(clippy::too_many_lines)]

use std::net::SocketAddr;
use std::sync::Arc;

use i2pr_api::sam::limits::SamLimits;
use i2pr_daemon::config::SamConfig;
use i2pr_daemon::sam::SamServiceState;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

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
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    (state, bound_address, scope, parent)
}

async fn read_one_line(stream: &mut TcpStream) -> String {
    let mut buf = Vec::new();
    loop {
        let mut byte = [0_u8; 1];
        match stream.read_exact(&mut byte).await {
            Ok(0) => break,
            Ok(_) => {
                buf.push(byte[0]);
                if byte[0] == b'\n' {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf)
        .into_owned()
        .trim_end_matches(['\r', '\n'])
        .to_owned()
}

async fn write_all(stream: &mut TcpStream, bytes: &[u8]) {
    stream.write_all(bytes).await.expect("write_all");
    stream.flush().await.expect("flush");
}

async fn read_n(stream: &mut TcpStream, expected: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(expected);
    while out.len() < expected {
        let remaining = expected - out.len();
        let mut chunk = vec![0_u8; remaining.min(16 * 1024)];
        let read = match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        out.extend_from_slice(&chunk[..read]);
    }
    out
}

async fn hello_3_1(stream: &mut TcpStream) {
    write_all(stream, b"HELLO VERSION MIN=3.1 MAX=3.1\n").await;
    let reply = read_one_line(stream).await;
    assert!(
        reply.starts_with("HELLO REPLY RESULT=OK VERSION=3.1"),
        "expected HELLO OK, got {reply:?}"
    );
}

async fn session_create(stream: &mut TcpStream, id: &str, destination: &str) -> (String, String) {
    let line = format!("SESSION CREATE STYLE=STREAM ID={id} DESTINATION={destination}\n");
    write_all(stream, line.as_bytes()).await;
    let reply = read_one_line(stream).await;
    assert!(
        reply.starts_with("SESSION STATUS RESULT=OK"),
        "SESSION CREATE for {id} did not return OK: {reply:?}"
    );
    assert_eq!(
        extract_value_field(&reply).as_deref(),
        Some(destination),
        "SESSION STATUS must return the imported/private destination"
    );
    // Per SAM 3.1, SESSION STATUS RESULT=OK DESTINATION=<priv> — retrieve
    // the public destination via NAMING LOOKUP NAME=ME so downstream
    // STREAM CONNECT commands can target the peer by public key.
    write_all(stream, b"NAMING LOOKUP NAME=ME\n").await;
    let naming_reply = read_one_line(stream).await;
    assert!(
        naming_reply.starts_with("NAMING REPLY RESULT=OK VALUE="),
        "NAMING LOOKUP NAME=ME after SESSION CREATE for {id} failed: {naming_reply:?}"
    );
    let pub_value = naming_reply
        .split_whitespace()
        .find_map(|tok| tok.strip_prefix("VALUE=").map(strip_sam_quotes))
        .expect("NAMING REPLY contains VALUE=<pub>");
    (reply, pub_value)
}

fn extract_value_field(reply: &str) -> Option<String> {
    for token in reply.split_whitespace() {
        if let Some(rest) = token.strip_prefix("DESTINATION=") {
            return Some(strip_sam_quotes(rest));
        }
    }
    None
}

fn strip_sam_quotes(value: &str) -> String {
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        value[1..value.len() - 1].to_owned()
    } else {
        value.to_owned()
    }
}

async fn transient_destination(address: SocketAddr) -> String {
    let mut helper = TcpStream::connect(address)
        .await
        .expect("transient connect");
    hello_3_1(&mut helper).await;
    write_all(&mut helper, b"DEST GENERATE SIGNATURE_TYPE=7\n").await;
    let reply = read_one_line(&mut helper).await;
    assert!(
        reply.starts_with("DEST REPLY RESULT=OK"),
        "DEST GENERATE failed: {reply:?}"
    );
    let mut priv_value = None;
    for token in reply.split_whitespace() {
        if let Some(rest) = token.strip_prefix("PRIV=") {
            priv_value = Some(rest.to_owned());
        }
    }
    let priv_value = priv_value.expect("PRIV= value present in DEST REPLY");
    drop(helper);
    priv_value
}

#[tokio::test(flavor = "current_thread")]
async fn plan149_self_composed_black_box_connects_and_transfers_bytes() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;

    let priv_a = transient_destination(address).await;
    let priv_b = transient_destination(address).await;
    assert_ne!(priv_a, priv_b, "transient destinations must differ");

    let mut client_a = TcpStream::connect(address).await.expect("connect a");
    let mut client_b = TcpStream::connect(address).await.expect("connect b");
    hello_3_1(&mut client_a).await;
    hello_3_1(&mut client_b).await;

    let (reply_a, pub_a) = session_create(&mut client_a, "alpha", &priv_a).await;
    let (reply_b, pub_b) = session_create(&mut client_b, "beta", &priv_b).await;
    assert!(
        reply_a.contains("DESTINATION="),
        "session A reply missing DESTINATION=: {reply_a:?}"
    );
    assert!(
        reply_b.contains("DESTINATION="),
        "session B reply missing DESTINATION=: {reply_b:?}"
    );

    let accept_task = tokio::spawn(async move {
        write_all(&mut client_b, b"STREAM ACCEPT ID=beta\n").await;
        let line = read_one_line(&mut client_b).await;
        let peer_line = read_one_line(&mut client_b).await;
        (client_b, line, peer_line)
    });
    let connect_task = tokio::spawn(async move {
        let cmd = format!("STREAM CONNECT ID=alpha DESTINATION={pub_b}\n");
        write_all(&mut client_a, cmd.as_bytes()).await;
        let line = read_one_line(&mut client_a).await;
        (client_a, line)
    });
    let (accept_result, connect_result) = tokio::join!(accept_task, connect_task);
    let (mut client_b, accept_line, accept_peer_line) = accept_result.expect("accept task");
    let (mut client_a, connect_line) = connect_result.expect("connect task");
    assert!(
        accept_line.starts_with("STREAM STATUS RESULT=OK"),
        "ACCEPT reply not OK: {accept_line:?}"
    );
    assert!(
        connect_line.starts_with("STREAM STATUS RESULT=OK"),
        "CONNECT reply not OK: {connect_line:?}"
    );
    assert!(
        accept_peer_line.starts_with("DESTINATION="),
        "ACCEPT peer destination line missing: {accept_peer_line:?}"
    );
    assert_eq!(
        extract_value_field(&accept_peer_line),
        Some(pub_a),
        "ACCEPT peer destination was not the authenticated CONNECT destination"
    );

    // Bidirectional byte exchange: the same shape as Plan 147 §10
    // but driven through the self-composed listener.
    let payload_a: Vec<u8> = (0..(2 * 1024 * 1024_u32))
        .map(|i| ((i.wrapping_mul(17) ^ i.rotate_left(7)) & 0xFF) as u8)
        .collect();
    let payload_b: Vec<u8> = (0..(2 * 1024 * 1024_u32))
        .map(|i| ((i.wrapping_mul(13) ^ i.rotate_left(11)) & 0xFF) as u8)
        .collect();
    let write_a = async {
        write_all(&mut client_a, &payload_a).await;
        read_n(&mut client_a, payload_b.len()).await
    };
    let write_b = async {
        write_all(&mut client_b, &payload_b).await;
        read_n(&mut client_b, payload_a.len()).await
    };
    let (received_a, received_b) = tokio::join!(write_a, write_b);
    assert_eq!(
        received_a.len(),
        payload_b.len(),
        "client_a received {} of {} bytes",
        received_a.len(),
        payload_b.len()
    );
    assert_eq!(
        received_b.len(),
        payload_a.len(),
        "client_b received {} of {} bytes",
        received_b.len(),
        payload_a.len()
    );
    assert_eq!(received_a, payload_b, "client_a payload mismatch");
    assert_eq!(received_b, payload_a, "client_b payload mismatch");

    drop(client_a);
    drop(client_b);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    assert_eq!(state.session_registry().session_count(), 0);
    assert_eq!(state.destination_registry().lock().unwrap().len(), 0);
    assert_eq!(state.streaming_pools().lock().unwrap().len(), 0);
    assert_eq!(state.stream_registry().active_session_count(), 0);
    assert_eq!(state.stream_registry().attachment_count(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn plan149_silent_connect_writes_no_status_line() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;

    let priv_a = transient_destination(address).await;
    let priv_b = transient_destination(address).await;

    let mut client_a = TcpStream::connect(address).await.expect("connect a");
    let mut client_b = TcpStream::connect(address).await.expect("connect b");
    hello_3_1(&mut client_a).await;
    hello_3_1(&mut client_b).await;
    let (_reply_a, _pub_a) = session_create(&mut client_a, "alpha", &priv_a).await;
    let (_, pub_b) = session_create(&mut client_b, "beta", &priv_b).await;
    write_all(&mut client_b, b"STREAM ACCEPT ID=beta SILENT=true\n").await;

    let accept_task = tokio::spawn(async move {
        let received = read_n(&mut client_b, 6).await;
        let response = [0x61_u8, 0x00, 0x62, 0x63, 0xFE, 0x64];
        write_all(&mut client_b, &response).await;
        (client_b, received, response)
    });
    let sentinel = [0x10_u8, 0x20, 0x30, 0x40, 0x50, 0x60];
    let mut same_read =
        format!("STREAM CONNECT ID=alpha DESTINATION={pub_b} SILENT=true\n").into_bytes();
    same_read.extend_from_slice(&sentinel);
    write_all(&mut client_a, &same_read).await;
    let (client_b, accepted_bytes, response) = accept_task.await.expect("accept task");
    assert_eq!(
        accepted_bytes, sentinel,
        "ACCEPT SILENT=true did not see raw sentinel bytes; got {accepted_bytes:?}"
    );
    let echoed = read_n(&mut client_a, response.len()).await;
    assert_eq!(
        echoed, response,
        "CONNECT SILENT=true emitted a status line before raw bytes"
    );

    drop(client_b);
    drop(client_a);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    assert_eq!(state.session_registry().session_count(), 0);
    assert_eq!(state.destination_registry().lock().unwrap().len(), 0);
    assert_eq!(state.streaming_pools().lock().unwrap().len(), 0);
    assert_eq!(state.stream_registry().active_session_count(), 0);
    assert_eq!(state.stream_registry().attachment_count(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn plan149_session_create_tears_down_cleanly() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    let priv_a = transient_destination(address).await;
    let (_reply, _pub_a) = session_create(&mut client, "alpha", &priv_a).await;
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let report = scope.shutdown().await;
    assert!(
        !report.failed(),
        "destination driver panicked during teardown: {report:?}"
    );
    assert_eq!(state.session_registry().session_count(), 0);
    assert_eq!(state.destination_registry().lock().unwrap().len(), 0);
    assert_eq!(state.streaming_pools().lock().unwrap().len(), 0);
    assert_eq!(state.stream_registry().active_session_count(), 0);
    assert_eq!(state.stream_registry().attachment_count(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn plan149_same_read_buffered_raw_bytes_after_command() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;

    let priv_a = transient_destination(address).await;
    let priv_b = transient_destination(address).await;

    let mut client_a = TcpStream::connect(address).await.expect("connect a");
    let mut client_b = TcpStream::connect(address).await.expect("connect b");
    hello_3_1(&mut client_a).await;
    hello_3_1(&mut client_b).await;
    let (_reply_a, _pub_a) = session_create(&mut client_a, "alpha", &priv_a).await;
    let (_reply_b, pub_b) = session_create(&mut client_b, "beta", &priv_b).await;

    let accept_task = tokio::spawn(async move {
        write_all(&mut client_b, b"STREAM ACCEPT ID=beta\n").await;
        let status = read_one_line(&mut client_b).await;
        let peer = read_one_line(&mut client_b).await;
        let response = [0xA1_u8, 0x00, 0xFE, b'\r', b'P', b'O'];
        write_all(&mut client_b, &response).await;
        let bytes = read_n(&mut client_b, 6).await;
        (client_b, status, peer, response, bytes)
    });
    let sentinel = [0x91_u8, 0x00, 0xFF, b'\n', b'P', b'I'];
    let mut same_read = format!("STREAM CONNECT ID=alpha DESTINATION={pub_b}\n").into_bytes();
    same_read.extend_from_slice(&sentinel);
    write_all(&mut client_a, &same_read).await;
    let status_a = read_one_line(&mut client_a).await;
    let bytes_a = read_n(&mut client_a, 6).await;
    let (client_b, status_b, peer_b, response, bytes_b) = accept_task.await.expect("accept task");
    assert!(
        status_a.starts_with("STREAM STATUS RESULT=OK"),
        "CONNECT status not OK: {status_a:?}"
    );
    assert!(
        status_b.starts_with("STREAM STATUS RESULT=OK"),
        "ACCEPT status not OK: {status_b:?}"
    );
    assert!(
        peer_b.starts_with("DESTINATION="),
        "ACCEPT peer destination missing: {peer_b:?}"
    );
    assert_eq!(bytes_a, response, "CONNECT raw response was not delivered");
    assert_eq!(
        bytes_b, sentinel,
        "ACCEPT same-read bytes were not delivered"
    );

    drop(client_a);
    drop(client_b);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    assert_eq!(state.session_registry().session_count(), 0);
    assert_eq!(state.destination_registry().lock().unwrap().len(), 0);
    assert_eq!(state.streaming_pools().lock().unwrap().len(), 0);
    assert_eq!(state.stream_registry().active_session_count(), 0);
    assert_eq!(state.stream_registry().attachment_count(), 0);
}
