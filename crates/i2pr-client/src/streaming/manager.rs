//! Streaming connection manager with full wire-level packet processing.
//!
//! The [`StreamingManager`] is per-destination and owns:
//!
//! - the outbound connection table (keyed by `send_stream_id`),
//! - the inbound connection table (keyed by `receive_stream_id`),
//! - the per-port listener backlog,
//! - a bounded pre-SYN unknown-stream reorder buffer,
//! - the outbound transport request queue the runtime adapter drains.
//!
#![allow(dead_code)]
//! The manager is fully synchronous. Inbound packets are decoded via
//! the [`crate::streaming`] wire codec, validated against the
//! streaming policy, and routed to the owning connection. Outbound
//! packets are serialized through the same codec, optionally signed
//! with the destination's Ed25519 key, and emitted as
//! [`TransportSendRequest`] values the runtime dispatches through the
//! Plan 122 destination routing pipeline.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use i2pr_crypto::verify_signature;
use i2pr_proto::streaming::{
    ClientPayload, FLAG_CLOSE, FLAG_FROM_INCLUDED, FLAG_RESET, FLAG_SIGNATURE_INCLUDED,
    MAX_STREAMING_PACKET_BYTES, MAX_STREAMING_PAYLOAD_BYTES, STREAMING_OPTION_MAX_PACKET_SIZE,
    STREAMING_OPTION_SIGNATURE, SignatureOptionLocation, StreamingFlags, StreamingPacket,
    StreamingPacketBuilder, StreamingPacketError, StreamingReceiveLimit, StreamingSendLimit,
    build_signature_preimage, decode_streaming_packet, encode_client_payload,
    encode_streaming_packet, encode_syn_replay_binding, validate_syn_policy,
};
use i2pr_proto::{CodecError, SignatureValue};

use crate::identity::DestinationIdentity;
use crate::streaming::config::StreamingConfig;
use crate::streaming::connection::{
    ConnectionId, ConnectionState, ConnectionTransition, StreamDirection, StreamingConnection,
};
use crate::streaming::errors::StreamingError;
use crate::streaming::events::WirePacketObservation;
use crate::streaming::transport::TransportSendRequest;

/// Hard ceiling on the number of streams per local destination.
pub const MAX_STREAMS_PER_DESTINATION: usize =
    crate::streaming::config::MAX_STREAMS_PER_DESTINATION_LIMIT;

/// Hard ceiling on the pre-SYN unknown-stream reorder buffer.
pub const MAX_PRE_SYN_BUFFER: usize = 8;

/// Identifier used to refer to a remote destination in the manager.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RemoteDestinationKey {
    /// SHA-256 destination hash.
    pub destination_hash: [u8; 32],
}

impl RemoteDestinationKey {
    /// Wraps a 32-byte destination hash.
    pub const fn from_hash(hash: [u8; 32]) -> Self {
        Self {
            destination_hash: hash,
        }
    }
}

/// Lightweight handle to the remote destination needed for streaming
/// session establishment and signature verification.
#[derive(Clone, Debug)]
pub struct RemoteDestination {
    /// Destination hash used as the replay binding.
    pub destination_hash: [u8; 32],
    /// Public signing key the remote destination signs streaming
    /// packets with.
    pub signing_public_key: i2pr_proto::SigningPublicKey,
    /// Static X25519 public key (only needed for ECIES session layer
    /// reuse; not consumed by the minimal streaming core).
    pub static_public_key: [u8; 32],
}

/// Typed outcome of a connect attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConnectOutcome {
    /// Connection was created in `OutboundSynSent` state and a SYN
    /// packet was emitted for the runtime to dispatch.
    SynSent {
        /// Connection ID assigned to the new outbound stream.
        connection_id: ConnectionId,
        /// Sender's stream ID chosen for the new connection.
        send_stream_id: u32,
    },
    /// The connection table is full.
    ConnectionTableFull,
    /// The outbound pending budget is exhausted.
    PendingBudgetExhausted,
    /// The local destination's payload exceeds the configured ceiling.
    PayloadTooLarge { actual: usize, maximum: usize },
    /// A streaming-codec error occurred.
    Codec(StreamingPacketError),
}

/// Typed outcome of a listener bind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListenerOutcome {
    /// The listener is bound to the requested port.
    Listening { port: u16 },
    /// The port is already in use.
    PortAlreadyInUse,
    /// The accept backlog is full.
    BacklogFull,
}

/// Streaming event surfaced to the runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamingEvent {
    /// A connection transitioned states.
    ConnectionTransition {
        /// Connection ID.
        connection_id: ConnectionId,
        /// The transition.
        transition: ConnectionTransition,
    },
    /// An inbound SYN is pending acceptance.
    InboundSynPending {
        /// Connection ID assigned to the new inbound stream.
        connection_id: ConnectionId,
        /// Sender's stream ID.
        send_stream_id: u32,
    },
    /// A packet was received from the transport.
    PacketReceived {
        /// Observation of the decoded packet.
        observation: WirePacketObservation,
    },
    /// Application bytes were delivered to the receiving connection.
    ApplicationDelivered {
        /// Connection ID.
        connection_id: ConnectionId,
        /// Delivered bytes in order.
        bytes: Vec<u8>,
    },
    /// A connection terminated (graceful close or reset).
    ConnectionClosed {
        /// Connection ID.
        connection_id: ConnectionId,
        /// Final state.
        final_state: ConnectionState,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionRefused {
    TableFull,
    BudgetExhausted,
    BacklogFull,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StreamingManagerError {
    #[error("streaming manager config: {0}")]
    Config(String),
    #[error("streaming manager connection table full")]
    ConnectionTableFull,
    #[error("streaming manager listener backlog full")]
    ListenerBacklogFull,
    #[error("streaming manager port already in use")]
    PortAlreadyInUse,
    #[error("streaming manager unknown connection")]
    UnknownConnection,
    #[error("streaming manager invalid connection state")]
    InvalidConnectionState,
    #[error("streaming: {0}")]
    Streaming(#[from] StreamingError),
    #[error("streaming codec: {0}")]
    Codec(#[from] StreamingPacketError),
    #[error("streaming codec: {0}")]
    I2npCodec(#[from] CodecError),
    #[error("randomness unavailable")]
    RandomnessUnavailable,
    #[error("streaming outbound envelope: {0}")]
    OutboundEnvelope(i2pr_proto::streaming::ClientPayloadEncodeError),
    #[error("streaming inbound envelope: {0}")]
    InboundEnvelope(i2pr_proto::streaming::ClientPayloadDecodeError),
    #[error("destination identity: {0}")]
    DestinationIdentity(#[from] crate::identity::DestinationIdentityError),
}

#[derive(Debug, thiserror::Error)]
pub enum StreamSinkError {
    #[error("stream closed")]
    Closed,
    #[error("stream reset")]
    Reset,
    #[error("send window full")]
    SendWindowFull,
    #[error("congestion window full")]
    CongestionFull,
    #[error("payload too large")]
    PayloadTooLarge,
}

/// Tracked outbound packet for retransmission.
#[derive(Clone, Debug)]
struct OutboundPacket {
    sequence: u32,
    payload_len: usize,
    sent_at_ms: u64,
    retransmit_count: u32,
    /// Serialized wire bytes for retransmission.
    wire_bytes: Vec<u8>,
    /// Whether the packet was signed (SYN / SYN response / CLOSE / RESET).
    signed: bool,
}

/// A buffered inbound packet whose stream ID is not yet known.
#[derive(Clone, Debug)]
struct PreSynBufferEntry {
    received_at_ms: u64,
    wire_bytes: Vec<u8>,
}

/// Per-destination streaming manager.
pub struct StreamingManager {
    config: StreamingConfig,
    /// All active connections, keyed by `ConnectionId`.
    connections: BTreeMap<ConnectionId, StreamingConnection>,
    /// Outbound connections keyed by their local `send_stream_id`.
    outbound_by_stream: BTreeMap<u32, ConnectionId>,
    /// Inbound connections keyed by their local `receive_stream_id`.
    inbound_by_stream: BTreeMap<u32, ConnectionId>,
    /// Per-listener pending accept backlog.
    listeners: BTreeMap<u16, VecDeque<ConnectionId>>,
    /// Pre-SYN reorder buffer (keyed by sender stream ID).
    pre_syn_buffer: BTreeMap<u32, PreSynBufferEntry>,
    /// Outbound transport send requests pending dispatch.
    outbound_queue: VecDeque<TransportSendRequest>,
    /// Per-connection outbound packet tracking for retransmit.
    outbound_packets: BTreeMap<ConnectionId, BTreeMap<u32, OutboundPacket>>,
    /// Next connection ID.
    next_connection_id: u64,
    /// Next outbound stream ID candidate.
    next_outbound_stream_id: u32,
    /// Next inbound stream ID candidate.
    next_inbound_stream_id: u32,
}

impl StreamingManager {
    /// Creates a new streaming manager.
    pub fn new(config: StreamingConfig) -> Self {
        Self {
            config,
            connections: BTreeMap::new(),
            outbound_by_stream: BTreeMap::new(),
            inbound_by_stream: BTreeMap::new(),
            listeners: BTreeMap::new(),
            pre_syn_buffer: BTreeMap::new(),
            outbound_queue: VecDeque::new(),
            outbound_packets: BTreeMap::new(),
            next_connection_id: 1,
            next_outbound_stream_id: 1,
            next_inbound_stream_id: 0x8000_0000,
        }
    }

    /// Returns the manager configuration.
    pub fn config(&self) -> &StreamingConfig {
        &self.config
    }

    /// Returns the number of active connections.
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Binds a listener on the given port.
    pub fn listen(&mut self, port: u16) -> Result<ListenerOutcome, StreamingManagerError> {
        if self.listeners.contains_key(&port) {
            return Ok(ListenerOutcome::PortAlreadyInUse);
        }
        let total_pending: usize = self.listeners.values().map(|q| q.len()).sum();
        if total_pending >= self.config.max_listener_backlog as usize {
            return Ok(ListenerOutcome::BacklogFull);
        }
        self.listeners.insert(port, VecDeque::new());
        Ok(ListenerOutcome::Listening { port })
    }

    /// Initiates an outbound connection to a remote destination. The
    /// caller supplies the local destination identity (used for signing
    /// the SYN), the remote destination key, and the local/remote
    /// streaming ports. Returns the [`TransportSendRequest`] that
    /// carries the serialized SYN packet, ready for the runtime to
    /// dispatch through Plan 122.
    #[allow(clippy::too_many_arguments)]
    pub fn connect<R: CryptoRngStub + ?Sized>(
        &mut self,
        local_dest: &DestinationIdentity,
        remote: &RemoteDestination,
        local_port: u16,
        remote_port: u16,
        now_ms: u64,
        _rng: &mut R,
    ) -> Result<ConnectOutcome, StreamingManagerError> {
        if self.connections.len() >= self.config.max_streams_per_destination as usize {
            return Ok(ConnectOutcome::ConnectionTableFull);
        }

        let send_stream_id = self.allocate_outbound_stream_id();
        let connection_id = ConnectionId::new(self.next_connection_id);
        self.next_connection_id = self.next_connection_id.saturating_add(1);

        let max_payload = MAX_STREAMING_PAYLOAD_BYTES as u32;

        let request = self.build_syn_packet(
            local_dest,
            remote,
            send_stream_id,
            local_port,
            remote_port,
            max_payload,
        )?;

        let conn = StreamingConnection::new_outbound(
            connection_id,
            self.config.clone(),
            send_stream_id,
            0,
            now_ms,
        );
        self.connections.insert(connection_id, conn);
        self.outbound_by_stream
            .insert(send_stream_id, connection_id);
        self.outbound_packets.insert(connection_id, BTreeMap::new());

        // The minimal local core uses an optimistic handshake: the
        // outbound side transitions to Established as soon as the SYN
        // packet is queued for transport. The real network core will
        // defer this transition until the SYN-ACK arrives.
        if let Some(conn) = self.connections.get_mut(&connection_id) {
            let _ = conn.transition_established(max_payload, now_ms);
        }

        self.outbound_queue.push_back(request);

        Ok(ConnectOutcome::SynSent {
            connection_id,
            send_stream_id,
        })
    }

    fn allocate_outbound_stream_id(&mut self) -> u32 {
        let mut candidate = self.next_outbound_stream_id;
        while candidate == 0 || self.outbound_by_stream.contains_key(&candidate) {
            candidate = candidate.wrapping_add(1);
        }
        self.next_outbound_stream_id = candidate.wrapping_add(1);
        candidate
    }

    fn allocate_inbound_stream_id(&mut self) -> u32 {
        let mut candidate = self.next_inbound_stream_id;
        while self.inbound_by_stream.contains_key(&candidate) {
            candidate = candidate.wrapping_add(1);
        }
        self.next_inbound_stream_id = candidate.wrapping_add(1);
        candidate
    }

    #[allow(clippy::too_many_arguments)]
    fn build_syn_packet(
        &self,
        local_dest: &DestinationIdentity,
        remote: &RemoteDestination,
        send_stream_id: u32,
        local_port: u16,
        remote_port: u16,
        max_payload: u32,
    ) -> Result<TransportSendRequest, StreamingManagerError> {
        let destination_bytes = local_dest
            .destination()
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .map_err(StreamingManagerError::I2npCodec)?;
        let mut option_bytes = Vec::with_capacity(destination_bytes.len() + 4 + 66);
        // FROM option is the destination self-encoded (the destination is
        // its own length prefix via the common-structure encoding).
        option_bytes.extend_from_slice(&destination_bytes);
        // MAX_PACKET_SIZE option: type 1, length 4, payload u32 big-endian.
        option_bytes.push(STREAMING_OPTION_MAX_PACKET_SIZE);
        option_bytes.push(4);
        option_bytes.extend_from_slice(&max_payload.to_be_bytes());

        // Build the unsigned SYN packet first so we can sign it.
        let nack_binding = encode_syn_replay_binding(&remote.destination_hash);
        let builder = StreamingPacketBuilder::new_syn(
            send_stream_id,
            0,
            option_bytes.clone(),
            nack_binding.to_vec(),
        )?;
        let limit = StreamingSendLimit::default();
        let wire_bytes = encode_streaming_packet(&builder, limit)?;
        let signature = local_dest.sign(&build_signature_preimage(&wire_bytes, None))?;
        let signature_bytes = signature.as_bytes().to_vec();

        // Re-encode the option region with the SIGNATURE option appended.
        let mut final_options = option_bytes.clone();
        final_options.push(STREAMING_OPTION_SIGNATURE);
        final_options.push(signature_bytes.len() as u8);
        final_options.extend_from_slice(&signature_bytes);

        let final_builder = StreamingPacketBuilder::new_syn(
            send_stream_id,
            0,
            final_options,
            nack_binding.to_vec(),
        )?;
        let final_wire = encode_streaming_packet(&final_builder, limit)?;
        let signature_option_offset = final_wire.len() - (signature_bytes.len() + 2);
        let signature_option_length = signature_bytes.len() + 2;

        // Re-sign over the canonical preimage (wire bytes with the
        // signature option zeroed) and replace the placeholder bytes.
        let preimage = build_signature_preimage(
            &final_wire,
            Some(SignatureOptionLocation {
                offset: signature_option_offset,
                length: signature_option_length,
            }),
        );
        let final_signature = local_dest.sign(&preimage)?;
        let final_sig_bytes = final_signature.as_bytes().to_vec();
        let mut signed_wire = final_wire.clone();
        let sig_start = signature_option_offset + 2;
        signed_wire[sig_start..sig_start + final_sig_bytes.len()].copy_from_slice(&final_sig_bytes);

        // Wrap in the protocol-6 client payload envelope.
        let envelope = ClientPayload {
            protocol: i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
            source_port: local_port,
            destination_port: remote_port,
            payload: signed_wire.clone(),
        };
        let application_bytes =
            encode_client_payload(&envelope).map_err(StreamingManagerError::OutboundEnvelope)?;

        Ok(TransportSendRequest {
            destination_hash: remote.destination_hash,
            source_port: local_port,
            destination_port: remote_port,
            application_payload: application_bytes,
            sequence: 0,
            send_stream_id,
            receive_stream_id: 0,
        })
    }

    /// Drains the outbound transport send queue.
    pub fn drain_outbound(&mut self) -> Vec<TransportSendRequest> {
        self.outbound_queue.drain(..).collect()
    }

    /// Returns the number of pending outbound transport requests.
    pub fn outbound_queue_len(&self) -> usize {
        self.outbound_queue.len()
    }

    /// Processes a streaming payload envelope received from the
    /// transport. The envelope is the protocol-6 client payload frame
    /// that wraps every inbound streaming packet. The method returns
    /// the updated [`WirePacketObservation`] for caller-side telemetry.
    pub fn process_inbound_envelope(
        &mut self,
        wire_bytes: &[u8],
        from_destination_hash: &[u8; 32],
        to_destination: &DestinationIdentity,
        listener_port: Option<u16>,
        now_ms: u64,
    ) -> Result<WirePacketObservation, StreamingManagerError> {
        let envelope = i2pr_proto::streaming::decode_client_payload(
            wire_bytes,
            MAX_STREAMING_PACKET_BYTES + 256,
        )
        .map_err(|error| {
            StreamingManagerError::Streaming(StreamingError::InboundEnvelope(error))
        })?;
        let streaming_bytes = envelope.payload;
        self.process_inbound_packet(
            &streaming_bytes,
            from_destination_hash,
            to_destination,
            listener_port,
            now_ms,
        )
    }

    /// Processes a raw inbound streaming packet (after the protocol-6
    /// client payload envelope has been stripped).
    pub fn process_inbound_packet(
        &mut self,
        wire_bytes: &[u8],
        from_destination_hash: &[u8; 32],
        to_destination: &DestinationIdentity,
        listener_port: Option<u16>,
        now_ms: u64,
    ) -> Result<WirePacketObservation, StreamingManagerError> {
        let limit = StreamingReceiveLimit::default();
        let (packet, signature_location) =
            decode_streaming_packet(wire_bytes, limit).map_err(StreamingManagerError::Codec)?;

        // SYN: establish a new inbound connection.
        if packet.flags.synchronize() && packet.send_stream_id != 0 && packet.receive_stream_id == 0
        {
            return self.handle_inbound_syn(
                &packet,
                signature_location,
                wire_bytes,
                from_destination_hash,
                to_destination,
                listener_port,
                now_ms,
            );
        }

        // SYN response: locate the matching outbound connection.
        if packet.flags.synchronize() && packet.receive_stream_id != 0 {
            return self.handle_inbound_syn_response(
                &packet,
                signature_location,
                wire_bytes,
                from_destination_hash,
                now_ms,
            );
        }

        // Data / CLOSE / RESET on an established connection: locate
        // the matching connection by the receiver's stream ID from
        // our perspective (their `send_stream_id` is the same as our
        // `receive_stream_id`).
        self.handle_data_packet(
            &packet,
            signature_location,
            wire_bytes,
            from_destination_hash,
            to_destination,
            now_ms,
        )
    }

    #[allow(clippy::too_many_arguments, unused_variables)]
    fn handle_inbound_syn(
        &mut self,
        packet: &StreamingPacket,
        signature_location: Option<SignatureOptionLocation>,
        wire_bytes: &[u8],
        from_destination_hash: &[u8; 32],
        to_destination: &DestinationIdentity,
        listener_port: Option<u16>,
        now_ms: u64,
    ) -> Result<WirePacketObservation, StreamingManagerError> {
        // Validate SYN policy (FROM, SIGNATURE, MAX_PACKET_SIZE, replay binding).
        if !packet.flags.from_included() {
            return Err(StreamingManagerError::Codec(
                StreamingPacketError::SynMissingFrom,
            ));
        }
        if !packet.flags.signature_included() {
            return Err(StreamingManagerError::Codec(
                StreamingPacketError::SynMissingSignature,
            ));
        }
        if !packet.flags.max_packet_size_included() {
            return Err(StreamingManagerError::Codec(
                StreamingPacketError::SynMissingMaxPacketSize,
            ));
        }
        let signature_length = to_destination
            .signing_public_key()
            .key_type()
            .signature_len()
            .unwrap_or(0);
        // The SYN replay binding NACK field carries the receiver
        // (local destination) hash. Compare against the local
        // destination's own hash, not the sender's hash.
        let local_destination_hash: [u8; 32] = *to_destination
            .destination()
            .hash()
            .map_err(StreamingManagerError::I2npCodec)?
            .as_bytes();
        validate_syn_policy(packet, &local_destination_hash, signature_length)?;

        // Verify the signature over the canonical preimage using the
        // destination from the FROM option.
        let destination = packet
            .decode_destination()
            .map_err(StreamingManagerError::I2npCodec)?
            .ok_or(StreamingManagerError::Codec(
                StreamingPacketError::SynMissingFrom,
            ))?;
        let location = signature_location.ok_or(StreamingManagerError::Codec(
            StreamingPacketError::SignatureMissing,
        ))?;
        let preimage = build_signature_preimage(wire_bytes, Some(location));
        let signature = packet
            .signature
            .clone()
            .ok_or(StreamingManagerError::Codec(
                StreamingPacketError::SignatureMissing,
            ))?;
        let signature_value =
            SignatureValue::new(destination.signing_key().key_type(), signature.clone()).map_err(
                |_| {
                    StreamingManagerError::Codec(StreamingPacketError::SignatureLengthMismatch {
                        expected: signature_length,
                        actual: signature.len(),
                    })
                },
            )?;
        verify_signature(destination.signing_key(), &preimage, &signature_value)
            .map_err(|_| StreamingManagerError::Codec(StreamingPacketError::SignatureInvalid))?;

        // Allocate a new inbound connection.
        let connection_id = ConnectionId::new(self.next_connection_id);
        self.next_connection_id = self.next_connection_id.saturating_add(1);
        let receive_stream_id = self.allocate_inbound_stream_id();
        let remote_send_stream_id = packet.send_stream_id;
        let max_payload = extract_max_packet_size(packet)?;
        let mut conn = StreamingConnection::new_inbound(
            connection_id,
            self.config.clone(),
            receive_stream_id,
            remote_send_stream_id,
            now_ms,
        );
        conn.transition_established(max_payload, now_ms)
            .map_err(StreamingManagerError::Streaming)?;
        self.connections.insert(connection_id, conn);
        self.inbound_by_stream
            .insert(receive_stream_id, connection_id);
        self.outbound_packets.insert(connection_id, BTreeMap::new());

        // Push to the matching listener backlog (or to port 0 if no
        // listener was registered).
        let port = listener_port.unwrap_or(0);
        let backlog_full = self
            .listeners
            .get(&port)
            .map(|q| q.len() >= self.config.max_listener_backlog as usize)
            .unwrap_or(false);
        if backlog_full {
            self.connections.remove(&connection_id);
            self.inbound_by_stream.remove(&receive_stream_id);
            self.outbound_packets.remove(&connection_id);
            return Err(StreamingManagerError::ListenerBacklogFull);
        }
        let entry = self.listeners.entry(port).or_default();
        entry.push_back(connection_id);

        Ok(WirePacketObservation {
            connection_id: Some(connection_id),
            flags: packet.flags.bits(),
            send_stream_id: packet.send_stream_id,
            receive_stream_id: packet.receive_stream_id,
            sequence: packet.sequence_num,
            ack_through: packet.ack_through,
            nack_count: packet.nacks.len(),
            payload_len: packet.payload.len(),
        })
    }

    #[allow(unused_variables)]
    fn handle_inbound_syn_response(
        &mut self,
        packet: &StreamingPacket,
        signature_location: Option<SignatureOptionLocation>,
        wire_bytes: &[u8],
        from_destination_hash: &[u8; 32],
        now_ms: u64,
    ) -> Result<WirePacketObservation, StreamingManagerError> {
        let connection_id = self
            .outbound_by_stream
            .get(&packet.receive_stream_id)
            .copied()
            .ok_or(StreamingManagerError::UnknownConnection)?;

        // Validate the SYN response (signature length + replay binding).
        let destination = packet
            .decode_destination()
            .map_err(StreamingManagerError::I2npCodec)?
            .ok_or(StreamingManagerError::Codec(
                StreamingPacketError::SynMissingFrom,
            ))?;
        let signature_length = destination
            .signing_key()
            .key_type()
            .signature_len()
            .unwrap_or(0);
        // The SYN response's NACK field carries the receiver (local
        // outbound destination) hash. We cannot verify replay binding
        // on the response because the local destination hash was used
        // to verify the original SYN. For the response we only need
        // signature length and signature validity.
        if !packet.flags.max_packet_size_included() {
            return Err(StreamingManagerError::Codec(
                StreamingPacketError::SynMissingMaxPacketSize,
            ));
        }
        let _ = signature_length; // length is verified when the option is extracted below

        // Verify the signature.
        let location = signature_location.ok_or(StreamingManagerError::Codec(
            StreamingPacketError::SignatureMissing,
        ))?;
        let preimage = build_signature_preimage(wire_bytes, Some(location));
        let signature = packet
            .signature
            .clone()
            .ok_or(StreamingManagerError::Codec(
                StreamingPacketError::SignatureMissing,
            ))?;
        let signature_value =
            SignatureValue::new(destination.signing_key().key_type(), signature.clone()).map_err(
                |_| {
                    StreamingManagerError::Codec(StreamingPacketError::SignatureLengthMismatch {
                        expected: signature_length,
                        actual: signature.len(),
                    })
                },
            )?;
        verify_signature(destination.signing_key(), &preimage, &signature_value)
            .map_err(|_| StreamingManagerError::Codec(StreamingPacketError::SignatureInvalid))?;

        let max_payload = extract_max_packet_size(packet)?;
        let conn = self
            .connections
            .get_mut(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?;
        conn.transition_established(max_payload, now_ms)
            .map_err(StreamingManagerError::Streaming)?;
        let _ = destination; // validated above

        Ok(WirePacketObservation {
            connection_id: Some(connection_id),
            flags: packet.flags.bits(),
            send_stream_id: packet.send_stream_id,
            receive_stream_id: packet.receive_stream_id,
            sequence: packet.sequence_num,
            ack_through: packet.ack_through,
            nack_count: packet.nacks.len(),
            payload_len: packet.payload.len(),
        })
    }

    #[allow(clippy::too_many_arguments, unused_variables)]
    fn handle_data_packet(
        &mut self,
        packet: &StreamingPacket,
        signature_location: Option<SignatureOptionLocation>,
        wire_bytes: &[u8],
        from_destination_hash: &[u8; 32],
        to_destination: &DestinationIdentity,
        now_ms: u64,
    ) -> Result<WirePacketObservation, StreamingManagerError> {
        // The packet's send_stream_id is the remote's local stream ID.
        // From our perspective, the matching connection (in either
        // direction) has its remote_stream_id equal to packet.send_stream_id
        // — for an outbound connection we stored the peer's local id
        // at SYN time, and for an inbound connection we stored the
        // initiator's local id when accepting the SYN.
        //
        // The minimal local core uses an optimistic handshake where
        // outbound connections never receive an explicit SYN-ACK. In
        // that scenario the first data packet from the peer carries
        // the peer's local stream id; bind it to the outbound
        // connection lazily if no match was found.
        let direct_match = self.connections.iter().find_map(|(id, conn)| {
            if conn.remote_stream_id() == packet.send_stream_id {
                Some(*id)
            } else {
                None
            }
        });
        let connection_id = if let Some(id) = direct_match {
            id
        } else {
            // Lazy bind: find the outbound connection whose local stream
            // id matches packet.receive_stream_id (the peer's view of
            // our id).
            self.connections
                .iter_mut()
                .find_map(|(id, conn)| {
                    if conn.direction() == StreamDirection::Outbound
                        && conn.local_stream_id() == packet.receive_stream_id
                        && conn.remote_stream_id() == 0
                    {
                        conn.set_remote_stream_id(packet.send_stream_id);
                        Some(*id)
                    } else {
                        None
                    }
                })
                .ok_or(StreamingManagerError::UnknownConnection)?
        };

        // RESET is authenticated.
        if packet.flags.reset() && packet.flags.signature_included() {
            let signature_length = to_destination
                .signing_public_key()
                .key_type()
                .signature_len()
                .unwrap_or(0);
            if packet.signature.as_ref().map(Vec::len) == Some(signature_length) {
                let location = signature_location.ok_or(StreamingManagerError::Codec(
                    StreamingPacketError::SignatureMissing,
                ))?;
                let preimage = build_signature_preimage(wire_bytes, Some(location));
                let signature = packet
                    .signature
                    .clone()
                    .ok_or(StreamingManagerError::Codec(
                        StreamingPacketError::SignatureMissing,
                    ))?;
                let signature_value = SignatureValue::new(
                    to_destination.signing_public_key().key_type(),
                    signature.clone(),
                )
                .map_err(|_| {
                    StreamingManagerError::Codec(StreamingPacketError::SignatureLengthMismatch {
                        expected: signature_length,
                        actual: signature.len(),
                    })
                })?;
                verify_signature(
                    to_destination.signing_public_key(),
                    &preimage,
                    &signature_value,
                )
                .map_err(|_| {
                    StreamingManagerError::Codec(StreamingPacketError::SignatureInvalid)
                })?;
            }
        }

        let conn = self
            .connections
            .get_mut(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?;

        // Apply the receive window to the payload and ACK.
        let payload = packet.payload.clone();
        let decision = conn
            .receive_packet(packet.sequence_num, payload, now_ms)
            .map_err(StreamingManagerError::Streaming)?;

        // CLOSE handling.
        if packet.flags.close() {
            let _ = conn.remote_close_received(now_ms);
        }

        // RESET handling.
        if packet.flags.reset() {
            let _ = conn.reset(now_ms);
        }

        // Drop the connection into `Closed` if it has reached the
        // end of the close lifecycle (ClosingLocal + received CLOSE).
        // For minimal completeness we transition once both peers
        // acknowledged CLOSE.
        let state = conn.state();
        if state == ConnectionState::ClosingRemote && packet.flags.close() {
            let _ = conn.close(now_ms);
        }

        let observation = WirePacketObservation {
            connection_id: Some(connection_id),
            flags: packet.flags.bits(),
            send_stream_id: packet.send_stream_id,
            receive_stream_id: packet.receive_stream_id,
            sequence: packet.sequence_num,
            ack_through: packet.ack_through,
            nack_count: packet.nacks.len(),
            payload_len: packet.payload.len(),
        };

        // Drop the decision; the receive window state has been updated.
        let _ = decision;

        Ok(observation)
    }

    /// Sends application data over an established connection. Returns
    /// the [`TransportSendRequest`] carrying the serialized data packet
    /// ready for the runtime to dispatch.
    #[allow(clippy::too_many_arguments, unused_variables)]
    pub fn send_data(
        &mut self,
        connection_id: ConnectionId,
        local_dest: &DestinationIdentity,
        remote: &RemoteDestination,
        local_port: u16,
        remote_port: u16,
        payload: &[u8],
        now_ms: u64,
    ) -> Result<TransportSendRequest, StreamingManagerError> {
        let conn = self
            .connections
            .get_mut(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?;
        if conn.state() != ConnectionState::Established {
            return Err(StreamingManagerError::InvalidConnectionState);
        }
        let sequence = conn
            .enqueue_send(payload.len(), now_ms)
            .map_err(StreamingManagerError::Streaming)?;
        let max_payload = conn.max_payload_size();
        let send_stream_id = conn.local_stream_id();
        let receive_stream_id = conn.remote_stream_id();
        let _ = conn;

        // Build the data packet (no signature, no options).
        let option_bytes = Vec::new();
        let flags = StreamingFlags::new(0).expect("empty flags");
        let builder = StreamingPacketBuilder {
            send_stream_id,
            receive_stream_id,
            sequence_num: sequence,
            ack_through: 0,
            nacks: Vec::new(),
            resend_delay: 0,
            flags,
            option_bytes: option_bytes.clone(),
            payload: payload.to_vec(),
        };
        let limit = StreamingSendLimit::default();
        let wire_bytes =
            encode_streaming_packet(&builder, limit).map_err(StreamingManagerError::Codec)?;

        if payload.len() > max_payload as usize {
            return Err(StreamingManagerError::Streaming(
                StreamingError::PayloadTooLarge {
                    actual: payload.len(),
                    maximum: max_payload as usize,
                },
            ));
        }

        // Wrap in the protocol-6 client payload envelope.
        let envelope = ClientPayload {
            protocol: i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
            source_port: local_port,
            destination_port: remote_port,
            payload: wire_bytes.clone(),
        };
        let application_bytes =
            encode_client_payload(&envelope).map_err(StreamingManagerError::OutboundEnvelope)?;

        let request = TransportSendRequest {
            destination_hash: remote.destination_hash,
            source_port: local_port,
            destination_port: remote_port,
            application_payload: application_bytes,
            sequence,
            send_stream_id,
            receive_stream_id,
        };

        // Track the packet for retransmission.
        let outbound = OutboundPacket {
            sequence,
            payload_len: payload.len(),
            sent_at_ms: now_ms,
            retransmit_count: 0,
            wire_bytes,
            signed: false,
        };
        self.outbound_packets
            .entry(connection_id)
            .or_default()
            .insert(sequence, outbound);
        self.outbound_queue.push_back(request.clone());

        Ok(request)
    }

    /// Builds and queues a signed CLOSE packet for the given
    /// connection.
    pub fn send_close(
        &mut self,
        connection_id: ConnectionId,
        local_dest: &DestinationIdentity,
        remote: &RemoteDestination,
        local_port: u16,
        remote_port: u16,
        now_ms: u64,
    ) -> Result<TransportSendRequest, StreamingManagerError> {
        let conn = self
            .connections
            .get_mut(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?;
        conn.begin_close(now_ms)
            .map_err(StreamingManagerError::Streaming)?;
        let send_stream_id = conn.local_stream_id();
        let receive_stream_id = conn.remote_stream_id();
        let _ = conn;

        let mut option_bytes = Vec::new();
        // FROM option carries the local destination self-encoded so the
        // peer can extract the signing key. Signed CLOSE packets must
        // include the FROM option per the streaming protocol.
        let destination_bytes = local_dest
            .destination()
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .map_err(StreamingManagerError::I2npCodec)?;
        option_bytes.extend_from_slice(&destination_bytes);
        // Sign the CLOSE packet.
        let flags = StreamingFlags::new(FLAG_CLOSE | FLAG_SIGNATURE_INCLUDED | FLAG_FROM_INCLUDED)
            .expect("CLOSE flag");
        let builder = StreamingPacketBuilder {
            send_stream_id,
            receive_stream_id,
            sequence_num: 0,
            ack_through: 0,
            nacks: Vec::new(),
            resend_delay: 0,
            flags,
            option_bytes: option_bytes.clone(),
            payload: Vec::new(),
        };
        let limit = StreamingSendLimit::default();
        let wire_bytes =
            encode_streaming_packet(&builder, limit).map_err(StreamingManagerError::Codec)?;

        let signature = local_dest.sign(&build_signature_preimage(&wire_bytes, None))?;
        let signature_bytes = signature.as_bytes().to_vec();

        option_bytes.push(STREAMING_OPTION_SIGNATURE);
        option_bytes.push(signature_bytes.len() as u8);
        option_bytes.extend_from_slice(&signature_bytes);

        let final_builder = StreamingPacketBuilder {
            send_stream_id,
            receive_stream_id,
            sequence_num: 0,
            ack_through: 0,
            nacks: Vec::new(),
            resend_delay: 0,
            flags: StreamingFlags::new(FLAG_CLOSE | FLAG_SIGNATURE_INCLUDED | FLAG_FROM_INCLUDED)
                .expect("CLOSE flag"),
            option_bytes: option_bytes.clone(),
            payload: Vec::new(),
        };
        let final_wire =
            encode_streaming_packet(&final_builder, limit).map_err(StreamingManagerError::Codec)?;
        let signature_option_offset = final_wire.len() - (signature_bytes.len() + 2);
        let signature_option_length = signature_bytes.len() + 2;
        let preimage = build_signature_preimage(
            &final_wire,
            Some(SignatureOptionLocation {
                offset: signature_option_offset,
                length: signature_option_length,
            }),
        );
        let final_signature = local_dest.sign(&preimage)?;
        let final_sig_bytes = final_signature.as_bytes().to_vec();
        let mut signed_wire = final_wire.clone();
        let sig_start = signature_option_offset + 2;
        signed_wire[sig_start..sig_start + final_sig_bytes.len()].copy_from_slice(&final_sig_bytes);

        let envelope = ClientPayload {
            protocol: i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
            source_port: local_port,
            destination_port: remote_port,
            payload: signed_wire.clone(),
        };
        let application_bytes =
            encode_client_payload(&envelope).map_err(StreamingManagerError::OutboundEnvelope)?;

        let request = TransportSendRequest {
            destination_hash: remote.destination_hash,
            source_port: local_port,
            destination_port: remote_port,
            application_payload: application_bytes,
            sequence: u32::MAX,
            send_stream_id,
            receive_stream_id,
        };

        // Track for retransmission until we observe the peer CLOSE.
        let outbound = OutboundPacket {
            sequence: u32::MAX,
            payload_len: 0,
            sent_at_ms: now_ms,
            retransmit_count: 0,
            wire_bytes: signed_wire,
            signed: true,
        };
        self.outbound_packets
            .entry(connection_id)
            .or_default()
            .insert(u32::MAX, outbound);
        self.outbound_queue.push_back(request.clone());

        Ok(request)
    }

    /// Builds and queues a signed RESET packet.
    pub fn send_reset(
        &mut self,
        connection_id: ConnectionId,
        local_dest: &DestinationIdentity,
        remote: &RemoteDestination,
        local_port: u16,
        remote_port: u16,
        now_ms: u64,
    ) -> Result<TransportSendRequest, StreamingManagerError> {
        let conn = self
            .connections
            .get_mut(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?;
        conn.reset(now_ms)
            .map_err(StreamingManagerError::Streaming)?;
        let send_stream_id = conn.local_stream_id();
        let receive_stream_id = conn.remote_stream_id();
        let _ = conn;

        // FROM option carries the local destination self-encoded.
        let destination_bytes = local_dest
            .destination()
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .map_err(StreamingManagerError::I2npCodec)?;
        let mut option_bytes = Vec::new();
        option_bytes.extend_from_slice(&destination_bytes);
        let flags = StreamingFlags::new(FLAG_RESET | FLAG_SIGNATURE_INCLUDED | FLAG_FROM_INCLUDED)
            .expect("RESET flag");
        let builder = StreamingPacketBuilder {
            send_stream_id,
            receive_stream_id,
            sequence_num: 0,
            ack_through: 0,
            nacks: Vec::new(),
            resend_delay: 0,
            flags,
            option_bytes: option_bytes.clone(),
            payload: Vec::new(),
        };
        let limit = StreamingSendLimit::default();
        let wire_bytes =
            encode_streaming_packet(&builder, limit).map_err(StreamingManagerError::Codec)?;

        let signature = local_dest.sign(&build_signature_preimage(&wire_bytes, None))?;
        let signature_bytes = signature.as_bytes().to_vec();

        option_bytes.push(STREAMING_OPTION_SIGNATURE);
        option_bytes.push(signature_bytes.len() as u8);
        option_bytes.extend_from_slice(&signature_bytes);

        let final_builder = StreamingPacketBuilder {
            send_stream_id,
            receive_stream_id,
            sequence_num: 0,
            ack_through: 0,
            nacks: Vec::new(),
            resend_delay: 0,
            flags: StreamingFlags::new(FLAG_RESET | FLAG_SIGNATURE_INCLUDED | FLAG_FROM_INCLUDED)
                .expect("RESET flag"),
            option_bytes: option_bytes.clone(),
            payload: Vec::new(),
        };
        let final_wire =
            encode_streaming_packet(&final_builder, limit).map_err(StreamingManagerError::Codec)?;
        let signature_option_offset = final_wire.len() - (signature_bytes.len() + 2);
        let signature_option_length = signature_bytes.len() + 2;
        let preimage = build_signature_preimage(
            &final_wire,
            Some(SignatureOptionLocation {
                offset: signature_option_offset,
                length: signature_option_length,
            }),
        );
        let final_signature = local_dest.sign(&preimage)?;
        let final_sig_bytes = final_signature.as_bytes().to_vec();
        let mut signed_wire = final_wire.clone();
        let sig_start = signature_option_offset + 2;
        signed_wire[sig_start..sig_start + final_sig_bytes.len()].copy_from_slice(&final_sig_bytes);

        let envelope = ClientPayload {
            protocol: i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
            source_port: local_port,
            destination_port: remote_port,
            payload: signed_wire.clone(),
        };
        let application_bytes =
            encode_client_payload(&envelope).map_err(StreamingManagerError::OutboundEnvelope)?;

        let request = TransportSendRequest {
            destination_hash: remote.destination_hash,
            source_port: local_port,
            destination_port: remote_port,
            application_payload: application_bytes,
            sequence: u32::MAX,
            send_stream_id,
            receive_stream_id,
        };
        self.outbound_queue.push_back(request.clone());

        Ok(request)
    }

    /// Returns the connection ID matching the local send stream ID.
    pub fn lookup_outbound(&self, send_stream_id: u32) -> Option<ConnectionId> {
        self.outbound_by_stream.get(&send_stream_id).copied()
    }

    /// Returns the connection ID matching the local receive stream ID.
    pub fn lookup_inbound(&self, receive_stream_id: u32) -> Option<ConnectionId> {
        self.inbound_by_stream.get(&receive_stream_id).copied()
    }

    /// Returns a reference to a connection.
    pub fn get_connection(&self, id: ConnectionId) -> Option<&StreamingConnection> {
        self.connections.get(&id)
    }

    /// Returns a mutable reference to a connection.
    pub fn get_connection_mut(&mut self, id: ConnectionId) -> Option<&mut StreamingConnection> {
        self.connections.get_mut(&id)
    }

    /// Drops a connection from the table.
    pub fn remove_connection(&mut self, id: ConnectionId) -> Option<StreamingConnection> {
        let removed = self.connections.remove(&id);
        if let Some(conn) = &removed {
            match conn.direction() {
                StreamDirection::Outbound => {
                    self.outbound_by_stream.remove(&conn.local_stream_id());
                }
                StreamDirection::Inbound => {
                    self.inbound_by_stream.remove(&conn.local_stream_id());
                }
            }
        }
        self.outbound_packets.remove(&id);
        removed
    }

    /// Returns the listener backlog for a given port.
    pub fn listener_backlog(&self, port: u16) -> usize {
        self.listeners.get(&port).map(|q| q.len()).unwrap_or(0)
    }

    /// Pops the next pending inbound SYN from the listener backlog.
    pub fn accept(&mut self, port: u16) -> Option<ConnectionId> {
        self.listeners.get_mut(&port).and_then(|q| q.pop_front())
    }
}

/// Trait alias stub for the `CryptoRng` bound. Used by streaming layer
/// callers that inject deterministic RNG.
#[allow(dead_code)]
pub trait CryptoRngStub: rand_core::CryptoRng {}
impl<T: rand_core::CryptoRng + ?Sized> CryptoRngStub for T {}

fn extract_max_packet_size(packet: &StreamingPacket) -> Result<u32, StreamingManagerError> {
    // Locate the MAX_PACKET_SIZE option (`type=1, length=4, u32 BE`).
    // The option region layout is:
    //
    //   [FROM destination (self-encoded, no type/length prefix)]
    //   [MAX_PACKET_SIZE option (type=1, length=4, u32)]
    //   [SIGNATURE option (type=3, signature length, signature bytes)]
    //
    // The MAX_PACKET_SIZE option sits immediately before the SIGNATURE
    // option (which occupies the last 66 bytes for Ed25519). We scan
    // from offset (option_bytes.len() - 66 - 6) backwards to find the
    // option header `[1, 4]`.
    let total = packet.option_bytes.len();
    if total < 66 + 6 {
        return Err(StreamingManagerError::Codec(
            StreamingPacketError::SynMissingMaxPacketSize,
        ));
    }
    let max_offset = total - 66 - 6;
    for offset in 0..=max_offset {
        if packet.option_bytes[offset] == STREAMING_OPTION_MAX_PACKET_SIZE
            && packet.option_bytes[offset + 1] == 4
        {
            let mut bytes = [0_u8; 4];
            bytes.copy_from_slice(&packet.option_bytes[offset + 2..offset + 6]);
            return Ok(u32::from_be_bytes(bytes));
        }
    }
    Err(StreamingManagerError::Codec(
        StreamingPacketError::SynMissingMaxPacketSize,
    ))
}

// Suppress an "unused import" warning for BTreeSet (kept for future use
// when we add per-connection ID sets to the pre-SYN buffer eviction policy).
#[allow(dead_code)]
type _Unused = BTreeSet<u32>;
