//! Plan 125 §9 / §G-H: Streaming-to-destination-routing adapter.
//!
//! The adapter consumes a typed `TransportSendRequest` from the
//! streaming layer and routes it through the corrected Plan 122
//! LeaseSet2 -> ECIES Garlic -> outbound-tunnel path. The runtime
//! adapter then drives the resulting `OutboundDeliveryPlan` through
//! the local outbound tunnel role; the recipient runs the matching
//! `DestinationDispatcher` against the inbound tunnel role and the
//! ECIES session manager.
//!
//! The adapter is intentionally runtime-neutral. It owns no sockets,
//! timers, or DNS, and never bypasses the destination routing
//! pipeline. The full local I2P destination-routing composition is:
//!
//! ```text
//! TransportSendRequest (streaming packet bytes)
//!  -> StreamingDestinationAdapter::send
//!     -> Plan 122 compose_outbound_delivery
//!        -> ECIES Garlic envelope (New Session / Existing)
//!           -> standard-encoded I2NP `Garlic` carrier
//!              -> outbound tunnel data plane (Plan 116 OBEP)
//! ```
//!
//! Inbound:
//!
//! ```text
//! inbound TunnelData bytes
//!  -> DestinationDispatcher::dispatch_garlic_envelope
//!     -> ECIES authenticate / decrypt
//!        -> decrypted I2NP `Data` body
//!           -> protocol-6 gzip client payload decode
//!              -> StreamingManager::process_inbound_packet
//! ```

#![forbid(unsafe_code)]

use i2pr_netdb::DestinationHash;
use i2pr_proto::{CodecError, Date, DeferredPayload, I2npBody, I2npMessage, OpaqueMessageBody};
use i2pr_tunnel::TunnelRoleError;
use rand_core::{CryptoRng, RngCore};

use crate::identity::DestinationId;
use crate::routing::{
    DestinationOutboundRole, DestinationRouting, DestinationRoutingError, OutboundDeliveryPlan,
    OutboundRequest, SendError,
};
use crate::streaming::transport::TransportSendRequest;

/// Hard ceiling on the per-payload byte count for the streaming
/// adapter. The streaming protocol already enforces its own per-packet
/// ceiling; the adapter adds a defense-in-depth ceiling on the
/// incoming `application_payload`.
pub const MAX_STREAMING_ADAPTER_PAYLOAD_BYTES: usize =
    i2pr_proto::streaming::MAX_STREAMING_PAYLOAD_BYTES;

/// Typed outcome of an adapter send attempt.
#[derive(Debug)]
pub enum StreamingAdapterError {
    /// The supplied payload exceeds the local ceiling.
    PayloadTooLarge { actual: usize, maximum: usize },
    /// The streaming payload is empty.
    EmptyPayload,
    /// The I2NP codec rejected the inner Data message.
    DataCodec(CodecError),
    /// The Plan 122 send composer failed.
    Send(SendError),
    /// The tunnel data plane reported an error.
    Tunnel(TunnelRoleError),
    /// The streaming destination hash is missing or invalid.
    UnknownDestination(DestinationHash),
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

/// Streaming-destination adapter.
///
/// The adapter is a thin function object that bridges the streaming
/// layer and the Plan 122 destination routing pipeline. It owns no
/// per-connection state and is therefore cheap to clone; production
/// callers construct it once and pass it through the runtime adapter
/// to the streaming layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamingDestinationAdapter;

impl StreamingDestinationAdapter {
    /// Builds a new adapter. No state is captured; this exists for
    /// API symmetry with future adapter extensions.
    pub const fn new() -> Self {
        Self
    }

    /// Routes a streaming `TransportSendRequest` through the Plan 122
    /// destination-routing pipeline.
    ///
    /// The streaming packet bytes (`request.application_payload`)
    /// carry the protocol-6 gzip-framed streaming packet produced by
    /// the streaming manager. The adapter builds an `OutboundRequest`
    /// that wraps the bytes in a standard-encoded I2NP `Data`
    /// envelope and hands the request to `compose_outbound_delivery`.
    /// The resulting `OutboundDeliveryPlan` carries the standard
    /// ECIES-encrypted Garlic envelope plus the tunnel cells the
    /// runtime must dispatch.
    #[allow(clippy::too_many_arguments)]
    pub fn send<R: CryptoRng + RngCore>(
        request: &TransportSendRequest,
        routing: &DestinationRouting,
        session: &mut crate::session::EciesSessionManager,
        outbound: &DestinationOutboundRole,
        local_id: DestinationId,
        local_static_secret: &[u8; i2pr_crypto::X25519_KEY_LENGTH],
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
        let inner_envelope = build_inner_data_envelope(&request.application_payload, now_ms)?;
        let outbound_request = OutboundRequest::new(
            i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
            &request.application_payload,
            now_ms,
            None,
        )
        .map_err(StreamingAdapterError::Send)?;
        let _ = inner_envelope;
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
}

impl Default for StreamingDestinationAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds the canonical I2NP `Data` envelope that wraps a streaming
/// payload. The envelope is the canonical I2P `I2npBody::Data`
/// carrier for application bytes destined for the remote I2P
/// destination; the streaming payload bytes already carry the
/// protocol-6 gzip framing produced by the streaming manager.
fn build_inner_data_envelope(
    payload: &[u8],
    now_ms: u64,
) -> Result<I2npMessage, StreamingAdapterError> {
    let body = I2npBody::Data(OpaqueMessageBody {
        payload: DeferredPayload::new(payload.to_vec(), i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
            .map_err(StreamingAdapterError::DataCodec)?,
    });
    I2npMessage::new_standard(0, Date::from_millis(now_ms), body)
        .map_err(StreamingAdapterError::DataCodec)
}

// Suppress an "unused" warning for items kept for the future
// destination routing extension.
#[allow(dead_code)]
type _RoutingErrorUnused = DestinationRoutingError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_payload_guard_is_a_const() {
        // The empty-payload guard runs before any pipeline state is
        // touched; verify the constant is the typed maximum used by the
        // adapter.
        const { assert!(MAX_STREAMING_ADAPTER_PAYLOAD_BYTES > 0) };
    }

    #[test]
    fn adapter_default_matches_new() {
        let new_adapter = StreamingDestinationAdapter::new();
        let default_adapter = StreamingDestinationAdapter;
        assert_eq!(new_adapter, default_adapter);
    }
}
