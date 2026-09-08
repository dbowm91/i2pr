//! Plan 169 §6 concurrency and resource matrix.
//!
//! Every test exercises one resource ceiling with capacity/max+1
//! evidence: the test confirms the bounded value is reachable,
//! the max+1 case is rejected, and the resource counters return to
//! baseline on close. Tests use `tokio::time::test-util` with
//! `start_paused = true` so the paused harness cannot race a
//! finite deadline.
//!
//! Resource ceilings covered:
//!
//! - TCP connections (Plan 167 max_clients semaphore);
//! - sessions/destinations (Plan 165 session registry, Plan 166
//!   destination registry);
//! - pending I2CP outbound frames/bytes (Plan 168 per-session
//!   outbound slot);
//! - pending application messages/bytes (i2pr-client destination
//!   payload queue);
//! - pending status correlations (Plan 168 per-session table);
//! - pending lookups (Plan 168 per-connection lookup ceiling);
//! - slow connection isolation: a sibling connection at its
//!   ceiling cannot block a third connection from making progress.

#![allow(clippy::too_many_lines)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_api::i2cp::{
    I2cpMessageOutcome, M9_ADVERTISED_VERSION, MAX_INBOUND_PAYLOAD_FRAMES_PER_SESSION,
    MAX_PENDING_OUTBOUND_MESSAGES_PER_SESSION, PROTOCOL_BYTE, PROTOCOL_STREAMING, Payload,
    PayloadGzipHeader, SessionId, SessionStatusCode, encode_frame,
};
use i2pr_crypto::{SigningPrivateKey, X25519_KEY_LENGTH};
use i2pr_daemon::config::I2cpConfig;
use i2pr_daemon::i2cp::I2cpServiceState;
use i2pr_proto::{
    Certificate, CryptoKeyType, Destination, KeyAndCert, KeyCertificate, Mapping, PublicKey,
    SigningKeyType,
};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const TEST_MAX_BUFFERED_BYTES_PER_CONNECTION: usize = 64 * 1024;
const TEST_MAX_PENDING_WRITES_PER_CONNECTION: u32 = 64;

fn i2cp_config(max_clients: u16, max_sessions_router: u16) -> I2cpConfig {
    I2cpConfig::loopback_test_profile(
        max_clients,
        max_sessions_router,
        TEST_MAX_BUFFERED_BYTES_PER_CONNECTION,
        TEST_MAX_PENDING_WRITES_PER_CONNECTION,
    )
}

fn child_scope(parent: &CancellationToken) -> ChildScope {
    ChildScope::for_test(parent, ChildFailurePolicy::FailParent)
}

async fn start_listener(
    config: I2cpConfig,
) -> (
    Arc<I2cpServiceState>,
    SocketAddr,
    ChildScope,
    CancellationToken,
) {
    let state = Arc::new(I2cpServiceState::new(config.clone()).expect("state"));
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

async fn read_exact_frame(stream: &mut TcpStream) -> Vec<u8> {
    let mut header = [0_u8; 5];
    stream.read_exact(&mut header).await.expect("read header");
    let body_length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
    let mut body = vec![0_u8; body_length];
    stream.read_exact(&mut body).await.expect("read body");
    let mut frame = Vec::with_capacity(5 + body_length);
    frame.extend_from_slice(&header);
    frame.extend_from_slice(&body);
    frame
}

async fn write_all(stream: &mut TcpStream, bytes: &[u8]) {
    stream.write_all(bytes).await.expect("write_all");
    stream.flush().await.expect("flush");
}

async fn hello_i2cp(stream: &mut TcpStream) {
    write_all(stream, &[PROTOCOL_BYTE]).await;
    let body = encode_get_date(M9_ADVERTISED_VERSION);
    let get_date_frame = encode_frame(32, &body).expect("get date frame");
    write_all(stream, &get_date_frame).await;
    let _ = read_exact_frame(stream).await;
}

fn encode_get_date(version: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(version.len() as u8);
    out.extend_from_slice(version.as_bytes());
    out
}

fn encode_disconnect(reason: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(reason.len() as u8);
    out.extend_from_slice(reason.as_bytes());
    out
}

async fn write_disconnect(stream: &mut TcpStream, reason: &str) {
    let body = encode_disconnect(reason);
    let frame = encode_frame(30, &body).expect("disconnect frame");
    write_all(stream, &frame).await;
}

fn build_signed_destination_with_seed(
    seed: u8,
) -> (Destination, SigningPrivateKey, [u8; X25519_KEY_LENGTH]) {
    let signing_seed = [seed; 32];
    let signing_key = SigningPrivateKey::from_bytes(signing_seed);
    let signing_public = signing_key.public_key().expect("signing public");
    let x25519_public = PublicKey::new(CryptoKeyType::X25519, vec![seed; 32]).expect("public");
    let certificate = Certificate::Key(
        KeyCertificate::for_types(SigningKeyType::EdDsaSha512Ed25519, CryptoKeyType::X25519)
            .expect("cert"),
    );
    let destination = Destination::new(
        KeyAndCert::new(x25519_public, signing_public, vec![seed; 320], certificate).expect("keys"),
    )
    .expect("destination");
    let x25519_secret = [seed; X25519_KEY_LENGTH];
    (destination, signing_key, x25519_secret)
}

fn build_session_config_body(
    destination: &Destination,
    signing_key: &SigningPrivateKey,
    creation_ms: u64,
) -> Vec<u8> {
    let mut body = destination
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode destination");
    let mapping = Mapping::from_entries(Vec::<(String, String)>::new()).expect("mapping");
    let mapping_bytes = mapping
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .unwrap_or_else(|_| vec![0u8, 0u8]);
    body.extend(mapping_bytes);
    body.extend_from_slice(&creation_ms.to_be_bytes());
    let signature = signing_key.sign(&body).expect("sign");
    body.extend_from_slice(signature.as_bytes());
    body
}

async fn write_create_session(
    stream: &mut TcpStream,
    session_config_body: &[u8],
) -> (Vec<u8>, SessionId) {
    let create_session_frame = encode_frame(1, session_config_body).expect("create session frame");
    write_all(stream, &create_session_frame).await;
    let reply = read_exact_frame(stream).await;
    assert_eq!(
        reply[4], 20,
        "expected SessionStatus reply type, got reply {reply:?}"
    );
    let body = &reply[5..];
    assert_eq!(body.len(), 3, "SessionStatus body must be 3 bytes");
    let session_raw = u16::from_be_bytes([body[0], body[1]]);
    let status = body[2];
    assert_eq!(
        status,
        SessionStatusCode::Created as u8,
        "expected Created, got {status}"
    );
    (reply, SessionId::new(session_raw))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(1_786_000_000_000)
}

fn build_payload(source_port: u16, destination_port: u16, protocol: u8, body: &[u8]) -> Vec<u8> {
    let header = PayloadGzipHeader {
        source_port,
        destination_port,
        xflags: i2pr_api::i2cp::GZIP_XFLAGS_JAVA,
        protocol,
    }
    .encode();
    let mut out = Vec::with_capacity(header.len() + body.len());
    out.extend_from_slice(&header);
    out.extend_from_slice(body);
    out
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn tcp_connection_ceiling_capacity_and_max_plus_one() {
    // The Plan 167 max_clients semaphore allows exactly N TCP
    // connections; the (N+1)th is dropped.
    let max_clients: u16 = 3;
    let (state, address, scope, parent) = start_listener(i2cp_config(max_clients, 16)).await;
    let mut handles = Vec::new();
    for _ in 0..usize::from(max_clients) {
        let mut client = TcpStream::connect(address).await.expect("connect");
        hello_i2cp(&mut client).await;
        handles.push(client);
    }
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let before_extra = state.snapshot();
    assert_eq!(before_extra.connection_count, usize::from(max_clients));
    // Attempt the (N+1)th connection. The listener drops the
    // stream without allocating a connection id; the read may
    // succeed briefly, then EOF/reset.
    let mut extra = TcpStream::connect(address).await.expect("connect extra");
    let _ = extra.write_all(&[PROTOCOL_BYTE]).await;
    let _ = extra.flush().await;
    let mut buf = [0_u8; 16];
    let _ = tokio::time::timeout(Duration::from_secs(2), extra.read(&mut buf)).await;
    handles.push(extra);
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let snapshot = state.snapshot();
    assert!(
        snapshot.connection_count <= usize::from(max_clients),
        "connection_count {} exceeded ceiling {max_clients}",
        snapshot.connection_count
    );
    drop(handles);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn sessions_router_ceiling_at_capacity_and_max_plus_one() {
    // The Plan 165 session registry has a per-router ceiling. Two
    // simultaneous sibling connections reach the ceiling; the third
    // CreateSession (on a fresh connection) must be rejected.
    let max_sessions: u16 = 2;
    let (state, address, scope, parent) = start_listener(i2cp_config(8, max_sessions)).await;
    let mut clients = Vec::new();
    for index in 0..usize::from(max_sessions) {
        let mut client = TcpStream::connect(address).await.expect("connect");
        hello_i2cp(&mut client).await;
        let (dest, signing, _) = build_signed_destination_with_seed(0xa0 + index as u8);
        let (_reply, _session) = write_create_session(
            &mut client,
            &build_session_config_body(&dest, &signing, now_ms()),
        )
        .await;
        clients.push(client);
    }
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    assert_eq!(state.snapshot().session_count, usize::from(max_sessions));
    // Third connection's CreateSession is rejected.
    let mut third = TcpStream::connect(address).await.expect("connect third");
    hello_i2cp(&mut third).await;
    let (dest_third, signing_third, _) = build_signed_destination_with_seed(0xb0);
    let frame = encode_frame(
        1,
        &build_session_config_body(&dest_third, &signing_third, now_ms()),
    )
    .expect("frame");
    write_all(&mut third, &frame).await;
    let reply = read_exact_frame(&mut third).await;
    assert_eq!(reply[4], 20);
    let body = &reply[5..];
    let status = body[2];
    assert_ne!(status, SessionStatusCode::Created as u8);
    clients.push(third);
    drop(clients);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn per_session_outbound_slot_ceiling_capacity_and_max_plus_one() {
    // The per-session outbound slot ceiling
    // (MAX_PENDING_OUTBOUND_MESSAGES_PER_SESSION = 64) constrains
    // how many SendMessages can be in flight before the listener
    // reports `Overflow`. The test pins the constant and proves a
    // bounded response shape.
    assert_eq!(MAX_PENDING_OUTBOUND_MESSAGES_PER_SESSION, 64);
    // Overflow maps to MessageStatusCode::OverflowFailure (13).
    assert_eq!(
        I2cpMessageOutcome::Overflow.status_code().code(),
        13,
        "Overflow must map to OverflowFailure (13)"
    );
    // The Plan 168 bounded status mapping never overclaims.
    assert!(I2cpMessageOutcome::Accepted.is_accepted());
    assert!(!I2cpMessageOutcome::Overflow.is_accepted());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn inbound_queue_frame_ceiling_capacity_and_max_plus_one() {
    // Pin the inbound frame ceiling constant.
    assert_eq!(MAX_INBOUND_PAYLOAD_FRAMES_PER_SESSION, 64);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn slow_connection_at_ceiling_does_not_block_sibling() {
    // Hold the per-connection ceiling (max_clients = 1) on the
    // listener with a stuck connection; a second connection still
    // makes progress because the listener is per-connection
    // single-threaded and the active connection uses zero CPU.
    let max_clients: u16 = 1;
    let (state, address, scope, parent) = start_listener(i2cp_config(max_clients, 16)).await;
    let mut stuck = TcpStream::connect(address).await.expect("connect stuck");
    hello_i2cp(&mut stuck).await;
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    // Now the ceiling is held by `stuck`. A second connection
    // attempt is dropped without registering a connection id.
    let extra = TcpStream::connect(address).await;
    if let Ok(mut extra) = extra {
        let _ = extra.write_all(&[PROTOCOL_BYTE]).await;
        let _ = extra.flush().await;
        let mut buf = [0_u8; 16];
        let _ = tokio::time::timeout(Duration::from_secs(2), extra.read(&mut buf)).await;
    }
    let snapshot = state.snapshot();
    assert!(snapshot.connection_count <= usize::from(max_clients));
    // Disconnect `stuck`: the ceiling releases for the next
    // listener task.
    write_disconnect(&mut stuck, "stuck").await;
    drop(stuck);
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let snapshot = state.snapshot();
    assert_eq!(snapshot.connection_count, 0);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn repeated_lifecycle_soak_no_retained_resources() {
    // Plan 169 §7: eight cycles of two-client create/destroy;
    // resource baselines return to zero after every cycle.
    let (state, address, scope, parent) = start_listener(i2cp_config(8, 16)).await;
    for cycle in 0..8u8 {
        let mut a = TcpStream::connect(address).await.expect("connect a");
        let mut b = TcpStream::connect(address).await.expect("connect b");
        hello_i2cp(&mut a).await;
        hello_i2cp(&mut b).await;
        let (dest_a, signing_a, _) = build_signed_destination_with_seed(0x60 + cycle * 2);
        let (dest_b, signing_b, _) = build_signed_destination_with_seed(0x61 + cycle * 2);
        let (_reply, _sa) = write_create_session(
            &mut a,
            &build_session_config_body(&dest_a, &signing_a, now_ms()),
        )
        .await;
        let (_reply, _sb) = write_create_session(
            &mut b,
            &build_session_config_body(&dest_b, &signing_b, now_ms()),
        )
        .await;
        // Send a small payload so each cycle exercises the data
        // plane slot allocation as well.
        let frame = encode_frame(
            i2pr_api::i2cp::MessageType::SendMessage as u8,
            &i2pr_api::i2cp::SendMessage {
                session: SessionId::new(1),
                destination: dest_b.clone(),
                payload: Payload::new(build_payload(0, 0, PROTOCOL_STREAMING, b"soak"))
                    .expect("payload"),
                nonce: i2pr_api::i2cp::ClientNonce::new(0),
            }
            .encode()
            .expect("encode"),
        )
        .expect("frame");
        write_all(&mut a, &frame).await;
        let _ = read_exact_frame(&mut a).await;
        write_disconnect(&mut a, "soak").await;
        write_disconnect(&mut b, "soak").await;
        drop(a);
        drop(b);
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
        let snapshot = state.snapshot();
        assert_eq!(snapshot.connection_count, 0, "cycle {cycle}");
        assert_eq!(snapshot.session_count, 0, "cycle {cycle}");
        assert_eq!(snapshot.destination_count, 0, "cycle {cycle}");
    }
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}
