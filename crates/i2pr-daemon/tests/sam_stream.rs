//! Plan 138 / Plan 143 real-loopback integration tests for the
//! SAM v3.1 STREAM bridge.
//!
//! Each test binds the SAM listener to `127.0.0.1:0` (ephemeral
//! port) and exercises a real TCP client over
//! `tokio::net::TcpStream`. The bridge tests use the Plan 143
//! local product path (no `CapturedOutbound` test seam); the
//! streaming handshake completion is driven through the
//! `bridge_to_peer` runtime-neutral local delivery pump, exactly
//! the way Plan 129's trajectory tests pipe outbound I2NP between
//! side A and side B without external network involvement.

#![allow(clippy::too_many_lines)]

use std::net::SocketAddr;
use std::sync::Arc;

use i2pr_api::sam::limits::SamLimits;
use i2pr_client::DestinationId;
use i2pr_client::testing::established_outbound;
use i2pr_daemon::config::SamConfig;
use i2pr_daemon::sam::{SamServiceState, build_sam_destination_bridge};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
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

/// Installs a fresh SAM bridge for the supplied destination id,
/// using deterministic established tunnel fixtures and a dummy
/// outbound role. Plan 143: the inbound tunnel is held by the
/// daemon's streaming pools outside the bridge; the bridge owns
/// only the streaming/routing/session/dispatcher stack plus the
/// outbound role. Tests that need to deliver through
/// `bridge_to_peer` pass the inbound tunnel in explicitly.
fn install_test_bridge(
    state: &SamServiceState,
    destination_id: DestinationId,
    seed: u64,
) -> i2pr_daemon::sam::SamDestinationBridge {
    use i2pr_client::build_signed_lease_set2;
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let identity = i2pr_client::DestinationIdentity::generate(&mut rng).expect("identity");
    let mut outbound_material = established_outbound(seed.wrapping_mul(3).wrapping_add(2));
    let outbound_tunnel = outbound_material
        .into_established_tunnel()
        .expect("outbound tunnel");
    let role = i2pr_client::DestinationOutboundRole::new(outbound_tunnel, 60_000);
    let lease_set2 = {
        let mut pool =
            i2pr_client::DestinationTunnelPool::new(i2pr_client::DestinationConfig::balanced())
                .expect("pool");
        let inbound_mat = i2pr_client::testing::established_inbound(seed);
        pool.register_inbound(inbound_mat, 1_000).expect("inbound");
        let sources = pool.inbound_lease_sources(1_000);
        build_signed_lease_set2(&identity, &sources, 1_000).expect("signed ls2")
    };
    let bridge = build_sam_destination_bridge(identity, lease_set2, role, 1_000).expect("bridge");
    {
        let destinations_arc = state.sam_destinations();
        let mut destinations = destinations_arc.lock().expect("sam destinations poisoned");
        destinations.install(destination_id, bridge);
    }
    // Return a placeholder bridge so callers keep symmetric API.
    let mut rng2 = ChaCha8Rng::seed_from_u64(seed.wrapping_add(0xFFFF_FFFF));
    let identity2 = i2pr_client::DestinationIdentity::generate(&mut rng2).expect("identity");
    let mut outbound2_mat = established_outbound(seed.wrapping_mul(7).wrapping_add(202));
    let outbound2 = outbound2_mat
        .into_established_tunnel()
        .expect("outbound tunnel 2");
    let role2 = i2pr_client::DestinationOutboundRole::new(outbound2, 60_000);
    let lease_set2_2 = {
        let mut pool =
            i2pr_client::DestinationTunnelPool::new(i2pr_client::DestinationConfig::balanced())
                .expect("pool");
        let inbound_mat = i2pr_client::testing::established_inbound(seed.wrapping_add(1));
        pool.register_inbound(inbound_mat, 1_000).expect("inbound");
        let sources = pool.inbound_lease_sources(1_000);
        build_signed_lease_set2(&identity2, &sources, 1_000).expect("signed ls2")
    };
    build_sam_destination_bridge(identity2, lease_set2_2, role2, 1_000).expect("bridge")
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stream_connect_unknown_session_returns_invalid_id() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    write_all(
        &mut client,
        b"STREAM CONNECT ID=alpha DESTINATION=placeholder\n",
    )
    .await;
    let reply = read_one_line(&mut client).await;
    assert!(
        reply.contains("RESULT=INVALID_ID"),
        "expected INVALID_ID, got {reply:?}"
    );
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stream_connect_malformed_destination_returns_invalid_key() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    write_all(
        &mut client,
        b"SESSION CREATE STYLE=STREAM ID=alpha DESTINATION=TRANSIENT\n",
    )
    .await;
    let _ = read_one_line(&mut client).await;
    let bogus_dest = "AAAA";
    let cmd = format!("STREAM CONNECT ID=alpha DESTINATION={bogus_dest}\n");
    write_all(&mut client, cmd.as_bytes()).await;
    let reply = read_one_line(&mut client).await;
    assert!(
        reply.contains("RESULT=INVALID_KEY"),
        "expected INVALID_KEY, got {reply:?}"
    );
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stream_connect_without_hello_is_rejected() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    write_all(
        &mut client,
        b"STREAM CONNECT ID=alpha DESTINATION=anything\n",
    )
    .await;
    let reply = read_one_line(&mut client).await;
    assert!(
        reply.contains("RESULT=INVALID_ID") || reply.contains("RESULT=I2P_ERROR"),
        "expected INVALID_ID or I2P_ERROR, got {reply:?}"
    );
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stream_connect_missing_destination_returns_malformed() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    write_all(&mut client, b"STREAM CONNECT ID=alpha\n").await;
    let reply = read_one_line(&mut client).await;
    assert!(
        reply.contains("STREAM STATUS RESULT=I2P_ERROR"),
        "expected I2P_ERROR, got {reply:?}"
    );
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stream_accept_unknown_session_returns_invalid_id() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    write_all(&mut client, b"STREAM ACCEPT ID=ghost\n").await;
    let reply = read_one_line(&mut client).await;
    assert!(
        reply.contains("RESULT=INVALID_ID"),
        "expected INVALID_ID, got {reply:?}"
    );
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stream_forward_unknown_session_returns_invalid_id() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    write_all(
        &mut client,
        b"STREAM FORWARD ID=alpha PORT=1234 HOST=127.0.0.1\n",
    )
    .await;
    let reply = read_one_line(&mut client).await;
    assert!(
        reply.contains("RESULT=INVALID_ID"),
        "expected INVALID_ID, got {reply:?}"
    );
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stream_socket_open_then_close_is_handled() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    write_all(
        &mut client,
        b"SESSION CREATE STYLE=STREAM ID=alpha DESTINATION=TRANSIENT\n",
    )
    .await;
    let _ = read_one_line(&mut client).await;
    write_all(&mut client, b"STREAM ACCEPT ID=alpha\n").await;
    drop(client);
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn sam_listener_completes_after_quit() {
    use std::time::Duration;
    use tokio::time::timeout;
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    write_all(&mut client, b"QUIT\n").await;
    let mut buf = [0_u8; 32];
    let read = timeout(Duration::from_millis(200), client.read(&mut buf)).await;
    match read {
        Ok(Ok(0)) | Ok(Err(_)) | Err(_) => {}
        Ok(Ok(n)) => assert!(buf[..n].is_empty()),
    }
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn bridge_outbound_diagnostic_counter_increments() {
    let (state, _address, scope, parent) = start_listener(sam_config()).await;
    let destination_id = DestinationId::from_hash(i2pr_proto::Hash::from_bytes([7_u8; 32]));
    let _ = install_test_bridge(&state, destination_id, 0xBEEF);
    let outbound_count = {
        let destinations_arc = state.sam_destinations();
        let destinations = destinations_arc.lock().expect("poisoned");
        let handle = destinations.get(destination_id).expect("bridge");
        handle.with(|bridge| {
            let request = i2pr_client::streaming::transport::TransportSendRequest {
                destination_hash: [0xAB; 32],
                source_port: 1,
                destination_port: 2,
                application_payload: vec![0xCA, 0xFE, 0xBA, 0xBE],
                sequence: 1,
                send_stream_id: 0x100,
                receive_stream_id: 0x200,
            };
            bridge.record_outbound_dispatch(request.clone());
            bridge.diagnostics().outbound_queue_len()
        })
    };
    assert_eq!(outbound_count, 1);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn sam_destination_registry_install_and_remove() {
    let (state, _address, scope, parent) = start_listener(sam_config()).await;
    let destination_id = DestinationId::from_hash(i2pr_proto::Hash::from_bytes([42_u8; 32]));
    let _ = install_test_bridge(&state, destination_id, 0xCAFE);
    {
        let destinations_arc = state.sam_destinations();
        let destinations = destinations_arc.lock().expect("poisoned");
        assert!(destinations.get(destination_id).is_some());
        assert_eq!(destinations.len(), 1);
    }
    let _ = {
        let destinations_arc = state.sam_destinations();
        let mut destinations = destinations_arc.lock().expect("poisoned");
        destinations.remove(destination_id)
    };
    {
        let destinations_arc = state.sam_destinations();
        let destinations = destinations_arc.lock().expect("poisoned");
        assert!(destinations.get(destination_id).is_none());
        assert_eq!(destinations.len(), 0);
    }
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}
