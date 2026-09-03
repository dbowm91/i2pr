//! Plan 147 SAM 3.1 dedicated raw STREAM socket driver.
//!
//! The Plan 143 command-mode socket handling was a regression: it
//! marked the SAM attachment `Established` after a SYN was queued,
//! the production CONNECT path used a deterministic `ChaCha8Rng`,
//! and no TCP <-> `StreamingManager` loop existed. This module owns
//! the corrected product path:
//!
//! ```text
//!  SAM STREAM socket (TCP)
//!      |
//!      v  HELLO + STREAM CONNECT or STREAM ACCEPT
//!  command-mode connection task
//!      |   (waits for the real `ConnectionState::Established`)
//!      v
//!  raw-mode RawStreamHandoff  (sole owner of TcpStream)
//!      |
//!      v
//!  RawStreamDriver
//!      |   inbound  TCP -> StreamingManager::send_data
//!      |   outbound StreamingManager::drain_delivered -> TCP
//!      v
//!  StreamingDestinationAdapter / i2pr_client::deliver
//!      |
//!      v
//!  peer StreamingManager
//! ```
//!
//! Ownership is transferred at exactly one point (the
//! `RawStreamHandoff` produced by `execute_stream_connect` /
//! `execute_stream_accept`); before the handoff the connection
//! task owns the `TcpStream`; after the handoff the
//! `RawStreamDriver` owns it and the command-mode task no
//! longer has any reference to the socket. The same applies to
//! the `LineReader`: it is dropped at the handoff and no
//! subsequent byte can ever be parsed as a SAM command line.

#![forbid(unsafe_code)]

use std::sync::Arc;

use i2pr_api::sam::session::SamSessionId;
use i2pr_client::DestinationId;
use i2pr_client::streaming::connection::{ConnectionId, ConnectionState};
use i2pr_client::streaming::manager::RemoteDestination;
use i2pr_runtime::CancellationToken;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, warn};

use crate::sam::{SamServiceState, sam_now_seconds, streaming_now_ms};

/// Direction of the underlying Streaming connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawDirection {
    /// `STREAM CONNECT` originated the connection.
    Outbound,
    /// `STREAM ACCEPT` was the local endpoint of the connection.
    Inbound,
}

/// Snapshot of the raw-mode handoff at the command->raw transition.
///
/// Exactly one `RawStreamHandoff` is produced per successful
/// `STREAM CONNECT` or `STREAM ACCEPT`; the command-mode connection
/// task is responsible for moving the entire `TcpStream` plus its
/// `LineReader::take_buffered` output into the handoff. After the
/// handoff the connection task no longer holds the socket.
pub struct RawStreamHandoff {
    /// Owned TCP socket. The command-mode task relinquishes every
    /// reference to this socket before constructing the handoff.
    pub stream: TcpStream,
    /// Owning SAM session identifier.
    pub session_id: SamSessionId,
    /// Owning local destination identifier.
    pub destination_id: DestinationId,
    /// Allocated SAM stream attachment id.
    pub attachment_id: u32,
    /// Streaming connection id on the local destination's
    /// `StreamingManager`.
    pub connection_id: ConnectionId,
    /// Resolved peer destination (CONNECT supplied, ACCEPT learned
    /// from the inbound SYN).
    pub peer_destination: RemoteDestination,
    /// Bytes already buffered by `LineReader` after the command
    /// newline; the raw driver emits them as the first TCP->Streaming
    /// payload before reading any further socket data.
    pub initial_raw_bytes: Vec<u8>,
    /// `true` when the SAM `SILENT=true` option was supplied.
    pub silent: bool,
    /// `Outbound` for `STREAM CONNECT`, `Inbound` for `STREAM ACCEPT`.
    pub direction: RawDirection,
}

impl std::fmt::Debug for RawStreamHandoff {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RawStreamHandoff")
            .field("stream", &"<redacted>")
            .field("session_id", &self.session_id)
            .field("destination_id", &self.destination_id)
            .field("attachment_id", &self.attachment_id)
            .field("connection_id", &self.connection_id)
            .field("peer_destination", &"<redacted>")
            .field("initial_raw_bytes", &self.initial_raw_bytes.len())
            .field("silent", &self.silent)
            .field("direction", &self.direction)
            .finish()
    }
}

/// Outcome of a raw-mode handshake attempt surfaced to the
/// connection task. The connection task converts this into a SAM
/// `STREAM STATUS RESULT=...` line.
#[derive(Clone, Debug)]
pub enum RawStreamOutcome {
    /// The local `StreamingManager` reached `Established` (or the
    /// inbound SYN was accepted and the SYN response was queued).
    /// The connection task should write the SAM `STREAM STATUS
    /// RESULT=OK` line and hand the socket to the raw driver.
    Established {
        /// Allocated SAM stream attachment id.
        attachment_id: u32,
        /// Resolved peer destination (when known). For ACCEPT this
        /// is the authenticated peer identity from the inbound SYN.
        peer_destination: Option<RemoteDestination>,
    },
    /// The handshake could not complete in time. The connection
    /// task should write a typed SAM error and close the socket.
    TimedOut,
    /// The runtime rejected the handshake with a typed SAM
    /// `ReplyResult`.
    Failed {
        /// SAM-side reply code.
        result: i2pr_api::sam::reply::ReplyResult,
        /// Human-readable diagnostic.
        message: String,
    },
}

/// SAM-aware raw-mode outcome tagged with the final handshake
/// disposition. The driver task consumes this and proceeds.
pub struct RawStreamHandoffResolved {
    /// The handoff struct itself (socket, peer, initial bytes).
    pub handoff: RawStreamHandoff,
    /// Final handshake outcome.
    pub outcome: RawStreamOutcome,
    /// Peer destination b64 used to write the SAM `DESTINATION=...`
    /// ACCEPT line when the connection is not silent.
    pub peer_destination_b64: Option<String>,
}

impl std::fmt::Debug for RawStreamHandoffResolved {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RawStreamHandoffResolved")
            .field("outcome", &self.outcome)
            .field(
                "peer_destination_b64",
                &self.peer_destination_b64.as_deref().map(|_| "<redacted>"),
            )
            .finish_non_exhaustive()
    }
}

/// Metadata retained by the command-mode task so the raw driver can
/// release its Streaming and SAM ownership after the TCP socket exits.
#[derive(Clone, Debug)]
pub(crate) struct RawStreamCleanup {
    pub(crate) session_id: SamSessionId,
    pub(crate) destination_id: DestinationId,
    pub(crate) attachment_id: u32,
    pub(crate) connection_id: ConnectionId,
    pub(crate) peer_destination: RemoteDestination,
    pub(crate) direction: RawDirection,
}

/// Spins one raw-mode TCP <-> StreamingManager byte pump for the
/// supplied handoff. The driver terminates when:
/// - the parent cancellation token fires (parent socket close,
///   service shutdown, SAM session teardown);
/// - the TCP read returns EOF or I/O error;
/// - the local application closes the connection;
/// - the underlying `StreamingManager` connection becomes terminal.
///
/// The driver does **not** own the `StreamingManager`; it borrows
/// it through the bridge handle the runtime driver keeps alive.
pub async fn run_raw_stream(
    state: Arc<SamServiceState>,
    handoff: RawStreamHandoff,
    cancellation: CancellationToken,
) -> Result<(), RawStreamError> {
    let RawStreamHandoff {
        mut stream,
        session_id,
        destination_id,
        attachment_id,
        connection_id,
        peer_destination,
        initial_raw_bytes,
        silent: _silent,
        direction,
    } = handoff;

    debug!(
        session_id = %session_id,
        destination = ?destination_id,
        attachment_id = attachment_id,
        initial_bytes = initial_raw_bytes.len(),
        "raw stream driver started"
    );

    // Bound on the per-iteration TCP read chunk. The send-window
    // admission is also bounded; the two together enforce the
    // Plan 147 backpressure contract.
    let max_chunk = state
        .limits()
        .max_buffered_bytes_per_stream_direction
        .clamp(1, 32 * 1024);
    let mut chunk = vec![0_u8; max_chunk];
    let mut carry: Vec<u8> = initial_raw_bytes;
    let mut eof = false;
    // Plan 147: the TCP read must not indefinitely starve the
    // Streaming->TCP drain. The test runs payloads in both
    // directions simultaneously; if A blocks on TCP read while B's
    // data sits in A's StreamingManager, EOF is never observed.
    // A 20 ms read timeout lets the loop periodically drain
    // without adding a new Notify.
    let mut read_timeout = tokio::time::interval(tokio::time::Duration::from_millis(20));
    read_timeout.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    read_timeout.tick().await;

    while !eof {
        // ----- TCP -> Streaming: read bounded chunk and admit -----
        let mut timed_out = false;
        let mut backpressured = false;
        let read_size = if carry.is_empty() {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                _ = read_timeout.tick() => {
                    // Timeout: fall through to the drain step even
                    // though no TCP byte arrived.
                    timed_out = true;
                    0
                }
                result = stream.read(&mut chunk) => match result {
                    Ok(0) => 0,
                    Ok(n) => n,
                    Err(error) => return Err(RawStreamError::Io(error)),
                },
            }
        } else {
            carry.len()
        };
        if timed_out {
            // No TCP byte, but we may have Streaming->TCP data to
            // forward. Fall through to the drain section without
            // admitting any TCP chunk.
        } else if read_size == 0 && carry.is_empty() {
            eof = true;
        }
        if !eof && !timed_out {
            let payload: Vec<u8> = if carry.is_empty() {
                chunk[..read_size].to_vec()
            } else {
                std::mem::take(&mut carry)
            };

            // Segment the payload to the negotiated Streaming max payload
            // (Plan 147 §8 step 3). Bounded admission. The
            // connection lives on `bridge.streaming` for outbound
            // (CONNECT) attachments and on `bridge.receiver_streaming`
            // for inbound (ACCEPT) attachments because the SYN
            // routing in `LocalDeliveryReceiver::deliver` lands
            // inbound SYNs on the mirror.
            let max_payload = {
                let mut negotiated = i2pr_proto::streaming::DEFAULT_ADVERTISED_MAX_PAYLOAD as usize;
                let destinations = state.sam_destinations();
                let destinations = destinations.lock().expect("sam destinations poisoned");
                if let Some(bridge) = destinations.get(destination_id) {
                    let observed = bridge.with(|b| match direction {
                        RawDirection::Outbound => b
                            .streaming()
                            .get_connection(connection_id)
                            .map(|c| c.max_payload_size() as usize),
                        RawDirection::Inbound => b
                            .receiver_streaming()
                            .get_connection(connection_id)
                            .map(|c| c.max_payload_size() as usize),
                    });
                    if let Some(observed) = observed {
                        negotiated = observed;
                    }
                }
                negotiated.max(1)
            };

            let mut offset = 0_usize;
            while offset < payload.len() {
                let end = (offset + max_payload).min(payload.len());
                let segment = &payload[offset..end];
                let produced = state.send_data_segment(
                    destination_id,
                    connection_id,
                    &peer_destination,
                    segment,
                    direction,
                    streaming_now_ms(),
                );
                match produced {
                    Ok(true) => {
                        offset = end;
                        // Plan 147: wake the per-destination driver
                        // so the STREAMING packet is routed through
                        // `bridge_to_peer` without waiting for the
                        // 250 ms poll.
                        state.notify_outbound_signal(destination_id);
                    }
                    Ok(false) => {
                        // Backpressure: stop reading until the send
                        // window drains. Park the remainder and try
                        // again next iteration.
                        carry = payload[offset..].to_vec();
                        backpressured = true;
                        break;
                    }
                    Err(error) => {
                        warn!(
                            session_id = %session_id,
                            destination = ?destination_id,
                            connection_id = connection_id.raw(),
                            direction = ?direction,
                            initial_bytes = carry.len(),
                            segment_len = segment.len(),
                            error = %error,
                            "raw driver send_data failed"
                        );
                        return Err(RawStreamError::Streaming(error.to_string()));
                    }
                }
            }
            // Give the runtime driver a chance to run `deliver_outbound`
            // before we loop back to TCP read; otherwise, if this task
            // loops fast enough it can starve the driver task on a
            // single-threaded runtime.
            tokio::task::yield_now().await;
        }

        // ----- Streaming -> TCP: drain delivered bytes, write TCP -----
        // Plan 151 sibling isolation: drain only this stream's bytes.
        // A shared whole-queue drain would discard bytes owned by a
        // sibling stream whose ACKs the sender already received.
        let drained = {
            let destinations = state.sam_destinations();
            let destinations = destinations.lock().expect("sam destinations poisoned");
            let bridge = match destinations.get(destination_id) {
                Some(b) => b,
                None => return Ok(()),
            };
            bridge.with(|b| match direction {
                RawDirection::Outbound => b.streaming_mut().drain_delivered_for(connection_id),
                RawDirection::Inbound => b
                    .receiver_streaming_mut()
                    .drain_delivered_for(connection_id),
            })
        };
        for delivered in drained {
            if delivered.bytes.is_empty() {
                continue;
            }
            let write = tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                result = stream.write_all(&delivered.bytes) => result,
            };
            if let Err(error) = write {
                return Err(RawStreamError::Io(error));
            }
            let flush = stream.flush().await;
            if let Err(error) = flush {
                return Err(RawStreamError::Io(error));
            }
        }
        let remote_terminal = {
            let destinations = state.sam_destinations();
            let destinations = destinations
                .lock()
                .map_err(|_| RawStreamError::Streaming("sam destinations poisoned".to_owned()))?;
            let Some(bridge) = destinations.get(destination_id) else {
                return Ok(());
            };
            bridge.with(|bridge| {
                let connection = match direction {
                    RawDirection::Outbound => bridge.streaming().get_connection(connection_id),
                    RawDirection::Inbound => {
                        bridge.receiver_streaming().get_connection(connection_id)
                    }
                };
                connection.is_none_or(|connection| {
                    matches!(
                        connection.state(),
                        ConnectionState::ClosingRemote
                            | ConnectionState::Closed
                            | ConnectionState::Reset
                    )
                })
            })
        };
        if remote_terminal {
            eof = true;
        }
        if backpressured && !carry.is_empty() {
            // A full congestion/send window is an expected flow-control
            // result, not a terminal stream error. Avoid a ready-loop
            // while the peer's delayed ACK timer is running; the short
            // bounded park also gives the per-destination driver a fair
            // opportunity to poll and dispatch that ACK.
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(5)) => {}
            }
        }
    }

    debug!(
        session_id = %session_id,
        destination = ?destination_id,
        attachment_id = attachment_id,
        "raw stream driver reached EOF"
    );
    Ok(())
}

impl SamServiceState {
    /// Removes the local Streaming connection associated with a request
    /// whose local delivery could not be completed. This makes a typed
    /// delivery degradation terminal for the affected raw stream, so a
    /// waiter or byte pump cannot remain parked on a connection whose
    /// packet has already been rejected.
    fn terminate_failed_delivery(
        &self,
        destination_id: DestinationId,
        request: &i2pr_client::streaming::transport::TransportSendRequest,
    ) {
        let destinations_arc = self.sam_destinations();
        let Ok(destinations) = destinations_arc.lock() else {
            return;
        };
        let Some(handle) = destinations.get(destination_id) else {
            return;
        };
        handle.with(|bridge| {
            let stream_id = request.receive_stream_id;
            if let Some(connection_id) = bridge
                .streaming()
                .lookup_outbound(stream_id)
                .or_else(|| bridge.receiver_streaming().lookup_inbound(stream_id))
            {
                if bridge.streaming().get_connection(connection_id).is_some() {
                    let _ = bridge.streaming_mut().remove_connection(connection_id);
                } else {
                    let _ = bridge
                        .receiver_streaming_mut()
                        .remove_connection(connection_id);
                }
            }
        });
    }

    /// Completes ownership cleanup after the raw TCP driver exits. A
    /// normal EOF emits the Streaming CLOSE packet; an I/O or protocol
    /// failure emits RESET. In both cases the local connection and SAM
    /// attachment are then released, while the peer receives the terminal
    /// packet through the same supervised local-delivery path.
    pub(crate) fn finish_raw_stream(&self, cleanup: RawStreamCleanup, reset: bool) {
        let RawStreamCleanup {
            session_id,
            destination_id,
            attachment_id,
            connection_id,
            peer_destination,
            direction,
        } = cleanup;
        let now_ms = streaming_now_ms();
        let terminal_queued = self
            .sam_destinations()
            .lock()
            .ok()
            .and_then(|destinations| {
                let bridge = destinations.get(destination_id)?;
                Some(bridge.with(|bridge| {
                    let identity = bridge.identity();
                    let manager = match direction {
                        RawDirection::Outbound => bridge.streaming_mut(),
                        RawDirection::Inbound => bridge.receiver_streaming_mut(),
                    };
                    let connection = manager.get_connection(connection_id)?;
                    let local_port = connection.local_port();
                    let remote_port = connection.remote_port();
                    let request = if reset {
                        manager.send_reset(
                            connection_id,
                            identity.as_ref(),
                            &peer_destination,
                            local_port,
                            remote_port,
                            now_ms,
                        )
                    } else {
                        manager.send_close(
                            connection_id,
                            identity.as_ref(),
                            &peer_destination,
                            local_port,
                            remote_port,
                            now_ms,
                        )
                    };
                    request.ok()
                }))
            })
            .flatten()
            .is_some();

        if terminal_queued {
            self.notify_outbound_signal(destination_id);
            let _ = self.deliver_outbound(destination_id, sam_now_seconds(), now_ms);
        }

        if let Ok(destinations) = self.sam_destinations().lock()
            && let Some(bridge) = destinations.get(destination_id)
        {
            bridge.with(|bridge| match direction {
                RawDirection::Outbound => {
                    let _ = bridge.streaming_mut().remove_connection(connection_id);
                }
                RawDirection::Inbound => {
                    let _ = bridge
                        .receiver_streaming_mut()
                        .remove_connection(connection_id);
                }
            });
        }
        let _ = self
            .stream_registry()
            .release_attachment(&session_id, attachment_id);
        let forward_active = self
            .stream_registry()
            .inbound_mode(&session_id)
            .is_ok_and(|mode| {
                matches!(mode, i2pr_api::sam::streams::InboundMode::Forwarding { .. })
            });
        if self.stream_registry().attachment_count_for(&session_id) == 0 && !forward_active {
            self.teardown_session(&session_id, destination_id);
        }
    }

    /// Sends one bounded `send_data` segment into the local
    /// `StreamingManager` for the supplied connection. Returns
    /// `true` when the manager accepted the segment, `false` when
    /// the send window rejected it (backpressure). The caller uses
    /// the boolean to throttle the TCP read loop.
    pub fn send_data_segment(
        &self,
        destination_id: DestinationId,
        connection_id: ConnectionId,
        peer: &RemoteDestination,
        payload: &[u8],
        direction: RawDirection,
        now_ms: u64,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let destinations = self.sam_destinations();
        let destinations = destinations.lock().expect("sam destinations poisoned");
        let bridge = match destinations.get(destination_id) {
            Some(bridge) => bridge,
            None => {
                return Err(Box::new(std::io::Error::other(
                    "destination has no installed bridge",
                )));
            }
        };
        let outcome: Result<(), i2pr_client::streaming::manager::StreamingManagerError> = bridge
            .with(|bridge| {
                // Inbound (ACCEPT) attachments own their connection on
                // the receiver-mirror manager; outbound (CONNECT)
                // attachments own it on the canonical manager. Pick the
                // correct one based on the raw direction so that
                // `send_data` finds the connection.
                let (conn_opt, send_through_receiver) = match direction {
                    RawDirection::Outbound => {
                        (bridge.streaming().get_connection(connection_id), false)
                    }
                    RawDirection::Inbound => (
                        bridge.receiver_streaming().get_connection(connection_id),
                        true,
                    ),
                };
                let conn = match conn_opt {
                    Some(conn) => conn,
                    None => {
                        return Err(
                        i2pr_client::streaming::manager::StreamingManagerError::UnknownConnection,
                    );
                    }
                };
                let local_port = conn.local_port();
                let remote_port = conn.remote_port();
                let identity = bridge.identity();
                let result = if send_through_receiver {
                    bridge.receiver_streaming_mut().send_data(
                        connection_id,
                        identity.as_ref(),
                        peer,
                        local_port,
                        remote_port,
                        payload,
                        now_ms,
                    )
                } else {
                    bridge.streaming_mut().send_data(
                        connection_id,
                        identity.as_ref(),
                        peer,
                        local_port,
                        remote_port,
                        payload,
                        now_ms,
                    )
                };
                match result {
                    Ok(_) => Ok(()),
                    Err(error) => Err(error),
                }
            });
        match outcome {
            Ok(()) => Ok(true),
            Err(i2pr_client::streaming::manager::StreamingManagerError::Streaming(
                i2pr_client::streaming::StreamingError::SendWindowFull
                | i2pr_client::streaming::StreamingError::CongestionRejected,
            )) => Ok(false),
            Err(error) => Err(Box::new(error) as Box<dyn std::error::Error + Send + Sync>),
        }
    }

    /// Applies the Plan 151 §8 pre-start fault profile to one drained
    /// sweep. The default profile is inert and returns the input
    /// unchanged. Fault drops remove requests without touching
    /// production delivery counters or connection state.
    fn apply_test_fault_profile(
        &self,
        requests: Vec<i2pr_client::streaming::transport::TransportSendRequest>,
    ) -> Vec<i2pr_client::streaming::transport::TransportSendRequest> {
        let handle = self.fault_profile_handle();
        let Ok(mut profile) = handle.lock() else {
            return requests;
        };
        profile.apply_to_sweep(requests)
    }

    /// Drains every queued `TransportSendRequest` from both the
    /// canonical and the receiver-mirror `StreamingManager`s,
    /// delivers each through the Plan 129 local seam to the
    /// registered peer bridge (looked up by destination hash), and
    /// returns typed per-sweep counters. Plan 149 §8 requires
    /// bounded typed accounting: the caller (the per-destination
    /// runtime driver) records every typed failure rather than
    /// silently dropping queued requests.
    ///
    /// Tests supply a deterministic inbound-tunnel factory on each
    /// bridge through
    /// [`crate::sam::SamDestinationHandle::install_inbound_tunnel_factory`].
    /// The factory is consumed once per call.
    pub fn deliver_outbound(
        &self,
        destination_id: DestinationId,
        now_seconds: u32,
        now_ms: u64,
    ) -> Result<crate::sam::fabric::DeliverySweepCounters, Box<dyn std::error::Error + Send + Sync>>
    {
        let destinations_arc = self.sam_destinations();
        let sender = {
            let destinations = destinations_arc.lock().expect("sam destinations poisoned");
            match destinations.get(destination_id) {
                Some(bridge) => bridge,
                None => return Ok(Default::default()),
            }
        };
        // Step 1: drain canonical + receiver outbound queues.
        let requests: Vec<i2pr_client::streaming::transport::TransportSendRequest> =
            sender.with(|bridge| {
                let mut all = bridge.streaming_mut().drain_outbound();
                all.extend(bridge.receiver_streaming_mut().drain_outbound());
                all
            });
        // Plan 151 §8: apply the deterministic pre-start fault
        // profile (inert by default) before normal delivery. Fault
        // drops hold no production counters and never terminate the
        // connection; the sender's Streaming retransmit state owns
        // recovery. Handshake and CLOSE/RESET control always pass.
        let requests = self.apply_test_fault_profile(requests);
        if requests.is_empty() {
            return Ok(Default::default());
        }
        debug!(
            destination = ?destination_id,
            request_count = requests.len(),
            "deliver_outbound drained queue"
        );
        let mut counters = crate::sam::fabric::DeliverySweepCounters {
            delivered: 0,
            missing_factory: 0,
            factory_exhausted: 0,
            unknown_peer: 0,
            delivery_failed: 0,
        };
        // Step 2: deliver each request.
        let outbound_hop0_hash = i2pr_proto::Hash::from_bytes([0xA1; 32]);
        let outbound_hop1_hash = i2pr_proto::Hash::from_bytes([0xA2; 32]);
        let outbound_tunnel_id = i2pr_tunnel::TunnelId::new(0x0200_0000).map_err(
            |error| -> Box<dyn std::error::Error + Send + Sync> {
                Box::new(std::io::Error::other(format!("tunnel id: {error}")))
            },
        )?;
        // Plan 147 §11: production SAM path uses CSPRNG, never a
        // deterministic seed. `OsRng` is a `TryCryptoRng`; wrap it in
        // `UnwrapMut` so the `bridge_to_peer` `CryptoRng + RngCore`
        // bound is satisfied.
        let mut os_rng = i2pr_crypto::OsRng;
        let mut rng = rand_core::UnwrapMut(&mut os_rng);
        for request in requests {
            let peer_destination_hash = request.destination_hash;
            let peer = destinations_arc
                .lock()
                .expect("sam destinations poisoned")
                .lookup_by_peer_hash(&peer_destination_hash);
            let peer = match peer {
                Some(peer) => peer,
                None => {
                    debug!(
                        destination = ?destination_id,
                        peer_hash = ?peer_destination_hash,
                        "deliver_outbound: no peer bridge registered"
                    );
                    counters.unknown_peer = counters.unknown_peer.saturating_add(1);
                    self.terminate_failed_delivery(destination_id, &request);
                    continue;
                }
            };
            let sender_clone = destinations_arc
                .lock()
                .expect("sam destinations poisoned")
                .get(destination_id)
                .expect("sender still registered");
            let (peer_lease_set2, peer_identity_key) =
                peer.with(|bridge| (bridge.lease_set2().clone(), bridge.identity_netdb_key()));
            let peer_lease_set2 = match i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(
                peer_lease_set2,
                Some(peer_identity_key),
                i2pr_netdb::LeaseSet2ValidationContext::new(now_seconds),
            ) {
                Ok(validated) => validated,
                Err(error) => {
                    debug!(error = %error, "local peer LeaseSet2 validation failed");
                    counters.delivery_failed = counters.delivery_failed.saturating_add(1);
                    self.terminate_failed_delivery(destination_id, &request);
                    continue;
                }
            };
            if let Err(error) = sender_clone.with(|bridge| {
                bridge
                    .routing_mut()
                    .install_remote_lease_set2(peer_lease_set2)
            }) {
                debug!(error = %error, "local peer LeaseSet2 install failed");
                counters.delivery_failed = counters.delivery_failed.saturating_add(1);
                self.terminate_failed_delivery(destination_id, &request);
                continue;
            }
            let inbound_factory_present =
                peer.with(|bridge| bridge.inbound_tunnel_factory().is_some());
            let inbound_tunnel = peer.with(|bridge| {
                let factory = bridge.inbound_tunnel_factory();
                match factory {
                    Some(factory) => factory.build_inbound_tunnel().ok(),
                    None => None,
                }
            });
            let inbound_tunnel = match inbound_tunnel {
                Some(t) => t,
                None => {
                    debug!(
                        destination = ?destination_id,
                        "deliver_outbound: no inbound tunnel factory or build failed"
                    );
                    if inbound_factory_present {
                        counters.factory_exhausted = counters.factory_exhausted.saturating_add(1);
                    } else {
                        counters.missing_factory = counters.missing_factory.saturating_add(1);
                    }
                    self.terminate_failed_delivery(destination_id, &request);
                    continue;
                }
            };
            let delivery = crate::sam::bridge_to_peer(
                &sender_clone,
                &peer,
                outbound_hop0_hash,
                outbound_hop1_hash,
                &request,
                now_seconds,
                now_ms,
                outbound_tunnel_id,
                inbound_tunnel,
                &mut rng,
            );
            debug!(
                destination = ?destination_id,
                peer_hash = ?request.destination_hash,
                result = ?delivery,
                "deliver_outbound: bridge_to_peer result"
            );
            if delivery.is_ok() {
                counters.delivered = counters.delivered.saturating_add(1);
            } else {
                // Plan 149 §8: surface bridge_to_peer failures via
                // the same sweep counters so the driver can wake
                // waiters and avoid silent drops.
                counters.delivery_failed = counters.delivery_failed.saturating_add(1);
                self.terminate_failed_delivery(destination_id, &request);
            }
        }
        Ok(counters)
    }
}

/// Typed failure mode of [`run_raw_stream`].
#[derive(Debug, thiserror::Error)]
pub enum RawStreamError {
    /// TCP read or write returned a typed I/O error.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// `StreamingManager` rejected the application data.
    #[error("streaming manager: {0}")]
    Streaming(String),
}
