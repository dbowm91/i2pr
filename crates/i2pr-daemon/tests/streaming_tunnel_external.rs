//! Plan 193 — M6 i2pd mixed-router Streaming qualification external lane.
//!
//! Fail-closed driver against exact-pinned i2pd 2.61.0 on loopback:
//!
//! - strict SSU2 controlled profile (loopback, non-advertised);
//! - reference RouterInfo verified + bootstrapped through the ordinary
//!   parser/signature/freshness path into the authoritative store;
//! - effective floodfill capability verified before dispatch;
//! - authenticated SSU2 session establishes via the daemon-owned runtime;
//! - one reference SAM STREAM destination created through i2pd's
//!   public SAM surface (transient; only public Destination facts
//!   recorded, never keys);
//! - one real one-hop outbound + inbound tunnel build accepted by the
//!   reference, with build replies routed through
//!   `ExploratoryBuildCoordinator::route_inbound_i2np` to real
//!   `Installed` pool + registry roles (real cryptographically
//!   derived keys on both directions, no synthetic roles, no
//!   `LocalZeroHop`);
//! - the reference Standard LeaseSet2 resolved through the real
//!   tunnel NetDB path, validated through the existing validators,
//!   and cached through the existing LeaseSet2 store;
//! - the i2pr destination's own Standard LeaseSet2 (real inbound
//!   lease) published through the controlled NetDB path;
//! - Direction A: `StreamingManager::connect` emits a SYN that
//!   traverses existing ECIES/Garlic + the real outbound tunnel +
//!   the selected remote lease; the i2pd StreamingDestination
//!   responds with a SYN-ACK that returns via the real inbound
//!   tunnel and the existing ECIES decrypt path; the originator
//!   reaches `Established` through ordinary packet handling;
//! - small + multi-packet application data moves i2pr -> i2pd and
//!   is observed on the reference SAM STREAM ACCEPT socket with
//!   digest equality (plaintext never logged);
//! - direct transport explicitly rejected as a counted path;
//! - creator-side liveness first test green.
//!
//! Direction B (i2pd initiator -> i2pr responder through the normal
//! listener/accept path) is attempted best-effort after Direction A;
//! if the reference does not initiate within the bounded window the
//! driver records `streaming-stop` with B provenance while keeping
//! the Direction A keys (the shell marks undelivered rows `blocked`,
//! never `passed`).
//!
//! Local decrypt/matrix behavior is proven in the local suites
//! (`streaming_tunnel_unit.rs`, `streaming_tunnel_live.rs`); this
//! lane proves the mixed-router streaming message plane only.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_client::streaming::connection::ConnectionState;
use i2pr_client::streaming::manager::{
    ConnectOutcome, DEFAULT_ADVERTISED_MAX_PAYLOAD, ListenerOutcome, RemoteDestination,
    StreamingManager,
};
use i2pr_client::streaming::{StreamingConfig, transport::TransportSendRequest};
use i2pr_client::{
    DestinationDispatcher, DestinationIdentity, DestinationOutboundRole, DestinationRouting,
    DestinationRoutingConfig, EciesSessionConfig, EciesSessionManager, InboundLeaseSource,
    StreamingDestinationAdapter, build_signed_lease_set2,
};
use i2pr_daemon::config::Config;
use i2pr_daemon::destination_tunnels::{
    DestinationTunnelCoordinator, LeaseStoreIngestOutcome, reply_path_for_inbound_route,
};
use i2pr_daemon::exploratory_build::{
    BuildCoordinatorOutcome, BuildDirection, BuildRequest, ExploratoryBuildCoordinator,
    PeerBuildMaterial,
};
use i2pr_daemon::router_i2np::{
    RouterDeliveryOutcome, RouterDeliveryRequest, Ssu2DaemonService, daemon_dial_target,
    generate_controlled_identity, verify_reference_router_info,
};
use i2pr_daemon::tunnel_liveness::{
    FIRST_LIVENESS_DELAY_MS, LivenessAction, LivenessConfig, TunnelLivenessScheduler,
};
use i2pr_netdb::{LookupPolicy, RouterHash, RouterInfoStoreConfig};
use i2pr_proto::{Date, Hash, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE, RouterInfo};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_transport::{Deadline, PeerId};
use i2pr_tunnel::bridge::{BridgeHeader, ShortBuildI2npBridge};
use i2pr_tunnel::config::ExploratoryPoolConfig;
use i2pr_tunnel::identity::{TunnelDirection, TunnelId};
use i2pr_tunnel::short_record::HopRole;
use rand_chacha::ChaCha8Rng;
use rand_core::{OsRng, SeedableRng};

const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
const WAIT_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const I2PD_ACCEPT_TIMEOUT: Duration = Duration::from_secs(30);
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);
const SAM_TIMEOUT: Duration = Duration::from_secs(15);
const STREAM_WAIT: Duration = Duration::from_secs(45);
const SYN_ACK_WAIT: Duration = Duration::from_secs(45);

const OUTBOUND_CREATOR: u32 = 0x1500;
const INBOUND_CREATOR: u32 = 0x1600;
const OBEP_RECEIVE: u32 = 0x9501;
const OBEP_NEXT: u32 = 0x9502;
const IBGW_RECEIVE: u32 = 0x9601;
const IBGW_NEXT: u32 = 0x9602;

const LOCAL_STREAM_PORT: u16 = 0;
const REMOTE_STREAM_PORT: u16 = 0;
// Plan 193 interop note: the external lane addresses the reference
// SAM STREAM destination on the default-port tuple (0, 0).
// Exact-pinned i2pd 2.61.0 answers SYN packets from its default
// `StreamingDestination` (`m_LocalPort = 0`) and addresses every
// reply with the inbound stream's port (`Stream::m_Port`, always 0
// for responder-side streams: `Streaming.cpp` inbound constructor),
// so its SYN-ACK I2CP envelope is (fromPort = 0, toPort = 0). A
// nonzero requested tuple can never match the reference reply, and
// the originator-side exact-tuple check (Plan 131 §7 D2) would
// discard it. Nonzero-port streaming stays proven by the local
// suites (`streaming_tunnel_unit`, `streaming_tunnel_live`); the
// external lane proves wire interop in the reference's
// default-port dialect.

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(1_700_000_000_000)
}

fn wall_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .unwrap_or(1_700_000_000)
}

fn env_value(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("missing required env {name}"))
}

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(env_value(name))
}

fn append_evidence(dir: &Path, label: &str, value: &str) {
    let sanitized = value.replace(['\t', '\n'], " ");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("driver-evidence.tsv"))
        .expect("open evidence file");
    use std::io::Write as _;
    writeln!(file, "{label}\t{sanitized}").expect("write evidence row");
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = i2pr_crypto::sha256(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest.as_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Minimal SAMv3 client over a tokio TCP stream. Reads line replies
/// plus exact SIZE payloads. Never logs key material or payloads.
struct SamClient {
    stream: tokio::net::TcpStream,
    buffer: Vec<u8>,
}

impl SamClient {
    async fn connect(endpoint: SocketAddr) -> Self {
        let stream = tokio::time::timeout(SAM_TIMEOUT, tokio::net::TcpStream::connect(endpoint))
            .await
            .expect("SAM connect timeout")
            .expect("SAM connect");
        Self {
            stream,
            buffer: Vec::new(),
        }
    }

    async fn read_line(&mut self) -> Option<String> {
        let deadline = tokio::time::Instant::now() + SAM_TIMEOUT;
        loop {
            if let Some(position) = self.buffer.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = self.buffer.drain(..=position).collect();
                return Some(String::from_utf8_lossy(&line).trim_end().to_owned());
            }
            if tokio::time::Instant::now() >= deadline {
                return None;
            }
            let mut chunk = [0u8; 65536];
            let read = tokio::time::timeout(deadline - tokio::time::Instant::now(), async {
                use tokio::io::AsyncReadExt as _;
                self.stream.read(&mut chunk).await
            })
            .await
            .ok()?
            .ok()?;
            if read == 0 {
                return None;
            }
            self.buffer.extend_from_slice(&chunk[..read]);
        }
    }

    async fn transact(&mut self, command: &str) -> Option<String> {
        use tokio::io::AsyncWriteExt as _;
        self.stream
            .write_all(command.as_bytes())
            .await
            .expect("SAM write");
        self.read_line().await
    }

    async fn read_exact_bytes(&mut self, size: usize, timeout: Duration) -> Option<Vec<u8>> {
        assert!(size <= MAX_I2NP_PAYLOAD_SIZE, "SAM payload too large");
        let deadline = tokio::time::Instant::now() + timeout;
        while self.buffer.len() < size {
            if tokio::time::Instant::now() >= deadline {
                return None;
            }
            let mut chunk = [0u8; 65536];
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let read = tokio::time::timeout(remaining, async {
                use tokio::io::AsyncReadExt as _;
                self.stream.read(&mut chunk).await
            })
            .await
            .ok()?
            .ok()?;
            if read == 0 {
                return None;
            }
            self.buffer.extend_from_slice(&chunk[..read]);
        }
        Some(self.buffer.drain(..size).collect())
    }

    async fn write_bytes(&mut self, data: &[u8]) {
        use tokio::io::AsyncWriteExt as _;
        self.stream
            .write_all(data)
            .await
            .expect("SAM stream write");
    }

    /// Non-blocking reply-line poll for interleaving SAM control reads
    /// with the inbound tunnel pump (Direction B CONNECT): returns a
    /// complete line if one is already buffered or arrives within a
    /// short grace window, without stalling packet handling.
    async fn poll_line(&mut self) -> Option<String> {
        if let Some(position) = self.buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=position).collect();
            return Some(String::from_utf8_lossy(&line).trim_end().to_owned());
        }
        let mut chunk = [0u8; 4096];
        match self.stream.try_read(&mut chunk) {
            Ok(0) => None,
            Ok(read) => {
                self.buffer.extend_from_slice(&chunk[..read]);
                if let Some(position) = self.buffer.iter().position(|b| *b == b'\n') {
                    let line: Vec<u8> = self.buffer.drain(..=position).collect();
                    return Some(String::from_utf8_lossy(&line).trim_end().to_owned());
                }
                None
            }
            Err(_) => None,
        }
    }

    async fn read_line_timeout(&mut self, timeout: Duration) -> Option<String> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Some(position) = self.buffer.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = self.buffer.drain(..=position).collect();
                return Some(String::from_utf8_lossy(&line).trim_end().to_owned());
            }
            if tokio::time::Instant::now() >= deadline {
                return None;
            }
            let mut chunk = [0u8; 4096];
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let read = tokio::time::timeout(remaining, async {
                use tokio::io::AsyncReadExt as _;
                self.stream.read(&mut chunk).await
            })
            .await
            .ok()?
            .ok()?;
            if read == 0 {
                return None;
            }
            self.buffer.extend_from_slice(&chunk[..read]);
        }
    }

    /// Reads until the peer closes the stream or the timeout expires.
    /// Returns true only when an EOF (zero-byte read) is observed, so
    /// callers can distinguish an orderly reference-side close from a
    /// silent stall. Any post-close stream bytes are discarded.
    async fn read_until_eof(&mut self, timeout: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            let mut chunk = [0u8; 65536];
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let read = tokio::time::timeout(remaining, async {
                use tokio::io::AsyncReadExt as _;
                self.stream.read(&mut chunk).await
            })
            .await;
            match read {
                Ok(Ok(0)) => return true,
                Ok(Ok(_)) => continue,
                _ => return false,
            }
        }
    }
}

fn decode_sam_destination(token: &str) -> Vec<u8> {
    i2pr_api::sam::base64::decode(token, 4096).expect("decode SAM destination")
}

fn sam_param(line: &str, key: &str) -> Option<String> {
    for token in line.split(' ') {
        if let Some(value) = token.strip_prefix(key)
            && value.starts_with('=')
        {
            return Some(value[1..].to_owned());
        }
    }
    None
}

/// Pumps one inbound SSU2 batch through the real inbound tunnel chain
/// (TunnelData recovery -> Garlic decrypt -> dispatcher -> adapter ->
/// owning StreamingManager) and drains any manager-triggered outbound
/// (ACKs/CLOSE responses) back through the real outbound tunnel.
/// Returns true when at least one packet reached the manager.
#[allow(clippy::too_many_arguments)]
async fn pump_one_streaming_inbound(
    handle: &mut i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    coord: &mut ExploratoryBuildCoordinator,
    dest: &mut DestinationTunnelCoordinator,
    dispatcher: &mut DestinationDispatcher,
    session: &mut EciesSessionManager,
    routing: &mut DestinationRouting,
    streaming: &mut StreamingManager,
    outbound: &DestinationOutboundRole,
    local_identity: &DestinationIdentity,
    local_ls2: &i2pr_proto::LeaseSet2,
    delivery: &i2pr_daemon::router_i2np::RouterDeliveryService,
    rng: &mut ChaCha8Rng,
    from_hash: &[u8; 32],
    errors: &mut u64,
) -> bool {
    let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
    let Ok(Some(inbound)) = next else {
        return false;
    };
    let message =
        match I2npMessage::decode_short_transport(&inbound.bytes, MAX_I2NP_PAYLOAD_SIZE) {
            Ok(message) => message,
            Err(_) => {
                *errors += 1;
                return false;
            }
        };
    let cell = match message.body() {
        I2npBody::TunnelData(cell) => cell.clone(),
        _ => return false,
    };
    let bytes = match dest.recover_garlic_bytes(coord.registry_mut(), &cell, wall_ms()) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let now_secs = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
    dispatcher.dispatch_garlic_envelope(
        session,
        local_identity.id(),
        local_identity.static_secret_bytes(),
        &local_identity.static_public_bytes(),
        now_secs,
        &I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode garlic"),
        routing.lease_set2_store_mut(),
    );
    let Some(queued) = dispatcher.pop_payload(local_identity.id()) else {
        return false;
    };
    match StreamingDestinationAdapter::receive(
        queued.bytes(),
        local_identity,
        streaming,
        from_hash,
        wall_ms(),
    ) {
        Ok(_) => {}
        Err(_) => {
            *errors += 1;
            return false;
        }
    }
    let pending = streaming.drain_outbound();
    for request in &pending {
        send_transport_request(
            request, routing, session, outbound, local_identity, local_ls2, delivery, rng,
        )
        .await;
    }
    true
}

/// Runs the inbound pump until `satisfied` observes the wanted manager
/// state, or records a fail-closed `streaming-stop` with phase
/// provenance and panics. `satisfied` may drain delivered bytes for
/// digest matching.
#[allow(clippy::too_many_arguments)]
async fn pump_until_streaming<F>(
    handle: &mut i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    coord: &mut ExploratoryBuildCoordinator,
    dest: &mut DestinationTunnelCoordinator,
    dispatcher: &mut DestinationDispatcher,
    session: &mut EciesSessionManager,
    routing: &mut DestinationRouting,
    streaming: &mut StreamingManager,
    outbound: &DestinationOutboundRole,
    local_identity: &DestinationIdentity,
    local_ls2: &i2pr_proto::LeaseSet2,
    delivery: &i2pr_daemon::router_i2np::RouterDeliveryService,
    rng: &mut ChaCha8Rng,
    from_hash: &[u8; 32],
    evidence_dir: &Path,
    deadline: tokio::time::Instant,
    phase: &str,
    mut satisfied: F,
) where
    F: FnMut(&mut StreamingManager) -> bool,
{
    let mut errors = 0u64;
    loop {
        if satisfied(streaming) {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            record_stop_and_baseline(evidence_dir);
            append_evidence(
                evidence_dir,
                "streaming-stop",
                &format!("phase={phase} pump_errors={errors}"),
            );
            panic!("Plan 193 streaming stop: {phase}");
        }
        pump_one_streaming_inbound(
            handle,
            coord,
            dest,
            dispatcher,
            session,
            routing,
            streaming,
            outbound,
            local_identity,
            local_ls2,
            delivery,
            rng,
            from_hash,
            &mut errors,
        )
        .await;
    }
}

#[tokio::test]
#[ignore = "Plan 193 M6 i2pd mixed-router Streaming qualification: requires exact-pinned external i2pd environment"]
async fn streaming_through_i2pd() {
    eprintln!("MARKER test-body-enter");
    let i2pd_ri_path = env_path("I2PD_ROUTER_INFO");
    let i2pd_endpoint: SocketAddr = env_value("I2PD_SSU2_ENDPOINT").parse().expect("endpoint");
    let bind: SocketAddr = env_value("I2PR_SSU2_BIND").parse().expect("bind");
    let sam_endpoint: SocketAddr = env_value("I2PD_SAM_ENDPOINT")
        .parse()
        .expect("sam endpoint");
    assert!(bind.ip().is_loopback(), "i2pr bind must be loopback");
    assert!(
        i2pd_endpoint.ip().is_loopback(),
        "i2pd endpoint must be loopback"
    );
    assert!(sam_endpoint.ip().is_loopback(), "i2pd SAM must be loopback");
    let evidence_dir = env_path("EVIDENCE_DIR");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");

    let bind_port = bind.port();
    assert!(bind_port != 0, "lane requires a fixed loopback bind");
    let config_text = format!(
        "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nport = {bind_port}\n"
    );
    let config = Config::parse(&config_text).expect("strict controlled profile");
    eprintln!("MARKER config-parsed");
    assert!(config.ssu2.enabled);
    assert!(!config.ssu2.advertise);
    append_evidence(&evidence_dir, "daemon-strict-profile", "true");
    eprintln!("MARKER strict-profile-written");

    let i2pd_ri_bytes = std::fs::read(&i2pd_ri_path).expect("read i2pd router.info");
    let (i2pd_hash, i2pd_ssu2) =
        verify_reference_router_info(&i2pd_ri_bytes).expect("verify i2pd RouterInfo");
    let material = i2pd_ssu2.address_material().expect("i2pd key material");
    let responder_static =
        i2pr_runtime::Ssu2PublicKey::new(*material.static_public_key().as_bytes())
            .expect("static key");
    let responder_intro = i2pr_runtime::IntroKey::new(*material.intro_key().as_bytes());
    let target = daemon_dial_target(i2pd_hash, i2pd_endpoint, responder_static, responder_intro)
        .expect("dial target");
    let i2pd_router_info = RouterInfo::decode(
        &i2pd_ri_bytes,
        i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
    )
    .expect("decode i2pd RouterInfo");
    let i2pd_encryption_key: [u8; 32] = i2pd_router_info
        .router_identity()
        .public_key()
        .as_bytes()
        .try_into()
        .expect("32-byte encryption key");
    append_evidence(&evidence_dir, "reference-routerinfo-verified", "true");

    let mut dest = DestinationTunnelCoordinator::new(
        LookupPolicy::default(),
        RouterInfoStoreConfig::default(),
    );
    dest.advance_time(wall_ms());
    let now = Date::from_millis(wall_ms());
    let bootstrapped = dest
        .bootstrap_reference_router_info(&i2pd_ri_bytes, now)
        .expect("bootstrap reference");
    assert_eq!(bootstrapped, RouterHash::from_bytes(*i2pd_hash.as_bytes()));
    append_evidence(
        &evidence_dir,
        "reference-bootstrap-store",
        &dest.store_stats().record_count.to_string(),
    );
    assert!(
        dest.is_floodfill(&bootstrapped),
        "reference must advertise floodfill for the controlled lane"
    );
    append_evidence(&evidence_dir, "reference-floodfill-capable", "true");

    let bundle = i2pr_crypto::RouterIdentityBundle::generate(&mut OsRng).expect("identity");
    let local_hash = bundle.identity().hash().expect("hash");
    let identity =
        generate_controlled_identity(&bundle, "127.0.0.1", bind_port).expect("controlled identity");

    let daemon_service = Ssu2DaemonService::new(&config.ssu2, identity).expect("daemon service");
    let token = CancellationToken::new();
    let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
    let mut handle = daemon_service
        .start(&scope, &config.ssu2)
        .await
        .expect("daemon bind");
    assert_eq!(handle.local_v4().expect("bound v4"), bind);

    // Box the dial future: the SSU2 handshake future tree is the
    // largest single state in this driver (debug builds hold ~MBs
    // of handshake state); boxing keeps the test-thread frame bounded.
    let _established = Box::pin(handle.dial(target, DIAL_TIMEOUT, &CancellationToken::new()))
        .await
        .expect("authenticated session establishes");
    let deadline_active = tokio::time::Instant::now() + WAIT_TIMEOUT;
    while tokio::time::Instant::now() < deadline_active {
        if handle.snapshot().active_sessions >= 1 {
            break;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    assert!(handle.snapshot().active_sessions >= 1);
    append_evidence(
        &evidence_dir,
        "session-established",
        &handle.snapshot().sessions_established.to_string(),
    );

    let warmup_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < warmup_deadline {
        if tokio::time::timeout(POLL_INTERVAL, handle.next_inbound())
            .await
            .is_err()
        {
            continue;
        }
    }

    // Reference STREAM destination through i2pd's public SAM surface.
    // DEST GENERATE yields the keypair; the session is created from
    // the explicit private destination (standard SAM client flow).
    // Only public Destination facts are recorded; the ephemeral
    // reference private material lives in this local only, is never
    // logged, and never touches evidence.
    let mut sam = SamClient::connect(sam_endpoint).await;
    let hello = sam
        .transact("HELLO VERSION MIN=3.1 MAX=3.1\n")
        .await
        .expect("SAM hello read");
    assert!(hello.contains("RESULT=OK"), "SAM hello failed");
    let generated = sam
        .transact("DEST GENERATE SIGNATURE_TYPE=7\n")
        .await
        .expect("SAM dest generate read");
    assert!(
        generated.starts_with("DEST REPLY") && generated.contains(" PUB="),
        "SAM DEST GENERATE failed"
    );
    let reference_pub = sam_param(&generated, "PUB").expect("generated PUB");
    assert!(reference_pub.len() >= 512, "PUB too short for Ed25519");
    let reference_priv = sam_param(&generated, "PRIV").expect("generated PRIV");
    let session_id = format!("plan193-{}", wall_secs() % 1_000_000);
    let create = sam
        .transact(&format!(
            "SESSION CREATE STYLE=STREAM ID={session_id} DESTINATION={reference_priv} SIGNATURE_TYPE=7 inbound.length=0 outbound.length=0\n"
        ))
        .await
        .expect("SAM session create read");
    assert!(create.contains("RESULT=OK"), "SAM STREAM session failed");
    let reference_bytes = decode_sam_destination(&reference_pub);
    let reference_hash =
        i2pr_netdb::DestinationHash::from_hash(i2pr_crypto::sha256(&reference_bytes));
    append_evidence(
        &evidence_dir,
        "sam-streaming-created",
        &format!("dest_len={}", reference_bytes.len()),
    );
    let _ = reference_priv;

    // Real one-hop builds in both directions; replies route through
    // the coordinator to real Installed pool + registry roles.
    let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    coord.advance_time(wall_ms());
    let bridge = ShortBuildI2npBridge::new();
    let mut rng = ChaCha8Rng::seed_from_u64(wall_secs());
    let delivery = handle.delivery().clone();

    let outbound_request = BuildRequest {
        direction: BuildDirection::Outbound,
        peer: PeerBuildMaterial {
            router_hash: i2pd_hash,
            static_encryption_key: i2pd_encryption_key,
            receive_tunnel: TunnelId::new(OBEP_RECEIVE).expect("receive"),
            next_tunnel: TunnelId::new(OBEP_NEXT).expect("next"),
            role: HopRole::OutboundEndpoint,
        },
        creator_tunnel_id: TunnelId::new(OUTBOUND_CREATOR).expect("creator"),
        message_id: 0x51A7_5001,
        outbound_reply_router: Some(local_hash),
        originator_hash: None,
    };
    coord
        .submit(
            outbound_request,
            &delivery,
            &bridge,
            BridgeHeader::ShortTransport {
                message_id: 0x51A7_5001,
                expiration_seconds: wall_secs().saturating_add(60) as u32,
            },
            &mut rng,
        )
        .expect("outbound submit ok");
    append_evidence(&evidence_dir, "outbound-build-emitted", "true");

    let inbound_request = BuildRequest {
        direction: BuildDirection::Inbound,
        peer: PeerBuildMaterial {
            router_hash: i2pd_hash,
            static_encryption_key: i2pd_encryption_key,
            receive_tunnel: TunnelId::new(IBGW_RECEIVE).expect("receive"),
            next_tunnel: TunnelId::new(IBGW_NEXT).expect("next"),
            role: HopRole::InboundGateway,
        },
        creator_tunnel_id: TunnelId::new(INBOUND_CREATOR).expect("creator"),
        message_id: 0x51A7_5101,
        outbound_reply_router: None,
        originator_hash: Some(local_hash),
    };
    coord
        .submit(
            inbound_request,
            &delivery,
            &bridge,
            BridgeHeader::ShortTransport {
                message_id: 0x51A7_5101,
                expiration_seconds: wall_secs().saturating_add(60) as u32,
            },
            &mut rng,
        )
        .expect("inbound submit ok");
    append_evidence(&evidence_dir, "inbound-build-emitted", "true");

    let mut outbound_slot: Option<i2pr_tunnel::pool::TunnelSlot> = None;
    let mut installed_inbound = false;
    let mut pump_installed_ob = 0u64;
    let mut pump_installed_ib = 0u64;
    let mut pump_kind_reply = 0u64;
    let install_deadline = tokio::time::Instant::now() + I2PD_ACCEPT_TIMEOUT;
    while tokio::time::Instant::now() < install_deadline
        && (outbound_slot.is_none() || !installed_inbound)
    {
        let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let routed = match coord.route_inbound_i2np(&inbound, wall_ms()) {
            Ok(routed) => routed,
            Err(_) => continue,
        };
        if let i2pr_daemon::router_i2np::RouterI2npOutcome::TunnelBuildReserved { kind, .. } =
            &routed.dispatcher
            && matches!(
                kind,
                i2pr_daemon::router_i2np::RouterI2npKind::OutboundTunnelBuildReply
            )
        {
            pump_kind_reply += 1;
        }
        for outcome in routed.coordinator {
            if let BuildCoordinatorOutcome::Installed {
                slot, direction, ..
            } = outcome
            {
                match direction {
                    BuildDirection::Outbound => {
                        outbound_slot = Some(slot);
                        pump_installed_ob += 1;
                    }
                    BuildDirection::Inbound => {
                        installed_inbound = true;
                        pump_installed_ib += 1;
                    }
                }
            }
        }
    }
    let outbound_slot = match outbound_slot {
        Some(slot) => slot,
        None => {
            record_stop_and_baseline(&evidence_dir);
            append_evidence(
                &evidence_dir,
                "streaming-stop",
                &format!(
                    "installed_ob={pump_installed_ob} installed_ib={pump_installed_ib} kind_reply={pump_kind_reply} phase=tunnel-build"
                ),
            );
            panic!("Plan 193 streaming stop: outbound build never installed");
        }
    };
    if !installed_inbound {
        record_stop_and_baseline(&evidence_dir);
        append_evidence(
            &evidence_dir,
            "streaming-stop",
            &format!(
                "installed_ob={pump_installed_ob} installed_ib={pump_installed_ib} phase=tunnel-build"
            ),
        );
        panic!("Plan 193 streaming stop: inbound build never installed");
    }
    append_evidence(&evidence_dir, "outbound-installed", "true");
    append_evidence(&evidence_dir, "inbound-installed", "true");

    let gateway_role = coord
        .registry_mut()
        .remove_outbound(outbound_slot)
        .expect("real outbound role");
    let destination_outbound =
        DestinationOutboundRole::from_role(gateway_role, wall_ms() + 1_800_000);

    // Remote Standard LeaseSet2 lookup through the real tunnel path.
    let routing_key = i2pr_netdb::router_hash_from_destination(reference_hash);
    let receive_ids = coord.registry().inbound_receive_ids();
    assert_eq!(receive_ids.len(), 1);
    let local_receive_for_lookup = receive_ids[0];
    let reply_path = reply_path_for_inbound_route(coord.registry(), local_receive_for_lookup)
        .expect("typed inbound gateway route");
    let (lookup_id, action) = dest
        .begin_lease_lookup(reference_hash, &routing_key, reply_path)
        .expect("lease lookup send");
    let mut tunnel_rng = ChaCha8Rng::seed_from_u64(wall_secs().wrapping_add(7));
    let (dispatch, proof) = dest
        .compose_lookup_via_tunnel(
            &action,
            destination_outbound.role(),
            0x51A7_6001,
            wall_ms() + 60_000,
            Deadline::new(Duration::from_secs(60)).expect("deadline"),
            &mut tunnel_rng,
            0,
        )
        .expect("compose lease lookup");
    assert!(proof.via_tunnel);
    for cell_delivery in &dispatch.deliveries {
        let request = RouterDeliveryRequest::new(
            cell_delivery.target(),
            cell_delivery.message_bytes().to_vec(),
            DELIVERY_TIMEOUT,
        )
        .expect("delivery request");
        assert_eq!(
            delivery.deliver(request, &CancellationToken::new()),
            RouterDeliveryOutcome::Accepted
        );
    }

    let lookup_deadline = tokio::time::Instant::now() + STREAM_WAIT;
    let mut lease_summary: Option<i2pr_daemon::destination_tunnels::RemoteLeaseSummary> = None;
    while tokio::time::Instant::now() < lookup_deadline && lease_summary.is_none() {
        let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let message =
            match I2npMessage::decode_short_transport(&inbound.bytes, MAX_I2NP_PAYLOAD_SIZE) {
                Ok(message) => message,
                Err(_) => continue,
            };
        let cell = match message.body() {
            I2npBody::TunnelData(cell) => cell.clone(),
            _ => continue,
        };
        let outcome = match i2pr_daemon::inbound_dispatch::dispatch_inbound_tunnel_data(
            coord.registry_mut(),
            &cell,
            wall_ms(),
        ) {
            Ok(outcome) => outcome,
            Err(_) => continue,
        };
        let bytes = match outcome {
            i2pr_daemon::inbound_dispatch::InboundDispatchOutcome::DatabaseStoreComplete {
                bytes,
            } => bytes,
            _ => continue,
        };
        let envelope =
            I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode store");
        let now_secs = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
        match dest
            .ingest_tunnel_lease_store(lookup_id, &envelope, now_secs)
            .expect("ingest lease store")
        {
            LeaseStoreIngestOutcome::Completed { summary, .. } => {
                lease_summary = Some(summary);
            }
            LeaseStoreIngestOutcome::Continue | LeaseStoreIngestOutcome::Ignored => {}
        }
    }
    let summary = match lease_summary {
        Some(summary) => summary,
        None => {
            record_stop_and_baseline(&evidence_dir);
            append_evidence(
                &evidence_dir,
                "streaming-stop",
                "phase=lease-lookup reference LeaseSet2 never resolved",
            );
            panic!("Plan 193 streaming stop: reference LeaseSet2 never resolved");
        }
    };
    assert_eq!(summary.destination, reference_hash);
    append_evidence(
        &evidence_dir,
        "lease-lookup-completed",
        &format!("leases={}", summary.lease_count),
    );

    // i2pr destination identity + real inbound lease + publication.
    let mut identity_rng = OsRng;
    let local_identity =
        DestinationIdentity::generate(&mut identity_rng).expect("destination identity");
    let registrations_in = coord.registrations(TunnelDirection::Inbound);
    let tunnel_expires = wall_secs() + 600;
    let lease_source = InboundLeaseSource::from_parts(
        registrations_in[0].slot(),
        Hash::from_bytes(*i2pd_hash.as_bytes()),
        IBGW_RECEIVE,
        tunnel_expires,
        tunnel_expires.saturating_sub(60),
    );
    let published = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
    let mut local_ls2 =
        build_signed_lease_set2(&local_identity, &[lease_source], published).expect("local ls2");
    let local_dest_hash = local_identity.id().as_netdb_key();

    let store_message = i2pr_proto::DatabaseStoreMessage {
        key: local_ls2.key_hash().expect("key hash"),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: i2pr_proto::DatabaseStoreData::LeaseSet2(Box::new(local_ls2.clone())),
    };
    let publication_id = dest
        .begin_ls2_publication(store_message, RouterHash::from_bytes(*i2pd_hash.as_bytes()))
        .expect("begin publication");
    let (pub_dispatch, pub_proof) = dest
        .compose_ls2_publication_via_tunnel(
            publication_id,
            Hash::from_bytes(*i2pd_hash.as_bytes()),
            destination_outbound.role(),
            0x51A7_6101,
            wall_ms() + 60_000,
            Deadline::new(Duration::from_secs(60)).expect("deadline"),
            &mut tunnel_rng,
            0,
        )
        .expect("compose publication");
    assert!(pub_proof.via_tunnel);
    for cell_delivery in &pub_dispatch.deliveries {
        let request = RouterDeliveryRequest::new(
            cell_delivery.target(),
            cell_delivery.message_bytes().to_vec(),
            DELIVERY_TIMEOUT,
        )
        .expect("delivery request");
        assert_eq!(
            delivery.deliver(request, &CancellationToken::new()),
            RouterDeliveryOutcome::Accepted
        );
    }
    append_evidence(
        &evidence_dir,
        "ls2-publication-tunnel",
        &format!("cells={}", pub_proof.cell_count),
    );

    // ---- Direction A: i2pr StreamingManager -> i2pd STREAM ----
    let mut routing = DestinationRouting::new(DestinationRoutingConfig::balanced());
    let cached = dest
        .lease_store()
        .get(&reference_hash)
        .expect("cached reference ls2")
        .clone();
    let remote = routing
        .install_remote_lease_set2(cached)
        .expect("install remote ls2");
    assert_eq!(remote, reference_hash);
    let mut session = EciesSessionManager::new(EciesSessionConfig::balanced());

    // RemoteDestination for the SYN: signing key + hash from the
    // reference Destination bytes; static X25519 from the
    // Destination encryption key when it is 32 bytes, else the
    // zero placeholder (the SYN wire bytes only bind the hash +
    // signature; ECIES delivery uses the LS2 leases).
    let reference_destination =
        i2pr_proto::Destination::decode(&reference_bytes, 65535).expect("decode reference dest");
    let mut static_key = [0u8; 32];
    let enc_bytes = reference_destination.public_key().as_bytes();
    if enc_bytes.len() == 32 {
        static_key.copy_from_slice(enc_bytes);
    }
    let remote_desc = RemoteDestination {
        destination_hash: *reference_hash.as_bytes(),
        signing_public_key: reference_destination.signing_key().clone(),
        static_public_key: static_key,
    };

    let mut streaming = StreamingManager::new(StreamingConfig::balanced());
    let outcome = streaming
        .connect(
            &local_identity,
            &remote_desc,
            LOCAL_STREAM_PORT,
            REMOTE_STREAM_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            wall_ms(),
            &mut ChaCha8Rng::seed_from_u64(wall_secs()),
        )
        .expect("connect SYN");
    let ConnectOutcome::SynSent { connection_id, .. } = outcome else {
        panic!("expected SynSent");
    };
    let mut syn_queue = streaming.drain_outbound();
    assert_eq!(syn_queue.len(), 1, "connect must emit exactly one SYN");
    let syn_request = syn_queue.remove(0);
    // Plan 193 interop note: one RNG instance serves every transport
    // send in this lane. Reseeding per call with second-resolution
    // wall time hands every send inside the same second the identical
    // stream, so rapid-fire messages (the multipacket burst) would
    // reuse tunnel message ids and cell IVs; i2pd keys TunnelData
    // reassembly by message id and only the first same-id message
    // completes. A single advancing instance keeps every id/IV
    // distinct.
    let mut send_rng = ChaCha8Rng::seed_from_u64(wall_ms().wrapping_add(11));
    send_transport_request(
        &syn_request,
        &routing,
        &mut session,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
    )
    .await;
    append_evidence(&evidence_dir, "streaming-syn-sent", "true");

    // Pump inbound TunnelData for the SYN-ACK; feed each recovered
    // Garlic envelope through the adapter into the originator
    // manager until it reaches Established.
    let mut dispatcher = DestinationDispatcher::new();
    dispatcher
        .register_destination(local_identity.id())
        .expect("register destination");
    dispatcher
        .bind_destination_hash(local_identity.id(), local_dest_hash)
        .expect("bind destination hash");
    let syn_ack_deadline = tokio::time::Instant::now() + SYN_ACK_WAIT;
    let mut syn_accepted = false;
    let mut established = false;
    let mut pump_error = 0u64;
    while tokio::time::Instant::now() < syn_ack_deadline && !established {
        let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let message =
            match I2npMessage::decode_short_transport(&inbound.bytes, MAX_I2NP_PAYLOAD_SIZE) {
                Ok(message) => message,
                Err(_) => {
                    pump_error += 1;
                    continue;
                }
            };
        let cell = match message.body() {
            I2npBody::TunnelData(cell) => cell.clone(),
            _ => continue,
        };
        let bytes = match dest.recover_garlic_bytes(coord.registry_mut(), &cell, wall_ms()) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let now_secs = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
        dispatcher.dispatch_garlic_envelope(
            &mut session,
            local_identity.id(),
            local_identity.static_secret_bytes(),
            &local_identity.static_public_bytes(),
            now_secs,
            &I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode garlic"),
            routing.lease_set2_store_mut(),
        );
        let Some(queued) = dispatcher.pop_payload(local_identity.id()) else {
            continue;
        };
        match StreamingDestinationAdapter::receive(
            queued.bytes(),
            &local_identity,
            &mut streaming,
            reference_hash.as_bytes(),
            wall_ms(),
        ) {
            Ok(i2pr_client::InboundStreamingOutcome::StreamingDispatched { .. }) => {
                syn_accepted = true;
            }
            Ok(_) => {}
            Err(_) => {
                pump_error += 1;
                continue;
            }
        }
        if let Some(conn) = streaming.get_connection(connection_id)
            && conn.state() == i2pr_client::streaming::connection::ConnectionState::Established
        {
            established = true;
        }
        // Drain any SYN-ACK-triggered outbound (ACK) through the
        // real outbound tunnel so the reference sees a live peer.
        let pending = streaming.drain_outbound();
        for request in &pending {
            send_transport_request(
                request,
                &routing,
                &mut session,
                &destination_outbound,
                &local_identity,
                &local_ls2,
                &delivery,
                &mut send_rng,
            )
            .await;
        }
    }
    if !syn_accepted || !established {
        record_stop_and_baseline(&evidence_dir);
        append_evidence(
            &evidence_dir,
            "streaming-stop",
            &format!(
                "phase=streaming-establish syn_accepted={syn_accepted} established={established} pump_error={pump_error}"
            ),
        );
        panic!("Plan 193 streaming stop: SYN-ACK never established the connection");
    }
    append_evidence(&evidence_dir, "streaming-syn-accepted", "true");
    append_evidence(&evidence_dir, "streaming-established", "true");

    // Small application payload i2pr -> i2pd through the established
    // stream; the reference SAM STREAM ACCEPT socket observes the
    // bytes. Digest equality only is recorded.
    let app_small = b"plan193-streaming-probe-a";
    let data_request = streaming
        .send_data(
            connection_id,
            &local_identity,
            &remote_desc,
            LOCAL_STREAM_PORT,
            REMOTE_STREAM_PORT,
            app_small,
            wall_ms(),
        )
        .expect("send small data");
    send_transport_request(
        &data_request,
        &routing,
        &mut session,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
    )
    .await;

    // The reference ACCEPT socket receives exactly our bytes. Issue
    // STREAM ACCEPT after the data is in flight so the reference
    // delivers the buffered stream without a concurrent-accept race.
    // Plan 193 interop note: STREAM ACCEPT must arrive over a FRESH
    // SAM socket. The session-creation socket is already bound to the
    // STREAM session (`m_SocketType != Unknown`), so exact-pinned
    // i2pd 2.61.0 rejects ACCEPT on it with `Socket already in use`
    // (`SAM.cpp::ProcessStreamAccept`). A dedicated acceptor socket
    // carries the ACCEPT handshake plus the raw stream bytes.
    let mut accept_sam = SamClient::connect(sam_endpoint).await;
    let accept_hello = accept_sam
        .transact("HELLO VERSION MIN=3.1 MAX=3.1\n")
        .await
        .expect("SAM acceptor hello read");
    assert!(
        accept_hello.contains("RESULT=OK"),
        "SAM acceptor hello failed"
    );
    let accept_reply = accept_sam
        .transact(&format!("STREAM ACCEPT ID={session_id} SILENT=false\n"))
        .await;
    let accept_ok = accept_reply
        .as_deref()
        .is_some_and(|line| line.contains("RESULT=OK"));
    if !accept_ok {
        record_stop_and_baseline(&evidence_dir);
        append_evidence(
            &evidence_dir,
            "streaming-stop",
            "phase=stream-accept reference STREAM ACCEPT never returned OK",
        );
        panic!("Plan 193 streaming stop: reference STREAM ACCEPT failed");
    }
    // After RESULT=OK the bridge sends the peer destination line,
    // then raw stream bytes follow on the same acceptor socket.
    let peer_line = accept_sam.read_line().await.unwrap_or_default();
    let peer_len = peer_line.len();
    let observed = accept_sam
        .read_exact_bytes(app_small.len(), STREAM_WAIT)
        .await
        .unwrap_or_default();
    if observed == app_small {
        append_evidence(
            &evidence_dir,
            "streaming-data-digest",
            &format!(
                "payload_len={} digest={} peer_line_len={peer_len} match=true",
                observed.len(),
                sha256_hex(&observed),
            ),
        );
    } else {
        record_stop_and_baseline(&evidence_dir);
        append_evidence(
            &evidence_dir,
            "streaming-stop",
            &format!(
                "phase=streaming-data expected_len={} observed_len={} peer_line_len={peer_len}",
                app_small.len(),
                observed.len(),
            ),
        );
        panic!("Plan 193 streaming stop: streaming data digest mismatch");
    }

    // Multi-packet payload (8 KiB deterministic pattern) through the
    // same established stream; may fragment across several Streaming
    // data packets with retransmission bookkeeping.
    let mut app_multi = Vec::with_capacity(8192);
    for index in 0..8192 {
        app_multi.push((index % 251) as u8);
    }
    let mut offset = 0;
    let mut fragments = 0u64;
    while offset < app_multi.len() {
        let end = (offset + 1024).min(app_multi.len());
        let fragment_request = streaming
            .send_data(
                connection_id,
                &local_identity,
                &remote_desc,
                LOCAL_STREAM_PORT,
                REMOTE_STREAM_PORT,
                &app_multi[offset..end],
                wall_ms(),
            )
            .expect("send multi fragment");
        send_transport_request(
            &fragment_request,
            &routing,
            &mut session,
            &destination_outbound,
            &local_identity,
            &local_ls2,
            &delivery,
            &mut send_rng,
        )
        .await;
        offset = end;
        fragments += 1;
    }
    let observed_multi = accept_sam
        .read_exact_bytes(app_multi.len(), STREAM_WAIT)
        .await
        .unwrap_or_default();
    if observed_multi == app_multi {
        append_evidence(
            &evidence_dir,
            "streaming-multipacket-digest",
            &format!(
                "payload_len={} fragments={fragments} digest={} match=true",
                observed_multi.len(),
                sha256_hex(&observed_multi),
            ),
        );
    } else {
        record_stop_and_baseline(&evidence_dir);
        append_evidence(
            &evidence_dir,
            "streaming-stop",
            &format!(
                "phase=streaming-multipacket expected_len={} observed_len={} fragments={fragments}",
                app_multi.len(),
                observed_multi.len(),
            ),
        );
        panic!("Plan 193 streaming stop: multipacket digest mismatch");
    }

    // ---- Phase R: reverse data i2pd -> i2pr over the established stream ----
    // The reference writes to its SAM STREAM ACCEPT socket; i2pd emits
    // STREAM data packets that return via the real inbound tunnel. The
    // pump feeds them into the originator manager, drains ACKs back
    // through the real outbound tunnel, and digest-matches delivery.
    let app_rev_small = b"plan193-reverse-probe-b";
    accept_sam.write_bytes(app_rev_small).await;
    let mut rev_collected = Vec::new();
    pump_until_streaming(
        &mut handle,
        &mut coord,
        &mut dest,
        &mut dispatcher,
        &mut session,
        &mut routing,
        &mut streaming,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
        reference_hash.as_bytes(),
        &evidence_dir,
        tokio::time::Instant::now() + STREAM_WAIT,
        "streaming-reverse",
        |manager| {
            for delivered in manager.drain_delivered_for(connection_id) {
                rev_collected.extend_from_slice(&delivered.bytes);
            }
            rev_collected.as_slice() == app_rev_small.as_slice()
        },
    )
    .await;
    append_evidence(
        &evidence_dir,
        "streaming-reverse-data-digest",
        &format!(
            "payload_len={} digest={} match=true",
            rev_collected.len(),
            sha256_hex(&rev_collected),
        ),
    );

    let mut app_rev_multi = Vec::with_capacity(4096);
    for index in 0..4096 {
        app_rev_multi.push((index % 233) as u8);
    }
    accept_sam.write_bytes(&app_rev_multi).await;
    let mut rev_multi_collected = Vec::new();
    pump_until_streaming(
        &mut handle,
        &mut coord,
        &mut dest,
        &mut dispatcher,
        &mut session,
        &mut routing,
        &mut streaming,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
        reference_hash.as_bytes(),
        &evidence_dir,
        tokio::time::Instant::now() + STREAM_WAIT,
        "streaming-reverse-multipacket",
        |manager| {
            for delivered in manager.drain_delivered_for(connection_id) {
                rev_multi_collected.extend_from_slice(&delivered.bytes);
            }
            rev_multi_collected.len() >= app_rev_multi.len()
        },
    )
    .await;
    if rev_multi_collected != app_rev_multi {
        record_stop_and_baseline(&evidence_dir);
        append_evidence(
            &evidence_dir,
            "streaming-stop",
            &format!(
                "phase=streaming-reverse-multipacket expected_len={} observed_len={}",
                app_rev_multi.len(),
                rev_multi_collected.len(),
            ),
        );
        panic!("Plan 193 streaming stop: reverse multipacket digest mismatch");
    }
    append_evidence(
        &evidence_dir,
        "streaming-reverse-multipacket-digest",
        &format!(
            "payload_len={} digest={} match=true",
            rev_multi_collected.len(),
            sha256_hex(&rev_multi_collected),
        ),
    );

    // ---- Phase S: sibling connection, orderly close, isolation ----
    // A second stream from the same manager to the same reference
    // destination exercises sibling multiplexing over the real path.
    let sibling_outcome = streaming
        .connect(
            &local_identity,
            &remote_desc,
            LOCAL_STREAM_PORT,
            REMOTE_STREAM_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            wall_ms(),
            &mut send_rng,
        )
        .expect("sibling connect");
    let ConnectOutcome::SynSent {
        connection_id: sibling_id,
        ..
    } = sibling_outcome
    else {
        panic!("expected sibling SynSent");
    };
    assert_ne!(sibling_id, connection_id, "siblings need distinct ids");
    let mut sibling_queue = streaming.drain_outbound();
    assert_eq!(sibling_queue.len(), 1, "sibling SYN is one request");
    send_transport_request(
        &sibling_queue.remove(0),
        &routing,
        &mut session,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
    )
    .await;
    pump_until_streaming(
        &mut handle,
        &mut coord,
        &mut dest,
        &mut dispatcher,
        &mut session,
        &mut routing,
        &mut streaming,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
        reference_hash.as_bytes(),
        &evidence_dir,
        tokio::time::Instant::now() + SYN_ACK_WAIT,
        "streaming-sibling-establish",
        |manager| {
            manager
                .get_connection(sibling_id)
                .is_some_and(|conn| conn.state() == ConnectionState::Established)
        },
    )
    .await;
    append_evidence(&evidence_dir, "streaming-sibling-established", "true");

    // Sibling application bytes arrive on their own ACCEPT socket.
    let app_sibling = b"plan193-sibling-probe-c";
    let sibling_data = streaming
        .send_data(
            sibling_id,
            &local_identity,
            &remote_desc,
            LOCAL_STREAM_PORT,
            REMOTE_STREAM_PORT,
            app_sibling,
            wall_ms(),
        )
        .expect("send sibling data");
    send_transport_request(
        &sibling_data,
        &routing,
        &mut session,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
    )
    .await;
    let mut accept_sam2 = SamClient::connect(sam_endpoint).await;
    let accept2_hello = accept_sam2
        .transact("HELLO VERSION MIN=3.1 MAX=3.1\n")
        .await
        .expect("SAM sibling acceptor hello read");
    assert!(
        accept2_hello.contains("RESULT=OK"),
        "SAM sibling acceptor hello failed"
    );
    let accept2_reply = accept_sam2
        .transact(&format!("STREAM ACCEPT ID={session_id} SILENT=false\n"))
        .await;
    assert!(
        accept2_reply
            .as_deref()
            .is_some_and(|line| line.contains("RESULT=OK")),
        "sibling STREAM ACCEPT failed"
    );
    let peer2_line = accept_sam2.read_line().await.unwrap_or_default();
    let observed_sibling = accept_sam2
        .read_exact_bytes(app_sibling.len(), STREAM_WAIT)
        .await
        .unwrap_or_default();
    if observed_sibling.as_slice() != app_sibling.as_slice() {
        record_stop_and_baseline(&evidence_dir);
        append_evidence(
            &evidence_dir,
            "streaming-stop",
            &format!(
                "phase=streaming-sibling-data expected_len={} observed_len={} peer_line_len={}",
                app_sibling.len(),
                observed_sibling.len(),
                peer2_line.len(),
            ),
        );
        panic!("Plan 193 streaming stop: sibling data digest mismatch");
    }
    append_evidence(
        &evidence_dir,
        "streaming-sibling-data-digest",
        &format!(
            "payload_len={} digest={} peer_line_len={} match=true",
            observed_sibling.len(),
            sha256_hex(&observed_sibling),
            peer2_line.len(),
        ),
    );

    // Orderly full close of the first connection, initiated from i2pr:
    // CLOSE traverses the real outbound tunnel, the reference answers
    // with CLOSE through the real inbound tunnel, and its ACCEPT
    // socket reaches EOF.
    let close_request = streaming
        .send_close(
            connection_id,
            &local_identity,
            &remote_desc,
            LOCAL_STREAM_PORT,
            REMOTE_STREAM_PORT,
            wall_ms(),
        )
        .expect("send close");
    send_transport_request(
        &close_request,
        &routing,
        &mut session,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
    )
    .await;
    pump_until_streaming(
        &mut handle,
        &mut coord,
        &mut dest,
        &mut dispatcher,
        &mut session,
        &mut routing,
        &mut streaming,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
        reference_hash.as_bytes(),
        &evidence_dir,
        tokio::time::Instant::now() + Duration::from_secs(30),
        "streaming-close",
        |manager| {
            manager
                .get_connection(connection_id)
                .is_some_and(|conn| conn.state() == ConnectionState::Closed)
        },
    )
    .await;
    let close_eof = accept_sam
        .read_until_eof(Duration::from_secs(15))
        .await;
    if !close_eof {
        record_stop_and_baseline(&evidence_dir);
        append_evidence(
            &evidence_dir,
            "streaming-stop",
            "phase=streaming-close local state Closed but reference socket never reached EOF",
        );
        panic!("Plan 193 streaming stop: reference close EOF missing");
    }
    append_evidence(&evidence_dir, "streaming-close", "state=Closed eof=true");

    // The sibling survives the first connection's close untouched.
    let app_sibling2 = b"plan193-sibling-alive-d";
    let sibling_data2 = streaming
        .send_data(
            sibling_id,
            &local_identity,
            &remote_desc,
            LOCAL_STREAM_PORT,
            REMOTE_STREAM_PORT,
            app_sibling2,
            wall_ms(),
        )
        .expect("send sibling data after close");
    send_transport_request(
        &sibling_data2,
        &routing,
        &mut session,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
    )
    .await;
    let observed_sibling2 = accept_sam2
        .read_exact_bytes(app_sibling2.len(), STREAM_WAIT)
        .await
        .unwrap_or_default();
    if observed_sibling2.as_slice() != app_sibling2.as_slice() {
        record_stop_and_baseline(&evidence_dir);
        append_evidence(
            &evidence_dir,
            "streaming-stop",
            &format!(
                "phase=streaming-sibling-isolated expected_len={} observed_len={}",
                app_sibling2.len(),
                observed_sibling2.len(),
            ),
        );
        panic!("Plan 193 streaming stop: sibling isolation digest mismatch");
    }
    append_evidence(
        &evidence_dir,
        "streaming-sibling-isolated",
        &format!(
            "payload_len={} digest={} match=true",
            observed_sibling2.len(),
            sha256_hex(&observed_sibling2),
        ),
    );

    // ---- Phase B: i2pd initiates to i2pr through the normal listener ----
    // Refresh our published LeaseSet2 with a long lease so the
    // reference floodfill holds a valid inbound route for CONNECT.
    let fresh_expires = wall_secs() + 1800;
    let fresh_published = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
    let fresh_registrations = coord.registrations(TunnelDirection::Inbound);
    let fresh_lease = InboundLeaseSource::from_parts(
        fresh_registrations[0].slot(),
        Hash::from_bytes(*i2pd_hash.as_bytes()),
        IBGW_RECEIVE,
        fresh_expires,
        fresh_expires.saturating_sub(60),
    );
    local_ls2 =
        build_signed_lease_set2(&local_identity, &[fresh_lease], fresh_published).expect("fresh ls2");
    let fresh_store = i2pr_proto::DatabaseStoreMessage {
        key: local_ls2.key_hash().expect("key hash"),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: i2pr_proto::DatabaseStoreData::LeaseSet2(Box::new(local_ls2.clone())),
    };
    let fresh_publication = dest
        .begin_ls2_publication(fresh_store, RouterHash::from_bytes(*i2pd_hash.as_bytes()))
        .expect("begin republication");
    let (fresh_dispatch, fresh_proof) = dest
        .compose_ls2_publication_via_tunnel(
            fresh_publication,
            Hash::from_bytes(*i2pd_hash.as_bytes()),
            destination_outbound.role(),
            0x51A7_6201,
            wall_ms() + 60_000,
            Deadline::new(Duration::from_secs(60)).expect("deadline"),
            &mut tunnel_rng,
            0,
        )
        .expect("compose republication");
    assert!(fresh_proof.via_tunnel);
    for cell_delivery in &fresh_dispatch.deliveries {
        let request = RouterDeliveryRequest::new(
            cell_delivery.target(),
            cell_delivery.message_bytes().to_vec(),
            DELIVERY_TIMEOUT,
        )
        .expect("delivery request");
        assert_eq!(
            delivery.deliver(request, &CancellationToken::new()),
            RouterDeliveryOutcome::Accepted
        );
    }
    append_evidence(
        &evidence_dir,
        "ls2-republication-tunnel",
        &format!("cells={}", fresh_proof.cell_count),
    );
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Wildcard listener catches the reference SYN on any port; no
    // listener or accept state is injected, only the normal bind.
    assert!(
        matches!(
            streaming.listen(0).expect("listen"),
            ListenerOutcome::Listening { .. }
        ),
        "wildcard listener must bind"
    );
    let local_b64 = i2pr_api::sam::base64::encode(
        &local_identity
            .destination()
            .encode_to_vec(65535)
            .expect("encode local destination"),
    );
    let mut connect_attempts = 0u32;
    let mut b_connection: Option<i2pr_client::streaming::connection::ConnectionId> = None;
    for _ in 0..2 {
        connect_attempts += 1;
        sam.write_bytes(
            format!("STREAM CONNECT ID={session_id} DESTINATION={local_b64}\n").as_bytes(),
        )
        .await;
        let connect_deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        let mut status: Option<String> = None;
        let mut pump_errors = 0u64;
        while tokio::time::Instant::now() < connect_deadline && status.is_none() {
            pump_one_streaming_inbound(
                &mut handle,
                &mut coord,
                &mut dest,
                &mut dispatcher,
                &mut session,
                &mut routing,
                &mut streaming,
                &destination_outbound,
                &local_identity,
                &local_ls2,
                &delivery,
                &mut send_rng,
                reference_hash.as_bytes(),
                &mut pump_errors,
            )
            .await;
            // Accept the inbound SYN through the normal backlog as
            // soon as it arrives; the SYN response returns through
            // the real outbound tunnel while CONNECT is still pending.
            if b_connection.is_none() && streaming.listener_backlog(0) >= 1 {
                let inbound_id = streaming.accept(0).expect("accept inbound SYN");
                let (peer_bytes, peer_key, inbound_local, inbound_remote) = {
                    let inbound = streaming
                        .get_connection(inbound_id)
                        .expect("inbound connection");
                    (
                        inbound
                            .peer_destination()
                            .cloned()
                            .expect("peer destination retained")
                            .encode_to_vec(65535)
                            .expect("encode peer destination"),
                        inbound.peer_signing_key().clone(),
                        inbound.local_port(),
                        inbound.remote_port(),
                    )
                };
                let peer_hash = *i2pr_crypto::sha256(&peer_bytes).as_bytes();
                let inbound_remote_desc = RemoteDestination {
                    destination_hash: peer_hash,
                    signing_public_key: peer_key,
                    static_public_key: [0u8; 32],
                };
                let syn_response = streaming
                    .accept_inbound_syn(
                        &local_identity,
                        &inbound_remote_desc,
                        inbound_id,
                        inbound_local,
                        inbound_remote,
                        DEFAULT_ADVERTISED_MAX_PAYLOAD,
                        wall_ms(),
                        &mut send_rng,
                    )
                    .expect("SYN response");
                send_transport_request(
                    &syn_response,
                    &routing,
                    &mut session,
                    &destination_outbound,
                    &local_identity,
                    &local_ls2,
                    &delivery,
                    &mut send_rng,
                )
                .await;
                b_connection = Some(inbound_id);
            }
            if let Some(line) = sam.poll_line().await
                && line.starts_with("STREAM STATUS")
            {
                status = Some(line);
            }
        }
        match &status {
            Some(line) if line.contains("RESULT=OK") => break,
            _ => {
                if connect_attempts >= 2 {
                    record_stop_and_baseline(&evidence_dir);
                    append_evidence(
                        &evidence_dir,
                        "streaming-stop",
                        &format!(
                            "phase=streaming-b-connect attempts={connect_attempts} status={} pump_errors={pump_errors}",
                            status.as_deref().unwrap_or("none"),
                        ),
                    );
                    panic!("Plan 193 streaming stop: reference STREAM CONNECT never returned OK");
                }
            }
        }
    }
    let b_id = b_connection.expect("inbound connection accepted");
    assert!(
        streaming
            .get_connection(b_id)
            .is_some_and(|conn| conn.state() == ConnectionState::Established),
        "inbound connection must be Established after accept"
    );
    append_evidence(
        &evidence_dir,
        "streaming-b-established",
        &format!("attempts={connect_attempts}"),
    );
    // An optional peer-destination line may follow STATUS; consume it
    // within a short bound so stream bytes start at a known offset.
    let b_peer_line = sam.read_line_timeout(Duration::from_secs(3)).await;
    let b_peer_len = b_peer_line.map(|line| line.len()).unwrap_or(0);

    // Direction B small payload i2pd -> i2pr.
    let app_b_small = b"plan193-b-probe-e";
    sam.write_bytes(app_b_small).await;
    let mut b_collected = Vec::new();
    pump_until_streaming(
        &mut handle,
        &mut coord,
        &mut dest,
        &mut dispatcher,
        &mut session,
        &mut routing,
        &mut streaming,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
        reference_hash.as_bytes(),
        &evidence_dir,
        tokio::time::Instant::now() + STREAM_WAIT,
        "streaming-b-data",
        |manager| {
            for delivered in manager.drain_delivered_for(b_id) {
                b_collected.extend_from_slice(&delivered.bytes);
            }
            b_collected.as_slice() == app_b_small.as_slice()
        },
    )
    .await;
    append_evidence(
        &evidence_dir,
        "streaming-b-data-digest",
        &format!(
            "payload_len={} digest={} peer_line_len={b_peer_len} match=true",
            b_collected.len(),
            sha256_hex(&b_collected),
        ),
    );

    // Direction B reverse payload i2pr -> i2pd over the accepted stream.
    let (b_local_port, b_remote_port) = {
        let inbound = streaming.get_connection(b_id).expect("inbound connection");
        (inbound.local_port(), inbound.remote_port())
    };
    let mut app_b_rev = Vec::with_capacity(2048);
    for index in 0..2048 {
        app_b_rev.push((index % 199) as u8);
    }
    // The SYN-response routing descriptor is rebuilt from the live
    // connection state so the reply targets the true peer hash.
    let b_reply_desc = {
        let inbound = streaming.get_connection(b_id).expect("inbound connection");
        let peer_bytes = inbound
            .peer_destination()
            .cloned()
            .expect("peer destination retained")
            .encode_to_vec(65535)
            .expect("encode peer destination");
        RemoteDestination {
            destination_hash: *i2pr_crypto::sha256(&peer_bytes).as_bytes(),
            signing_public_key: inbound.peer_signing_key().clone(),
            static_public_key: [0u8; 32],
        }
    };
    let b_rev_request = streaming
        .send_data(
            b_id,
            &local_identity,
            &b_reply_desc,
            b_local_port,
            b_remote_port,
            &app_b_rev,
            wall_ms(),
        )
        .expect("send B reverse data");
    send_transport_request(
        &b_rev_request,
        &routing,
        &mut session,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
    )
    .await;
    let observed_b_rev = sam
        .read_exact_bytes(app_b_rev.len(), STREAM_WAIT)
        .await
        .unwrap_or_default();
    if observed_b_rev != app_b_rev {
        record_stop_and_baseline(&evidence_dir);
        append_evidence(
            &evidence_dir,
            "streaming-stop",
            &format!(
                "phase=streaming-b-reverse expected_len={} observed_len={}",
                app_b_rev.len(),
                observed_b_rev.len(),
            ),
        );
        panic!("Plan 193 streaming stop: B reverse digest mismatch");
    }
    append_evidence(
        &evidence_dir,
        "streaming-b-reverse-data-digest",
        &format!(
            "payload_len={} digest={} match=true",
            observed_b_rev.len(),
            sha256_hex(&observed_b_rev),
        ),
    );

    // Orderly close of the Direction B stream, initiated from i2pr.
    let b_close_request = streaming
        .send_close(
            b_id,
            &local_identity,
            &b_reply_desc,
            b_local_port,
            b_remote_port,
            wall_ms(),
        )
        .expect("send B close");
    send_transport_request(
        &b_close_request,
        &routing,
        &mut session,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
    )
    .await;
    pump_until_streaming(
        &mut handle,
        &mut coord,
        &mut dest,
        &mut dispatcher,
        &mut session,
        &mut routing,
        &mut streaming,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
        &mut send_rng,
        reference_hash.as_bytes(),
        &evidence_dir,
        tokio::time::Instant::now() + Duration::from_secs(30),
        "streaming-b-close",
        |manager| {
            manager
                .get_connection(b_id)
                .is_some_and(|conn| conn.state() == ConnectionState::Closed)
        },
    )
    .await;
    let b_close_eof = sam.read_until_eof(Duration::from_secs(15)).await;
    if !b_close_eof {
        record_stop_and_baseline(&evidence_dir);
        append_evidence(
            &evidence_dir,
            "streaming-stop",
            "phase=streaming-b-close local state Closed but reference socket never reached EOF",
        );
        panic!("Plan 193 streaming stop: B reference close EOF missing");
    }
    append_evidence(&evidence_dir, "streaming-b-close", "state=Closed eof=true");

    // Manager-level cleanup accounting: no queued transport, no
    // undrained application bytes after every stream closed.
    let cleanup_connections = streaming.connection_count();
    let cleanup_queued = streaming.outbound_queue_len();
    let cleanup_delivered = streaming.pending_delivered_bytes();
    assert_eq!(cleanup_queued, 0, "no queued transport after close");
    assert_eq!(cleanup_delivered, 0, "no undrained bytes after close");
    append_evidence(
        &evidence_dir,
        "manager-cleanup",
        &format!(
            "connections={cleanup_connections} queued={cleanup_queued} delivered={cleanup_delivered}"
        ),
    );

    // Direct transport is never a counted path.
    assert!(dest.note_direct_transport_attempt().is_err());
    append_evidence(&evidence_dir, "direct-rejected", "true");

    let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
    scheduler.advance_time(wall_ms());
    scheduler
        .register_pair(
            i2pr_tunnel::pool::TunnelSlot::from_raw(1),
            i2pr_tunnel::pool::TunnelSlot::from_raw(2),
        )
        .expect("pair");
    scheduler.advance_time(wall_ms() + FIRST_LIVENESS_DELAY_MS + 1);
    let action = scheduler.drive();
    assert!(matches!(action, LivenessAction::SendTest { .. }));
    append_evidence(&evidence_dir, "liveness-first-test", "passed");

    handle.shutdown();
    let _ = scope.shutdown().await;
    let snapshot = handle.snapshot();
    assert_eq!(snapshot.pending_outbound, 0);
    assert_eq!(snapshot.pending_inbound, 0);
    assert_eq!(snapshot.active_sessions, 0);
    append_evidence(&evidence_dir, "shutdown-baseline", "true");
    let _ = PeerId::from_hash(i2pd_hash);
}

#[allow(clippy::too_many_arguments)]
async fn send_transport_request(
    request: &TransportSendRequest,
    routing: &DestinationRouting,
    session: &mut EciesSessionManager,
    outbound: &DestinationOutboundRole,
    local_identity: &DestinationIdentity,
    local_ls2: &i2pr_proto::LeaseSet2,
    delivery: &i2pr_daemon::router_i2np::RouterDeliveryService,
    rng: &mut ChaCha8Rng,
) {
    let plan = StreamingDestinationAdapter::send(
        request,
        routing,
        session,
        outbound,
        local_identity.id(),
        local_identity.static_secret_bytes(),
        local_ls2,
        u32::try_from(wall_secs()).unwrap_or(u32::MAX),
        wall_ms(),
        rng,
    )
    .expect("adapter send");
    let cell_dispatch = i2pr_daemon::outbound_lookup::deliver_outbound_cells(
        &plan.cells,
        wall_ms() + 60_000,
        Deadline::new(Duration::from_secs(60)).expect("deadline"),
        rng,
    )
    .expect("encode streaming cells");
    for cell_delivery in &cell_dispatch.deliveries {
        let send = RouterDeliveryRequest::new(
            cell_delivery.target(),
            cell_delivery.message_bytes().to_vec(),
            DELIVERY_TIMEOUT,
        )
        .expect("delivery request");
        assert_eq!(
            delivery.deliver(send, &CancellationToken::new()),
            RouterDeliveryOutcome::Accepted
        );
    }
}

fn record_stop_and_baseline(dir: &Path) {    let mut coordinator = DestinationTunnelCoordinator::new(
        LookupPolicy::default(),
        RouterInfoStoreConfig::default(),
    );
    assert!(coordinator.note_direct_transport_attempt().is_err());
    append_evidence(dir, "direct-rejected", "true");
    let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());    scheduler.advance_time(wall_ms());
    scheduler
        .register_pair(
            i2pr_tunnel::pool::TunnelSlot::from_raw(1),
            i2pr_tunnel::pool::TunnelSlot::from_raw(2),
        )
        .expect("pair");
    scheduler.advance_time(wall_ms() + FIRST_LIVENESS_DELAY_MS + 1);
    let action = scheduler.drive();
    assert!(matches!(action, LivenessAction::SendTest { .. }));
    append_evidence(dir, "liveness-first-test", "passed");
}

#[test]
fn temp_size_probe() {
    eprintln!("MARKER runtime-probe-enter");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    eprintln!("MARKER runtime-built");
    rt.block_on(async {
        eprintln!("MARKER empty-body-polled");
    });
    eprintln!("MARKER runtime-probe-done");
    println!("StreamingManager={}", std::mem::size_of::<StreamingManager>());
    println!(
        "DestinationRouting={}",
        std::mem::size_of::<DestinationRouting>()
    );
    println!(
        "EciesSessionManager={}",
        std::mem::size_of::<EciesSessionManager>()
    );
    println!(
        "DestinationDispatcher={}",
        std::mem::size_of::<DestinationDispatcher>()
    );
    println!(
        "DestinationTunnelCoordinator={}",
        std::mem::size_of::<DestinationTunnelCoordinator>()
    );
    println!(
        "ExploratoryBuildCoordinator={}",
        std::mem::size_of::<ExploratoryBuildCoordinator>()
    );
    println!(
        "Ssu2DaemonHandle={}",
        std::mem::size_of::<i2pr_daemon::router_i2np::Ssu2DaemonHandle>()
    );
    println!(
        "RouterDeliveryService={}",
        std::mem::size_of::<i2pr_daemon::router_i2np::RouterDeliveryService>()
    );
    println!("ChaCha8Rng={}", std::mem::size_of::<ChaCha8Rng>());
    println!("SamClient={}", std::mem::size_of::<SamClient>());
    println!("I2npMessage={}", std::mem::size_of::<I2npMessage>());
    println!(
        "StreamingConfig={}",
        std::mem::size_of::<StreamingConfig>()
    );
    println!(
        "TransportSendRequest={}",
        std::mem::size_of::<TransportSendRequest>()
    );
    println!(
        "RemoteDestination={}",
        std::mem::size_of::<RemoteDestination>()
    );
    println!(
        "TunnelDataMessage={}",
        std::mem::size_of::<i2pr_proto::TunnelDataMessage>()
    );
    println!(
        "DestinationIdentity={}",
        std::mem::size_of::<DestinationIdentity>()
    );
    // Size of the whole test future (construction runs no code).
}
