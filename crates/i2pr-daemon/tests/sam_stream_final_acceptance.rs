//! Plan 151 §§6–9 + §13 — SAM 3.1 final acceptance black-box suite.
//!
//! This is the focused final-acceptance evidence for sibling-stream
//! isolation, slow-peer boundedness, the deterministic fault matrix,
//! CLOSE/RESET lifecycle, and default-log hygiene. Like the canonical
//! Plan 149 suite, every test drives behavior through SAM TCP and raw
//! bytes only after listener startup:
//!
//! - helpers open `tokio::net::TcpStream`s and speak SAM commands;
//! - deterministic faults are installed through
//!   [`i2pr_daemon::sam::SamServiceState::install_test_fault_profile`]
//!   **before** the listener starts serving (pre-start configuration
//!   below the SAM socket boundary, no production `i2pr-testkit`
//!   dependency);
//! - after startup, tests only read non-secret counters/snapshots
//!   (`session_registry`, `stream_registry`, `delivery_counters`,
//!   `fault_counters`, Streaming queue gauges) to prove boundedness
//!   and resource release.
//!
//! The suite never calls private bridge, LeaseSet2, tunnel-factory,
//! driver, delivery, or byte-moving setup APIs after startup.
//!
//! Row mapping (see `plans/151-status.md` for the ledger):
//!
//! - `sibling-stream-isolation` — two simultaneous streams, distinct
//!   Streaming connection IDs, close one, prove the other usable.
//! - `slow-reader` / `slow-writer` — stalled-reader pressure stays
//!   within explicit reservoirs derived from
//!   [`i2pr_client::streaming::StreamingConfig::balanced`], writer
//!   stalls rather than buffering unboundedly, exact recovery.
//! - `fault-data-drop`, `fault-ack-drop`, `fault-duplicate`,
//!   `fault-reorder`, `fault-corruption`, `fault-retransmit-ceiling`
//!   — deterministic single-fault recovery beneath real SAM sockets.
//! - `close-reset-lifecycle` — graceful EOF, RST-driven RESET with
//!   wire RESET evidence, control teardown, repeated cycles.
//! - `privacy-log` — failure paths never log private destinations or
//!   application payload bytes.

#![allow(clippy::too_many_lines)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use i2pr_api::sam::limits::SamLimits;
use i2pr_client::streaming::{ConnectionState, StreamingConfig};
use i2pr_daemon::config::SamConfig;
use i2pr_daemon::sam::{SamDeliveryFaultProfile, SamServiceState};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const HELLO_3_1: &[u8] = b"HELLO VERSION MIN=3.1 MAX=3.1\n";
const SMALL_TRANSFER_TIMEOUT: Duration = Duration::from_secs(60);
const LIFECYCLE_POLL_TIMEOUT: Duration = Duration::from_secs(15);

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
    faults: Option<SamDeliveryFaultProfile>,
) -> (
    Arc<SamServiceState>,
    SocketAddr,
    ChildScope,
    CancellationToken,
) {
    let state = Arc::new(SamServiceState::new(config.clone()).expect("state"));
    if let Some(profile) = faults {
        state.install_test_fault_profile(profile);
    }
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

async fn read_one_line<R: AsyncReadExt + Unpin>(stream: &mut R) -> String {
    let mut buf = Vec::new();
    loop {
        let mut byte = [0_u8; 1];
        match stream.read_exact(&mut byte).await {
            Ok(0) => break,
            Ok(_) => {
                buf.push(byte[0]);
                if byte[0] == b'\n' {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf)
        .into_owned()
        .trim_end_matches(['\r', '\n'])
        .to_owned()
}

async fn write_all<W: AsyncWriteExt + Unpin>(stream: &mut W, bytes: &[u8]) {
    stream.write_all(bytes).await.expect("write_all");
    stream.flush().await.expect("flush");
}

async fn read_until_eof<R: AsyncReadExt + Unpin>(stream: &mut R, timeout: Duration) -> Vec<u8> {
    let mut out = Vec::new();
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let mut chunk = vec![0_u8; 4096];
        match tokio::time::timeout(remaining, stream.read(&mut chunk)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(read)) => out.extend_from_slice(&chunk[..read]),
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }
    out
}

async fn hello_3_1(stream: &mut TcpStream) {
    write_all(stream, HELLO_3_1).await;
    let reply = read_one_line(stream).await;
    assert!(
        reply.starts_with("HELLO REPLY RESULT=OK VERSION=3.1"),
        "expected HELLO OK, got {reply:?}"
    );
}

fn strip_sam_quotes(value: &str) -> String {
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        value[1..value.len() - 1].to_owned()
    } else {
        value.to_owned()
    }
}

/// Creates session `id` on `stream` and returns the peer-addressable
/// public destination.
async fn session_create(stream: &mut TcpStream, id: &str, destination: &str) -> String {
    let line = format!("SESSION CREATE STYLE=STREAM ID={id} DESTINATION={destination}\n");
    write_all(stream, line.as_bytes()).await;
    let reply = read_one_line(stream).await;
    assert!(
        reply.starts_with("SESSION STATUS RESULT=OK"),
        "SESSION CREATE for {id} did not return OK: {reply:?}"
    );
    write_all(stream, b"NAMING LOOKUP NAME=ME\n").await;
    let naming_reply = read_one_line(stream).await;
    assert!(
        naming_reply.starts_with("NAMING REPLY RESULT=OK VALUE="),
        "NAMING LOOKUP NAME=ME for {id} failed: {naming_reply:?}"
    );
    naming_reply
        .split_whitespace()
        .find_map(|tok| tok.strip_prefix("VALUE=").map(strip_sam_quotes))
        .expect("NAMING REPLY contains VALUE=<pub>")
}

async fn transient_destination(address: SocketAddr) -> String {
    let mut helper = TcpStream::connect(address)
        .await
        .expect("transient connect");
    hello_3_1(&mut helper).await;
    write_all(&mut helper, b"DEST GENERATE SIGNATURE_TYPE=7\n").await;
    let reply = read_one_line(&mut helper).await;
    assert!(
        reply.starts_with("DEST REPLY RESULT=OK"),
        "DEST GENERATE failed: {reply:?}"
    );
    let mut priv_value = None;
    for token in reply.split_whitespace() {
        if let Some(rest) = token.strip_prefix("PRIV=") {
            priv_value = Some(rest.to_owned());
        }
    }
    priv_value.expect("PRIV= value present in DEST REPLY")
}

/// Opens a fresh socket, HELLOs, issues `STREAM CONNECT`, and returns
/// the raw stream socket after `STREAM STATUS RESULT=OK`.
async fn connect_stream(address: SocketAddr, session_id: &str, peer_pub: &str) -> TcpStream {
    let mut stream = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut stream).await;
    let cmd = format!("STREAM CONNECT ID={session_id} DESTINATION={peer_pub}\n");
    write_all(&mut stream, cmd.as_bytes()).await;
    let line = read_one_line(&mut stream).await;
    assert!(
        line.starts_with("STREAM STATUS RESULT=OK"),
        "CONNECT for {session_id} not OK: {line:?}"
    );
    stream
}

/// Opens a fresh socket, HELLOs, issues `STREAM ACCEPT`, and returns
/// the raw stream socket plus the authenticated peer-destination line.
async fn accept_stream(address: SocketAddr, session_id: &str) -> (TcpStream, String) {
    let mut stream = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut stream).await;
    write_all(
        &mut stream,
        format!("STREAM ACCEPT ID={session_id}\n").as_bytes(),
    )
    .await;
    let line = read_one_line(&mut stream).await;
    assert!(
        line.starts_with("STREAM STATUS RESULT=OK"),
        "ACCEPT for {session_id} not OK: {line:?}"
    );
    let peer_line = read_one_line(&mut stream).await;
    assert!(
        peer_line.starts_with("DESTINATION="),
        "ACCEPT peer destination line missing: {peer_line:?}"
    );
    (stream, peer_line)
}

/// Establishes one bidirectional stream pair. SESSION CREATE and
/// STREAM CONNECT/ACCEPT share one socket per session (the proven
/// Plan 149 path), so no separate control socket exists: dropping a
/// returned socket closes its stream, and the last close tears the
/// session down through the normal attachment-release path.
async fn establish_pair(address: SocketAddr) -> (TcpStream, TcpStream) {
    let priv_a = transient_destination(address).await;
    let priv_b = transient_destination(address).await;
    let mut client_a = TcpStream::connect(address).await.expect("connect a");
    let mut client_b = TcpStream::connect(address).await.expect("connect b");
    hello_3_1(&mut client_a).await;
    hello_3_1(&mut client_b).await;
    let _pub_a = session_create(&mut client_a, "alpha", &priv_a).await;
    let pub_b = session_create(&mut client_b, "beta", &priv_b).await;
    let accept_task = tokio::spawn(async move {
        write_all(&mut client_b, b"STREAM ACCEPT ID=beta\n").await;
        let line = read_one_line(&mut client_b).await;
        assert!(
            line.starts_with("STREAM STATUS RESULT=OK"),
            "ACCEPT not OK: {line:?}"
        );
        let peer = read_one_line(&mut client_b).await;
        assert!(peer.starts_with("DESTINATION="), "peer line: {peer:?}");
        client_b
    });
    let connect_task = tokio::spawn(async move {
        let cmd = format!("STREAM CONNECT ID=alpha DESTINATION={pub_b}\n");
        write_all(&mut client_a, cmd.as_bytes()).await;
        let line = read_one_line(&mut client_a).await;
        assert!(
            line.starts_with("STREAM STATUS RESULT=OK"),
            "CONNECT not OK: {line:?}"
        );
        client_a
    });
    let (accept_result, connect_result) = tokio::join!(accept_task, connect_task);
    (
        connect_result.expect("connect task"),
        accept_result.expect("accept task"),
    )
}

fn test_pattern(len: usize, seed: u32) -> Vec<u8> {
    (0..len as u32)
        .map(|i| ((i.wrapping_mul(17).wrapping_add(seed) ^ i.rotate_left(7)) & 0xFF) as u8)
        .collect()
}

/// Transfers `payload` A→B and `reply` B→A concurrently (split halves
/// so large bidirectional payloads cannot deadlock on TCP buffers).
async fn exchange_exact(
    a_raw: &mut TcpStream,
    b_raw: &mut TcpStream,
    payload: &[u8],
    reply: &[u8],
) {
    let (mut a_read, mut a_write) = a_raw.split();
    let (mut b_read, mut b_write) = b_raw.split();
    let payload_len = payload.len();
    let reply_len = reply.len();
    let (received_a, received_b) = tokio::join!(
        async {
            let mut out = vec![0_u8; reply_len];
            write_all(&mut a_write, payload).await;
            tokio::time::timeout(SMALL_TRANSFER_TIMEOUT, a_read.read_exact(&mut out))
                .await
                .expect("bounded A read")
                .expect("A read_exact");
            out
        },
        async {
            let mut out = vec![0_u8; payload_len];
            write_all(&mut b_write, reply).await;
            tokio::time::timeout(SMALL_TRANSFER_TIMEOUT, b_read.read_exact(&mut out))
                .await
                .expect("bounded B read")
                .expect("B read_exact");
            out
        }
    );
    assert_eq!(received_a, reply, "A payload mismatch");
    assert_eq!(received_b, payload, "B payload mismatch");
}

/// One-direction exact transfer with a bounded deadline.
async fn send_exact_one_way(sender: &mut TcpStream, receiver: &mut TcpStream, payload: &[u8]) {
    write_all(sender, payload).await;
    let mut out = vec![0_u8; payload.len()];
    tokio::time::timeout(SMALL_TRANSFER_TIMEOUT, receiver.read_exact(&mut out))
        .await
        .expect("bounded one-way read")
        .expect("receiver read_exact");
    assert_eq!(out, payload, "one-way payload mismatch");
}

async fn wait_for_condition(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if condition() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    condition()
}

fn destination_id_for(state: &SamServiceState, session_id: &str) -> i2pr_client::DestinationId {
    let session = i2pr_api::SamSessionId::new(session_id).expect("session id");
    state
        .session_registry()
        .get(&session)
        .expect("session registered")
        .destination_id()
}

/// Non-secret per-destination Streaming queue gauges.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct QueueGauges {
    outbound_queued: usize,
    delivered_units: usize,
    delivered_bytes: usize,
    tracked_retransmits: usize,
    connections: usize,
    unacked_packets: usize,
    pending_acks: usize,
}

fn queue_gauges(state: &SamServiceState, session_id: &str) -> QueueGauges {
    let destination_id = destination_id_for(state, session_id);
    let handle = state
        .sam_destinations()
        .lock()
        .expect("sam destinations poisoned")
        .get(destination_id)
        .expect("bridge registered");
    handle.with(|bridge| {
        let mut gauges = QueueGauges::default();
        for manager in [bridge.streaming(), bridge.receiver_streaming()] {
            gauges.outbound_queued += manager.outbound_queue_len();
            gauges.delivered_units += manager.pending_delivered_len();
            gauges.delivered_bytes += manager.pending_delivered_bytes();
            gauges.tracked_retransmits += manager.tracked_retransmit_count();
            gauges.pending_acks += manager.pending_ack_count();
            gauges.connections += manager.connection_count();
            for connection in manager.iter_connections() {
                gauges.unacked_packets += connection.send_window().unacked_count();
            }
        }
        gauges
    })
}

fn streaming_connection_ids(state: &SamServiceState, session_id: &str) -> Vec<u64> {
    let destination_id = destination_id_for(state, session_id);
    let handle = state
        .sam_destinations()
        .lock()
        .expect("sam destinations poisoned")
        .get(destination_id)
        .expect("bridge registered");
    handle.with(|bridge| {
        bridge
            .streaming()
            .iter_connections()
            .chain(bridge.receiver_streaming().iter_connections())
            .map(|connection| connection.local_stream_id() as u64)
            .collect()
    })
}

fn established_connection_count(state: &SamServiceState, session_id: &str) -> usize {
    let destination_id = destination_id_for(state, session_id);
    let handle = state
        .sam_destinations()
        .lock()
        .expect("sam destinations poisoned")
        .get(destination_id)
        .expect("bridge registered");
    handle.with(|bridge| {
        bridge
            .streaming()
            .iter_connections()
            .chain(bridge.receiver_streaming().iter_connections())
            .filter(|connection| matches!(connection.state(), ConnectionState::Established))
            .count()
    })
}

fn unacked_packets(state: &SamServiceState, session_id: &str) -> usize {
    queue_gauges(state, session_id).unacked_packets
}

/// Tolerant gauge read for post-close baselines: once every
/// attachment has closed, the session (and its bridge) is
/// legitimately torn down, so missing state reads as zero rather
/// than panicking. A zero snapshot after teardown IS the baseline.
fn queue_gauges_quiet(state: &SamServiceState, session_id: &str) -> QueueGauges {
    let session = match i2pr_api::SamSessionId::new(session_id) {
        Some(session) => session,
        None => return QueueGauges::default(),
    };
    let destination_id = match state.session_registry().get(&session) {
        Some(entry) => entry.destination_id(),
        None => return QueueGauges::default(),
    };
    let destinations = state.sam_destinations();
    let bridges = match destinations.lock() {
        Ok(bridges) => bridges,
        Err(_) => return QueueGauges::default(),
    };
    let Some(handle) = bridges.get(destination_id) else {
        return QueueGauges::default();
    };
    handle.with(|bridge| {
        let mut gauges = QueueGauges::default();
        for manager in [bridge.streaming(), bridge.receiver_streaming()] {
            gauges.outbound_queued += manager.outbound_queue_len();
            gauges.delivered_units += manager.pending_delivered_len();
            gauges.delivered_bytes += manager.pending_delivered_bytes();
            gauges.tracked_retransmits += manager.tracked_retransmit_count();
            gauges.pending_acks += manager.pending_ack_count();
            gauges.connections += manager.connection_count();
            for connection in manager.iter_connections() {
                gauges.unacked_packets += connection.send_window().unacked_count();
            }
        }
        gauges
    })
}

async fn shutdown_and_assert_baselines(
    state: &Arc<SamServiceState>,
    scope: ChildScope,
    parent: CancellationToken,
) {
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    assert_eq!(state.session_registry().session_count(), 0);
    assert_eq!(state.destination_registry().lock().unwrap().len(), 0);
    assert_eq!(state.streaming_pools().lock().unwrap().len(), 0);
    assert_eq!(state.stream_registry().active_session_count(), 0);
    assert_eq!(state.stream_registry().attachment_count(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn plan151_sibling_streams_isolate_close_one() {
    let (state, address, scope, parent) = start_listener(sam_config(), None).await;

    let priv_a = transient_destination(address).await;
    let priv_b = transient_destination(address).await;
    let mut client_a = TcpStream::connect(address).await.expect("connect a");
    let mut client_b = TcpStream::connect(address).await.expect("connect b");
    hello_3_1(&mut client_a).await;
    hello_3_1(&mut client_b).await;
    let pub_a = session_create(&mut client_a, "alpha", &priv_a).await;
    let pub_b = session_create(&mut client_b, "beta", &priv_b).await;

    // Stream one uses the session sockets themselves (the proven
    // Plan 149 same-socket path); stream two uses fresh sockets
    // against the same live sessions.
    let accept_one = tokio::spawn(async move {
        write_all(&mut client_b, b"STREAM ACCEPT ID=beta\n").await;
        let line = read_one_line(&mut client_b).await;
        assert!(line.starts_with("STREAM STATUS RESULT=OK"), "{line:?}");
        let peer = read_one_line(&mut client_b).await;
        assert!(peer.starts_with("DESTINATION="), "{peer:?}");
        (client_b, peer)
    });
    let pub_b_two = pub_b.clone();
    let connect_one = tokio::spawn(async move {
        let cmd = format!("STREAM CONNECT ID=alpha DESTINATION={pub_b}\n");
        write_all(&mut client_a, cmd.as_bytes()).await;
        let line = read_one_line(&mut client_a).await;
        assert!(line.starts_with("STREAM STATUS RESULT=OK"), "{line:?}");
        client_a
    });
    // The second ACCEPT waiter is queued before either CONNECT lands
    // so both pairings can complete.
    let accept_two = tokio::spawn(async move { accept_stream(address, "beta").await });
    tokio::task::yield_now().await;
    let connect_two =
        tokio::spawn(async move { connect_stream(address, "alpha", &pub_b_two).await });
    let (accept_one, connect_one, accept_two, connect_two) =
        tokio::join!(accept_one, connect_one, accept_two, connect_two);
    let (mut b_one, peer_one) = accept_one.expect("accept one");
    let mut a_one = connect_one.expect("connect one");
    let (mut b_two, peer_two) = accept_two.expect("accept two");
    let mut a_two = connect_two.expect("connect two");
    assert!(
        peer_one.contains(&pub_a),
        "sibling one peer was not the authenticated CONNECT destination"
    );
    assert!(
        peer_two.contains(&pub_a),
        "sibling two peer was not the authenticated CONNECT destination"
    );

    // Both streams carry unique binary payloads in both directions.
    let payload_one_a = test_pattern(32 * 1024, 0xA1);
    let payload_one_b = test_pattern(32 * 1024, 0xB1);
    let payload_two_a = test_pattern(32 * 1024, 0xA2);
    let payload_two_b = test_pattern(32 * 1024, 0xB2);
    let (first, second) = tokio::join!(
        async {
            exchange_exact(&mut a_one, &mut b_one, &payload_one_a, &payload_one_b).await;
        },
        async {
            exchange_exact(&mut a_two, &mut b_two, &payload_two_a, &payload_two_b).await;
        }
    );
    let _ = (first, second);

    // The two siblings own distinct Streaming connections on both
    // halves, all established.
    let alpha_ids = streaming_connection_ids(&state, "alpha");
    let beta_ids = streaming_connection_ids(&state, "beta");
    assert_eq!(alpha_ids.len(), 2, "alpha owns two stream connections");
    assert_eq!(beta_ids.len(), 2, "beta owns two stream connections");
    assert_ne!(
        alpha_ids[0], alpha_ids[1],
        "sibling streams share one connection"
    );
    assert_ne!(
        beta_ids[0], beta_ids[1],
        "sibling streams share one connection"
    );
    assert_eq!(established_connection_count(&state, "alpha"), 2);
    assert_eq!(established_connection_count(&state, "beta"), 2);

    // Close stream one from the alpha end only. The beta end must
    // observe EOF, while stream two remains fully usable.
    drop(a_one);
    let tail_one = read_until_eof(&mut b_one, LIFECYCLE_POLL_TIMEOUT).await;
    assert!(
        tail_one.is_empty(),
        "closed sibling delivered {} unexpected tail bytes",
        tail_one.len()
    );
    assert!(
        wait_for_condition(LIFECYCLE_POLL_TIMEOUT, || {
            state.stream_registry().attachment_count() == 2
        })
        .await,
        "attachments did not release the closed sibling"
    );

    // Stream two transfers fresh unique payloads in both directions
    // after its sibling closed.
    let payload_two_c = test_pattern(32 * 1024, 0xC2);
    let payload_two_d = test_pattern(32 * 1024, 0xD2);
    exchange_exact(&mut a_two, &mut b_two, &payload_two_c, &payload_two_d).await;
    assert_eq!(established_connection_count(&state, "alpha"), 1);
    assert_eq!(established_connection_count(&state, "beta"), 1);

    drop(a_two);
    drop(b_one);
    drop(b_two);
    assert!(
        wait_for_condition(LIFECYCLE_POLL_TIMEOUT, || {
            state.stream_registry().attachment_count() == 0
                && state.session_registry().session_count() == 0
        })
        .await,
        "sibling lifecycle did not return to baseline"
    );
    shutdown_and_assert_baselines(&state, scope, parent).await;
}

fn slow_peer_ceilings(streams: usize) -> (usize, usize, usize, usize) {
    // Explicit reservoirs derived from the balanced Streaming
    // profile: send/recv windows bound in-flight packets per
    // connection, and each packet carries at most
    // DEFAULT_ADVERTISED_MAX_PAYLOAD bytes. Ceilings scale with the
    // live connection count and carry 2x headroom over the
    // theoretical maxima, so a failure proves retention proportional
    // to offered load (megabytes) rather than scheduler jitter.
    // The sender-side unacked/tracked bounds are exact window
    // invariants (admission control enforces them with or without
    // any fix); the receiver-side delivered-bytes ceiling is the
    // discriminating bound for receive-side backpressure.
    let config = StreamingConfig::balanced();
    let per_packet = i2pr_proto::streaming::DEFAULT_ADVERTISED_MAX_PAYLOAD as usize;
    let window_bytes = config.max_send_window_packets as usize * per_packet;
    let window_packets = config.max_send_window_packets as usize;
    let delivered_ceiling = streams * window_bytes * 2;
    let unacked_ceiling = streams * window_packets + 16;
    let queue_ceiling = 256_usize;
    let tracked_ceiling = streams * window_packets + 16;
    (
        delivered_ceiling,
        unacked_ceiling,
        queue_ceiling,
        tracked_ceiling,
    )
}

fn assert_gauges_within_ceilings(
    gauges: QueueGauges,
    delivered_ceiling: usize,
    unacked_ceiling: usize,
    queue_ceiling: usize,
    tracked_ceiling: usize,
    context: &str,
) {
    assert!(
        gauges.delivered_bytes <= delivered_ceiling,
        "{context}: stalled receiver retained {} bytes (ceiling {delivered_ceiling})",
        gauges.delivered_bytes
    );
    assert!(
        gauges.unacked_packets <= unacked_ceiling,
        "{context}: sender held {} unacked packets (ceiling {unacked_ceiling})",
        gauges.unacked_packets
    );
    assert!(
        gauges.outbound_queued <= queue_ceiling,
        "{context}: outbound queue held {} requests (ceiling {queue_ceiling})",
        gauges.outbound_queued
    );
    assert!(
        gauges.tracked_retransmits <= tracked_ceiling,
        "{context}: retransmit tracking held {} records (ceiling {tracked_ceiling})",
        gauges.tracked_retransmits
    );
}

/// Drives `streams` stalled-reader bulk transfers: every beta socket
/// stops reading while every alpha socket offers `bytes_per_stream`.
/// Stream one uses the session sockets themselves (the proven Plan
/// 149 same-socket path); streams two and up use fresh sockets
/// against the same live sessions. Returns the writer tasks (which
/// hold every alpha socket open so no EOF truncates the offered
/// load), the beta sockets, and the payloads for post-resume
/// verification.
async fn start_stalled_bulk(
    address: SocketAddr,
    streams: usize,
    bytes_per_stream: usize,
) -> (
    Vec<tokio::task::JoinHandle<()>>,
    Vec<TcpStream>,
    Vec<Vec<u8>>,
) {
    assert!(streams >= 1, "at least one bulk stream required");
    let priv_a = transient_destination(address).await;
    let priv_b = transient_destination(address).await;
    let mut client_a = TcpStream::connect(address).await.expect("connect a");
    let mut client_b = TcpStream::connect(address).await.expect("connect b");
    hello_3_1(&mut client_a).await;
    hello_3_1(&mut client_b).await;
    let pub_b = session_create(&mut client_b, "beta", &priv_b).await;
    let _ = session_create(&mut client_a, "alpha", &priv_a).await;

    // First pair on the session sockets themselves.
    let accept_first = tokio::spawn(async move {
        write_all(&mut client_b, b"STREAM ACCEPT ID=beta\n").await;
        let line = read_one_line(&mut client_b).await;
        assert!(line.starts_with("STREAM STATUS RESULT=OK"), "{line:?}");
        let peer = read_one_line(&mut client_b).await;
        assert!(peer.starts_with("DESTINATION="), "{peer:?}");
        client_b
    });
    let pub_b_first = pub_b.clone();
    let connect_first = tokio::spawn(async move {
        let cmd = format!("STREAM CONNECT ID=alpha DESTINATION={pub_b_first}\n");
        write_all(&mut client_a, cmd.as_bytes()).await;
        let line = read_one_line(&mut client_a).await;
        assert!(line.starts_with("STREAM STATUS RESULT=OK"), "{line:?}");
        client_a
    });
    let mut accepts = Vec::new();
    for _ in 1..streams {
        accepts.push(tokio::spawn(
            async move { accept_stream(address, "beta").await },
        ));
    }
    tokio::task::yield_now().await;
    let mut connects = Vec::new();
    connects.push(connect_first);
    for _ in 1..streams {
        let peer = pub_b.clone();
        connects.push(tokio::spawn(async move {
            connect_stream(address, "alpha", &peer).await
        }));
    }
    let mut a_sockets = Vec::new();
    for task in connects {
        a_sockets.push(task.await.expect("connect task"));
    }
    let mut b_sockets = Vec::new();
    b_sockets.push(accept_first.await.expect("accept task"));
    for task in accepts {
        let (stream, _) = task.await.expect("accept task");
        b_sockets.push(stream);
    }
    assert_eq!(a_sockets.len(), streams);
    assert_eq!(b_sockets.len(), streams);

    let mut writers = Vec::new();
    let mut payloads = Vec::new();
    for (index, mut socket) in a_sockets.into_iter().enumerate() {
        let payload = test_pattern(bytes_per_stream, 0x50 + index as u32);
        payloads.push(payload.clone());
        writers.push(tokio::spawn(async move {
            let _ = socket.write_all(&payload).await;
            let _ = socket.flush().await;
            // Hold the socket open so the server never observes EOF
            // while pressure is applied; dropping it here would
            // truncate the offered load (half-close artifact).
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }));
    }
    (writers, b_sockets, payloads)
}

/// Cumulative typed sweep-failure counters for both destinations
/// (missing factory, exhausted factory, unknown peer, rejected
/// delivery) observed during a bulk drain.
type SweepFailures = (usize, usize, usize, usize, usize, usize, usize, usize);

/// Drains every socket concurrently after a stalled phase, asserting
/// exact ordered recovery on each stream. Concurrent (not sequential)
/// draining shares the single-threaded scheduler fairly; per-stream
/// progress counters plus queue/sweep gauges expose stall-vs-slow in
/// the log. Consumes the sockets: all are closed when the drain tasks
/// finish, so lifecycle can return to baseline afterwards.
async fn drain_all_concurrent(
    label: &'static str,
    state: &Arc<SamServiceState>,
    sockets: Vec<TcpStream>,
    payloads: Vec<Vec<u8>>,
    budget: Duration,
) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    assert_eq!(sockets.len(), payloads.len(), "socket/payload pairing");
    let progress: Vec<Arc<AtomicUsize>> = sockets
        .iter()
        .map(|_| Arc::new(AtomicUsize::new(0)))
        .collect();
    let mut drains = Vec::new();
    for ((index, mut socket), payload) in sockets.into_iter().enumerate().zip(payloads) {
        let counter = progress[index].clone();
        drains.push(tokio::spawn(async move {
            let mut out = vec![0_u8; payload.len()];
            let mut filled = 0_usize;
            let outcome = tokio::time::timeout(budget, async {
                while filled < out.len() {
                    let n = socket.read(&mut out[filled..]).await.expect("drain read");
                    if n == 0 {
                        break;
                    }
                    filled += n;
                    counter.store(filled, Ordering::SeqCst);
                }
                filled
            })
            .await;
            (index, outcome, out, payload)
        }));
    }
    let alpha_id = destination_id_for(state, "alpha");
    let beta_id = destination_id_for(state, "beta");
    let deadline = tokio::time::Instant::now() + budget;
    // Last cumulative sweep snapshots taken while the sessions were
    // still alive; teardown removes the entries, so the final
    // failure accounting is asserted from these.
    let mut last_failures: Option<SweepFailures> = None;
    loop {
        tokio::time::sleep(Duration::from_secs(15)).await;
        if drains.iter().all(|drain| drain.is_finished()) {
            break;
        }
        let bytes: Vec<usize> = progress
            .iter()
            .map(|counter| counter.load(Ordering::SeqCst))
            .collect();
        let total: usize = bytes.iter().sum();
        // A finished drain closes its socket, which releases its
        // attachment; once every attachment is gone the session (and
        // its gauges) is legitimately torn down. Read the snapshot
        // tolerantly and stop logging at that point — the awaits
        // below surface any short drain as hard evidence.
        let gauges_alpha = queue_gauges_quiet(state, "alpha");
        let gauges_beta = queue_gauges_quiet(state, "beta");
        let sweep_alpha = state.delivery_counters(alpha_id);
        let sweep_beta = state.delivery_counters(beta_id);
        // Failure counters are cumulative and monotone, and teardown
        // removes the entries (reading back zero): merge by maximum
        // so one post-teardown snapshot cannot hide a failure seen
        // while the sessions were alive.
        let observed = (
            sweep_alpha.missing_factory,
            sweep_alpha.factory_exhausted,
            sweep_alpha.unknown_peer,
            sweep_alpha.delivery_failed,
            sweep_beta.missing_factory,
            sweep_beta.factory_exhausted,
            sweep_beta.unknown_peer,
            sweep_beta.delivery_failed,
        );
        last_failures = Some(match last_failures {
            None => observed,
            Some(previous) => (
                previous.0.max(observed.0),
                previous.1.max(observed.1),
                previous.2.max(observed.2),
                previous.3.max(observed.3),
                previous.4.max(observed.4),
                previous.5.max(observed.5),
                previous.6.max(observed.6),
                previous.7.max(observed.7),
            ),
        });
        eprintln!(
            "plan151 {label} drain progress bytes={bytes:?} total={total} alpha={gauges_alpha:?} beta={gauges_beta:?} sweep_a={sweep_alpha:?} sweep_b={sweep_beta:?}"
        );
        if tokio::time::Instant::now() >= deadline {
            break;
        }
    }
    for drain in drains {
        let (index, outcome, out, payload) = drain.await.expect("drain task");
        let filled = outcome.expect("bounded drain");
        assert_eq!(filled, payload.len(), "{label} stream{index} short drain");
        assert_eq!(&out, &payload, "{label} recovered payload mismatch");
    }
    // Bulk recovery must be failure-free at the typed sweep layer:
    // no missed factory, no unknown peer, no rejected delivery.
    if let Some(failures) = last_failures {
        assert_eq!(
            failures,
            (0, 0, 0, 0, 0, 0, 0, 0),
            "{label} typed delivery failures during recovery: {failures:?}"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn plan151_slow_reader_stays_bounded_and_recovers() {
    // Six parallel streams x 2 MiB = 12 MiB offered against stalled
    // receivers: far above the combined Streaming windows
    // (6 x 64 x 1730 B) and kernel TCP buffers, so Streaming-layer
    // retention (not socket buffering) dominates the gauges within
    // seconds. Plan 151 §7 is not a throughput benchmark, so the
    // profile stays deliberately small enough to drain exactly on a
    // debug single-threaded runtime.
    const STREAMS: usize = 6;
    const BYTES_PER_STREAM: usize = 2 * 1024 * 1024;
    let (state, address, scope, parent) = start_listener(sam_config(), None).await;
    let (delivered_ceiling, unacked_ceiling, queue_ceiling, tracked_ceiling) =
        slow_peer_ceilings(STREAMS);

    let (writers, b_sockets, payloads) =
        start_stalled_bulk(address, STREAMS, BYTES_PER_STREAM).await;

    // While every reader is stalled, retained bytes must stay within
    // the explicit reservoirs even as megabytes are offered.
    for (round, wait) in [20_u64, 15_u64].iter().enumerate() {
        tokio::time::sleep(Duration::from_secs(*wait)).await;
        for session in ["alpha", "beta"] {
            let gauges = queue_gauges(&state, session);
            eprintln!("plan151 slow-reader round{round} session={session} gauges={gauges:?}");
            assert_gauges_within_ceilings(
                gauges,
                delivered_ceiling,
                unacked_ceiling,
                queue_ceiling,
                tracked_ceiling,
                &format!("slow-reader round{round} session={session} gauges={gauges:?}"),
            );
        }
    }
    // Every writer is still parked on backpressure: none of the
    // 12 MiB could have been fully absorbed.
    for writer in writers.iter() {
        assert!(
            !writer.is_finished(),
            "writer finished while every reader was stalled: no backpressure applied"
        );
    }

    // Resume every reader concurrently: the full 12 MiB must arrive
    // exactly, in order, on every stream.
    drain_all_concurrent(
        "slow-reader",
        &state,
        b_sockets,
        payloads,
        Duration::from_secs(180),
    )
    .await;
    for writer in writers {
        writer.abort();
    }
    // The drain closed every socket, so both sessions may already be
    // legitimately torn down; a missing bridge reads as zero gauges,
    // which IS the queue baseline after close.
    assert!(
        wait_for_condition(LIFECYCLE_POLL_TIMEOUT, || {
            queue_gauges_quiet(&state, "alpha") == QueueGauges::default()
                && queue_gauges_quiet(&state, "beta") == QueueGauges::default()
        })
        .await,
        "slow-reader queues did not drain after resume"
    );
    // Drain tasks own the B-side sockets now; awaiting them above
    // closed those sockets, so lifecycle can return to baseline.
    assert!(
        wait_for_condition(LIFECYCLE_POLL_TIMEOUT, || {
            state.stream_registry().attachment_count() == 0
                && state.session_registry().session_count() == 0
        })
        .await,
        "slow-reader lifecycle did not return to baseline"
    );
    shutdown_and_assert_baselines(&state, scope, parent).await;
}

#[tokio::test(flavor = "current_thread")]
async fn plan151_slow_writer_reverse_pressure_recovers() {
    // Mirror image: beta offers bulk while alpha stops reading, so
    // the reverse raw-bridge half is exercised under pressure. Same
    // deliberately small profile as the slow reader (Plan 151 §7 is
    // not a throughput benchmark).
    const STREAMS: usize = 6;
    const BYTES_PER_STREAM: usize = 2 * 1024 * 1024;
    let (state, address, scope, parent) = start_listener(sam_config(), None).await;
    let (delivered_ceiling, unacked_ceiling, queue_ceiling, tracked_ceiling) =
        slow_peer_ceilings(STREAMS);

    let priv_a = transient_destination(address).await;
    let priv_b = transient_destination(address).await;
    let mut client_a = TcpStream::connect(address).await.expect("connect a");
    let mut client_b = TcpStream::connect(address).await.expect("connect b");
    hello_3_1(&mut client_a).await;
    hello_3_1(&mut client_b).await;
    let pub_a = session_create(&mut client_a, "alpha", &priv_a).await;
    let _ = session_create(&mut client_b, "beta", &priv_b).await;

    // Alpha ACCEPTs (stalled readers), beta CONNECTs (writers).
    // First pair on the session sockets themselves.
    let accept_first = tokio::spawn(async move {
        write_all(&mut client_a, b"STREAM ACCEPT ID=alpha\n").await;
        let line = read_one_line(&mut client_a).await;
        assert!(line.starts_with("STREAM STATUS RESULT=OK"), "{line:?}");
        let peer = read_one_line(&mut client_a).await;
        assert!(peer.starts_with("DESTINATION="), "{peer:?}");
        client_a
    });
    let pub_a_first = pub_a.clone();
    let connect_first = tokio::spawn(async move {
        let cmd = format!("STREAM CONNECT ID=beta DESTINATION={pub_a_first}\n");
        write_all(&mut client_b, cmd.as_bytes()).await;
        let line = read_one_line(&mut client_b).await;
        assert!(line.starts_with("STREAM STATUS RESULT=OK"), "{line:?}");
        client_b
    });
    let mut accepts = Vec::new();
    for _ in 1..STREAMS {
        accepts.push(tokio::spawn(async move {
            accept_stream(address, "alpha").await
        }));
    }
    tokio::task::yield_now().await;
    let mut connects = Vec::new();
    connects.push(connect_first);
    for _ in 1..STREAMS {
        let peer = pub_a.clone();
        connects.push(tokio::spawn(async move {
            connect_stream(address, "beta", &peer).await
        }));
    }
    let mut b_sockets = Vec::new();
    for task in connects {
        b_sockets.push(task.await.expect("connect task"));
    }
    let mut a_sockets = Vec::new();
    a_sockets.push(accept_first.await.expect("accept task"));
    for task in accepts {
        let (stream, _) = task.await.expect("accept task");
        a_sockets.push(stream);
    }
    assert_eq!(a_sockets.len(), STREAMS);
    assert_eq!(b_sockets.len(), STREAMS);

    let mut writers = Vec::new();
    let mut payloads = Vec::new();
    for (index, mut socket) in b_sockets.into_iter().enumerate() {
        let payload = test_pattern(BYTES_PER_STREAM, 0x70 + index as u32);
        payloads.push(payload.clone());
        writers.push(tokio::spawn(async move {
            let _ = socket.write_all(&payload).await;
            let _ = socket.flush().await;
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }));
    }

    for (round, wait) in [20_u64, 15_u64].iter().enumerate() {
        tokio::time::sleep(Duration::from_secs(*wait)).await;
        for session in ["alpha", "beta"] {
            let gauges = queue_gauges(&state, session);
            eprintln!("plan151 slow-writer round{round} session={session} gauges={gauges:?}");
            assert_gauges_within_ceilings(
                gauges,
                delivered_ceiling,
                unacked_ceiling,
                queue_ceiling,
                tracked_ceiling,
                &format!("slow-writer round{round} session={session} gauges={gauges:?}"),
            );
        }
    }
    for writer in writers.iter() {
        assert!(
            !writer.is_finished(),
            "reverse writer finished while every reader was stalled"
        );
    }

    // Resume every reader concurrently: the full reverse 12 MiB
    // must arrive exactly, in order, on every stream.
    drain_all_concurrent(
        "slow-writer",
        &state,
        a_sockets,
        payloads,
        Duration::from_secs(180),
    )
    .await;
    for writer in writers {
        writer.abort();
    }
    // Same legitimate-teardown baseline as the slow reader: missing
    // bridges read as zero gauges.
    assert!(
        wait_for_condition(LIFECYCLE_POLL_TIMEOUT, || {
            queue_gauges_quiet(&state, "alpha") == QueueGauges::default()
                && queue_gauges_quiet(&state, "beta") == QueueGauges::default()
        })
        .await,
        "slow-writer queues did not drain after resume"
    );
    assert!(
        wait_for_condition(LIFECYCLE_POLL_TIMEOUT, || {
            state.stream_registry().attachment_count() == 0
                && state.session_registry().session_count() == 0
        })
        .await,
        "slow-writer lifecycle did not return to baseline"
    );
    shutdown_and_assert_baselines(&state, scope, parent).await;
}

#[tokio::test(flavor = "current_thread")]
async fn plan151_fault_data_drop_recovers_by_retransmission() {
    let mut profile = SamDeliveryFaultProfile::disabled();
    profile.arm_drop_data(1);
    let (state, address, scope, parent) = start_listener(sam_config(), Some(profile)).await;
    let (mut a_raw, mut b_raw) = establish_pair(address).await;

    // The first DATA packet is dropped below the SAM socket; the
    // sender's retransmit path must recover it with exact-once,
    // in-order application delivery.
    let payload = test_pattern(4096, 0xD0);
    let reply = test_pattern(4096, 0xD1);
    exchange_exact(&mut a_raw, &mut b_raw, &payload, &reply).await;
    let counters = state.fault_counters();
    assert_eq!(
        counters.dropped_data, 1,
        "drop fault never fired: {counters:?}"
    );
    assert!(
        counters.handshake_passthrough >= 2,
        "handshake control must pass through: {counters:?}"
    );
    // Full recovery leaves no stranded retransmit state.
    assert!(
        wait_for_condition(LIFECYCLE_POLL_TIMEOUT, || {
            queue_gauges(&state, "alpha").tracked_retransmits == 0
                && unacked_packets(&state, "alpha") == 0
        })
        .await,
        "sender retransmit state did not clear after recovery"
    );

    drop(a_raw);
    drop(b_raw);
    shutdown_and_assert_baselines(&state, scope, parent).await;
}

#[tokio::test(flavor = "current_thread")]
async fn plan151_fault_ack_drop_recovers_without_loop() {
    let mut profile = SamDeliveryFaultProfile::disabled();
    profile.arm_drop_ack(1);
    let (state, address, scope, parent) = start_listener(sam_config(), Some(profile)).await;
    let (mut a_raw, mut b_raw) = establish_pair(address).await;

    // One-direction transfer only, so the receiver's acknowledgement
    // must travel as a standalone delayed ACK (nothing to piggyback
    // on). That standalone ACK is suppressed; the sender
    // retransmits, the receiver deduplicates, and the stream
    // converges with exact bytes and no busy retry loop.
    let payload = test_pattern(4096, 0xA0);
    send_exact_one_way(&mut a_raw, &mut b_raw, &payload).await;
    assert!(
        wait_for_condition(Duration::from_secs(15), || {
            state.fault_counters().dropped_ack == 1
        })
        .await,
        "ack-drop fault never fired: {:?}",
        state.fault_counters()
    );
    // Full sender recovery must precede any reverse traffic:
    // reverse DATA would piggyback the missing acknowledgement and
    // mask a stranded send window. Every retransmitted packet must
    // be acknowledged and cleared on the forward path alone.
    assert!(
        wait_for_condition(Duration::from_secs(20), || {
            queue_gauges(&state, "alpha").tracked_retransmits == 0
                && unacked_packets(&state, "alpha") == 0
        })
        .await,
        "sender state did not clear after ACK recovery"
    );
    // Both directions remain usable after ACK recovery.
    let reply = test_pattern(1024, 0xA1);
    send_exact_one_way(&mut b_raw, &mut a_raw, &reply).await;
    let after = test_pattern(1024, 0xA2);
    send_exact_one_way(&mut a_raw, &mut b_raw, &after).await;

    drop(a_raw);
    drop(b_raw);
    shutdown_and_assert_baselines(&state, scope, parent).await;
}

#[tokio::test(flavor = "current_thread")]
async fn plan151_fault_duplicate_delivers_exactly_once() {
    let mut profile = SamDeliveryFaultProfile::disabled();
    profile.arm_duplicate_data(1);
    let (state, address, scope, parent) = start_listener(sam_config(), Some(profile)).await;
    let (mut a_raw, mut b_raw) = establish_pair(address).await;

    let payload = test_pattern(4096, 0xDD);
    let reply = test_pattern(1024, 0xDE);
    exchange_exact(&mut a_raw, &mut b_raw, &payload, &reply).await;
    let counters = state.fault_counters();
    assert_eq!(
        counters.duplicated, 1,
        "duplicate fault never fired: {counters:?}"
    );

    drop(a_raw);
    drop(b_raw);
    shutdown_and_assert_baselines(&state, scope, parent).await;
}

#[tokio::test(flavor = "current_thread")]
async fn plan151_fault_reorder_delivers_in_order() {
    let mut profile = SamDeliveryFaultProfile::disabled();
    profile.arm_reorder_one();
    let (state, address, scope, parent) = start_listener(sam_config(), Some(profile)).await;
    let (mut a_raw, mut b_raw) = establish_pair(address).await;

    // 8 KiB spans several Streaming packets, so the reordered pair
    // is guaranteed to exist on the wire.
    let payload = test_pattern(8192, 0x90);
    let reply = test_pattern(1024, 0x91);
    exchange_exact(&mut a_raw, &mut b_raw, &payload, &reply).await;
    let counters = state.fault_counters();
    assert_eq!(
        counters.reordered, 1,
        "reorder fault never fired: {counters:?}"
    );

    drop(a_raw);
    drop(b_raw);
    shutdown_and_assert_baselines(&state, scope, parent).await;
}

#[derive(Clone)]
struct LogCapture {
    buffer: Arc<std::sync::Mutex<Vec<u8>>>,
}

impl std::io::Write for LogCapture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.buffer
            .lock()
            .expect("log capture poisoned")
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogCapture {
    type Writer = LogCapture;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[tokio::test(flavor = "current_thread")]
async fn plan151_fault_corruption_rejected_without_delivery() {
    // Default-log capture: no failure path may emit private
    // destinations or raw application payload bytes.
    let capture = LogCapture {
        buffer: Arc::new(std::sync::Mutex::new(Vec::new())),
    };
    let subscriber = tracing_subscriber::fmt()
        .with_writer(capture.clone())
        .with_max_level(tracing::Level::DEBUG)
        .finish();
    let _log_guard = tracing::dispatcher::set_default(&tracing::Dispatch::new(subscriber));

    let mut profile = SamDeliveryFaultProfile::disabled();
    profile.arm_corrupt_data_after(1, 1);
    let (state, address, scope, parent) = start_listener(sam_config(), Some(profile)).await;
    let (mut a_raw, mut b_raw) = establish_pair(address).await;

    // Distinctive application bytes plus the session private values
    // must never appear in captured logs.
    let sentinel: Vec<u8> = b"PLAN151-LOG-SENTINEL-"
        .iter()
        .cycle()
        .take(512)
        .copied()
        .collect();
    send_exact_one_way(&mut a_raw, &mut b_raw, &sentinel).await;

    let after: Vec<u8> = b"PLAN151-AFTER-CORRUPTION-"
        .iter()
        .cycle()
        .take(512)
        .copied()
        .collect();
    write_all(&mut a_raw, &after).await;
    // The corrupted delivery is rejected below the application: the
    // peer observes no further bytes (short bounded read), and the
    // existing typed delivery-failure semantics terminate the sender
    // connection (and then its session) instead of retrying
    // silently. Session teardown removes the cumulative delivery
    // counters, so typed accounting is observed through the captured
    // driver degradation log plus the deterministic terminal path —
    // never through a racy post-teardown counter read.
    let tail = read_until_eof(&mut b_raw, Duration::from_secs(5)).await;
    assert!(
        tail.is_empty(),
        "corrupted material reached the application: {} bytes",
        tail.len()
    );
    let counters = state.fault_counters();
    assert_eq!(
        counters.corrupted, 1,
        "corruption fault never fired: {counters:?}"
    );
    assert!(
        wait_for_condition(LIFECYCLE_POLL_TIMEOUT, || {
            let alpha = i2pr_api::SamSessionId::new("alpha").expect("session id");
            state.session_registry().get(&alpha).is_none()
        })
        .await,
        "corrupted delivery did not terminate the sender session"
    );

    drop(a_raw);
    drop(b_raw);
    assert!(
        wait_for_condition(LIFECYCLE_POLL_TIMEOUT, || {
            state.stream_registry().attachment_count() == 0
                && state.session_registry().session_count() == 0
        })
        .await,
        "corruption lifecycle did not return to baseline"
    );

    let logs = String::from_utf8_lossy(&capture.buffer.lock().expect("logs").clone()).into_owned();
    assert!(
        logs.contains("local-delivery degradation"),
        "driver never logged typed delivery degradation for the rejected delivery"
    );
    let sentinel_text = String::from_utf8_lossy(&sentinel);
    assert!(
        !logs.contains(sentinel_text.as_ref()),
        "default logs leaked application payload bytes"
    );
    assert!(
        !logs.contains("PLAN151-AFTER-CORRUPTION"),
        "default logs leaked post-corruption payload bytes"
    );
    drop(_log_guard);
    shutdown_and_assert_baselines(&state, scope, parent).await;
}

#[tokio::test(flavor = "current_thread")]
async fn plan151_fault_retransmit_ceiling_terminates_bounded() {
    let mut profile = SamDeliveryFaultProfile::disabled();
    profile.arm_drop_all_data_ack();
    let (state, address, scope, parent) = start_listener(sam_config(), Some(profile)).await;
    let (mut a_raw, mut b_raw) = establish_pair(address).await;
    let counters = state.fault_counters();
    assert!(
        counters.handshake_passthrough >= 2,
        "stream must establish through the ceiling arm: {counters:?}"
    );
    assert_eq!(established_connection_count(&state, "alpha"), 1);

    // One small payload against persistent non-delivery: the sender
    // exhausts its bounded retransmit budget (1 initial + 8
    // retransmits for one packet) and then goes quiet. No infinite
    // retry, no busy loop.
    let payload = test_pattern(512, 0xC0);
    write_all(&mut a_raw, &payload).await;
    assert!(
        wait_for_condition(Duration::from_secs(90), || {
            state.fault_counters().dropped_ceiling >= 9
        })
        .await,
        "retransmit ceiling never exhausted: {:?}",
        state.fault_counters()
    );
    let first = state.fault_counters().dropped_ceiling;
    tokio::time::sleep(Duration::from_secs(12)).await;
    let second = state.fault_counters().dropped_ceiling;
    assert_eq!(
        first, second,
        "retransmit activity did not plateau after the ceiling (went {first} -> {second})"
    );
    assert!(
        wait_for_condition(LIFECYCLE_POLL_TIMEOUT, || {
            queue_gauges(&state, "alpha").tracked_retransmits == 0
        })
        .await,
        "retransmit tracking did not release after the ceiling"
    );
    // The peer never observed any of the persistently dropped bytes.
    let tail = read_until_eof(&mut b_raw, Duration::from_secs(3)).await;
    assert!(tail.is_empty(), "ceiling-dropped bytes reached the peer");

    // Disarm so CLOSE/RESET control flows normally, then prove the
    // stream still closes cleanly and releases every resource.
    state.disarm_test_fault_ceiling();
    drop(a_raw);
    let peer_tail = read_until_eof(&mut b_raw, LIFECYCLE_POLL_TIMEOUT).await;
    assert!(
        peer_tail.is_empty(),
        "unexpected bytes after ceiling teardown: {}",
        peer_tail.len()
    );
    drop(b_raw);
    assert!(
        wait_for_condition(LIFECYCLE_POLL_TIMEOUT, || {
            state.stream_registry().attachment_count() == 0
                && state.session_registry().session_count() == 0
        })
        .await,
        "ceiling lifecycle did not return to baseline"
    );
    shutdown_and_assert_baselines(&state, scope, parent).await;
}

#[tokio::test(flavor = "current_thread")]
async fn plan151_close_reset_lifecycle() {
    // Observe-only profile: wire CLOSE/RESET control is counted
    // while behavior stays fully normal.
    let mut observe = SamDeliveryFaultProfile::disabled();
    observe.arm_control_observability();
    let (state, address, scope, parent) = start_listener(sam_config(), Some(observe)).await;

    // Case 1: graceful local EOF delivers accepted bytes then EOF.
    let (mut a_raw, mut b_raw) = establish_pair(address).await;
    let graceful = test_pattern(1024, 0x60);
    write_all(&mut a_raw, &graceful).await;
    a_raw.shutdown().await.expect("shutdown write half");
    let mut received = vec![0_u8; graceful.len()];
    tokio::time::timeout(SMALL_TRANSFER_TIMEOUT, b_raw.read_exact(&mut received))
        .await
        .expect("bounded graceful read")
        .expect("read_exact");
    assert_eq!(received, graceful, "graceful close lost accepted bytes");
    let tail = read_until_eof(&mut b_raw, LIFECYCLE_POLL_TIMEOUT).await;
    assert!(tail.is_empty(), "graceful close emitted trailing bytes");
    drop(a_raw);
    drop(b_raw);

    // Case 2: abortive close (RST) drives the RESET path. Beta
    // offers bulk that alpha never reads, so alpha's kernel socket
    // holds unread received bytes; dropping alpha's socket then
    // emits a TCP RST (close-with-unread-data). The daemon-side pump
    // observes the reset, queues a Streaming RESET through the still
    // installed bridge, and the fault counters observe the wire
    // RESET control packet — proving the abrupt-failure path
    // executed rather than a graceful close.
    let (a_two, b_two) = establish_pair(address).await;
    let (mut b_two_read, mut b_two_write) = b_two.into_split();
    let abort_bulk = test_pattern(2 * 1024 * 1024, 0x61);
    let abort_writer = tokio::spawn(async move {
        let _ = b_two_write.write_all(&abort_bulk).await;
        let _ = b_two_write.flush().await;
        tokio::time::sleep(Duration::from_secs(3600)).await;
    });
    // Let enough bulk accumulate that alpha's socket holds unread
    // bytes even after kernel buffering.
    tokio::time::sleep(Duration::from_secs(8)).await;
    drop(a_two);
    // Beta observes prompt bounded termination after the peer reset.
    let tail_two = read_until_eof(&mut b_two_read, LIFECYCLE_POLL_TIMEOUT).await;
    let _ = tail_two;
    abort_writer.abort();
    drop(b_two_read);
    assert!(
        wait_for_condition(LIFECYCLE_POLL_TIMEOUT, || {
            state.stream_registry().attachment_count() == 0
        })
        .await,
        "abortive close did not release its attachment"
    );
    assert!(
        state.fault_counters().close_reset_passthrough >= 1,
        "no wire RESET observed for the abortive close: {:?}",
        state.fault_counters()
    );

    // Case 3: repeated create/connect/close cycles return every
    // registry to baseline without a supervisor restart.
    for round in 0..5_u32 {
        let (mut a_raw, mut b_raw) = establish_pair(address).await;
        let payload = test_pattern(1024, 0x62 + round);
        send_exact_one_way(&mut a_raw, &mut b_raw, &payload).await;
        drop(a_raw);
        drop(b_raw);
        assert!(
            wait_for_condition(LIFECYCLE_POLL_TIMEOUT, || {
                state.stream_registry().attachment_count() == 0
                    && state.session_registry().session_count() == 0
                    && state.stream_registry().active_session_count() == 0
            })
            .await,
            "lifecycle cycle {round} did not return to baseline"
        );
    }

    // Case 4: supervisor teardown with an active stream terminates
    // children within the shutdown bound. The sockets stay open on
    // purpose: cancellation (not TCP EOF) must drive every raw
    // driver, destination driver, and listener task to release.
    let (mut a_live, _b_live) = establish_pair(address).await;
    let live_payload = test_pattern(512, 0x6F);
    write_all(&mut a_live, &live_payload).await;
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    tokio::time::timeout(Duration::from_secs(30), scope.shutdown())
        .await
        .expect("shutdown with an active stream exceeded its bound");
    assert_eq!(state.session_registry().session_count(), 0);
    assert_eq!(state.destination_registry().lock().unwrap().len(), 0);
    assert_eq!(state.streaming_pools().lock().unwrap().len(), 0);
    assert_eq!(state.stream_registry().active_session_count(), 0);
    assert_eq!(state.stream_registry().attachment_count(), 0);
}
