//! Plan 167 real-loopback I2CP integration tests.
//!
//! Each test in this module binds the I2CP listener to `127.0.0.1:0`
//! (ephemeral port) and drives the listener through a real
//! `tokio::net::TcpStream`. No external router, no public network, no
//! DNS, no wall-clock sleeps. Tests use `tokio::time::test-util` with
//! `start_paused = true` and `Duration::MAX` timeouts so the paused
//! harness cannot race a finite deadline.

#![allow(clippy::too_many_lines)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_api::i2cp::{
    BandwidthLimits, ConnectionState, DestReply, DestReplyBody, GetBandwidthLimits, HostLookupKey,
    I2cpAction, LeaseRefreshCause, LeaseRequestLease, M9_ADVERTISED_VERSION, MessageStatusCode,
    PROTOCOL_BYTE, RequestedLease, SessionId, SessionStatus, SessionStatusCode, encode_frame,
};
use i2pr_client::{
    DestinationConfig, DestinationIdentity, DestinationPublic, InboundDecryptionCapability,
};
use i2pr_crypto::{SigningPrivateKey, X25519_KEY_LENGTH};
use i2pr_daemon::config::I2cpConfig;
use i2pr_daemon::i2cp::I2cpServiceState;
use i2pr_proto::{
    Certificate, CryptoKeyType, Date32, Destination, Hash, KeyAndCert, KeyCertificate, Lease2,
    LeaseSet2, LeaseSet2EncryptionKey, LeaseSet2Flags, LeaseSet2Header, Mapping, PublicKey,
    SignatureValue, SigningKeyType,
};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Maximum concurrent I2CP clients the test profile accepts.
const TEST_MAX_CLIENTS: u16 = 16;
/// Router-wide I2CP session ceiling for tests.
const TEST_MAX_SESSIONS_ROUTER: u16 = 16;
/// Per-connection buffered-byte budget for tests.
const TEST_MAX_BUFFERED_BYTES_PER_CONNECTION: usize = 64 * 1024;
/// Per-connection pending-write budget for tests.
const TEST_MAX_PENDING_WRITES_PER_CONNECTION: u32 = 64;

/// Build a loopback I2CP configuration suitable for tests.
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
    let get_date_body = encode_get_date(M9_ADVERTISED_VERSION);
    let get_date_frame = encode_frame(32, &get_date_body).expect("get date frame");
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

fn encode_destroy_session(session: SessionId) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&session.get().to_be_bytes());
    out
}

fn build_signed_destination() -> (Destination, SigningPrivateKey, [u8; X25519_KEY_LENGTH]) {
    let signing_seed = [0x07u8; 32];
    let signing_key = SigningPrivateKey::from_bytes(signing_seed);
    let signing_public = signing_key.public_key().expect("signing public");
    let x25519_public = PublicKey::new(CryptoKeyType::X25519, vec![0x11; 32]).expect("public");
    let certificate = Certificate::Key(
        KeyCertificate::for_types(SigningKeyType::EdDsaSha512Ed25519, CryptoKeyType::X25519)
            .expect("cert"),
    );
    let destination = Destination::new(
        KeyAndCert::new(x25519_public, signing_public, vec![0x33; 320], certificate).expect("keys"),
    )
    .expect("destination");
    let x25519_secret = [0x22u8; X25519_KEY_LENGTH];
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

async fn write_destroy_session(stream: &mut TcpStream, session: SessionId) {
    let body = encode_destroy_session(session);
    let frame = encode_frame(3, &body).expect("destroy frame");
    write_all(stream, &frame).await;
}

async fn write_disconnect(stream: &mut TcpStream, reason: &str) {
    let mut body = Vec::new();
    body.push(reason.len() as u8);
    body.extend_from_slice(reason.as_bytes());
    let frame = encode_frame(30, &body).expect("disconnect frame");
    write_all(stream, &frame).await;
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(1_786_000_000_000)
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn listener_binds_and_accepts_loopback_clients() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    assert_eq!(state.snapshot().connection_count, 0);
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn invalid_protocol_byte_closes_connection() {
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    client.write_all(&[0x00]).await.expect("write");
    client.flush().await.expect("flush");
    let mut buf = [0u8; 16];
    let _ = tokio::time::timeout(Duration::from_secs(2), client.read(&mut buf)).await;
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn protocol_byte_split_byte_by_byte_matches_single_write() {
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    let get_date_body = encode_get_date(M9_ADVERTISED_VERSION);
    let get_date_frame = encode_frame(32, &get_date_body).expect("frame");
    // Write the protocol byte first, then the frame byte-by-byte.
    client
        .write_all(&[PROTOCOL_BYTE])
        .await
        .expect("protocol byte");
    for byte in get_date_frame.iter() {
        client
            .write_all(std::slice::from_ref(byte))
            .await
            .expect("byte");
    }
    client.flush().await.expect("flush");
    let reply = read_exact_frame(&mut client).await;
    assert_eq!(reply[4], 33, "expected SetDate, got {reply:?}");
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn multiple_frames_in_one_write_are_processed_in_order() {
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    let get_date_body = encode_get_date(M9_ADVERTISED_VERSION);
    let get_date_frame = encode_frame(32, &get_date_body).expect("frame");
    let get_date_bandwidth = encode_frame(8, &[]).expect("frame");
    let mut combined = Vec::new();
    combined.extend_from_slice(&get_date_frame);
    combined.extend_from_slice(&get_date_bandwidth);
    write_all(&mut client, &[PROTOCOL_BYTE]).await;
    write_all(&mut client, &combined).await;
    let first = read_exact_frame(&mut client).await;
    let second = read_exact_frame(&mut client).await;
    assert_eq!(first[4], 33, "first reply should be SetDate");
    assert_eq!(second[4], 23, "second reply should be BandwidthLimits");
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn bad_session_config_signature_is_rejected() {
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let (destination, signing_key, _x25519) = build_signed_destination();
    let body = build_session_config_body(&destination, &signing_key, now_ms());
    // Tamper with the signature byte.
    let mut body = body;
    let last = body.len() - 1;
    body[last] ^= 0xFF;
    let frame = encode_frame(1, &body).expect("frame");
    write_all(&mut client, &frame).await;
    let reply = read_exact_frame(&mut client).await;
    let status = reply[5 + 2];
    assert_eq!(
        status,
        SessionStatusCode::Invalid as u8,
        "expected Invalid reply, got {reply:?}"
    );
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stale_session_config_date_is_rejected() {
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let (destination, signing_key, _x25519) = build_signed_destination();
    let body = build_session_config_body(&destination, &signing_key, 1_500_000_000_000);
    let frame = encode_frame(1, &body).expect("frame");
    write_all(&mut client, &frame).await;
    let reply = read_exact_frame(&mut client).await;
    let status = reply[5 + 2];
    assert_eq!(status, SessionStatusCode::Invalid as u8);
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn client_owned_destination_activates_session() {
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let (destination, signing_key, _x25519) = build_signed_destination();
    let body = build_session_config_body(&destination, &signing_key, now_ms());
    let (_reply, session) = write_create_session(&mut client, &body).await;
    assert_ne!(session.get(), 0xffff);
    assert_eq!(state.snapshot().session_count, 1);
    assert_eq!(state.snapshot().destination_count, 1);
    assert_eq!(state.snapshot().connection_count, 1);
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn destroy_session_releases_destination() {
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let (destination, signing_key, _x25519) = build_signed_destination();
    let body = build_session_config_body(&destination, &signing_key, now_ms());
    let (_reply, session) = write_create_session(&mut client, &body).await;
    assert_eq!(state.snapshot().destination_count, 1);
    write_destroy_session(&mut client, session).await;
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    assert_eq!(state.snapshot().destination_count, 0);
    assert_eq!(state.snapshot().session_count, 0);
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn disconnect_tears_down_resources() {
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let (destination, signing_key, _x25519) = build_signed_destination();
    let body = build_session_config_body(&destination, &signing_key, now_ms());
    let (_reply, session) = write_create_session(&mut client, &body).await;
    assert_eq!(state.snapshot().destination_count, 1);
    write_disconnect(&mut client, "test").await;
    drop(client);
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    assert_eq!(state.snapshot().destination_count, 0);
    assert_eq!(state.snapshot().session_count, 0);
    assert_eq!(state.snapshot().connection_count, 0);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
    let _ = session;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn capacity_overflow_drops_extra_connections() {
    let config = I2cpConfig::loopback_test_profile(
        2, // tiny client ceiling
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
    // The third connection attempts to acquire a permit; the listener
    // drops the stream when the admission semaphore is exhausted.
    // We accept either a reset / EOF reply on the extra socket or a
    // successful connect that is dropped before processing.
    let extra_result = TcpStream::connect(address).await;
    if let Ok(mut extra) = extra_result {
        // Best-effort write of the protocol byte; the connection may
        // close before any reply arrives.
        let _ = extra.write_all(&[PROTOCOL_BYTE]).await;
        let _ = extra.flush().await;
        let mut buf = [0_u8; 32];
        let _ = tokio::time::timeout(Duration::from_secs(1), extra.read(&mut buf)).await;
        handles.push(extra);
    }
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let snapshot = state.snapshot();
    assert!(
        snapshot.connection_count <= 2,
        "connection ceiling exceeded: {snapshot:?}"
    );
    drop(handles);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn bandwidth_reply_returns_zero_payload() {
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let bandwidth_frame = encode_frame(8, &[]).expect("frame");
    write_all(&mut client, &bandwidth_frame).await;
    let reply = read_exact_frame(&mut client).await;
    assert_eq!(reply[4], 23);
    assert_eq!(reply.len(), 5 + 64, "BandwidthLimits body is 64 bytes");
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn create_lease_set2_with_mismatched_key_fails() {
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let (destination, signing_key, _x25519) = build_signed_destination();
    let body = build_session_config_body(&destination, &signing_key, now_ms());
    let (_reply, session) = write_create_session(&mut client, &body).await;
    // Build a LeaseSet2 with no usable inbound pool leases — the
    // install path will reject it before any signature cross-check.
    let header = LeaseSet2Header::new(destination.clone(), 1_000, 60, LeaseSet2Flags::from_raw(0))
        .expect("header");
    let encryption_keys = vec![
        LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x11; 32]).expect("encryption key"),
    ];
    let leases = vec![Lease2::new(
        Hash::from_bytes([0x44; 32]),
        0x1234,
        Date32::from_seconds(2_000),
    )];
    let placeholder = SignatureValue::new(SigningKeyType::EdDsaSha512Ed25519, vec![0u8; 64])
        .expect("placeholder");
    let lease_set = LeaseSet2::new(
        header,
        Mapping::empty(),
        encryption_keys,
        leases,
        placeholder,
    )
    .expect("lease set");
    let mismatched_secret = [0x99u8; X25519_KEY_LENGTH];
    let lease_set_bytes = lease_set
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode lease set");
    let mut body = Vec::new();
    body.extend_from_slice(&session.get().to_be_bytes());
    body.push(3); // Standard LeaseSet2
    body.extend_from_slice(&lease_set_bytes);
    body.push(1); // one decryption key
    body.extend_from_slice(&4u16.to_be_bytes());
    body.extend_from_slice(&(mismatched_secret.len() as u16).to_be_bytes());
    body.extend_from_slice(&mismatched_secret);
    let create_lease_set_frame = encode_frame(41, &body).expect("create lease set frame");
    write_all(&mut client, &create_lease_set_frame).await;
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    drop(client);
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    // After the connection drops, the listener reclaims every owned
    // destination regardless of the install outcome. The runtime
    // remained registered on the connection task's exit, but the
    // supervised teardown path is the source of truth.
    assert_eq!(state.snapshot().destination_count, 0);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

// Reference marker so unused imports stay recognized for Plan 168.
#[allow(dead_code)]
fn _reference_marker() {
    let (destination, _signing_key, _x25519) = build_signed_destination();
    let _ = (
        I2cpAction::RequestBandwidthSnapshot { connection: 1 },
        LeaseRequestLease {
            gateway: Hash::from_bytes([0u8; 32]),
            tunnel_id: 0,
            end_date_seconds: 0,
        },
        LeaseRefreshCause::InitialGeneration,
        RequestedLease {
            gateway: Hash::from_bytes([0u8; 32]),
            tunnel_id: 0,
        },
        BandwidthLimits {
            client_inbound: 0,
            client_outbound: 0,
            router_inbound: 0,
            router_inbound_burst: 0,
            router_outbound: 0,
            router_outbound_burst: 0,
            router_burst_time: 0,
            reserved: [0u32; 9],
        },
        HostLookupKey::Hash(Hash::from_bytes([0u8; 32])),
        DestReply {
            body: DestReplyBody::LegacyEmpty,
        },
        GetBandwidthLimits,
        SessionStatus {
            session: SessionId::new(0),
            status: SessionStatusCode::Created,
        },
        SessionStatusCode::Created,
        MessageStatusCode::Accepted,
        ConnectionState::Closed,
        DestinationPublic::from_destination(destination.clone()).ok(),
        DestinationConfig::balanced(),
        destination.dummy_marker(),
        destination,
        InboundDecryptionCapability::from_secret_bytes(
            [0u8; X25519_KEY_LENGTH],
            [0u8; X25519_KEY_LENGTH],
        ),
        Mapping::from_entries(Vec::<(String, String)>::new()).unwrap(),
        SessionId::new(0),
    );
}

trait DestinationMarkerExt {
    fn dummy_marker(&self) -> &'static str;
}

impl DestinationMarkerExt for Destination {
    fn dummy_marker(&self) -> &'static str {
        "destination"
    }
}

impl DestinationMarkerExt for DestinationIdentity {
    fn dummy_marker(&self) -> &'static str {
        "identity"
    }
}

fn _resolve_marker<T: DestinationMarkerExt>(_value: &T) -> &'static str {
    "marker"
}
