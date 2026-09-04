//! Plan 161 independent IPv4 interop driver (i2pr side).
//!
//! A single ordered test drives direction A against one ephemeral
//! exact-pinned i2pd 2.61.0 process over real loopback UDP: i2pr
//! initiator -> i2pd responder with tokenless Retry establishment,
//! mutual authentication, one small and one fragmented DatabaseStore
//! with direct DeliveryStatus replies proving bidirectional delivery,
//! then graceful termination with resource baseline.
//!
//! Direction B (i2pd initiator), the token/Retry matrix beyond the
//! tokenless path, and malformed/spoof rows are later stages of this
//! same test file and are not claimed by this revision.
//!
//! Environment (all required, fail-closed when absent):
//!
//! ```text
//! I2PD_ROUTER_INFO   path to the live i2pd router.info bytes
//! I2PD_SSU2_ENDPOINT out-of-band dial endpoint, e.g. 127.0.0.1:43823
//! I2PR_SSU2_BIND     fixed loopback bind for i2pr, e.g. 127.0.0.1:44001
//! I2PR_SSU2_FLOODFILL 1 to advertise the floodfill cap (direction-B trigger)
//! EVIDENCE_DIR       directory for expected-RI and evidence-fragment files
//! ```
//!
//! No secret material is written to the evidence directory: only public
//! signed RouterInfo bytes (exchanged in clear on the wire), lengths,
//! message IDs, digests, and privacy-safe counters.

use std::io::Write as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flate2::Compression;
use flate2::write::GzEncoder;
use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_proto::{
    DatabaseStoreData, DatabaseStoreMessage, Date, DeferredPayload, Hash, I2npBody, I2npMessage,
    Mapping, RouterAddress, RouterInfo,
};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, Ssu2DialTarget, Ssu2EstablishedLink,
    Ssu2IdentityMaterial, Ssu2RuntimeConfig, Ssu2ServiceHandle, Ssu2SocketConfig,
};
use i2pr_transport::{EncodedI2npMessage, MAX_I2NP_MESSAGE_BYTES, PeerId, TerminationCategory};
use i2pr_transport_ssu2::{IntroKey, Ssu2PublicKey, Ssu2RouterAddress, constants};
use rand_core::{OsRng, TryRngCore};

const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

const SMALL_STORE_MSG_ID: u32 = 0x51A4_1101;
const LARGE_STORE_MSG_ID: u32 = 0x51A4_1102;
const SMALL_STORE_REPLY_TOKEN: u32 = 0x51A4_2001;
const LARGE_STORE_REPLY_TOKEN: u32 = 0x51A4_2002;
const DELIVERY_STATUS_TYPE: u8 = 10;

fn env_value(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("missing required env {name}"))
}

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(env_value(name))
}

fn wall_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(1_700_000_000)
}

/// I2P-base64 encoder for test-only RouterAddress construction.
fn i2p_b64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let mut n: u32 = 0;
        for byte in chunk {
            n = (n << 8) | u32::from(*byte);
        }
        n <<= 8 * (3 - chunk.len());
        let digits = match chunk.len() {
            1 => 2,
            2 => 3,
            _ => 4,
        };
        for index in 0..digits {
            output.push(ALPHABET[((n >> (18 - 6 * index)) & 0x3f) as usize] as char);
        }
        for _ in digits..4 {
            output.push('=');
        }
    }
    output
}

fn gzip_member(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes).expect("gzip");
    encoder.finish().expect("finish")
}

struct TestRouter {
    hash: Hash,
    static_bytes: [u8; 32],
    intro: IntroKey,
    router_info: Vec<u8>,
}

fn ssu2_address(host: &str, port: u16) -> RouterAddress {
    let static_key = X25519PrivateKey::generate(&mut OsRng).expect("static");
    let mut intro_bytes = [0_u8; 32];
    loop {
        OsRng.try_fill_bytes(&mut intro_bytes).expect("rng");
        if intro_bytes.iter().any(|byte| *byte != 0) {
            break;
        }
    }
    let options = Mapping::from_entries(vec![
        ("host".to_string(), host.to_string()),
        ("port".to_string(), port.to_string()),
        ("v".to_string(), "2".to_string()),
        ("s".to_string(), i2p_b64_encode(&static_key.public_bytes())),
        ("i".to_string(), i2p_b64_encode(&intro_bytes)),
    ])
    .expect("options");
    RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .expect("address")
}

/// Builds one deterministic test RouterInfo.
///
/// `extra_addresses` appends additional valid SSU2 addresses so the
/// encoded RouterInfo reaches the requested size class (small fits one
/// SSU2 datagram; large requires I2NP fragmentation). `floodfill`
/// advertises the RI-level floodfill cap.
fn make_test_router(host: &str, port: u16, extra_addresses: usize, floodfill: bool) -> TestRouter {
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
    let hash = bundle.identity().hash().expect("hash");
    let static_key = X25519PrivateKey::generate(&mut OsRng).expect("static");
    let static_bytes = *static_key.secret_bytes();
    let mut intro_bytes = [0_u8; 32];
    loop {
        OsRng.try_fill_bytes(&mut intro_bytes).expect("rng");
        if intro_bytes.iter().any(|byte| *byte != 0) {
            break;
        }
    }
    let intro = IntroKey::new(intro_bytes);
    let mut addresses = vec![{
        let options = Mapping::from_entries(vec![
            ("host".to_string(), host.to_string()),
            ("port".to_string(), port.to_string()),
            ("v".to_string(), "2".to_string()),
            ("s".to_string(), i2p_b64_encode(&static_key.public_bytes())),
            ("i".to_string(), i2p_b64_encode(&intro_bytes)),
        ])
        .expect("options");
        RouterAddress::new(
            10,
            Date::from_millis(9_999_999_999_999),
            "SSU2".to_string(),
            options,
        )
        .expect("address")
    }];
    for index in 0..extra_addresses {
        // Distinct deterministic loopback ports keep every address valid
        // without binding any socket.
        let extra_port = 43100_u16.saturating_add(index as u16);
        addresses.push(ssu2_address(host, extra_port));
    }
    // The pinned i2pd 2.61.0 peer enforces two ingest gates on
    // SessionConfirmed/DatabaseStore RouterInfos: `router.version`
    // digits must clear its 0.9.58 minimum-allowed floor, and a
    // `netId` property matching its network (mainnet 2) must be
    // present — a missing netId marks the RouterInfo unreachable
    // without logging, so NetDB silently refuses to persist it. The
    // version value asserts the wire-compatibility floor under test,
    // not a software-identity claim.
    let options = if floodfill {
        Mapping::from_entries(vec![
            ("caps".to_string(), "fO".to_string()),
            ("router.version".to_string(), "0.9.58".to_string()),
            ("netId".to_string(), "2".to_string()),
        ])
        .expect("caps")
    } else {
        Mapping::from_entries(vec![
            ("router.version".to_string(), "0.9.58".to_string()),
            ("netId".to_string(), "2".to_string()),
        ])
        .expect("version")
    };
    let info = bundle
        .sign_router_info(
            Date::from_millis(wall_secs().saturating_mul(1000)),
            addresses,
            Vec::new(),
            options,
        )
        .expect("sign");
    let router_info = info
        .encode_to_vec(constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .expect("encode");
    TestRouter {
        hash,
        static_bytes,
        intro,
        router_info,
    }
}

/// Encodes one DatabaseStore(RouterInfo) I2NP message in the 9-byte
/// NTCP2/SSU2 short-transport form both routers carry over SSU2. A
/// nonzero reply token with a direct (tunnel 0) gateway asks i2pd for a
/// DeliveryStatus echo over the same session.
fn database_store_wire(
    key: Hash,
    router_info: &[u8],
    message_id: u32,
    reply_token: u32,
    reply_gateway: Hash,
) -> Vec<u8> {
    let gzip = gzip_member(router_info);
    let payload = DeferredPayload::new(gzip, usize::from(u16::MAX)).expect("deferred payload");
    let store = DatabaseStoreMessage {
        key,
        reply_token,
        reply_tunnel_id: Some(0),
        reply_gateway: Some(reply_gateway),
        data: DatabaseStoreData::RouterInfoCompressed(payload),
    };
    let body = I2npBody::DatabaseStore(Box::new(store));
    // Transport expiration must land inside the pinned peer's accept
    // window (60s past to 180s future): the peer converts the short
    // seconds field to milliseconds and drops anything farther out, so
    // the tunnel-style +3600s horizon never persists.
    let expiration = wall_secs().saturating_add(60).min(u64::from(u32::MAX)) as u32;
    let message = I2npMessage::new_short_transport(message_id, expiration, body).expect("message");
    message
        .encode_short_transport_to_vec(MAX_I2NP_MESSAGE_BYTES)
        .expect("encode")
}

/// Formats one SHA-256 digest for evidence rows.
fn digest_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn write_file(dir: &Path, name: &str, bytes: &[u8]) {
    std::fs::write(dir.join(name), bytes).unwrap_or_else(|_| panic!("write {name}"));
}

async fn wait_for_active(service: &i2pr_runtime::Ssu2RuntimeService, expected: usize, what: &str) {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        if service.snapshot().active_sessions == expected {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "active sessions did not reach {expected} ({what})"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Polls the service inbound handoff until the DeliveryStatus replies
/// for both store tokens arrive, asserting type, token echo, and peer.
/// i2pd answers each ingested DatabaseStore directly (tunnel 0) over the
/// same session, so the two replies prove small + fragmented delivery
/// i2pr -> i2pd and the authenticated return path in one round trip.
async fn collect_delivery_status(
    handle: &mut Ssu2ServiceHandle,
    peer: PeerId,
    small_token: u32,
    large_token: u32,
) -> (Vec<u8>, Vec<u8>) {
    let deadline = tokio::time::Instant::now() + REPLY_TIMEOUT;
    let mut small: Option<Vec<u8>> = None;
    let mut large: Option<Vec<u8>> = None;
    while small.is_none() || large.is_none() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "i2pd DeliveryStatus replies missing"
        );
        let inbound = match tokio::time::timeout(POLL_INTERVAL * 4, handle.next_inbound()).await {
            Ok(inbound) => inbound.expect("service live"),
            Err(_) => continue,
        };
        assert_eq!(inbound.peer, peer, "unexpected inbound peer");
        let bytes = inbound.bytes;
        // i2pd randomizes the short header message ID on send, so the
        // reply token is matched in the DeliveryStatus body (offset 9),
        // not in the transport header.
        if bytes.len() < 21 || bytes[0] != DELIVERY_STATUS_TYPE {
            continue;
        }
        let token = u32::from_be_bytes(bytes[9..13].try_into().expect("token bytes"));
        if token == small_token && small.is_none() {
            small = Some(bytes);
        } else if token == large_token && large.is_none() {
            large = Some(bytes);
        }
    }
    (small.expect("small reply"), large.expect("large reply"))
}

struct Evidence {
    rows: Vec<(String, String)>,
}

impl Evidence {
    fn new() -> Self {
        Self { rows: Vec::new() }
    }

    fn record(&mut self, label: &str, value: impl ToString) {
        self.rows.push((
            label.to_string(),
            value.to_string().replace(['\t', '\n'], " "),
        ));
    }

    fn write(&self, dir: &Path) {
        let mut text = String::new();
        for (label, value) in &self.rows {
            text.push_str(label);
            text.push('\t');
            text.push_str(value);
            text.push('\n');
        }
        write_file(dir, "driver-evidence.tsv", text.as_bytes());
    }
}

#[tokio::test]
async fn ssu2_independent_ipv4_interop() {
    let i2pd_ri_path = env_path("I2PD_ROUTER_INFO");
    let i2pd_endpoint: SocketAddr = env_value("I2PD_SSU2_ENDPOINT").parse().expect("endpoint");
    let bind: SocketAddr = env_value("I2PR_SSU2_BIND").parse().expect("bind");
    assert!(bind.ip().is_loopback(), "i2pr bind must be loopback");
    assert!(
        i2pd_endpoint.ip().is_loopback(),
        "i2pd endpoint must be loopback"
    );
    let floodfill = env_value("I2PR_SSU2_FLOODFILL") == "1";
    let evidence_dir = env_path("EVIDENCE_DIR");
    let mut evidence = Evidence::new();

    // ---- Out-of-band i2pd RouterInfo ingest -------------------------------
    let i2pd_ri_bytes = std::fs::read(&i2pd_ri_path).expect("read i2pd router.info");
    evidence.record("i2pd-routerinfo-len", i2pd_ri_bytes.len());
    let i2pd_info = RouterInfo::decode(
        &i2pd_ri_bytes,
        constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
    )
    .expect("decode i2pd RouterInfo");
    let i2pd_hash = i2pd_info.router_identity().hash().expect("hash");
    let i2pd_peer = PeerId::from_hash(i2pd_hash);
    let mut i2pd_ssu2: Option<Ssu2RouterAddress> = None;
    for address in i2pd_info.addresses() {
        if address.transport_style() == "SSU2" {
            i2pd_ssu2 = Some(Ssu2RouterAddress::parse(address).expect("parse i2pd SSU2"));
        }
    }
    let i2pd_ssu2 = i2pd_ssu2.expect("i2pd SSU2 address");
    evidence.record(
        "i2pd-ssu2-class",
        format!("{:?}", i2pd_ssu2.address_class()),
    );
    let material = i2pd_ssu2.address_material().expect("i2pd key material");
    let responder_static =
        Ssu2PublicKey::new(*material.static_public_key().as_bytes()).expect("static key");
    let responder_intro = IntroKey::new(*material.intro_key().as_bytes());
    let target = Ssu2DialTarget::new(
        i2pd_peer,
        i2pd_hash,
        i2pd_endpoint,
        responder_static,
        responder_intro,
    )
    .expect("dial target");

    // ---- i2pr identity (fixed bind so the advertised endpoint is exact) --
    let local = make_test_router("127.0.0.1", bind.port(), 0, floodfill);
    evidence.record("i2pr-routerinfo-len", local.router_info.len());
    write_file(&evidence_dir, "i2pr-router-info.ri", &local.router_info);

    let service = i2pr_runtime::Ssu2RuntimeService::new(
        Ssu2RuntimeConfig::default(),
        Ssu2IdentityMaterial {
            router_hash: local.hash,
            static_secret_bytes: local.static_bytes,
            intro_key: local.intro,
            router_info: local.router_info.clone(),
        },
    )
    .expect("service");
    let token = CancellationToken::new();
    let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
    let mut handle: Ssu2ServiceHandle = service
        .start(
            &scope,
            Ssu2SocketConfig {
                ipv4: Some(bind),
                ipv6: None,
            },
        )
        .await
        .expect("bind");
    // The OS must honor the fixed bind: the advertised endpoint is exact.
    assert_eq!(handle.local_v4().expect("bound v4"), bind);

    // ---- Direction A: i2pr initiator -> i2pd responder --------------------
    let baseline = service.snapshot();
    eprintln!("debug: bind={bind} i2pd={i2pd_endpoint} sockets...");
    let dial_result = service
        .dial_ssu2(target, DIAL_TIMEOUT, &CancellationToken::new())
        .await;
    eprintln!(
        "debug: dial_result={dial_result:?} snapshot={:?}",
        service.snapshot()
    );
    let established: Ssu2EstablishedLink = dial_result.expect("direction-A dial");
    assert!(
        !established.used_cached_token,
        "first dial must take the tokenless Retry path"
    );
    evidence.record("direction-a-cached-token", "false");
    wait_for_active(&service, 1, "direction A").await;
    let after_dial = service.snapshot();
    assert!(after_dial.sessions_established > baseline.sessions_established);
    evidence.record("direction-a-established", after_dial.sessions_established);

    // Warmup: i2pd registers the session peer asynchronously after
    // establishment and emits its own chatter (NewToken-driven ACKs,
    // its DatabaseStore). Draining here settles peer registration so
    // the later direct DeliveryStatus replies route over the live
    // session instead of triggering a redundant redial.
    let warmup_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let mut warmup_received = 0_u64;
    while tokio::time::Instant::now() < warmup_deadline {
        match tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await {
            Ok(Some(_)) => warmup_received += 1,
            Ok(None) => break,
            Err(_) => {}
        }
    }
    evidence.record("direction-a-warmup-received", warmup_received);

    // Small DatabaseStore: one test RouterInfo fitting one SSU2 datagram.
    let small = make_test_router("127.0.0.1", 43201, 0, false);
    evidence.record("small-routerinfo-len", small.router_info.len());
    assert!(
        small.router_info.len() < 1000,
        "small fixture must fit one datagram"
    );
    write_file(&evidence_dir, "expected-small.ri", &small.router_info);
    let small_wire = database_store_wire(
        small.hash,
        &small.router_info,
        SMALL_STORE_MSG_ID,
        SMALL_STORE_REPLY_TOKEN,
        local.hash,
    );
    evidence.record("small-i2np-len", small_wire.len());
    evidence.record(
        "small-i2np-digest",
        digest_hex(i2pr_crypto::sha256(&small_wire).as_bytes()),
    );
    write_file(&evidence_dir, "sent-small.i2np", &small_wire);
    assert_eq!(
        service.send_i2np(
            i2pd_peer,
            EncodedI2npMessage::new(small_wire).expect("message"),
            Duration::from_secs(5),
        ),
        i2pr_runtime::Ssu2SendOutcome::Accepted
    );

    // Fragmented DatabaseStore: a test RouterInfo requiring SSU2 I2NP
    // fragmentation/reassembly across the independent boundary.
    let large = make_test_router("127.0.0.1", 43301, 10, false);
    evidence.record("large-routerinfo-len", large.router_info.len());
    assert!(
        large.router_info.len() > 1400,
        "large fixture must require fragmentation"
    );
    assert!(
        large.router_info.len() < 3072,
        "large fixture must fit the independent 3 KiB RouterInfo ceiling"
    );
    write_file(&evidence_dir, "expected-large.ri", &large.router_info);
    let large_wire = database_store_wire(
        large.hash,
        &large.router_info,
        LARGE_STORE_MSG_ID,
        LARGE_STORE_REPLY_TOKEN,
        local.hash,
    );
    evidence.record("large-i2np-len", large_wire.len());
    evidence.record(
        "large-i2np-digest",
        digest_hex(i2pr_crypto::sha256(&large_wire).as_bytes()),
    );
    write_file(&evidence_dir, "sent-large.i2np", &large_wire);
    assert_eq!(
        service.send_i2np(
            i2pd_peer,
            EncodedI2npMessage::new(large_wire).expect("message"),
            Duration::from_secs(5),
        ),
        i2pr_runtime::Ssu2SendOutcome::Accepted
    );
    // i2pd-side proof: each ingested DatabaseStore earns a direct
    // DeliveryStatus echoing our reply token over the same session.
    // The two echoes prove small + fragmented delivery i2pr -> i2pd
    // and the authenticated return path without depending on NetDB
    // flush hygiene (i2pd purges RIs it cannot keep connected).
    let (small_reply, large_reply) = collect_delivery_status(
        &mut handle,
        i2pd_peer,
        SMALL_STORE_REPLY_TOKEN,
        LARGE_STORE_REPLY_TOKEN,
    )
    .await;
    evidence.record("small-reply-len", small_reply.len());
    evidence.record(
        "small-reply-digest",
        digest_hex(i2pr_crypto::sha256(&small_reply).as_bytes()),
    );
    write_file(&evidence_dir, "reply-small.i2np", &small_reply);
    evidence.record("large-reply-len", large_reply.len());
    evidence.record(
        "large-reply-digest",
        digest_hex(i2pr_crypto::sha256(&large_reply).as_bytes()),
    );
    write_file(&evidence_dir, "reply-large.i2np", &large_reply);

    // Graceful termination returns the i2pr session/task baseline.
    established.link.close(TerminationCategory::LocalShutdown);
    wait_for_active(&service, 0, "direction-A close").await;
    let after_close = service.snapshot();
    assert_eq!(after_close.pending_outbound, 0);
    assert_eq!(after_close.pending_inbound, 0);
    assert!(after_close.sessions_closed > baseline.sessions_closed);
    evidence.record("direction-a-closed", after_close.sessions_closed);

    // Drain any in-band control (NewToken) without asserting on it here;
    // the token matrix is a later stage of this same test.
    let drain_deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < drain_deadline {
        match tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await {
            Ok(Some(_)) => {}
            _ => break,
        }
    }

    evidence.record("resource-active", after_close.active_sessions);
    evidence.record("resource-pending-out", after_close.pending_outbound);
    evidence.record("resource-pending-in", after_close.pending_inbound);
    evidence.record("resource-cheap-drops", after_close.cheap_drops);
    evidence.record("resource-auth-failures", after_close.auth_failures);
    evidence.record("resource-i2np-sent", after_close.i2np_sent);
    evidence.record("resource-i2np-received", after_close.i2np_received);
    evidence.write(&evidence_dir);

    service.shutdown();
    let _joined = scope.shutdown().await;
}
