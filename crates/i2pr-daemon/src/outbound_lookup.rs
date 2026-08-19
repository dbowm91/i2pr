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
use i2pr_proto::{
    DatabaseLookupMessage, DatabaseStoreMessage, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE,
};
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
        write!(
            formatter,
            "OutboundLookupDispatch(cells={})",
            self.cell_count
        )
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
///
/// The tunnel ROUTER delivery target is the selected floodfill peer F
/// (not the lookup key K). The `DeliveryRequest` target is the
/// outbound first-hop router P from the tunnel role.
pub fn compose_outbound_lookup<R: CryptoRng + RngCore>(
    action: &LookupAction,
    role: &OutboundGatewayRole,
    message_id: u32,
    expiration_ms: u64,
    deadline: Deadline,
    rng: &mut R,
    now_ms: u64,
) -> Result<OutboundLookupDispatch, OutboundLookupError> {
    let (peer_hash, lookup) = match action {
        LookupAction::SendDatabaselookup { peer, message, .. } => {
            (i2pr_proto::Hash::from_bytes(*peer.as_bytes()), message)
        }
        _ => return Err(OutboundLookupError::UnsupportedAction),
    };
    let selected_floodfill = peer_hash;
    let envelope = encode_standard_envelope(lookup, message_id, expiration_ms)?;
    let payload_header = TunnelPayloadHeader {
        delivery: TunnelDelivery::Router {
            router: selected_floodfill,
        },
        message_id,
        expiration_ms,
    };
    dispatch_cells(
        payload_header,
        envelope,
        role,
        deadline,
        rng,
        now_ms,
        MAX_OUTBOUND_LOOKUP_CELLS,
    )
}

/// Encode a `TunnelDataMessage` as a complete short-transport I2NP
/// message suitable for the NTCP2/SSU2 authenticated link boundary.
///
/// The function generates a fresh outer message ID from the injected
/// CSPRNG, converts the expiration to checked short-header seconds,
/// constructs `I2npMessage::new_short_transport(...)`, and returns the
/// complete encoded bytes wrapped in `EncodedI2npMessage`.
fn encode_transport_tunnel_data<R: CryptoRng + RngCore>(
    cell: i2pr_proto::TunnelDataMessage,
    expiration_ms: u64,
    rng: &mut R,
) -> Result<EncodedI2npMessage, OutboundLookupError> {
    let outer_message_id = rng.next_u32();
    let expiration_seconds: u32 = u32::try_from(expiration_ms / 1000).map_err(|_| {
        OutboundLookupError::Codec("expiration seconds exceeds u32 range".to_string())
    })?;
    let message = I2npMessage::new_short_transport(
        outer_message_id,
        expiration_seconds,
        I2npBody::TunnelData(Box::new(cell)),
    )
    .map_err(|error| OutboundLookupError::Codec(error.to_string()))?;
    let bytes = message
        .encode_short_transport_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .map_err(|error| OutboundLookupError::Codec(error.to_string()))?;
    EncodedI2npMessage::new(bytes).map_err(|error| OutboundLookupError::Codec(error.to_string()))
}

fn dispatch_cells<R: CryptoRng + RngCore>(
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
        let encoded = encode_transport_tunnel_data(cell.cell, payload_header.expiration_ms, rng)?;
        let delivery_id = i2pr_transport::DeliveryId::generate()
            .map_err(|error| OutboundLookupError::Codec(format!("{error:?}")))?;
        let delivery = DeliveryRequest::with_id(
            delivery_id,
            PeerId::from_hash(cell.target_router.hash()),
            encoded,
            deadline,
        );
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
#[allow(clippy::too_many_arguments)]
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
    use i2pr_netdb::{LookupId, LookupKind, ReplyPath, RouterHash};
    use i2pr_proto::{Hash, SHORT_TRANSPORT_HEADER_SIZE, TUNNEL_DATA_PAYLOAD_SIZE};
    use i2pr_tunnel::build_crypto::LayerKeys;
    use i2pr_tunnel::established::{
        EstablishedHop, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
    };
    use i2pr_tunnel::identity::{TunnelDirection, TunnelId, TunnelPeer};
    use i2pr_tunnel::layer::DuplicateWindow;
    use i2pr_tunnel::roles::{
        OutboundEndpointRole, OutboundGatewayRole, OutboundParticipantRole, RouterDeliveryAction,
        RouterDeliveryKind,
    };
    use rand_core::{CryptoRng, SeedableRng};
    use std::collections::HashSet;

    fn rng_seed(seed: u64) -> impl CryptoRng {
        rand_chacha::ChaCha8Rng::seed_from_u64(seed)
    }

    fn peer(value: u8) -> TunnelPeer {
        TunnelPeer::from_hash(Hash::from_bytes([value; 32]))
    }

    fn keys(seed: u8) -> LayerKeys {
        LayerKeys::new(
            [seed; 32],
            [seed.wrapping_add(1); 32],
            [seed.wrapping_add(2); 32],
        )
    }

    /// One-hop outbound: [OBEP].
    fn build_one_hop_outbound() -> OutboundGatewayRole {
        let hops = vec![EstablishedHop::terminal(
            peer(1),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(0x100).expect("id"),
            keys(0x10),
        )];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            None,
            None,
        )
        .expect("tunnel");
        OutboundGatewayRole::new(tunnel, 60_000)
    }

    /// Two-hop outbound: [Participant, OBEP].
    fn build_two_hop_outbound() -> OutboundGatewayRole {
        let hops = vec![
            EstablishedHop::with_next(
                peer(1),
                EstablishedRole::Participant,
                TunnelId::new(0x100).expect("id"),
                keys(0x10),
                EstablishedNextHop::new(peer(2), TunnelId::new(0x200).expect("id")),
            ),
            EstablishedHop::terminal(
                peer(2),
                EstablishedRole::OutboundEndpoint,
                TunnelId::new(0x200).expect("id"),
                keys(0x11),
            ),
        ];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            None,
            None,
        )
        .expect("tunnel");
        OutboundGatewayRole::new(tunnel, 60_000)
    }

    fn make_lookup_action(
        lookup_key: &Hash,
        peer: &RouterHash,
        gateway: &RouterHash,
        tunnel_id: u32,
    ) -> LookupAction {
        let message = i2pr_netdb::build_databaselookup(
            &RouterHash::from_bytes(*lookup_key.as_bytes()),
            LookupKind::RouterInfo,
            Some(&ReplyPath::new(*gateway, tunnel_id).expect("path")),
            &[RouterHash::from_bytes(*lookup_key.as_bytes())],
        )
        .expect("lookup");
        LookupAction::SendDatabaselookup {
            lookup_id: LookupId::new(1, LookupKind::RouterInfo, *peer),
            peer: *peer,
            message,
        }
    }

    /// Process a TunnelData cell through the OBEP (one-hop case) to
    /// recover the delivery action.
    fn process_through_obep(
        obep: &mut OutboundEndpointRole,
        cell: &i2pr_proto::TunnelDataMessage,
    ) -> RouterDeliveryAction {
        obep.process(&peer(0).hash(), cell, 0)
            .expect("OBEP process")
            .expect("delivery action")
    }

    /// Process a TunnelData cell through participant + OBEP (two-hop case).
    fn process_through_participant_and_obep(
        participant: &mut OutboundParticipantRole,
        obep: &mut OutboundEndpointRole,
        cell: &i2pr_proto::TunnelDataMessage,
    ) -> RouterDeliveryAction {
        let after_participant = participant
            .process(&peer(0).hash(), cell, 0)
            .expect("participant forward");
        obep.process(&peer(1).hash(), &after_participant, 0)
            .expect("OBEP process")
            .expect("delivery action")
    }

    /// Build OBEP and (optionally) participant from the role's hops.
    fn build_obep_and_participant(
        role: &OutboundGatewayRole,
    ) -> (Option<OutboundParticipantRole>, OutboundEndpointRole) {
        let tunnel = role.established();
        let hops = tunnel.hops();
        let participant = if hops.len() >= 2 {
            Some(
                OutboundParticipantRole::new(&hops[0], DuplicateWindow::new(16), 60_000)
                    .expect("participant"),
            )
        } else {
            None
        };
        let obep_index = hops.len() - 1;
        let obep = OutboundEndpointRole::new(
            &hops[obep_index],
            DuplicateWindow::new(16),
            16,
            1 << 20,
            60_000,
            60_000,
            0,
        );
        (participant, obep)
    }

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

    // -----------------------------------------------------------------------
    // C1 — routing identity tests
    // -----------------------------------------------------------------------

    #[test]
    fn outbound_lookup_routes_to_selected_floodfill_not_lookup_key() {
        let lookup_key = Hash::from_bytes([0x11u8; 32]);
        let selected_floodfill = Hash::from_bytes([0x22u8; 32]);
        let outbound_first_hop = Hash::from_bytes([0x33u8; 32]);

        let peer_rh = RouterHash::from_bytes(*selected_floodfill.as_bytes());
        let gateway_rh = RouterHash::from_bytes(*outbound_first_hop.as_bytes());
        let action = make_lookup_action(&lookup_key, &peer_rh, &gateway_rh, 0x100);

        let role = build_one_hop_outbound();
        let dispatch = compose_outbound_lookup(
            &action,
            &role,
            42,
            60_000,
            Deadline::new(std::time::Duration::from_secs(60_000)).expect("deadline"),
            &mut rng_seed(1),
            0,
        )
        .expect("dispatch");

        let delivery = dispatch.first().expect("one delivery");

        // DeliveryRequest target must equal outbound first hop P (peer(1) from the role).
        assert_eq!(
            delivery.target(),
            PeerId::from_hash(peer(1).hash()),
            "DeliveryRequest target must be the outbound first hop"
        );

        // Decode the outer short-transport to get the TunnelData cell.
        let delivery_bytes = delivery.message_bytes();
        let decoded = I2npMessage::decode_short_transport(delivery_bytes, MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode short transport");
        let I2npBody::TunnelData(tunnel_data) = decoded.body() else {
            panic!("expected TunnelData body");
        };
        assert_eq!(tunnel_data.tunnel_id, 0x100);

        // Process through the OBEP to recover the delivery instruction.
        let (mut participant_opt, mut obep) = build_obep_and_participant(&role);
        let action_result = if let Some(ref mut participant) = participant_opt {
            process_through_participant_and_obep(participant, &mut obep, tunnel_data)
        } else {
            process_through_obep(&mut obep, tunnel_data)
        };

        // The OBEP must report Router delivery to the selected floodfill F, not K.
        assert_eq!(action_result.kind, RouterDeliveryKind::Router);
        assert_eq!(
            action_result.target_router, selected_floodfill,
            "Tunnel ROUTER delivery must target the selected floodfill F, not lookup key K"
        );
    }

    #[test]
    fn outbound_lookup_peer_and_key_may_differ() {
        let lookup_key = Hash::from_bytes([0xAAu8; 32]);
        let selected_floodfill = Hash::from_bytes([0xBBu8; 32]);
        let outbound_first_hop = Hash::from_bytes([0x33u8; 32]);

        let peer_rh = RouterHash::from_bytes(*selected_floodfill.as_bytes());
        let gateway_rh = RouterHash::from_bytes(*outbound_first_hop.as_bytes());
        let action = make_lookup_action(&lookup_key, &peer_rh, &gateway_rh, 0x100);

        let role = build_one_hop_outbound();
        let dispatch = compose_outbound_lookup(
            &action,
            &role,
            1,
            60_000,
            Deadline::new(std::time::Duration::from_secs(60_000)).expect("deadline"),
            &mut rng_seed(2),
            0,
        )
        .expect("dispatch");

        assert_eq!(dispatch.cell_count, 1);
        let delivery = dispatch.first().expect("one delivery");

        // Decode the outer short-transport.
        let bytes = delivery.message_bytes();
        let decoded =
            I2npMessage::decode_short_transport(bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode");
        let I2npBody::TunnelData(tunnel_data) = decoded.body() else {
            panic!("expected TunnelData body");
        };
        assert_eq!(tunnel_data.tunnel_id, 0x100);

        // Process through the OBEP and verify the DatabaseLookup key is K.
        let (mut participant_opt, mut obep) = build_obep_and_participant(&role);
        let action_result = if let Some(ref mut participant) = participant_opt {
            process_through_participant_and_obep(participant, &mut obep, tunnel_data)
        } else {
            process_through_obep(&mut obep, tunnel_data)
        };

        // The message bytes are the nested standard-header I2NP DatabaseLookup.
        let nested = I2npMessage::decode_standard(&action_result.message, MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode standard");
        let I2npBody::DatabaseLookup(db_lookup) = nested.body() else {
            panic!("expected DatabaseLookup body");
        };
        assert_eq!(
            db_lookup.key, lookup_key,
            "DatabaseLookup.key must remain the lookup key K, not the peer F"
        );
    }

    #[test]
    fn publication_routes_to_selected_floodfill() {
        let local_hash = Hash::from_bytes([0xAAu8; 32]);
        let selected_floodfill = Hash::from_bytes([0xBBu8; 32]);

        let store_message = DatabaseStoreMessage {
            key: local_hash,
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: i2pr_proto::DatabaseStoreData::RouterInfoCompressed(
                i2pr_proto::DeferredPayload::new(vec![0xAA_u8; 64], MAX_I2NP_PAYLOAD_SIZE)
                    .expect("payload"),
            ),
        };

        let role = build_one_hop_outbound();
        let dispatch = compose_outbound_publication(
            &store_message,
            selected_floodfill,
            &role,
            1,
            60_000,
            Deadline::new(std::time::Duration::from_secs(60_000)).expect("deadline"),
            &mut rng_seed(7),
            0,
        )
        .expect("dispatch");

        let delivery = dispatch.first().expect("one delivery");

        // DeliveryRequest target must equal outbound first hop.
        assert_eq!(
            delivery.target(),
            PeerId::from_hash(peer(1).hash()),
            "publication DeliveryRequest target must be the outbound first hop"
        );

        // Decode the outer short-transport and process through OBEP.
        let delivery_bytes = delivery.message_bytes();
        let decoded = I2npMessage::decode_short_transport(delivery_bytes, MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode");
        let I2npBody::TunnelData(tunnel_data) = decoded.body() else {
            panic!("expected TunnelData body");
        };

        let (mut participant_opt, mut obep) = build_obep_and_participant(&role);
        let action_result = if let Some(ref mut participant) = participant_opt {
            process_through_participant_and_obep(participant, &mut obep, tunnel_data)
        } else {
            process_through_obep(&mut obep, tunnel_data)
        };

        // Verify the stored key is still local_hash (not the floodfill).
        let nested = I2npMessage::decode_standard(&action_result.message, MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode standard");
        let I2npBody::DatabaseStore(db_store) = nested.body() else {
            panic!("expected DatabaseStore body");
        };
        assert_eq!(
            db_store.key, local_hash,
            "DatabaseStore.key must remain the local router hash"
        );
    }

    // -----------------------------------------------------------------------
    // C2 — outer framing tests
    // -----------------------------------------------------------------------

    #[test]
    fn outbound_lookup_delivery_is_complete_short_transport_tunneldata() {
        let lookup_key = Hash::from_bytes([0x11u8; 32]);
        let selected_floodfill = Hash::from_bytes([0x22u8; 32]);
        let outbound_first_hop = Hash::from_bytes([0x33u8; 32]);

        let peer_rh = RouterHash::from_bytes(*selected_floodfill.as_bytes());
        let gateway_rh = RouterHash::from_bytes(*outbound_first_hop.as_bytes());
        let action = make_lookup_action(&lookup_key, &peer_rh, &gateway_rh, 0x100);

        let role = build_one_hop_outbound();
        let dispatch = compose_outbound_lookup(
            &action,
            &role,
            42,
            60_000,
            Deadline::new(std::time::Duration::from_secs(60_000)).expect("deadline"),
            &mut rng_seed(3),
            0,
        )
        .expect("dispatch");

        for delivery in &dispatch.deliveries {
            let bytes = delivery.message_bytes();
            // Must be a valid short-transport message.
            let decoded =
                I2npMessage::decode_short_transport(bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode");
            // Must be TunnelData.
            let I2npBody::TunnelData(tunnel_data) = decoded.body() else {
                panic!("expected TunnelData body");
            };
            // TunnelData tunnel_id must match the outbound first-hop receive tunnel ID.
            assert_eq!(tunnel_data.tunnel_id, 0x100);
            // TunnelData data must be exactly TUNNEL_DATA_PAYLOAD_SIZE.
            assert_eq!(tunnel_data.data.len(), TUNNEL_DATA_PAYLOAD_SIZE);
        }
    }

    #[test]
    fn raw_1028_byte_tunneldata_body_is_not_transport_message() {
        let lookup_key = Hash::from_bytes([0x11u8; 32]);
        let selected_floodfill = Hash::from_bytes([0x22u8; 32]);
        let outbound_first_hop = Hash::from_bytes([0x33u8; 32]);

        let peer_rh = RouterHash::from_bytes(*selected_floodfill.as_bytes());
        let gateway_rh = RouterHash::from_bytes(*outbound_first_hop.as_bytes());
        let action = make_lookup_action(&lookup_key, &peer_rh, &gateway_rh, 0x100);

        let role = build_one_hop_outbound();
        let dispatch = compose_outbound_lookup(
            &action,
            &role,
            42,
            60_000,
            Deadline::new(std::time::Duration::from_secs(60_000)).expect("deadline"),
            &mut rng_seed(4),
            0,
        )
        .expect("dispatch");

        let delivery = dispatch.first().expect("one delivery");
        let encoded_len = delivery.message_len();
        // A raw TunnelData body is 1028 bytes (4 tunnel_id + 1024 data).
        // The encoded message must include the 9-byte short-transport header,
        // so encoded_len >= 1028 + 9 = 1037.
        assert!(
            encoded_len >= TUNNEL_DATA_PAYLOAD_SIZE + SHORT_TRANSPORT_HEADER_SIZE + 4,
            "encoded message must be at least 1028 + 9 = 1037 bytes, got {}",
            encoded_len
        );
    }

    #[test]
    fn nested_database_lookup_remains_standard_header() {
        let lookup_key = Hash::from_bytes([0x11u8; 32]);
        let selected_floodfill = Hash::from_bytes([0x22u8; 32]);
        let outbound_first_hop = Hash::from_bytes([0x33u8; 32]);

        let peer_rh = RouterHash::from_bytes(*selected_floodfill.as_bytes());
        let gateway_rh = RouterHash::from_bytes(*outbound_first_hop.as_bytes());
        let action = make_lookup_action(&lookup_key, &peer_rh, &gateway_rh, 0x100);

        let role = build_one_hop_outbound();
        let dispatch = compose_outbound_lookup(
            &action,
            &role,
            42,
            60_000,
            Deadline::new(std::time::Duration::from_secs(60_000)).expect("deadline"),
            &mut rng_seed(5),
            0,
        )
        .expect("dispatch");

        let delivery = dispatch.first().expect("one delivery");
        let outer_bytes = delivery.message_bytes();
        let outer = I2npMessage::decode_short_transport(outer_bytes, MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode outer");
        let I2npBody::TunnelData(tunnel_data) = outer.body() else {
            panic!("expected TunnelData body");
        };

        // Process through the OBEP to recover the nested message.
        let (mut participant_opt, mut obep) = build_obep_and_participant(&role);
        let action_result = if let Some(ref mut participant) = participant_opt {
            process_through_participant_and_obep(participant, &mut obep, tunnel_data)
        } else {
            process_through_obep(&mut obep, tunnel_data)
        };

        // The nested DatabaseLookup must decode as a standard I2NP header, not short transport.
        let nested = I2npMessage::decode_standard(&action_result.message, MAX_I2NP_PAYLOAD_SIZE)
            .expect("decode standard");
        let I2npBody::DatabaseLookup(db_lookup) = nested.body() else {
            panic!("expected DatabaseLookup body");
        };
        assert_eq!(db_lookup.key, lookup_key);
    }

    #[test]
    fn fragmented_dispatch_uses_distinct_outer_message_ids() {
        // Use a two-hop role to exercise the multi-hop path.
        let lookup_key = Hash::from_bytes([0x11u8; 32]);
        let selected_floodfill = Hash::from_bytes([0x22u8; 32]);
        let outbound_first_hop = Hash::from_bytes([0x33u8; 32]);

        let peer_rh = RouterHash::from_bytes(*selected_floodfill.as_bytes());
        let gateway_rh = RouterHash::from_bytes(*outbound_first_hop.as_bytes());
        let action = make_lookup_action(&lookup_key, &peer_rh, &gateway_rh, 0x100);

        let role = build_two_hop_outbound();
        let dispatch = compose_outbound_lookup(
            &action,
            &role,
            42,
            60_000,
            Deadline::new(std::time::Duration::from_secs(60_000)).expect("deadline"),
            &mut rng_seed(6),
            0,
        )
        .expect("dispatch");

        // Collect outer message IDs from all deliveries.
        let mut outer_ids = HashSet::new();
        for delivery in &dispatch.deliveries {
            let bytes = delivery.message_bytes();
            let decoded =
                I2npMessage::decode_short_transport(bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode");
            let header = decoded.header();
            let id = match header {
                i2pr_proto::I2npHeader::ShortTransport { message_id, .. } => message_id,
                _ => panic!("expected ShortTransport header"),
            };
            outer_ids.insert(id);
        }
        // With a fresh CSPRNG, each cell should get a distinct outer message ID.
        assert_eq!(
            outer_ids.len(),
            dispatch.cell_count,
            "each cell must have a distinct outer message ID"
        );
    }

    #[test]
    fn publication_delivery_is_complete_short_transport_tunneldata() {
        let local_hash = Hash::from_bytes([0xAAu8; 32]);
        let selected_floodfill = Hash::from_bytes([0xBBu8; 32]);

        let store_message = DatabaseStoreMessage {
            key: local_hash,
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: i2pr_proto::DatabaseStoreData::RouterInfoCompressed(
                i2pr_proto::DeferredPayload::new(vec![0xAA_u8; 64], MAX_I2NP_PAYLOAD_SIZE)
                    .expect("payload"),
            ),
        };

        let role = build_one_hop_outbound();
        let dispatch = compose_outbound_publication(
            &store_message,
            selected_floodfill,
            &role,
            1,
            60_000,
            Deadline::new(std::time::Duration::from_secs(60_000)).expect("deadline"),
            &mut rng_seed(8),
            0,
        )
        .expect("dispatch");

        for delivery in &dispatch.deliveries {
            let bytes = delivery.message_bytes();
            let decoded =
                I2npMessage::decode_short_transport(bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode");
            let I2npBody::TunnelData(tunnel_data) = decoded.body() else {
                panic!("expected TunnelData body");
            };
            assert_eq!(tunnel_data.tunnel_id, 0x100);
            assert_eq!(tunnel_data.data.len(), TUNNEL_DATA_PAYLOAD_SIZE);
        }
    }

    #[test]
    fn short_transport_expiration_overflow_fails_closed() {
        let lookup_key = Hash::from_bytes([0x11u8; 32]);
        let selected_floodfill = Hash::from_bytes([0x22u8; 32]);
        let outbound_first_hop = Hash::from_bytes([0x33u8; 32]);

        let peer_rh = RouterHash::from_bytes(*selected_floodfill.as_bytes());
        let gateway_rh = RouterHash::from_bytes(*outbound_first_hop.as_bytes());
        let action = make_lookup_action(&lookup_key, &peer_rh, &gateway_rh, 0x100);

        let role = build_one_hop_outbound();
        let result = compose_outbound_lookup(
            &action,
            &role,
            42,
            u64::MAX, // expiration_ms overflow
            Deadline::new(std::time::Duration::from_secs(60_000)).expect("deadline"),
            &mut rng_seed(9),
            0,
        );
        assert!(
            result.is_err(),
            "overflow expiration must fail closed, not panic"
        );
    }

    #[test]
    fn outbound_lookup_first_hop_is_role_first_hop() {
        let lookup_key = Hash::from_bytes([0x11u8; 32]);
        let selected_floodfill = Hash::from_bytes([0x22u8; 32]);
        let outbound_first_hop = Hash::from_bytes([0x33u8; 32]);

        let peer_rh = RouterHash::from_bytes(*selected_floodfill.as_bytes());
        let gateway_rh = RouterHash::from_bytes(*outbound_first_hop.as_bytes());
        let action = make_lookup_action(&lookup_key, &peer_rh, &gateway_rh, 0x100);

        let role = build_two_hop_outbound();
        let dispatch = compose_outbound_lookup(
            &action,
            &role,
            42,
            60_000,
            Deadline::new(std::time::Duration::from_secs(60_000)).expect("deadline"),
            &mut rng_seed(10),
            0,
        )
        .expect("dispatch");

        // The first hop of the two-hop role is peer(1) with receive tunnel 0x100.
        let delivery = dispatch.first().expect("one delivery");
        assert_eq!(
            delivery.target(),
            PeerId::from_hash(peer(1).hash()),
            "DeliveryRequest target must be the OBGW role's first_hop_router (P)"
        );

        // The tunnel_id in the TunnelData cell must be the first-hop receive tunnel.
        let bytes = delivery.message_bytes();
        let decoded =
            I2npMessage::decode_short_transport(bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode");
        let I2npBody::TunnelData(tunnel_data) = decoded.body() else {
            panic!("expected TunnelData body");
        };
        assert_eq!(tunnel_data.tunnel_id, 0x100);
    }
}
