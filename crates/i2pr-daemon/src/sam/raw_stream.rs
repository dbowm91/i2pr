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
use tokio::net::TcpStream;
use tracing::{debug, warn};

use crate::destination_streaming::{
    PumpConfig, PumpEndpointError, PumpSendDisposition, StreamPumpEndpoint, run_stream_pump,
};
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

/// SAM narrow capability adapting one Streaming connection to the
/// shared pump.
///
/// The endpoint references exactly one router-owned destination
/// runtime (via `SamServiceState` + `destination_id`), one
/// `StreamingManager` (canonical for CONNECT, receiver-mirror for
/// ACCEPT, selected by `direction`), the existing
/// `StreamingDestinationAdapter` routing seam (through
/// `send_data_segment` + `notify_outbound_signal` +
/// `deliver_outbound` driven by the per-destination driver),
/// per-destination driver cancellation/notification, and bounded
/// delivery accounting. It exposes no `Arc<RouterContext>` service
/// locator and no SAM command parser state.
struct SamPumpEndpoint {
    state: Arc<SamServiceState>,
    destination_id: DestinationId,
    connection_id: ConnectionId,
    peer_destination: RemoteDestination,
    direction: RawDirection,
}

impl StreamPumpEndpoint for SamPumpEndpoint {
    fn max_payload_bytes(&self) -> usize {
        let mut negotiated = i2pr_proto::streaming::DEFAULT_ADVERTISED_MAX_PAYLOAD as usize;
        let destinations = self.state.sam_destinations();
        let Ok(destinations) = destinations.lock() else {
            return negotiated.max(1);
        };
        if let Some(bridge) = destinations.get(self.destination_id) {
            let observed = bridge.with(|b| match self.direction {
                RawDirection::Outbound => b
                    .streaming()
                    .get_connection(self.connection_id)
                    .map(|c| c.max_payload_size() as usize),
                RawDirection::Inbound => b
                    .receiver_streaming()
                    .get_connection(self.connection_id)
                    .map(|c| c.max_payload_size() as usize),
            });
            if let Some(observed) = observed {
                negotiated = observed;
            }
        }
        negotiated.max(1)
    }

    fn try_send(&self, segment: &[u8]) -> Result<PumpSendDisposition, PumpEndpointError> {
        match self.state.send_data_segment(
            self.destination_id,
            self.connection_id,
            &self.peer_destination,
            segment,
            self.direction,
            streaming_now_ms(),
        ) {
            Ok(true) => Ok(PumpSendDisposition::Accepted),
            Ok(false) => Ok(PumpSendDisposition::Backpressured),
            Err(error) => {
                let message = error.to_string();
                if message.contains("UnknownConnection")
                    || message.contains("unknown")
                    || message.contains("no installed bridge")
                {
                    Err(PumpEndpointError::UnknownConnection)
                } else if message.contains("InvalidConnectionState")
                    || message.contains("PortTupleMismatch")
                {
                    Err(PumpEndpointError::InvalidState)
                } else {
                    Err(PumpEndpointError::Streaming(message))
                }
            }
        }
    }

    fn drain_delivered(&self) -> Vec<Vec<u8>> {
        let destinations = self.state.sam_destinations();
        let Ok(destinations) = destinations.lock() else {
            return Vec::new();
        };
        let Some(bridge) = destinations.get(self.destination_id) else {
            return Vec::new();
        };
        bridge
            .with(|b| match self.direction {
                RawDirection::Outbound => b.streaming_mut().drain_delivered_for(self.connection_id),
                RawDirection::Inbound => b
                    .receiver_streaming_mut()
                    .drain_delivered_for(self.connection_id),
            })
            .into_iter()
            .filter_map(|delivered| {
                if delivered.bytes.is_empty() {
                    None
                } else {
                    Some(delivered.bytes)
                }
            })
            .collect()
    }

    fn is_terminal(&self) -> bool {
        let destinations = self.state.sam_destinations();
        let Ok(destinations) = destinations.lock() else {
            return true;
        };
        let Some(bridge) = destinations.get(self.destination_id) else {
            return true;
        };
        bridge.with(|bridge| {
            let connection = match self.direction {
                RawDirection::Outbound => bridge.streaming().get_connection(self.connection_id),
                RawDirection::Inbound => bridge
                    .receiver_streaming()
                    .get_connection(self.connection_id),
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
    }

    fn notify_outbound(&self) {
        self.state.notify_outbound_signal(self.destination_id);
    }
}

/// Spins one raw-mode TCP <-> StreamingManager byte pump for the
/// supplied handoff.
///
/// Plan 174: this is now a thin SAM adaptation over the shared
/// `run_stream_pump` primitive. The generic pump owns the socket
/// loop (bounded chunk, negotiated segmentation, backpressure,
/// sibling-isolated drain, cancellation/EOF/terminal convergence);
/// this wrapper only constructs the narrow `SamPumpEndpoint`
/// capability. Terminal CLOSE/RESET emission and
/// SAM attachment release remain with `finish_raw_stream`.
///
/// The driver terminates when the parent cancellation token fires,
/// the TCP read returns EOF or I/O error, the local application
/// closes the connection, or the Streaming connection becomes
/// terminal. The driver does **not** own the `StreamingManager`; it
/// borrows it through the bridge handle the runtime driver keeps
/// alive.
pub async fn run_raw_stream(
    state: Arc<SamServiceState>,
    handoff: RawStreamHandoff,
    cancellation: CancellationToken,
) -> Result<(), RawStreamError> {
    let RawStreamHandoff {
        stream,
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
    let endpoint: Arc<dyn StreamPumpEndpoint> = Arc::new(SamPumpEndpoint {
        state: Arc::clone(&state),
        destination_id,
        connection_id,
        peer_destination,
        direction,
    });
    let config = PumpConfig::defaults(max_chunk);
    match run_stream_pump(stream, initial_raw_bytes, endpoint, config, cancellation).await {
        Ok(()) => {}
        Err(crate::destination_streaming::PumpError::Io(error)) => {
            return Err(RawStreamError::Io(error));
        }
        Err(crate::destination_streaming::PumpError::Endpoint(endpoint_error)) => {
            match &endpoint_error {
                PumpEndpointError::UnknownConnection | PumpEndpointError::InvalidState => {
                    warn!(
                        session_id = %session_id,
                        destination = ?destination_id,
                        connection_id = connection_id.raw(),
                        direction = ?direction,
                        error = %endpoint_error,
                        "raw driver endpoint terminal"
                    );
                }
                PumpEndpointError::Streaming(_) => {
                    warn!(
                        session_id = %session_id,
                        destination = ?destination_id,
                        connection_id = connection_id.raw(),
                        direction = ?direction,
                        error = %endpoint_error,
                        "raw driver send_data failed"
                    );
                }
            }
            return Err(RawStreamError::Streaming(endpoint_error.to_string()));
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
