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
    CLOSE_FLAGS, ClientPayload, INITIAL_SYN_FLAGS, MAX_STREAMING_PACKET_BYTES, RESET_FLAGS,
    SYN_RESPONSE_FLAGS, SignatureLocation, StreamingFlags, StreamingOptionDecodeContext,
    StreamingOptions, StreamingPacket, StreamingPacketBuilder, StreamingPacketError,
    StreamingReceiveLimit, StreamingSendLimit, build_signature_preimage, decode_client_payload,
    decode_streaming_packet, encode_client_payload, encode_streaming_packet,
    encode_syn_replay_binding, install_packet_signature, peek_streaming_header,
    validate_initial_syn, validate_syn_response,
};
use i2pr_proto::{CodecError, SignatureValue};

use crate::identity::DestinationIdentity;
use crate::streaming::config::StreamingConfig;
use crate::streaming::connection::{
    ConnectionId, ConnectionState, ConnectionTransition, StreamDirection, StreamingConnection,
};
use crate::streaming::errors::StreamingError;
use crate::streaming::events::WirePacketObservation;
use crate::streaming::send_window::SendWindowDecision;
use crate::streaming::transport::TransportSendRequest;

/// Hard ceiling on the number of streams per local destination.
pub const MAX_STREAMS_PER_DESTINATION: usize =
    crate::streaming::config::MAX_STREAMS_PER_DESTINATION_LIMIT;

/// Hard ceiling on the pre-SYN unknown-stream reorder buffer.
pub const MAX_PRE_SYN_BUFFER: usize = 8;

/// Default local maximum Streaming payload bytes advertised through
/// the SYN's MAX_PACKET_SIZE option. This bounds the payload only;
/// the full encoded packet is larger (header + NACKs + options).
pub const DEFAULT_ADVERTISED_MAX_PAYLOAD: u16 =
    i2pr_proto::streaming::DEFAULT_ADVERTISED_MAX_PAYLOAD;

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
    #[error("no streaming listener matches destination port {destination_port}")]
    NoMatchingListener {
        /// The wire destination port no listener claimed.
        destination_port: u16,
    },
    #[error(
        "streaming port tuple mismatch: expected source {expected_source}/destination \
         {expected_destination}, got source {actual_source}/destination {actual_destination}"
    )]
    PortTupleMismatch {
        /// Local port recorded at establishment.
        expected_destination: u16,
        /// Remote port recorded at establishment.
        expected_source: u16,
        /// Wire source port of the rejected delivery.
        actual_source: u16,
        /// Wire destination port of the rejected delivery.
        actual_destination: u16,
    },
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

/// Tracked outbound packet for retransmission. Plan 129 owns the
/// integrated-path contract: the tracked record keeps the exact
/// [`TransportSendRequest`] that was originally queued so a
/// retransmission re-encodes nothing and re-signs nothing; it simply
/// traverses the gzip -> ECIES -> outbound-tunnel pipeline again.
#[derive(Clone, Debug)]
struct OutboundPacket {
    sequence: u32,
    payload_len: usize,
    sent_at_ms: u64,
    retransmit_count: u32,
    /// The original transport request (client-payload framing
    /// included) ready for redelivery.
    request: TransportSendRequest,
    /// Whether the packet was signed (SYN / SYN response / CLOSE / RESET).
    signed: bool,
}

/// Application bytes delivered in order by one processed inbound
/// packet. Surfaced through [`StreamingManager::drain_delivered`] so
/// the runtime adapter can hand the original byte order to the
/// application (Plan 129 §8 reorder contract).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveredApplicationBytes {
    /// Connection that received the bytes.
    pub connection_id: ConnectionId,
    /// Concatenated payloads in ascending sequence order.
    pub bytes: Vec<u8>,
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
    /// Per-connection standalone delayed-ACK deadlines (Plan 130 §7
    /// D3). Bounded by the connection count; entries are cancelled by
    /// any piggybacking outbound packet and purged on termination.
    pending_acks: BTreeMap<ConnectionId, u64>,
    /// Next connection ID.
    next_connection_id: u64,
    /// Next inbound stream ID candidate (the id we assign as our
    /// `local_receive_stream_id`).
    next_inbound_stream_id: u32,
    /// In-order application bytes delivered by processed inbound
    /// packets, awaiting the runtime drain.
    pending_delivered: VecDeque<DeliveredApplicationBytes>,
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
            pending_acks: BTreeMap::new(),
            next_connection_id: 1,
            // The non-zero range is canonical for I2P Streaming.
            next_inbound_stream_id: 0x8000_0000,
            pending_delivered: VecDeque::new(),
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
        advertised_max_payload: u16,
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
            advertised_max_payload,
        )?;

        let mut conn = StreamingConnection::new_outbound(
            connection_id,
            self.config.clone(),
            local_receive_stream_id,
            0,
            remote.signing_public_key.clone(),
            remote.destination_hash,
            local_port,
            remote_port,
            now_ms,
        );
        conn.set_local_advertised_max_payload(advertised_max_payload);
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

    /// Encodes, signs, and wraps one signed streaming packet
    /// (SYN / SYN response / CLOSE / RESET). The option region ends
    /// with a zeroed signature placeholder of the exact signing-key
    /// length; the complete placeholder packet is signed directly (it
    /// already equals the canonical preimage) and the real signature
    /// is patched into place.
    #[allow(clippy::too_many_arguments)]
    fn build_signed_packet(
        &self,
        local_dest: &DestinationIdentity,
        remote: &RemoteDestination,
        send_stream_id: u32,
        receive_stream_id: u32,
        sequence_num: u32,
        ack_through: u32,
        flags_bits: u16,
        options: &StreamingOptions,
        nacks: Vec<u32>,
        local_port: u16,
        remote_port: u16,
    ) -> Result<TransportSendRequest, StreamingManagerError> {
        let flags = StreamingFlags::new(flags_bits)?;
        let signature_length = local_dest
            .signing_public_key()
            .key_type()
            .signature_len()
            .ok_or(StreamingPacketError::SignatureContextUnavailable)?;
        let option_bytes = options.encode_with_placeholder(flags, signature_length)?;
        let builder = StreamingPacketBuilder {
            send_stream_id,
            receive_stream_id,
            sequence_num,
            ack_through,
            nacks,
            resend_delay: 0,
            flags,
            option_bytes,
            payload: Vec::new(),
        };
        let mut wire_bytes = encode_streaming_packet(&builder, StreamingSendLimit::default())?;
        // `wire_bytes` still carries the zeroed placeholder, so it is
        // exactly the canonical preimage.
        let signature = local_dest.sign(&wire_bytes)?;
        install_packet_signature(&mut wire_bytes, signature.as_bytes())?;

        let envelope = ClientPayload {
            protocol: i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
            source_port: local_port,
            destination_port: remote_port,
            payload: wire_bytes,
        };
        let application_bytes =
            encode_client_payload(&envelope).map_err(StreamingManagerError::OutboundEnvelope)?;

        Ok(TransportSendRequest {
            destination_hash: remote.destination_hash,
            source_port: local_port,
            destination_port: remote_port,
            application_payload: application_bytes,
            sequence: sequence_num,
            send_stream_id,
            receive_stream_id,
        })
    }

    /// Builds the originator SYN packet and wraps it in a Streaming
    /// client payload frame. Plan 128 §7: the SYN carries
    /// `INITIAL_SYN_FLAGS` (`0x04A9`), advertises the default maximum
    /// payload (1730), and carries eight replay-binding NACK words
    /// holding the remote Destination hash. The signature covers the
    /// replay hash through the canonical preimage.
    #[allow(clippy::too_many_arguments)]
    fn build_syn_packet(
        &self,
        local_dest: &DestinationIdentity,
        remote: &RemoteDestination,
        local_receive_stream_id: u32,
        local_port: u16,
        remote_port: u16,
        advertised_max_payload: u16,
    ) -> Result<TransportSendRequest, StreamingManagerError> {
        let options = StreamingOptions {
            delay_requested: None,
            from_destination: Some(local_dest.destination().clone()),
            max_payload_size: Some(advertised_max_payload),
            signature: None,
        };
        let nack_binding = encode_syn_replay_binding(&remote.destination_hash).to_vec();
        self.build_signed_packet(
            local_dest,
            remote,
            0, // sendStreamId = 0 for originator SYN
            local_receive_stream_id,
            0,
            0,
            INITIAL_SYN_FLAGS,
            &options,
            nack_binding,
            local_port,
            remote_port,
        )
    }

    /// Builds a signed SYN response packet after the local destination
    /// accepts an inbound SYN. Plan 128 §8: the response carries
    /// `SYN_RESPONSE_FLAGS` (`0x00A9`), zero replay NACKs, and a valid
    /// `ackThrough`; the response's `sendStreamId` is the originator's
    /// receive stream id and its `receiveStreamId` is the freshly
    /// selected local id. The `ackThrough` field carries this side's
    /// current acknowledgement state (Plan 130 §7: it acknowledges the
    /// peer's sequence-0 SYN).
    #[allow(clippy::too_many_arguments)]
    fn build_syn_response_packet(
        &self,
        local_dest: &DestinationIdentity,
        remote: &RemoteDestination,
        inbound_connection: &StreamingConnection,
        local_port: u16,
        remote_port: u16,
        advertised_max_payload: u16,
    ) -> Result<TransportSendRequest, StreamingManagerError> {
        let options = StreamingOptions {
            delay_requested: None,
            from_destination: Some(local_dest.destination().clone()),
            max_payload_size: Some(advertised_max_payload),
            signature: None,
        };
        let (ack_through, nacks) = inbound_connection.recv_window().ack_view();
        self.build_signed_packet(
            local_dest,
            remote,
            inbound_connection.remote_stream_id(),
            inbound_connection.local_stream_id(),
            0,
            ack_through,
            SYN_RESPONSE_FLAGS,
            &options,
            nacks,
            local_port,
            remote_port,
        )
    }

    /// Drains the outbound transport send queue.
    pub fn drain_outbound(&mut self) -> Vec<TransportSendRequest> {
        self.outbound_queue.drain(..).collect()
    }

    /// Queues a single transport send request into the outbound
    /// queue. Used by callers that produce requests from
    /// `accept_inbound_syn` / direct session sealing paths where the
    /// manager does not own the corresponding state transition. The
    /// request is appended at the queue tail.
    pub fn queue_outbound_packet(&mut self, request: TransportSendRequest) {
        self.outbound_queue.push_back(request);
    }

    /// Returns the number of pending outbound transport requests.
    pub fn outbound_queue_len(&self) -> usize {
        self.outbound_queue.len()
    }

    /// Processes a streaming payload envelope received from the
    /// transport. The envelope is the protocol-6 client payload frame
    /// that wraps every inbound streaming packet. The envelope's wire
    /// ports are authoritative (Plan 130 §8): the caller cannot
    /// redirect a delivery to another listener.
    pub fn process_inbound_envelope(
        &mut self,
        wire_bytes: &[u8],
        from_destination_hash: &[u8; 32],
        to_destination: &DestinationIdentity,
        source_port: u16,
        destination_port: u16,
        now_ms: u64,
    ) -> Result<WirePacketObservation, StreamingManagerError> {
        let envelope = decode_client_payload(wire_bytes, MAX_STREAMING_PACKET_BYTES + 256)
            .map_err(|error| {
                StreamingManagerError::Streaming(StreamingError::InboundEnvelope(error))
            })?;
        if envelope.source_port != source_port || envelope.destination_port != destination_port {
            return Err(StreamingManagerError::PortTupleMismatch {
                expected_destination: destination_port,
                expected_source: source_port,
                actual_source: envelope.source_port,
                actual_destination: envelope.destination_port,
            });
        }
        let streaming_bytes = envelope.payload;
        self.process_inbound_packet(
            &streaming_bytes,
            from_destination_hash,
            to_destination,
            source_port,
            destination_port,
            now_ms,
        )
    }

    /// Processes a raw inbound streaming packet (after the protocol-6
    /// client payload envelope has been stripped). The `source_port`
    /// and `destination_port` arguments are the **wire** I2P ports
    /// decoded from the client payload; they select the listener for
    /// an inbound SYN and are validated against the established port
    /// tuple otherwise (Plan 130 §8).
    ///
    /// The packet header is peeked first to route the packet; the full
    /// strict decode then runs with the option context the route
    /// requires (SYN/SYN-response packets carry FROM; established
    /// connection control packets verify against the retained peer
    /// signing key).
    #[allow(clippy::too_many_arguments)]
    pub fn process_inbound_packet(
        &mut self,
        wire_bytes: &[u8],
        from_destination_hash: &[u8; 32],
        to_destination: &DestinationIdentity,
        source_port: u16,
        destination_port: u16,
        now_ms: u64,
    ) -> Result<WirePacketObservation, StreamingManagerError> {
        let peek = peek_streaming_header(wire_bytes)?;
        let flags_bits = peek.flags_bits & !i2pr_proto::streaming::FLAG_RESERVED_MASK;

        // Inbound SYN (originator): packet.sendStreamId == 0 AND
        // packet.receiveStreamId != 0 (the originator picked an id
        // for us to address them by).
        if flags_bits & i2pr_proto::streaming::FLAG_SYNCHRONIZE != 0
            && peek.send_stream_id == 0
            && peek.receive_stream_id != 0
        {
            let limit = StreamingReceiveLimit::default();
            let (packet, signature_location) = decode_streaming_packet(
                wire_bytes,
                limit,
                StreamingOptionDecodeContext::anonymous(),
            )
            .map_err(StreamingManagerError::Codec)?;
            return self.handle_inbound_syn(
                &packet,
                signature_location,
                wire_bytes,
                from_destination_hash,
                to_destination,
                source_port,
                destination_port,
                now_ms,
            );
        }

        // SYN response (recipient): packet.sendStreamId == our
        // local_receive_stream_id AND packet.receiveStreamId != 0
        // (the recipient picked an id for us to address them by).
        if flags_bits & i2pr_proto::streaming::FLAG_SYNCHRONIZE != 0
            && peek.send_stream_id != 0
            && peek.receive_stream_id != 0
        {
            // Plan 131 §7 D2: validate the decoded ClientPayload
            // source/destination ports against the outbound
            // connection established by `connect()`. The wire
            // response must arrive on the exact port tuple our SYN
            // requested, otherwise the connection stays in
            // `OutboundSynSent` and the wrong-port response is
            // discarded.
            let outbound_id = self.outbound_by_stream.get(&peek.send_stream_id).copied();
            let tuple_ok = outbound_id
                .and_then(|cid| self.connections.get(&cid))
                .is_some_and(|conn| conn.ports_match(source_port, destination_port));
            if !tuple_ok {
                let (expected_source, expected_destination) = outbound_id
                    .and_then(|cid| self.connections.get(&cid))
                    .map(|conn| (conn.remote_port(), conn.local_port()))
                    .unwrap_or((0, 0));
                return Err(StreamingManagerError::PortTupleMismatch {
                    expected_destination,
                    expected_source,
                    actual_source: source_port,
                    actual_destination: destination_port,
                });
            }
            let limit = StreamingReceiveLimit::default();
            let (packet, signature_location) = decode_streaming_packet(
                wire_bytes,
                limit,
                StreamingOptionDecodeContext::anonymous(),
            )
            .map_err(StreamingManagerError::Codec)?;
            return self.handle_inbound_syn_response(
                &packet,
                signature_location,
                wire_bytes,
                from_destination_hash,
                now_ms,
            );
        }

        // Data / CLOSE / RESET / plain ACK on an established
        // connection. Signed control packets without FROM verify
        // against the retained peer signing key, so resolve the
        // connection before the full strict decode supplies its option
        // context.
        let connection_id = self
            .inbound_by_stream
            .get(&peek.send_stream_id)
            .copied()
            .or_else(|| self.outbound_by_stream.get(&peek.send_stream_id).copied())
            .ok_or(StreamingManagerError::UnknownConnection)?;

        // Plan 130 §8 E3: established traffic must carry the exact
        // port tuple fixed by the handshake. A mismatched delivery is
        // rejected without corrupting connection state.
        let tuple_ok = self
            .connections
            .get(&connection_id)
            .map(|conn| conn.ports_match(source_port, destination_port))
            .unwrap_or(false);
        if !tuple_ok {
            let (expected_source, expected_destination) = self
                .connections
                .get(&connection_id)
                .map(|conn| (conn.remote_port(), conn.local_port()))
                .expect("connection exists");
            return Err(StreamingManagerError::PortTupleMismatch {
                expected_destination,
                expected_source,
                actual_source: source_port,
                actual_destination: destination_port,
            });
        }

        let peer_signing_key = self
            .connections
            .get(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?
            .peer_signing_key()
            .clone();
        let limit = StreamingReceiveLimit::default();
        let (packet, signature_location) = decode_streaming_packet(
            wire_bytes,
            limit,
            StreamingOptionDecodeContext::with_peer_key(&peer_signing_key),
        )
        .map_err(StreamingManagerError::Codec)?;
        self.handle_data_packet(
            &packet,
            signature_location,
            wire_bytes,
            from_destination_hash,
            connection_id,
            now_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_inbound_syn(
        &mut self,
        packet: &StreamingPacket,
        signature_location: Option<SignatureLocation>,
        wire_bytes: &[u8],
        from_destination_hash: &[u8; 32],
        to_destination: &DestinationIdentity,
        source_port: u16,
        destination_port: u16,
        now_ms: u64,
    ) -> Result<WirePacketObservation, StreamingManagerError> {
        // The SYN replay binding NACK field carries the receiver
        // (local destination) hash.
        let local_destination_hash: [u8; 32] = *to_destination
            .destination()
            .hash()
            .map_err(StreamingManagerError::I2npCodec)?
            .as_bytes();
        validate_initial_syn(packet, &local_destination_hash)?;

        // Verify the originator's signature over the canonical
        // preimage (the full packet with the raw signature zeroed)
        // against the FROM destination's signing key.
        let destination =
            packet
                .options
                .from_destination
                .clone()
                .ok_or(StreamingManagerError::Codec(
                    StreamingPacketError::SynMissingFrom,
                ))?;
        let location = signature_location.ok_or(StreamingManagerError::Codec(
            StreamingPacketError::SignatureMissing,
        ))?;
        let preimage = build_signature_preimage(wire_bytes, Some(location));
        let signature = packet
            .options
            .signature
            .clone()
            .ok_or(StreamingManagerError::Codec(
                StreamingPacketError::SignatureMissing,
            ))?;
        let signature_value = SignatureValue::new(destination.signing_key().key_type(), signature)
            .map_err(|_| {
                StreamingManagerError::Codec(StreamingPacketError::SignatureContextUnavailable)
            })?;
        verify_signature(destination.signing_key(), &preimage, &signature_value)
            .map_err(|_| StreamingManagerError::Codec(StreamingPacketError::SignatureInvalid))?;

        // Allocate a new inbound connection. The originator's
        // `receiveStreamId` is the id they want us to use in our
        // `sendStreamId`; from our perspective that's their
        // `remote_stream_id` (which we send to them as
        // `receiveStreamId`). The peer signing key is retained so
        // later signed control packets can verify without FROM.
        let connection_id = ConnectionId::new(self.next_connection_id);
        self.next_connection_id = self.next_connection_id.saturating_add(1);
        let local_receive_stream_id = self.allocate_inbound_stream_id();
        let remote_send_stream_id = packet.receive_stream_id;
        let remote_advertised_max = packet.options.max_payload_size;
        let mut conn = StreamingConnection::new_inbound(
            connection_id,
            self.config.clone(),
            local_receive_stream_id,
            remote_send_stream_id,
            destination.signing_key().clone(),
            *from_destination_hash,
            destination_port,
            source_port,
            now_ms,
        );
        if let Some(max) = remote_advertised_max {
            conn.set_remote_advertised_max_payload(max);
        }
        // Retain the authenticated public Destination so a SAM ACCEPT
        // endpoint can emit peer metadata without reconstructing it from
        // only a hash or an untrusted command argument.
        conn.set_peer_destination(destination.clone());
        conn.set_local_advertised_max_payload(DEFAULT_ADVERTISED_MAX_PAYLOAD);
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

        // Plan 130 §8 E2: listener selection follows the reference
        // I2P demultiplexer contract. The wire `destination_port` is
        // authoritative: an exact listener wins; otherwise a listener
        // explicitly bound to the wildcard port 0 (`PORT_ANY`)
        // catches the delivery. No listener is created by side
        // effect, and unclaimed ports fail closed with a typed
        // error instead of entering any backlog.
        let matched_listener = if self.listeners.contains_key(&destination_port) {
            destination_port
        } else if self.listeners.contains_key(&0) {
            0
        } else {
            self.connections.remove(&connection_id);
            self.inbound_by_stream.remove(&local_receive_stream_id);
            self.outbound_packets.remove(&connection_id);
            return Err(StreamingManagerError::NoMatchingListener { destination_port });
        };
        let backlog_full = self
            .listeners
            .get(&matched_listener)
            .map(|q| q.len() >= self.config.max_listener_backlog as usize)
            .unwrap_or(false);
        if backlog_full {
            self.connections.remove(&connection_id);
            self.inbound_by_stream.remove(&local_receive_stream_id);
            self.outbound_packets.remove(&connection_id);
            return Err(StreamingManagerError::ListenerBacklogFull);
        }
        if let Some(entry) = self.listeners.get_mut(&matched_listener) {
            entry.push_back(connection_id);
        }

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
        advertised_max_payload: u16,
        now_ms: u64,
        _rng: &mut R,
    ) -> Result<TransportSendRequest, StreamingManagerError> {
        // Plan 130 §8 E3: the accept path may not redirect the stream
        // to a different port tuple than the SYN established.
        {
            let conn = self
                .connections
                .get(&connection_id)
                .ok_or(StreamingManagerError::UnknownConnection)?;
            if conn.local_port() != local_port || conn.remote_port() != remote_port {
                return Err(StreamingManagerError::PortTupleMismatch {
                    expected_destination: conn.local_port(),
                    expected_source: conn.remote_port(),
                    actual_source: remote_port,
                    actual_destination: local_port,
                });
            }
        }
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
            advertised_max_payload,
        )?;
        let remote_max = conn.remote_advertised_max_payload();
        let conn = self
            .connections
            .get_mut(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?;
        // Record our own advertisement so negotiation is
        // `min(local advertised, remote advertised)`.
        conn.set_local_advertised_max_payload(advertised_max_payload);
        // Negotiation is `min(local, remote)`; the connection records
        // the peer's advertised payload max from its SYN.
        let negotiated = remote_max.unwrap_or(DEFAULT_ADVERTISED_MAX_PAYLOAD);
        conn.transition_established(u32::from(negotiated), now_ms)
            .map_err(StreamingManagerError::Streaming)?;
        // Track the SYN response packet for retransmission until the
        // originator confirms with a non-SYN packet.
        let outbound = OutboundPacket {
            sequence: 0,
            payload_len: 0,
            sent_at_ms: now_ms,
            retransmit_count: 0,
            request: request.clone(),
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
        signature_location: Option<SignatureLocation>,
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

        validate_syn_response(packet)?;

        // Verify the responder's signature over the canonical
        // preimage against the FROM destination's signing key.
        let destination =
            packet
                .options
                .from_destination
                .clone()
                .ok_or(StreamingManagerError::Codec(
                    StreamingPacketError::SynMissingFrom,
                ))?;
        let location = signature_location.ok_or(StreamingManagerError::Codec(
            StreamingPacketError::SignatureMissing,
        ))?;
        let preimage = build_signature_preimage(wire_bytes, Some(location));
        let signature = packet
            .options
            .signature
            .clone()
            .ok_or(StreamingManagerError::Codec(
                StreamingPacketError::SignatureMissing,
            ))?;
        let signature_value = SignatureValue::new(destination.signing_key().key_type(), signature)
            .map_err(|_| {
                StreamingManagerError::Codec(StreamingPacketError::SignatureContextUnavailable)
            })?;
        verify_signature(destination.signing_key(), &preimage, &signature_value)
            .map_err(|_| StreamingManagerError::Codec(StreamingPacketError::SignatureInvalid))?;

        let remote_max = packet.options.max_payload_size;
        let conn = self
            .connections
            .get_mut(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?;
        if let Some(max) = remote_max {
            conn.set_remote_advertised_max_payload(max);
        }
        // Plan 125 §6 / Plan 128 §12: peer receive stream id is the
        // `receiveStreamId` the SYN response supplied; set it on the
        // outbound connection before transitioning to Established.
        // Negotiated payload max is `min(local, remote)`.
        let negotiated = remote_max.unwrap_or(DEFAULT_ADVERTISED_MAX_PAYLOAD);
        conn.set_remote_stream_id(packet.receive_stream_id);
        conn.transition_established(u32::from(negotiated), now_ms)
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
        signature_location: Option<SignatureLocation>,
        wire_bytes: &[u8],
        from_destination_hash: &[u8; 32],
        connection_id: ConnectionId,
        now_ms: u64,
    ) -> Result<WirePacketObservation, StreamingManagerError> {
        // CLOSE and RESET require SIGNATURE_INCLUDED; FROM is not
        // required since 0.9.20 because verification uses the peer
        // signing key retained in connection state.
        if packet.flags.close() && !packet.flags.signature_included() {
            return Err(StreamingManagerError::Codec(
                StreamingPacketError::CloseMissingSignature,
            ));
        }
        if packet.flags.reset() && !packet.flags.signature_included() {
            return Err(StreamingManagerError::Codec(
                StreamingPacketError::ResetMissingSignature,
            ));
        }
        let peer_signing_key = self
            .connections
            .get(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?
            .peer_signing_key()
            .clone();

        // Verify any signed packet (CLOSE / RESET / signed data)
        // against the retained peer identity.
        if packet.flags.signature_included() {
            let expected = peer_signing_key
                .key_type()
                .signature_len()
                .ok_or(StreamingPacketError::SignatureContextUnavailable)?;
            let signature =
                packet
                    .options
                    .signature
                    .as_ref()
                    .ok_or(StreamingManagerError::Codec(
                        StreamingPacketError::SignatureMissing,
                    ))?;
            if signature.len() != expected {
                return Err(StreamingManagerError::Codec(
                    StreamingPacketError::SignatureLengthMismatch {
                        expected,
                        actual: signature.len(),
                    },
                ));
            }
            let location = signature_location.ok_or(StreamingManagerError::Codec(
                StreamingPacketError::SignatureMissing,
            ))?;
            let preimage = build_signature_preimage(wire_bytes, Some(location));
            let signature_value = SignatureValue::new(
                peer_signing_key.key_type(),
                signature.clone(),
            )
            .map_err(|_| {
                StreamingManagerError::Codec(StreamingPacketError::SignatureLengthMismatch {
                    expected,
                    actual: signature.len(),
                })
            })?;
            verify_signature(&peer_signing_key, &preimage, &signature_value).map_err(|_| {
                StreamingManagerError::Codec(StreamingPacketError::SignatureInvalid)
            })?;
        }

        let conn = self
            .connections
            .get_mut(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?;

        // Plan 129 §10/§11: control flags first, then delivery.
        // A CLOSE received while Established moves the connection to
        // `ClosingRemote` (draining); a CLOSE received while the
        // local side already sent its own CLOSE (`ClosingLocal`)
        // completes the graceful close — the local side never marks
        // itself Closed merely because it queued a CLOSE.
        if packet.flags.close() {
            match conn.state() {
                ConnectionState::Established => {
                    let _ = conn.remote_close_received(now_ms);
                }
                ConnectionState::ClosingLocal => {
                    let _ = conn.close(now_ms);
                }
                _ => {}
            }
        }
        if packet.flags.reset() {
            let _ = conn.reset(now_ms);
        }

        // Plan 130 §7 D1: acknowledgement information is present
        // whenever the packet semantics carry it. Numeric zero is a
        // valid cumulative ACK (it acknowledges sequence 0); only the
        // NO_ACK flag suppresses processing. The previous
        // "ackThrough == 0 means no ack" reading was wrong.
        let terminated = matches!(
            conn.state(),
            ConnectionState::Closed | ConnectionState::Reset
        );
        if !packet.flags.no_ack() && !terminated {
            let observation = conn.receive_ack(packet.ack_through, &packet.nacks, now_ms);
            let _ = observation;
            if let Some(tracked) = self.outbound_packets.get_mut(&connection_id) {
                // NACK-aware clearing (Plan 130 §7 D4): sequences the
                // receiver explicitly NACKed stay tracked so their
                // retransmission records survive; everything else at
                // or below ack_through is cleared.
                let nacked: std::collections::BTreeSet<u32> = packet
                    .nacks
                    .iter()
                    .copied()
                    .filter(|&sequence| sequence < packet.ack_through)
                    .collect();
                let covered: Vec<u32> = tracked
                    .range(..=packet.ack_through)
                    .map(|(&sequence, _)| sequence)
                    .filter(|sequence| !nacked.contains(sequence))
                    .collect();
                for sequence in covered {
                    tracked.remove(&sequence);
                }
            }
        }

        // Plan 130 §6 C2: sequence 0 without SYNCHRONIZE is the
        // plain-ACK control form (or a hostile seq-0 data attempt).
        // It never enters the application receive window: no
        // delivery, no reorder buffering, no delivered-count advance,
        // and no new pending ACK (an ACK-only packet must not cause
        // an ACK-of-ACK loop).
        let plain_ack_form = packet.sequence_num == 0 && !packet.flags.synchronize();
        let mut decision_opt = None;
        if !terminated && !plain_ack_form {
            let payload = packet.payload.clone();
            let decision = conn
                .receive_packet(packet.sequence_num, payload, now_ms)
                .map_err(StreamingManagerError::Streaming)?;
            decision_opt = Some(decision);
        }

        let state = conn.state();
        // Plan 130 §7 D3/D5: newly received data — delivered in order
        // OR accepted into the reorder buffer — schedules (or keeps)
        // one coalescing standalone ACK per connection with a bounded
        // deadline, so reorder feedback reaches the peer without
        // waiting for reverse application traffic. Duplicates and
        // window-overflow drops do not extend any deadline; a
        // terminated connection never retains a pending standalone
        // ACK.
        let received_new_data = matches!(
            decision_opt,
            Some(crate::streaming::recv_window::RecvWindowDecision::Delivered { .. })
                | Some(crate::streaming::recv_window::RecvWindowDecision::Buffered { .. })
        );
        let connection_active = !matches!(
            state,
            ConnectionState::ClosingLocal
                | ConnectionState::ClosingRemote
                | ConnectionState::Closed
                | ConnectionState::Reset
        );
        let delay_requested_now =
            received_new_data && connection_active && packet.options.delay_requested == Some(0);
        let _ = conn;

        if terminated {
            self.pending_acks.remove(&connection_id);
        }

        if received_new_data && connection_active {
            // An explicit zero-delay request from the peer is honored
            // immediately (specification: delay value 0 requests an
            // immediate ack); otherwise the reference default delayed-
            // ACK deadline applies.
            if delay_requested_now {
                let request = self.build_simple_ack_request(connection_id);
                if let Some(request) = request {
                    self.outbound_queue.push_back(request);
                }
            } else {
                let deadline = now_ms.saturating_add(self.config.delayed_ack_ms);
                self.pending_acks.entry(connection_id).or_insert(deadline);
            }
        }

        // Surface in-order delivered payloads (including reorder
        // buffer drains) so the adapter observes the original byte
        // order (Plan 129 §8).
        if let Some(crate::streaming::recv_window::RecvWindowDecision::Delivered {
            delivered,
            ..
        }) = decision_opt
        {
            let mut bytes = Vec::new();
            for entry in delivered {
                bytes.extend_from_slice(&entry.payload);
            }
            self.pending_delivered.push_back(DeliveredApplicationBytes {
                connection_id,
                bytes,
            });
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

        Ok(observation)
    }

    /// Sends application data over an established connection. Returns
    /// the [`TransportSendRequest`] carrying the serialized data packet
    /// ready for the runtime to dispatch.
    ///
    /// Plan 131 §7 D1: the connection owns its I2P port tuple after
    /// the handshake completes. The `local_port` / `remote_port`
    /// parameters are therefore treated only as **assertions** — if
    /// they do not match the connection-stored tuple, the call fails
    /// closed with [`StreamingManagerError::PortTupleMismatch`] before
    /// any sequence is allocated, send-window state is touched, or
    /// outbound queue state mutates. The wire ClientPayload ports
    /// always come from the stored connection.
    ///
    /// Plan 131 §8 E1: oversized writes are rejected **before**
    /// sequence allocation, send-window mutation, retransmit
    /// tracking, or outbound queue mutation. A rejected write
    /// therefore consumes no sequence and creates no state.
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
        // 1. Look up the connection (no state mutation yet).
        let conn = self
            .connections
            .get(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?;

        // 2. Validate state.
        if conn.state() != ConnectionState::Established {
            return Err(StreamingManagerError::InvalidConnectionState);
        }

        // 3. Validate caller port assertion against the
        //    connection-stored tuple. A mismatch is a typed error
        //    and never reaches state mutation.
        if conn.local_port() != local_port || conn.remote_port() != remote_port {
            return Err(StreamingManagerError::PortTupleMismatch {
                expected_destination: conn.local_port(),
                expected_source: conn.remote_port(),
                actual_source: remote_port,
                actual_destination: local_port,
            });
        }

        // 4. Validate payload size against the negotiated maximum
        //    **before** sequence allocation. Plan 131 §8 E1.
        let max_payload = conn.max_payload_size() as usize;
        if payload.len() > max_payload {
            return Err(StreamingManagerError::Streaming(
                StreamingError::PayloadTooLarge {
                    actual: payload.len(),
                    maximum: max_payload,
                },
            ));
        }

        // 5. Validate send-window capacity (Plan 131 §8 E1). The
        //    window state is still untouched here; only the next
        //    `enqueue_send` will mutate it on success.
        if conn.send_window().evaluate(payload.len()) == SendWindowDecision::Backpressure {
            return Err(StreamingManagerError::Streaming(
                StreamingError::SendWindowFull,
            ));
        }

        // Snapshot every piece of connection-owned state the
        // encoding step will need. The connection's outbound
        // identifiers and ports are owned by the connection
        // record; the caller-supplied ports were validated above
        // and are no longer consulted.
        let planned_sequence = conn.send_window().next_sequence();
        let local_receive_stream_id = conn.local_stream_id();
        let peer_receive_stream_id = conn.remote_stream_id();
        let (ack_through, ack_nacks) = conn.recv_window().ack_view();
        let wire_local_port = conn.local_port();
        let wire_remote_port = conn.remote_port();
        let _ = conn;

        // ---- Phase 2: build every fallible wire artifact ----------------------
        // The data packet uses our peer receive stream id as
        // `sendStreamId` and our local receive stream id as
        // `receiveStreamId`. Every data packet piggybacks this
        // side's current acknowledgement state (`ackThrough` =
        // highest received sequence, plus any bounded
        // missing-sequence NACKs).
        let flags = StreamingFlags::new(0).expect("empty flags");
        let builder = StreamingPacketBuilder {
            send_stream_id: peer_receive_stream_id,
            receive_stream_id: local_receive_stream_id,
            sequence_num: planned_sequence,
            ack_through,
            nacks: ack_nacks,
            resend_delay: 0,
            flags,
            option_bytes: Vec::new(),
            payload: payload.to_vec(),
        };
        let wire_bytes = encode_streaming_packet(&builder, StreamingSendLimit::default())
            .map_err(StreamingManagerError::Codec)?;

        let envelope = ClientPayload {
            protocol: i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
            source_port: wire_local_port,
            destination_port: wire_remote_port,
            payload: wire_bytes,
        };
        let application_bytes =
            encode_client_payload(&envelope).map_err(StreamingManagerError::OutboundEnvelope)?;

        let request = TransportSendRequest {
            destination_hash: remote.destination_hash,
            source_port: wire_local_port,
            destination_port: wire_remote_port,
            application_payload: application_bytes,
            sequence: planned_sequence,
            send_stream_id: peer_receive_stream_id,
            receive_stream_id: local_receive_stream_id,
        };

        // ---- Phase 3: protocol-state commit (single fallible mutation) --------
        // Every commit point below must be infallible or have an
        // explicit rollback. The first commit is `enqueue_send`;
        // every subsequent step is therefore infallible. The
        // sequence the window assigns must equal `planned_sequence`;
        // any divergence is a programming error.
        let conn = self
            .connections
            .get_mut(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?;
        let sequence = conn
            .enqueue_send(payload.len(), now_ms)
            .map_err(StreamingManagerError::Streaming)?;
        assert_eq!(
            sequence, planned_sequence,
            "send_window must assign the planned sequence on commit"
        );

        let outbound = OutboundPacket {
            sequence,
            payload_len: payload.len(),
            sent_at_ms: now_ms,
            retransmit_count: 0,
            request: request.clone(),
            signed: false,
        };
        self.outbound_packets
            .entry(connection_id)
            .or_default()
            .insert(sequence, outbound);
        self.outbound_queue.push_back(request.clone());
        // The piggybacked acknowledgement state satisfies any
        // pending standalone ACK.
        self.pending_acks.remove(&connection_id);

        let _ = local_dest;
        let _ = remote;
        Ok(request)
    }

    /// Builds and queues a signed CLOSE packet for the given
    /// connection. Plan 128 §9: CLOSE carries `CLOSE_FLAGS`
    /// (`0x000A`, CLOSE | SIGNATURE_INCLUDED) with the raw signature
    /// as the final option field; FROM is not included.
    ///
    /// Plan 129 §10 graceful-close policy: calling this from
    /// `Established` begins the local close (`ClosingLocal`) and the
    /// side stays there until the peer's CLOSE response arrives over
    /// the reverse destination path. Calling this from
    /// `ClosingRemote` (a close already received from the peer)
    /// emits the required CLOSE response and completes the local
    /// half of the shutdown.
    ///
    /// Plan 131 §7 D1: the connection owns its I2P port tuple. The
    /// `local_port` / `remote_port` parameters are asserted against
    /// the stored tuple; mismatches fail closed before any state
    /// transition. Plan 131 §8 E3 extends the same guard to CLOSE
    /// and RESET so neither can be redirected by caller-supplied
    /// ports.
    pub fn send_close(
        &mut self,
        connection_id: ConnectionId,
        local_dest: &DestinationIdentity,
        remote: &RemoteDestination,
        local_port: u16,
        remote_port: u16,
        now_ms: u64,
    ) -> Result<TransportSendRequest, StreamingManagerError> {
        // ---- Phase 1: immutable validation ------------------------------------
        // Validate the port assertion and the pre-state-transition state
        // before any mutation. A mismatched port tuple, or an invalid
        // starting state, returns a typed error without touching the
        // connection.
        let (local_receive_stream_id, peer_receive_stream_id, was_closing_remote) = {
            let conn = self
                .connections
                .get(&connection_id)
                .ok_or(StreamingManagerError::UnknownConnection)?;
            if conn.local_port() != local_port || conn.remote_port() != remote_port {
                return Err(StreamingManagerError::PortTupleMismatch {
                    expected_destination: conn.local_port(),
                    expected_source: conn.remote_port(),
                    actual_source: remote_port,
                    actual_destination: local_port,
                });
            }
            match conn.state() {
                ConnectionState::Established | ConnectionState::ClosingRemote => {}
                other => {
                    return Err(StreamingManagerError::Streaming(
                        StreamingError::InvalidStateTransition {
                            from: other.label(),
                            to: "ClosingLocal",
                        },
                    ));
                }
            }
            (
                conn.local_stream_id(),
                conn.remote_stream_id(),
                conn.state() == ConnectionState::ClosingRemote,
            )
        };

        // Snapshot everything needed to build the CLOSE request.
        let conn_snapshot = self
            .connections
            .get(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?;
        // Plan 130 §6 C1: CLOSE is an ordinary message in the stream
        // sequence space (only plain ACKs and retransmissions are
        // exempt), so it allocates the next sequence number.
        let sequence_num = conn_snapshot.send_window().next_sequence();
        let (ack_through, ack_nacks) = conn_snapshot.recv_window().ack_view();
        let wire_local_port = conn_snapshot.local_port();
        let wire_remote_port = conn_snapshot.remote_port();
        let _ = conn_snapshot;

        // ---- Phase 2: build the signed CLOSE request (fallible) ---------------
        let options = StreamingOptions {
            delay_requested: None,
            from_destination: None,
            max_payload_size: None,
            signature: None,
        };
        let request = self.build_signed_packet(
            local_dest,
            remote,
            peer_receive_stream_id,
            local_receive_stream_id,
            sequence_num,
            ack_through,
            CLOSE_FLAGS,
            &options,
            ack_nacks,
            wire_local_port,
            wire_remote_port,
        )?;

        // ---- Phase 3: protocol-state commit (infallible after build) -----------
        // Begin the close transition only after the wire request has
        // been built and signed. A failure to build leaves the
        // connection in its prior state.
        let conn = self
            .connections
            .get_mut(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?;
        if conn.state() == ConnectionState::Established {
            conn.begin_close(now_ms)
                .map_err(StreamingManagerError::Streaming)?;
        }

        // A CLOSE emitted from `ClosingRemote` answers the peer's
        // CLOSE; once our own CLOSE is on the wire this side's half
        // of the shutdown is complete.
        if was_closing_remote {
            let _ = conn.close(now_ms);
        }

        let outbound = OutboundPacket {
            sequence: sequence_num,
            payload_len: 0,
            sent_at_ms: now_ms,
            retransmit_count: 0,
            request: request.clone(),
            signed: true,
        };
        self.outbound_packets
            .entry(connection_id)
            .or_default()
            .insert(sequence_num, outbound);
        self.outbound_queue.push_back(request.clone());
        self.pending_acks.remove(&connection_id);

        Ok(request)
    }

    /// Builds and queues a signed RESET packet. Plan 128 §9: RESET
    /// carries `RESET_FLAGS` (`0x000C`, RESET | SIGNATURE_INCLUDED)
    /// with the raw signature as the final option field; FROM is not
    /// required since 0.9.20.
    ///
    /// Plan 131 §8 E3: the caller-supplied ports are asserted
    /// against the stored tuple before the state transition. A
    /// mismatch fails closed without touching the connection.
    pub fn send_reset(
        &mut self,
        connection_id: ConnectionId,
        local_dest: &DestinationIdentity,
        remote: &RemoteDestination,
        local_port: u16,
        remote_port: u16,
        now_ms: u64,
    ) -> Result<TransportSendRequest, StreamingManagerError> {
        // ---- Phase 1: immutable validation ------------------------------------
        // Validate the port assertion before any state mutation. A
        // mismatched port tuple returns a typed error without
        // touching the connection.
        let (local_receive_stream_id, peer_receive_stream_id, ack_through, ack_nacks) = {
            let conn = self
                .connections
                .get(&connection_id)
                .ok_or(StreamingManagerError::UnknownConnection)?;
            if conn.local_port() != local_port || conn.remote_port() != remote_port {
                return Err(StreamingManagerError::PortTupleMismatch {
                    expected_destination: conn.local_port(),
                    expected_source: conn.remote_port(),
                    actual_source: remote_port,
                    actual_destination: local_port,
                });
            }
            let (ack_through, ack_nacks) = conn.recv_window().ack_view();
            (
                conn.local_stream_id(),
                conn.remote_stream_id(),
                ack_through,
                ack_nacks,
            )
        };

        // ---- Phase 2: build the signed RESET request (fallible) ---------------
        let options = StreamingOptions {
            delay_requested: None,
            from_destination: None,
            max_payload_size: None,
            signature: None,
        };
        let request = self.build_signed_packet(
            local_dest,
            remote,
            peer_receive_stream_id,
            local_receive_stream_id,
            0,
            ack_through,
            RESET_FLAGS,
            &options,
            ack_nacks,
            local_port,
            remote_port,
        )?;

        // ---- Phase 3: protocol-state commit (infallible after build) -----------
        // Reset the connection only after the wire request has been
        // built and signed. A failure to build leaves the connection
        // in its prior state.
        let conn = self
            .connections
            .get_mut(&connection_id)
            .ok_or(StreamingManagerError::UnknownConnection)?;
        conn.reset(now_ms)
            .map_err(StreamingManagerError::Streaming)?;

        // Plan 130 §7 D3: a reset connection never retains a pending
        // standalone ACK, and the RESET carries the final cumulative
        // acknowledgement state.
        self.pending_acks.remove(&connection_id);

        self.outbound_queue.push_back(request.clone());

        Ok(request)
    }

    /// Builds one unsigned plain-ACK request for the connection's
    /// current acknowledgement state (Plan 130 §7 D2). The packet
    /// follows the reference simple-ACK form: sequence number 0,
    /// SYNCHRONIZE clear, no payload, valid cumulative `ackThrough`
    /// and bounded NACK fields. Per the Streaming specification a
    /// plain ACK "should not be ACKed", so receiving it never
    /// schedules another acknowledgement — no ACK-of-ACK loop.
    ///
    /// Returns `None` when the connection vanished between the
    /// deadline scan and this build.
    fn build_simple_ack_request(
        &mut self,
        connection_id: ConnectionId,
    ) -> Option<TransportSendRequest> {
        let conn = self.connections.get(&connection_id)?;
        let (ack_through, nacks) = conn.recv_window().ack_view();
        let flags = StreamingFlags::new(0).expect("empty flags");
        let builder = StreamingPacketBuilder {
            send_stream_id: conn.remote_stream_id(),
            receive_stream_id: conn.local_stream_id(),
            sequence_num: 0,
            ack_through,
            nacks,
            resend_delay: 0,
            flags,
            option_bytes: Vec::new(),
            payload: Vec::new(),
        };
        let wire_bytes = encode_streaming_packet(&builder, StreamingSendLimit::default()).ok()?;
        let envelope = ClientPayload {
            protocol: i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
            source_port: conn.local_port(),
            destination_port: conn.remote_port(),
            payload: wire_bytes,
        };
        let application_bytes = encode_client_payload(&envelope)
            .map_err(StreamingManagerError::OutboundEnvelope)
            .ok()?;
        Some(TransportSendRequest {
            destination_hash: *conn.peer_destination_hash(),
            source_port: conn.local_port(),
            destination_port: conn.remote_port(),
            application_payload: application_bytes,
            sequence: 0,
            send_stream_id: conn.remote_stream_id(),
            receive_stream_id: conn.local_stream_id(),
        })
    }

    /// Emits every due standalone delayed ACK (Plan 130 §7 D3). The
    /// caller polls with the current monotonic clock exactly as it
    /// polls [`Self::poll_retransmits`]; nothing here allocates a
    /// timer, task, or socket. Before any deadline the poll returns
    /// an empty vector; after a deadline each eligible connection
    /// emits at most one coalesced plain-ACK request, so the output
    /// is bounded by the pending-ACK (connection) count. Terminated
    /// or vanished connections are pruned without emitting.
    pub fn poll_acks(&mut self, now_ms: u64) -> Vec<TransportSendRequest> {
        let mut out = Vec::new();
        while let Some((&connection_id, _)) = self
            .pending_acks
            .iter()
            .find(|(_, deadline)| **deadline <= now_ms)
        {
            self.pending_acks.remove(&connection_id);
            let emit = self.connections.get(&connection_id).is_some_and(|conn| {
                !matches!(
                    conn.state(),
                    ConnectionState::ClosingLocal
                        | ConnectionState::ClosingRemote
                        | ConnectionState::Closed
                        | ConnectionState::Reset
                )
            });
            if !emit {
                continue;
            }
            if let Some(request) = self.build_simple_ack_request(connection_id) {
                self.outbound_queue.push_back(request.clone());
                out.push(request);
            }
        }
        out
    }

    /// Returns the number of connections with a pending standalone
    /// ACK (Plan 130 §7 D3 diagnostic).
    pub fn pending_ack_count(&self) -> usize {
        self.pending_acks.len()
    }

    /// Re-emits every tracked outbound packet whose retransmission
    /// deadline has expired. Plan 129 §8 owns the integrated-path
    /// retransmission contract: the retransmitted request carries the
    /// exact original client-payload bytes (no re-encoding, no
    /// re-signing) so it traverses the gzip -> ECIES -> outbound
    /// tunnel pipeline again and the receiver's Streaming sequence
    /// dedup delivers the application bytes exactly once.
    ///
    /// The per-attempt window is the connection's current RTO;
    /// attempts beyond the configured maximum drop the tracking entry
    /// instead of retrying forever.
    pub fn poll_retransmits(&mut self, now_ms: u64) -> Vec<TransportSendRequest> {
        let mut out = Vec::new();
        let ids: Vec<ConnectionId> = self.outbound_packets.keys().copied().collect();
        for id in ids {
            let Some(rto_ms) = self
                .connections
                .get(&id)
                .map(|conn| conn.retransmit().current_rto_ms())
            else {
                continue;
            };
            let max_attempts = u32::from(self.config.max_retransmit_count);
            let Some(tracked) = self.outbound_packets.get_mut(&id) else {
                continue;
            };
            let sequences: Vec<u32> = tracked.keys().copied().collect();
            for sequence in sequences {
                let Some(packet) = tracked.get_mut(&sequence) else {
                    continue;
                };
                if now_ms.saturating_sub(packet.sent_at_ms) < rto_ms {
                    continue;
                }
                if packet.retransmit_count >= max_attempts {
                    tracked.remove(&sequence);
                    continue;
                }
                packet.retransmit_count = packet.retransmit_count.saturating_add(1);
                packet.sent_at_ms = now_ms;
                if let Some(conn) = self.connections.get_mut(&id) {
                    conn.send_window_mut().mark_retransmitted(sequence, now_ms);
                }
                self.outbound_queue.push_back(packet.request.clone());
                out.push(packet.request.clone());
            }
        }
        out
    }

    /// Drains the in-order application bytes delivered by processed
    /// inbound packets (Plan 129 §8: after a reorder the receiver
    /// observes the original byte order through this drain).
    pub fn drain_delivered(&mut self) -> Vec<DeliveredApplicationBytes> {
        self.pending_delivered.drain(..).collect()
    }

    /// Returns the number of tracked retransmission records across
    /// every connection.
    pub fn tracked_retransmit_count(&self) -> usize {
        self.outbound_packets.values().map(BTreeMap::len).sum()
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

    /// Iterates over active connections for runtime supervision and
    /// diagnostics without exposing the manager's backing table.
    pub fn iter_connections(&self) -> impl Iterator<Item = &StreamingConnection> {
        self.connections.values()
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
        // Plan 130 §7 D3: closed/reset/removed connections never leak
        // pending ACK state.
        self.pending_acks.remove(&id);
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

// Suppress an "unused import" warning for BTreeSet (kept for future use
// when we add per-connection ID sets to the pre-SYN buffer eviction policy).
#[allow(dead_code)]
type _Unused = BTreeSet<u32>;

#[cfg(test)]
mod tests {
    use super::{ConnectOutcome, RemoteDestination, StreamingManager};
    use crate::DestinationIdentity;
    use crate::streaming::StreamingConfig;
    use i2pr_proto::streaming::{
        StreamingFlags, StreamingPacketBuilder, StreamingSendLimit, decode_client_payload,
        decode_streaming_packet, encode_streaming_packet,
    };
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    const LOCAL_PORT: u16 = 10_134;
    const REMOTE_PORT: u16 = 20_134;
    const REMOTE_STREAM_ID: u32 = 0x1340_0001;
    const NOW_MS: u64 = 134_000;

    fn destination(seed: u64) -> DestinationIdentity {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        DestinationIdentity::generate(&mut rng).expect("destination identity")
    }

    fn raw_data_packet(send_stream_id: u32, receive_stream_id: u32, sequence: u32) -> Vec<u8> {
        encode_streaming_packet(
            &StreamingPacketBuilder {
                send_stream_id,
                receive_stream_id,
                sequence_num: sequence,
                ack_through: 0,
                nacks: Vec::new(),
                resend_delay: 0,
                flags: StreamingFlags::empty(),
                option_bytes: Vec::new(),
                payload: vec![sequence as u8],
            },
            StreamingSendLimit::default(),
        )
        .expect("data packet encoding")
    }

    #[test]
    fn rejected_far_ahead_packet_cannot_poison_later_production_ack() {
        let local = destination(0x1340_0001);
        let remote_identity = destination(0x1340_0002);
        let remote = RemoteDestination {
            destination_hash: *remote_identity.id().as_hash().as_bytes(),
            signing_public_key: remote_identity.destination().signing_key().clone(),
            static_public_key: remote_identity.static_public_bytes(),
        };
        let mut manager = StreamingManager::new(StreamingConfig::balanced());
        let ConnectOutcome::SynSent {
            connection_id,
            receive_stream_id,
            ..
        } = manager
            .connect(
                &local,
                &remote,
                LOCAL_PORT,
                REMOTE_PORT,
                super::DEFAULT_ADVERTISED_MAX_PAYLOAD,
                NOW_MS,
                &mut ChaCha8Rng::seed_from_u64(0x1340_0003),
            )
            .expect("connect")
        else {
            panic!("expected outbound SYN");
        };
        manager.drain_outbound();

        let connection = manager
            .get_connection_mut(connection_id)
            .expect("connection");
        connection.set_remote_stream_id(REMOTE_STREAM_ID);
        connection
            .transition_established(super::DEFAULT_ADVERTISED_MAX_PAYLOAD.into(), NOW_MS)
            .expect("established connection");

        let packet = raw_data_packet(receive_stream_id, REMOTE_STREAM_ID, 1);
        manager
            .process_inbound_packet(
                &packet,
                &[0_u8; 32],
                &local,
                REMOTE_PORT,
                LOCAL_PORT,
                NOW_MS,
            )
            .expect("accepted application packet");
        let connection = manager.get_connection(connection_id).expect("connection");
        assert_eq!(connection.recv_window().ack_view(), (1, Vec::new()));
        assert_eq!(manager.pending_ack_count(), 1);

        let far_ahead = {
            let connection = manager.get_connection(connection_id).expect("connection");
            connection
                .recv_window()
                .next_expected()
                .saturating_add(u32::from(manager.config().max_recv_window_packets))
        };
        let packet = raw_data_packet(receive_stream_id, REMOTE_STREAM_ID, far_ahead);
        let observation = manager
            .process_inbound_packet(
                &packet,
                &[0_u8; 32],
                &local,
                REMOTE_PORT,
                LOCAL_PORT,
                NOW_MS + 100,
            )
            .expect("far-ahead packet is a typed non-delivery observation");
        assert_eq!(observation.sequence, far_ahead);
        let connection = manager.get_connection(connection_id).expect("connection");
        assert_eq!(connection.recv_window().ack_view(), (1, Vec::new()));
        assert_eq!(manager.pending_ack_count(), 1);
        assert!(manager.poll_acks(NOW_MS + 749).is_empty());

        let outbound = manager
            .send_data(
                connection_id,
                &local,
                &remote,
                LOCAL_PORT,
                REMOTE_PORT,
                b"reply",
                NOW_MS + 100,
            )
            .expect("production data packet");
        let envelope = decode_client_payload(
            &outbound.application_payload,
            i2pr_proto::streaming::MAX_CLIENT_PAYLOAD_BYTES,
        )
        .expect("client payload");
        let (packet, _) = decode_streaming_packet(
            &envelope.payload,
            i2pr_proto::streaming::StreamingReceiveLimit::default(),
            i2pr_proto::streaming::StreamingOptionDecodeContext::anonymous(),
        )
        .expect("streaming packet");
        assert_eq!(packet.ack_through, 1);
        assert!(packet.nacks.is_empty());
        assert_eq!(packet.sequence_num, 1);
        let delivered = manager.drain_delivered();
        assert_eq!(delivered.len(), 1);
        assert_eq!(delivered[0].bytes, vec![1]);
    }
}
