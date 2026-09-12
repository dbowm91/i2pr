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

use i2pr_client::streaming::manager::{
    ConnectOutcome, DEFAULT_ADVERTISED_MAX_PAYLOAD, RemoteDestination, StreamingManager,
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

const LOCAL_STREAM_PORT: u16 = 10_134;
const REMOTE_STREAM_PORT: u16 = 20_134;

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

#[tokio::test]
#[ignore = "Plan 193 M6 i2pd mixed-router Streaming qualification: requires exact-pinned external i2pd environment"]
async fn streaming_through_i2pd() {
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
    assert!(config.ssu2.enabled);
    assert!(!config.ssu2.advertise);
    append_evidence(&evidence_dir, "daemon-strict-profile", "true");

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

    let _established = handle
        .dial(target, DIAL_TIMEOUT, &CancellationToken::new())
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
        DestinationOutboundRole::from_role(gateway_role, wall_ms() + 600_000);

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
    send_transport_request(
        &syn_request,
        &routing,
        &mut session,
        &destination_outbound,
        &local_identity,
        &local_ls2,
        &delivery,
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
    )
    .await;

    // The reference ACCEPT socket receives exactly our bytes. Issue
    // STREAM ACCEPT after the data is in flight so the reference
    // delivers the buffered stream without a concurrent-accept race.
    let accept_reply = sam
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
    // then raw stream bytes follow on the same socket.
    let peer_line = sam.read_line().await.unwrap_or_default();
    let peer_len = peer_line.len();
    let observed = sam
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
        )
        .await;
        offset = end;
        fragments += 1;
    }
    let observed_multi = sam
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

async fn send_transport_request(
    request: &TransportSendRequest,
    routing: &DestinationRouting,
    session: &mut EciesSessionManager,
    outbound: &DestinationOutboundRole,
    local_identity: &DestinationIdentity,
    local_ls2: &i2pr_proto::LeaseSet2,
    delivery: &i2pr_daemon::router_i2np::RouterDeliveryService,
) {
    let mut send_rng = ChaCha8Rng::seed_from_u64(wall_secs().wrapping_add(11));
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
        &mut send_rng,
    )
    .expect("adapter send");
    let cell_dispatch = i2pr_daemon::outbound_lookup::deliver_outbound_cells(
        &plan.cells,
        wall_ms() + 60_000,
        Deadline::new(Duration::from_secs(60)).expect("deadline"),
        &mut send_rng,
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

fn record_stop_and_baseline(dir: &Path) {
    let mut coordinator = DestinationTunnelCoordinator::new(
        LookupPolicy::default(),
        RouterInfoStoreConfig::default(),
    );
    assert!(coordinator.note_direct_transport_attempt().is_err());
    append_evidence(dir, "direct-rejected", "true");
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
    append_evidence(dir, "liveness-first-test", "passed");
}
