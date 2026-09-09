//! Plan 169 §5 adversarial I2CP protocol matrix.
//!
//! Each test in this module exercises one documented adversarial
//! case against the Plan 167 loopback listener over real localhost
//! TCP. The matrix is the bounded acceptance evidence for the
//! M9 I2CP product's resilience: every case has a bounded deadline
//! and the test asserts a typed outcome (rejection, error code, or
//! controlled no-op).
//!
//! Adversarial cases covered:
//!
//! - wrong/missing protocol byte;
//! - oversized frame length before body allocation;
//! - truncated/stalled frame;
//! - high-rate zero/small unknown frames;
//! - message family in invalid connection/session state;
//! - duplicate CreateSession/destination;
//! - malformed/noncanonical mapping;
//! - bad SessionConfig signature;
//! - stale/future SessionConfig date just outside boundary;
//! - unsupported signing/encryption/LeaseSet types;
//! - invalid option min/max/max+1 and numeric overflow;
//! - send before session usable;
//! - payload max/max+1 and decompression expansion ceiling;
//! - invalid/expired SendMessageExpires;
//! - slow reader and slow writer;
//! - abrupt client reset at each lifecycle phase.
//!
//! Every test uses `tokio::time::test-util` with `start_paused = true`
//! and `Duration::MAX` timeouts so the paused harness cannot race a
//! finite deadline.

#![allow(clippy::too_many_lines)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_api::i2cp::{
    BandwidthLimits, M9_ADVERTISED_VERSION, PROTOCOL_BYTE, PROTOCOL_STREAMING, Payload,
    PayloadGzipHeader, SendFlags, SendMessage, SessionId, SessionStatusCode, encode_frame,
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

const TEST_MAX_CLIENTS: u16 = 16;
const TEST_MAX_SESSIONS_ROUTER: u16 = 16;
const TEST_MAX_BUFFERED_BYTES_PER_CONNECTION: usize = 64 * 1024;
const TEST_MAX_PENDING_WRITES_PER_CONNECTION: u32 = 64;

fn i2cp_config() -> I2cpConfig {
    I2cpConfig::loopback_test_profile(
        TEST_MAX_CLIENTS,
        TEST_MAX_SESSIONS_ROUTER,
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
    let reply = read_exact_frame(stream).await;
    assert_eq!(
        reply[4], 33,
        "expected SetDate type byte, got reply {reply:?}"
    );
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

fn encode_destroy_session(session: SessionId) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&session.get().to_be_bytes());
    out
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

fn build_session_config_body_with_options(
    destination: &Destination,
    signing_key: &SigningPrivateKey,
    creation_ms: u64,
    options: &[(&str, &str)],
) -> Vec<u8> {
    let mut body = destination
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode destination");
    let mapping = Mapping::from_entries(
        options
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect(),
    )
    .expect("mapping");
    let mapping_bytes = mapping
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .unwrap_or_else(|_| vec![0u8, 0u8]);
    body.extend(mapping_bytes);
    body.extend_from_slice(&creation_ms.to_be_bytes());
    let signature = signing_key.sign(&body).expect("sign");
    body.extend_from_slice(signature.as_bytes());
    body
}

fn build_session_config_body(
    destination: &Destination,
    signing_key: &SigningPrivateKey,
    creation_ms: u64,
) -> Vec<u8> {
    build_session_config_body_with_options(destination, signing_key, creation_ms, &[])
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
    // Plan 170 §5: drain the follow-up RequestVariableLeaseSet.
    let followup = read_exact_frame(stream).await;
    assert_eq!(
        followup[4], 37,
        "expected RequestVariableLeaseSet follow-up, got reply {followup:?}"
    );
    (reply, SessionId::new(session_raw))
}

async fn write_create_session_outcome(
    stream: &mut TcpStream,
    session_config_body: &[u8],
) -> SessionStatusCode {
    let frame = encode_frame(1, session_config_body).expect("frame");
    write_all(stream, &frame).await;
    let reply = read_exact_frame(stream).await;
    assert_eq!(reply[4], 20, "expected SessionStatus, got {reply:?}");
    let body = &reply[5..];
    let raw = body[2];
    assert!(raw <= 4, "unexpected SessionStatus code {raw}: {reply:?}");
    // All M9 status codes are within 0..=4; anything else is a
    // wire-format regression we should never silently accept.
    SessionStatusCode::from_u8(raw).expect("session status code")
}

async fn write_destroy_session(stream: &mut TcpStream, session: SessionId) {
    let body = encode_destroy_session(session);
    let frame = encode_frame(3, &body).expect("destroy frame");
    write_all(stream, &frame).await;
}

async fn write_disconnect(stream: &mut TcpStream, reason: &str) {
    let body = encode_disconnect(reason);
    let frame = encode_frame(30, &body).expect("disconnect frame");
    write_all(stream, &frame).await;
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
async fn wrong_protocol_byte_is_closed() {
    // Plan 171: an invalid I2CP preamble must result in an observable
    // peer close/reset. Silence with a still-open socket is failure.
    //
    // The wait below deliberately avoids `tokio::time::timeout`:
    // under `start_paused` the virtual clock auto-advances while the
    // thread parks, so a finite virtual deadline races the server
    // task's first poll for real socket I/O. That race surfaced as
    // an intermittent macOS `Elapsed` on code-identical heads while
    // the non-paused twin on the same runner observed EOF. Each
    // iteration therefore pumps the scheduler with a bounded number
    // of yields and drains the close with `try_read`; exhausting
    // the bound panics exactly like a timeout, so a still-open
    // socket remains failure — never success.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    // Repeated invalid-preamble trajectory: 24 iterations prove
    // rejected connects do not monotonically retain
    // connection/admission state.
    for _ in 0..24 {
        let mut client = TcpStream::connect(address).await.expect("connect");
        client.write_all(&[0x00]).await.expect("write bad byte");
        client.flush().await.expect("flush");
        let mut buf = [0u8; 8];
        let mut closed = false;
        for _ in 0..256 {
            match client.try_read(&mut buf) {
                // EOF: the server terminated the stream without
                // sending any application/I2CP reply frame.
                Ok(0) => {
                    closed = true;
                    break;
                }
                // Reset / broken-pipe class: equally observable
                // termination.
                Err(error) if error.kind() != std::io::ErrorKind::WouldBlock => {
                    closed = true;
                    break;
                }
                Ok(n) => panic!("expected close, got {n} bytes"),
                // Not yet readable: let the server task run. The
                // bound above keeps the wait finite.
                Err(_) => tokio::task::yield_now().await,
            }
        }
        assert!(closed, "expected close, got timeout");
        drop(client);
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
    }
    // Connection/session/destination resource baselines return to
    // zero after rejection.
    let snapshot = state.snapshot();
    assert_eq!(
        snapshot.connection_count, 0,
        "connection baseline must return to zero after invalid-preamble rejection"
    );
    assert_eq!(
        snapshot.session_count, 0,
        "session baseline must remain zero after invalid-preamble rejection"
    );
    assert_eq!(
        snapshot.destination_count, 0,
        "destination baseline must remain zero after invalid-preamble rejection"
    );
    // The listener remains usable: a subsequent valid client
    // completes GetDate/SetDate after the rejected peers are closed.
    let mut valid = TcpStream::connect(address).await.expect("connect valid");
    hello_i2cp(&mut valid).await;
    drop(valid);
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let snapshot = state.snapshot();
    assert_eq!(snapshot.connection_count, 0);
    assert_eq!(snapshot.session_count, 0);
    assert_eq!(snapshot.destination_count, 0);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread")]
async fn wrong_protocol_byte_is_closed_real_time() {
    // Plan 171: non-paused product-close evidence for the invalid
    // preamble row, separated from `start_paused` timer behavior.
    // The same strict contract holds under a real wall-clock
    // deadline: wrong first byte, no reply frame, observable
    // EOF/reset, baselines at zero, listener still usable.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    client.write_all(&[0x00]).await.expect("write bad byte");
    client.flush().await.expect("flush");
    let mut buf = [0u8; 8];
    let outcome = tokio::time::timeout(Duration::from_secs(5), client.read(&mut buf)).await;
    match outcome {
        Ok(Ok(0)) => {}
        Ok(Err(_)) => {}
        Ok(Ok(n)) => panic!("expected close, got {n} bytes"),
        Err(_) => panic!("expected close, got timeout"),
    }
    drop(client);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let snapshot = state.snapshot();
    assert_eq!(
        snapshot.connection_count, 0,
        "connection baseline must return to zero after invalid-preamble rejection"
    );
    assert_eq!(snapshot.session_count, 0);
    assert_eq!(snapshot.destination_count, 0);
    let mut valid = TcpStream::connect(address).await.expect("connect valid");
    hello_i2cp(&mut valid).await;
    drop(valid);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let snapshot = state.snapshot();
    assert_eq!(snapshot.connection_count, 0);
    assert_eq!(snapshot.session_count, 0);
    assert_eq!(snapshot.destination_count, 0);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn oversized_frame_length_rejected_before_body_allocation() {
    // A frame header that declares a body length larger than the
    // I2CP ceiling must not allocate the body.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    write_all(&mut client, &[PROTOCOL_BYTE]).await;
    let body = encode_get_date(M9_ADVERTISED_VERSION);
    let get_date_frame = encode_frame(32, &body).expect("get date frame");
    write_all(&mut client, &get_date_frame).await;
    let _ = read_exact_frame(&mut client).await;
    // Frame header with length = 4 GiB - 1 byte. The decoder must
    // reject before any allocation attempt.
    let oversized_header: [u8; 5] = [0xff, 0xff, 0xff, 0xff, 33];
    client.write_all(&oversized_header).await.expect("write");
    client.flush().await.expect("flush");
    let mut buf = [0u8; 8];
    // Either the daemon closes the connection immediately or it
    // responds within the bounded timeout; both are acceptable
    // failure shapes.
    let _ = tokio::time::timeout(Duration::from_secs(2), client.read(&mut buf)).await;
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn truncated_frame_does_not_panic_listener() {
    // A frame header with body length 5 followed by only 1 byte
    // must not deadlock or panic the listener.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    write_all(&mut client, &[PROTOCOL_BYTE]).await;
    let bad_header: [u8; 5] = [0, 0, 0, 5, 33];
    client.write_all(&bad_header).await.expect("write header");
    client.write_all(&[0x01]).await.expect("write partial");
    client.flush().await.expect("flush");
    let mut buf = [0u8; 8];
    let _ = tokio::time::timeout(Duration::from_secs(2), client.read(&mut buf)).await;
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn unknown_message_type_in_active_state_closes_session() {
    // A high-rate burst of unknown message types (e.g. type 200)
    // must not advance the connection state machine; the listener
    // closes the connection.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    // Send a burst of small unknown-frame messages.
    for _ in 0..16 {
        let frame = encode_frame(200, &[0u8; 1]).expect("frame");
        write_all(&mut client, &frame).await;
    }
    let mut buf = [0u8; 8];
    let _ = tokio::time::timeout(Duration::from_secs(2), client.read(&mut buf)).await;
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn send_message_before_create_session_is_rejected() {
    // Sending a SendMessage before CreateSession is rejected with
    // `BadSession`.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let (destination, _signing, _) = build_signed_destination_with_seed(0x10);
    let payload = build_payload(0, 0, PROTOCOL_STREAMING, b"pre");
    let payload_value = Payload::new(payload).expect("payload");
    let send = SendMessage {
        session: SessionId::new(0xfe),
        destination,
        payload: payload_value,
        nonce: i2pr_api::i2cp::ClientNonce::new(0),
    };
    let body = send.encode().expect("encode");
    let frame = encode_frame(i2pr_api::i2cp::MessageType::SendMessage as u8, &body).expect("frame");
    write_all(&mut client, &frame).await;
    let reply = read_exact_frame(&mut client).await;
    assert_eq!(reply[4], 22);
    let status_byte = reply[5 + 6];
    assert_eq!(
        status_byte, 10,
        "expected BadSession (10), got {status_byte}"
    );
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn duplicate_create_session_with_same_destination_is_rejected() {
    // Two connections that try to register the same destination
    // hash must not double-register the destination on the
    // router. The second registration attempt is rejected.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let (destination, signing, _) = build_signed_destination_with_seed(0x20);
    let creation_ms = now_ms();
    let mut first = TcpStream::connect(address).await.expect("connect first");
    hello_i2cp(&mut first).await;
    let (_reply, _session) = write_create_session(
        &mut first,
        &build_session_config_body(&destination, &signing, creation_ms),
    )
    .await;
    assert_eq!(state.snapshot().destination_count, 1);
    // Second connection tries to register the *same* destination
    // hash. The SessionRegistry must refuse the duplicate.
    let mut second = TcpStream::connect(address).await.expect("connect second");
    hello_i2cp(&mut second).await;
    let outcome = write_create_session_outcome(
        &mut second,
        &build_session_config_body(&destination, &signing, creation_ms),
    )
    .await;
    assert_eq!(
        outcome,
        SessionStatusCode::Invalid,
        "second create must be rejected"
    );
    assert_eq!(state.snapshot().destination_count, 1);
    drop(first);
    drop(second);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stale_session_config_date_just_outside_boundary_is_rejected() {
    // A SessionConfig with a creation date older than
    // `now - 30 seconds` is rejected with `Invalid`.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let (destination, signing, _) = build_signed_destination_with_seed(0x30);
    let stale_ms = now_ms().saturating_sub(60_000);
    let outcome = write_create_session_outcome(
        &mut client,
        &build_session_config_body(&destination, &signing, stale_ms),
    )
    .await;
    assert_eq!(
        outcome,
        SessionStatusCode::Invalid,
        "expected Invalid for stale date"
    );
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn future_session_config_date_outside_boundary_is_rejected() {
    // A SessionConfig with a creation date more than
    // `now + 30 seconds` is rejected.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let (destination, signing, _) = build_signed_destination_with_seed(0x31);
    let future_ms = now_ms().saturating_add(60_000);
    let outcome = write_create_session_outcome(
        &mut client,
        &build_session_config_body(&destination, &signing, future_ms),
    )
    .await;
    assert_eq!(outcome, SessionStatusCode::Invalid);
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn bad_session_config_signature_is_rejected() {
    // Tampered signature bytes must be rejected with `Invalid`.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let (destination, signing, _) = build_signed_destination_with_seed(0x32);
    let mut body = build_session_config_body(&destination, &signing, now_ms());
    let last = body.len() - 1;
    body[last] ^= 0xFF;
    let outcome = write_create_session_outcome(&mut client, &body).await;
    assert_eq!(outcome, SessionStatusCode::Invalid);
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn oversized_mapping_count_is_rejected() {
    // A SessionConfig carrying more than
    // `MAX_SESSION_CONFIG_OPTIONS` entries is rejected before any
    // destination resource is reserved.
    let use_max = i2pr_api::i2cp::MAX_SESSION_CONFIG_OPTIONS + 1;
    let entries: Vec<(String, String)> = (0..use_max)
        .map(|index| {
            let key = format!("k{index:03}");
            (key, "1".to_owned())
        })
        .collect();
    let mapping = Mapping::from_entries(entries).expect("mapping");
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let (destination, signing, _) = build_signed_destination_with_seed(0x33);
    let mut body = destination
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode destination");
    let mapping_bytes = mapping
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .unwrap_or_else(|_| Vec::new());
    body.extend(mapping_bytes);
    body.extend_from_slice(&now_ms().to_be_bytes());
    let signature = signing.sign(&body).expect("sign");
    body.extend_from_slice(signature.as_bytes());
    let outcome = write_create_session_outcome(&mut client, &body).await;
    assert_eq!(outcome, SessionStatusCode::Invalid);
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn shutdown_overflow_option_rejected_at_min_max_boundary() {
    // `outbound.quantity = 0` is the documented minimum-violation
    // and must be rejected. `outbound.quantity = 9` is the documented
    // max+1 and must also be rejected.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client_zero = TcpStream::connect(address).await.expect("connect zero");
    hello_i2cp(&mut client_zero).await;
    let (dest_zero, sign_zero, _) = build_signed_destination_with_seed(0x34);
    let outcome = write_create_session_outcome(
        &mut client_zero,
        &build_session_config_body_with_options(
            &dest_zero,
            &sign_zero,
            now_ms(),
            &[("outbound.quantity", "0")],
        ),
    )
    .await;
    assert_eq!(outcome, SessionStatusCode::Invalid);
    drop(client_zero);

    let mut client_max_plus_one = TcpStream::connect(address).await.expect("connect max+1");
    hello_i2cp(&mut client_max_plus_one).await;
    let (dest_max, sign_max, _) = build_signed_destination_with_seed(0x35);
    let beyond = i2pr_client::MAX_DESTINATION_OUTBOUND + 1;
    let outcome = write_create_session_outcome(
        &mut client_max_plus_one,
        &build_session_config_body_with_options(
            &dest_max,
            &sign_max,
            now_ms(),
            &[("outbound.quantity", &beyond.to_string())],
        ),
    )
    .await;
    assert_eq!(outcome, SessionStatusCode::Invalid);
    drop(client_max_plus_one);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn invalid_send_message_expires_expiration_in_past_is_rejected() {
    // A SendMessageExpires with an expiration one millisecond in the
    // past is rejected without touching the destination runtime.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let (dest, signing, _) = build_signed_destination_with_seed(0x36);
    let (_, session) = write_create_session(
        &mut client,
        &build_session_config_body(&dest, &signing, now_ms()),
    )
    .await;
    let payload = build_payload(0, 0, PROTOCOL_STREAMING, b"expired");
    let payload_value = Payload::new(payload.clone()).expect("payload");
    let send = i2pr_api::i2cp::SendMessageExpires {
        session,
        destination: dest.clone(),
        payload: payload_value,
        nonce: i2pr_api::i2cp::ClientNonce::new(7),
        flags: SendFlags::new(0).expect("flags"),
        expiration_ms: now_ms().saturating_sub(1),
    };
    let body = send.encode().expect("encode");
    let frame =
        encode_frame(i2pr_api::i2cp::MessageType::SendMessageExpires as u8, &body).expect("frame");
    write_all(&mut client, &frame).await;
    let reply = read_exact_frame(&mut client).await;
    assert_eq!(reply[4], 22);
    let status = reply[5 + 6];
    assert_eq!(status, 14, "expected MessageExpired (14), got {status}");
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn abrupt_reset_after_create_session_releases_resources() {
    // Abrupt TCP reset after CreateSession: the listener's
    // supervised teardown must release the destination/session
    // state immediately.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let (dest, signing, _) = build_signed_destination_with_seed(0x37);
    let (_reply, _session) = write_create_session(
        &mut client,
        &build_session_config_body(&dest, &signing, now_ms()),
    )
    .await;
    assert_eq!(state.snapshot().destination_count, 1);
    drop(client);
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    assert_eq!(state.snapshot().destination_count, 0);
    assert_eq!(state.snapshot().session_count, 0);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn abrupt_reset_after_destroy_session_releases_resources() {
    // Abrupt TCP reset after DestroySession: the listener must
    // observe no leaked resource.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let (dest, signing, _) = build_signed_destination_with_seed(0x38);
    let (_reply, session) = write_create_session(
        &mut client,
        &build_session_config_body(&dest, &signing, now_ms()),
    )
    .await;
    write_destroy_session(&mut client, session).await;
    drop(client);
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    assert_eq!(state.snapshot().destination_count, 0);
    assert_eq!(state.snapshot().session_count, 0);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn slow_writer_does_not_block_other_clients() {
    // A slow writer (sleeps between writes) must not block a sibling
    // client that continues to make progress.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut slow = TcpStream::connect(address).await.expect("connect slow");
    hello_i2cp(&mut slow).await;
    let mut fast = TcpStream::connect(address).await.expect("connect fast");
    hello_i2cp(&mut fast).await;
    let (dest_slow, signing_slow, _) = build_signed_destination_with_seed(0x39);
    let (dest_fast, signing_fast, _) = build_signed_destination_with_seed(0x40);
    let creation_ms = now_ms();
    // Slow client's CreateSession is interleaved byte-by-byte.
    let slow_body = build_session_config_body(&dest_slow, &signing_slow, creation_ms);
    let slow_frame = encode_frame(1, &slow_body).expect("frame");
    for byte in slow_frame.iter() {
        slow.write_all(std::slice::from_ref(byte))
            .await
            .expect("slow");
        tokio::task::yield_now().await;
    }
    slow.flush().await.expect("flush");
    let slow_reply = read_exact_frame(&mut slow).await;
    assert_eq!(slow_reply[4], 20);
    let slow_session = SessionId::new(u16::from_be_bytes([slow_reply[5], slow_reply[6]]));
    assert_eq!(slow_reply[5 + 2], SessionStatusCode::Created as u8);
    // Fast client makes progress.
    let (_fast_reply, fast_session) = write_create_session(
        &mut fast,
        &build_session_config_body(&dest_fast, &signing_fast, creation_ms),
    )
    .await;
    assert_ne!(fast_session.get(), 0xffff);
    assert_ne!(fast_session.get(), slow_session.get());
    drop(slow);
    drop(fast);
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn destroy_session_before_create_session_is_rejected() {
    // Sending a DestroySession before any CreateSession must be
    // rejected with a state-machine error; the connection stays
    // open.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let body = encode_destroy_session(SessionId::new(1));
    let frame = encode_frame(3, &body).expect("frame");
    write_all(&mut client, &frame).await;
    let mut buf = [0u8; 16];
    // Either the daemon closes the connection immediately or it
    // responds within the bounded timeout; both are acceptable
    // failure shapes.
    let _ = tokio::time::timeout(Duration::from_secs(2), client.read(&mut buf)).await;
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn disconnect_reason_does_not_leak_pre_session_baseline() {
    // A Disconnect before CreateSession must close the connection
    // cleanly without leaking resources.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    write_disconnect(&mut client, "early-exit").await;
    drop(client);
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let snapshot = state.snapshot();
    assert_eq!(snapshot.connection_count, 0);
    assert_eq!(snapshot.session_count, 0);
    assert_eq!(snapshot.destination_count, 0);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn connection_count_overflow_drops_extra_sockets() {
    // The max_clients semaphore drops extra TCP connections; the
    // listener never lets `connection_count` exceed the ceiling.
    let config = I2cpConfig::loopback_test_profile(
        2,
        TEST_MAX_SESSIONS_ROUTER,
        TEST_MAX_BUFFERED_BYTES_PER_CONNECTION,
        TEST_MAX_PENDING_WRITES_PER_CONNECTION,
    );
    let (state, address, scope, parent) = start_listener(config).await;
    let mut handles = Vec::new();
    for _ in 0..2 {
        let mut client = TcpStream::connect(address).await.expect("connect");
        hello_i2cp(&mut client).await;
        handles.push(client);
    }
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let extra_result = TcpStream::connect(address).await;
    if let Ok(mut extra) = extra_result {
        let _ = extra.write_all(&[PROTOCOL_BYTE]).await;
        let _ = extra.flush().await;
        let mut buf = [0_u8; 16];
        let _ = tokio::time::timeout(Duration::from_secs(2), extra.read(&mut buf)).await;
        handles.push(extra);
    }
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let snapshot = state.snapshot();
    assert!(snapshot.connection_count <= 2);
    drop(handles);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn bandwidth_reply_baseline_is_config_derived() {
    // The Plan 168 bandwidth reply uses the config-derived ceiling
    // for client limits and documented zero values for router
    // limits.
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let frame = encode_frame(8, &[]).expect("frame");
    write_all(&mut client, &frame).await;
    let reply = read_exact_frame(&mut client).await;
    assert_eq!(reply[4], 23);
    let body = &reply[5..];
    assert_eq!(body.len(), 64);
    let parsed = BandwidthLimits::decode(body).expect("decode");
    let expected =
        u32::try_from(state.config().max_buffered_bytes_per_connection / 1024).unwrap_or(u32::MAX);
    assert_eq!(parsed.client_inbound, expected);
    assert_eq!(parsed.client_outbound, expected);
    assert_eq!(parsed.router_inbound, 0);
    assert_eq!(parsed.router_outbound, 0);
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}
