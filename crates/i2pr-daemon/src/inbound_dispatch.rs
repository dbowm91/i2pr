//! Plan 117 §9 inbound exploratory `TunnelData` dispatch.
//!
//! The runtime adapter reads one raw `TunnelData` cell off an
//! authenticated inbound transport link and hands it to
//! [`dispatch_inbound_tunnel_data`]. The helper:
//!
//! 1. routes the cell by `tunnel_id` to the activated
//!    [`i2pr_tunnel::LocalInboundEndpointRole`] in the supplied
//!    [`i2pr_tunnel::DataPlaneRegistry`];
//! 2. runs the cell through `LocalInboundEndpointRole::process`,
//!    recovering the standard I2NP envelope the inbound endpoint
//!    has just delivered locally;
//! 3. decodes the recovered envelope **once** through the existing
//!    standard `i2pr-proto` decoder;
//! 4. normalizes the typed NetDB response kind.
//!
//! The helper refuses every cell whose `tunnel_id` does not match
//! an activated inbound endpoint role. Unknown tunnel identifiers
//! never allocate state in the registry and never reach a
//! reassembler. The helper does **not** persist cells, does not
//! poll the registry for a missing tunnel id, and does not fall
//! back to a direct dispatch path.

#![forbid(unsafe_code)]

use i2pr_netdb::{
    handle_databasestore_message, handle_searchreply_message, RouterInfoLookup,
    RouterInfoStore,
};
use i2pr_proto::{I2npBody, I2npMessage, MessageType, TunnelDataMessage, MAX_I2NP_PAYLOAD_SIZE};
use i2pr_tunnel::data_plane_registry::DataPlaneRegistry;
use i2pr_tunnel::identity::TunnelId;
use i2pr_tunnel::roles::TunnelRoleError;
use thiserror::Error;

/// Maximum recovered envelope size after decoding one
/// `TunnelData` cell. The local endpoint may reassemble a message
/// that exceeds a single cell; the canonical I2NP payload ceiling
/// matches the transport boundary.
pub const MAX_RECOVERED_ENVELOPE: usize = MAX_I2NP_PAYLOAD_SIZE;

/// Failure categories for inbound exploratory `TunnelData`
/// dispatch.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum InboundDispatchError {
    /// The cell's `tunnel_id` did not match an activated inbound
    /// endpoint role in the registry. The cell is rejected without
    /// allocating any state.
    #[error("inbound tunnel id {0} does not match an activated local endpoint")]
    UnknownTunnelId(u32),
    /// The supplied cell has an unparseable `tunnel_id`.
    #[error("inbound tunnel id is out of range")]
    InvalidTunnelId,
    /// The local endpoint reported a role-level failure.
    #[error("inbound endpoint role failed: {0}")]
    EndpointRole(String),
    /// The standard I2NP envelope decoder rejected the recovered
    /// payload.
    #[error("recovered I2NP envelope decoding failed: {0}")]
    Codec(String),
    /// The recovered envelope was not a NetDB response.
    #[error("recovered I2NP envelope is not a NetDB response (type 18={type_byte})")]
    UnsupportedBodyType {
        /// I2NP type byte of the rejected envelope.
        type_byte: u8,
    },
    /// The recovered envelope carried expiration time in the past
    /// or non-positive ttl.
    #[error("recovered I2NP envelope expired at expiration_ms")]
    Expired,
    /// The supplied registry is required and missing or empty.
    #[error("registry is empty for tunnel id {0}")]
    NoActiveInbound(u32),
}

/// Convert a typed [`TunnelRoleError`] into an
/// [`InboundDispatchError`].
fn endpoint_error(error: TunnelRoleError) -> InboundDispatchError {
    match error {
        TunnelRoleError::TunnelUnavailable => {
            InboundDispatchError::EndpointRole("tunnel unavailable".to_owned())
        }
        TunnelRoleError::ZeroTunnelId => {
            InboundDispatchError::EndpointRole("zero tunnel id".to_owned())
        }
        TunnelRoleError::ReceiveTunnelMismatch { actual, .. } => {
            InboundDispatchError::EndpointRole(format!(
                "receive tunnel mismatch (actual={})",
                actual.get()
            ))
        }
        TunnelRoleError::LocalInboundNonLocalDelivery => {
            InboundDispatchError::EndpointRole("non-local delivery instruction".to_owned())
        }
        TunnelRoleError::UnspecifiedDeliveryInstruction { message_id } => {
            InboundDispatchError::EndpointRole(format!(
                "unspecified delivery instruction for message {message_id}"
            ))
        }
        TunnelRoleError::Reassembly(error) => {
            InboundDispatchError::EndpointRole(format!("reassembler {error:?}"))
        }
        TunnelRoleError::DuplicateWindow(error) => {
            InboundDispatchError::EndpointRole(format!("duplicate window {error:?}"))
        }
        TunnelRoleError::TunnelMessage(message) => {
            InboundDispatchError::EndpointRole(format!("{message:?}"))
        }
        TunnelRoleError::PreviousPeerMismatch => {
            InboundDispatchError::EndpointRole("previous peer mismatch".to_owned())
        }
        TunnelRoleError::NotTunnelGateway => {
            InboundDispatchError::EndpointRole("not TunnelGateway".to_owned())
        }
        TunnelRoleError::GatewayTunnelMismatch { actual, expected } => {
            InboundDispatchError::EndpointRole(format!(
                "TunnelGateway mismatch actual={} expected={}",
                actual.get(),
                expected.get()
            ))
        }
        TunnelRoleError::UnsupportedDeliveryInstruction => {
            InboundDispatchError::EndpointRole("unsupported delivery instruction".to_owned())
        }
        TunnelRoleError::MissingNextHop => {
            InboundDispatchError::EndpointRole("missing next hop".to_owned())
        }
    }
}

/// Normalized kind for one recovered NetDB response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InboundResponseKind {
    /// DatabaseStore (I2NP wire code 0x01).
    DatabaseStore,
    /// DatabaseSearchReply (I2NP wire code 0x03).
    DatabaseSearchReply,
    /// DeliveryStatus (I2NP wire code 0x0A).
    DeliveryStatus,
}

impl InboundResponseKind {
    /// Returns the I2NP type byte that identifies this body kind.
    pub const fn wire_code(self) -> u8 {
        match self {
            Self::DatabaseStore => MessageType::DatabaseStore.code(),
            Self::DatabaseSearchReply => MessageType::DatabaseSearchReply.code(),
            Self::DeliveryStatus => MessageType::DeliveryStatus.code(),
        }
    }
}

/// Categorical outcome of an inbound exploratory dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InboundDispatchOutcome {
    /// The cell did not complete a message in the local
    /// reassembler. The runtime should not attempt to drive the
    /// lookup state machine from this outcome.
    CellAccepted,
    /// The cell completed a `DatabaseStore` message. The caller
    /// may hand the bytes to [`route_databasestore`] for state
    /// machine ingestion.
    DatabaseStoreComplete {
        /// Reassembled I2NP envelope.
        bytes: Vec<u8>,
    },
    /// The cell completed a `DatabaseSearchReply` message.
    DatabaseSearchReplyComplete {
        /// Reassembled I2NP envelope.
        bytes: Vec<u8>,
    },
    /// The cell completed a `DeliveryStatus` message.
    DeliveryStatusComplete {
        /// Reassembled I2NP envelope.
        bytes: Vec<u8>,
    },
}

fn body_kind(body: &I2npBody) -> InboundResponseKind {
    match body {
        I2npBody::DatabaseStore(_) => InboundResponseKind::DatabaseStore,
        I2npBody::DatabaseSearchReply(_) => InboundResponseKind::DatabaseSearchReply,
        I2npBody::DeliveryStatus(_) => InboundResponseKind::DeliveryStatus,
        _ => InboundResponseKind::DeliveryStatus,
    }
}

/// Routes one inbound `TunnelDataMessage` through the supplied
/// registry. The returned outcome tells the caller whether the
/// dispatch completed a reassembled message; non-complete
/// outcomes are not errors.
pub fn dispatch_inbound_tunnel_data(
    registry: &mut DataPlaneRegistry,
    cell: &TunnelDataMessage,
    now_ms: u64,
) -> Result<InboundDispatchOutcome, InboundDispatchError> {
    let tunnel_id = TunnelId::new(cell.tunnel_id).map_err(|_| InboundDispatchError::InvalidTunnelId)?;
    let previous_peer = registry
        .inbound_first_hop(tunnel_id)
        .ok_or(InboundDispatchError::UnknownTunnelId(cell.tunnel_id))?;
    let outcome = match registry.inbound_mut(tunnel_id) {
        Some(role) => role.process(&previous_peer, cell, now_ms),
        None => return Err(InboundDispatchError::NoActiveInbound(cell.tunnel_id)),
    };
    let bytes = match outcome {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return Ok(InboundDispatchOutcome::CellAccepted),
        Err(error) => return Err(endpoint_error(error)),
    };
    if bytes.len() > MAX_RECOVERED_ENVELOPE {
        return Err(InboundDispatchError::Codec(format!(
            "recovered envelope {} bytes exceeded {MAX_RECOVERED_ENVELOPE}",
            bytes.len()
        )));
    }
    let envelope = I2npMessage::decode_standard(&bytes, MAX_RECOVERED_ENVELOPE)
        .map_err(|error| InboundDispatchError::Codec(format!("{error:?}")))?;
    match envelope.body() {
        I2npBody::DatabaseStore(_) => Ok(InboundDispatchOutcome::DatabaseStoreComplete {
            bytes: bytes.to_vec(),
        }),
        I2npBody::DatabaseSearchReply(_) => Ok(
            InboundDispatchOutcome::DatabaseSearchReplyComplete {
                bytes: bytes.to_vec(),
            },
        ),
        I2npBody::DeliveryStatus(_) => Ok(
            InboundDispatchOutcome::DeliveryStatusComplete {
                bytes: bytes.to_vec(),
            },
        ),
        other => {
            let _ = body_kind(other);
            let type_byte = other.message_type().code();
            Err(InboundDispatchError::UnsupportedBodyType { type_byte })
        }
    }
}

/// Routes one inbound `DatabaseStore` envelope into the lookup
/// state machine. Decodes the supplied envelope exactly once.
pub fn route_databasestore(
    lookup: &mut RouterInfoLookup,
    store: &mut RouterInfoStore,
    lookup_id: i2pr_netdb::LookupId,
    envelope: &I2npMessage,
    context: i2pr_netdb::ValidationContext,
) -> Result<
    i2pr_netdb::ResponseOutcome,
    i2pr_netdb::LookupEngineError,
> {
    handle_databasestore_message(lookup, store, lookup_id, envelope, context)
}

/// Routes one inbound `DatabaseSearchReply` envelope into the
/// lookup state machine. Decodes the supplied envelope exactly
/// once.
pub fn route_database_search_reply(
    lookup: &mut RouterInfoLookup,
    lookup_id: i2pr_netdb::LookupId,
    envelope: &I2npMessage,
    policy: &i2pr_netdb::LookupPolicy,
) -> Result<
    i2pr_netdb::ResponseOutcome,
    i2pr_netdb::LookupEngineError,
> {
    handle_searchreply_message(lookup, lookup_id, envelope, policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_kind_wire_codes_match_i2np_body_kind() {
        assert_eq!(InboundResponseKind::DatabaseStore.wire_code(), 0x01);
        assert_eq!(InboundResponseKind::DatabaseSearchReply.wire_code(), 0x03);
        assert_eq!(InboundResponseKind::DeliveryStatus.wire_code(), 0x0A);
    }
}
