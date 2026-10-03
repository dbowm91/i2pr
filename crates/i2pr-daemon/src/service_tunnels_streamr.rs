//! Plan 291 Streamr media subscriber/publisher executors.
//!
//! Java I2PTunnel Streamr (`net.i2p.i2ptunnel.streamr`) couples a
//! repliable-datagram control plane (one-byte subscribe `0x00` /
//! unsubscribe `0x01` over Datagram1, protocol 17) with an
//! unauthenticated raw-datagram media plane (protocol 18):
//!
//! ```text
//! streamrclient: I2P client, no persistent key; UDP side sends
//!   received media to a configured loopback target; subscribes on
//!   a bounded cadence (fast-start 2 s x5, then steady 10 s) and
//!   unsubscribes once on shutdown.
//! streamrserver: I2P server with a persistent destination; UDP
//!   side receives media on a configured loopback port; keeps a
//!   bounded authenticated subscriber table (10 entries, 60 s
//!   expiry, ports swapped for replies) and fans raw media out.
//! ```
//!
//! Both halves compose the runtime-neutral
//! [`i2pr_client::datagram`] substrate with daemon-owned loopback
//! UDP sockets. No TCP listeners or targets exist on either half;
//! UDP endpoints are validated loopback by spec construction. See
//! `specs/protocols/12-repliable-datagrams-streamr.md` for the
//! frozen behavior reference.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use i2pr_client::datagram::{DATAGRAM1_PROTOCOL, DatagramSendRequest, RAW_DATAGRAM_PROTOCOL};
use i2pr_client::streaming::manager::RemoteDestination;
use i2pr_runtime::CancellationToken;
use i2pr_service_tunnels::StreamrOptions;
use tokio::net::UdpSocket;
use tracing::{debug, warn};

use crate::service_tunnels::{
    ServiceRuntime, ServiceTunnelError, ServiceTunnelManager, service_streaming_now_ms,
};

/// Poll cadence for datagram drains and subscription expiry.
const STREAMR_POLL: Duration = Duration::from_millis(50);
/// UDP media read chunk: one read is at most one datagram; larger
/// reads are rejected, never split.
const UDP_READ_CHUNK: usize = 2048;
/// Subscribe control payload: subscribe.
const SUBSCRIBE: &[u8] = &[0x00];
/// Subscribe control payload: unsubscribe.
const UNSUBSCRIBE: &[u8] = &[0x01];

/// One authenticated subscriber: the producer replies to the
/// swapped port pair (Java `MSink` semantics).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct SubscriberKey {
    /// SHA-256 hash of the subscriber destination.
    destination_hash: [u8; 32],
    /// I2P source port for replies (subscriber's destination port).
    source_port: u16,
    /// I2P destination port for replies (subscriber's source port).
    destination_port: u16,
}

/// Outcome of one Streamr connection-task step (typed for tests).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamrLoopOutcome {
    /// The loop exited on cancellation or drain.
    Stopped,
    /// Configuration or bind failure before the loop started.
    Failed,
}

/// Sends one datagram through the destination manager and wakes
/// the delivery driver. Returns `false` when the destination
/// bridge is gone.
fn admit_datagram(
    manager: &ServiceTunnelManager,
    destination_id: i2pr_client::DestinationId,
    request: DatagramSendRequest,
) -> bool {
    let admitted = manager.with_destination_bridge(destination_id, |bridge| {
        let identity = bridge.identity();
        bridge
            .datagrams_mut()
            .send(identity.as_ref(), &request)
            .is_ok()
    });
    if admitted.unwrap_or(false) {
        manager.notify_outbound_signal(destination_id);
        true
    } else {
        false
    }
}

/// Runs the per-service supervisor loop for a Streamr publisher:
/// loopback UDP media source plus the authenticated subscriber
/// table under one supervisor task, one generation, and one
/// persistent destination identity.
pub async fn run_streamr_server_loop(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    spec: &i2pr_service_tunnels::ServiceTunnelSpec,
    cancellation: &CancellationToken,
) -> Result<(), ServiceTunnelError> {
    let options = spec.streamr_options.ok_or_else(|| {
        ServiceTunnelError::InvalidConfig("streamr-server missing streamr_options".to_owned())
    })?;
    let local_udp = options.local_udp.ok_or_else(|| {
        ServiceTunnelError::InvalidConfig("streamr-server missing loopback UDP source".to_owned())
    })?;
    if !local_udp.ip().is_loopback() {
        return Err(ServiceTunnelError::InvalidConfig(
            "streamr-server UDP source must be loopback".to_owned(),
        ));
    }
    let socket = UdpSocket::bind(local_udp)
        .await
        .map_err(|error| ServiceTunnelError::Bind(format!("streamr-server UDP bind: {error}")))?;
    tracing::info!(
        service = %spec.id.as_str(),
        udp = %socket.local_addr().map(|addr| addr.to_string()).unwrap_or_default(),
        "streamr server bound loopback UDP media source"
    );
    let outcome = drive_streamr_server(manager, runtime, socket, &options, cancellation).await;
    if outcome == StreamrLoopOutcome::Failed {
        runtime_failed(runtime);
    }
    Ok(())
}

/// Drives one Streamr publisher: UDP media in, raw fanout per
/// subscriber, subscribe/unsubscribe intake, expiry sweep.
async fn drive_streamr_server(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    socket: UdpSocket,
    options: &StreamrOptions,
    cancellation: &CancellationToken,
) -> StreamrLoopOutcome {
    let drain_cancel = runtime.cancellation_token();
    let mut subscribers: HashMap<SubscriberKey, u64> = HashMap::new();
    let mut chunk = [0_u8; UDP_READ_CHUNK];
    let mut ticker = tokio::time::interval(STREAMR_POLL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await;
    loop {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => break,
            _ = drain_cancel.cancelled() => break,
            received = socket.recv_from(&mut chunk) => {
                let (length, _peer) = match received {
                    Ok(value) => value,
                    Err(error) => {
                        debug!(service = %runtime.spec_id, error = %error, "streamr UDP recv failed");
                        continue;
                    }
                };
                // The media source is the bound loopback socket's
                // peer space; any loopback sender may feed it (the
                // operator points their media source here). Payload
                // over the ceiling is rejected, never fragmented.
                if length > options.payload_limit_bytes {
                    debug!(
                        service = %runtime.spec_id,
                        length,
                        "streamr media over ceiling dropped"
                    );
                    continue;
                }
                if subscribers.is_empty() {
                    continue;
                }
                let media = chunk[..length].to_vec();
                for key in subscribers.keys() {
                    let request = DatagramSendRequest {
                        destination_hash: key.destination_hash,
                        source_port: key.source_port,
                        destination_port: key.destination_port,
                        protocol: RAW_DATAGRAM_PROTOCOL,
                        payload: media.clone(),
                    };
                    if !admit_datagram(manager, runtime.destination_id, request) {
                        debug!(
                            service = %runtime.spec_id,
                            "streamr fanout admission failed"
                        );
                    }
                }
            }
            _ = ticker.tick() => {
                // Intake subscribes first so a just-arrived
                // subscriber joins before the next media read.
                let drained = manager
                    .with_destination_bridge(runtime.destination_id, |bridge| {
                        bridge.datagrams_mut().drain_received()
                    })
                    .unwrap_or_default();
                for event in drained {
                    if event.protocol != DATAGRAM1_PROTOCOL {
                        continue;
                    }
                    if handle_subscribe(
                        runtime.spec_id.as_str(),
                        &mut subscribers,
                        options,
                        &event,
                        service_streaming_now_ms(),
                    ) {
                        // Subscription intake is publisher-visible
                        // liveness (Plan 292 idle sweep input).
                        runtime.streamr_subscribers.store(
                            subscribers.len(),
                            std::sync::atomic::Ordering::Relaxed,
                        );
                        runtime.note_activity(service_streaming_now_ms());
                    }
                }
                let before = subscribers.len();
                sweep_expired(&mut subscribers, options, service_streaming_now_ms());
                if subscribers.len() != before {
                    runtime.streamr_subscribers.store(
                        subscribers.len(),
                        std::sync::atomic::Ordering::Relaxed,
                    );
                    runtime.note_activity(service_streaming_now_ms());
                }
            }
        }
    }
    StreamrLoopOutcome::Stopped
}

/// Applies one subscribe/unsubscribe control event to the
/// subscriber table. Anything that is not exactly one byte
/// `0x00`/`0x01` is dropped: the substrate authenticates, this
/// layer enforces the control vocabulary.
fn handle_subscribe(
    service_id: &str,
    subscribers: &mut HashMap<SubscriberKey, u64>,
    options: &StreamrOptions,
    event: &i2pr_client::datagram::DatagramReceiveEvent,
    now_ms: u64,
) -> bool {
    if event.payload.len() != 1 {
        debug!(
            service = service_id,
            "streamr control with wrong length dropped"
        );
        return false;
    }
    // Replies swap the observed port pair (Java `MSink`).
    let key = SubscriberKey {
        destination_hash: event.from_hash,
        source_port: event.destination_port,
        destination_port: event.source_port,
    };
    match event.payload[0] {
        0x00 => {
            if subscribers.contains_key(&key) {
                subscribers.insert(key, now_ms);
                true
            } else if subscribers.len() >= options.max_subscribers {
                warn!(
                    service = service_id,
                    "streamr subscriber table full; denying subscription"
                );
                false
            } else {
                subscribers.insert(key, now_ms);
                debug!(service = service_id, "streamr subscription added");
                true
            }
        }
        0x01 => {
            if subscribers.remove(&key).is_some() {
                debug!(service = service_id, "streamr subscription removed");
                true
            } else {
                false
            }
        }
        other => {
            debug!(
                service = service_id,
                flag = other,
                "streamr control with bad flag dropped"
            );
            false
        }
    }
}

/// Removes subscriptions whose last refresh predates the expiry.
fn sweep_expired(
    subscribers: &mut HashMap<SubscriberKey, u64>,
    options: &StreamrOptions,
    now_ms: u64,
) {
    subscribers
        .retain(|_, refreshed| now_ms.saturating_sub(*refreshed) < options.subscription_expiry_ms);
}

/// Returns whether one drained event is forwardable producer
/// media: raw protocol, exact producer binding, within the
/// payload ceiling. Anything else is dropped so a confused
/// deputy can never steer local UDP.
fn is_producer_media(
    event: &i2pr_client::datagram::DatagramReceiveEvent,
    producer_hash: &[u8; 32],
    options: &StreamrOptions,
) -> bool {
    event.protocol == RAW_DATAGRAM_PROTOCOL
        && event.from_hash == *producer_hash
        && event.payload.len() <= options.payload_limit_bytes
}

/// Runs the per-service supervisor loop for a Streamr subscriber:
/// bounded subscribe cadence toward the configured producer,
/// producer-bound raw media forwarding to the loopback UDP target,
/// and one best-effort unsubscribe on shutdown.
pub async fn run_streamr_client_loop(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    spec: &i2pr_service_tunnels::ServiceTunnelSpec,
    cancellation: &CancellationToken,
) -> Result<(), ServiceTunnelError> {
    let options = spec.streamr_options.ok_or_else(|| {
        ServiceTunnelError::InvalidConfig("streamr-client missing streamr_options".to_owned())
    })?;
    let local_udp = options.local_udp.ok_or_else(|| {
        ServiceTunnelError::InvalidConfig("streamr-client missing loopback UDP target".to_owned())
    })?;
    if !local_udp.ip().is_loopback() {
        return Err(ServiceTunnelError::InvalidConfig(
            "streamr-client UDP target must be loopback".to_owned(),
        ));
    }
    // Plan 292: a configured sink redirect receives the media
    // instead of the local target; the socket binds the local
    // media host with an ephemeral port either way
    // (loopback-confined; validated at the spec boundary).
    let remote_udp = options
        .remote_sink
        .or(Some(local_udp))
        .expect("local UDP set");
    if !remote_udp.ip().is_loopback() {
        return Err(ServiceTunnelError::InvalidConfig(
            "streamr-client UDP sink must be loopback".to_owned(),
        ));
    }
    let destination_ref = spec.destination.clone().ok_or_else(|| {
        ServiceTunnelError::InvalidConfig("streamr-client missing producer destination".to_owned())
    })?;
    let producer = manager.resolve_reference(&destination_ref).map_err(|_| {
        ServiceTunnelError::InvalidConfig(
            "streamr-client producer destination does not resolve".to_owned(),
        )
    })?;
    // The UDP socket is send-only (media toward the sink); its
    // bound port anchors the subscribe `fromPort` (Java `UDPSink`
    // parity). The bind host follows the configured local media
    // host so multi-home loopback setups keep working.
    let socket = UdpSocket::bind(SocketAddr::new(local_udp.ip(), 0))
        .await
        .map_err(|error| ServiceTunnelError::Bind(format!("streamr-client UDP bind: {error}")))?;
    let from_port = socket.local_addr().map(|addr| addr.port()).unwrap_or(0);
    tracing::info!(
        service = %spec.id.as_str(),
        target = %remote_udp,
        "streamr client bound loopback UDP socket"
    );
    let outcome = drive_streamr_client(&ClientDrive {
        manager,
        runtime,
        socket: &socket,
        options,
        producer: producer.remote.clone(),
        remote_udp,
        from_port,
        cancellation: cancellation.clone(),
    })
    .await;
    if outcome == StreamrLoopOutcome::Failed {
        runtime_failed(runtime);
    }
    Ok(())
}

/// Owned drive context for one Streamr subscriber loop (keeps
/// the per-tick function under the argument ceiling).
struct ClientDrive<'a> {
    manager: &'a Arc<ServiceTunnelManager>,
    runtime: &'a Arc<ServiceRuntime>,
    socket: &'a UdpSocket,
    options: StreamrOptions,
    producer: RemoteDestination,
    remote_udp: SocketAddr,
    from_port: u16,
    cancellation: CancellationToken,
}

/// Drives one Streamr subscriber: cadence subscribes, media
/// forwarding, terminal unsubscribe.
async fn drive_streamr_client(drive: &ClientDrive<'_>) -> StreamrLoopOutcome {
    use i2pr_service_tunnels::streamr::{
        SUBSCRIBE_FAST_START_COUNT, SUBSCRIBE_FAST_START_INTERVAL_MS,
    };
    let manager = drive.manager;
    let runtime = drive.runtime;
    let socket = drive.socket;
    let options = &drive.options;
    let producer = &drive.producer;
    let remote_udp = drive.remote_udp;
    let from_port = drive.from_port;
    let cancellation = &drive.cancellation;
    let drain_cancel = runtime.cancellation_token();
    let mut ticker = tokio::time::interval(STREAMR_POLL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await;
    let mut subscribes_sent = 0_usize;
    let mut next_subscribe_in_ticks = 0_usize;
    // Fast-start ticks at the 50 ms poll cadence: 2 s = 40 ticks.
    let fast_ticks = (SUBSCRIBE_FAST_START_INTERVAL_MS / STREAMR_POLL.as_millis() as u64) as usize;
    let steady_ticks =
        (options.subscribe_interval_ms / STREAMR_POLL.as_millis() as u64).max(1) as usize;
    loop {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => break,
            _ = drain_cancel.cancelled() => break,
            _ = ticker.tick() => {
                // Bounded subscribe cadence: fast-start burst,
                // then the configured steady interval.
                if next_subscribe_in_ticks == 0 {
                    let request = DatagramSendRequest {
                        destination_hash: producer.destination_hash,
                        source_port: from_port,
                        destination_port: options.target_i2p_port,
                        protocol: DATAGRAM1_PROTOCOL,
                        payload: SUBSCRIBE.to_vec(),
                    };
                    if admit_datagram(manager, runtime.destination_id, request) {
                        subscribes_sent = subscribes_sent.saturating_add(1);
                    } else {
                        debug!(
                            service = %runtime.spec_id,
                            "streamr subscribe admission failed"
                        );
                    }
                    next_subscribe_in_ticks = if subscribes_sent < SUBSCRIBE_FAST_START_COUNT {
                        fast_ticks.max(1)
                    } else {
                        steady_ticks
                    };
                } else {
                    next_subscribe_in_ticks = next_subscribe_in_ticks.saturating_sub(1);
                }
                // Media intake: forward only producer-bound raw
                // media to the configured loopback target (see
                // `is_producer_media`).
                for event in manager
                    .with_destination_bridge(runtime.destination_id, |bridge| {
                        bridge.datagrams_mut().drain_received()
                    })
                    .unwrap_or_default()
                {
                    if !is_producer_media(&event, &producer.destination_hash, options) {
                        continue;
                    }
                    // Producer-bound media arrival is subscriber
                    // liveness (Plan 292 idle sweep input).
                    runtime.note_activity(service_streaming_now_ms());
                    if let Err(error) = socket.send_to(&event.payload, remote_udp).await {
                        debug!(
                            service = %runtime.spec_id,
                            error = %error,
                            "streamr media UDP send failed"
                        );
                    }
                }
            }
        }
    }
    // Deterministic shutdown: one best-effort unsubscribe so the
    // producer releases the slot without waiting for expiry.
    let bye = DatagramSendRequest {
        destination_hash: producer.destination_hash,
        source_port: from_port,
        destination_port: options.target_i2p_port,
        protocol: DATAGRAM1_PROTOCOL,
        payload: UNSUBSCRIBE.to_vec(),
    };
    let _ = admit_datagram(manager, runtime.destination_id, bye);
    StreamrLoopOutcome::Stopped
}

/// Records a loop failure on the service counters.
fn runtime_failed(runtime: &ServiceRuntime) {
    runtime
        .failed_connects
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_client::datagram::DatagramReceiveEvent;

    fn control_event(hash: [u8; 32], payload: Vec<u8>) -> DatagramReceiveEvent {
        DatagramReceiveEvent {
            from_hash: hash,
            source_port: 5001,
            destination_port: 0,
            protocol: DATAGRAM1_PROTOCOL,
            payload,
            received_at_ms: 0,
        }
    }

    #[test]
    fn subscribe_refresh_unsubscribe_and_expiry() {
        let options = StreamrOptions::default();
        let mut table = HashMap::new();
        let hash = [0x11_u8; 32];
        // Subscribe adds with swapped ports.
        handle_subscribe(
            "test",
            &mut table,
            &options,
            &control_event(hash, vec![0x00]),
            1_000,
        );
        assert_eq!(table.len(), 1);
        let key = SubscriberKey {
            destination_hash: hash,
            source_port: 0,
            destination_port: 5001,
        };
        assert_eq!(table.get(&key), Some(&1_000));
        // Refresh updates the timestamp.
        handle_subscribe(
            "test",
            &mut table,
            &options,
            &control_event(hash, vec![0x00]),
            2_000,
        );
        assert_eq!(table.get(&key), Some(&2_000));
        // Fresh entries survive the sweep; stale ones go.
        sweep_expired(&mut table, &options, 2_000 + 59_999);
        assert_eq!(table.len(), 1);
        sweep_expired(&mut table, &options, 2_000 + 60_000);
        assert!(table.is_empty());
        // Unsubscribe removes.
        handle_subscribe(
            "test",
            &mut table,
            &options,
            &control_event(hash, vec![0x00]),
            3_000,
        );
        assert_eq!(table.len(), 1);
        handle_subscribe(
            "test",
            &mut table,
            &options,
            &control_event(hash, vec![0x01]),
            3_001,
        );
        assert!(table.is_empty());
    }

    #[test]
    fn table_cap_denies_the_eleventh_java_parity() {
        let options = StreamrOptions::default();
        let mut table = HashMap::new();
        for index in 0..10_u8 {
            let mut hash = [0_u8; 32];
            hash[0] = index;
            handle_subscribe(
                "test",
                &mut table,
                &options,
                &control_event(hash, vec![0x00]),
                1_000,
            );
        }
        assert_eq!(table.len(), 10);
        handle_subscribe(
            "test",
            &mut table,
            &options,
            &control_event([0xFF_u8; 32], vec![0x00]),
            1_000,
        );
        assert_eq!(table.len(), 10, "eleventh subscription denied");
    }

    #[test]
    fn malformed_control_never_subscribes() {
        let options = StreamrOptions::default();
        let mut table = HashMap::new();
        let hash = [0x22_u8; 32];
        // Wrong length.
        handle_subscribe(
            "test",
            &mut table,
            &options,
            &control_event(hash, vec![0x00, 0x01]),
            1_000,
        );
        // Bad flag.
        handle_subscribe(
            "test",
            &mut table,
            &options,
            &control_event(hash, vec![0x02]),
            1_000,
        );
        assert!(table.is_empty());
    }

    #[test]
    fn producer_media_binding_is_exact() {
        let options = StreamrOptions::default();
        let producer = [0x33_u8; 32];
        let good = DatagramReceiveEvent {
            from_hash: producer,
            source_port: 0,
            destination_port: 5001,
            protocol: RAW_DATAGRAM_PROTOCOL,
            payload: vec![0x55_u8; 100],
            received_at_ms: 0,
        };
        assert!(is_producer_media(&good, &producer, &options));
        // Wrong sender.
        let mut bad = good.clone();
        bad.from_hash = [0x34_u8; 32];
        assert!(!is_producer_media(&bad, &producer, &options));
        // Wrong protocol (even repliable control from the producer
        // is not media).
        let mut bad = good.clone();
        bad.protocol = DATAGRAM1_PROTOCOL;
        assert!(!is_producer_media(&bad, &producer, &options));
        // Over ceiling.
        let mut bad = good.clone();
        bad.payload = vec![0x55_u8; 1201];
        assert!(!is_producer_media(&bad, &producer, &options));
    }
}
