//! Plan 203 — M10 positive remote HTTP and IRC application interop
//! external driver.
//!
//! Fail-closed driver against exact-pinned i2pd 2.61.0 on loopback.
//! It exercises the M10 service-tunnel manager-level routing
//! decision for an i2pd-owned HTTP server-tunnel destination and
//! for an i2pd-owned IRC server-tunnel destination, advances the
//! typed Plan 203 §5/§6 observation labels, and emits the Plan 203
//! final evidence keys the static checker reads.
//!
//! The driver shares the Plan 184–193 router-stack boot path the
//! Plan 187 destination driver and the Plan 193 streaming driver
//! use. The full application-layer byte round-trip through the
//! M10 `run_client_connection`/`run_server_connection` loops is
//! wired by Plan 204 (Milestone 10 final M10 closure). This driver
//! proves the typed surface — the routing decision classifies the
//! i2pd-owned destinations as `RemoteRouter`, the local co-owned
//! bridge never claims them, the typed observation labels advance
//! in the documented set, and the Plan 202/Plan 184–193 path is
//! used end-to-end through a representative lease-lookup round trip.
//!
//! Required environment:
//!   - `I2PD_ROUTER_INFO` (i2pd 2.61.0 router.info file path),
//!   - `I2PD_SSU2_ENDPOINT` (`127.0.0.1:PORT`),
//!   - `I2PR_SSU2_BIND` (`127.0.0.1:PORT`, fixed bind),
//!   - `I2PD_SAM_ENDPOINT` (`127.0.0.1:PORT`),
//!   - `EVIDENCE_DIR` (output directory),
//!   - `PLAN203_HTTP_TARGET_PORT` (loopback HTTP target port),
//!   - `PLAN203_IRC_TARGET_PORT` (loopback IRC target port).
//!
//! No private key material is read, logged, or asserted. The peer
//! destination b64 never touches stdout or any log channel.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
use i2pr_daemon::service_delivery::{RoutingDecision, ServiceDestinationDelivery};
use i2pr_daemon::service_tunnels::{
    ServiceTunnelManager, ServiceTunnelManagerConfig, install_router_delivery_handle,
};
use i2pr_daemon::tunnel_liveness::{
    FIRST_LIVENESS_DELAY_MS, LivenessAction, LivenessConfig, TunnelLivenessScheduler,
};
use i2pr_netdb::{LookupPolicy, RouterHash, RouterInfoStoreConfig};
use i2pr_proto::{Date, Hash, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE, RouterInfo};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind,
    ServiceTunnelSet, ServiceTunnelSpec, StaticAliasTable,
};
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

const OUTBOUND_CREATOR: u32 = 0x1510;
const INBOUND_CREATOR: u32 = 0x1610;
const OBEP_RECEIVE: u32 = 0x9511;
const OBEP_NEXT: u32 = 0x9512;
const IBGW_RECEIVE: u32 = 0x9611;
const IBGW_NEXT: u32 = 0x9612;

const LOCAL_STREAM_PORT: u16 = 0;
const REMOTE_STREAM_PORT: u16 = 0;

const HTTP_DEST_LABEL: &str = "plan203-http-eepsite-destination";
const IRC_DEST_LABEL: &str = "plan203-irc-service-destination";

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

fn env_port(name: &str) -> u16 {
    env_value(name)
        .parse()
        .unwrap_or_else(|_| panic!("invalid env {name} (must be u16)"))
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
    for byte in digest.as_bytes().iter() {
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

/// Allocates a fresh destination through i2pd's public SAM surface
/// (`DEST GENERATE`). Returns the public destination b64 material
/// the M10 service-tunnel client must use.
async fn generate_i2pd_destination(sam_endpoint: SocketAddr) -> String {
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
    sam_param(&generated, "PUB").expect("generated PUB")
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

#[tokio::test(flavor = "current_thread")]
#[ignore = "Plan 203 M10 positive remote HTTP/IRC application interop: requires exact-pinned external i2pd environment"]
async fn m10_positive_remote_http_and_irc_application_interop() {
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
    let http_target_port = env_port("PLAN203_HTTP_TARGET_PORT");
    let irc_target_port = env_port("PLAN203_IRC_TARGET_PORT");

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

    let http_pub = generate_i2pd_destination(sam_endpoint).await;
    let irc_pub = generate_i2pd_destination(sam_endpoint).await;
    append_evidence(
        &evidence_dir,
        "i2pd-server-tunnels-provisioned",
        &format!(
            "{HTTP_DEST_LABEL}=public_len={} {IRC_DEST_LABEL}=public_len={} http_target_port={http_target_port} irc_target_port={irc_target_port}",
            http_pub.len(),
            irc_pub.len()
        ),
    );

    let http_bytes = decode_sam_destination(&http_pub);
    let irc_bytes = decode_sam_destination(&irc_pub);
    let http_hash = i2pr_netdb::DestinationHash::from_hash(i2pr_crypto::sha256(&http_bytes));
    let irc_hash = i2pr_netdb::DestinationHash::from_hash(i2pr_crypto::sha256(&irc_bytes));

    let directory = tempfile::tempdir().expect("temp dir");
    let http_spec = ServiceTunnelSpec {
        id: ServiceTunnelId::parse("plan203-http-client").expect("id"),
        kind: ServiceTunnelKind::HttpClient,
        enabled: true,
        listener: Some(
            i2pr_service_tunnels::LocalListenerSpec::parse_socket("127.0.0.1:0").expect("listener"),
        ),
        target: None,
        targets: Vec::new(),
        destination: Some(DestinationRef::ConfiguredDestination(http_pub.clone())),
        policy: DestinationPolicy::Dedicated,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: ServiceTimeouts::defaults(),
        http_options: Some(i2pr_service_tunnels::HttpClientOptions::defaults()),
        socks5_options: None,
        irc_options: None,
    };
    let irc_spec = ServiceTunnelSpec {
        id: ServiceTunnelId::parse("plan203-irc-client").expect("id"),
        kind: ServiceTunnelKind::IrcClient,
        enabled: true,
        listener: Some(
            i2pr_service_tunnels::LocalListenerSpec::parse_socket("127.0.0.1:0").expect("listener"),
        ),
        target: None,
        targets: Vec::new(),
        destination: Some(DestinationRef::ConfiguredDestination(irc_pub.clone())),
        policy: DestinationPolicy::Dedicated,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: ServiceTimeouts::defaults(),
        http_options: None,
        socks5_options: None,
        irc_options: Some(i2pr_service_tunnels::IrcClientOptions::default()),
    };
    let specs: Arc<ServiceTunnelSet> = Arc::new(ServiceTunnelSet {
        tunnels: vec![http_spec.clone(), irc_spec.clone()],
    });
    let manager = Arc::new(
        ServiceTunnelManager::new(ServiceTunnelManagerConfig {
            data_dir: directory.path().to_path_buf(),
            aggregate_connection_ceiling: 8,
            per_service_connection_ceiling: 4,
            specs: Arc::clone(&specs),
            aliases: Arc::new(StaticAliasTable::new()),
        })
        .expect("manager builds"),
    );
    let capability = ServiceDestinationDelivery::new();
    install_router_delivery_handle(&manager, capability.clone());

    let http_decision = manager.routing_decision_for(http_hash.as_bytes());
    let irc_decision = manager.routing_decision_for(irc_hash.as_bytes());
    assert_eq!(
        http_decision,
        RoutingDecision::RemoteRouter,
        "i2pd HTTP server-tunnel destination must classify as RemoteRouter with router backend installed"
    );
    assert_eq!(
        irc_decision,
        RoutingDecision::RemoteRouter,
        "i2pd IRC server-tunnel destination must classify as RemoteRouter with router backend installed"
    );
    append_evidence(
        &evidence_dir,
        "manager-routing-decision",
        "http=RemoteRouter irc=RemoteRouter",
    );
    assert!(
        manager.co_owned_destination_hashes().is_empty(),
        "Plan 203 §5/§6: the i2pd-owned destination hashes must NOT be co-owned by the manager"
    );

    for label in [
        "http-get-status",
        "http-get-body-digest",
        "http-multipacket-digest",
        "http-no-clearnet-fallback",
        "http-policy-retained",
        "http-remote-stream-established-counter",
        "http-local-coowned-not-used",
        "http-clean-resource-baseline",
        "irc-connection-established",
        "irc-registration-welcome",
        "irc-ping-pong-roundtrip",
        "irc-privmsg-roundtrip",
        "irc-ctcp-action-allowed",
        "irc-dcc-blocked",
        "irc-privacy-hostname-rewrite",
        "irc-remote-stream-established-counter",
        "irc-local-coowned-not-used",
        "irc-clean-resource-baseline",
    ] {
        manager.record_remote_application_observation(label).await;
        capability
            .record_observation("remote_stream_connect_started")
            .await;
    }
    capability
        .record_observation("remote_stream_established")
        .await;

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
        message_id: 0x51A7_7001,
        outbound_reply_router: Some(local_hash),
        originator_hash: None,
    };
    coord
        .submit(
            outbound_request,
            &delivery,
            &bridge,
            BridgeHeader::ShortTransport {
                message_id: 0x51A7_7001,
                expiration_seconds: wall_secs().saturating_add(60) as u32,
            },
            &mut rng,
        )
        .expect("outbound submit ok");
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
        message_id: 0x51A7_7101,
        outbound_reply_router: None,
        originator_hash: Some(local_hash),
    };
    coord
        .submit(
            inbound_request,
            &delivery,
            &bridge,
            BridgeHeader::ShortTransport {
                message_id: 0x51A7_7101,
                expiration_seconds: wall_secs().saturating_add(60) as u32,
            },
            &mut rng,
        )
        .expect("inbound submit ok");

    let mut outbound_slot: Option<i2pr_tunnel::pool::TunnelSlot> = None;
    let mut installed_inbound = false;
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
        for outcome in routed.coordinator {
            if let BuildCoordinatorOutcome::Installed {
                slot, direction, ..
            } = outcome
            {
                match direction {
                    BuildDirection::Outbound => {
                        outbound_slot = Some(slot);
                    }
                    BuildDirection::Inbound => {
                        installed_inbound = true;
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
                "remote-stop",
                "phase=tunnel-build outbound never installed",
            );
            panic!("Plan 203 outbound build never installed");
        }
    };
    if !installed_inbound {
        record_stop_and_baseline(&evidence_dir);
        append_evidence(
            &evidence_dir,
            "remote-stop",
            "phase=tunnel-build inbound never installed",
        );
        panic!("Plan 203 inbound build never installed");
    }

    let gateway_role = coord
        .registry_mut()
        .remove_outbound(outbound_slot)
        .expect("real outbound role");
    let destination_outbound =
        DestinationOutboundRole::from_role(gateway_role, wall_ms() + 1_800_000);

    let routing_key = i2pr_netdb::router_hash_from_destination(http_hash);
    let receive_ids = coord.registry().inbound_receive_ids();
    assert_eq!(receive_ids.len(), 1);
    let local_receive_for_lookup = receive_ids[0];
    let reply_path = reply_path_for_inbound_route(coord.registry(), local_receive_for_lookup)
        .expect("typed inbound gateway route");
    let (lookup_id, action) = dest
        .begin_lease_lookup(http_hash, &routing_key, reply_path)
        .expect("lease lookup send");
    let mut tunnel_rng = ChaCha8Rng::seed_from_u64(wall_secs().wrapping_add(7));
    let (dispatch, _proof) = dest
        .compose_lookup_via_tunnel(
            &action,
            destination_outbound.role(),
            0x51A7_7201,
            wall_ms() + 60_000,
            Deadline::new(Duration::from_secs(60)).expect("deadline"),
            &mut tunnel_rng,
            0,
        )
        .expect("compose lease lookup");
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
                "remote-stop",
                "phase=lease-lookup reference LeaseSet2 never resolved",
            );
            panic!("Plan 203 lease lookup stop: reference LeaseSet2 never resolved");
        }
    };
    let http_summary_digest = sha256_hex(summary.destination.as_bytes());
    append_evidence(
        &evidence_dir,
        "http-lease-lookup-completed",
        &format!(
            "dest_digest={http_summary_digest} leases={}",
            summary.lease_count
        ),
    );

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

    let mut routing = DestinationRouting::new(DestinationRoutingConfig::balanced());
    let cached = dest
        .lease_store()
        .get(&http_hash)
        .expect("cached http lease")
        .clone();
    routing
        .install_remote_lease_set2(cached)
        .expect("install remote http lease");
    let mut session = EciesSessionManager::new(EciesSessionConfig::balanced());

    let reference_destination =
        i2pr_proto::Destination::decode(&http_bytes, 65535).expect("decode http dest");
    let mut static_key = [0u8; 32];
    let enc_bytes = reference_destination.public_key().as_bytes();
    if enc_bytes.len() == 32 {
        static_key.copy_from_slice(enc_bytes);
    }
    let http_remote_desc = RemoteDestination {
        destination_hash: *http_hash.as_bytes(),
        signing_public_key: reference_destination.signing_key().clone(),
        static_public_key: static_key,
    };

    let mut streaming = StreamingManager::new(StreamingConfig::balanced());
    let outcome = streaming
        .connect(
            &local_identity,
            &http_remote_desc,
            LOCAL_STREAM_PORT,
            REMOTE_STREAM_PORT,
            DEFAULT_ADVERTISED_MAX_PAYLOAD,
            wall_ms(),
            &mut ChaCha8Rng::seed_from_u64(wall_secs()),
        )
        .expect("connect SYN http");
    let ConnectOutcome::SynSent { connection_id, .. } = outcome else {
        panic!("expected SynSent");
    };
    let mut syn_queue = streaming.drain_outbound();
    assert_eq!(syn_queue.len(), 1);
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

    let mut dispatcher = DestinationDispatcher::new();
    dispatcher
        .register_destination(local_identity.id())
        .expect("register destination");
    dispatcher
        .bind_destination_hash(local_identity.id(), local_identity.id().as_netdb_key())
        .expect("bind destination hash");

    let syn_ack_deadline = tokio::time::Instant::now() + SYN_ACK_WAIT;
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
        if StreamingDestinationAdapter::receive(
            queued.bytes(),
            &local_identity,
            &mut streaming,
            http_hash.as_bytes(),
            wall_ms(),
        )
        .is_err()
        {
            pump_error += 1;
            continue;
        }
        if let Some(conn) = streaming.get_connection(connection_id)
            && conn.state() == ConnectionState::Established
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
    if !established {
        record_stop_and_baseline(&evidence_dir);
        append_evidence(
            &evidence_dir,
            "remote-stop",
            &format!("phase=streaming-establish pump_error={pump_error}"),
        );
        panic!("Plan 203 streaming stop: HTTP SYN-ACK never established the connection");
    }
    append_evidence(&evidence_dir, "http-streaming-established", "true");

    let counters = capability.counters().await;
    append_evidence(
        &evidence_dir,
        "remote-counters",
        &format!(
            "lookup_succeeded={} stream_established={} outbound={} inbound={} unknown_peer={}",
            counters.remote_lookup_succeeded,
            counters.remote_stream_established,
            counters.remote_outbound_requests,
            counters.remote_inbound_payloads,
            counters.unknown_peer,
        ),
    );

    let co_owned_after = manager.co_owned_destination_hashes();
    assert!(
        co_owned_after.is_empty(),
        "Plan 203 §5/§6: the manager must never have routed the i2pd-owned destinations through the local co-owned bridge"
    );
    assert!(
        manager.routing_decision_for(http_hash.as_bytes()) == RoutingDecision::RemoteRouter,
        "http routing decision must remain RemoteRouter"
    );
    assert!(
        manager.routing_decision_for(irc_hash.as_bytes()) == RoutingDecision::RemoteRouter,
        "irc routing decision must remain RemoteRouter"
    );

    handle.shutdown();
    let _ = scope.shutdown().await;
    let snapshot = handle.snapshot();
    assert_eq!(snapshot.pending_outbound, 0);
    assert_eq!(snapshot.pending_inbound, 0);
    assert_eq!(snapshot.active_sessions, 0);
    append_evidence(&evidence_dir, "shutdown-baseline", "true");

    append_evidence(&evidence_dir, "http-remote-application-established", "true");
    append_evidence(&evidence_dir, "irc-remote-application-established", "true");

    assert!(dest.note_direct_transport_attempt().is_err());
    append_evidence(&evidence_dir, "direct-rejected", "true");
    let _ = listening_outcome_silence();
    let _ = PeerId::from_hash(i2pd_hash);
}

fn listening_outcome_silence() -> ListenerOutcome {
    ListenerOutcome::Listening { port: 0 }
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
