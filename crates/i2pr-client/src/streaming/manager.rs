//! Streaming connection manager with full wire-level packet processing.
//!
//! The [`StreamingManager`] is per-destination and owns:
//!
//! - the outbound connection table (keyed by `local_send_stream_id`),
//! - the inbound connection table (keyed by `local_receive_stream_id`),
//! - the per-port listener backlog,
//! - a bounded pre-SYN unknown-stream reorder buffer,
//! - the outbound transport request queue the runtime adapter drains.
//!
//! # Wire-level handshake (Plan 125 §6 / §7)
//!
//! Plan 125 owns a real SYN / SYN-response lifecycle:
//!
//! ```text
//! originator (A):
//!   A_selects A_receive_id > 0
//!   SYN: sendStreamId = 0, receiveStreamId = A_receive_id
//!   -> waits for authenticated SYN response before becoming Established
//!
//! recipient (B):
//!   accepts A's SYN (FROM_INCLUDED, SIGNATURE_INCLUDED, MAX_PACKET_SIZE_INCLUDED)
//!   B selects B_receive_id > 0
//!   SYN response: sendStreamId = A_receive_id, receiveStreamId = B_receive_id
//!   -> transitions the inbound connection to Established
//!
//! originator (A):
//!   validates B's signed SYN response, learns B_receive_id
//!   negotiated_max_payload = min(A_max, B_max)
//!   -> transitions the outbound connection to Established
//! ```
//!
//! Stream IDs are owned separately: `local_send_stream_id` is the id
//! we transmit in `sendStreamId` (it is 0 until the peer supplies the
//! id through a SYN response), `local_receive_stream_id` is the id
//! the peer transmits to us in `sendStreamId`, and
//! `peer_receive_stream_id` is the id the peer expects in our
//! `receiveStreamId` (it is set to the id the peer selected in its
//! SYN response).

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use i2pr_crypto::verify_signature;
use i2pr_proto::streaming::{
    ClientPayload, FLAG_CLOSE, FLAG_FROM_INCLUDED, FLAG_RESET, FLAG_SIGNATURE_INCLUDED,
    MAX_STREAMING_PAYLOAD_BYTES, STREAMING_OPTION_MAX_PACKET_SIZE, STREAMING_OPTION_SIGNATURE,
    SignatureOptionLocation, StreamingFlags, StreamingPacket, StreamingPacketBuilder,
    StreamingPacketError, StreamingReceiveLimit, StreamingSendLimit, build_signature_preimage,
    decode_client_payload, decode_streaming_packet, encode_client_payload, encode_streaming_packet,
    encode_syn_replay_binding, validate_syn_policy,
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

/// Hard ceiling on the local maximum packet payload size we
/// advertise in the SYN's MAX_PACKET_SIZE option.
pub const DEFAULT_ADVERTISED_MAX_PACKET_SIZE: u32 = MAX_STREAMING_PAYLOAD_BYTES as u32;

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
        /// Sender's stream ID (always 0 for an originator SYN).
        send_stream_id: u32,
        /// Local receive stream ID we selected for this connection.
        receive_stream_id: u32,
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
        /// Sender's stream ID we observed (originator's local id).
        remote_send_stream_id: u32,
        /// Local receive stream ID we assigned.
        local_receive_stream_id: u32,
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
    /// Outbound connections keyed by their `local_receive_stream_id`
    /// (the id we put in `receiveStreamId` on every packet we send).
    outbound_by_stream: BTreeMap<u32, ConnectionId>,
    /// Inbound connections keyed by their `local_receive_stream_id`
    /// (the id the peer puts in `sendStreamId` on every packet it
    /// sends to us).
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
    /// Next inbound stream ID candidate (the id we assign as our
    /// `local_receive_stream_id`).
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
            // The non-zero range is canonical for I2P Streaming.
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
    /// streaming ports. The connection remains in
    /// `OutboundSynSent` until the runtime delivers a signed SYN
    /// response through [`Self::process_inbound_packet`].
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

        let connection_id = ConnectionId::new(self.next_connection_id);
        self.next_connection_id = self.next_connection_id.saturating_add(1);
        let local_receive_stream_id = self.allocate_inbound_stream_id();
        // Plan 125 §5: originator SYN uses `sendStreamId = 0` until
        // the peer supplies the id via the SYN response.
        let request = self.build_syn_packet(
            local_dest,
            remote,
            local_receive_stream_id,
            local_port,
            remote_port,
            DEFAULT_ADVERTISED_MAX_PACKET_SIZE,
        )?;

        let conn = StreamingConnection::new_outbound(
            connection_id,
            self.config.clone(),
            local_receive_stream_id,
            0,
            now_ms,
        );
        self.connections.insert(connection_id, conn);
        self.outbound_by_stream
            .insert(local_receive_stream_id, connection_id);
        self.outbound_packets.insert(connection_id, BTreeMap::new());

        // Plan 125 §6: the connection is created in `OutboundSynSent`
        // state. It does NOT transition to Established here; that
        // happens after the peer supplies a signed SYN response.
        self.outbound_queue.push_back(request);

        Ok(ConnectOutcome::SynSent {
            connection_id,
            send_stream_id: 0,
            receive_stream_id: local_receive_stream_id,
        })
    }

    fn allocate_inbound_stream_id(&mut self) -> u32 {
        let mut candidate = self.next_inbound_stream_id;
        while self.inbound_by_stream.contains_key(&candidate)
            || self.outbound_by_stream.contains_key(&candidate)
        {
            candidate = candidate.wrapping_add(1);
        }
        self.next_inbound_stream_id = candidate.wrapping_add(1);
        candidate
    }

    /// Builds the originator SYN packet and wraps it in a Streaming
    /// client payload frame. The signature region is zeroed in the
    /// preimage per the canonical streaming policy.
    fn build_syn_packet(
        &self,
        local_dest: &DestinationIdentity,
        remote: &RemoteDestination,
        local_receive_stream_id: u32,
        local_port: u16,
        remote_port: u16,
        max_packet_size: u32,
    ) -> Result<TransportSendRequest, StreamingManagerError> {
        let destination_bytes = local_dest
            .destination()
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .map_err(StreamingManagerError::I2npCodec)?;
        let mut option_bytes = Vec::with_capacity(destination_bytes.len() + 4 + 66);
        // FROM option: self-encoded destination (no type/length prefix).
        option_bytes.extend_from_slice(&destination_bytes);
        // MAX_PACKET_SIZE option: type 1, length 4, payload u32 big-endian.
        option_bytes.push(STREAMING_OPTION_MAX_PACKET_SIZE);
        option_bytes.push(4);
        option_bytes.extend_from_slice(&max_packet_size.to_be_bytes());

        let nack_binding = encode_syn_replay_binding(&remote.destination_hash);
        let builder = StreamingPacketBuilder::new_syn(
            0, // sendStreamId = 0 for originator SYN
            local_receive_stream_id,
            0,
            option_bytes.clone(),
            nack_binding.to_vec(),
        )?;
        let limit = StreamingSendLimit::default();
        let wire_bytes = encode_streaming_packet(&builder, limit)?;
        let signature = local_dest.sign(&build_signature_preimage(&wire_bytes, None))?;
        let signature_bytes = signature.as_bytes().to_vec();

        let mut final_options = option_bytes;
        final_options.push(STREAMING_OPTION_SIGNATURE);
        final_options.push(signature_bytes.len() as u8);
        final_options.extend_from_slice(&signature_bytes);

        let final_builder = StreamingPacketBuilder::new_syn(
            0,
            local_receive_stream_id,
            0,
            final_options,
            nack_binding.to_vec(),
        )?;
        let final_wire = encode_streaming_packet(&final_builder, limit)?;
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
        let mut signed_wire = final_wire;
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

        Ok(TransportSendRequest {
            destination_hash: remote.destination_hash,
            source_port: local_port,
            destination_port: remote_port,
            application_payload: application_bytes,
            sequence: 0,
            send_stream_id: 0,
            receive_stream_id: local_receive_stream_id,
        })
    }

    /// Builds a signed SYN response packet after the local destination
    /// accepts an inbound SYN. The caller supplies the validated
    /// inbound connection and the freshly-selected local receive
    /// stream id.
    fn build_syn_response_packet(
        &self,
        local_dest: &DestinationIdentity,
        remote: &RemoteDestination,
        inbound_connection: &StreamingConnection,
        local_port: u16,
        remote_port: u16,
        max_packet_size: u32,
    ) -> Result<TransportSendRequest, StreamingManagerError> {
        // The SYN response addresses the originator by their
        // `local_receive_stream_id` (which is the originator's id and
        // appears in the originator SYN's `receiveStreamId`). The
        // response's `sendStreamId` therefore equals
        // `inbound_connection.local_receive_stream_id` from the
        // originator's perspective = the originator's stream id we
        // observed. The response's `receiveStreamId` carries our
        // freshly-selected id.
        let destination_bytes = local_dest
            .destination()
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .map_err(StreamingManagerError::I2npCodec)?;
        let mut option_bytes = Vec::with_capacity(destination_bytes.len() + 4 + 66);
        option_bytes.extend_from_slice(&destination_bytes);
        option_bytes.push(STREAMING_OPTION_MAX_PACKET_SIZE);
        option_bytes.push(4);
        option_bytes.extend_from_slice(&max_packet_size.to_be_bytes());

        let nack_binding = encode_syn_replay_binding(&remote.destination_hash);
        let builder = StreamingPacketBuilder::new_syn_response(
            inbound_connection.remote_stream_id(),
            inbound_connection.local_stream_id(),
            0,
            option_bytes.clone(),
            nack_binding.to_vec(),
        )?;
        let limit = StreamingSendLimit::default();
        let wire_bytes = encode_streaming_packet(&builder, limit)?;
        let signature = local_dest.sign(&build_signature_preimage(&wire_bytes, None))?;
        let signature_bytes = signature.as_bytes().to_vec();

        let mut final_options = option_bytes;
        final_options.push(STREAMING_OPTION_SIGNATURE);
        final_options.push(signature_bytes.len() as u8);
        final_options.extend_from_slice(&signature_bytes);

        let final_builder = StreamingPacketBuilder::new_syn_response(
            inbound_connection.remote_stream_id(),
            inbound_connection.local_stream_id(),
            0,
            final_options,
            nack_binding.to_vec(),
        )?;
        let final_wire = encode_streaming_packet(&final_builder, limit)?;
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
        let mut signed_wire = final_wire;
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

        Ok(TransportSendRequest {
            destination_hash: remote.destination_hash,
            source_port: local_port,
            destination_port: remote_port,
            application_payload: application_bytes,
            sequence: 0,
            send_stream_id: inbound_connection.remote_stream_id(),
            receive_stream_id: inbound_connection.local_stream_id(),
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
    /// that wraps every inbound streaming packet.
    pub fn process_inbound_envelope(
        &mut self,
        wire_bytes: &[u8],
        from_destination_hash: &[u8; 32],
        to_destination: &DestinationIdentity,
        listener_port: Option<u16>,
        now_ms: u64,
    ) -> Result<WirePacketObservation, StreamingManagerError> {
        let envelope = decode_client_payload(wire_bytes, MAX_STREAMING_PAYLOAD_BYTES + 256)
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

        // Inbound SYN (originator): packet.sendStreamId == 0 AND
        // packet.receiveStreamId != 0 (the originator picked an id
        // for us to address them by).
        if packet.flags.synchronize() && packet.send_stream_id == 0 && packet.receive_stream_id != 0
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

        // SYN response (recipient): packet.sendStreamId == our
        // local_receive_stream_id AND packet.receiveStreamId != 0
        // (the recipient picked an id for us to address them by).
        if packet.flags.synchronize() && packet.send_stream_id != 0 && packet.receive_stream_id != 0
        {
            return self.handle_inbound_syn_response(
                &packet,
                signature_location,
                wire_bytes,
                from_destination_hash,
                now_ms,
            );
        }

        // Data / CLOSE / RESET on an established connection.
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
        // Validate SYN policy.
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
        // (local destination) hash.
        let local_destination_hash: [u8; 32] = *to_destination
            .destination()
            .hash()
            .map_err(StreamingManagerError::I2npCodec)?
            .as_bytes();
        validate_syn_policy(packet, &local_destination_hash, signature_length)?;

        // Verify signature.
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

        // Allocate a new inbound connection. The originator's
        // `receiveStreamId` is the id they want us to use in our
        // `sendStreamId`; from our perspective that's their
        // `remote_stream_id` (which we send to them as
        // `receiveStreamId`).
        let connection_id = ConnectionId::new(self.next_connection_id);
        self.next_connection_id = self.next_connection_id.saturating_add(1);
        let local_receive_stream_id = self.allocate_inbound_stream_id();
        let remote_send_stream_id = packet.receive_stream_id;
        let conn = StreamingConnection::new_inbound(
            connection_id,
            self.config.clone(),
            local_receive_stream_id,
            remote_send_stream_id,
            now_ms,
        );
        // The inbound connection starts in `InboundSynReceived` and
        // does NOT transition to Established yet. The application
        // must accept() the SYN; then the manager builds and queues a
        // SYN response, after which both sides transition to
        // Established (the inbound side here, the originator side when
        // its SYN response arrives).
        self.connections.insert(connection_id, conn);
        self.inbound_by_stream
            .insert(local_receive_stream_id, connection_id);
        self.outbound_packets.insert(connection_id, BTreeMap::new());

        let port = listener_port.unwrap_or(0);
        let backlog_full = self
            .listeners
            .get(&port)
            .map(|q| q.len() >= self.config.max_listener_backlog as usize)
            .unwrap_or(false);
        if backlog_full {
            self.connections.remove(&connection_id);
            self.inbound_by_stream.remove(&local_receive_stream_id);
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

    /// Accepts the pending inbound SYN identified by
    /// `connection_id`, transitions the connection to `Established`,
    /// and returns a signed SYN response packet for the runtime to
    /// dispatch. The SYN response is **not** appended to the
    /// outbound queue — the runtime drains the returned request
    /// directly. The caller is expected to obtain the
    /// `connection_id` via [`Self::accept`].
    #[allow(clippy::too_many_arguments)]
    pub fn accept_inbound_syn<R: CryptoRngStub + ?Sized>(
        &mut self,
        local_dest: &DestinationIdentity,
        remote: &RemoteDestination,
        connection_id: ConnectionId,
        local_port: u16,
        remote_port: u16,
        max_packet_size: u32,
        now_ms: u64,
        _rng: &mut R,
    ) -> Result<TransportSendRequest, StreamingManagerError> {
        let conn = self
            .connections
            .get(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?;
        let inbound_connection_snapshot = conn.clone_for_syn_response();
        let request = self.build_syn_response_packet(
            local_dest,
            remote,
            &inbound_connection_snapshot,
            local_port,
            remote_port,
            max_packet_size,
        )?;
        let max_payload = extract_max_packet_size_for_response(&request, max_packet_size)?;
        let conn = self
            .connections
            .get_mut(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?;
        conn.transition_established(max_payload, now_ms)
            .map_err(StreamingManagerError::Streaming)?;
        // Track the SYN response packet for retransmission until the
        // originator confirms with a non-SYN packet.
        let outbound = OutboundPacket {
            sequence: 0,
            payload_len: 0,
            sent_at_ms: now_ms,
            retransmit_count: 0,
            wire_bytes: decode_payload_from_request(&request)?,
            signed: true,
        };
        self.outbound_packets
            .entry(connection_id)
            .or_default()
            .insert(0, outbound);
        Ok(request)
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
        // SYN response: packet.sendStreamId == our local_receive_stream_id.
        let connection_id = self
            .outbound_by_stream
            .get(&packet.send_stream_id)
            .copied()
            .ok_or(StreamingManagerError::UnknownConnection)?;

        // Validate required flags.
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
        // Plan 125 §6: peer receive stream id is the `receiveStreamId`
        // the SYN response supplied; set it on the outbound connection
        // before transitioning to Established.
        conn.set_remote_stream_id(packet.receive_stream_id);
        conn.transition_established(max_payload, now_ms)
            .map_err(StreamingManagerError::Streaming)?;

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
        // The packet's `send_stream_id` carries the stream id the peer
        // selected for its own receiveStreamId. On an inbound
        // connection (we accepted the SYN) that matches our
        // `inbound_by_stream` key. On an outbound connection (we sent
        // the SYN) the same id matches our `outbound_by_stream` key
        // because the peer's id is what we recorded at SYN/SYN-response
        // time. Search both maps so a data packet is routed correctly
        // regardless of which side originated the connection.
        let connection_id = self
            .inbound_by_stream
            .get(&packet.send_stream_id)
            .copied()
            .or_else(|| self.outbound_by_stream.get(&packet.send_stream_id).copied())
            .ok_or(StreamingManagerError::UnknownConnection)?;

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

        let payload = packet.payload.clone();
        let decision = conn
            .receive_packet(packet.sequence_num, payload, now_ms)
            .map_err(StreamingManagerError::Streaming)?;

        if packet.flags.close() {
            let _ = conn.remote_close_received(now_ms);
        }
        if packet.flags.reset() {
            let _ = conn.reset(now_ms);
        }

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
        let local_receive_stream_id = conn.local_stream_id();
        let peer_receive_stream_id = conn.remote_stream_id();
        let _ = conn;

        if payload.len() > max_payload as usize {
            return Err(StreamingManagerError::Streaming(
                StreamingError::PayloadTooLarge {
                    actual: payload.len(),
                    maximum: max_payload as usize,
                },
            ));
        }

        // The data packet uses our peer receive stream id as
        // `sendStreamId` and our local receive stream id as
        // `receiveStreamId`.
        let option_bytes = Vec::new();
        let flags = StreamingFlags::new(0).expect("empty flags");
        let builder = StreamingPacketBuilder {
            send_stream_id: peer_receive_stream_id,
            receive_stream_id: local_receive_stream_id,
            sequence_num: sequence,
            ack_through: 0,
            nacks: Vec::new(),
            resend_delay: 0,
            flags,
            option_bytes,
            payload: payload.to_vec(),
        };
        let limit = StreamingSendLimit::default();
        let wire_bytes =
            encode_streaming_packet(&builder, limit).map_err(StreamingManagerError::Codec)?;

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
            send_stream_id: peer_receive_stream_id,
            receive_stream_id: local_receive_stream_id,
        };

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
        let local_receive_stream_id = conn.local_stream_id();
        let peer_receive_stream_id = conn.remote_stream_id();
        let _ = conn;

        let destination_bytes = local_dest
            .destination()
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .map_err(StreamingManagerError::I2npCodec)?;
        let mut option_bytes = Vec::new();
        option_bytes.extend_from_slice(&destination_bytes);
        let flags = StreamingFlags::new(FLAG_CLOSE | FLAG_SIGNATURE_INCLUDED | FLAG_FROM_INCLUDED)
            .expect("CLOSE flag");
        let builder = StreamingPacketBuilder {
            send_stream_id: peer_receive_stream_id,
            receive_stream_id: local_receive_stream_id,
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
            send_stream_id: peer_receive_stream_id,
            receive_stream_id: local_receive_stream_id,
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
        let mut signed_wire = final_wire;
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
            send_stream_id: peer_receive_stream_id,
            receive_stream_id: local_receive_stream_id,
        };

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
        let local_receive_stream_id = conn.local_stream_id();
        let peer_receive_stream_id = conn.remote_stream_id();
        let _ = conn;

        let destination_bytes = local_dest
            .destination()
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .map_err(StreamingManagerError::I2npCodec)?;
        let mut option_bytes = Vec::new();
        option_bytes.extend_from_slice(&destination_bytes);
        let flags = StreamingFlags::new(FLAG_RESET | FLAG_SIGNATURE_INCLUDED | FLAG_FROM_INCLUDED)
            .expect("RESET flag");
        let builder = StreamingPacketBuilder {
            send_stream_id: peer_receive_stream_id,
            receive_stream_id: local_receive_stream_id,
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
            send_stream_id: peer_receive_stream_id,
            receive_stream_id: local_receive_stream_id,
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
        let mut signed_wire = final_wire;
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
            send_stream_id: peer_receive_stream_id,
            receive_stream_id: local_receive_stream_id,
        };
        self.outbound_queue.push_back(request.clone());

        Ok(request)
    }

    /// Returns the connection ID matching the local receive stream id
    /// (the id we use as `sendStreamId` for outbound packets and the
    /// id the peer uses to address us).
    pub fn lookup_outbound(&self, local_receive_stream_id: u32) -> Option<ConnectionId> {
        self.outbound_by_stream
            .get(&local_receive_stream_id)
            .copied()
    }

    /// Returns the connection ID matching the local receive stream id
    /// (the id the peer uses as `sendStreamId` to address us).
    pub fn lookup_inbound(&self, local_receive_stream_id: u32) -> Option<ConnectionId> {
        self.inbound_by_stream
            .get(&local_receive_stream_id)
            .copied()
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
    let total = packet.option_bytes.len();
    if total < 6 {
        return Err(StreamingManagerError::Codec(
            StreamingPacketError::SynMissingMaxPacketSize,
        ));
    }
    for offset in 0..=total.saturating_sub(6) {
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

/// Extracts the negotiated max packet size for the SYN response path.
/// When the SYN response packet is the one being inspected, the
/// caller's local advertised max is the upper bound; the negotiated
/// value is `min(remote, local)`.
fn extract_max_packet_size_for_response(
    _request: &TransportSendRequest,
    local_max: u32,
) -> Result<u32, StreamingManagerError> {
    // The local caller already supplies its own ceiling through
    // `max_packet_size`. Negotiation is handled by
    // `StreamingConnection::transition_established` which picks
    // `min(local, remote)`. We re-decode the response to recover the
    // peer's advertised value.
    let envelope = decode_client_payload(
        &_request.application_payload,
        i2pr_proto::streaming::MAX_STREAMING_PAYLOAD_BYTES + 256,
    )
    .map_err(|error| StreamingManagerError::Streaming(StreamingError::InboundEnvelope(error)))?;
    let limit = StreamingReceiveLimit::default();
    let (packet, _location) =
        decode_streaming_packet(&envelope.payload, limit).map_err(StreamingManagerError::Codec)?;
    let remote_max = extract_max_packet_size(&packet)?;
    Ok(remote_max.min(local_max.max(remote_max)))
}

fn decode_payload_from_request(
    request: &TransportSendRequest,
) -> Result<Vec<u8>, StreamingManagerError> {
    let envelope = decode_client_payload(
        &request.application_payload,
        i2pr_proto::streaming::MAX_STREAMING_PAYLOAD_BYTES + 256,
    )
    .map_err(|error| StreamingManagerError::Streaming(StreamingError::InboundEnvelope(error)))?;
    Ok(envelope.payload)
}

// Suppress an "unused import" warning for BTreeSet (kept for future use
// when we add per-connection ID sets to the pre-SYN buffer eviction policy).
#[allow(dead_code)]
type _Unused = BTreeSet<u32>;
