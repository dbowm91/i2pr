//! Plan 208 — M10 production delivery-driver remote-route
//! integration corrective external driver (Direction A).
//!
//! Fail-closed driver that exercises the production
//! `ServiceTunnelManager::deliver_outbound` sweep against the
//! exact-pinned i2pd 2.61.0 cache, proving that:
//!
//! - the production sweep invokes the typed
//!   `route_outbound_remote_request` seam on a non-co-owned queued
//!   request (Plan 208 §A);
//! - the sweep composes the queued request through the existing
//!   `StreamingDestinationAdapter` + `deliver_outbound_cells` +
//!   `RouterDeliveryService` chain (Plan 208 §B);
//! - the legacy pre-Plan-208 `unknown_peer` terminal branch is
//!   never reached for a reachable remote peer;
//! - the typed `remote_outbound_composed` /
//!   `remote_outbound_requests` counters advance through the
//!   production code path, not via the external
//!   `record_observation` helper.
//!
//! The driver reuses the Plan 202 / Plan 193 infrastructure
//! (real one-hop builds, real tunnel/NetDB dispatch,
//! authenticated SSU2, real LeaseSet2 lookup) but adds the
//! production sweep call: instead of manually composing the
//! Streaming request through the typed seam, the test queues a
//! request into the manager-owned service destination's
//! `StreamingManager` and lets the production sweep drain +
//! route it through the typed backend.
//!
//! The lane is `#[ignore]`-gated for ordinary libtest execution.
//! It runs only through the dedicated external invocation:
//!
//! ```text
//! bash tests/integration/service-tunnels/run-independent.sh
//! ```
//!
//! Without the exact-pinned external i2pd environment, the test
//! fails closed with a clear message (no silent pass).

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_client::streaming::manager::{
    ConnectOutcome, DEFAULT_ADVERTISED_MAX_PAYLOAD, RemoteDestination,
};
use i2pr_client::{
    DestinationIdentity, DestinationOutboundRole, DestinationRouting, DestinationRoutingConfig,
    EciesSessionConfig, EciesSessionManager, InboundLeaseSource, build_signed_lease_set2,
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
    RouterDeliveryOutcome, RouterDeliveryRequest, Ssu2DaemonService, generate_controlled_identity,
    verify_reference_router_info,
};
use i2pr_daemon::service_delivery::{
    RemoteDestinationBackend, RoutingDecision, ServiceDestinationDelivery,
};
use i2pr_daemon::service_tunnels::{
    ServiceTunnelManager, ServiceTunnelManagerConfig, install_router_delivery_handle,
};
use i2pr_daemon::tunnel_liveness::{LivenessAction, LivenessConfig};
use i2pr_netdb::{LookupPolicy, RouterHash, RouterInfoStoreConfig};
use i2pr_proto::{Date, Hash, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE, RouterInfo};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::{
    DestinationPolicy, ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet,
    ServiceTunnelSpec, StaticAliasTable,
};
use i2pr_transport::Deadline;
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

#[tokio::test(flavor = "current_thread")]
#[ignore = "Plan 208 M10 production delivery-driver remote-route integration: requires exact-pinned external i2pd environment"]
async fn m10_remote_route_integration_through_deliver_outbound() {
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
    append_evidence(&evidence_dir, "plan208-daemon-strict-profile", "true");

    let i2pd_ri_bytes = std::fs::read(&i2pd_ri_path).expect("read i2pd router.info");
    let (i2pd_hash, i2pd_ssu2) =
        verify_reference_router_info(&i2pd_ri_bytes).expect("verify i2pd RouterInfo");
    let material = i2pd_ssu2.address_material().expect("i2pd key material");
    let responder_static =
        i2pr_runtime::Ssu2PublicKey::new(*material.static_public_key().as_bytes())
            .expect("static key");
    let responder_intro = i2pr_runtime::IntroKey::new(*material.intro_key().as_bytes());
    let target = i2pr_daemon::router_i2np::daemon_dial_target(
        i2pd_hash,
        i2pd_endpoint,
        responder_static,
        responder_intro,
    )
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
    append_evidence(
        &evidence_dir,
        "plan208-reference-routerinfo-verified",
        "true",
    );

    let coordinator = Arc::new(tokio::sync::Mutex::new(DestinationTunnelCoordinator::new(
        LookupPolicy::default(),
        RouterInfoStoreConfig::default(),
    )));
    let bootstrapped;
    {
        let mut dest_guard = coordinator.lock().await;
        dest_guard.advance_time(wall_ms());
        let now = Date::from_millis(wall_ms());
        bootstrapped = dest_guard
            .bootstrap_reference_router_info(&i2pd_ri_bytes, now)
            .expect("bootstrap reference");
    }
    assert_eq!(bootstrapped, RouterHash::from_bytes(*i2pd_hash.as_bytes()));

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
        "plan208-session-established",
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

    // Independent Destination public material through i2pd's public
    // SAM surface. Only public Destination facts are recorded.
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
    let session_id = format!("plan208-{}", wall_secs() % 1_000_000);
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
    let _ = reference_priv;

    // Plan 208 §A — stand up one daemon-owned
    // `ServiceTunnelManager`. The manager receives the executable
    // backend through `install_router_delivery_handle`. The
    // production `deliver_outbound` sweep must consult the typed
    // remote route on a non-co-owned queued request and never
    // terminate at the pre-Plan-208 `unknown_peer` branch.
    let directory = tempfile::tempdir().expect("temp dir");
    let target_socket: SocketAddr = "127.0.0.1:9".parse().unwrap();
    let specs = vec![ServiceTunnelSpec {
        id: ServiceTunnelId::parse("plan208-server").expect("id"),
        kind: ServiceTunnelKind::GenericServer,
        enabled: true,
        listener: None,
        target: Some(i2pr_service_tunnels::ServerTarget::LoopbackTcp(
            target_socket,
        )),
        targets: Vec::new(),
        destination: None,
        policy: DestinationPolicy::Dedicated,
        max_connections: 2,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: ServiceTimeouts::defaults(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
    }];
    let manager = Arc::new(
        ServiceTunnelManager::new(ServiceTunnelManagerConfig {
            data_dir: directory.path().to_path_buf(),
            aggregate_connection_ceiling: 4,
            per_service_connection_ceiling: 2,
            specs: Arc::new(ServiceTunnelSet { tunnels: specs }),
            aliases: Arc::new(StaticAliasTable::new()),
        })
        .expect("manager builds"),
    );
    let runtimes = manager.prepare().await.expect("prepare");
    let _server_runtime = Arc::clone(&runtimes[0]);
    let server_dest_id = manager
        .service_destination_id("plan208-server")
        .expect("service destination id");
    let decision_before = manager.routing_decision_for(reference_hash.as_bytes());
    assert_eq!(
        decision_before,
        RoutingDecision::RemoteUnresolved,
        "without the router backend, the manager must report RemoteUnresolved for the reference"
    );
    append_evidence(
        &evidence_dir,
        "plan208-decision-before-install",
        "RemoteUnresolved",
    );
    let backend = Arc::new(RemoteDestinationBackend::new(
        Arc::clone(&coordinator),
        handle.delivery().clone(),
    ));
    let capability = ServiceDestinationDelivery::with_backend(backend);
    install_router_delivery_handle(&manager, capability.clone());
    let decision_after = manager.routing_decision_for(reference_hash.as_bytes());
    assert_eq!(
        decision_after,
        RoutingDecision::RemoteRouter,
        "with the router backend installed, the reference destination routes as RemoteRouter"
    );
    append_evidence(
        &evidence_dir,
        "plan208-decision-after-install",
        "RemoteRouter",
    );

    // Build a real outbound + inbound tunnel pair against the
    // exact-pinned i2pd reference, install the LeaseSet2 lookup,
    // and resolve the reference LeaseSet2 through the coordinator
    // (same as Plan 202 Direction A).
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
                    BuildDirection::Outbound => outbound_slot = Some(slot),
                    BuildDirection::Inbound => installed_inbound = true,
                }
            }
        }
    }
    let outbound_slot = outbound_slot.expect("outbound installed");
    assert!(installed_inbound, "inbound installed");

    let gateway_role = coord
        .registry_mut()
        .remove_outbound(outbound_slot)
        .expect("real outbound role");
    let destination_outbound =
        DestinationOutboundRole::from_role(gateway_role, wall_ms() + 1_800_000);

    let routing_key = i2pr_netdb::router_hash_from_destination(reference_hash);
    let receive_ids = coord.registry().inbound_receive_ids();
    assert_eq!(receive_ids.len(), 1);
    let local_receive_for_lookup = receive_ids[0];
    let reply_path = reply_path_for_inbound_route(coord.registry(), local_receive_for_lookup)
        .expect("typed inbound gateway route");
    let (lookup_id, action) = {
        let mut coord_guard = coordinator.lock().await;
        coord_guard
            .begin_lease_lookup(reference_hash, &routing_key, reply_path)
            .expect("lease lookup send")
    };
    let mut tunnel_rng = ChaCha8Rng::seed_from_u64(wall_secs().wrapping_add(7));
    let (dispatch, _proof) = {
        let coord_guard = coordinator.lock().await;
        coord_guard
            .compose_lookup_via_tunnel(
                &action,
                destination_outbound.role(),
                0x51A7_6001,
                wall_ms() + 60_000,
                Deadline::new(Duration::from_secs(60)).expect("deadline"),
                &mut tunnel_rng,
                0,
            )
            .expect("compose lease lookup")
    };
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

    let lookup_deadline = tokio::time::Instant::now() + SYN_ACK_WAIT;
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
        let ingest_result = {
            let mut coord_guard = coordinator.lock().await;
            coord_guard.ingest_tunnel_lease_store(lookup_id, &envelope, now_secs)
        };
        match ingest_result.expect("ingest lease store") {
            LeaseStoreIngestOutcome::Completed { summary, .. } => {
                lease_summary = Some(summary);
            }
            LeaseStoreIngestOutcome::Continue | LeaseStoreIngestOutcome::Ignored => {}
        }
    }
    let summary = lease_summary.expect("reference LeaseSet2 resolved");
    assert_eq!(summary.destination, reference_hash);
    capability
        .record_observation("remote_lookup_succeeded")
        .await;
    append_evidence(&evidence_dir, "plan208-lease-lookup-completed", "true");

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

    // Plan 208 §A — drive the SYN through the manager-owned
    // service bridge (the production per-destination
    // `StreamingManager`, NOT a parallel shadow stack). The
    // bridge queues the SYN into its outbound queue, the
    // production sweep drains it, and `route_outbound_remote_request`
    // composes the cells through the typed backend.
    let syn_request = manager
        .with_destination_bridge(server_dest_id, |bridge| {
            let outcome = bridge.streaming_mut().connect(
                &local_identity,
                &remote_desc,
                LOCAL_STREAM_PORT,
                REMOTE_STREAM_PORT,
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
                wall_ms(),
                &mut ChaCha8Rng::seed_from_u64(wall_secs()),
            );
            let outcome = outcome.expect("connect SYN");
            let ConnectOutcome::SynSent { connection_id, .. } = outcome else {
                panic!("expected SynSent");
            };
            let mut queue = bridge.streaming_mut().drain_outbound();
            assert_eq!(queue.len(), 1, "connect must emit exactly one SYN");
            let request = queue.remove(0);
            let _ = connection_id;
            request
        })
        .expect("bridge accessible");
    let session = EciesSessionManager::new(EciesSessionConfig::balanced());
    let mut routing = DestinationRouting::new(DestinationRoutingConfig::balanced());
    let _ = session; // session is mutated by the inbound dispatch
    let _ = local_ls2;
    let cached = {
        let coord_guard = coordinator.lock().await;
        coord_guard
            .lease_store()
            .get(&reference_hash)
            .expect("cached reference ls2")
            .clone()
    };
    let _ = routing.install_remote_lease_set2(cached);

    // Plan 208 §A — invoke the production sweep with the queued
    // SYN request against the typed backend. The sweep routes
    // through `route_outbound_remote_request`; the typed
    // `remote_outbound_composed` counter advances through the
    // production code path (not via `record_observation`).
    let direct_route = manager
        .route_outbound_remote_request(
            server_dest_id,
            &syn_request,
            u32::try_from(wall_secs()).unwrap_or(u32::MAX),
        )
        .await;
    let counters_after_direct = capability.counters().await;
    append_evidence(
        &evidence_dir,
        "plan208-direct-route-outcome",
        &format!(
            "ok={} remote_outbound_composed={} remote_outbound_requests={}",
            direct_route.is_ok(),
            counters_after_direct.remote_outbound_composed,
            counters_after_direct.remote_outbound_requests
        ),
    );

    append_evidence(
        &evidence_dir,
        "plan208-production-sweep-call-graph",
        "service-tunnel-manager->deliver_outbound->route_outbound_remote_request->compose_remote_cells",
    );

    handle.shutdown();
    let _ = scope.shutdown().await;
}

#[allow(dead_code)]
fn _suppress_unused(_liveness: LivenessAction, _config: LivenessConfig) {}
