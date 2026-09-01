//! Plan 147 SAM 3.1 dedicated raw STREAM driver end-to-end test.
//!
//! This is the canonical Plan 147 evidence for the dedicated raw
//! TCP↔Streaming product path. Two SAM destinations wire up real
//! bridges in the daemon's `SamDestinations` registry; the test
//!
//! 1. binds two TCP clients to the SAM listener,
//! 2. issues HELLO + SESSION CREATE on both,
//! 3. issues STREAM CONNECT on side A and STREAM ACCEPT on side B,
//! 4. lets the per-destination runtime drivers drive the SYN/SYN
//!    response handshake through the Plan 129 local seam,
//! 5. once both sides report `STREAM STATUS RESULT=OK`, exchanges
//!    application bytes through the dedicated raw driver,
//! 6. verifies byte-for-byte equality across the bridge.
//!
//! The test never invokes `record_captured`, `adapter_send`, or
//! `CapturedOutbound`; the Plan 138 seams are removed from
//! acceptance. The runtime driver is the production driver spawned
//! via `SamServiceState::spawn_destination_driver`, not a custom
//! in-process loop.

#![allow(clippy::too_many_lines)]

use std::net::SocketAddr;
use std::sync::Arc;

use i2pr_api::sam::limits::SamLimits;
use i2pr_client::DestinationId;
use i2pr_daemon::config::SamConfig;
use i2pr_daemon::sam::{
    InboundTunnelBuildError, InboundTunnelFactory, SamServiceState, build_sam_destination_bridge,
};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_tunnel::{
    EstablishedHop, EstablishedNextHop, EstablishedRole, EstablishedTunnel, LayerKeys,
    TunnelDirection, TunnelId, TunnelPeer,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn sam_config() -> SamConfig {
    SamConfig {
        enabled: true,
        bind_address: "127.0.0.1".parse().unwrap(),
        port: 0,
        limits: SamLimits::loopback_test_profile(),
    }
}

fn child_scope(parent: &CancellationToken) -> ChildScope {
    ChildScope::for_test(parent, ChildFailurePolicy::FailParent)
}

async fn start_listener(
    config: SamConfig,
) -> (
    Arc<SamServiceState>,
    SocketAddr,
    ChildScope,
    CancellationToken,
) {
    let state = Arc::new(SamServiceState::new(config.clone()).expect("state"));
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

async fn read_one_line(stream: &mut TcpStream) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 256];
    loop {
        match stream.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.last() == Some(&b'\n') {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf)
        .trim_end_matches(['\r', '\n'])
        .to_owned()
}

async fn write_all(stream: &mut TcpStream, bytes: &[u8]) {
    stream.write_all(bytes).await.expect("write_all");
    stream.flush().await.expect("flush");
}

async fn hello_3_1(stream: &mut TcpStream) {
    write_all(stream, b"HELLO VERSION MIN=3.1 MAX=3.1\n").await;
    let reply = read_one_line(stream).await;
    assert!(
        reply.starts_with("HELLO REPLY RESULT=OK VERSION=3.1"),
        "expected HELLO OK, got {reply:?}"
    );
}

fn hop_router_hash(seed: u64, index: u8) -> i2pr_proto::Hash {
    let mut bytes = [0_u8; 32];
    for (offset, byte) in bytes.iter_mut().enumerate() {
        *byte = index.wrapping_add(offset as u8).wrapping_add(seed as u8);
    }
    i2pr_proto::Hash::from_bytes(bytes)
}

fn destination_identity(seed: u64) -> i2pr_client::DestinationIdentity {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    i2pr_client::DestinationIdentity::generate(&mut rng).expect("identity")
}

fn outbound_tunnel_direct(seed: u64) -> EstablishedTunnel {
    let hops = vec![
        EstablishedHop::with_next(
            TunnelPeer::from_hash(hop_router_hash(seed, 1)),
            EstablishedRole::Participant,
            TunnelId::new(0x0100_0000_u32.wrapping_add(seed as u32)).expect("id"),
            LayerKeys::new(
                [seed as u8; 32],
                [seed.wrapping_add(1) as u8; 32],
                [seed.wrapping_add(2) as u8; 32],
            ),
            EstablishedNextHop::new(
                TunnelPeer::from_hash(hop_router_hash(seed, 2)),
                TunnelId::new(0x0100_0001_u32.wrapping_add(seed as u32)).expect("id"),
            ),
        ),
        EstablishedHop::terminal(
            TunnelPeer::from_hash(hop_router_hash(seed, 2)),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(0x0100_0001_u32.wrapping_add(seed as u32)).expect("id"),
            LayerKeys::new(
                [seed.wrapping_add(1) as u8; 32],
                [seed.wrapping_add(2) as u8; 32],
                [seed.wrapping_add(3) as u8; 32],
            ),
        ),
    ];
    EstablishedTunnel::new(
        TunnelDirection::Outbound,
        TunnelId::new(0x0200_0000_u32.wrapping_add(seed as u32)).expect("id"),
        hops,
        0,
        None,
        None,
    )
    .expect("outbound established")
}

fn inbound_tunnel_direct(seed: u64) -> EstablishedTunnel {
    let ibgw_tunnel = TunnelId::new(0x0400_0000_u32.wrapping_add(seed as u32)).expect("id");
    let local_receive = TunnelId::new(0x0300_0000_u32.wrapping_add(seed as u32)).expect("id");
    let hops = vec![
        EstablishedHop::with_next(
            TunnelPeer::from_hash(hop_router_hash(seed, 1)),
            EstablishedRole::InboundGateway,
            ibgw_tunnel,
            LayerKeys::new(
                [seed.wrapping_add(4) as u8; 32],
                [seed.wrapping_add(5) as u8; 32],
                [seed.wrapping_add(6) as u8; 32],
            ),
            EstablishedNextHop::new(
                TunnelPeer::from_hash(hop_router_hash(seed, 2)),
                TunnelId::new(0x0400_0001_u32.wrapping_add(seed as u32)).expect("id"),
            ),
        ),
        EstablishedHop::with_next(
            TunnelPeer::from_hash(hop_router_hash(seed, 2)),
            EstablishedRole::Participant,
            TunnelId::new(0x0400_0001_u32.wrapping_add(seed as u32)).expect("id"),
            LayerKeys::new(
                [seed.wrapping_add(5) as u8; 32],
                [seed.wrapping_add(6) as u8; 32],
                [seed.wrapping_add(7) as u8; 32],
            ),
            EstablishedNextHop::new(
                TunnelPeer::from_hash(hop_router_hash(seed, 3)),
                local_receive,
            ),
        ),
    ];
    EstablishedTunnel::new(
        TunnelDirection::Inbound,
        TunnelId::new(0x0500_0000_u32.wrapping_add(seed as u32)).expect("id"),
        hops,
        0,
        Some((TunnelPeer::from_hash(hop_router_hash(seed, 1)), ibgw_tunnel)),
        Some(local_receive),
    )
    .expect("inbound established")
}

/// Deterministic inbound-tunnel factory used by the runtime driver.
/// Plan 129's local seam consumes the inbound tunnel once per delivery
/// so the factory rebuilds a fresh copy on each call.
#[derive(Clone)]
struct DeterministicInboundFactory {
    seed: u64,
}

impl InboundTunnelFactory for DeterministicInboundFactory {
    fn build_inbound_tunnel(&self) -> Result<EstablishedTunnel, InboundTunnelBuildError> {
        Ok(inbound_tunnel_direct(self.seed))
    }
}

fn install_peer_lease_set2(state: &SamServiceState, owner: DestinationId, peer: DestinationId) {
    let destinations_arc = state.sam_destinations();
    let destinations = destinations_arc.lock().expect("poisoned");
    let peer_lease_set2 = destinations
        .get(peer)
        .expect("peer bridge")
        .with(|bridge| bridge.lease_set2().clone());
    let peer_netdb_key = destinations
        .get(peer)
        .expect("peer bridge")
        .with(|bridge| bridge.identity_netdb_key());
    let validated = i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(
        peer_lease_set2,
        Some(peer_netdb_key),
        i2pr_netdb::LeaseSet2ValidationContext::new(NOW_SECONDS),
    )
    .expect("validated ls2");
    destinations
        .get(owner)
        .expect("owner bridge")
        .with(|bridge| {
            bridge
                .routing_mut()
                .install_remote_lease_set2(validated)
                .expect("install peer ls2");
        });
}

fn install_bridge_with_factory(
    state: &SamServiceState,
    destination_id: DestinationId,
    identity: i2pr_client::DestinationIdentity,
    seed: u64,
) {
    use i2pr_client::build_signed_lease_set2;
    let outbound_tunnel = outbound_tunnel_direct(seed);
    let role = i2pr_client::DestinationOutboundRole::new(outbound_tunnel, NOW_MS + 60_000);
    let lease_set2 = {
        let inbound_for_pool = inbound_tunnel_direct(seed);
        let mut pool =
            i2pr_client::DestinationTunnelPool::new(i2pr_client::DestinationConfig::balanced())
                .expect("pool");
        let inbound_mat = inbound_for_pool.into_extracted();
        pool.register_inbound(inbound_mat, NOW_SECONDS as u64)
            .expect("inbound");
        let sources = pool.inbound_lease_sources(NOW_SECONDS as u64);
        build_signed_lease_set2(&identity, &sources, NOW_SECONDS).expect("signed ls2")
    };
    let bridge =
        build_sam_destination_bridge(identity, lease_set2, role, NOW_SECONDS).expect("bridge");
    let destinations_arc = state.sam_destinations();
    let handle = {
        let mut destinations = destinations_arc.lock().expect("sam destinations poisoned");
        destinations.install(destination_id, bridge)
    };
    let _ = handle.install_inbound_tunnel_factory(Arc::new(DeterministicInboundFactory { seed }));
}

const NOW_MS: u64 = 64_000;
const NOW_SECONDS: u32 = 64;

fn _unused_marker() {}

#[tokio::test(flavor = "current_thread")]
async fn plan147_dedicated_raw_driver_exchanges_application_bytes() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;

    // ----- TCP clients: A = CONNECT, B = ACCEPT -----
    let mut client_a = TcpStream::connect(address).await.expect("connect a");
    let mut client_b = TcpStream::connect(address).await.expect("connect b");
    hello_3_1(&mut client_a).await;
    hello_3_1(&mut client_b).await;

    // SESSION CREATE STYLE=STREAM ID=alpha DESTINATION=<priv> imports
    // the supplied private destination. We pre-generate the
    // identity in the test (so we know the destination_id) and
    // import the priv via the SAM Base64 encoding. After SESSION
    // CREATE succeeds, we install the SAM bridge for the known
    // destination id.
    let identity_a = destination_identity(0xA1);
    let identity_b = destination_identity(0xB2);
    let destination_a = identity_a.id();
    let destination_b = identity_b.id();
    let priv_a =
        i2pr_api::sam::private_destination::SamPrivateDestination::from_identity(&identity_a)
            .expect("identity a round-trips")
            .encode_base64();
    let priv_b =
        i2pr_api::sam::private_destination::SamPrivateDestination::from_identity(&identity_b)
            .expect("identity b round-trips")
            .encode_base64();
    let pub_b =
        i2pr_api::sam::private_destination::SamPrivateDestination::from_identity(&identity_b)
            .expect("identity b round-trips")
            .encode_public_base64();

    write_all(
        &mut client_a,
        format!("SESSION CREATE STYLE=STREAM ID=alpha DESTINATION={priv_a}\n").as_bytes(),
    )
    .await;
    let reply_a = read_one_line(&mut client_a).await;
    eprintln!("reply_a={reply_a:?}");
    assert!(
        reply_a.contains("SESSION STATUS RESULT=OK"),
        "session alpha A failed: {reply_a:?}"
    );
    write_all(
        &mut client_b,
        format!("SESSION CREATE STYLE=STREAM ID=beta DESTINATION={priv_b}\n").as_bytes(),
    )
    .await;
    let reply_b = read_one_line(&mut client_b).await;
    eprintln!("reply_b={reply_b:?}");
    assert!(
        reply_b.contains("SESSION STATUS RESULT=OK"),
        "session beta B failed: {reply_b:?}"
    );

    // Plan 147 §8: install bridges for the imported destinations.
    install_bridge_with_factory(&state, destination_a, identity_a, 0xA1);
    install_bridge_with_factory(&state, destination_b, identity_b, 0xB2);

    // Plan 147: cross-install the validated LeaseSet2 records so
    // `compose_outbound_delivery` finds an `active_remotes` entry
    // for the peer; without it the outbound seam returns the
    // typed `LeaseSet2LookupPending` and the SYN never reaches the
    // peer's StreamingManager.
    install_peer_lease_set2(&state, destination_a, destination_b);
    install_peer_lease_set2(&state, destination_b, destination_a);

    // Plan 147 §8: spawn the per-destination runtime driver task.
    state
        .spawn_destination_driver(destination_a, &scope, parent.clone())
        .expect("spawn driver a");
    state
        .spawn_destination_driver(destination_b, &scope, parent.clone())
        .expect("spawn driver b");

    // Issue STREAM ACCEPT on B; issue STREAM CONNECT on A.
    // Both calls happen in parallel tasks so the runtime driver
    // can drive the handshake while both clients are parked on
    // their `read_one_line` futures.
    eprintln!("[test] sleeping 200ms before issuing commands");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    eprintln!("[test] issuing STREAM ACCEPT on B");
    let state_for_accept = Arc::clone(&state);
    let accept_task = tokio::spawn(async move {
        write_all(&mut client_b, b"STREAM ACCEPT ID=beta\n").await;
        let reply = read_one_line(&mut client_b).await;
        (client_b, reply)
    });
    eprintln!("[test] issuing STREAM CONNECT on A");
    let state_for_connect = Arc::clone(&state);
    let connect_task = tokio::spawn(async move {
        let cmd = format!("STREAM CONNECT ID=alpha DESTINATION={pub_b}\n");
        write_all(&mut client_a, cmd.as_bytes()).await;
        let reply = read_one_line(&mut client_a).await;
        (client_a, reply)
    });
    eprintln!("[test] joining");
    let (accept_result, connect_result) = tokio::join!(accept_task, connect_task);
    eprintln!("[test] joined");
    let (mut client_b, accept_reply) = accept_result.expect("accept task");
    let (mut client_a, connect_reply) = connect_result.expect("connect task");
    assert!(
        accept_reply.contains("STREAM STATUS RESULT=OK"),
        "ACCEPT did not return OK, got {accept_reply:?}"
    );
    eprintln!("connect_reply={connect_reply:?}");
    assert!(
        connect_reply.contains("STREAM STATUS RESULT=OK"),
        "CONNECT did not return OK, got {connect_reply:?}"
    );
    let _ = state_for_accept;
    let _ = state_for_connect;

    // ----- Drive raw byte exchange through the dedicated driver -----
    let payload_a: Vec<u8> = (0..1024_u32).map(|i| (i & 0xFF) as u8).collect();
    let payload_b: Vec<u8> = (0..2048_u32)
        .map(|i| ((i.wrapping_mul(7)) & 0xFF) as u8)
        .collect();
    let write_a = async {
        write_all(&mut client_a, &payload_a).await;
        let mut buf = Vec::with_capacity(payload_b.len());
        let mut chunk = [0_u8; 256];
        let mut received = 0_usize;
        while received < payload_b.len() {
            let n = client_a.read(&mut chunk).await.expect("client_a read");
            assert!(n > 0, "client_a EOF before receiving payload_b");
            buf.extend_from_slice(&chunk[..n]);
            received += n;
        }
        buf
    };
    let write_b = async {
        write_all(&mut client_b, &payload_b).await;
        let mut buf = Vec::with_capacity(payload_a.len());
        let mut chunk = [0_u8; 256];
        let mut received = 0_usize;
        while received < payload_a.len() {
            let n = client_b.read(&mut chunk).await.expect("client_b read");
            assert!(n > 0, "client_b EOF before receiving payload_a");
            buf.extend_from_slice(&chunk[..n]);
            received += n;
        }
        buf
    };
    let (received_a, received_b) = tokio::join!(write_a, write_b);
    assert_eq!(received_a, payload_b, "client_a did not receive payload_b");
    assert_eq!(received_b, payload_a, "client_b did not receive payload_a");

    drop(client_a);
    drop(client_b);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}
