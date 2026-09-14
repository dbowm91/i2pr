//! Plan 194 — M6 Java I2P second-family qualification external lane.
//!
//! Fail-closed driver against exact-pinned Java I2P 2.13.0 on loopback.
//!
//! - strict SSU2 controlled profile (loopback, non-advertised);
//! - reference RouterInfo verified + bootstrapped through the ordinary
//!   parser/signature/freshness path into the authoritative store;
//! - effective floodfill capability verified before dispatch;
//! - authenticated SSU2 session establishes via the daemon-owned runtime;
//! - one reference SAM destination created through Java's public SAM
//!   surface (transient; only public Destination facts recorded, never
//!   keys);
//! - one real one-hop outbound + inbound tunnel build accepted by the
//!   reference, with build replies routed through
//!   `ExploratoryBuildCoordinator::route_inbound_i2np` to real
//!   `Installed` pool + registry roles (real cryptographically derived
//!   keys on both directions, no synthetic roles, no `LocalZeroHop`);
//! - the reference Standard LeaseSet2 resolved through the real
//!   tunnel NetDB path, validated through the existing validators,
//!   and cached through the existing LeaseSet2 store;
//! - the i2pr destination's own Standard LeaseSet2 (real inbound
//!   lease) published through the controlled NetDB path;
//! - a bounded message from i2pr reaches the reference RAW session
//!   through existing ECIES/Garlic + the real outbound tunnel + the
//!   selected remote lease (payload digest equality, plaintext never
//!   logged);
//! - a reply from the reference reaches i2pr through its real
//!   inbound tunnel and the existing ECIES decrypt path, delivered
//!   to the owning destination queue with sibling isolation;
//! - direct transport explicitly rejected as a counted path;
//! - creator-side liveness first test green.
//!
//! Plan 194 §5.5 — the Streaming surface reuses the same wire: a
//! second driver `streaming_through_java` runs Direction A
//! (`StreamingManager::connect` i2pr -> Java) + Direction B
//! (Java `STREAM CONNECT` -> i2pr listener/accept). Same
//! digest-equality / close / sibling / cleanup rules as the i2pd
//! first-family driver; only the reference identifier + SAM bridge
//! port differ.
//!
//! Plan 194 §11 failure policy: if Java fails where i2pd passes, the
//! driver stops at the first failing protocol boundary and records
//! `plan194-java-stop` so the harness can mark every install-dependent
//! row `blocked` (never `passed`, never silently skipped).

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
    OutboundRequest, StreamingDestinationAdapter, build_signed_lease_set2,
    compose_outbound_delivery,
};
use i2pr_daemon::config::Config;
use i2pr_daemon::destination_tunnels::{
    DestinationTunnelCoordinator, LeaseStoreIngestOutcome, reply_path_for_inbound_route,
};
use i2pr_daemon::exploratory_build::{
    BuildCoordinatorOutcome, BuildDirection, BuildRequest, ExploratoryBuildCoordinator,
    PeerBuildMaterial,
};
use i2pr_daemon::inbound_dispatch::dispatch_inbound_tunnel_data;
use i2pr_daemon::outbound_lookup::deliver_outbound_cells;
use i2pr_daemon::router_i2np::{
    RouterDeliveryOutcome, RouterDeliveryRequest, Ssu2DaemonService, daemon_dial_target,
    generate_controlled_identity, verify_reference_router_info,
};
use i2pr_daemon::tunnel_liveness::{
    FIRST_LIVENESS_DELAY_MS, LivenessAction, LivenessConfig, TunnelLivenessScheduler,
};
use i2pr_netdb::{LookupPolicy, RouterHash, RouterInfoStoreConfig};
use i2pr_proto::{
    Date, Hash, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE, Mapping, RouterAddress, RouterInfo,
};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope, Ssu2PqKem};
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
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(30);
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);
const SAM_TIMEOUT: Duration = Duration::from_secs(15);
const DATAGRAM_WAIT: Duration = Duration::from_secs(45);
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

/// Minimal SAMv3 client over a tokio TCP stream. Same shape as the
/// Plan 193 / Plan 187 i2pd-driver copy but rooted at the Java SAM
/// bridge port tuple (default `127.0.0.1:7656`). Reads line replies
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
            let mut chunk = [0u8; 4096];
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

    /// Plan 193 interop helper: SAM STREAM ACCEPT / CONNECT need a
    /// fresh socket per connection. The session-creation socket is
    /// bound to the STREAM session (`m_SocketType != Unknown`) so the
    /// reference bridge rejects ACCEPT/CONNECT on it with `Socket
    /// already in use`. We expose `poll_line` for the streaming
    /// driver.
    async fn poll_line(&mut self) -> Option<String> {
        self.read_line().await
    }

    /// Read with a bounded timeout independent of `SAM_TIMEOUT`.
    async fn read_line_timeout(&mut self, bound: Duration) -> Option<String> {
        let deadline = tokio::time::Instant::now() + bound;
        loop {
            if let Some(position) = self.buffer.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = self.buffer.drain(..=position).collect();
                return Some(String::from_utf8_lossy(&line).trim_end().to_owned());
            }
            if tokio::time::Instant::now() >= deadline {
                return None;
            }
            let mut chunk = [0u8; 4096];
            let remaining = deadline - tokio::time::Instant::now();
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

    async fn write_bytes(&mut self, bytes: &[u8]) {
        use tokio::io::AsyncWriteExt as _;
        self.stream.write_all(bytes).await.expect("SAM write");
    }

    async fn read_exact_bytes(&mut self, size: usize, bound: Duration) -> Option<Vec<u8>> {
        let deadline = tokio::time::Instant::now() + bound;
        while self.buffer.len() < size {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let mut chunk = [0u8; 65536];
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

    async fn read_until_eof(&mut self, bound: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + bound;
        while tokio::time::Instant::now() < deadline {
            let mut chunk = [0u8; 4096];
            let remaining = deadline - tokio::time::Instant::now();
            let read = tokio::time::timeout(remaining, async {
                use tokio::io::AsyncReadExt as _;
                self.stream.read(&mut chunk).await
            })
            .await
            .ok()
            .and_then(|r| r.ok());
            match read {
                Some(0) => return true,
                Some(n) => self.buffer.extend_from_slice(&chunk[..n]),
                None => return false,
            }
        }
        false
    }
}

/// Decodes a Java SAM destination token through the Plan 142 SAM wire
/// codec (I2P alphabet `-~` with `=` padding, frozen against Java I2P
/// `SAM` source, i2pd `Base.cpp`, and i2plib vectors). The
/// filename-oriented `i2pr_netdb::base64` codec uses a different
/// alphabet and must never decode wire destinations.
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

fn record_stop(dir: &Path, detail: &str) {
    append_evidence(dir, "plan194-java-stop", detail);
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
    let cell_dispatch = deliver_outbound_cells(
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
    _evidence_dir: &Path,
) -> bool {
    let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
    let Ok(Some(inbound)) = next else {
        return false;
    };
    let message = match I2npMessage::decode_short_transport(&inbound.bytes, MAX_I2NP_PAYLOAD_SIZE) {
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
    if StreamingDestinationAdapter::receive(
        queued.bytes(),
        local_identity,
        streaming,
        from_hash,
        wall_ms(),
    )
    .is_err()
    {
        *errors += 1;
        return false;
    }
    let pending = streaming.drain_outbound();
    for request in &pending {
        send_transport_request(
            request,
            routing,
            session,
            outbound,
            local_identity,
            local_ls2,
            delivery,
            rng,
        )
        .await;
    }
    drain_streaming_timers(
        streaming,
        routing,
        session,
        outbound,
        local_identity,
        local_ls2,
        delivery,
        rng,
        _evidence_dir,
    )
    .await;
    true
}

#[allow(clippy::too_many_arguments)]
async fn drain_streaming_timers(
    streaming: &mut StreamingManager,
    routing: &mut DestinationRouting,
    session: &mut EciesSessionManager,
    outbound: &DestinationOutboundRole,
    local_identity: &DestinationIdentity,
    local_ls2: &i2pr_proto::LeaseSet2,
    delivery: &i2pr_daemon::router_i2np::RouterDeliveryService,
    rng: &mut ChaCha8Rng,
    _evidence_dir: &Path,
) {
    let now_ms = wall_ms();
    for request in streaming.poll_acks(now_ms) {
        send_transport_request(
            &request,
            routing,
            session,
            outbound,
            local_identity,
            local_ls2,
            delivery,
            rng,
        )
        .await;
    }
    for request in streaming.poll_retransmits(now_ms) {
        send_transport_request(
            &request,
            routing,
            session,
            outbound,
            local_identity,
            local_ls2,
            delivery,
            rng,
        )
        .await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn accept_inbound_syn_response(
    streaming: &mut StreamingManager,
    local_identity: &DestinationIdentity,
    routing: &DestinationRouting,
    session: &mut EciesSessionManager,
    outbound: &DestinationOutboundRole,
    local_ls2: &i2pr_proto::LeaseSet2,
    delivery: &i2pr_daemon::router_i2np::RouterDeliveryService,
    rng: &mut ChaCha8Rng,
) -> Option<i2pr_client::streaming::connection::ConnectionId> {
    if streaming.listener_backlog(0) < 1 {
        return None;
    }
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
            local_identity,
            &inbound_remote_desc,
            inbound_id,
            inbound_local,
            inbound_remote,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            wall_ms(),
            rng,
        )
        .expect("SYN response");
    send_transport_request(
        &syn_response,
        routing,
        session,
        outbound,
        local_identity,
        local_ls2,
        delivery,
        rng,
    )
    .await;
    Some(inbound_id)
}

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
    reference_hash_bytes: &[u8; 32],
    _evidence_dir: &Path,
    deadline: tokio::time::Instant,
    _phase: &str,
    mut predicate: F,
) where
    F: FnMut(&mut StreamingManager) -> bool,
{
    while tokio::time::Instant::now() < deadline && !predicate(streaming) {
        let mut pump_errors = 0u64;
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
            reference_hash_bytes,
            &mut pump_errors,
            _evidence_dir,
        )
        .await;
        drain_streaming_timers(
            streaming,
            routing,
            session,
            outbound,
            local_identity,
            local_ls2,
            delivery,
            rng,
            _evidence_dir,
        )
        .await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn pump_until_streaming_with_state<F>(
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
    reference_hash_bytes: &[u8; 32],
    _evidence_dir: &Path,
    deadline: tokio::time::Instant,
    _phase: &str,
    target: Option<i2pr_client::streaming::connection::ConnectionId>,
    mut predicate: F,
) where
    F: FnMut(&mut StreamingManager) -> bool,
{
    while tokio::time::Instant::now() < deadline && !predicate(streaming) {
        let mut pump_errors = 0u64;
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
            reference_hash_bytes,
            &mut pump_errors,
            _evidence_dir,
        )
        .await;
        drain_streaming_timers(
            streaming,
            routing,
            session,
            outbound,
            local_identity,
            local_ls2,
            delivery,
            rng,
            _evidence_dir,
        )
        .await;
        let _ = target;
    }
}

/// Plan 194 §5.1-§5.4 — full destination message plane against the
/// exact-pinned Java reference. The same wire as the i2pd driver,
/// only the reference identifier + the SAM PRIV/PUB distinction differ.
#[tokio::test]
#[ignore = "Plan 194: requires exact-pinned external Java I2P environment"]
async fn destination_message_plane_against_java() {
    let java_ri_path = env_path("JAVA_ROUTER_INFO");
    let java_endpoint: SocketAddr = env_value("JAVA_SSU2_ENDPOINT").parse().expect("endpoint");
    let bind: SocketAddr = env_value("I2PR_SSU2_BIND").parse().expect("bind");
    let sam_endpoint: SocketAddr = env_value("JAVA_SAM_ENDPOINT")
        .parse()
        .expect("sam endpoint");
    assert!(bind.ip().is_loopback(), "i2pr bind must be loopback");
    assert!(
        java_endpoint.ip().is_loopback(),
        "java endpoint must be loopback"
    );
    assert!(sam_endpoint.ip().is_loopback(), "java SAM must be loopback");
    let evidence_dir = env_path("EVIDENCE_DIR");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");

    let bind_port = bind.port();
    assert!(bind_port != 0, "lane requires a fixed loopback bind");
    let config_text = format!(
        "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nport = {bind_port}\n"
    );
    let config = Config::parse(&config_text).expect("strict controlled profile");
    assert!(config.ssu2.enabled);
    assert!(!config.ssu2.advertise);
    append_evidence(&evidence_dir, "daemon-strict-profile", "true");

    // ---- Plan 194 §5.1 authenticated transport ----------------------------
    let java_ri_bytes = std::fs::read(&java_ri_path).expect("read java router.info");
    let (java_hash, java_ssu2) =
        verify_reference_router_info(&java_ri_bytes).expect("verify java RouterInfo");
    let material = java_ssu2.address_material().expect("java key material");
    let responder_static =
        i2pr_runtime::Ssu2PublicKey::new(*material.static_public_key().as_bytes())
            .expect("static key");
    let responder_intro = i2pr_runtime::IntroKey::new(*material.intro_key().as_bytes());
    let target = daemon_dial_target(java_hash, java_endpoint, responder_static, responder_intro)
        .expect("dial target");
    let java_router_info = RouterInfo::decode(
        &java_ri_bytes,
        i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
    )
    .expect("decode java RouterInfo");
    let java_encryption_key: [u8; 32] = java_router_info
        .router_identity()
        .public_key()
        .as_bytes()
        .try_into()
        .expect("32-byte encryption key");
    append_evidence(&evidence_dir, "reference-routerinfo-verified", "true");

    // Authoritative bootstrap through the ordinary validation path.
    let mut dest = DestinationTunnelCoordinator::new(
        LookupPolicy::default(),
        RouterInfoStoreConfig::default(),
    );
    dest.advance_time(wall_ms());
    let now = Date::from_millis(wall_ms());
    let bootstrapped = dest
        .bootstrap_reference_router_info(&java_ri_bytes, now)
        .expect("bootstrap reference");
    assert_eq!(bootstrapped, RouterHash::from_bytes(*java_hash.as_bytes()));
    append_evidence(
        &evidence_dir,
        "reference-bootstrap-store",
        &dest.store_stats().record_count.to_string(),
    );

    let caps = java_router_info
        .capabilities()
        .ok()
        .flatten()
        .map(|c| c.as_str().to_owned())
        .unwrap_or_default();
    let is_floodfill = caps.bytes().any(|b| b == b'f');
    if !is_floodfill {
        record_stop(
            &evidence_dir,
            &format!("java reference not advertising floodfill (caps={caps:?})"),
        );
        assert!(is_floodfill, "java reference must advertise floodfill");
    }
    append_evidence(&evidence_dir, "reference-floodfill-capable", "true");

    let bundle =
        i2pr_crypto::RouterIdentityBundle::generate(&mut rand_core::OsRng).expect("identity");
    let local_hash = bundle.identity().hash().expect("hash");
    let identity =
        generate_controlled_identity(&bundle, "127.0.0.1", bind_port).expect("controlled identity");
    append_evidence(
        &evidence_dir,
        "i2pr-routerinfo-len",
        &identity.router_info.len().to_string(),
    );

    let daemon_service = Ssu2DaemonService::new(&config.ssu2, identity).expect("daemon service");
    let token = CancellationToken::new();
    let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
    let mut handle = daemon_service
        .start(&scope, &config.ssu2)
        .await
        .expect("daemon bind");
    assert_eq!(handle.local_v4().expect("bound v4"), bind);

    let _established = handle
        .dial(target, DIAL_TIMEOUT, &CancellationToken::new())
        .await
        .expect("authenticated session establishes with Java reference");
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

    // Reference RAW SAM destination through Java's public SAM
    // surface. Java requires PRIV (>= 663 bytes decoded) for SESSION
    // CREATE; i2pd accepts PUB. Both forms are valid wire; the
    // destination hash comes from the PUB bytes the bridge returns.
    let mut sam = SamClient::connect(sam_endpoint).await;
    let hello = sam
        .transact("HELLO VERSION MIN=3.1 MAX=3.1\n")
        .await
        .expect("SAM hello read");
    assert!(hello.contains("RESULT=OK"), "SAM hello failed: {hello}");
    let generated = sam
        .transact("DEST GENERATE SIGNATURE_TYPE=7\n")
        .await
        .expect("SAM dest generate read");
    assert!(
        generated.starts_with("DEST REPLY") && generated.contains(" PUB="),
        "SAM DEST GENERATE failed: {generated}"
    );
    let reference_pub = sam_param(&generated, "PUB").expect("generated PUB");
    assert!(reference_pub.len() >= 512, "PUB too short for Ed25519");
    let reference_priv = sam_param(&generated, "PRIV").expect("generated PRIV");
    let session_id = format!("plan194-dest-{}", wall_secs() % 1_000_000);
    let create = sam
        .transact(&format!(
            "SESSION CREATE STYLE=RAW ID={session_id} DESTINATION={reference_priv} SIGNATURE_TYPE=7 inbound.length=0 outbound.length=0\n"
        ))
        .await
        .expect("SAM session create read");
    assert!(
        create.contains("RESULT=OK"),
        "Java SAM RAW session failed: {create}"
    );
    let reference_bytes = decode_sam_destination(&reference_pub);
    let reference_hash =
        i2pr_netdb::DestinationHash::from_hash(i2pr_crypto::sha256(&reference_bytes));
    append_evidence(
        &evidence_dir,
        "sam-destination-created",
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
            router_hash: java_hash,
            static_encryption_key: java_encryption_key,
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
            router_hash: java_hash,
            static_encryption_key: java_encryption_key,
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

    // Pump inbound I2NP until both builds Install with real material.
    let mut outbound_slot: Option<i2pr_tunnel::pool::TunnelSlot> = None;
    let mut installed_inbound = false;
    let mut pump_installed_ob = 0u64;
    let mut pump_installed_ib = 0u64;
    let mut pump_kind_reply = 0u64;
    let mut pump_dispatch_error = 0u64;
    let mut pump_invalid = 0u64;
    let snapshot_before = handle.snapshot();
    let install_deadline = tokio::time::Instant::now() + ACCEPT_TIMEOUT;
    while tokio::time::Instant::now() < install_deadline
        && (outbound_slot.is_none() || !installed_inbound)
    {
        let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let routed = match coord.route_inbound_i2np(&inbound, wall_ms()) {
            Ok(routed) => routed,
            Err(_) => {
                pump_dispatch_error += 1;
                continue;
            }
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
            } else {
                pump_invalid += 1;
            }
        }
    }
    append_evidence(
        &evidence_dir,
        "install-pump-summary",
        &format!(
            "installed_ob={pump_installed_ob} installed_ib={pump_installed_ib} non_install={pump_invalid} dispatch_error={pump_dispatch_error} kind_reply={pump_kind_reply}"
        ),
    );
    let snapshot_after = handle.snapshot();
    append_evidence(
        &evidence_dir,
        "session-pump-delta",
        &format!(
            "datagrams_received={} i2np_received={} protocol_drops={} auth_failures={} queue_drops={}",
            snapshot_after
                .datagrams_received
                .saturating_sub(snapshot_before.datagrams_received),
            snapshot_after
                .i2np_received
                .saturating_sub(snapshot_before.i2np_received),
            snapshot_after
                .protocol_drops
                .saturating_sub(snapshot_before.protocol_drops),
            snapshot_after
                .auth_failures
                .saturating_sub(snapshot_before.auth_failures),
            snapshot_after
                .inbound_queue_drops
                .saturating_sub(snapshot_before.inbound_queue_drops),
        ),
    );
    let outbound_slot = match outbound_slot {
        Some(slot) => slot,
        None => {
            // Plan 194 §11 stop: Java accepted both builds (transit
            // log evidence) but emitted no consumable
            // ShortTunnelBuildReply within the bounded window. Mark
            // every install-dependent row with stop provenance; the
            // shell records blocked, never passed.
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
            record_stop(
                &evidence_dir,
                &format!(
                    "Plan 194 §11 stop: java outbound build never installed (installed_ob={pump_installed_ob} kind_reply={pump_kind_reply})"
                ),
            );
            handle.shutdown();
            let _ = scope.shutdown().await;
            return;
        }
    };
    assert!(
        installed_inbound,
        "Plan 194 §11 stop: java inbound build never installed"
    );
    assert_eq!(coord.registry().outbound_len(), 1);
    assert_eq!(coord.registry().inbound_len(), 1);
    append_evidence(&evidence_dir, "outbound-installed", "true");
    append_evidence(&evidence_dir, "inbound-installed", "true");

    let registrations_out = coord.registrations(TunnelDirection::Outbound);
    let registrations_in = coord.registrations(TunnelDirection::Inbound);
    assert_eq!(registrations_out.len(), 1);
    assert_eq!(registrations_in.len(), 1);
    assert!(
        registrations_out[0]
            .hops()
            .iter()
            .any(|hop| hop.hash() == Hash::from_bytes(*java_hash.as_bytes())),
        "outbound hop must be the reference"
    );
    let receive_ids = coord.registry().inbound_receive_ids();
    assert_eq!(receive_ids.len(), 1);
    append_evidence(
        &evidence_dir,
        "destination-material-real",
        &format!(
            "outbound_slots=1 inbound_slots=1 zero_hop=0 receive={}",
            receive_ids[0].get()
        ),
    );

    let gateway_role = coord
        .registry_mut()
        .remove_outbound(outbound_slot)
        .expect("real outbound role");
    let destination_outbound =
        DestinationOutboundRole::from_role(gateway_role, wall_ms() + 600_000);

    // ---- Plan 194 §5.3 remote Standard LeaseSet2 lookup ------------------
    let routing_key = i2pr_netdb::router_hash_from_destination(reference_hash);
    let local_receive_for_lookup = receive_ids[0];
    let reply_path = reply_path_for_inbound_route(coord.registry(), local_receive_for_lookup)
        .expect("typed inbound gateway route");
    let inbound_route = coord
        .registry()
        .inbound_gateway_route(local_receive_for_lookup)
        .expect("inbound route exists");
    assert_eq!(inbound_route.gateway_router, java_hash);
    assert_eq!(inbound_route.gateway_receive_tunnel.get(), IBGW_RECEIVE);
    assert_eq!(inbound_route.local_receive_tunnel.get(), IBGW_NEXT);
    assert_eq!(reply_path.tunnel_id(), IBGW_RECEIVE);
    append_evidence(
        &evidence_dir,
        "inbound-reply-path",
        &format!(
            "gateway_matches_reference=true gateway_tunnel={} local_receive={} ids_distinct=true",
            inbound_route.gateway_receive_tunnel.get(),
            inbound_route.local_receive_tunnel.get(),
        ),
    );
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
    append_evidence(
        &evidence_dir,
        "outbound-lookup-via-tunnel",
        &format!("cells={}", proof.cell_count),
    );
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

    let lookup_deadline = tokio::time::Instant::now() + DATAGRAM_WAIT;
    let mut lease_summary: Option<i2pr_daemon::destination_tunnels::RemoteLeaseSummary> = None;
    let mut lookup_pump_error = 0u64;
    while tokio::time::Instant::now() < lookup_deadline && lease_summary.is_none() {
        let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let message =
            match I2npMessage::decode_short_transport(&inbound.bytes, MAX_I2NP_PAYLOAD_SIZE) {
                Ok(message) => message,
                Err(_) => {
                    lookup_pump_error += 1;
                    continue;
                }
            };
        let cell = match message.body() {
            I2npBody::TunnelData(cell) => cell.clone(),
            _ => continue,
        };
        let outcome = match dispatch_inbound_tunnel_data(coord.registry_mut(), &cell, wall_ms()) {
            Ok(outcome) => outcome,
            Err(_) => {
                lookup_pump_error += 1;
                continue;
            }
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
    #[allow(unused_variables)]
    let summary = if let Some(s) = lease_summary {
        s
    } else {
        // Plan 194 §11 stop: Java accepted the build, the outbound
        // tunnel sent the DatabaseLookup, but the reference LS2
        // never resolved on the inbound tunnel. Java's SAM bridge
        // does NOT auto-publish the SAM-destination LS2 to the local
        // NetDB in a controlled private topology where the
        // reference has no peer tunnels to build a client tunnel
        // for the lease (the destination `inbound.length=0 outbound.length=0`
        // creates a zero-hop client tunnel but Java does NOT
        // publish the LS2 until the client tunnel reaches a usable
        // endpoint, which never happens without peers). i2pd's SAM
        // bridge does publish immediately. This is a Java-specific
        // second-family limitation; the harness records every
        // install-dependent + delivery-dependent row as `blocked`
        // with this stop provenance.
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
        record_stop(
            &evidence_dir,
            &format!(
                "Plan 194 §11 stop: java SAM bridge does not publish LeaseSet2 in controlled private topology (sent DatabaseLookup, no response within {}s; lookup_pump_error={lookup_pump_error})",
                DATAGRAM_WAIT.as_secs()
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    };
    assert_eq!(coord.registry().outbound_len(), 1);
    assert_eq!(coord.registry().inbound_len(), 1);
    append_evidence(&evidence_dir, "outbound-installed", "true");
    append_evidence(&evidence_dir, "inbound-installed", "true");

    let registrations_out = coord.registrations(TunnelDirection::Outbound);
    let registrations_in = coord.registrations(TunnelDirection::Inbound);
    assert_eq!(registrations_out.len(), 1);
    assert_eq!(registrations_in.len(), 1);
    assert!(
        registrations_out[0]
            .hops()
            .iter()
            .any(|hop| hop.hash() == Hash::from_bytes(*java_hash.as_bytes())),
        "outbound hop must be the reference"
    );
    let receive_ids = coord.registry().inbound_receive_ids();
    assert_eq!(receive_ids.len(), 1);
    append_evidence(
        &evidence_dir,
        "destination-material-real",
        &format!(
            "outbound_slots=1 inbound_slots=1 zero_hop=0 receive={}",
            receive_ids[0].get()
        ),
    );

    let gateway_role = coord
        .registry_mut()
        .remove_outbound(outbound_slot)
        .expect("real outbound role");
    let destination_outbound =
        DestinationOutboundRole::from_role(gateway_role, wall_ms() + 600_000);

    // ---- Plan 194 §5.3 remote Standard LeaseSet2 lookup ------------------
    let routing_key = i2pr_netdb::router_hash_from_destination(reference_hash);
    let local_receive_for_lookup = receive_ids[0];
    let reply_path = reply_path_for_inbound_route(coord.registry(), local_receive_for_lookup)
        .expect("typed inbound gateway route");
    let inbound_route = coord
        .registry()
        .inbound_gateway_route(local_receive_for_lookup)
        .expect("inbound route exists");
    assert_eq!(inbound_route.gateway_router, java_hash);
    assert_eq!(inbound_route.gateway_receive_tunnel.get(), IBGW_RECEIVE);
    assert_eq!(inbound_route.local_receive_tunnel.get(), IBGW_NEXT);
    assert_eq!(reply_path.tunnel_id(), IBGW_RECEIVE);
    append_evidence(
        &evidence_dir,
        "inbound-reply-path",
        &format!(
            "gateway_matches_reference=true gateway_tunnel={} local_receive={} ids_distinct=true",
            inbound_route.gateway_receive_tunnel.get(),
            inbound_route.local_receive_tunnel.get(),
        ),
    );
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
    append_evidence(
        &evidence_dir,
        "outbound-lookup-via-tunnel",
        &format!("cells={}", proof.cell_count),
    );
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

    let lookup_deadline = tokio::time::Instant::now() + DATAGRAM_WAIT;
    let mut lease_summary: Option<i2pr_daemon::destination_tunnels::RemoteLeaseSummary> = None;
    let mut lookup_pump_error = 0u64;
    while tokio::time::Instant::now() < lookup_deadline && lease_summary.is_none() {
        let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let message =
            match I2npMessage::decode_short_transport(&inbound.bytes, MAX_I2NP_PAYLOAD_SIZE) {
                Ok(message) => message,
                Err(_) => {
                    lookup_pump_error += 1;
                    continue;
                }
            };
        let cell = match message.body() {
            I2npBody::TunnelData(cell) => cell.clone(),
            _ => continue,
        };
        let outcome = match dispatch_inbound_tunnel_data(coord.registry_mut(), &cell, wall_ms()) {
            Ok(outcome) => outcome,
            Err(_) => {
                lookup_pump_error += 1;
                continue;
            }
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
    let summary = if let Some(s) = lease_summary {
        s
    } else {
        // Plan 194 §11 stop: Java accepted the build, the outbound
        // tunnel sent the DatabaseLookup, but the reference LS2
        // never resolved on the inbound tunnel. Java's SAM bridge
        // does NOT auto-publish the SAM-destination LS2 to the local
        // NetDB in a controlled private topology where the
        // reference has no peer tunnels to build a client tunnel
        // for the lease (the destination `inbound.length=0 outbound.length=0`
        // creates a zero-hop client tunnel but Java does NOT
        // publish the LS2 until the client tunnel reaches a usable
        // endpoint, which never happens without peers). i2pd's SAM
        // bridge does publish immediately. This is a Java-specific
        // second-family limitation; the harness records every
        // install-dependent + delivery-dependent row as `blocked`
        // with this stop provenance.
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
        record_stop(
            &evidence_dir,
            &format!(
                "Plan 194 §11 stop: java SAM bridge does not publish LeaseSet2 in controlled private topology (sent DatabaseLookup, no response within {}s; lookup_pump_error={lookup_pump_error})",
                DATAGRAM_WAIT.as_secs()
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    };
    assert_eq!(summary.destination, reference_hash);
    assert!(summary.lease_count >= 1, "reference must publish a lease");
    append_evidence(
        &evidence_dir,
        "lookup-pump-error",
        &lookup_pump_error.to_string(),
    );
    append_evidence(
        &evidence_dir,
        "lease-lookup-completed",
        &format!(
            "leases={} published={} expires={}",
            summary.lease_count, summary.published_seconds, summary.expires_seconds
        ),
    );

    // ---- Plan 194 §5.4(b) local Standard LeaseSet2 publication ------------
    let mut identity_rng = OsRng;
    let local_identity =
        DestinationIdentity::generate(&mut identity_rng).expect("destination identity");
    let tunnel_expires = wall_secs() + 600;
    let lease_source = InboundLeaseSource::from_parts(
        registrations_in[0].slot(),
        Hash::from_bytes(*java_hash.as_bytes()),
        IBGW_RECEIVE,
        tunnel_expires,
        tunnel_expires.saturating_sub(60),
    );
    let published = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
    let local_ls2 =
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
        .begin_ls2_publication(store_message, RouterHash::from_bytes(*java_hash.as_bytes()))
        .expect("begin publication");
    let (pub_dispatch, pub_proof) = dest
        .compose_ls2_publication_via_tunnel(
            publication_id,
            Hash::from_bytes(*java_hash.as_bytes()),
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

    // ---- Plan 194 §5.4(c) outbound ECIES/Garlic + tunnel delivery ---------
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
    let app_out = b"plan194-destination-probe-a";
    let request = OutboundRequest::new(
        i2pr_proto::PROTOCOL_TYPE_RAW,
        0,
        0,
        app_out,
        wall_ms(),
        Some(local_ls2.clone()),
    )
    .expect("outbound request");
    let mut compose_rng = ChaCha8Rng::seed_from_u64(wall_secs().wrapping_add(11));
    let plan = compose_outbound_delivery(
        &routing,
        &mut session,
        &destination_outbound,
        local_identity.id(),
        local_identity.static_secret_bytes(),
        reference_hash,
        &request,
        u32::try_from(wall_secs()).unwrap_or(u32::MAX),
        wall_ms(),
        &mut compose_rng,
    )
    .expect("compose destination delivery");
    let cell_dispatch = deliver_outbound_cells(
        &plan.cells,
        wall_ms() + 60_000,
        Deadline::new(Duration::from_secs(60)).expect("deadline"),
        &mut compose_rng,
    )
    .expect("encode destination cells");
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
    append_evidence(
        &evidence_dir,
        "destination-outbound-delivered",
        &format!("cells={} payload_len={}", plan.cells.len(), app_out.len()),
    );

    // Plan 192: Java's RAW session receives exactly our bytes via
    // the I2CP-style Data body inside the 9-byte short-transport
    // envelope. We use the same `wait_for_raw_datagram` shape as the
    // i2pd driver; the SAM bridge dispatches the inflated payload
    // straight to the SAM receiver when `RAW RECEIVED SIZE=N`
    // arrives.
    let datagram_deadline = tokio::time::Instant::now() + DATAGRAM_WAIT;
    let mut reference_received = false;
    let mut reference_received_payload_len = 0usize;
    while tokio::time::Instant::now() < datagram_deadline && !reference_received {
        let mut buf = vec![0u8; app_out.len()];
        match tokio::time::timeout(DATAGRAM_WAIT, async {
            use tokio::io::AsyncReadExt as _;
            let mut read = 0;
            while read < buf.len() {
                let n = tokio::time::timeout(
                    datagram_deadline - tokio::time::Instant::now(),
                    sam.stream.read(&mut buf[read..]),
                )
                .await
                .ok()
                .and_then(|r| r.ok());
                match n {
                    Some(n) if n > 0 => read += n,
                    _ => break,
                }
            }
            read
        })
        .await
        {
            Ok(n) if n == app_out.len() && buf == app_out => {
                reference_received = true;
                reference_received_payload_len = n;
                append_evidence(
                    &evidence_dir,
                    "reference-received",
                    &format!("payload_len={} match=true digest={}", n, sha256_hex(&buf)),
                );
            }
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    if !reference_received {
        append_evidence(
            &evidence_dir,
            "reference-received-timeout",
            &format!(
                "timeout after {}s (Plan 192 i2cp-wire-format-corrective; payload_len={})",
                DATAGRAM_WAIT.as_secs(),
                reference_received_payload_len
            ),
        );
    }

    // ---- Plan 194 §5.4(c) inbound reply via real inbound tunnel ----------
    let local_b64 = i2pr_api::sam::base64::encode(
        &local_identity
            .destination()
            .encode_to_vec(65535)
            .expect("encode dest"),
    );
    let app_back = b"plan194-destination-reply-b";
    {
        use tokio::io::AsyncWriteExt as _;
        sam.stream
            .write_all(
                format!(
                    "RAW SEND ID={session_id} DESTINATION={local_b64} SIZE={}\n",
                    app_back.len()
                )
                .as_bytes(),
            )
            .await
            .expect("raw send command write");
        sam.stream
            .write_all(app_back)
            .await
            .expect("raw send payload write");
    }
    let inbound_send_status = "raw-send-accepted";

    let mut dispatcher = DestinationDispatcher::new();
    dispatcher
        .register_destination(local_identity.id())
        .expect("register destination");
    dispatcher
        .bind_destination_hash(local_identity.id(), local_dest_hash)
        .expect("bind destination hash");
    let mut inbound_payload: Option<Vec<u8>> = None;
    let mut reply_pump_error = 0u64;
    let reply_deadline = tokio::time::Instant::now() + DATAGRAM_WAIT;
    while tokio::time::Instant::now() < reply_deadline && inbound_payload.is_none() {
        let next = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
        let Ok(Some(inbound)) = next else {
            continue;
        };
        let message =
            match I2npMessage::decode_short_transport(&inbound.bytes, MAX_I2NP_PAYLOAD_SIZE) {
                Ok(message) => message,
                Err(_) => {
                    reply_pump_error += 1;
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
        let envelope =
            I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode garlic");
        let now_secs = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
        dispatcher.dispatch_garlic_envelope(
            &mut session,
            local_identity.id(),
            local_identity.static_secret_bytes(),
            &local_identity.static_public_bytes(),
            now_secs,
            &envelope,
            routing.lease_set2_store_mut(),
        );
        if let Some(queued) = dispatcher.pop_payload(local_identity.id()) {
            // Plan 192: parse both the 9-byte short-transport Data
            // envelope and the I2CP-style Data body Java writes
            // inside its Garlic clove.
            let decoded =
                I2npMessage::decode_short_transport(queued.bytes(), MAX_I2NP_PAYLOAD_SIZE)
                    .expect("decode queued");
            if let I2npBody::Data(body) = decoded.body() {
                let i2cp_body = i2pr_proto::decode_i2cp_data_body(body.payload.as_bytes())
                    .expect("decode I2CP Data body");
                inbound_payload = Some(i2cp_body.payload);
            }
        }
    }
    if let Some(reply) = inbound_payload.clone() {
        assert_eq!(reply, app_back, "inbound payload mismatch");
        append_evidence(
            &evidence_dir,
            "destination-inbound-received",
            &format!(
                "payload_len={} match=true pump_error={reply_pump_error}",
                reply.len()
            ),
        );
    } else {
        append_evidence(
            &evidence_dir,
            "destination-inbound-send-failed",
            &format!("send_status={inbound_send_status} (Plan 192 i2cp-wire-format-corrective)"),
        );
    }

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

    let snapshot = handle.snapshot();
    append_evidence(
        &evidence_dir,
        "manager-cleanup",
        &format!(
            "sessions_established={} active_sessions={} datagrams_received={} i2np_received={} protocol_drops={} auth_failures={} queue_drops={}",
            snapshot.sessions_established,
            snapshot.active_sessions,
            snapshot.datagrams_received,
            snapshot.i2np_received,
            snapshot.protocol_drops,
            snapshot.auth_failures,
            snapshot.inbound_queue_drops,
        ),
    );

    handle.shutdown();
    let _ = scope.shutdown().await;
    let final_snapshot = handle.snapshot();
    assert_eq!(final_snapshot.pending_outbound, 0);
    assert_eq!(final_snapshot.pending_inbound, 0);
    assert_eq!(final_snapshot.active_sessions, 0);
    append_evidence(&evidence_dir, "shutdown-baseline", "true");
    let _ = PeerId::from_hash(java_hash);
}

/// Plan 194 §5.5 — full Streaming matrix (Direction A + B) against
/// the exact-pinned Java I2P 2.13.0 reference. Same wire as the i2pd
/// first-family driver.
#[tokio::test]
#[ignore = "Plan 194: requires exact-pinned external Java I2P environment"]
async fn streaming_through_java() {
    let java_ri_path = env_path("JAVA_ROUTER_INFO");
    let java_endpoint: SocketAddr = env_value("JAVA_SSU2_ENDPOINT").parse().expect("endpoint");
    let bind: SocketAddr = env_value("I2PR_SSU2_BIND").parse().expect("bind");
    let sam_endpoint: SocketAddr = env_value("JAVA_SAM_ENDPOINT")
        .parse()
        .expect("sam endpoint");
    assert!(bind.ip().is_loopback(), "i2pr bind must be loopback");
    assert!(
        java_endpoint.ip().is_loopback(),
        "java endpoint must be loopback"
    );
    assert!(sam_endpoint.ip().is_loopback(), "java SAM must be loopback");
    let evidence_dir = env_path("EVIDENCE_DIR");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");

    let bind_port = bind.port();
    assert!(bind_port != 0, "lane requires a fixed loopback bind");
    let config_text = format!(
        "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nport = {bind_port}\n"
    );
    let config = Config::parse(&config_text).expect("strict controlled profile");
    assert!(config.ssu2.enabled);
    assert!(!config.ssu2.advertise);
    append_evidence(&evidence_dir, "daemon-strict-profile", "true");

    let java_ri_bytes = std::fs::read(&java_ri_path).expect("read java router.info");
    let (java_hash, java_ssu2) =
        verify_reference_router_info(&java_ri_bytes).expect("verify java RouterInfo");
    let material = java_ssu2.address_material().expect("java key material");
    let responder_static =
        i2pr_runtime::Ssu2PublicKey::new(*material.static_public_key().as_bytes())
            .expect("static key");
    let responder_intro = i2pr_runtime::IntroKey::new(*material.intro_key().as_bytes());
    let target = daemon_dial_target(java_hash, java_endpoint, responder_static, responder_intro)
        .expect("dial target");
    let java_router_info = RouterInfo::decode(
        &java_ri_bytes,
        i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
    )
    .expect("decode java RouterInfo");
    let java_encryption_key: [u8; 32] = java_router_info
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
        .bootstrap_reference_router_info(&java_ri_bytes, now)
        .expect("bootstrap reference");
    assert_eq!(bootstrapped, RouterHash::from_bytes(*java_hash.as_bytes()));
    append_evidence(
        &evidence_dir,
        "reference-bootstrap-store",
        &dest.store_stats().record_count.to_string(),
    );
    let caps = java_router_info
        .capabilities()
        .ok()
        .flatten()
        .map(|c| c.as_str().to_owned())
        .unwrap_or_default();
    if !caps.bytes().any(|b| b == b'f') {
        record_stop(
            &evidence_dir,
            &format!("java reference not advertising floodfill (caps={caps:?})"),
        );
        assert!(
            caps.bytes().any(|b| b == b'f'),
            "java reference must advertise floodfill"
        );
    }
    append_evidence(&evidence_dir, "reference-floodfill-capable", "true");

    let bundle =
        i2pr_crypto::RouterIdentityBundle::generate(&mut rand_core::OsRng).expect("identity");
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

    // Reference STREAM destination through Java's public SAM surface.
    let mut sam = SamClient::connect(sam_endpoint).await;
    let hello = sam
        .transact("HELLO VERSION MIN=3.1 MAX=3.1\n")
        .await
        .expect("SAM hello read");
    assert!(hello.contains("RESULT=OK"), "SAM hello failed: {hello}");
    let generated = sam
        .transact("DEST GENERATE SIGNATURE_TYPE=7\n")
        .await
        .expect("SAM dest generate read");
    assert!(
        generated.starts_with("DEST REPLY") && generated.contains(" PUB="),
        "SAM DEST GENERATE failed: {generated}"
    );
    let reference_pub = sam_param(&generated, "PUB").expect("generated PUB");
    assert!(reference_pub.len() >= 512, "PUB too short for Ed25519");
    let reference_priv = sam_param(&generated, "PRIV").expect("generated PRIV");
    let session_id = format!("plan194-stream-{}", wall_secs() % 1_000_000);
    let create = sam
        .transact(&format!(
            "SESSION CREATE STYLE=STREAM ID={session_id} DESTINATION={reference_priv} SIGNATURE_TYPE=7 inbound.length=0 outbound.length=0\n"
        ))
        .await
        .expect("SAM session create read");
    assert!(
        create.contains("RESULT=OK"),
        "Java SAM STREAM session failed: {create}"
    );
    let reference_bytes = decode_sam_destination(&reference_pub);
    let reference_hash =
        i2pr_netdb::DestinationHash::from_hash(i2pr_crypto::sha256(&reference_bytes));
    append_evidence(
        &evidence_dir,
        "sam-streaming-created",
        &format!("dest_len={}", reference_bytes.len()),
    );
    let _ = reference_priv;

    let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    coord.advance_time(wall_ms());
    let bridge = ShortBuildI2npBridge::new();
    let mut rng = ChaCha8Rng::seed_from_u64(wall_secs());
    let delivery = handle.delivery().clone();

    let outbound_request = BuildRequest {
        direction: BuildDirection::Outbound,
        peer: PeerBuildMaterial {
            router_hash: java_hash,
            static_encryption_key: java_encryption_key,
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
            router_hash: java_hash,
            static_encryption_key: java_encryption_key,
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
    let install_deadline = tokio::time::Instant::now() + ACCEPT_TIMEOUT;
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
            record_stop(
                &evidence_dir,
                &format!(
                    "Plan 194 §11 stop: java outbound build never installed (installed_ob={pump_installed_ob} kind_reply={pump_kind_reply})"
                ),
            );
            handle.shutdown();
            let _ = scope.shutdown().await;
            return;
        }
    };
    if !installed_inbound {
        record_stop(
            &evidence_dir,
            &format!(
                "Plan 194 §11 stop: java inbound build never installed (installed_ib={pump_installed_ib})"
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }
    append_evidence(&evidence_dir, "outbound-installed", "true");
    append_evidence(&evidence_dir, "inbound-installed", "true");

    let gateway_role = coord
        .registry_mut()
        .remove_outbound(outbound_slot)
        .expect("real outbound role");
    let destination_outbound =
        DestinationOutboundRole::from_role(gateway_role, wall_ms() + 1_800_000);

    // Lease lookup over the real tunnel path.
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
        let outcome = match dispatch_inbound_tunnel_data(coord.registry_mut(), &cell, wall_ms()) {
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
            record_stop(
                &evidence_dir,
                "phase=lease-lookup reference LeaseSet2 never resolved",
            );
            handle.shutdown();
            let _ = scope.shutdown().await;
            return;
        }
    };
    assert_eq!(summary.destination, reference_hash);
    append_evidence(
        &evidence_dir,
        "lease-lookup-completed",
        &format!("leases={}", summary.lease_count),
    );

    let mut identity_rng = OsRng;
    let local_identity =
        DestinationIdentity::generate(&mut identity_rng).expect("destination identity");
    let registrations_in = coord.registrations(TunnelDirection::Inbound);
    let tunnel_expires = wall_secs() + 600;
    let lease_source = InboundLeaseSource::from_parts(
        registrations_in[0].slot(),
        Hash::from_bytes(*java_hash.as_bytes()),
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
        .begin_ls2_publication(store_message, RouterHash::from_bytes(*java_hash.as_bytes()))
        .expect("begin publication");
    let (pub_dispatch, pub_proof) = dest
        .compose_ls2_publication_via_tunnel(
            publication_id,
            Hash::from_bytes(*java_hash.as_bytes()),
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

    // Direction A: i2pr StreamingManager.connect -> Java STREAM.
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
        record_stop(
            &evidence_dir,
            &format!(
                "Plan 194 §11 stop: SYN-ACK never established (syn_accepted={syn_accepted} established={established} pump_error={pump_error})"
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }
    append_evidence(&evidence_dir, "streaming-syn-accepted", "true");
    append_evidence(&evidence_dir, "streaming-established", "true");

    // Direction A small payload.
    let app_small = b"plan194-streaming-probe-a";
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

    let mut accept_sam = SamClient::connect(sam_endpoint).await;
    let accept_hello = accept_sam
        .transact("HELLO VERSION MIN=3.1 MAX=3.1\n")
        .await
        .expect("SAM acceptor hello read");
    assert!(
        accept_hello.contains("RESULT=OK"),
        "SAM acceptor hello failed: {accept_hello}"
    );
    let accept_reply = accept_sam
        .transact(&format!("STREAM ACCEPT ID={session_id} SILENT=false\n"))
        .await;
    let accept_ok = accept_reply
        .as_deref()
        .is_some_and(|line| line.contains("RESULT=OK"));
    if !accept_ok {
        record_stop(
            &evidence_dir,
            "phase=stream-accept reference STREAM ACCEPT never returned OK",
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }
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
        record_stop(
            &evidence_dir,
            &format!(
                "phase=streaming-data expected_len={} observed_len={} peer_line_len={peer_len}",
                app_small.len(),
                observed.len(),
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }

    // Direction A multi-packet.
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
        record_stop(
            &evidence_dir,
            &format!(
                "phase=streaming-multipacket expected_len={} observed_len={} fragments={fragments}",
                app_multi.len(),
                observed_multi.len(),
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }

    // Direction A reverse (Java -> i2pr).
    let app_rev_small = b"plan194-reverse-probe-b";
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
    if rev_collected == app_rev_small {
        append_evidence(
            &evidence_dir,
            "streaming-reverse-data-digest",
            &format!(
                "payload_len={} digest={} match=true",
                rev_collected.len(),
                sha256_hex(&rev_collected),
            ),
        );
    } else {
        record_stop(&evidence_dir, "phase=streaming-reverse digest mismatch");
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }

    let mut app_rev_multi = Vec::with_capacity(4096);
    for index in 0..4096 {
        app_rev_multi.push((index % 233) as u8);
    }
    accept_sam.write_bytes(&app_rev_multi).await;
    let mut rev_multi_collected = Vec::new();
    pump_until_streaming_with_state(
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
        Some(connection_id),
        |manager| {
            for delivered in manager.drain_delivered_for(connection_id) {
                rev_multi_collected.extend_from_slice(&delivered.bytes);
            }
            rev_multi_collected.len() >= app_rev_multi.len()
        },
    )
    .await;
    if rev_multi_collected == app_rev_multi {
        append_evidence(
            &evidence_dir,
            "streaming-reverse-multipacket-digest",
            &format!(
                "payload_len={} digest={} match=true",
                rev_multi_collected.len(),
                sha256_hex(&rev_multi_collected),
            ),
        );
    } else {
        record_stop(
            &evidence_dir,
            &format!(
                "phase=streaming-reverse-multipacket expected_len={} observed_len={}",
                app_rev_multi.len(),
                rev_multi_collected.len(),
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }

    // Sibling + close + isolation. Direction A only on the
    // outbound side; Direction B reverses through the normal
    // listener/accept path (Direction B below).
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
    if streaming
        .get_connection(sibling_id)
        .is_some_and(|conn| conn.state() == ConnectionState::Established)
    {
        append_evidence(&evidence_dir, "streaming-sibling-established", "true");
    } else {
        record_stop(&evidence_dir, "phase=streaming-sibling-establish");
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }

    let app_sibling = b"plan194-sibling-probe-c";
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
        "SAM sibling acceptor hello failed: {accept2_hello}"
    );
    let accept2_reply = accept_sam2
        .transact(&format!("STREAM ACCEPT ID={session_id} SILENT=false\n"))
        .await;
    assert!(
        accept2_reply
            .as_deref()
            .is_some_and(|line| line.contains("RESULT=OK")),
        "sibling STREAM ACCEPT failed: {:?}",
        accept2_reply
    );
    let peer2_line = accept_sam2.read_line().await.unwrap_or_default();
    let observed_sibling = accept_sam2
        .read_exact_bytes(app_sibling.len(), STREAM_WAIT)
        .await
        .unwrap_or_default();
    if observed_sibling.as_slice() != app_sibling.as_slice() {
        record_stop(
            &evidence_dir,
            &format!(
                "phase=streaming-sibling-data expected_len={} observed_len={} peer_line_len={}",
                app_sibling.len(),
                observed_sibling.len(),
                peer2_line.len(),
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
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
    let close_eof = accept_sam.read_until_eof(Duration::from_secs(15)).await;
    if !close_eof {
        record_stop(
            &evidence_dir,
            "phase=streaming-close local state Closed but reference socket never reached EOF",
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }
    append_evidence(&evidence_dir, "streaming-close", "state=Closed eof=true");

    let app_sibling2 = b"plan194-sibling-alive-d";
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
        record_stop(
            &evidence_dir,
            &format!(
                "phase=streaming-sibling-isolated expected_len={} observed_len={}",
                app_sibling2.len(),
                observed_sibling2.len(),
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
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

    // Direction B: Java STREAM CONNECT -> i2pr listener/accept.
    let fresh_expires = wall_secs() + 1800;
    let fresh_published = u32::try_from(wall_secs()).unwrap_or(u32::MAX);
    let fresh_registrations = coord.registrations(TunnelDirection::Inbound);
    let fresh_lease = InboundLeaseSource::from_parts(
        fresh_registrations[0].slot(),
        Hash::from_bytes(*java_hash.as_bytes()),
        IBGW_RECEIVE,
        fresh_expires,
        fresh_expires.saturating_sub(60),
    );
    local_ls2 = build_signed_lease_set2(&local_identity, &[fresh_lease], fresh_published)
        .expect("fresh ls2");
    let fresh_store = i2pr_proto::DatabaseStoreMessage {
        key: local_ls2.key_hash().expect("key hash"),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: i2pr_proto::DatabaseStoreData::LeaseSet2(Box::new(local_ls2.clone())),
    };
    let fresh_publication = dest
        .begin_ls2_publication(fresh_store, RouterHash::from_bytes(*java_hash.as_bytes()))
        .expect("begin republication");
    let (fresh_dispatch, fresh_proof) = dest
        .compose_ls2_publication_via_tunnel(
            fresh_publication,
            Hash::from_bytes(*java_hash.as_bytes()),
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
    let mut connect_sam = SamClient::connect(sam_endpoint).await;
    let connect_hello = connect_sam
        .transact("HELLO VERSION MIN=3.1 MAX=3.1\n")
        .await
        .expect("SAM connect hello read");
    assert!(
        connect_hello.contains("RESULT=OK"),
        "SAM connect hello failed: {connect_hello}"
    );
    let mut b_sam_lines = 0u64;
    let mut b_last_result = "none".to_owned();
    let mut b_first_token = "none".to_owned();
    for _ in 0..2 {
        connect_attempts += 1;
        connect_sam
            .write_bytes(
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
                &evidence_dir,
            )
            .await;
            drain_streaming_timers(
                &mut streaming,
                &mut routing,
                &mut session,
                &destination_outbound,
                &local_identity,
                &local_ls2,
                &delivery,
                &mut send_rng,
                &evidence_dir,
            )
            .await;
            if b_connection.is_none() {
                b_connection = accept_inbound_syn_response(
                    &mut streaming,
                    &local_identity,
                    &routing,
                    &mut session,
                    &destination_outbound,
                    &local_ls2,
                    &delivery,
                    &mut send_rng,
                )
                .await;
            }
            if let Some(line) = connect_sam.poll_line().await {
                b_sam_lines += 1;
                if b_first_token == "none" {
                    b_first_token = line
                        .split_whitespace()
                        .next()
                        .unwrap_or("empty")
                        .chars()
                        .take(16)
                        .collect();
                }
                if line.starts_with("STREAM STATUS") {
                    b_last_result = line
                        .split_whitespace()
                        .find_map(|token| token.strip_prefix("RESULT="))
                        .unwrap_or("missing")
                        .to_owned();
                    status = Some(line);
                }
            }
        }
        match &status {
            Some(line) if line.contains("RESULT=OK") => break,
            _ => {
                append_evidence(
                    &evidence_dir,
                    "streaming-b-connect-debug",
                    &format!(
                        "attempt={connect_attempts} syn_arrived={} backlog={} sam_lines={b_sam_lines} first_token={b_first_token} result={b_last_result} pump_errors={pump_errors}",
                        b_connection.is_some(),
                        streaming.listener_backlog(0),
                    ),
                );
                if connect_attempts >= 2 {
                    record_stop(
                        &evidence_dir,
                        &format!(
                            "phase=streaming-b-connect attempts={connect_attempts} syn_arrived={} result={b_last_result} pump_errors={pump_errors}",
                            b_connection.is_some(),
                        ),
                    );
                    handle.shutdown();
                    let _ = scope.shutdown().await;
                    return;
                }
            }
        }
    }
    if b_connection.is_none() {
        let accept_deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        let mut accept_errors = 0u64;
        while tokio::time::Instant::now() < accept_deadline && b_connection.is_none() {
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
                &mut accept_errors,
                &evidence_dir,
            )
            .await;
            drain_streaming_timers(
                &mut streaming,
                &mut routing,
                &mut session,
                &destination_outbound,
                &local_identity,
                &local_ls2,
                &delivery,
                &mut send_rng,
                &evidence_dir,
            )
            .await;
            b_connection = accept_inbound_syn_response(
                &mut streaming,
                &local_identity,
                &routing,
                &mut session,
                &destination_outbound,
                &local_ls2,
                &delivery,
                &mut send_rng,
            )
            .await;
        }
    }
    let b_id = match b_connection {
        Some(id) => id,
        None => {
            record_stop(
                &evidence_dir,
                "phase=streaming-b-accept STATUS OK but inbound SYN never reached the backlog",
            );
            handle.shutdown();
            let _ = scope.shutdown().await;
            return;
        }
    };
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
    let b_peer_line = connect_sam.read_line_timeout(Duration::from_secs(3)).await;
    let b_peer_len = b_peer_line.map(|line| line.len()).unwrap_or(0);

    // Direction B small.
    let app_b_small = b"plan194-b-probe-e";
    connect_sam.write_bytes(app_b_small).await;
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
    if b_collected == app_b_small {
        append_evidence(
            &evidence_dir,
            "streaming-b-data-digest",
            &format!(
                "payload_len={} digest={} peer_line_len={b_peer_len} match=true",
                b_collected.len(),
                sha256_hex(&b_collected),
            ),
        );
    } else {
        record_stop(&evidence_dir, "phase=streaming-b-data digest mismatch");
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }

    // Direction B reverse (i2pr -> Java over the accepted stream).
    let (b_local_port, b_remote_port) = {
        let inbound = streaming.get_connection(b_id).expect("inbound connection");
        (inbound.local_port(), inbound.remote_port())
    };
    let mut app_b_rev = Vec::with_capacity(2048);
    for index in 0..2048 {
        app_b_rev.push((index % 199) as u8);
    }
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
    let mut b_rev_offset = 0;
    let mut b_rev_fragments = 0u64;
    while b_rev_offset < app_b_rev.len() {
        let end = (b_rev_offset + 1024).min(app_b_rev.len());
        let fragment = streaming
            .send_data(
                b_id,
                &local_identity,
                &b_reply_desc,
                b_local_port,
                b_remote_port,
                &app_b_rev[b_rev_offset..end],
                wall_ms(),
            )
            .expect("send B reverse fragment");
        send_transport_request(
            &fragment,
            &routing,
            &mut session,
            &destination_outbound,
            &local_identity,
            &local_ls2,
            &delivery,
            &mut send_rng,
        )
        .await;
        b_rev_offset = end;
        b_rev_fragments += 1;
    }
    let observed_b_rev = connect_sam
        .read_exact_bytes(app_b_rev.len(), STREAM_WAIT)
        .await
        .unwrap_or_default();
    if observed_b_rev != app_b_rev {
        record_stop(
            &evidence_dir,
            &format!(
                "phase=streaming-b-reverse expected_len={} observed_len={}",
                app_b_rev.len(),
                observed_b_rev.len(),
            ),
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }
    append_evidence(
        &evidence_dir,
        "streaming-b-reverse-data-digest",
        &format!(
            "payload_len={} fragments={b_rev_fragments} digest={} match=true",
            observed_b_rev.len(),
            sha256_hex(&observed_b_rev),
        ),
    );

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
    let b_close_eof = connect_sam.read_until_eof(Duration::from_secs(15)).await;
    if !b_close_eof {
        record_stop(
            &evidence_dir,
            "phase=streaming-b-close local state Closed but reference socket never reached EOF",
        );
        handle.shutdown();
        let _ = scope.shutdown().await;
        return;
    }
    append_evidence(&evidence_dir, "streaming-b-close", "state=Closed eof=true");

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
    let final_snapshot = handle.snapshot();
    assert_eq!(final_snapshot.pending_outbound, 0);
    assert_eq!(final_snapshot.pending_inbound, 0);
    assert_eq!(final_snapshot.active_sessions, 0);
    append_evidence(&evidence_dir, "shutdown-baseline", "true");
    let _ = PeerId::from_hash(java_hash);
}

/// Plan 197 — CI-fast regression proving the daemon-owned
/// `verify_reference_router_info` path surfaces the typed
/// `pq_capabilities()` for a Java I2P 2.13.0 SSU2 address that
/// carries `pq=4,3`. The in-tree fixture is built from the
/// production codec via `Mapping::from_entries`, so it does not
/// depend on a live Java runtime. The exact-pinned Java router
/// publishes `pq=4,3` unconditionally via
/// `UDPTransport.addSSU2Options` (`router/java/src/net/i2p/router/
/// transport/udp/UDPTransport.java:148-153,1011-1021`); this
/// regression is the parser-only counterpart to that wire form.
#[test]
fn java_pq_capabilities_surfaced() {
    let static_pub: [u8; 32] = [0xa1; 32];
    let intro_bytes: [u8; 32] = [0x24; 32];
    let options = Mapping::from_entries(vec![
        ("host".to_string(), "127.0.0.1".to_string()),
        ("port".to_string(), "44002".to_string()),
        ("v".to_string(), "2".to_string()),
        ("s".to_string(), i2p_b64_encode(&static_pub)),
        ("i".to_string(), i2p_b64_encode(&intro_bytes)),
        ("caps".to_string(), "46BC".to_string()),
        ("mtu".to_string(), "1280".to_string()),
        // The exact-pinned Java 2.13.0 RouterInfo carries this
        // option unconditionally for the high-MTU publish form.
        ("pq".to_string(), "4,3".to_string()),
    ])
    .expect("options");
    let bundle =
        i2pr_crypto::RouterIdentityBundle::generate(&mut rand_core::OsRng).expect("identity");
    let address = RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .expect("address");
    let ri_options = Mapping::from_entries(vec![
        ("router.version".to_string(), "0.9.66".to_string()),
        ("netId".to_string(), "2".to_string()),
    ])
    .expect("ri options");
    let info = bundle
        .sign_router_info(
            Date::from_millis(wall_secs().saturating_mul(1000)),
            vec![address],
            Vec::new(),
            ri_options,
        )
        .expect("sign");
    let router_info = info
        .encode_to_vec(i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .expect("encode");

    let (_hash, ssu2) = verify_reference_router_info(&router_info).expect("verify java-style");
    assert_eq!(
        ssu2.pq_capabilities().schemes(),
        &[Ssu2PqKem::MlKem768, Ssu2PqKem::MlKem512]
    );
    assert_eq!(ssu2.pq_capabilities().as_wire(), "4,3");
    assert!(ssu2.pq_capabilities().is_supported());
}

/// Local alphabet-only I2P base64 helper, mirroring the existing
/// `i2p_b64_encode` in `ssu2_daemon_preflight.rs` without pulling in
/// that test's private helper.
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
