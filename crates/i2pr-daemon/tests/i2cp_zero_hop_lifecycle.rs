//! Plan 172 local zero-hop LeaseSet2 lifecycle tests.
//!
//! Narrowly named Plan 172 suite (not an overload of the Plan 169
//! files). Every test binds the listener to `127.0.0.1:0` and drives
//! behavior only through TCP/I2CP inputs after startup, plus direct
//! runtime-neutral checks for option projection and pool bounds.
//!
//! Covered Plan 172 §14 items:
//! 1. empty RequestVariableLeaseSet never emitted for zero-hop;
//! 2. session not usable before CreateLeaseSet2 commits;
//! 11. allowZeroHop=false + length 0 rejects;
//! 12. remote constructors still reject empty hop lists;
//! 13. destroy releases zero-hop entries;
//! 14. sibling remains usable when one destroyed.

#![allow(clippy::too_many_lines)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use i2pr_api::i2cp::{
    M9_ADVERTISED_VERSION, PROTOCOL_BYTE, RequestVariableLeaseSet, SessionConfigLimits, SessionId,
    SessionStatusCode, encode_frame, project_options,
};
use i2pr_client::{DestinationConfig, LocalRouterContext};
use i2pr_crypto::{SigningPrivateKey, X25519_KEY_LENGTH, X25519PrivateKey};
use i2pr_daemon::config::I2cpConfig;
use i2pr_daemon::i2cp::{I2cpServiceState, I2cpSessionPhase};
use i2pr_proto::{
    Certificate, CryptoKeyType, Date32, Destination, Hash, KeyAndCert, KeyCertificate, Lease2,
    LeaseSet2, LeaseSet2EncryptionKey, LeaseSet2Flags, LeaseSet2Header, Mapping, PublicKey,
    SignatureValue, SigningKeyType,
};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_tunnel::{LocalZeroHopInbound, LocalZeroHopOutbound, TunnelDirection, TunnelId};
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

fn encode_get_date(version: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(version.len() as u8);
    out.extend_from_slice(version.as_bytes());
    out
}

async fn hello_i2cp(stream: &mut TcpStream) {
    write_all(stream, &[PROTOCOL_BYTE]).await;
    let get_date_body = encode_get_date(M9_ADVERTISED_VERSION);
    let get_date_frame = encode_frame(32, &get_date_body).expect("get date frame");
    write_all(stream, &get_date_frame).await;
    let reply = read_exact_frame(stream).await;
    assert_eq!(reply[4], 33, "expected SetDate, got {reply:?}");
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(1_786_000_000_000)
}

fn now_seconds_u32() -> u32 {
    u32::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(1_786_000_000),
    )
    .unwrap_or(1_786_000_000)
}

/// Builds a destination whose X25519 public matches the supplied secret.
fn build_matching_destination(
    signing_seed: [u8; 32],
    x25519_secret: [u8; X25519_KEY_LENGTH],
) -> (Destination, SigningPrivateKey, [u8; X25519_KEY_LENGTH]) {
    let signing_key = SigningPrivateKey::from_bytes(signing_seed);
    let signing_public = signing_key.public_key().expect("signing public");
    let private = X25519PrivateKey::from_bytes(x25519_secret);
    let public_bytes = private.public_bytes();
    let x25519_public =
        PublicKey::new(CryptoKeyType::X25519, public_bytes.to_vec()).expect("public");
    let certificate = Certificate::Key(
        KeyCertificate::for_types(SigningKeyType::EdDsaSha512Ed25519, CryptoKeyType::X25519)
            .expect("cert"),
    );
    // Legacy static slot: zeroed 320-byte padding is accepted by the
    // Plan 170 policy relocation; X25519 is enforced at install.
    let destination = Destination::new(
        KeyAndCert::new(x25519_public, signing_public, vec![0x33; 320], certificate).expect("keys"),
    )
    .expect("destination");
    (destination, signing_key, x25519_secret)
}

fn mapping_of(entries: &[(&str, &str)]) -> Mapping {
    let owned: Vec<(String, String)> = entries
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    Mapping::from_entries(owned).expect("mapping")
}

fn zero_hop_options() -> Mapping {
    mapping_of(&[
        ("inbound.length", "0"),
        ("outbound.length", "0"),
        ("inbound.quantity", "1"),
        ("outbound.quantity", "1"),
        ("inbound.backupQuantity", "0"),
        ("outbound.backupQuantity", "0"),
        ("inbound.allowZeroHop", "true"),
        ("outbound.allowZeroHop", "true"),
        ("i2cp.dontPublishLeaseSet", "true"),
        ("i2cp.leaseSetType", "3"),
        ("i2cp.leaseSetEncType", "4"),
        ("i2cp.fastReceive", "true"),
        ("i2cp.messageReliability", "BestEffort"),
    ])
}

fn build_session_config_body_with_mapping(
    destination: &Destination,
    signing_key: &SigningPrivateKey,
    creation_ms: u64,
    mapping: &Mapping,
) -> Vec<u8> {
    let mut body = destination
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode destination");
    let mapping_bytes = mapping
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .unwrap_or_else(|_| vec![0u8, 0u8]);
    body.extend(mapping_bytes);
    body.extend_from_slice(&creation_ms.to_be_bytes());
    let signature = signing_key.sign(&body).expect("sign");
    body.extend_from_slice(signature.as_bytes());
    body
}

async fn write_create_session_with_mapping(
    stream: &mut TcpStream,
    session_config_body: &[u8],
) -> (SessionId, RequestVariableLeaseSet) {
    let create_session_frame = encode_frame(1, session_config_body).expect("create session frame");
    write_all(stream, &create_session_frame).await;
    let reply = read_exact_frame(stream).await;
    assert_eq!(reply[4], 20, "expected SessionStatus, got {reply:?}");
    let body = &reply[5..];
    assert_eq!(body.len(), 3);
    let session_raw = u16::from_be_bytes([body[0], body[1]]);
    assert_eq!(body[2], SessionStatusCode::Created as u8);
    let session = SessionId::new(session_raw);
    let followup = read_exact_frame(stream).await;
    assert_eq!(
        followup[4], 37,
        "expected RequestVariableLeaseSet, got {followup:?}"
    );
    let request = RequestVariableLeaseSet::decode(&followup[5..]).expect("decode lease request");
    assert_eq!(request.session, session);
    (session, request)
}

fn encode_destroy_session(session: SessionId) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&session.get().to_be_bytes());
    out
}

async fn write_destroy_session(stream: &mut TcpStream, session: SessionId) {
    let body = encode_destroy_session(session);
    let frame = encode_frame(3, &body).expect("destroy frame");
    write_all(stream, &frame).await;
}

/// Builds a correctly signed Standard LeaseSet2 for the supplied lease.
#[allow(clippy::too_many_arguments)]
fn build_signed_ls2(
    destination: &Destination,
    signing_key: &SigningPrivateKey,
    x25519_public_bytes: [u8; X25519_KEY_LENGTH],
    gateway: Hash,
    tunnel_id: u32,
    published: u32,
    end_seconds: u32,
    expires: u32,
) -> LeaseSet2 {
    let offset = expires.saturating_sub(published);
    let offset_u16 = u16::try_from(offset).expect("offset fits");
    let _ = offset_u16;
    let header = LeaseSet2Header::new(
        destination.clone(),
        published,
        offset_u16,
        LeaseSet2Flags::from_raw(0),
    )
    .expect("header");
    let encryption_keys = vec![
        LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, x25519_public_bytes.to_vec())
            .expect("encryption key"),
    ];
    let leases = vec![Lease2::new(
        gateway,
        tunnel_id,
        Date32::from_seconds(end_seconds),
    )];
    let placeholder = SignatureValue::new(SigningKeyType::EdDsaSha512Ed25519, vec![0u8; 64])
        .expect("placeholder");
    let unsigned = LeaseSet2::new(
        header,
        Mapping::empty(),
        encryption_keys,
        leases,
        placeholder,
    )
    .expect("unsigned ls2");
    let signature = signing_key
        .sign(&unsigned.signature_preimage())
        .expect("sign ls2");
    let sig_value = SignatureValue::new(
        SigningKeyType::EdDsaSha512Ed25519,
        signature.as_bytes().to_vec(),
    )
    .expect("sig value");
    let header = LeaseSet2Header::new(
        destination.clone(),
        published,
        offset_u16,
        LeaseSet2Flags::from_raw(0),
    )
    .expect("header");
    let encryption_keys = vec![
        LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, x25519_public_bytes.to_vec())
            .expect("encryption key"),
    ];
    let leases = vec![Lease2::new(
        gateway,
        tunnel_id,
        Date32::from_seconds(end_seconds),
    )];
    LeaseSet2::new(header, Mapping::empty(), encryption_keys, leases, sig_value).expect("ls2")
}

async fn write_create_lease_set2(
    stream: &mut TcpStream,
    session: SessionId,
    lease_set: &LeaseSet2,
    x25519_secret: [u8; X25519_KEY_LENGTH],
) {
    let lease_set_bytes = lease_set
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode ls2");
    let mut body = Vec::new();
    body.extend_from_slice(&session.get().to_be_bytes());
    body.push(3);
    body.extend_from_slice(&lease_set_bytes);
    body.push(1);
    body.extend_from_slice(&4u16.to_be_bytes());
    body.extend_from_slice(&(x25519_secret.len() as u16).to_be_bytes());
    body.extend_from_slice(&x25519_secret);
    let frame = encode_frame(41, &body).expect("create lease set frame");
    write_all(stream, &frame).await;
}

fn build_send_message(
    session: SessionId,
    destination: &Destination,
    payload_bytes: &[u8],
) -> Vec<u8> {
    use i2pr_api::i2cp::{ClientNonce, MessageType, Payload, SendMessage};
    let payload = Payload::new(payload_bytes.to_vec()).expect("payload");
    let send = SendMessage {
        session,
        destination: destination.clone(),
        payload,
        nonce: ClientNonce::new(42),
    };
    let body = send.encode().expect("encode send");
    encode_frame(MessageType::SendMessage as u8, &body).expect("frame")
}

fn gzip_payload() -> Vec<u8> {
    use i2pr_api::i2cp::{GZIP_XFLAGS_JAVA, PayloadGzipHeader};
    let header = PayloadGzipHeader {
        source_port: 7,
        destination_port: 8,
        xflags: GZIP_XFLAGS_JAVA,
        protocol: 6,
    }
    .encode();
    let mut out = Vec::with_capacity(header.len() + 5);
    out.extend_from_slice(&header);
    out.extend_from_slice(b"hello");
    out
}

// ---- Plan 172 §14 item 11: option projection rejects bad zero-hop ----

#[test]
fn allow_zero_hop_false_with_length_zero_is_rejected() {
    let mapping = mapping_of(&[
        ("inbound.length", "0"),
        ("outbound.length", "0"),
        ("inbound.allowZeroHop", "false"),
        ("outbound.allowZeroHop", "false"),
    ]);
    let result = project_options(
        &mapping,
        &SessionConfigLimits::m9(),
        DestinationConfig::balanced(),
    );
    assert!(result.is_err(), "allowZeroHop=false + length 0 must reject");
}

#[test]
fn mixed_zero_hop_remote_modes_are_rejected() {
    let mapping = mapping_of(&[
        ("inbound.length", "0"),
        ("outbound.length", "2"),
        ("inbound.allowZeroHop", "true"),
        ("outbound.allowZeroHop", "true"),
    ]);
    let result = project_options(
        &mapping,
        &SessionConfigLimits::m9(),
        DestinationConfig::balanced(),
    );
    assert!(result.is_err(), "mixed modes must reject explicitly");
}

#[test]
fn zero_hop_profile_projects_to_local_mode() {
    let policy = project_options(
        &zero_hop_options(),
        &SessionConfigLimits::m9(),
        DestinationConfig::balanced(),
    )
    .expect("zero-hop profile accepted");
    assert_eq!(
        policy.tunnel_mode,
        i2pr_client::DestinationTunnelMode::LocalZeroHop
    );
    assert!(policy.inbound_allow_zero_hop);
    assert!(policy.outbound_allow_zero_hop);
    assert!(policy.dont_publish_lease_set);
}

#[test]
fn remote_profile_stays_remote_and_rejects_allow_zero_hop_true() {
    let mapping = mapping_of(&[("inbound.allowZeroHop", "true")]);
    let result = project_options(
        &mapping,
        &SessionConfigLimits::m9(),
        DestinationConfig::balanced(),
    );
    assert!(
        result.is_err(),
        "allowZeroHop=true without length 0 must reject"
    );
    let empty = Mapping::from_entries(Vec::<(String, String)>::new()).expect("empty");
    let policy = project_options(
        &empty,
        &SessionConfigLimits::m9(),
        DestinationConfig::balanced(),
    )
    .expect("empty maps to remote defaults");
    assert!(matches!(
        policy.tunnel_mode,
        i2pr_client::DestinationTunnelMode::Remote { .. }
    ));
}

// ---- Plan 172 §5 invariants: typed zero-hop vs remote ----

#[test]
fn local_zero_hop_rejects_zero_gateway_and_sentinel_id() {
    let id = TunnelId::new(0x1000).expect("id");
    let err = LocalZeroHopInbound::new(Hash::from_bytes([0; 32]), id, 0, 60).unwrap_err();
    assert_eq!(err, i2pr_tunnel::ZeroHopError::ZeroGateway);
    let sentinel = TunnelId::new(u32::MAX).expect("nonzero");
    let err = LocalZeroHopInbound::new(Hash::from_bytes([1; 32]), sentinel, 0, 60).unwrap_err();
    assert_eq!(err, i2pr_tunnel::ZeroHopError::SentinelTunnelId);
}

#[test]
fn remote_tunnel_still_rejects_empty_hop_list() {
    let err = i2pr_tunnel::EstablishedTunnel::new(
        TunnelDirection::Outbound,
        TunnelId::new(1).expect("id"),
        Vec::new(),
        0,
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(err, i2pr_tunnel::EstablishedTunnelError::EmptyHopList);
    assert_eq!(i2pr_tunnel::MIN_HOPS, 1);
}

#[test]
fn zero_hop_pool_becomes_usable_and_expires() {
    use i2pr_client::DestinationTunnelPool;
    use i2pr_tunnel::TunnelId;
    let config = DestinationConfig::balanced();
    let mut pool = DestinationTunnelPool::new(config).expect("pool");
    assert!(!pool.is_usable(0));
    let gateway = Hash::from_bytes([9; 32]);
    let inbound_id = TunnelId::new(0x1111).expect("id");
    let outbound_id = TunnelId::new(0x2222).expect("id");
    let inbound = LocalZeroHopInbound::new(gateway, inbound_id, 0, 600).expect("inbound");
    let outbound = LocalZeroHopOutbound::new(outbound_id, 0, 600).expect("outbound");
    pool.register_local_zero_hop_inbound(inbound, 0)
        .expect("register inbound");
    assert!(!pool.is_usable(0), "needs outbound too");
    pool.register_local_zero_hop_outbound(outbound, 0)
        .expect("register outbound");
    assert!(pool.is_usable(0));
    let sources = pool.inbound_lease_sources(0);
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].gateway(), gateway);
    assert_eq!(sources[0].gateway_receive_tunnel_id(), 0x1111);
    // Duplicate tunnel id is idempotent.
    let dup = LocalZeroHopInbound::new(gateway, inbound_id, 0, 600).expect("dup");
    let first_slot = sources[0].slot();
    let dup_slot = pool
        .register_local_zero_hop_inbound(dup, 0)
        .expect("dup ok");
    assert_eq!(first_slot, dup_slot);
    // Expiry removes the lease source.
    let evicted = pool.advance_time(600);
    assert!(!evicted.is_empty());
    assert!(pool.inbound_lease_sources(600).is_empty());
    assert!(!pool.is_usable(600));
    // Release returns to baseline.
    let mut pool2 = DestinationTunnelPool::new(config).expect("pool");
    let inbound = LocalZeroHopInbound::new(gateway, inbound_id, 0, 600).expect("inbound");
    let outbound = LocalZeroHopOutbound::new(outbound_id, 0, 600).expect("outbound");
    pool2
        .register_local_zero_hop_inbound(inbound, 0)
        .expect("in");
    pool2
        .register_local_zero_hop_outbound(outbound, 0)
        .expect("out");
    assert_eq!(pool2.release_all(), 2);
    assert!(pool2.is_empty());
}

#[test]
fn local_router_context_rejects_zero_hash() {
    let err = LocalRouterContext::new(Hash::from_bytes([0; 32])).unwrap_err();
    let _ = err;
}

// ---- TCP lifecycle: non-empty request, usability gating, install ----

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn zero_hop_request_is_nonempty_and_session_unusable_before_install() {
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let (destination, signing_key, _secret) = build_matching_destination([0x07; 32], [0x22; 32]);
    let body = build_session_config_body_with_mapping(
        &destination,
        &signing_key,
        now_ms(),
        &zero_hop_options(),
    );
    let (session, request) = write_create_session_with_mapping(&mut client, &body).await;
    // §14 item 1: never empty for the counted profile.
    assert!(
        !request.leases.is_empty(),
        "zero-hop request must carry >= 1 lease"
    );
    assert_eq!(request.leases.len(), 1);
    // Gateway is the process's actual local router hash, tunnel id non-zero non-sentinel.
    let lease = &request.leases[0];
    assert_eq!(lease.gateway, state.local_router_hash());
    assert_ne!(lease.gateway, Hash::from_bytes([0; 32]));
    assert_ne!(lease.tunnel_id, 0);
    assert_ne!(lease.tunnel_id, u32::MAX);
    // §14 item 2: not usable before install.
    let session_state = state.session_state(session).expect("session state");
    assert!(session_state.is_zero_hop);
    assert_eq!(session_state.phase(), I2cpSessionPhase::AwaitingLeaseSet2);
    assert!(!session_state.is_usable_for_data());
    let fact = state.session_lifecycle_fact(session).expect("fact");
    assert_eq!(fact, (true, false, false, 1));
    // SendMessage before install must not be Accepted (BadLocalLeaseSet).
    let peer = build_matching_destination([0x09; 32], [0x44; 32]).0;
    let send_frame = build_send_message(session, &peer, &gzip_payload());
    write_all(&mut client, &send_frame).await;
    let reply = read_exact_frame(&mut client).await;
    assert_eq!(reply[4], 22, "expected MessageStatus, got {reply:?}");
    // status byte is body[6]; Accepted == 0 per M9 mapping. Must not be Accepted.
    assert_ne!(reply[5 + 6], 0, "pre-install send must not be Accepted");
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn zero_hop_install_succeeds_and_session_becomes_usable() {
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let x25519_secret = [0x22; 32];
    let (destination, signing_key, _) = build_matching_destination([0x07; 32], x25519_secret);
    let private = X25519PrivateKey::from_bytes(x25519_secret);
    let public_bytes = private.public_bytes();
    let body = build_session_config_body_with_mapping(
        &destination,
        &signing_key,
        now_ms(),
        &zero_hop_options(),
    );
    let (session, request) = write_create_session_with_mapping(&mut client, &body).await;
    assert_eq!(request.leases.len(), 1);
    let lease = &request.leases[0];
    let published = now_seconds_u32();
    let end = published.saturating_add(300);
    let expires = published.saturating_add(500);
    let ls2 = build_signed_ls2(
        &destination,
        &signing_key,
        public_bytes,
        lease.gateway,
        lease.tunnel_id,
        published,
        end,
        expires,
    );
    write_create_lease_set2(&mut client, session, &ls2, x25519_secret).await;
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let session_state = state.session_state(session).expect("session state");
    assert_eq!(session_state.phase(), I2cpSessionPhase::Usable);
    assert!(session_state.is_usable_for_data());
    let fact = state.session_lifecycle_fact(session).expect("fact");
    assert_eq!(fact, (true, true, true, 1));
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn zero_lease_and_foreign_leases_are_rejected() {
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_i2cp(&mut client).await;
    let x25519_secret = [0x55; 32];
    let (destination, signing_key, _) = build_matching_destination([0x0A; 32], x25519_secret);
    let private = X25519PrivateKey::from_bytes(x25519_secret);
    let public_bytes = private.public_bytes();
    let body = build_session_config_body_with_mapping(
        &destination,
        &signing_key,
        now_ms(),
        &zero_hop_options(),
    );
    let (session, request) = write_create_session_with_mapping(&mut client, &body).await;
    assert_eq!(request.leases.len(), 1);
    let published = now_seconds_u32();
    // Zero-lease LS2 cannot even be constructed via the typed API
    // (fail-closed at the codec layer); this proves §14 item 3 at the
    // structural boundary. The TCP rejection path is exercised below
    // with a foreign lease.
    let header = LeaseSet2Header::new(
        destination.clone(),
        published,
        500,
        LeaseSet2Flags::from_raw(0),
    )
    .expect("header");
    let encryption_keys = vec![
        LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, public_bytes.to_vec()).expect("enc key"),
    ];
    let placeholder = SignatureValue::new(SigningKeyType::EdDsaSha512Ed25519, vec![0u8; 64])
        .expect("placeholder");
    let zero_result = LeaseSet2::new(
        header,
        Mapping::empty(),
        encryption_keys,
        Vec::new(),
        placeholder,
    );
    assert!(zero_result.is_err(), "zero-lease LS2 must be rejected");
    // Foreign gateway must be rejected over TCP.
    let foreign = build_signed_ls2(
        &destination,
        &signing_key,
        public_bytes,
        Hash::from_bytes([0xEE; 32]),
        0x9999,
        published,
        published.saturating_add(300),
        published.saturating_add(500),
    );
    write_create_lease_set2(&mut client, session, &foreign, x25519_secret).await;
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }
    // Install failure is a wire error on the common terminal path:
    // the connection closes and the session/destination are torn down
    // fail-closed with no installed LS2 and no leaked pool entries.
    // The peer-visible close is the proof; baselines must return to zero.
    let mut buf = [0u8; 16];
    let read_result =
        tokio::time::timeout(std::time::Duration::from_secs(2), client.read(&mut buf)).await;
    let closed = match read_result {
        Ok(Ok(0)) => true,
        Ok(Ok(_)) => false,
        Ok(Err(_)) => true,
        Err(_) => false,
    };
    assert!(
        closed,
        "foreign LS2 rejection must observably close the connection"
    );
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    assert!(
        state.session_state(session).is_none(),
        "rejected session must not retain state"
    );
    assert_eq!(state.snapshot().session_count, 0);
    assert_eq!(state.snapshot().destination_count, 0);
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn destroy_releases_zero_hop_and_sibling_stays_usable() {
    let (state, address, scope, parent) = start_listener(i2cp_config()).await;
    // Session A (zero-hop, installed).
    let mut client_a = TcpStream::connect(address).await.expect("connect a");
    hello_i2cp(&mut client_a).await;
    let secret_a = [0x2A; 32];
    let (dest_a, key_a, _) = build_matching_destination([0x0B; 32], secret_a);
    let public_a = X25519PrivateKey::from_bytes(secret_a).public_bytes();
    let body_a =
        build_session_config_body_with_mapping(&dest_a, &key_a, now_ms(), &zero_hop_options());
    let (session_a, request_a) = write_create_session_with_mapping(&mut client_a, &body_a).await;
    assert_eq!(request_a.leases.len(), 1);
    let published = now_seconds_u32();
    let ls2_a = build_signed_ls2(
        &dest_a,
        &key_a,
        public_a,
        request_a.leases[0].gateway,
        request_a.leases[0].tunnel_id,
        published,
        published.saturating_add(300),
        published.saturating_add(500),
    );
    write_create_lease_set2(&mut client_a, session_a, &ls2_a, secret_a).await;
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    assert!(
        state
            .session_state(session_a)
            .expect("a")
            .is_usable_for_data()
    );
    // Session B (zero-hop, installed, sibling).
    let mut client_b = TcpStream::connect(address).await.expect("connect b");
    hello_i2cp(&mut client_b).await;
    let secret_b = [0x2B; 32];
    let (dest_b, key_b, _) = build_matching_destination([0x0C; 32], secret_b);
    let public_b = X25519PrivateKey::from_bytes(secret_b).public_bytes();
    let body_b =
        build_session_config_body_with_mapping(&dest_b, &key_b, now_ms(), &zero_hop_options());
    let (session_b, request_b) = write_create_session_with_mapping(&mut client_b, &body_b).await;
    let ls2_b = build_signed_ls2(
        &dest_b,
        &key_b,
        public_b,
        request_b.leases[0].gateway,
        request_b.leases[0].tunnel_id,
        published,
        published.saturating_add(300),
        published.saturating_add(500),
    );
    write_create_lease_set2(&mut client_b, session_b, &ls2_b, secret_b).await;
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    assert!(
        state
            .session_state(session_b)
            .expect("b")
            .is_usable_for_data()
    );
    assert_eq!(state.snapshot().session_count, 2);
    // Destroy A; B remains usable.
    write_destroy_session(&mut client_a, session_a).await;
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }
    assert_eq!(state.snapshot().session_count, 1);
    assert_eq!(state.snapshot().destination_count, 1);
    let state_b = state.session_state(session_b).expect("sibling survives");
    assert!(state_b.is_usable_for_data());
    assert!(state.session_state(session_a).is_none());
    drop(client_a);
    drop(client_b);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}
