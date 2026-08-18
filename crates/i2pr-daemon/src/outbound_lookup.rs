//! Plan 117 §8 outbound `DatabaseLookup` composition.
//!
//! The composition root takes a [`LookupAction::SendDatabaselookup`]
//! action and produces the typed transport dispatch the runtime
//! adapter must hand to the NTCP2/SSU2 transport boundary:
//!
//! ```text
//! typed DatabaseLookupMessage
//!  -> standard I2NP envelope (Plan 117 §8.1)
//!  -> TunnelPayloadHeader { ROUTER delivery }
//!  -> OutboundGatewayRole::forward_cells
//!  -> outbound OBGW TunnelData cells
//!  -> DeliveryRequest(target = outbound first hop)
//! ```
//!
//! The module owns no secret material beyond what the caller passes
//! through the `OutboundGatewayRole`. It never opens sockets, never
//! owns a clock, and never logs raw TunnelData plaintext. The
//! function fails closed when the activated outbound role is
//! unavailable so the runtime scheduler can request a build rather
//! than fall back to a direct floodfill transport send.

#![forbid(unsafe_code)]

use std::fmt;

use i2pr_netdb::LookupAction;
use i2pr_proto::{DatabaseLookupMessage, DatabaseStoreMessage, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE};
use i2pr_transport::{Deadline, DeliveryRequest, EncodedI2npMessage, PeerId};
use i2pr_tunnel::data::{DeliveryInstruction as TunnelDelivery, TunnelPayloadHeader};
use i2pr_tunnel::roles::OutboundGatewayRole;
use rand_core::{CryptoRng, RngCore};
use thiserror::Error;

/// Hard ceiling on the number of outbound lookup TunnelData cells
/// the composition root may emit. A canonical DatabaseLookup fits
/// in one cell; the ceiling leaves headroom for the multi-cell
/// path that future larger lookups would require.
pub const MAX_OUTBOUND_LOOKUP_CELLS: usize = 8;

/// Hard ceiling on the number of outbound publication TunnelData
/// cells the composition root may emit.
pub const MAX_OUTBOUND_PUBLICATION_CELLS: usize = 8;

/// Failure categories for outbound lookup composition.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum OutboundLookupError {
    /// The `LookupAction` variant was not a `SendDatabaselookup`.
    /// The composition root refuses every other variant.
    #[error("lookup action is not SendDatabaselookup")]
    UnsupportedAction,
    /// The supplied encoded I2NP envelope exceeded the transport
    /// boundary.
    #[error("I2NP envelope exceeded {maximum}-byte transport boundary (actual {actual})")]
    EnvelopeTooLarge {
        /// Actual envelope length.
        actual: usize,
        /// Transport ceiling.
        maximum: usize,
    },
    /// The supplied outbound role has expired or was not usable at
    /// the supplied timestamp.
    #[error("outbound role is not usable")]
    RoleExpired,
    /// The CSPRNG could not provide the requested bytes.
    #[error("CSPRNG is unavailable")]
    RandomnessUnavailable,
    /// The I2NP encoder rejected the composed envelope.
    #[error("I2NP envelope encoding failed: {0}")]
    Codec(String),
}



/// Result of one outbound lookup composition. The runtime scheduler
/// dispatches every [`DeliveryRequest`] to the supplied peer. The
/// `cell_count` exposes the number of TunnelData cells the
/// composition root produced; canonical lookups emit one cell.
#[derive(Debug)]
pub struct OutboundLookupDispatch {
    /// The bounded set of [`DeliveryRequest`]s the runtime adapter
    /// must hand to the outbound first hop.
    pub deliveries: Vec<DeliveryRequest>,
    /// Number of TunnelData cells the composition root produced.
    /// Always within `[1, MAX_OUTBOUND_LOOKUP_CELLS]`.
    pub cell_count: usize,
}

impl OutboundLookupDispatch {
    /// Returns the first delivery request when one exists. The
    /// canonical outbound lookup produces exactly one delivery.
    pub fn first(&self) -> Option<&DeliveryRequest> {
        self.deliveries.first()
    }
}

impl fmt::Display for OutboundLookupDispatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "OutboundLookupDispatch(cells={})", self.cell_count)
    }
}

/// Wrap a typed [`DatabaseLookupMessage`] in the standard I2NP
/// envelope the runtime transport boundary consumes. The function
/// is the single canonical envelope helper for the Plan 117
/// composition root.
pub fn encode_standard_envelope(
    lookup: &DatabaseLookupMessage,
    message_id: u32,
    expiration_ms: u64,
) -> Result<Vec<u8>, OutboundLookupError> {
    let body = I2npBody::DatabaseLookup(Box::new(lookup.clone()));
    let envelope = I2npMessage::new_standard(
        message_id,
        i2pr_proto::Date::from_millis(expiration_ms),
        body,
    )
    .map_err(|error| OutboundLookupError::Codec(error.to_string()))?;
    envelope
        .encode_standard_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .map_err(|error| OutboundLookupError::Codec(error.to_string()))
}

/// Wrap a typed [`DatabaseStoreMessage`] in the standard I2NP
/// envelope the runtime transport boundary consumes. The helper is
/// the canonical envelope writer for outbound RouterInfo
/// publication (Plan 117 §10.1).
pub fn encode_store_envelope(
    store: &DatabaseStoreMessage,
    message_id: u32,
    expiration_ms: u64,
) -> Result<Vec<u8>, OutboundLookupError> {
    let body = I2npBody::DatabaseStore(Box::new(store.clone()));
    let envelope = I2npMessage::new_standard(
        message_id,
        i2pr_proto::Date::from_millis(expiration_ms),
        body,
    )
    .map_err(|error| OutboundLookupError::Codec(error.to_string()))?;
    envelope
        .encode_standard_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .map_err(|error| OutboundLookupError::Codec(error.to_string()))
}

/// Compose one outbound `DatabaseLookup` dispatch through the
/// supplied outbound role. The helper builds the standard I2NP
/// envelope, drives `OutboundGatewayRole::forward_cells` to produce
/// the outbound TunnelData cells, and packages the result as a
/// `DeliveryRequest` addressed to the outbound first hop.
pub fn compose_outbound_lookup<R: CryptoRng + RngCore>(
    action: &LookupAction,
    role: &OutboundGatewayRole,
    message_id: u32,
    expiration_ms: u64,
    deadline: Deadline,
    rng: &mut R,
    now_ms: u64,
) -> Result<OutboundLookupDispatch, OutboundLookupError> {
    let lookup = match action {
        LookupAction::SendDatabaselookup { message, .. } => message,
        _ => return Err(OutboundLookupError::UnsupportedAction),
    };
    let envelope = encode_standard_envelope(lookup, message_id, expiration_ms)?;
    let payload_header = TunnelPayloadHeader {
        delivery: TunnelDelivery::Router {
            router: lookup.key,
        },
        message_id,
        expiration_ms,
    };
    let target_floodfill = lookup.key;
    dispatch_cells(
        target_floodfill,
        payload_header,
        envelope,
        role,
        deadline,
        rng,
        now_ms,
        MAX_OUTBOUND_LOOKUP_CELLS,
    )
}

fn encode_tunnel_data_cell(cell: &i2pr_proto::TunnelDataMessage) -> Vec<u8> {
    // TunnelData cells carry the 4-byte tunnel id followed by the
    // 1024-byte (iv, payload) pair. The local endpoint expects
    // the raw canonical wire representation so the OBEP can
    // recover the original cell without an extra decoder hop.
    let mut bytes = Vec::with_capacity(4 + cell.data.len());
    bytes.extend_from_slice(&cell.tunnel_id.to_be_bytes());
    bytes.extend_from_slice(&cell.data);
    bytes
}

fn dispatch_cells<R: CryptoRng + RngCore>(
    target_router: i2pr_proto::Hash,
    payload_header: TunnelPayloadHeader,
    envelope: Vec<u8>,
    role: &OutboundGatewayRole,
    deadline: Deadline,
    rng: &mut R,
    now_ms: u64,
    max_cells: usize,
) -> Result<OutboundLookupDispatch, OutboundLookupError> {
    if envelope.len() > MAX_I2NP_PAYLOAD_SIZE {
        return Err(OutboundLookupError::EnvelopeTooLarge {
            actual: envelope.len(),
            maximum: MAX_I2NP_PAYLOAD_SIZE,
        });
    }
    let cells = role
        .forward_cells(&payload_header, &envelope, rng, now_ms)
        .map_err(|error| match error {
            i2pr_tunnel::roles::TunnelRoleError::TunnelUnavailable => {
                OutboundLookupError::RoleExpired
            }
            i2pr_tunnel::roles::TunnelRoleError::TunnelMessage(message) => match message {
                i2pr_tunnel::data::TunnelMessageError::RandomnessUnavailable => {
                    OutboundLookupError::RandomnessUnavailable
                }
                other => OutboundLookupError::Codec(other.to_string()),
            },
            other => OutboundLookupError::Codec(other.to_string()),
        })?;
    if cells.is_empty() || cells.len() > max_cells {
        return Err(OutboundLookupError::Codec(format!(
            "outbound role produced {} cells, expected 1..={}",
            cells.len(),
            max_cells
        )));
    }
    let mut deliveries = Vec::with_capacity(cells.len());
    for cell in cells {
        let bytes = encode_tunnel_data_cell(&cell.cell);
        let encoded = EncodedI2npMessage::new(bytes)
            .map_err(|error| OutboundLookupError::Codec(error.to_string()))?;
        let delivery_id = i2pr_transport::DeliveryId::generate()
            .map_err(|error| OutboundLookupError::Codec(format!("{error:?}")))?;
        let delivery = DeliveryRequest::with_id(
            delivery_id,
            PeerId::from_hash(cell.target_router.hash()),
            encoded,
            deadline,
        );
        let _ = target_router;
        deliveries.push(delivery);
    }
    let cell_count = deliveries.len();
    Ok(OutboundLookupDispatch {
        deliveries,
        cell_count,
    })
}

/// Compose one outbound RouterInfo publication through the supplied
/// outbound role. Plan 117 §10.1: the helper consumes a typed
/// [`DatabaseStoreMessage`] retained by the publication coordinator
/// and produces the transport delivery for the runtime adapter.
pub fn compose_outbound_publication<R: CryptoRng + RngCore>(
    store_message: &DatabaseStoreMessage,
    target_floodfill: i2pr_proto::Hash,
    role: &OutboundGatewayRole,
    message_id: u32,
    expiration_ms: u64,
    deadline: Deadline,
    rng: &mut R,
    now_ms: u64,
) -> Result<OutboundLookupDispatch, OutboundLookupError> {
    let envelope = encode_store_envelope(store_message, message_id, expiration_ms)?;
    let payload_header = TunnelPayloadHeader {
        delivery: TunnelDelivery::Router {
            router: target_floodfill,
        },
        message_id,
        expiration_ms,
    };
    dispatch_cells(
        target_floodfill,
        payload_header,
        envelope,
        role,
        deadline,
        rng,
        now_ms,
        MAX_OUTBOUND_PUBLICATION_CELLS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_netdb::RouterHash;

    #[test]
    fn envelope_helper_round_trips_database_lookup() {
        let target = RouterHash::from_bytes([0x42u8; 32]);
        let gateway = RouterHash::from_bytes([0x33u8; 32]);
        let message = i2pr_netdb::build_databaselookup(
            &target,
            i2pr_netdb::LookupKind::RouterInfo,
            Some(&i2pr_netdb::ReplyPath::new(gateway, 7).expect("path")),
            &[target],
        )
        .expect("lookup");
        let bytes = encode_standard_envelope(&message, 0x1234_5678, 60_000).expect("envelope");
        let decoded = i2pr_proto::I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode");
        let I2npBody::DatabaseLookup(recovered) = decoded.body() else {
            panic!("expected DatabaseLookup body");
        };
        assert_eq!(recovered.key, message.key);
        assert_eq!(recovered.from, message.from);
        assert_eq!(recovered.reply_tunnel_id, message.reply_tunnel_id);
        assert_eq!(recovered.lookup_type, 2);
    }
}