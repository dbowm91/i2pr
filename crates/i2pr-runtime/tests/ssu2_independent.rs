//! Plan 161 independent IPv4 interop driver (i2pr side).
//!
//! A single ordered test drives the full Plan 161 matrix against one
//! ephemeral exact-pinned i2pd 2.61.0 process over real loopback UDP:
//!
//! - direction A (i2pr initiator -> i2pd responder) with tokenless
//!   Retry establishment, mutual authentication, one small and one
//!   fragmented DatabaseStore with direct DeliveryStatus replies
//!   proving bidirectional delivery, then graceful termination;
//! - valid-token establishment: a second dial over the cached NewToken
//!   skipping the Retry round trip, with one more small store proved
//!   by a DeliveryStatus echo;
//! - direction B (i2pd initiator -> i2pr responder): the driver keeps
//!   its identity/service up after direction A so the pinned peer,
//!   which learned our RouterInfo in-band, initiates a new session;
//!   small + fragmented stores and both DeliveryStatus echoes are
//!   proved over the responder-promoted session before graceful close;
//! - malformed boundary: short/oversized/random datagrams against the
//!   i2pr socket are rejected without session creation or unbounded
//!   state.
//!
//! Expired/invalid-token and spoofed-source rows remain local evidence
//! (Plan 156/158/159 suites); the external lane exercises only states
//! the unmodified peer reaches naturally.
//!
//! Echo-loss tolerance: the peer's DeliveryStatus echo is fire-and-forget,
//! so each data phase allows at most two send/collect attempts with fresh
//! fixtures/IDs/tokens per attempt (a late echo from an earlier attempt can
//! never satisfy a later one). The per-attempt pass criterion is unchanged
//! (exact token echoes over the same authenticated session link); a missing
//! echo after the final attempt stays a hard failure. Evidence rows/files
//! are committed for the successful attempt only, plus per-phase
//! `*-echo-attempts` counts.
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
use i2pr_transport::{
    EncodedI2npMessage, LinkId, MAX_I2NP_MESSAGE_BYTES, PeerId, TerminationCategory,
};
use i2pr_transport_ssu2::{IntroKey, Ssu2PublicKey, Ssu2RouterAddress, constants};
use rand_core::{OsRng, TryRngCore};

const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);
/// Bounded echo-loss tolerance: at most two send/collect attempts per
/// data phase, each with fresh fixtures/IDs/tokens. The peer's
/// DeliveryStatus echo is fire-and-forget, so one lost echo datagram
/// under host load must not fail a healthy session; the per-attempt
/// pass criterion is unchanged.
const ECHO_ATTEMPTS: u32 = 2;
/// Per-attempt message-ID/reply-token bump. Bases are spaced so the
/// bumped values stay distinct across attempts and phases.
const ATTEMPT_BUMP: u32 = 0x10;
const POLL_INTERVAL: Duration = Duration::from_millis(250);
/// How long direction B waits for the pinned peer to initiate a new
/// session after it learned our RouterInfo in direction A.
const DIRECTION_B_WAIT: Duration = Duration::from_secs(150);
/// Settle between consecutive sessions so the peer fully retires the
/// previous hash-keyed session before the next handshake starts.
const SETTLE_BETWEEN_SESSIONS: Duration = Duration::from_secs(15);
/// How long the driver waits for the peer's in-band NewToken to land
/// in the initiator cache before the cached-token dial.
const CACHED_TOKEN_WAIT: Duration = Duration::from_secs(15);

const SMALL_STORE_MSG_ID: u32 = 0x51A4_1101;
const LARGE_STORE_MSG_ID: u32 = 0x51A4_1102;
const SMALL_STORE_REPLY_TOKEN: u32 = 0x51A4_2001;
const LARGE_STORE_REPLY_TOKEN: u32 = 0x51A4_2002;
const CACHED_STORE_MSG_ID: u32 = 0x51A4_1103;
const CACHED_STORE_REPLY_TOKEN: u32 = 0x51A4_2003;
const B_SMALL_STORE_MSG_ID: u32 = 0x51A4_1104;
const B_LARGE_STORE_MSG_ID: u32 = 0x51A4_1105;
const B_SMALL_STORE_REPLY_TOKEN: u32 = 0x51A4_2004;
const B_LARGE_STORE_REPLY_TOKEN: u32 = 0x51A4_2005;
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

/// Logs one fixture's public routing hash (base64, unpadded to match
/// the peer log format) plus sizes so peer NetDB ingest lines can be
/// mapped back to driver fixtures.
fn log_fixture(tag: &str, hash: &Hash, router_info_len: usize, wire_len: usize) {
    eprintln!(
        "fixture {tag} hash={} ri_len={router_info_len} wire_len={wire_len}",
        i2p_b64_encode(hash.as_bytes()).trim_end_matches('=')
    );
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
/// Returns both reply bodies plus the exact session link they arrived
/// on (both replies must share one link), or `None` on timeout: the
/// peer's DeliveryStatus echo is fire-and-forget, so a lost echo
/// datagram under host load must be distinguishable from a wrong peer,
/// a wrong token, or split-link delivery (those stay hard failures).
/// i2pd answers each ingested DatabaseStore directly (tunnel 0) over the
/// same session, so the two replies prove small + fragmented delivery
/// i2pr -> i2pd and the authenticated return path in one round trip.
async fn collect_delivery_status(
    handle: &mut Ssu2ServiceHandle,
    service: &i2pr_runtime::Ssu2RuntimeService,
    peer: PeerId,
    small_token: u32,
    large_token: u32,
) -> Option<(Vec<u8>, Vec<u8>, LinkId)> {
    let deadline = tokio::time::Instant::now() + REPLY_TIMEOUT;
    let mut small: Option<(Vec<u8>, LinkId)> = None;
    let mut large: Option<(Vec<u8>, LinkId)> = None;
    let mut last_progress = tokio::time::Instant::now();
    while small.is_none() || large.is_none() {
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        if last_progress.elapsed() > Duration::from_secs(5) {
            last_progress = tokio::time::Instant::now();
            eprintln!(
                "collect progress: small={} large={} snapshot={:?}",
                small.is_some(),
                large.is_some(),
                service.snapshot()
            );
        }
        let inbound = match tokio::time::timeout(POLL_INTERVAL * 4, handle.next_inbound()).await {
            Ok(inbound) => inbound.expect("service live"),
            Err(_) => continue,
        };
        assert_eq!(inbound.peer, peer, "unexpected inbound peer");
        let bytes = inbound.bytes;
        // i2pd randomizes the short header message ID on send, so the
        // reply token is matched in the DeliveryStatus body (offset 9),
        // not in the transport header. Tokens are fresh per attempt, so
        // a late echo from an earlier attempt can never match here.
        if bytes.len() < 21 || bytes[0] != DELIVERY_STATUS_TYPE {
            continue;
        }
        let token = u32::from_be_bytes(bytes[9..13].try_into().expect("token bytes"));
        if token == small_token && small.is_none() {
            small = Some((bytes, inbound.link_id));
        } else if token == large_token && large.is_none() {
            large = Some((bytes, inbound.link_id));
        }
    }
    let (small_bytes, small_link) = small.expect("small reply");
    let (large_bytes, large_link) = large.expect("large reply");
    assert_eq!(
        small_link, large_link,
        "both DeliveryStatus replies must share one session link"
    );
    Some((small_bytes, large_bytes, small_link))
}

/// Single-reply variant for the cached-token dial (one store, one echo).
/// Timeout returns `None` for the same bounded-retry reason as above.
async fn collect_one_delivery_status(
    handle: &mut Ssu2ServiceHandle,
    service: &i2pr_runtime::Ssu2RuntimeService,
    peer: PeerId,
    token: u32,
) -> Option<(Vec<u8>, LinkId)> {
    let deadline = tokio::time::Instant::now() + REPLY_TIMEOUT;
    let mut last_progress = tokio::time::Instant::now();
    loop {
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        if last_progress.elapsed() > Duration::from_secs(5) {
            last_progress = tokio::time::Instant::now();
            eprintln!("collect-one progress: snapshot={:?}", service.snapshot());
        }
        let inbound = match tokio::time::timeout(POLL_INTERVAL * 4, handle.next_inbound()).await {
            Ok(inbound) => inbound.expect("service live"),
            Err(_) => continue,
        };
        assert_eq!(inbound.peer, peer, "unexpected inbound peer");
        let bytes = inbound.bytes;
        if bytes.len() < 21 || bytes[0] != DELIVERY_STATUS_TYPE {
            continue;
        }
        let seen = u32::from_be_bytes(bytes[9..13].try_into().expect("token bytes"));
        if seen == token {
            return Some((bytes, inbound.link_id));
        }
    }
}

/// Sends one small plus one fragmented DatabaseStore and collects both
/// direct DeliveryStatus echoes over the same authenticated session.
///
/// Bounded echo-loss tolerance (`ECHO_ATTEMPTS`): every attempt uses
/// fresh fixtures, message IDs, and reply tokens, so a late echo from
/// an earlier attempt can never satisfy a later one. The pass criterion
/// per attempt is unchanged: both token echoes, same session link.
/// Evidence rows/files are committed only for the successful attempt,
/// plus a `{tag}-echo-attempts` row recording how many attempts ran.
/// A missing echo after the final attempt stays a hard failure.
/// Returns the shared session link both echoes arrived on (needed for
/// the graceful per-direction close).
#[allow(clippy::too_many_arguments)]
async fn prove_store_pair(
    service: &i2pr_runtime::Ssu2RuntimeService,
    handle: &mut Ssu2ServiceHandle,
    evidence: &mut Evidence,
    evidence_dir: &Path,
    peer: PeerId,
    gateway: Hash,
    tag: &str,
    name_small: &str,
    name_large: &str,
    small_port: u16,
    large_port: u16,
    small_msg_base: u32,
    large_msg_base: u32,
    small_token_base: u32,
    large_token_base: u32,
) -> LinkId {
    let mut attempt: u32 = 0;
    loop {
        assert!(
            attempt < ECHO_ATTEMPTS,
            "{tag} DeliveryStatus echoes missing after {ECHO_ATTEMPTS} bounded attempts"
        );
        let bump = attempt * ATTEMPT_BUMP;
        let small_token = small_token_base + bump;
        let large_token = large_token_base + bump;
        // Small DatabaseStore: one test RouterInfo fitting one SSU2 datagram.
        let small = make_test_router("127.0.0.1", small_port, 0, false);
        assert!(
            small.router_info.len() < 1000,
            "small fixture must fit one datagram"
        );
        let small_wire = database_store_wire(
            small.hash,
            &small.router_info,
            small_msg_base + bump,
            small_token,
            gateway,
        );
        log_fixture(
            &format!("{tag}-small"),
            &small.hash,
            small.router_info.len(),
            small_wire.len(),
        );
        assert_eq!(
            service.send_i2np(
                peer,
                EncodedI2npMessage::new(small_wire.clone()).expect("message"),
                Duration::from_secs(5),
            ),
            i2pr_runtime::Ssu2SendOutcome::Accepted
        );
        // Fragmented DatabaseStore: a test RouterInfo requiring SSU2 I2NP
        // fragmentation/reassembly across the independent boundary.
        let large = make_test_router("127.0.0.1", large_port, 10, false);
        assert!(
            large.router_info.len() > 1400,
            "large fixture must require fragmentation"
        );
        assert!(
            large.router_info.len() < 3072,
            "large fixture must fit the independent 3 KiB RouterInfo ceiling"
        );
        let large_wire = database_store_wire(
            large.hash,
            &large.router_info,
            large_msg_base + bump,
            large_token,
            gateway,
        );
        log_fixture(
            &format!("{tag}-large"),
            &large.hash,
            large.router_info.len(),
            large_wire.len(),
        );
        assert_eq!(
            service.send_i2np(
                peer,
                EncodedI2npMessage::new(large_wire.clone()).expect("message"),
                Duration::from_secs(5),
            ),
            i2pr_runtime::Ssu2SendOutcome::Accepted
        );
        // i2pd-side proof: each ingested DatabaseStore earns a direct
        // DeliveryStatus echoing our reply token over the same session.
        // The two echoes prove small + fragmented delivery i2pr -> i2pd
        // and the authenticated return path without depending on NetDB
        // flush hygiene (i2pd purges RIs it cannot keep connected).
        let collected =
            collect_delivery_status(handle, service, peer, small_token, large_token).await;
        let Some((small_reply, large_reply, link)) = collected else {
            attempt += 1;
            continue;
        };
        evidence.record(
            &format!("{name_small}-routerinfo-len"),
            small.router_info.len(),
        );
        write_file(
            evidence_dir,
            &format!("expected-{name_small}.ri"),
            &small.router_info,
        );
        evidence.record(&format!("{name_small}-i2np-len"), small_wire.len());
        evidence.record(
            &format!("{name_small}-i2np-digest"),
            digest_hex(i2pr_crypto::sha256(&small_wire).as_bytes()),
        );
        write_file(
            evidence_dir,
            &format!("sent-{name_small}.i2np"),
            &small_wire,
        );
        evidence.record(
            &format!("{name_large}-routerinfo-len"),
            large.router_info.len(),
        );
        write_file(
            evidence_dir,
            &format!("expected-{name_large}.ri"),
            &large.router_info,
        );
        evidence.record(&format!("{name_large}-i2np-len"), large_wire.len());
        evidence.record(
            &format!("{name_large}-i2np-digest"),
            digest_hex(i2pr_crypto::sha256(&large_wire).as_bytes()),
        );
        write_file(
            evidence_dir,
            &format!("sent-{name_large}.i2np"),
            &large_wire,
        );
        evidence.record(&format!("{name_small}-reply-len"), small_reply.len());
        evidence.record(
            &format!("{name_small}-reply-digest"),
            digest_hex(i2pr_crypto::sha256(&small_reply).as_bytes()),
        );
        write_file(
            evidence_dir,
            &format!("reply-{name_small}.i2np"),
            &small_reply,
        );
        evidence.record(&format!("{name_large}-reply-len"), large_reply.len());
        evidence.record(
            &format!("{name_large}-reply-digest"),
            digest_hex(i2pr_crypto::sha256(&large_reply).as_bytes()),
        );
        write_file(
            evidence_dir,
            &format!("reply-{name_large}.i2np"),
            &large_reply,
        );
        evidence.record(&format!("{tag}-echo-attempts"), attempt + 1);
        return link;
    }
}

/// Waits until the initiator token cache holds the peer's in-band
/// NewToken announcement from the previous establishment.
async fn wait_for_cached_token(service: &i2pr_runtime::Ssu2RuntimeService) {
    let deadline = tokio::time::Instant::now() + CACHED_TOKEN_WAIT;
    loop {
        if service.snapshot().cached_tokens >= 1 {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "peer NewToken never landed in the initiator cache"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Waits for the pinned peer to initiate a new session (direction B):
/// exactly one active session with a fresh establishment beyond the
/// recorded baseline.
async fn wait_for_peer_initiated_session(
    service: &i2pr_runtime::Ssu2RuntimeService,
    established_baseline: u64,
    what: &str,
) {
    let deadline = tokio::time::Instant::now() + DIRECTION_B_WAIT;
    let mut last_progress = tokio::time::Instant::now();
    loop {
        let snapshot = service.snapshot();
        if snapshot.active_sessions == 1 && snapshot.sessions_established > established_baseline {
            return;
        }
        if last_progress.elapsed() > Duration::from_secs(10) {
            last_progress = tokio::time::Instant::now();
            eprintln!("direction-b wait progress: snapshot={snapshot:?}");
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "pinned peer did not initiate a session ({what})"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
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
#[ignore = "Plan 161: requires exact-pinned external i2pd environment"]
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
    log_fixture("local-session", &local.hash, local.router_info.len(), 0);

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

    // Small + fragmented DatabaseStores with direct DeliveryStatus echoes
    // per direction (bounded echo-loss retry inside; evidence committed
    // for the successful attempt only).
    let _ = prove_store_pair(
        &service,
        &mut handle,
        &mut evidence,
        &evidence_dir,
        i2pd_peer,
        local.hash,
        "direction-a",
        "small",
        "large",
        43201,
        43301,
        SMALL_STORE_MSG_ID,
        LARGE_STORE_MSG_ID,
        SMALL_STORE_REPLY_TOKEN,
        LARGE_STORE_REPLY_TOKEN,
    )
    .await;

    // Graceful termination returns the i2pr session/task baseline.
    established.link.close(TerminationCategory::LocalShutdown);
    wait_for_active(&service, 0, "direction-A close").await;
    let after_close = service.snapshot();
    assert_eq!(after_close.pending_outbound, 0);
    assert_eq!(after_close.pending_inbound, 0);
    assert!(after_close.sessions_closed > baseline.sessions_closed);
    evidence.record("direction-a-closed", after_close.sessions_closed);
    eprintln!("phase: direction-a closed snapshot={after_close:?}");

    // Drain any in-band control without asserting on it here, then let
    // the peer retire direction A before it initiates direction B: its
    // session table is keyed by router hash, and a fresh initiation
    // must not race the previous session's teardown.
    // The direction-B baseline is captured BEFORE the settle sleep: the
    // pinned peer may redial during the settle window itself, and a
    // post-settle baseline would already include that initiation and
    // deadlock the wait below expecting a third session.
    let b_baseline = service.snapshot().sessions_established;
    let drain_deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < drain_deadline {
        match tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await {
            Ok(Some(_)) => {}
            _ => break,
        }
    }
    tokio::time::sleep(SETTLE_BETWEEN_SESSIONS).await;

    // ---- Direction B: i2pd initiator -> i2pr responder ------------------
    // The pinned peer learned our RouterInfo in-band during direction
    // A. Keep the same identity/service up and wait for it to initiate
    // a fresh session: token/source validation, Retry, and promotion
    // all run through the normal responder path (no test bypass).
    // `b_baseline` predates the settle sleep above, so a redial that
    // landed during the settle still counts as the fresh initiation.
    evidence.record("direction-b-wait-secs", DIRECTION_B_WAIT.as_secs());
    wait_for_peer_initiated_session(&service, b_baseline, "i2pd redial").await;
    let b_promoted = service.snapshot();
    assert!(b_promoted.sessions_established > b_baseline);
    evidence.record("direction-b-established", b_promoted.sessions_established);
    eprintln!("phase: direction-b promoted snapshot={b_promoted:?}");
    // Warmup: let the peer finish registering the session it just
    // initiated before the first store crosses it.
    let b_warmup_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let mut b_warmup_received = 0_u64;
    while tokio::time::Instant::now() < b_warmup_deadline {
        match tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await {
            Ok(Some(_)) => b_warmup_received += 1,
            Ok(None) => break,
            Err(_) => {}
        }
    }
    evidence.record("direction-b-warmup-received", b_warmup_received);

    // Bidirectional proof over the responder-promoted session: small +
    // fragmented stores i2pr -> i2pd (the peer answers with direct
    // DeliveryStatus echoes), with fresh fixtures/IDs/tokens so no
    // earlier bytes can be mistaken for direction-B proof. Bounded
    // echo-loss retry inside; the returned link is the exact session
    // link both echoes shared.
    let b_link = prove_store_pair(
        &service,
        &mut handle,
        &mut evidence,
        &evidence_dir,
        i2pd_peer,
        local.hash,
        "direction-b",
        "b-small",
        "b-large",
        43203,
        43302,
        B_SMALL_STORE_MSG_ID,
        B_LARGE_STORE_MSG_ID,
        B_SMALL_STORE_REPLY_TOKEN,
        B_LARGE_STORE_REPLY_TOKEN,
    )
    .await;

    // Graceful termination of the responder-promoted session through
    // the exact link handle observed on the wire, then baseline.
    service.close_ssu2(b_link, TerminationCategory::LocalShutdown);
    wait_for_active(&service, 0, "direction-B close").await;
    let after_b = service.snapshot();
    assert_eq!(after_b.pending_outbound, 0);
    assert_eq!(after_b.pending_inbound, 0);
    assert!(after_b.sessions_closed > after_close.sessions_closed);
    evidence.record("direction-b-closed", after_b.sessions_closed);
    eprintln!("phase: direction-b closed snapshot={:?}", after_b);
    // Settle: let the peer fully retire the responder-promoted session
    // before our next initiation, so its hash-keyed session table holds
    // no stale entry that could swallow the fresh handshake.
    tokio::time::sleep(SETTLE_BETWEEN_SESSIONS).await;

    // ---- Valid-token dial (no Retry round trip) -------------------------
    // The SessionCreated NewToken announcement cached from direction A
    // (52-minute peer lifetime) is presented here, after direction B,
    // so our initiations never race the peer's spontaneous redials.
    // This is the externally exercisable half of the Plan 161
    // token/Retry matrix (expired/invalid/source rows stay
    // local-evidence-only in the Plan 156/158 suites).
    wait_for_cached_token(&service).await;
    evidence.record("token-cached", service.snapshot().cached_tokens);
    let cached_target = Ssu2DialTarget::new(
        i2pd_peer,
        i2pd_hash,
        i2pd_endpoint,
        responder_static,
        responder_intro,
    )
    .expect("dial target");
    let cached: Ssu2EstablishedLink = service
        .dial_ssu2(cached_target, DIAL_TIMEOUT, &CancellationToken::new())
        .await
        .expect("cached-token dial");
    assert!(
        cached.used_cached_token,
        "second dial must take the cached-token path"
    );
    evidence.record("direction-a2-cached-token", "true");
    wait_for_active(&service, 1, "cached-token dial").await;
    // Warmup: same asynchronous peer-registration settle as direction
    // A, so the first store routes over the live session.
    let cached_warmup_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < cached_warmup_deadline {
        match tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await {
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => {}
        }
    }
    // Fresh fixture per attempt: earlier keys are already in the peer
    // NetDB, so a duplicate store may earn no DeliveryStatus echo.
    // Bounded echo-loss retry like the pair phases: fresh fixture/ID/
    // token per attempt, same-session proof, hard failure at the end.
    let mut cached_attempt: u32 = 0;
    loop {
        assert!(
            cached_attempt < ECHO_ATTEMPTS,
            "cached-token DeliveryStatus echo missing after {ECHO_ATTEMPTS} bounded attempts"
        );
        let bump = cached_attempt * ATTEMPT_BUMP;
        let cached_small = make_test_router("127.0.0.1", 43202, 0, false);
        assert!(
            cached_small.router_info.len() < 1000,
            "cached fixture must fit one datagram"
        );
        let cached_wire = database_store_wire(
            cached_small.hash,
            &cached_small.router_info,
            CACHED_STORE_MSG_ID + bump,
            CACHED_STORE_REPLY_TOKEN + bump,
            local.hash,
        );
        log_fixture(
            "cached-small",
            &cached_small.hash,
            cached_small.router_info.len(),
            cached_wire.len(),
        );
        assert_eq!(
            service.send_i2np(
                i2pd_peer,
                EncodedI2npMessage::new(cached_wire.clone()).expect("message"),
                Duration::from_secs(5),
            ),
            i2pr_runtime::Ssu2SendOutcome::Accepted
        );
        let collected = collect_one_delivery_status(
            &mut handle,
            &service,
            i2pd_peer,
            CACHED_STORE_REPLY_TOKEN + bump,
        )
        .await;
        let Some((cached_reply, _)) = collected else {
            cached_attempt += 1;
            continue;
        };
        evidence.record("cached-routerinfo-len", cached_small.router_info.len());
        write_file(
            &evidence_dir,
            "expected-cached.ri",
            &cached_small.router_info,
        );
        evidence.record("cached-i2np-len", cached_wire.len());
        evidence.record(
            "cached-i2np-digest",
            digest_hex(i2pr_crypto::sha256(&cached_wire).as_bytes()),
        );
        evidence.record("cached-reply-len", cached_reply.len());
        evidence.record(
            "cached-reply-digest",
            digest_hex(i2pr_crypto::sha256(&cached_reply).as_bytes()),
        );
        write_file(&evidence_dir, "reply-cached.i2np", &cached_reply);
        evidence.record("cached-echo-attempts", cached_attempt + 1);
        break;
    }
    cached.link.close(TerminationCategory::LocalShutdown);
    wait_for_active(&service, 0, "cached-token close").await;
    let after_cached = service.snapshot();
    assert_eq!(after_cached.pending_outbound, 0);
    assert_eq!(after_cached.pending_inbound, 0);
    assert!(after_cached.sessions_closed > after_b.sessions_closed);
    evidence.record("direction-a2-closed", after_cached.sessions_closed);
    eprintln!("phase: cached-token closed snapshot={:?}", after_cached);
    tokio::time::sleep(SETTLE_BETWEEN_SESSIONS).await;

    // ---- Malformed/spoof boundary at the i2pr socket -------------------
    // Short, oversized, and random datagrams from a raw loopback socket
    // must be rejected by cheap/auth checks: no session, no unbounded
    // state, bounded rejection counters only.
    let malformed_before = service.snapshot();
    let probe = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("probe socket");
    probe
        .send_to(&[0xFF_u8, 0x00, 0x01], bind)
        .await
        .expect("send short");
    probe
        .send_to(&[0xAA_u8; 4000], bind)
        .await
        .expect("send oversized");
    probe
        .send_to(&[0x55_u8; 64], bind)
        .await
        .expect("send random");
    tokio::time::sleep(Duration::from_secs(2)).await;
    let malformed_after = service.snapshot();
    assert_eq!(malformed_after.active_sessions, 0);
    assert_eq!(malformed_after.pending_outbound, 0);
    assert_eq!(malformed_after.pending_inbound, 0);
    let rejections_before = malformed_before.cheap_drops
        + malformed_before.auth_failures
        + malformed_before.protocol_drops;
    let rejections_after = malformed_after.cheap_drops
        + malformed_after.auth_failures
        + malformed_after.protocol_drops;
    assert!(
        rejections_after >= rejections_before.saturating_add(3),
        "all three malformed datagrams must be rejected cheaply"
    );
    evidence.record("malformed-cheap-drops", malformed_after.cheap_drops);
    evidence.record("malformed-auth-failures", malformed_after.auth_failures);
    evidence.record("malformed-protocol-drops", malformed_after.protocol_drops);

    evidence.record("resource-active", malformed_after.active_sessions);
    evidence.record("resource-pending-out", malformed_after.pending_outbound);
    evidence.record("resource-pending-in", malformed_after.pending_inbound);
    evidence.record("resource-cheap-drops", malformed_after.cheap_drops);
    evidence.record("resource-auth-failures", malformed_after.auth_failures);
    evidence.record("resource-i2np-sent", malformed_after.i2np_sent);
    evidence.record("resource-i2np-received", malformed_after.i2np_received);
    evidence.record(
        "resource-sessions-established",
        malformed_after.sessions_established,
    );
    evidence.record("resource-sessions-closed", malformed_after.sessions_closed);
    evidence.write(&evidence_dir);

    service.shutdown();
    let _joined = scope.shutdown().await;
}
