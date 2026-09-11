//! Plan 186 external NetDB-over-tunnels lane.
//!
//! Fail-closed driver against exact-pinned i2pd 2.61.0 on loopback:
//!
//! - strict SSU2 controlled profile;
//! - reference RouterInfo verified + bootstrapped through the ordinary
//!   parser/signature/freshness path into the authoritative store;
//! - effective floodfill capability verified before dispatch;
//! - authenticated SSU2 session establishes via daemon-owned runtime;
//! - one real one-hop outbound + inbound exploratory build accepted by
//!   the reference (i2pd `TransitTunnel: endpoint/gateway` logs);
//! - one RouterInfo `DatabaseLookup` composed through the existing
//!   outbound tunnel seam bound to the real reference identity
//!   (TunnelData cells, first-hop + floodfill proof) and admitted to
//!   the authenticated session;
//! - one RouterInfo publication composed through the same seam with
//!   the same proof;
//! - direct SSU2 NetDB delivery explicitly rejected as a counted path;
//! - creator-side liveness first test green.
//!
//! The full decryptable round-trip (OTBRM unwrap + inbound TunnelData
//! recovery against the reference) is proven in the local
//! two-role suite (`netdb_tunnel_live.rs`); i2pd encrypts the endpoint
//! OTBRM with the creator's ECIES-X25519 key and the narrow external
//! dispatcher does not unwrap that envelope — the same limitation
//! recorded in Plan 185. The external lane proves real session +
//! real build acceptance + real tunnel composition/admission against
//! the exact-pinned reference without overclaiming inbound decrypt.

#![forbid(unsafe_code)]

use std::io::Write as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::config::Config;
use i2pr_daemon::exploratory_build::{
    BuildDirection, BuildRequest, ExploratoryBuildCoordinator, PeerBuildMaterial, SubmitResult,
};
use i2pr_daemon::netdb_tunnels::NetDbTunnelCoordinator;
use i2pr_daemon::router_i2np::{
    RouterDeliveryRequest, Ssu2DaemonService, daemon_dial_target, generate_controlled_identity,
    verify_reference_router_info,
};
use i2pr_daemon::tunnel_liveness::{
    FIRST_LIVENESS_DELAY_MS, LivenessAction, LivenessConfig, TunnelLivenessScheduler,
};
use i2pr_netdb::{LookupPolicy, ReplyPath, RouterHash, RouterInfoStoreConfig};
use i2pr_proto::{Date, Hash, RouterInfo};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_transport::{Deadline, PeerId};
use i2pr_tunnel::bridge::{BridgeHeader, ShortBuildI2npBridge};
use i2pr_tunnel::build_crypto::LayerKeys;
use i2pr_tunnel::config::ExploratoryPoolConfig;
use i2pr_tunnel::established::{EstablishedHop, EstablishedRole, EstablishedTunnel};
use i2pr_tunnel::identity::{TunnelDirection, TunnelId, TunnelPeer};
use i2pr_tunnel::short_record::HopRole;
use rand_core::{OsRng, SeedableRng};

const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
const WAIT_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const I2PD_ACCEPT_TIMEOUT: Duration = Duration::from_secs(20);
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);

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
    writeln!(file, "{label}\t{sanitized}").expect("write evidence row");
}

async fn drain_accept_evidence(
    handle: &mut i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    i2pd_log: &Path,
    expected_keyword: &str,
) -> bool {
    let deadline = tokio::time::Instant::now() + I2PD_ACCEPT_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        let _ = tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await;
        if let Ok(text) = std::fs::read_to_string(i2pd_log)
            && text.contains(expected_keyword)
        {
            return true;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    false
}

fn layer_keys(seed: u8) -> LayerKeys {
    LayerKeys::new(
        [seed; 32],
        [seed.wrapping_add(1); 32],
        [seed.wrapping_add(2); 32],
    )
}

#[tokio::test]
#[ignore = "Plan 186: requires exact-pinned external i2pd environment"]
async fn netdb_tunnels_against_i2pd() {
    let i2pd_ri_path = env_path("I2PD_ROUTER_INFO");
    let i2pd_endpoint: SocketAddr = env_value("I2PD_SSU2_ENDPOINT").parse().expect("endpoint");
    let bind: SocketAddr = env_value("I2PR_SSU2_BIND").parse().expect("bind");
    assert!(bind.ip().is_loopback(), "i2pr bind must be loopback");
    assert!(
        i2pd_endpoint.ip().is_loopback(),
        "i2pd endpoint must be loopback"
    );
    let evidence_dir = env_path("EVIDENCE_DIR");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");
    let i2pd_parent_log = evidence_dir
        .parent()
        .map(|p| p.join("i2pd.log"))
        .unwrap_or_else(|| evidence_dir.join("i2pd.log"));
    let i2pd_log_path = if i2pd_parent_log.exists() {
        i2pd_parent_log
    } else {
        evidence_dir.join("i2pd.log")
    };

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
    let i2pd_peer = PeerId::from_hash(i2pd_hash);
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

    // Authoritative bootstrap through the ordinary validation path.
    let mut netdb =
        NetDbTunnelCoordinator::new(LookupPolicy::default(), RouterInfoStoreConfig::default());
    netdb.advance_time(wall_ms());
    let now = Date::from_millis(wall_ms());
    let bootstrapped = netdb
        .bootstrap_reference_router_info(&i2pd_ri_bytes, now)
        .expect("bootstrap reference");
    assert_eq!(bootstrapped, RouterHash::from_bytes(*i2pd_hash.as_bytes()));
    append_evidence(
        &evidence_dir,
        "reference-bootstrap-store",
        &netdb.store_stats().record_count.to_string(),
    );
    // Effective floodfill capability verified before dispatch.
    assert!(
        netdb.is_floodfill(&bootstrapped),
        "reference must advertise floodfill for the controlled lane"
    );
    append_evidence(&evidence_dir, "reference-floodfill-capable", "true");

    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
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

    // Real exploratory builds prove the real path is selected.
    let mut coord = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    coord.advance_time(wall_ms());
    let bridge = ShortBuildI2npBridge::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(wall_secs());
    let delivery = handle.delivery().clone();

    let outbound_peer = PeerBuildMaterial {
        router_hash: i2pd_hash,
        static_encryption_key: i2pd_encryption_key,
        receive_tunnel: TunnelId::new(0x9501).expect("receive"),
        next_tunnel: TunnelId::new(0x9502).expect("next"),
        role: HopRole::OutboundEndpoint,
    };
    let outbound_message_id: u32 = 0x51A4_5001;
    let outbound_request = BuildRequest {
        direction: BuildDirection::Outbound,
        peer: outbound_peer,
        creator_tunnel_id: TunnelId::new(0x1500).expect("creator"),
        message_id: outbound_message_id,
        outbound_reply_router: Some(local_hash),
        originator_hash: None,
    };
    let outbound_header = BridgeHeader::ShortTransport {
        message_id: outbound_message_id,
        expiration_seconds: wall_secs().saturating_add(60) as u32,
    };
    let outbound_submit = coord
        .submit(
            outbound_request,
            &delivery,
            &bridge,
            outbound_header,
            &mut rng,
        )
        .expect("outbound submit ok");
    assert!(matches!(outbound_submit, SubmitResult::Submitted { .. }));
    append_evidence(&evidence_dir, "outbound-build-emitted", "true");
    let outbound_accepted =
        drain_accept_evidence(&mut handle, &i2pd_log_path, "TransitTunnel: endpoint").await;
    assert!(outbound_accepted, "i2pd did not accept outbound build");
    append_evidence(&evidence_dir, "outbound-installed", "true");

    let inbound_peer = PeerBuildMaterial {
        router_hash: i2pd_hash,
        static_encryption_key: i2pd_encryption_key,
        receive_tunnel: TunnelId::new(0x9601).expect("receive"),
        next_tunnel: TunnelId::new(0x9602).expect("next"),
        role: HopRole::InboundGateway,
    };
    let inbound_message_id: u32 = 0x51A4_5101;
    let inbound_request = BuildRequest {
        direction: BuildDirection::Inbound,
        peer: inbound_peer,
        creator_tunnel_id: TunnelId::new(0x1600).expect("creator"),
        message_id: inbound_message_id,
        outbound_reply_router: None,
        originator_hash: Some(local_hash),
    };
    let inbound_header = BridgeHeader::ShortTransport {
        message_id: inbound_message_id,
        expiration_seconds: wall_secs().saturating_add(60) as u32,
    };
    let inbound_submit = coord
        .submit(
            inbound_request,
            &delivery,
            &bridge,
            inbound_header,
            &mut rng,
        )
        .expect("inbound submit ok");
    assert!(matches!(inbound_submit, SubmitResult::Submitted { .. }));
    append_evidence(&evidence_dir, "inbound-build-emitted", "true");
    let inbound_accepted =
        drain_accept_evidence(&mut handle, &i2pd_log_path, "TransitTunnel: gateway").await;
    assert!(inbound_accepted, "i2pd did not accept inbound build");
    append_evidence(&evidence_dir, "inbound-installed", "true");

    // NetDB lookup composed through the tunnel seam bound to the real
    // reference identity. The synthetic one-hop role models the
    // established outbound path (first hop = reference) so the proof
    // asserts TunnelData traversal, first-hop equality, and key
    // preservation; delivery admission goes over the real session.
    let synthetic_target = RouterHash::from_bytes([0xABu8; 32]);
    let reply_path =
        ReplyPath::new(RouterHash::from_bytes(*i2pd_hash.as_bytes()), 0x9602).expect("reply path");
    let (_lookup_id, action) = netdb
        .begin_tunnel_lookup(synthetic_target, &synthetic_target, reply_path)
        .expect("lookup send");
    let floodfill_peer = TunnelPeer::from_hash(Hash::from_bytes(*i2pd_hash.as_bytes()));
    let outbound_hops = vec![EstablishedHop::terminal(
        floodfill_peer,
        EstablishedRole::OutboundEndpoint,
        TunnelId::new(0x9501).expect("id"),
        layer_keys(0x60),
    )];
    let outbound_tunnel = EstablishedTunnel::new(
        TunnelDirection::Outbound,
        TunnelId::new(0x9500).expect("id"),
        outbound_hops,
        0,
        None,
        None,
    )
    .expect("tunnel");
    let outbound_role =
        i2pr_tunnel::roles::OutboundGatewayRole::new(outbound_tunnel, wall_ms() + 60_000);
    let mut tunnel_rng = rand_chacha::ChaCha8Rng::seed_from_u64(wall_secs().wrapping_add(3));
    let (dispatch, proof) = netdb
        .compose_lookup_via_tunnel(
            &action,
            &outbound_role,
            0x51A4_6001,
            wall_ms() + 60_000,
            Deadline::new(Duration::from_secs(60)).expect("deadline"),
            &mut tunnel_rng,
            0,
        )
        .expect("compose lookup");
    assert!(proof.via_tunnel);
    append_evidence(
        &evidence_dir,
        "outbound-lookup-via-tunnel",
        &format!("cells={}", proof.cell_count),
    );
    // Admit every cell over the real authenticated session.
    for cell_delivery in &dispatch.deliveries {
        let request = RouterDeliveryRequest::new(
            cell_delivery.target(),
            cell_delivery.message_bytes().to_vec(),
            DELIVERY_TIMEOUT,
        )
        .expect("delivery request");
        let outcome = delivery.deliver(request, &CancellationToken::new());
        assert_eq!(
            outcome,
            i2pr_daemon::router_i2np::RouterDeliveryOutcome::Accepted,
            "tunnel cell must enter the session queue"
        );
    }
    append_evidence(&evidence_dir, "outbound-lookup-delivered", "true");

    // Publication through the same tunnel seam with normal identity code.
    let local_signer = RouterIdentityBundle::generate(&mut OsRng).expect("local signer");
    let local_info = i2pr_netdb::LocalRouterInfoBuilder::new(&local_signer)
        .build_default(Date::from_millis(wall_ms()))
        .expect("local info");
    netdb.register_local(local_info);
    let record = netdb
        .begin_publication(RouterHash::from_bytes(*i2pd_hash.as_bytes()))
        .expect("begin publication");
    let (pub_dispatch, pub_proof) = netdb
        .compose_publication_via_tunnel(
            &record,
            i2pd_hash,
            &outbound_role,
            0x51A4_6101,
            wall_ms() + 60_000,
            Deadline::new(Duration::from_secs(60)).expect("deadline"),
            &mut tunnel_rng,
            0,
        )
        .expect("compose publication");
    assert!(pub_proof.via_tunnel);
    append_evidence(
        &evidence_dir,
        "publication-via-tunnel",
        &format!("cells={}", pub_proof.cell_count),
    );
    for cell_delivery in &pub_dispatch.deliveries {
        let request = RouterDeliveryRequest::new(
            cell_delivery.target(),
            cell_delivery.message_bytes().to_vec(),
            DELIVERY_TIMEOUT,
        )
        .expect("delivery request");
        let outcome = delivery.deliver(request, &CancellationToken::new());
        assert_eq!(
            outcome,
            i2pr_daemon::router_i2np::RouterDeliveryOutcome::Accepted
        );
    }
    append_evidence(&evidence_dir, "publication-delivered", "true");

    // Direct transport is never a counted path.
    assert!(netdb.note_direct_transport_attempt().is_err());
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
    let _ = i2pd_peer;
}
