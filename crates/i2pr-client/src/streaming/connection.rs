#![allow(dead_code)]

//! Streaming connection state machine.
//!
//! Each connection manages one bidirectional byte stream between a
//! local and remote destination. The state machine is synchronous
//! and deterministic; it never calls `sleep()` or performs I/O.

use core::fmt;

use i2pr_proto::SigningPublicKey;

use crate::streaming::config::StreamingConfig;
use crate::streaming::congestion::{CongestionConfig, CongestionDecision, CongestionPolicy};
use crate::streaming::errors::StreamingError;
use crate::streaming::events::{AckObservation, InboundStreamEvent, OutboundStreamEvent};
use crate::streaming::recv_window::{RecvWindowConfig, RecvWindowDecision, RecvWindowPolicy};
use crate::streaming::retransmit::{RetransmitConfig, RetransmitPolicy};
use crate::streaming::send_window::{SendWindowConfig, SendWindowDecision, SendWindowPolicy};

/// Opaque connection identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConnectionId(u64);

impl ConnectionId {
    /// Creates a new connection ID.
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the raw identifier.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Stream direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamDirection {
    /// Outbound (we initiated the connection).
    Outbound,
    /// Inbound (remote initiated the connection).
    Inbound,
}

/// Streaming connection states.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ConnectionState {
    /// Outbound SYN sent, awaiting SYN response.
    OutboundSynSent,
    /// Inbound SYN received, awaiting local acceptance.
    InboundSynReceived,
    /// Connection established; data may flow in both directions.
    Established,
    /// Local side initiated close; awaiting remote CLOSE ACK.
    ClosingLocal,
    /// Remote side sent CLOSE; we are draining remaining data.
    ClosingRemote,
    /// Connection was reset (abnormal termination).
    Reset,
    /// Connection fully closed; no further operations allowed.
    Closed,
}

impl ConnectionState {
    /// Returns a static label for logging/diagnostics.
    pub fn label(self) -> &'static str {
        match self {
            Self::OutboundSynSent => "OutboundSynSent",
            Self::InboundSynReceived => "InboundSynReceived",
            Self::Established => "Established",
            Self::ClosingLocal => "ClosingLocal",
            Self::ClosingRemote => "ClosingRemote",
            Self::Reset => "Reset",
            Self::Closed => "Closed",
        }
    }
}

impl fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// A recorded state transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionTransition {
    /// Previous state.
    pub from: ConnectionState,
    /// New state.
    pub to: ConnectionState,
    /// Timestamp of the transition (ms).
    pub at_ms: u64,
}

/// Events produced by connection state transitions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamingConnectionEvent {
    /// Connection state changed.
    Transition(ConnectionTransition),
    /// An outbound data packet was queued.
    OutboundData(OutboundStreamEvent),
    /// An inbound data packet was delivered.
    InboundData(InboundStreamEvent),
    /// A send-window backpressure event occurred.
    Backpressure,
}

/// The per-connection streaming state machine.
pub struct StreamingConnection {
    id: ConnectionId,
    direction: StreamDirection,
    state: ConnectionState,
    config: StreamingConfig,
    send_window: SendWindowPolicy,
    recv_window: RecvWindowPolicy,
    congestion: CongestionPolicy,
    retransmit: RetransmitPolicy,
    /// Local stream ID (what we send as sendStreamId for outbound).
    local_stream_id: u32,
    /// Remote stream ID (what we send as receiveStreamId for outbound).
    remote_stream_id: u32,
    /// Peer signing key retained in connection state. Signed control
    /// packets without FROM (CLOSE/RESET since 0.9.20) verify against
    /// this key.
    peer_signing_key: SigningPublicKey,
    /// Peer destination hash retained for outbound addressing of
    /// unsigned control packets such as plain ACKs (Plan 130 §7 D2).
    peer_destination_hash: [u8; 32],
    /// Local I2P destination port of the established stream tuple
    /// (Plan 130 §8 E3). Fixed by the wire handshake and validated on
    /// every subsequent delivery.
    local_port: u16,
    /// Remote I2P source port of the established stream tuple
    /// (Plan 130 §8 E3).
    remote_port: u16,
    /// Maximum payload bytes the local side advertised through its own
    /// MAX_PACKET_SIZE option.
    local_advertised_max_payload: u16,
    /// Maximum payload bytes the peer advertised through its
    /// MAX_PACKET_SIZE option, when observed.
    remote_advertised_max_payload: Option<u16>,
    /// Negotiated max payload size.
    max_payload_size: u32,
    /// Timestamp of last activity.
    last_activity_ms: u64,
    /// Timestamp when the connection was created.
    created_at_ms: u64,
}

impl StreamingConnection {
    /// Creates a new outbound connection in `OutboundSynSent` state.
    #[allow(clippy::too_many_arguments)]
    pub fn new_outbound(
        id: ConnectionId,
        config: StreamingConfig,
        local_stream_id: u32,
        remote_stream_id: u32,
        peer_signing_key: SigningPublicKey,
        peer_destination_hash: [u8; 32],
        local_port: u16,
        remote_port: u16,
        now_ms: u64,
    ) -> Self {
        let send_config = SendWindowConfig::from_config(&config);
        let recv_config = crate::streaming::recv_window::RecvWindowConfig::from_config(&config);
        let cong_config = CongestionConfig::from_config(&config);
        let ret_config = RetransmitConfig::from_config(&config);
        Self {
            id,
            direction: StreamDirection::Outbound,
            state: ConnectionState::OutboundSynSent,
            send_window: SendWindowPolicy::new(send_config),
            recv_window: RecvWindowPolicy::new(recv_config),
            congestion: CongestionPolicy::new(cong_config),
            retransmit: RetransmitPolicy::new(ret_config),
            config,
            local_stream_id,
            remote_stream_id,
            peer_signing_key,
            peer_destination_hash,
            local_port,
            remote_port,
            local_advertised_max_payload: crate::streaming::config::MAX_PACKET_PAYLOAD_BYTES as u16,
            remote_advertised_max_payload: None,
            max_payload_size: crate::streaming::config::MAX_PACKET_PAYLOAD_BYTES as u32,
            last_activity_ms: now_ms,
            created_at_ms: now_ms,
        }
    }

    /// Creates a new inbound connection in `InboundSynReceived` state.
    #[allow(clippy::too_many_arguments)]
    pub fn new_inbound(
        id: ConnectionId,
        config: StreamingConfig,
        local_stream_id: u32,
        remote_stream_id: u32,
        peer_signing_key: SigningPublicKey,
        peer_destination_hash: [u8; 32],
        local_port: u16,
        remote_port: u16,
        now_ms: u64,
    ) -> Self {
        let send_config = SendWindowConfig::from_config(&config);
        let recv_config = crate::streaming::recv_window::RecvWindowConfig::from_config(&config);
        let cong_config = CongestionConfig::from_config(&config);
        let ret_config = RetransmitConfig::from_config(&config);
        Self {
            id,
            direction: StreamDirection::Inbound,
            state: ConnectionState::InboundSynReceived,
            send_window: SendWindowPolicy::new(send_config),
            recv_window: RecvWindowPolicy::new(recv_config),
            congestion: CongestionPolicy::new(cong_config),
            retransmit: RetransmitPolicy::new(ret_config),
            config,
            local_stream_id,
            remote_stream_id,
            peer_signing_key,
            peer_destination_hash,
            local_port,
            remote_port,
            local_advertised_max_payload: crate::streaming::config::MAX_PACKET_PAYLOAD_BYTES as u16,
            remote_advertised_max_payload: None,
            max_payload_size: crate::streaming::config::MAX_PACKET_PAYLOAD_BYTES as u32,
            last_activity_ms: now_ms,
            created_at_ms: now_ms,
        }
    }

    /// Returns the connection ID.
    pub const fn id(&self) -> ConnectionId {
        self.id
    }

    /// Returns the current state.
    pub const fn state(&self) -> ConnectionState {
        self.state
    }

    /// Returns the stream direction.
    pub const fn direction(&self) -> StreamDirection {
        self.direction
    }

    /// Returns the local stream ID.
    pub const fn local_stream_id(&self) -> u32 {
        self.local_stream_id
    }

    /// Returns the remote stream ID.
    pub const fn remote_stream_id(&self) -> u32 {
        self.remote_stream_id
    }

    /// Sets the remote stream id. Used by the SYN response handler
    /// to learn the peer receive stream id before transitioning the
    /// outbound connection to Established.
    pub fn set_remote_stream_id(&mut self, id: u32) {
        self.remote_stream_id = id;
    }

    /// Returns the peer signing key retained in connection state.
    pub const fn peer_signing_key(&self) -> &SigningPublicKey {
        &self.peer_signing_key
    }

    /// Returns the local I2P destination port of the established
    /// stream tuple (Plan 130 §8 E3).
    pub const fn local_port(&self) -> u16 {
        self.local_port
    }

    /// Returns the remote I2P source port of the established stream
    /// tuple (Plan 130 §8 E3).
    pub const fn remote_port(&self) -> u16 {
        self.remote_port
    }

    /// Returns whether the supplied wire port tuple matches the tuple
    /// this connection established through its handshake.
    pub const fn ports_match(&self, source_port: u16, destination_port: u16) -> bool {
        self.remote_port == source_port && self.local_port == destination_port
    }

    /// Returns the peer destination hash retained for outbound
    /// addressing (Plan 130 §7 D2).
    pub const fn peer_destination_hash(&self) -> &[u8; 32] {
        &self.peer_destination_hash
    }

    /// Records the maximum payload bytes the peer advertised through
    /// its MAX_PACKET_SIZE option.
    pub fn set_remote_advertised_max_payload(&mut self, max: u16) {
        self.remote_advertised_max_payload = Some(max);
    }

    /// Returns the maximum payload bytes the peer advertised, when
    /// observed.
    pub const fn remote_advertised_max_payload(&self) -> Option<u16> {
        self.remote_advertised_max_payload
    }

    /// Records the maximum payload bytes the local side advertised
    /// through its own MAX_PACKET_SIZE option.
    pub fn set_local_advertised_max_payload(&mut self, max: u16) {
        self.local_advertised_max_payload = max;
    }

    /// Returns the maximum payload bytes the local side advertised.
    pub const fn local_advertised_max_payload(&self) -> u16 {
        self.local_advertised_max_payload
    }

    /// Returns a lightweight snapshot for SYN response construction.
    /// The snapshot only carries the stream-id fields and direction,
    /// which is what the SYN response builder needs.
    pub fn clone_for_syn_response(&self) -> Self {
        Self {
            id: self.id,
            direction: self.direction,
            state: self.state,
            config: self.config.clone(),
            send_window: SendWindowPolicy::new(SendWindowConfig::from_config(&self.config)),
            recv_window: RecvWindowPolicy::new(RecvWindowConfig::from_config(&self.config)),
            congestion: CongestionPolicy::new(CongestionConfig::from_config(&self.config)),
            retransmit: RetransmitPolicy::new(RetransmitConfig::from_config(&self.config)),
            local_stream_id: self.local_stream_id,
            remote_stream_id: self.remote_stream_id,
            peer_signing_key: self.peer_signing_key.clone(),
            peer_destination_hash: self.peer_destination_hash,
            local_port: self.local_port,
            remote_port: self.remote_port,
            local_advertised_max_payload: self.local_advertised_max_payload,
            remote_advertised_max_payload: self.remote_advertised_max_payload,
            max_payload_size: self.max_payload_size,
            last_activity_ms: self.last_activity_ms,
            created_at_ms: self.created_at_ms,
        }
    }

    /// Returns the negotiated max payload size.
    pub const fn max_payload_size(&self) -> u32 {
        self.max_payload_size
    }

    /// Transitions to `Established`. Only valid from `OutboundSynSent`
    /// (after receiving a SYN response) or `InboundSynReceived`
    /// (after sending a SYN response).
    pub fn transition_established(
        &mut self,
        remote_max_payload: u32,
        now_ms: u64,
    ) -> Result<ConnectionTransition, StreamingError> {
        let from = self.state;
        match from {
            ConnectionState::OutboundSynSent | ConnectionState::InboundSynReceived => {}
            _ => {
                return Err(StreamingError::InvalidStateTransition {
                    from: from.label(),
                    to: "Established",
                });
            }
        }
        self.state = ConnectionState::Established;
        self.last_activity_ms = now_ms;
        // Negotiated max payload = min(local advertised, remote
        // advertised).
        let local_max = u32::from(self.local_advertised_max_payload);
        self.max_payload_size = local_max.min(remote_max_payload);
        Ok(ConnectionTransition {
            from,
            to: self.state,
            at_ms: now_ms,
        })
    }

    /// Transitions to `ClosingLocal`. Only valid from `Established`.
    pub fn begin_close(&mut self, now_ms: u64) -> Result<ConnectionTransition, StreamingError> {
        if self.state != ConnectionState::Established {
            return Err(StreamingError::InvalidStateTransition {
                from: self.state.label(),
                to: "ClosingLocal",
            });
        }
        let from = self.state;
        self.state = ConnectionState::ClosingLocal;
        self.last_activity_ms = now_ms;
        Ok(ConnectionTransition {
            from,
            to: self.state,
            at_ms: now_ms,
        })
    }

    /// Transitions to `ClosingRemote` when a CLOSE is received from
    /// the remote peer. Only valid from `Established`.
    pub fn remote_close_received(
        &mut self,
        now_ms: u64,
    ) -> Result<ConnectionTransition, StreamingError> {
        if self.state != ConnectionState::Established {
            return Err(StreamingError::InvalidStateTransition {
                from: self.state.label(),
                to: "ClosingRemote",
            });
        }
        let from = self.state;
        self.state = ConnectionState::ClosingRemote;
        self.last_activity_ms = now_ms;
        Ok(ConnectionTransition {
            from,
            to: self.state,
            at_ms: now_ms,
        })
    }

    /// Transitions to `Closed`. Valid from any non-Closed state.
    pub fn close(&mut self, now_ms: u64) -> Result<ConnectionTransition, StreamingError> {
        if self.state == ConnectionState::Closed {
            return Err(StreamingError::ConnectionClosed);
        }
        let from = self.state;
        self.state = ConnectionState::Closed;
        self.last_activity_ms = now_ms;
        Ok(ConnectionTransition {
            from,
            to: self.state,
            at_ms: now_ms,
        })
    }

    /// Transitions to `Reset`.
    pub fn reset(&mut self, now_ms: u64) -> Result<ConnectionTransition, StreamingError> {
        if self.state == ConnectionState::Closed {
            return Err(StreamingError::ConnectionClosed);
        }
        let from = self.state;
        self.state = ConnectionState::Reset;
        self.last_activity_ms = now_ms;
        Ok(ConnectionTransition {
            from,
            to: self.state,
            at_ms: now_ms,
        })
    }

    /// Attempts to enqueue a data packet for sending.
    pub fn enqueue_send(&mut self, payload_len: usize, now_ms: u64) -> Result<u32, StreamingError> {
        if self.state != ConnectionState::Established {
            return Err(StreamingError::InvalidStateTransition {
                from: self.state.label(),
                to: "send (need Established)",
            });
        }
        let decision = self.congestion.evaluate();
        match decision {
            CongestionDecision::Full => return Err(StreamingError::CongestionRejected),
            CongestionDecision::Allow { .. } => {}
        }
        match self.send_window.evaluate(payload_len) {
            SendWindowDecision::Accept => {}
            SendWindowDecision::Backpressure => return Err(StreamingError::SendWindowFull),
        }
        let seq = self
            .send_window
            .enqueue(payload_len, now_ms)
            .map_err(|_| StreamingError::SendWindowFull)?;
        self.congestion.record_sent();
        self.last_activity_ms = now_ms;
        Ok(seq)
    }

    /// Processes an incoming data packet.
    pub fn receive_packet(
        &mut self,
        sequence: u32,
        payload: Vec<u8>,
        now_ms: u64,
    ) -> Result<RecvWindowDecision, StreamingError> {
        self.last_activity_ms = now_ms;
        Ok(self.recv_window.receive(sequence, payload))
    }

    /// Processes an incoming acknowledgement with its NACK list
    /// (Plan 130 §7 D4). The send window applies the reference
    /// cumulative/NACK contract; congestion registers an ack sample
    /// for newly acknowledged data and a loss signal when the peer
    /// explicitly NACKed retained packets.
    pub fn receive_ack(&mut self, ack_through: u32, nacks: &[u32], now_ms: u64) -> AckObservation {
        self.last_activity_ms = now_ms;
        let outcome = self.send_window.acknowledge(ack_through, nacks);
        if !outcome.newly_acked.is_empty() {
            self.congestion.record_acked();
        }
        if !outcome.retained_nacks.is_empty() {
            self.congestion.record_loss();
        }
        AckObservation {
            ack_through,
            nack_count: nacks.len(),
        }
    }

    /// Returns the missing sequences the remote peer should NACK.
    pub fn generate_nacks(&self, ack_through: u32) -> Vec<u32> {
        self.recv_window.missing_sequences(ack_through)
    }

    /// Checks if the connection has timed out.
    pub fn check_timeout(&self, now_ms: u64) -> bool {
        let timeout = match self.state {
            ConnectionState::Closed | ConnectionState::Reset => u64::MAX,
            ConnectionState::Established
            | ConnectionState::ClosingLocal
            | ConnectionState::ClosingRemote => self.config.idle_timeout_ms,
            ConnectionState::OutboundSynSent | ConnectionState::InboundSynReceived => {
                self.config.setup_timeout_ms
            }
        };
        now_ms.saturating_sub(self.last_activity_ms) > timeout
    }

    /// Returns a reference to the send window.
    pub fn send_window(&self) -> &SendWindowPolicy {
        &self.send_window
    }

    /// Returns a mutable reference to the send window.
    pub fn send_window_mut(&mut self) -> &mut SendWindowPolicy {
        &mut self.send_window
    }

    /// Returns a reference to the recv window.
    pub fn recv_window(&self) -> &RecvWindowPolicy {
        &self.recv_window
    }

    /// Returns a reference to the congestion policy.
    pub fn congestion(&self) -> &CongestionPolicy {
        &self.congestion
    }

    /// Returns a reference to the retransmit policy.
    pub fn retransmit(&self) -> &RetransmitPolicy {
        &self.retransmit
    }

    /// Returns whether the connection is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self.state, ConnectionState::Closed | ConnectionState::Reset)
    }

    /// Advance the clock: check timeouts and retransmit expiries.
    pub fn advance(&mut self, now_ms: u64) -> Vec<StreamingConnectionEvent> {
        let mut events = Vec::new();
        if self.is_terminal() {
            return events;
        }
        // Check timeout.
        if self.check_timeout(now_ms) {
            let transition = self.reset(now_ms).ok();
            if let Some(t) = transition {
                events.push(StreamingConnectionEvent::Transition(t));
            }
            return events;
        }
        // Check retransmit expiries.
        let rto = self.retransmit.current_rto_ms();
        let expired = self.send_window.expired_entries(now_ms, rto);
        for seq in expired {
            if let Some(entry) = self.send_window.get_unacked(seq) {
                let count = entry.retransmit_count;
                if count >= self.config.max_retransmit_count as u32 {
                    let _ = self.send_window.remove(seq);
                    events.push(StreamingConnectionEvent::OutboundData(
                        OutboundStreamEvent::Retransmitted { sequence: seq },
                    ));
                    continue;
                }
            }
            self.send_window.mark_retransmitted(seq, now_ms);
            events.push(StreamingConnectionEvent::OutboundData(
                OutboundStreamEvent::Retransmitted { sequence: seq },
            ));
        }
        events
    }
}
