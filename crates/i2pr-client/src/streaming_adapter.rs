//! Plan 125 §9 / Plan 129 §1-§3: Streaming-to-destination-routing
//! adapter boundary.
//!
//! The adapter is the single runtime-neutral composition surface
//! between the Streaming layer and the corrected destination-routing
//! pipeline. It owns no sockets, DNS, Tokio tasks, or router
//! transport.
//!
//! Outbound (Plan 129 §2):
//!
//! ```text
//! TransportSendRequest (the gzip-encoded complete Streaming packet)
//!  -> ceiling check against MAX_CLIENT_PAYLOAD_BYTES
//!  -> Plan 122 compose_outbound_delivery
//!     -> canonical I2NP Data envelope (single construction owner:
//!        OutboundRequest::new inside the routing composer)
//!        -> ECIES Garlic envelope (bound NS / NSR / ES)
//!           -> standard-encoded I2NP `Garlic` carrier
//!              -> outbound tunnel data plane (Plan 116 OBEP)
//! ```
//!
//! Inbound (Plan 129 §3):
//!
//! ```text
//! recovered inner I2NP message bytes (from DestinationDispatcher)
//!  -> decode standard I2NP message; require I2npBody::Data
//!     -> decode canonical I2P gzip client payload
//!        -> require protocol == 6 for the Streaming path
//!           -> read source_port / destination_port (I2P ports;
//!              no local TCP privileged-port policy applies)
//!           -> pass only the decoded Streaming packet bytes to
//!              StreamingManager::process_inbound_packet
//! ```
//!
//! Non-protocol-6 client payloads never reach Streaming; they surface
//! as [`InboundStreamingOutcome::UnsupportedProtocol`] for future
//! datagram/I2CP layers.

#![forbid(unsafe_code)]

use i2pr_netdb::DestinationHash;
use i2pr_proto::{
    ClientPayloadDecodeError, CodecError, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE,
};
use i2pr_tunnel::TunnelRoleError;
use rand_core::{CryptoRng, RngCore};

use crate::identity::{DestinationId, DestinationIdentity};
use crate::routing::{
    DestinationOutboundRole, DestinationRouting, OutboundDeliveryPlan, SendError,
};
use crate::streaming::events::WirePacketObservation;
use crate::streaming::manager::{StreamingManager, StreamingManagerError};
use crate::streaming::transport::TransportSendRequest;

/// Hard ceiling on the encoded client payload the outbound adapter
/// accepts. `TransportSendRequest.application_payload` carries the
/// **gzip-encoded complete Streaming packet**, not the application
/// payload inside one Streaming packet (Plan 128 separates the two
/// concepts), so the source floor is the client-payload/I2NP limit —
/// never the negotiated Streaming application payload MTU.
pub const MAX_STREAMING_ADAPTER_PAYLOAD_BYTES: usize =
    i2pr_proto::streaming::MAX_CLIENT_PAYLOAD_BYTES;

/// Typed outcome of an adapter send attempt.
#[derive(Debug)]
pub enum StreamingAdapterError {
    /// The supplied payload exceeds the local ceiling.
    PayloadTooLarge { actual: usize, maximum: usize },
    /// The streaming payload is empty.
    EmptyPayload,
    /// The I2NP codec rejected an envelope.
    DataCodec(CodecError),
    /// The Plan 122 send composer failed.
    Send(SendError),
    /// The tunnel data plane reported an error.
    Tunnel(TunnelRoleError),
    /// The streaming destination hash is missing or invalid.
    UnknownDestination(DestinationHash),
    /// The inbound I2NP body was not `Data`.
    NotI2npData,
    /// The inbound client payload failed to decode.
    ClientPayload(ClientPayloadDecodeError),
    /// The owning streaming manager rejected the decoded packet.
    Streaming(StreamingManagerError),
}

impl core::fmt::Display for StreamingAdapterError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::PayloadTooLarge { actual, maximum } => write!(
                formatter,
                "streaming adapter payload {actual} exceeds {maximum}-byte ceiling"
            ),
            Self::EmptyPayload => formatter.write_str("streaming adapter payload is empty"),
            Self::DataCodec(error) => write!(formatter, "streaming adapter codec: {error}"),
            Self::Send(error) => write!(formatter, "streaming adapter send: {error}"),
            Self::Tunnel(error) => write!(formatter, "streaming adapter tunnel: {error}"),
            Self::UnknownDestination(hash) => write!(
                formatter,
                "streaming adapter unknown destination hash {hash:?}"
            ),
            Self::NotI2npData => {
                formatter.write_str("inbound inner I2NP message is not a Data body")
            }
            Self::ClientPayload(error) => {
                write!(formatter, "streaming adapter client payload: {error}")
            }
            Self::Streaming(error) => write!(formatter, "streaming manager: {error}"),
        }
    }
}

impl std::error::Error for StreamingAdapterError {}

impl From<SendError> for StreamingAdapterError {
    fn from(error: SendError) -> Self {
        Self::Send(error)
    }
}

impl From<TunnelRoleError> for StreamingAdapterError {
    fn from(error: TunnelRoleError) -> Self {
        Self::Tunnel(error)
    }
}

/// Outcome of the inbound protocol-6 dispatch (Plan 129 §3).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InboundStreamingOutcome {
    /// A protocol-6 client payload was decoded and its Streaming
    /// packet bytes dispatched to the owning local destination's
    /// [`StreamingManager`].
    StreamingDispatched {
        /// Sender's I2P destination port carried by the client payload.
        source_port: u16,
        /// Receiver's I2P destination port carried by the client payload.
        destination_port: u16,
        /// Observation returned by the streaming manager.
        observation: WirePacketObservation,
    },
    /// The client payload carried a non-streaming protocol number.
    /// The payload never reaches Streaming; future datagram/I2CP
    /// layers dispatch from this typed outcome.
    UnsupportedProtocol {
        /// Observed I2P protocol number.
        protocol: u8,
    },
}

/// Streaming-destination adapter.
///
/// The adapter is a thin stateless function object bridging the
/// streaming layer and the destination routing pipeline. It owns no
/// per-connection state and is therefore cheap to copy. Selecting the
/// owning local destination's [`StreamingManager`] for an inbound
/// delivery belongs to the destination registry/runtime, which passes
/// the manager reference into [`Self::receive`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamingDestinationAdapter;

impl StreamingDestinationAdapter {
    /// Builds a new adapter. No state is captured; this exists for
    /// API symmetry.
    pub const fn new() -> Self {
        Self
    }

    /// Routes a streaming `TransportSendRequest` through the Plan 122
    /// destination-routing pipeline.
    ///
    /// `request.application_payload` must already be the gzip-encoded
    /// complete Streaming packet produced by the streaming manager.
    /// The adapter bounds those bytes against
    /// [`MAX_STREAMING_ADAPTER_PAYLOAD_BYTES`] (the client-payload
    /// limit), wraps them through the single canonical Data-envelope
    /// construction owner ([`crate::routing::OutboundRequest::new`]
    /// inside the routing composer — no redundant local I2NP
    /// envelope is built or discarded here), bundles the local
    /// destination's current signed Standard LeaseSet2 (Plan 127 §2:
    /// a fresh bound New Session must carry it so the receiver can
    /// bind and route back), and returns the resulting
    /// [`OutboundDeliveryPlan`] whose cells the runtime dispatches.
    #[allow(clippy::too_many_arguments)]
    pub fn send<R: CryptoRng + RngCore>(
        request: &TransportSendRequest,
        routing: &DestinationRouting,
        session: &mut crate::session::EciesSessionManager,
        outbound: &DestinationOutboundRole,
        local_id: DestinationId,
        local_static_secret: &[u8; i2pr_crypto::X25519_KEY_LENGTH],
        local_lease_set2: &i2pr_proto::LeaseSet2,
        now_seconds: u32,
        now_ms: u64,
        rng: &mut R,
    ) -> Result<OutboundDeliveryPlan, StreamingAdapterError> {
        if request.application_payload.is_empty() {
            return Err(StreamingAdapterError::EmptyPayload);
        }
        if request.application_payload.len() > MAX_STREAMING_ADAPTER_PAYLOAD_BYTES {
            return Err(StreamingAdapterError::PayloadTooLarge {
                actual: request.application_payload.len(),
                maximum: MAX_STREAMING_ADAPTER_PAYLOAD_BYTES,
            });
        }
        let remote_hash =
            DestinationHash::from_hash(i2pr_proto::Hash::from_bytes(request.destination_hash));
        let outbound_request = crate::routing::OutboundRequest::new(
            i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
            &request.application_payload,
            now_ms,
            Some(local_lease_set2.clone()),
        )
        .map_err(StreamingAdapterError::Send)?;
        let plan = crate::routing::compose_outbound_delivery(
            routing,
            session,
            outbound,
            local_id,
            local_static_secret,
            remote_hash,
            &outbound_request,
            now_seconds,
            now_ms,
            rng,
        )
        .map_err(StreamingAdapterError::Send)?;
        Ok(plan)
    }

    /// Inbound inverse (Plan 129 §3): decodes the recovered inner
    /// I2NP message, requires an `I2npBody::Data` body, decodes the
    /// canonical gzip client payload, requires protocol 6 for the
    /// Streaming path, and passes only the decoded Streaming packet
    /// bytes to the owning local destination's [`StreamingManager`].
    ///
    /// Destination ports are I2P ports; no local TCP privileged-port
    /// policy applies. `listener_port` names the local listening port
    /// whose backlog should receive a pending inbound SYN, when the
    /// delivery targets a listener.
    pub fn receive(
        recovered_i2np_bytes: &[u8],
        owning_destination: &DestinationIdentity,
        listener_port: Option<u16>,
        streaming: &mut StreamingManager,
        from_destination_hash: &[u8; 32],
        now_ms: u64,
    ) -> Result<InboundStreamingOutcome, StreamingAdapterError> {
        let message = I2npMessage::decode_standard(recovered_i2np_bytes, MAX_I2NP_PAYLOAD_SIZE)
            .map_err(StreamingAdapterError::DataCodec)?;
        let data_payload = match message.body() {
            I2npBody::Data(body) => body.payload.as_bytes().to_vec(),
            _ => return Err(StreamingAdapterError::NotI2npData),
        };
        let envelope = i2pr_proto::streaming::decode_client_payload(
            &data_payload,
            MAX_STREAMING_ADAPTER_PAYLOAD_BYTES,
        )
        .map_err(StreamingAdapterError::ClientPayload)?;
        if envelope.protocol != i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER {
            return Ok(InboundStreamingOutcome::UnsupportedProtocol {
                protocol: envelope.protocol,
            });
        }
        let observation = streaming
            .process_inbound_packet(
                &envelope.payload,
                from_destination_hash,
                owning_destination,
                listener_port,
                now_ms,
            )
            .map_err(StreamingAdapterError::Streaming)?;
        Ok(InboundStreamingOutcome::StreamingDispatched {
            source_port: envelope.source_port,
            destination_port: envelope.destination_port,
            observation,
        })
    }
}

impl Default for StreamingDestinationAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_ceiling_is_the_client_payload_limit_not_the_streaming_mtu() {
        // Plan 129 §2: the source floor equals MAX_CLIENT_PAYLOAD_BYTES
        // because application_payload is the gzip-encoded complete
        // Streaming packet, not the in-packet application payload.
        const {
            assert!(
                MAX_STREAMING_ADAPTER_PAYLOAD_BYTES
                    == i2pr_proto::streaming::MAX_CLIENT_PAYLOAD_BYTES
            )
        };
        const {
            assert!(
                MAX_STREAMING_ADAPTER_PAYLOAD_BYTES
                    > i2pr_proto::streaming::MAX_STREAMING_PACKET_BYTES
            )
        };
    }

    #[test]
    fn adapter_default_matches_new() {
        let new_adapter = StreamingDestinationAdapter::new();
        let default_adapter = StreamingDestinationAdapter;
        assert_eq!(new_adapter, default_adapter);
    }
}
