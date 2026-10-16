//! Plan 184 authenticated router-I2NP dispatch spine.
//!
//! The daemon-owned SSU2 runtime delivers authenticated inbound I2NP
//! through [`i2pr_runtime::Ssu2InboundI2np`]. This module owns the
//! single central dispatcher above the existing
//! [`crate::inbound_dispatch`] tunnel-payload helper plus the narrow
//! outbound router-delivery capability used by future tunnel/NetDB
//! plans.
//!
//! ```text
//! Ssu2InboundI2np { authenticated peer/link metadata, encoded message }
//!  -> bounded standard/short I2NP decode
//!  -> expiration/size/type validation
//!  -> typed dispatch outcome
//! ```
//!
//! Properties:
//!
//! - the authenticated peer/link identity travels with every outcome;
//!   no caller-supplied peer identity enters the live path;
//! - standard (16-byte) and short-transport (9-byte) headers are both
//!   accepted and explicitly classified; the obsolete 5-byte short-SSU
//!   header is rejected boundedly;
//! - expiration is validated against the supplied wall clock with a
//!   bounded future horizon; size is bounded by the transport
//!   boundary; unknown/unsupported bodies receive an explicit bounded
//!   disposition and never panic or retain unbounded state;
//! - outbound delivery resolves an already-established authenticated
//!   link through the existing `Ssu2RuntimeService::send_i2np` path
//!   with no task per delivery and no global router context leaking
//!   into tunnel/NetDB/client crates.
//!
//! Initial destinations:
//!
//! - `ShortTunnelBuild` / `OutboundTunnelBuildReply`: typed hook
//!   reserved for Plan 185 (no tunnel-build claim in Plan 184);
//! - `TunnelData`: typed hook into the existing data-plane
//!   registry/`inbound_dispatch` seam (Plan 185 owns the live cells);
//! - `DatabaseStore` / `DatabaseLookup` / `DatabaseSearchReply` /
//!   `DeliveryStatus`: typed router-control/NetDB hooks (Plan 186
//!   owns live NetDB exchange);
//! - every other known body or unknown type byte: bounded explicit
//!   `Unsupported` disposition.
//!
//! `inbound_dispatch.rs` stays focused on recovered tunnel payloads;
//! this dispatcher sits above it and never duplicates its NetDB
//! normalization logic.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use i2pr_crypto::{OsRng, RouterIdentityBundle, router_identity_hash};
use i2pr_netdb::{CONTROLLED_ROUTER_VERSION, controlled_router_options};
use i2pr_proto::{
    Date, Hash, I2npBody, I2npHeader, I2npMessage, Mapping, MessageType, RouterAddress,
    SHORT_BUILD_RECORD_SIZE,
};
use i2pr_runtime::{
    CancellationToken, ChildScope, IntroKey, Ssu2DialTarget, Ssu2EstablishedLink,
    Ssu2IdentityMaterial, Ssu2PublicKey, Ssu2RouterAddress, Ssu2RuntimeConfig,
    Ssu2RuntimeDeadlines, Ssu2RuntimeLimits, Ssu2RuntimeService, Ssu2SendOutcome,
    Ssu2ServiceHandle, Ssu2SocketConfig, constants,
};
use i2pr_storage::{Ntcp2TransportIdentityMaterial, Ssu2TransportIdentityMaterial};
use i2pr_transport::{EncodedI2npMessage, LinkId, MAX_I2NP_MESSAGE_BYTES, PeerId};
use thiserror::Error;

use crate::config::{Ssu2Config, Ssu2QualificationProfile};

/// Largest encoded router-I2NP message accepted by the dispatcher.
///
/// Matches the transport boundary so no second buffering policy can
/// diverge from `send_i2np` admission.
pub const MAX_ROUTER_I2NP_BYTES: usize = MAX_I2NP_MESSAGE_BYTES;
/// Bounded future horizon for router-I2NP expiration.
///
/// Inbound expirations beyond `now + horizon` are rejected as
/// `FarFuture` rather than queued. The horizon comfortably covers the
/// controlled-loopback control exchange (driver uses `now + 60 s`)
/// while rejecting tunnel-style multi-hour horizons that never belong
/// on the direct router link.
pub const MAX_ROUTER_I2NP_FUTURE_MS: u64 = 15 * 60 * 1000;
/// Maximum outbound router-delivery timeout accepted by the narrow
/// capability. Keeps one delivery bounded without a per-delivery task.
pub const MAX_ROUTER_DELIVERY_TIMEOUT: Duration = Duration::from_secs(60);
/// Minimum outbound router-delivery timeout (nonzero).
pub const MIN_ROUTER_DELIVERY_TIMEOUT: Duration = Duration::from_millis(1);

/// Which I2NP header variant carried the dispatched message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterI2npHeaderKind {
    /// 16-byte standard header with millisecond expiration.
    Standard,
    /// 9-byte NTCP2/SSU2 short-transport header with seconds expiration.
    ShortTransport,
}

/// Typed router-I2NP body classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterI2npKind {
    /// Short tunnel-build request (Plan 185 reserved).
    ShortTunnelBuild,
    /// Short tunnel-build reply (Plan 185 reserved).
    OutboundTunnelBuildReply,
    /// Tunnel data cell (Plan 185 live path).
    TunnelData,
    /// RouterInfo/LeaseSet store (Plan 186 live path).
    DatabaseStore,
    /// RouterInfo lookup (Plan 186 live path).
    DatabaseLookup,
    /// NetDB search reply (Plan 186 live path).
    DatabaseSearchReply,
    /// Delivery confirmation (preflight observable control).
    DeliveryStatus,
}

/// Typed outcome of one authenticated router-I2NP dispatch.
///
/// Every variant preserves the authenticated peer/link identity from
/// the live runtime handoff. Lengths are reported, never payloads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterI2npOutcome {
    /// Short-build request/reply reserved for the Plan 185 tunnel
    /// coordinator. No build is attempted in Plan 184.
    TunnelBuildReserved {
        /// Which reserved build body arrived.
        kind: RouterI2npKind,
        /// Which header variant carried it.
        header: RouterI2npHeaderKind,
        /// Transport message identifier.
        message_id: u32,
        /// Validated expiration in milliseconds since the Unix epoch.
        expiration_ms: u64,
        /// Authenticated peer reference.
        peer: PeerId,
        /// Exact link the message arrived on.
        link_id: LinkId,
        /// Encoded length of the dispatched message.
        encoded_len: usize,
    },
    /// Peer-testing message. The answer owed to the peer is derived from
    /// this authenticated delivery, never from a caller-supplied identity.
    PeerTest {
        /// Which header variant carried it.
        header: RouterI2npHeaderKind,
        /// Transport message identifier.
        message_id: u32,
        /// Validated expiration in milliseconds since the Unix epoch.
        expiration_ms: u64,
        /// Identifier the responder echoes unchanged.
        peer_test_msg_id: u32,
        /// Opaque timestamp the responder echoes unchanged.
        peer_test_timestamp: u64,
        /// Authenticated peer reference.
        peer: PeerId,
        /// Exact link the message arrived on.
        link_id: LinkId,
        /// Encoded length of the dispatched message.
        encoded_len: usize,
    },
    /// TunnelData cell for the existing data-plane registry seam.
    TunnelData {
        /// Which header variant carried it.
        header: RouterI2npHeaderKind,
        /// Transport message identifier.
        message_id: u32,
        /// Validated expiration in milliseconds since the Unix epoch.
        expiration_ms: u64,
        /// Tunnel identifier carried by the cell body.
        tunnel_id: u32,
        /// Authenticated peer reference.
        peer: PeerId,
        /// Exact link the message arrived on.
        link_id: LinkId,
        /// Encoded length of the dispatched message.
        encoded_len: usize,
    },
    /// Direct router-control/NetDB message for future hooks.
    RouterControl {
        /// Which control body arrived.
        kind: RouterI2npKind,
        /// Which header variant carried it.
        header: RouterI2npHeaderKind,
        /// Transport message identifier.
        message_id: u32,
        /// Validated expiration in milliseconds since the Unix epoch.
        expiration_ms: u64,
        /// Authenticated peer reference.
        peer: PeerId,
        /// Exact link the message arrived on.
        link_id: LinkId,
        /// Encoded length of the dispatched message.
        encoded_len: usize,
    },
    /// Bounded explicit disposition for unsupported/unknown bodies.
    /// Never a panic, never unbounded retention.
    Unsupported {
        /// I2NP type byte of the rejected body.
        type_byte: u8,
        /// Authenticated peer reference.
        peer: PeerId,
        /// Exact link the message arrived on.
        link_id: LinkId,
        /// Encoded length of the rejected message.
        encoded_len: usize,
    },
}

impl RouterI2npOutcome {
    /// Returns the authenticated peer preserved by this outcome.
    pub const fn peer(self) -> PeerId {
        match self {
            Self::TunnelBuildReserved { peer, .. }
            | Self::PeerTest { peer, .. }
            | Self::TunnelData { peer, .. }
            | Self::RouterControl { peer, .. }
            | Self::Unsupported { peer, .. } => peer,
        }
    }

    /// Returns the exact link preserved by this outcome.
    pub const fn link_id(self) -> LinkId {
        match self {
            Self::TunnelBuildReserved { link_id, .. }
            | Self::PeerTest { link_id, .. }
            | Self::TunnelData { link_id, .. }
            | Self::RouterControl { link_id, .. }
            | Self::Unsupported { link_id, .. } => link_id,
        }
    }

    /// Returns the decoded transport message ID where the outcome carries one.
    pub const fn message_id(self) -> Option<u32> {
        match self {
            Self::TunnelBuildReserved { message_id, .. }
            | Self::PeerTest { message_id, .. }
            | Self::TunnelData { message_id, .. }
            | Self::RouterControl { message_id, .. } => Some(message_id),
            Self::Unsupported { .. } => None,
        }
    }

    /// Returns the encoded length carried by this outcome.
    pub const fn encoded_len(self) -> usize {
        match self {
            Self::TunnelBuildReserved { encoded_len, .. }
            | Self::PeerTest { encoded_len, .. }
            | Self::TunnelData { encoded_len, .. }
            | Self::RouterControl { encoded_len, .. }
            | Self::Unsupported { encoded_len, .. } => encoded_len,
        }
    }
}

/// Bounded dispatcher failures.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RouterI2npError {
    /// No bytes were supplied.
    #[error("router I2NP message must not be empty")]
    Empty,
    /// The encoded message exceeds the transport boundary.
    #[error("router I2NP message exceeds its bound")]
    TooLarge {
        /// Supplied encoded length.
        actual: usize,
        /// Transport boundary maximum.
        maximum: usize,
    },
    /// Neither the standard nor the short-transport decoder accepted
    /// the bytes.
    #[error("router I2NP decoding failed: {0}")]
    Codec(String),
    /// The message expiration is in the past or undefined.
    #[error("router I2NP message is expired")]
    Expired,
    /// The message expiration exceeds the bounded future horizon.
    #[error("router I2NP expiration exceeds its horizon")]
    FarFuture,
}

/// Narrow typed handoff carrying the already-decoded transit bodies
/// from the single canonical router-I2NP decode (Plan 254 work
/// package B).
///
/// The struct is derived directly from the canonical decoded
/// [`I2npMessage`] inside
/// [`dispatch_router_i2np_with_transit_bodies`]; it performs no
/// second decode, retains no unbounded queue, and logs no payload.
/// Ordinary callers that only need [`RouterI2npOutcome`] keep using
/// [`dispatch_router_i2np`] and never retain these bytes.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct TransitInboundBodies {
    /// Exact count-prefixed STBM body expected by
    /// `process_short_build_message` (`[count] + records`). Present
    /// only for `ShortTunnelBuild`; never for
    /// `OutboundTunnelBuildReply`.
    pub short_build_body: Option<Vec<u8>>,
    /// Bounded copy of the inbound `TunnelData` cell. Present only
    /// for `TunnelData`.
    pub tunnel_data_cell: Option<i2pr_proto::TunnelDataMessage>,
    /// Bounded TunnelGateway ingress parts. Present only for
    /// `TunnelGateway`.
    pub tunnel_gateway: Option<TransitGatewayParts>,
    /// Bounded opaque payload of an inbound `Garlic` message.
    /// Present only for `Garlic`. The transit service may unwrap
    /// a Noise-N router garlic (a creator-routed build request
    /// the reference garlic-wrapped because its own outbound
    /// tunnel endpoint differs from this hop) and dispatch the
    /// inner short-build body; every other garlic keeps the
    /// existing `Unsupported` outcome untouched.
    pub garlic_opaque: Option<Vec<u8>>,
}

/// Bounded TunnelGateway ingress parts derived from the single
/// canonical decode.
#[derive(Debug, Eq, PartialEq)]
pub struct TransitGatewayParts {
    /// Destination tunnel identifier carried by the gateway body.
    pub tunnel_id: u32,
    /// Complete encoded nested standard I2NP message the gateway
    /// carries.
    pub nested: Vec<u8>,
}

/// Owned DatabaseStore/DatabaseLookup body extracted from the same authenticated bounded decode.
/// It intentionally has no Debug implementation because DatabaseStore can carry raw record bytes.
pub enum FloodfillControlBody {
    DatabaseStore(i2pr_proto::DatabaseStoreMessage),
    DatabaseLookup(i2pr_proto::DatabaseLookupMessage),
}

/// Narrow borrowed transit-build view (Plan 254 architecture
/// constraint).
///
/// The body slice is the exact count-prefixed STBM body expected by
/// `process_short_build_message`, bounded by existing I2NP limits,
/// with the authenticated `peer` / `link_id` attached to the same
/// decoded message.
#[derive(Debug, Eq, PartialEq)]
pub struct RouterI2npTransitBuild<'a> {
    /// Exact count-prefixed STBM body bytes.
    pub body: &'a [u8],
    /// Transport message identifier.
    pub message_id: u32,
    /// Validated expiration in milliseconds since the Unix epoch.
    pub expiration_ms: u64,
    /// Authenticated peer reference.
    pub peer: PeerId,
    /// Exact link the message arrived on.
    pub link_id: LinkId,
}

/// One canonical bounded decode of an authenticated router message. The
/// normal daemon owner shares this value with NetDB, tunnel, and service
/// consumers so each one observes the same peer/link attribution.
pub struct DecodedRouterI2np {
    message: Option<I2npMessage>,
    outcome: RouterI2npOutcome,
}

impl DecodedRouterI2np {
    /// Returns the decoded standard or short-transport message, if its type
    /// is recognized and supported by the codec.
    pub fn message(&self) -> Option<&I2npMessage> {
        self.message.as_ref()
    }

    /// Returns the typed admission and dispatch classification.
    pub const fn outcome(&self) -> RouterI2npOutcome {
        self.outcome
    }
}

/// Decodes one authenticated message once for all normal router consumers.
pub fn decode_authenticated_router_i2np(
    inbound: &i2pr_runtime::Ssu2InboundI2np,
    now_ms: u64,
) -> Result<DecodedRouterI2np, RouterI2npError> {
    let (message, outcome) =
        dispatch_inner_parts(inbound.peer, inbound.link_id, &inbound.bytes, now_ms)?;
    Ok(DecodedRouterI2np { message, outcome })
}

/// Projects the NetDB control bodies from the canonical decoded message.
pub fn floodfill_body_from_message(message: Option<&I2npMessage>) -> Option<FloodfillControlBody> {
    match message.map(I2npMessage::body) {
        Some(I2npBody::DatabaseStore(message)) => {
            Some(FloodfillControlBody::DatabaseStore((**message).clone()))
        }
        Some(I2npBody::DatabaseLookup(message)) => {
            Some(FloodfillControlBody::DatabaseLookup((**message).clone()))
        }
        _ => None,
    }
}

/// Dispatches one authenticated inbound I2NP message.
///
/// The caller passes the live [`i2pr_runtime::Ssu2InboundI2np`]
/// handoff by reference so the authenticated peer/link identity
/// cannot be substituted. `now_ms` is wall-clock milliseconds since
/// the Unix epoch. The function decodes standard first, then
/// short-transport, validates expiration/size/type, and returns the
/// typed outcome. Unknown type bytes and known-but-unsupported bodies
/// return `Ok(Unsupported)`; only malformed/expired/oversized inputs
/// return `Err`.
pub fn dispatch_router_i2np(
    inbound: &i2pr_runtime::Ssu2InboundI2np,
    now_ms: u64,
) -> Result<RouterI2npOutcome, RouterI2npError> {
    dispatch_router_i2np_parts(inbound.peer, inbound.link_id, &inbound.bytes, now_ms)
}

/// Dispatches one authenticated I2NP message from any router transport through
/// the same bounded decoder used by the SSU2 owner.
pub fn dispatch_router_i2np_parts(
    peer: PeerId,
    link_id: LinkId,
    bytes: &[u8],
    now_ms: u64,
) -> Result<RouterI2npOutcome, RouterI2npError> {
    dispatch_inner_parts(peer, link_id, bytes, now_ms).map(|(_, outcome)| outcome)
}

#[allow(dead_code)]
fn transit_build_view<'a>(
    body: &'a [u8],
    message_id: u32,
    expiration_ms: u64,
    peer: PeerId,
    link_id: LinkId,
) -> RouterI2npTransitBuild<'a> {
    RouterI2npTransitBuild {
        body,
        message_id,
        expiration_ms,
        peer,
        link_id,
    }
}

/// Dispatches one authenticated inbound I2NP message and derives the
/// narrow transit bodies from the same single canonical decode.
///
/// The returned [`TransitInboundBodies`] are derived from the exact
/// decoded [`I2npMessage`] that produced the outcome; no second
/// decoder runs. `short_build_body` is present only for inbound
/// `ShortTunnelBuild` (never for `OutboundTunnelBuildReply`);
/// malformed/expired/far-future input returns `Err` before transit
/// sees any body.
pub fn dispatch_router_i2np_with_transit_bodies(
    inbound: &i2pr_runtime::Ssu2InboundI2np,
    now_ms: u64,
) -> Result<(RouterI2npOutcome, TransitInboundBodies), RouterI2npError> {
    let (message, outcome) = dispatch_inner(inbound, now_ms)?;
    let mut bodies = TransitInboundBodies::default();
    let Some(message) = message.as_ref() else {
        return Ok((outcome, bodies));
    };
    match message.body() {
        I2npBody::ShortTunnelBuild(records) => {
            let expected =
                usize::from(records.count()).saturating_mul(usize::from(records.record_size()));
            if !records.records().is_empty() && records.records().len() == expected {
                let mut body = Vec::with_capacity(1 + expected);
                body.push(records.count());
                body.extend_from_slice(records.records());
                bodies.short_build_body = Some(body);
            }
        }
        I2npBody::TunnelData(cell) => {
            bodies.tunnel_data_cell = Some((**cell).clone());
        }
        I2npBody::TunnelGateway(gateway) => {
            if gateway.tunnel_id != 0
                && let Ok(nested) = gateway
                    .message
                    .encode_standard_to_vec(MAX_ROUTER_I2NP_BYTES)
            {
                bodies.tunnel_gateway = Some(TransitGatewayParts {
                    tunnel_id: gateway.tunnel_id,
                    nested,
                });
            }
        }
        I2npBody::Garlic(opaque)
            if !opaque.payload.as_bytes().is_empty()
                && opaque.payload.as_bytes().len() <= MAX_ROUTER_I2NP_BYTES =>
        {
            // Bounded copy only; unwrap happens in the transit
            // service (which owns the static key), never here.
            bodies.garlic_opaque = Some(opaque.payload.as_bytes().to_vec());
        }
        I2npBody::Garlic(_) => {}
        _ => {}
    }
    // Defence in depth: an OutboundTunnelBuildReply must never be
    // misclassified as an inbound transit build body.
    if matches!(
        outcome,
        RouterI2npOutcome::TunnelBuildReserved {
            kind: RouterI2npKind::OutboundTunnelBuildReply,
            ..
        }
    ) {
        bodies.short_build_body = None;
    }
    Ok((outcome, bodies))
}

/// Dispatches and extracts only DatabaseStore/DatabaseLookup bodies from one canonical decode.
pub fn dispatch_router_i2np_with_floodfill_body(
    inbound: &i2pr_runtime::Ssu2InboundI2np,
    now_ms: u64,
) -> Result<(RouterI2npOutcome, Option<FloodfillControlBody>), RouterI2npError> {
    let decoded = decode_authenticated_router_i2np(inbound, now_ms)?;
    Ok((
        decoded.outcome(),
        floodfill_body_from_message(decoded.message()),
    ))
}

fn dispatch_inner(
    inbound: &i2pr_runtime::Ssu2InboundI2np,
    now_ms: u64,
) -> Result<(Option<I2npMessage>, RouterI2npOutcome), RouterI2npError> {
    dispatch_inner_parts(inbound.peer, inbound.link_id, &inbound.bytes, now_ms)
}

fn dispatch_inner_parts(
    peer: PeerId,
    link_id: LinkId,
    bytes: &[u8],
    now_ms: u64,
) -> Result<(Option<I2npMessage>, RouterI2npOutcome), RouterI2npError> {
    if bytes.is_empty() {
        return Err(RouterI2npError::Empty);
    }
    if bytes.len() > MAX_ROUTER_I2NP_BYTES {
        return Err(RouterI2npError::TooLarge {
            actual: bytes.len(),
            maximum: MAX_ROUTER_I2NP_BYTES,
        });
    }
    // Unknown first-byte types receive the bounded unsupported
    // disposition without entering either decoder.
    let first = bytes[0];
    if matches!(MessageType::from_code(first), MessageType::Unknown(_)) {
        return Ok((
            None,
            RouterI2npOutcome::Unsupported {
                type_byte: first,
                peer,
                link_id,
                encoded_len: bytes.len(),
            },
        ));
    }
    let standard = I2npMessage::decode_standard(bytes, MAX_ROUTER_I2NP_BYTES);
    let (message, header_kind) = match standard {
        Ok(message) => (message, RouterI2npHeaderKind::Standard),
        Err(first_err) => match I2npMessage::decode_short_transport(bytes, MAX_ROUTER_I2NP_BYTES) {
            Ok(message) => (message, RouterI2npHeaderKind::ShortTransport),
            Err(_) => {
                return Err(RouterI2npError::Codec(format!("{first_err:?}")));
            }
        },
    };
    let (message_id, expiration_ms) = match message.header() {
        I2npHeader::Standard {
            message_id,
            expiration,
            ..
        } => (message_id, expiration.as_millis()),
        I2npHeader::ShortTransport {
            message_id,
            expiration_seconds,
            ..
        } => (
            message_id,
            u64::from(expiration_seconds).saturating_mul(1000),
        ),
        I2npHeader::ShortSsu { .. } => {
            return Err(RouterI2npError::Codec(
                "obsolete short-SSU header is not accepted on the router link".to_owned(),
            ));
        }
    };
    if expiration_ms == 0 || expiration_ms <= now_ms {
        return Err(RouterI2npError::Expired);
    }
    if expiration_ms.saturating_sub(now_ms) > MAX_ROUTER_I2NP_FUTURE_MS {
        return Err(RouterI2npError::FarFuture);
    }
    let outcome = classify_message(
        &message,
        header_kind,
        message_id,
        expiration_ms,
        peer,
        link_id,
        bytes.len(),
    )?;
    Ok((Some(message), outcome))
}

/// Extracts a count-prefixed short-build body from reconstructed
/// OBEP-delivered I2NP bytes when the inner message is a
/// `ShortTunnelBuild`. Returns the body plus the inner message id
/// for creator correlation. Returns `None` for any other message
/// type or undecodable input so opaque deliveries keep their
/// existing path. Envelope decoding lives here (not in transit
/// owners) per the router-handoff boundary: the canonical decode
/// enforces exact consumption, checksum, and bounded shape; the
/// build dispatch re-validates the `1 + n*218` shape before any
/// state mutation, so hostile inner bytes fail closed twice.
pub(crate) fn extract_inner_short_build(message: &[u8]) -> Option<(Vec<u8>, u32)> {
    let decoded = I2npMessage::decode_standard(message, MAX_ROUTER_I2NP_BYTES).ok()?;
    let message_id = decoded.header().message_id()?;
    let I2npBody::ShortTunnelBuild(records) = decoded.body() else {
        return None;
    };
    if usize::from(records.record_size()) != SHORT_BUILD_RECORD_SIZE {
        return None;
    }
    let mut body = Vec::with_capacity(1 + records.records().len());
    body.push(records.count());
    body.extend_from_slice(records.records());
    Some((body, message_id))
}

fn classify_message(
    message: &I2npMessage,
    header: RouterI2npHeaderKind,
    message_id: u32,
    expiration_ms: u64,
    peer: PeerId,
    link_id: LinkId,
    encoded_len: usize,
) -> Result<RouterI2npOutcome, RouterI2npError> {
    match message.body() {
        I2npBody::ShortTunnelBuild(_) => Ok(RouterI2npOutcome::TunnelBuildReserved {
            kind: RouterI2npKind::ShortTunnelBuild,
            header,
            message_id,
            expiration_ms,
            peer,
            link_id,
            encoded_len,
        }),
        I2npBody::OutboundTunnelBuildReply(_) => Ok(RouterI2npOutcome::TunnelBuildReserved {
            kind: RouterI2npKind::OutboundTunnelBuildReply,
            header,
            message_id,
            expiration_ms,
            peer,
            link_id,
            encoded_len,
        }),
        I2npBody::TunnelData(cell) => Ok(RouterI2npOutcome::TunnelData {
            header,
            message_id,
            expiration_ms,
            tunnel_id: cell.tunnel_id,
            peer,
            link_id,
            encoded_len,
        }),
        I2npBody::DatabaseStore(_) => Ok(RouterI2npOutcome::RouterControl {
            kind: RouterI2npKind::DatabaseStore,
            header,
            message_id,
            expiration_ms,
            peer,
            link_id,
            encoded_len,
        }),
        I2npBody::DatabaseLookup(_) => Ok(RouterI2npOutcome::RouterControl {
            kind: RouterI2npKind::DatabaseLookup,
            header,
            message_id,
            expiration_ms,
            peer,
            link_id,
            encoded_len,
        }),
        I2npBody::DatabaseSearchReply(_) => Ok(RouterI2npOutcome::RouterControl {
            kind: RouterI2npKind::DatabaseSearchReply,
            header,
            message_id,
            expiration_ms,
            peer,
            link_id,
            encoded_len,
        }),
        I2npBody::DeliveryStatus(_) => Ok(RouterI2npOutcome::RouterControl {
            kind: RouterI2npKind::DeliveryStatus,
            header,
            message_id,
            expiration_ms,
            peer,
            link_id,
            encoded_len,
        }),
        I2npBody::TunnelTest(test) => Ok(RouterI2npOutcome::PeerTest {
            header,
            message_id,
            expiration_ms,
            peer_test_msg_id: test.msg_id,
            peer_test_timestamp: test.timestamp,
            peer,
            link_id,
            encoded_len,
        }),
        other => Ok(RouterI2npOutcome::Unsupported {
            type_byte: other.message_type().code(),
            peer,
            link_id,
            encoded_len,
        }),
    }
}

/// Bounded outbound router-delivery request.
#[derive(Debug)]
pub struct RouterDeliveryRequest {
    peer: PeerId,
    message: EncodedI2npMessage,
    timeout: Duration,
}

impl RouterDeliveryRequest {
    /// Validates one outbound delivery without touching a socket.
    pub fn new(
        peer: PeerId,
        bytes: Vec<u8>,
        timeout: Duration,
    ) -> Result<Self, RouterDeliveryError> {
        if timeout < MIN_ROUTER_DELIVERY_TIMEOUT {
            return Err(RouterDeliveryError::ZeroTimeout);
        }
        if timeout > MAX_ROUTER_DELIVERY_TIMEOUT {
            return Err(RouterDeliveryError::TimeoutTooLong);
        }
        let message =
            EncodedI2npMessage::new(bytes).map_err(|_| RouterDeliveryError::MessageTooLarge)?;
        Ok(Self {
            peer,
            message,
            timeout,
        })
    }

    /// Returns the target peer reference.
    pub fn peer(&self) -> PeerId {
        self.peer
    }

    /// Returns the delivery timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

/// Builds one bounded direct DatabaseLookup for a controlled outbound-link
/// qualification. The target key is supplied by the caller so it can be
/// retained as the response correlation key; the reply route is always the
/// hash derived from the verified local signed RouterInfo.
pub(crate) fn build_initiator_database_lookup_probe(
    router_info_bytes: &[u8],
    lookup_key: Hash,
    now_ms: u64,
) -> Result<Vec<u8>, RouterDeliveryError> {
    if router_info_bytes.is_empty() {
        return Err(RouterDeliveryError::InvalidControlledProbe);
    }
    let router_info =
        i2pr_proto::RouterInfo::decode(router_info_bytes, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .map_err(|_| RouterDeliveryError::InvalidControlledProbe)?;
    i2pr_crypto::verify_router_info(&router_info)
        .map_err(|_| RouterDeliveryError::InvalidControlledProbe)?;
    let local_hash = router_identity_hash(router_info.router_identity())
        .map_err(|_| RouterDeliveryError::InvalidControlledProbe)?;
    let expiration_seconds = now_ms
        .checked_div(1_000)
        .and_then(|seconds| seconds.checked_add(90))
        .and_then(|seconds| u32::try_from(seconds).ok())
        .ok_or(RouterDeliveryError::InvalidControlledProbe)?;
    let mut envelope_id = u32::from_be_bytes(
        lookup_key.as_bytes()[..4]
            .try_into()
            .map_err(|_| RouterDeliveryError::InvalidControlledProbe)?,
    );
    if envelope_id == 0 {
        envelope_id = 1;
    }
    let lookup = i2pr_proto::DatabaseLookupMessage {
        key: lookup_key,
        from: local_hash,
        delivery_flag: false,
        reply_tunnel_id: None,
        // LeaseSet lookup is accepted directly from floodfill peers by the
        // pinned Java handler. ANY/EXPL requests have a separate anti-floodfill
        // drop branch and are not needed for this response-correlation probe.
        lookup_type: 1,
        excluded_peers: Vec::new(),
        reply_encryption: i2pr_proto::ReplyEncryption::None,
    };
    let message = I2npMessage::new_short_transport(
        envelope_id,
        expiration_seconds,
        I2npBody::DatabaseLookup(Box::new(lookup)),
    )
    .map_err(|_| RouterDeliveryError::InvalidControlledProbe)?;
    message
        .encode_short_transport_to_vec(MAX_ROUTER_I2NP_BYTES)
        .map_err(|_| RouterDeliveryError::InvalidControlledProbe)
}

/// Builds a controlled qualification DatabaseStore for the exact signed local
/// RouterInfo. Its response route is the authenticated router hash and tunnel
/// zero, matching the ordinary direct DeliveryStatus form.
pub(crate) fn build_reply_token_router_info_probe(
    router_info_bytes: &[u8],
    reply_token: u32,
    now_ms: u64,
) -> Result<(PeerId, Vec<u8>), RouterDeliveryError> {
    if reply_token == 0 || reply_token == u32::MAX || router_info_bytes.is_empty() {
        return Err(RouterDeliveryError::InvalidControlledProbe);
    }
    let router_info =
        i2pr_proto::RouterInfo::decode(router_info_bytes, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .map_err(|_| RouterDeliveryError::InvalidControlledProbe)?;
    i2pr_crypto::verify_router_info(&router_info)
        .map_err(|_| RouterDeliveryError::InvalidControlledProbe)?;
    let router_hash = router_identity_hash(router_info.router_identity())
        .map_err(|_| RouterDeliveryError::InvalidControlledProbe)?;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    use std::io::Write as _;
    encoder
        .write_all(router_info_bytes)
        .map_err(|_| RouterDeliveryError::InvalidControlledProbe)?;
    let compressed = encoder
        .finish()
        .map_err(|_| RouterDeliveryError::InvalidControlledProbe)?;
    let payload = i2pr_proto::DeferredPayload::new(compressed, i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
        .map_err(|_| RouterDeliveryError::InvalidControlledProbe)?;
    let store = i2pr_proto::DatabaseStoreMessage {
        key: router_hash,
        reply_token,
        reply_tunnel_id: Some(0),
        reply_gateway: Some(router_hash),
        data: i2pr_proto::DatabaseStoreData::RouterInfoCompressed(payload),
    };
    let expiration_seconds = now_ms
        .checked_div(1_000)
        .and_then(|seconds| seconds.checked_add(90))
        .and_then(|seconds| u32::try_from(seconds).ok())
        .ok_or(RouterDeliveryError::InvalidControlledProbe)?;
    let envelope_id = reply_token.rotate_left(7) ^ 0xA51C_3E29;
    let envelope_id = if envelope_id == 0 { 1 } else { envelope_id };
    let message = I2npMessage::new_short_transport(
        envelope_id,
        expiration_seconds,
        I2npBody::DatabaseStore(Box::new(store)),
    )
    .map_err(|_| RouterDeliveryError::InvalidControlledProbe)?;
    let bytes = message
        .encode_short_transport_to_vec(MAX_ROUTER_I2NP_BYTES)
        .map_err(|_| RouterDeliveryError::InvalidControlledProbe)?;
    Ok((PeerId::from_hash(router_hash), bytes))
}

/// Applies the body-token and timestamp checks used by the live one-shot
/// qualification consumer. The caller separately binds the peer, link
/// generation, and pending-request expiry.
pub(crate) fn reply_token_status_matches(
    expected_token: u32,
    received_token: u32,
    status_timestamp_ms: u64,
    now_ms: u64,
) -> bool {
    expected_token != 0
        && expected_token != u32::MAX
        && received_token == expected_token
        && status_timestamp_ms.saturating_add(300_000) >= now_ms
        && status_timestamp_ms <= now_ms.saturating_add(300_000)
}

/// Validation failures constructing or starting an outbound router-delivery request.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum RouterDeliveryError {
    /// The encoded message was empty or exceeds the transport boundary.
    #[error("router delivery message exceeds its bound")]
    MessageTooLarge,
    /// The timeout was zero.
    #[error("router delivery timeout must be nonzero")]
    ZeroTimeout,
    /// The timeout exceeds the bounded delivery horizon.
    #[error("router delivery timeout exceeds its horizon")]
    TimeoutTooLong,
    /// The controlled NTCP2 probe is invalid.
    #[error("controlled NTCP2 probe is invalid")]
    InvalidControlledProbe,
}

/// Typed outcome of one outbound router delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterDeliveryOutcome {
    /// The message entered the bounded session outbound queue.
    Accepted,
    /// The session or manager queue is full.
    QueueFull,
    /// A shared resource budget denied admission.
    ResourceDenied,
    /// No active authenticated session exists for the peer.
    NoActiveSession,
    /// The encoded message exceeds the transport boundary.
    TooLarge,
    /// The caller deadline had already elapsed.
    DeadlineElapsed,
    /// The caller cancelled before admission.
    Cancelled,
}

/// Process-local completion for one admitted router delivery. A positive
/// value means the selected socket writer accepted the whole delivery; it
/// does not assert remote receipt or protocol acceptance.
pub(crate) struct RouterDeliveryTicket {
    _peer: PeerId,
    _transport: RouterDeliveryTransport,
    receiver: tokio::sync::oneshot::Receiver<bool>,
}

impl RouterDeliveryTicket {
    pub(crate) fn from_receiver(
        peer: PeerId,
        transport: RouterDeliveryTransport,
        receiver: tokio::sync::oneshot::Receiver<bool>,
    ) -> Self {
        Self {
            _peer: peer,
            _transport: transport,
            receiver,
        }
    }

    pub(crate) fn try_result(&mut self) -> Option<bool> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => Some(false),
        }
    }
}

/// The authenticated transport path selected for one product delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RouterDeliveryTransport {
    /// An established SSU2 session accepted the message.
    Ssu2(LinkId),
    /// The exact authenticated NTCP2 link generation accepted the message.
    Ntcp2(LinkId),
}

impl RouterDeliveryTransport {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Ssu2(_) => "ssu2",
            Self::Ntcp2(_) => "ntcp2",
        }
    }
}

/// Bounded registry of currently authenticated NTCP2 link send queues.
///
/// The registry stores only queue handles keyed by authenticated peer and
/// link generation. Runtime active-link admission bounds the number of entries;
/// link tasks remain the sole owners of sockets and remove their exact route.
#[derive(Clone, Debug, Default)]
pub(crate) struct Ntcp2RouterDeliveryRegistry {
    links: Arc<Mutex<HashMap<(PeerId, LinkId), Ntcp2RouterLinkRoute>>>,
    pending_lookups: Arc<Mutex<PendingLookupMap>>,
    dial_requests: Arc<Mutex<Option<tokio::sync::mpsc::Sender<PeerId>>>>,
}

type PendingLookupMap = HashMap<(PeerId, LinkId), (Hash, u64)>;

/// One bounded message plus a source-owned completion edge for a queued
/// NTCP2 delivery. The completion is resolved by the authenticated link owner
/// only after its socket writer accepts the full frame.
pub(crate) struct Ntcp2RouterDeliveryMessage {
    pub(crate) bytes: Vec<u8>,
    pub(crate) completion: Option<tokio::sync::oneshot::Sender<bool>>,
}

#[derive(Clone, Debug)]
struct Ntcp2RouterLinkRoute {
    sender: tokio::sync::mpsc::Sender<Ntcp2RouterDeliveryMessage>,
    outbound: bool,
}

impl Ntcp2RouterDeliveryRegistry {
    /// Attaches the supervised outbound dial request queue.
    pub(crate) fn attach_dial_request_sender(
        &self,
        sender: tokio::sync::mpsc::Sender<PeerId>,
    ) -> bool {
        let Ok(mut requests) = self.dial_requests.lock() else {
            return false;
        };
        *requests = Some(sender);
        true
    }

    /// Requests an authenticated dial for a product destination after both
    /// existing-family route owners reported a miss. The queue is bounded and
    /// may coalesce by rejecting overflow; callers still receive the original
    /// `NoActiveSession` result and can retry after link establishment.
    pub(crate) fn request_outbound_dial(&self, peer: PeerId) -> bool {
        let Ok(requests) = self.dial_requests.lock() else {
            return false;
        };
        if let Some(sender) = requests.as_ref() {
            return sender.try_send(peer).is_ok();
        }
        false
    }

    /// Returns whether a live outbound authenticated generation routes to the
    /// peer. Inbound sessions do not satisfy an outbound dial request: both
    /// directions may coexist for the same authenticated peer.
    pub(crate) fn has_outbound_peer_route(&self, peer: PeerId) -> bool {
        let Ok(mut links) = self.links.lock() else {
            return false;
        };
        links.retain(|_, route| !route.sender.is_closed());
        links
            .iter()
            .any(|((route_peer, _), route)| *route_peer == peer && route.outbound)
    }

    /// Returns the newest live outbound authenticated generation for a peer.
    pub(crate) fn current_outbound_link(&self, peer: PeerId) -> Option<LinkId> {
        let Ok(mut links) = self.links.lock() else {
            return None;
        };
        links.retain(|_, route| !route.sender.is_closed());
        links
            .iter()
            .filter(|((route_peer, _), route)| *route_peer == peer && route.outbound)
            .map(|((_, link_id), _)| *link_id)
            .max_by_key(|link_id| link_id.value())
    }

    /// Returns the newest live authenticated generation for one peer,
    /// regardless of who initiated the link.
    pub(crate) fn current_peer_link(&self, peer: PeerId) -> Option<LinkId> {
        let Ok(mut links) = self.links.lock() else {
            return None;
        };
        links.retain(|_, route| !route.sender.is_closed());
        links
            .keys()
            .filter(|(route_peer, _)| *route_peer == peer)
            .map(|(_, link_id)| *link_id)
            .max_by_key(|link_id| link_id.value())
    }

    /// Registers one bounded queue for this authenticated peer generation.
    /// A peer may hold concurrent inbound and outbound links; generation keys
    /// keep delivery and response correlation attached to the exact socket.
    pub(crate) fn register(
        &self,
        peer: PeerId,
        link_id: LinkId,
        sender: tokio::sync::mpsc::Sender<Ntcp2RouterDeliveryMessage>,
    ) -> bool {
        self.register_link(peer, link_id, sender, false)
    }

    /// Registers a route established by the outbound peer dial owner.
    pub(crate) fn register_outbound(
        &self,
        peer: PeerId,
        link_id: LinkId,
        sender: tokio::sync::mpsc::Sender<Ntcp2RouterDeliveryMessage>,
    ) -> bool {
        self.register_link(peer, link_id, sender, true)
    }

    fn register_link(
        &self,
        peer: PeerId,
        link_id: LinkId,
        sender: tokio::sync::mpsc::Sender<Ntcp2RouterDeliveryMessage>,
        outbound: bool,
    ) -> bool {
        let Ok(mut links) = self.links.lock() else {
            return false;
        };
        if links
            .get(&(peer, link_id))
            .is_some_and(|route| !route.sender.is_closed())
        {
            return false;
        }
        links.insert((peer, link_id), Ntcp2RouterLinkRoute { sender, outbound });
        true
    }

    /// Removes one route only when its link generation still owns the peer.
    pub(crate) fn remove(&self, peer: PeerId, link_id: LinkId) {
        let Ok(mut links) = self.links.lock() else {
            return;
        };
        links.remove(&(peer, link_id));
        if let Ok(mut pending) = self.pending_lookups.lock() {
            pending.remove(&(peer, link_id));
        }
    }

    /// Tracks one expected direct DatabaseSearchReply/DatabaseStore per
    /// authenticated peer generation. The active-link ceiling bounds entries.
    pub(crate) fn register_lookup(
        &self,
        peer: PeerId,
        link_id: LinkId,
        key: Hash,
        now_ms: u64,
        expires_ms: u64,
        maximum: usize,
    ) -> bool {
        if maximum == 0 || expires_ms <= now_ms || !self.is_current(peer, link_id) {
            return false;
        }
        let Ok(mut pending) = self.pending_lookups.lock() else {
            return false;
        };
        pending.retain(|_, (_, expiry)| now_ms < *expiry);
        if pending.len() >= maximum || pending.contains_key(&(peer, link_id)) {
            return false;
        }
        pending.insert((peer, link_id), (key, expires_ms));
        true
    }

    /// Consumes a direct lookup response only when authenticated peer,
    /// current link generation, key, and pending deadline all match.
    pub(crate) fn correlate_lookup_response(
        &self,
        peer: PeerId,
        link_id: LinkId,
        key: Hash,
        now_ms: u64,
    ) -> bool {
        if !self.is_current(peer, link_id) {
            return false;
        }
        let Ok(mut pending) = self.pending_lookups.lock() else {
            return false;
        };
        let Some((expected, expires_ms)) = pending.get(&(peer, link_id)).copied() else {
            return false;
        };
        if now_ms >= expires_ms {
            pending.remove(&(peer, link_id));
            return false;
        }
        if expected != key {
            return false;
        }
        pending.remove(&(peer, link_id));
        true
    }

    /// Removes one pending response expectation after the matching send was
    /// rejected before it entered an authenticated link queue.
    pub(crate) fn cancel_lookup(&self, peer: PeerId, link_id: LinkId) {
        if let Ok(mut pending) = self.pending_lookups.lock() {
            pending.remove(&(peer, link_id));
        }
    }

    /// Checks that an inbound event still belongs to its registered
    /// authenticated connection generation.
    pub(crate) fn is_current(&self, peer: PeerId, link_id: LinkId) -> bool {
        let Ok(mut links) = self.links.lock() else {
            return false;
        };
        let key = (peer, link_id);
        let current = links
            .get(&key)
            .is_some_and(|route| !route.sender.is_closed());
        if !current
            && links
                .get(&key)
                .is_some_and(|route| route.sender.is_closed())
        {
            links.remove(&key);
        }
        current
    }

    /// Attempts bounded delivery and returns the exact selected generation.
    #[cfg(test)]
    fn deliver_with_link(
        &self,
        peer: PeerId,
        bytes: Vec<u8>,
    ) -> (RouterDeliveryOutcome, Option<LinkId>) {
        let (outcome, link, _) = self.deliver_with_link_and_receipt(peer, bytes, false);
        (outcome, link)
    }

    fn deliver_with_link_and_receipt(
        &self,
        peer: PeerId,
        bytes: Vec<u8>,
        completion: bool,
    ) -> (
        RouterDeliveryOutcome,
        Option<LinkId>,
        Option<tokio::sync::oneshot::Receiver<bool>>,
    ) {
        if bytes.is_empty() || bytes.len() > MAX_ROUTER_I2NP_BYTES {
            return (RouterDeliveryOutcome::TooLarge, None, None);
        }
        let Ok(mut links) = self.links.lock() else {
            return (RouterDeliveryOutcome::NoActiveSession, None, None);
        };
        let Some((key, route)) = links
            .iter()
            .filter(|((route_peer, _), route)| *route_peer == peer && !route.sender.is_closed())
            .max_by_key(|((_, link_id), _)| link_id.value())
            .map(|(key, route)| (*key, route.clone()))
        else {
            return (RouterDeliveryOutcome::NoActiveSession, None, None);
        };
        let (completion_sender, completion_receiver) = if completion {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            (Some(sender), Some(receiver))
        } else {
            (None, None)
        };
        match route.sender.try_send(Ntcp2RouterDeliveryMessage {
            bytes,
            completion: completion_sender,
        }) {
            Ok(()) => (
                RouterDeliveryOutcome::Accepted,
                Some(key.1),
                completion_receiver,
            ),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                (RouterDeliveryOutcome::QueueFull, Some(key.1), None)
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                links.remove(&key);
                (RouterDeliveryOutcome::NoActiveSession, None, None)
            }
        }
    }
}

/// Narrow transport-neutral outbound capability.
///
/// The owner first resolves established SSU2 sessions through the
/// existing `send_i2np` seam, then uses the bounded authenticated
/// NTCP2 link registry on a direct miss. No task is spawned per
/// delivery; per-link and runtime queue limits remain authoritative.
/// The type lives in the daemon so tunnel/NetDB/client crates never
/// receive a global router context.
#[derive(Clone, Debug)]
pub struct RouterDeliveryService {
    service: Ssu2RuntimeService,
    ntcp2: Arc<Mutex<Option<Ntcp2RouterDeliveryRegistry>>>,
}

impl RouterDeliveryService {
    /// Wraps the daemon-owned runtime service.
    pub fn new(service: Ssu2RuntimeService) -> Self {
        Self {
            service,
            ntcp2: Arc::new(Mutex::new(None)),
        }
    }

    /// Attaches the daemon-owned NTCP2 link registry for bounded fallback
    /// when the existing SSU2 session owner has no active peer session.
    pub(crate) fn attach_ntcp2_registry(&self, registry: Ntcp2RouterDeliveryRegistry) -> bool {
        let Ok(mut ntcp2) = self.ntcp2.lock() else {
            return false;
        };
        *ntcp2 = Some(registry);
        true
    }

    /// Delivers one validated request to its established session.
    ///
    /// Cancellation is checked before admission; no delivery task is
    /// spawned. Unknown/not-established peers report
    /// `NoActiveSession` explicitly.
    pub fn deliver(
        &self,
        request: RouterDeliveryRequest,
        cancellation: &CancellationToken,
    ) -> RouterDeliveryOutcome {
        self.deliver_with_transport(request, cancellation).0
    }

    /// Applies the common SSU2-first, NTCP2-fallback policy and returns the
    /// exact family/link generation selected for diagnostics and qualification.
    pub(crate) fn deliver_with_transport(
        &self,
        request: RouterDeliveryRequest,
        cancellation: &CancellationToken,
    ) -> (RouterDeliveryOutcome, Option<RouterDeliveryTransport>) {
        let (outcome, transport, _) = self.deliver_inner(request, cancellation, false);
        (outcome, transport)
    }

    /// Delivers one message and returns a private source-owned completion
    /// ticket where the selected writer supports it.
    pub(crate) fn deliver_with_ticket(
        &self,
        request: RouterDeliveryRequest,
        cancellation: &CancellationToken,
    ) -> (
        RouterDeliveryOutcome,
        Option<RouterDeliveryTransport>,
        Option<RouterDeliveryTicket>,
    ) {
        let peer = request.peer;
        let (outcome, transport, ticket) = self.deliver_inner(request, cancellation, true);
        (
            outcome,
            transport,
            ticket.zip(transport).map(|(receiver, transport)| {
                RouterDeliveryTicket::from_receiver(peer, transport, receiver)
            }),
        )
    }

    fn deliver_inner(
        &self,
        request: RouterDeliveryRequest,
        cancellation: &CancellationToken,
        with_ticket: bool,
    ) -> (
        RouterDeliveryOutcome,
        Option<RouterDeliveryTransport>,
        Option<tokio::sync::oneshot::Receiver<bool>>,
    ) {
        if cancellation.is_cancelled() {
            return (RouterDeliveryOutcome::Cancelled, None, None);
        }
        let RouterDeliveryRequest {
            peer,
            message,
            timeout,
        } = request;
        let ntcp2_bytes = message.as_bytes().to_vec();
        let (ssu2_outcome, ssu2_link, ssu2_ticket) = if with_ticket {
            let (outcome, link_id, ticket) =
                self.service.send_i2np_with_receipt(peer, message, timeout);
            (outcome, link_id, ticket)
        } else {
            let (outcome, link_id) = self.service.send_i2np_with_link(peer, message, timeout);
            (outcome, link_id, None)
        };
        let outcome = match ssu2_outcome {
            Ssu2SendOutcome::Accepted => RouterDeliveryOutcome::Accepted,
            Ssu2SendOutcome::QueueFull => RouterDeliveryOutcome::QueueFull,
            Ssu2SendOutcome::ResourceDenied => RouterDeliveryOutcome::ResourceDenied,
            Ssu2SendOutcome::Closed => RouterDeliveryOutcome::NoActiveSession,
            Ssu2SendOutcome::TooLarge => RouterDeliveryOutcome::TooLarge,
            Ssu2SendOutcome::DeadlineElapsed => RouterDeliveryOutcome::DeadlineElapsed,
        };
        if outcome != RouterDeliveryOutcome::NoActiveSession {
            let transport = ssu2_link.map(RouterDeliveryTransport::Ssu2);
            return (outcome, transport, ssu2_ticket);
        }
        let Ok(registry) = self.ntcp2.lock() else {
            return (outcome, None, None);
        };
        match registry.as_ref() {
            Some(registry) => {
                let (outcome, link_id, completion) =
                    registry.deliver_with_link_and_receipt(peer, ntcp2_bytes, with_ticket);
                if outcome == RouterDeliveryOutcome::NoActiveSession {
                    registry.request_outbound_dial(peer);
                }
                (
                    outcome,
                    link_id.map(RouterDeliveryTransport::Ntcp2),
                    completion,
                )
            }
            None => (outcome, None, None),
        }
    }
}

/// Failures constructing or starting the daemon-owned SSU2 service.
#[derive(Debug, Error)]
pub enum Ssu2ServiceError {
    /// The `[ssu2]` surface violates the strict controlled profile.
    #[error("SSU2 controlled profile rejected: {0}")]
    ControlledProfile(String),
    /// Local identity material was rejected.
    #[error("SSU2 identity material is invalid")]
    InvalidIdentity,
    /// The runtime configuration mapping failed validation.
    #[error("SSU2 runtime configuration is invalid: {0}")]
    RuntimeConfig(String),
    /// No loopback socket family was available.
    #[error("SSU2 service has no loopback socket to bind")]
    NoSocket,
    /// The OS rejected the UDP bind.
    #[error("SSU2 socket bind failed")]
    Bind,
    /// Shared service state was unavailable.
    #[error("SSU2 service state unavailable")]
    State,
    /// The child scope could not retain the loop task.
    #[error("SSU2 loop task rejected by scope")]
    Scope,
    /// The persistent router identity could not be loaded.
    #[error("SSU2 router identity unavailable: {0}")]
    Identity(String),
}

impl From<i2pr_runtime::Ssu2BindError> for Ssu2ServiceError {
    fn from(error: i2pr_runtime::Ssu2BindError) -> Self {
        match error {
            i2pr_runtime::Ssu2BindError::NoSocket => Self::NoSocket,
            i2pr_runtime::Ssu2BindError::Bind => Self::Bind,
            i2pr_runtime::Ssu2BindError::State => Self::State,
            i2pr_runtime::Ssu2BindError::Scope => Self::Scope,
        }
    }
}

/// Maps the daemon `[ssu2]` surface to the runtime policy.
///
/// Daemon ceilings stay authoritative; runtime-only fields use the
/// bounded runtime defaults clamped to the daemon ceilings so both
/// bounds hold without double buffering.
pub fn ssu2_runtime_config_from(
    config: &Ssu2Config,
) -> Result<Ssu2RuntimeConfig, Ssu2ServiceError> {
    let default_limits = Ssu2RuntimeLimits::default();
    let default_deadlines = Ssu2RuntimeDeadlines::default();
    let max_active_per_ip = default_limits
        .max_active_per_ip
        .min(config.max_active_sessions);
    let max_active_per_subnet = default_limits
        .max_active_per_subnet
        .min(config.max_active_sessions);
    let dial = default_deadlines.dial.max(config.handshake_timeout);
    let runtime = Ssu2RuntimeConfig {
        limits: Ssu2RuntimeLimits {
            max_pending_handshakes: config.max_pending_handshakes,
            max_pending_per_ip: config.max_pending_per_ip,
            max_pending_per_subnet: config.max_pending_per_subnet,
            max_active_sessions: config.max_active_sessions,
            max_active_per_ip: max_active_per_ip.max(1),
            max_active_per_subnet: max_active_per_subnet.max(1),
            max_outbound_datagrams: config.max_datagram_queue_items,
            max_outbound_bytes: config.max_datagram_queue_bytes,
            max_inbound_i2np_queue: config.max_inbound_i2np_queue,
        },
        deadlines: Ssu2RuntimeDeadlines {
            handshake: config.handshake_timeout,
            dial,
            idle: config.idle_timeout,
            queue_wait: default_deadlines.queue_wait,
            drain: default_deadlines.drain,
            scheduler_poll_max: config.scheduler_poll_max,
        },
        prefixes: i2pr_runtime::IpPrefixPolicy::default(),
        // Explicit-bind corroboration is inert for the private-LAN profile;
        // the runtime rejects non-loopback publication and this composition
        // never records reachability evidence for that profile.
        explicit_bind_corroboration: true,
        // Same posture for the controlled peer-test evidence path (Plan 283): the
        // service parses and routes peer-test datagrams but never responds on its
        // own, records nothing without a live started test, and starts tests only
        // through the narrow controlled APIs the M12 activation composition calls.
        controlled_peer_test: true,
        // Ssu2DaemonService rejects every non-qualified bind above. The
        // private-LAN and controlled-loopback profiles are explicit local
        // qualification modes; ordinary/default-off startup never constructs
        // this runtime observer.
        qualification_observer: true,
    };
    runtime
        .validate()
        .map_err(|error| Ssu2ServiceError::RuntimeConfig(format!("{error}")))
}

fn private_lan_bind_valid(config: &Ssu2Config) -> bool {
    config.enabled
        && config.port >= 1024
        && config.bind_ipv6.is_none()
        && matches!(config.bind_ipv4, Some(std::net::IpAddr::V4(ip)) if ip.is_private())
}

/// Builds the profile-permitted socket selection for the daemon-owned service.
///
/// Both families are re-validated fail-closed here so a future config
/// bypass cannot reach the socket layer. At least one permitted family
/// must be present; `port` comes from the daemon surface (`0` selects
/// an ephemeral port for tests).
pub fn ssu2_socket_config_from(config: &Ssu2Config) -> Result<Ssu2SocketConfig, Ssu2ServiceError> {
    if !config.enabled {
        return Err(Ssu2ServiceError::ControlledProfile(
            "SSU2 service requires enabled = true".to_owned(),
        ));
    }
    if config.advertise || config.introducer_service {
        return Err(Ssu2ServiceError::ControlledProfile(
            "SSU2 controlled profile forbids advertise/introducer".to_owned(),
        ));
    }
    if config.qualification_profile == crate::config::Ssu2QualificationProfile::PrivateLan
        && !private_lan_bind_valid(config)
    {
        return Err(Ssu2ServiceError::ControlledProfile(
            "private-LAN SSU2 requires one RFC1918 IPv4 bind and an explicit non-privileged port"
                .to_owned(),
        ));
    }
    let ipv4 = match config.bind_ipv4 {
        Some(ip) => {
            let allowed = ip.is_loopback()
                || (config.qualification_profile
                    == crate::config::Ssu2QualificationProfile::PrivateLan
                    && matches!(ip, std::net::IpAddr::V4(address) if address.is_private()));
            if !allowed {
                return Err(Ssu2ServiceError::ControlledProfile(
                    "SSU2 IPv4 bind is outside the selected qualification profile".to_owned(),
                ));
            }
            Some(SocketAddr::new(ip, config.port))
        }
        None => None,
    };
    let ipv6 = match config.bind_ipv6 {
        Some(ip) => {
            if !ip.is_loopback() {
                return Err(Ssu2ServiceError::ControlledProfile(
                    "SSU2 IPv6 bind must be loopback".to_owned(),
                ));
            }
            Some(SocketAddr::new(ip, config.port))
        }
        None => None,
    };
    if ipv4.is_none() && ipv6.is_none() {
        return Err(Ssu2ServiceError::NoSocket);
    }
    Ok(Ssu2SocketConfig { ipv4, ipv6 })
}

/// I2P Base64 alphabet (`A-Za-z0-9-~`, `=` padding) for controlled
/// RouterInfo construction. Matches the Plan 142 SAM correction.
fn i2p_b64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let mut n: u32 = 0;
        for byte in chunk {
            n = (n << 8) | u32::from(*byte);
        }
        n <<= 8 * (3 - chunk.len());
        let digits = match chunk.len() {
            1 => 2,
            2 => 3,
            _ => 4,
        };
        for index in 0..digits {
            output.push(ALPHABET[((n >> (18 - 6 * index)) & 0x3f) as usize] as char);
        }
        for _ in digits..4 {
            output.push('=');
        }
    }
    output
}

fn wall_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(1_700_000_000)
}

/// Builds controlled loopback SSU2 identity material for one daemon
/// service instance.
///
/// The caller supplies the persistent router bundle (loaded from the
/// daemon data directory). Static/intro secrets come from the OS
/// CSPRNG; the signed RouterInfo carries exactly one loopback SSU2
/// address for the supplied host/port with the controlled
/// `router.version`/`netId` options the pinned reference requires
/// for ingest. No secret material appears in diagnostics. The address
/// is loopback-only and `advertise = false` stays mandatory, so the
/// record is never a public advertisement.
pub fn generate_controlled_identity(
    bundle: &RouterIdentityBundle,
    host: &str,
    port: u16,
) -> Result<Ssu2IdentityMaterial, Ssu2ServiceError> {
    let hash = bundle
        .identity()
        .hash()
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let material = Ssu2TransportIdentityMaterial::generate(*hash.as_bytes(), &mut OsRng)
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    generate_controlled_identity_with_keys(bundle, host, port, &material)
}

/// Builds controlled SSU2 identity material from persistent transport keys.
///
/// The keys must be bound to the supplied router identity. This is the
/// production daemon path; the ephemeral wrapper above is retained for focused
/// unit tests and controlled fixtures.
pub fn generate_controlled_identity_with_keys(
    bundle: &RouterIdentityBundle,
    host: &str,
    port: u16,
    transport: &Ssu2TransportIdentityMaterial,
) -> Result<Ssu2IdentityMaterial, Ssu2ServiceError> {
    generate_identity_with_keys(bundle, host, port, transport, false)
}

/// Builds a fresh, signed NTCP2 RouterInfo for the explicit loopback profile.
///
/// The persisted static key must already be bound to the supplied RouterIdentity.
/// This RouterInfo is local handshake material only; callers must not publish it.
pub fn generate_controlled_ntcp2_router_info(
    bundle: &RouterIdentityBundle,
    host: std::net::IpAddr,
    port: u16,
    network_id: u16,
    transport: &Ntcp2TransportIdentityMaterial,
) -> Result<Vec<u8>, Ssu2ServiceError> {
    if !host.is_loopback() || port < 1024 {
        return Err(Ssu2ServiceError::ControlledProfile(
            "NTCP2 controlled profile requires a loopback endpoint and fixed port".to_owned(),
        ));
    }
    let network_id = u8::try_from(network_id).map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let hash = bundle
        .identity()
        .hash()
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    if transport.router_hash() != hash.as_bytes() {
        return Err(Ssu2ServiceError::InvalidIdentity);
    }
    let static_key = transport.transport().key();
    let options = Mapping::from_entries(vec![
        ("host".to_owned(), host.to_string()),
        ("port".to_owned(), port.to_string()),
        ("v".to_owned(), "2".to_owned()),
        ("s".to_owned(), i2p_b64_encode(&static_key.public_bytes())),
        ("i".to_owned(), i2p_b64_encode(transport.transport().iv())),
    ])
    .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let address = RouterAddress::new(
        10,
        Date::from_millis(wall_secs().saturating_add(86_400).saturating_mul(1000)),
        "NTCP2".to_owned(),
        options,
    )
    .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let router_options = Mapping::from_entries(vec![
        (
            "router.version".to_owned(),
            CONTROLLED_ROUTER_VERSION.to_owned(),
        ),
        ("netId".to_owned(), network_id.to_string()),
    ])
    .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let info = bundle
        .sign_router_info(
            Date::from_millis(wall_secs().saturating_mul(1000)),
            vec![address],
            Vec::new(),
            router_options,
        )
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let encoded = info
        .encode_to_vec(constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let decoded =
        i2pr_proto::RouterInfo::decode(&encoded, constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
            .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    i2pr_crypto::verify_router_info(&decoded).map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let published_address = decoded
        .addresses()
        .iter()
        .find(|candidate| candidate.transport_style() == "NTCP2")
        .ok_or(Ssu2ServiceError::InvalidIdentity)?;
    if published_address.options().get("host") != Some(host.to_string().as_str())
        || published_address.options().get("port") != Some(port.to_string().as_str())
        || published_address.options().get("v") != Some("2")
    {
        return Err(Ssu2ServiceError::InvalidIdentity);
    }
    Ok(encoded)
}

/// Builds one signed RouterInfo for the explicit dual-transport loopback
/// qualification profile. Both endpoint/key tuples are copied from the live
/// owners' validated configuration and identity-bound stores. No capability
/// letters are added here; the controlled floodfill permit path may later
/// publish a separate `caps=f` snapshot after the live role is active.
pub fn generate_controlled_dual_transport_router_info(
    bundle: &RouterIdentityBundle,
    ssu2_endpoint: SocketAddr,
    ssu2_transport: &Ssu2TransportIdentityMaterial,
    ntcp2_endpoint: SocketAddr,
    ntcp2_transport: &Ntcp2TransportIdentityMaterial,
    network_id: u16,
) -> Result<Vec<u8>, Ssu2ServiceError> {
    if !ssu2_endpoint.ip().is_loopback()
        || !ntcp2_endpoint.ip().is_loopback()
        || ssu2_endpoint.port() < 1024
        || ntcp2_endpoint.port() < 1024
        || ssu2_endpoint.is_ipv4() != ntcp2_endpoint.is_ipv4()
    {
        return Err(Ssu2ServiceError::ControlledProfile(
            "dual-transport qualification requires matching fixed loopback address families"
                .to_owned(),
        ));
    }
    let network_id = u8::try_from(network_id).map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let hash = bundle
        .identity()
        .hash()
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    if ssu2_transport.router_hash() != hash.as_bytes()
        || ntcp2_transport.router_hash() != hash.as_bytes()
    {
        return Err(Ssu2ServiceError::InvalidIdentity);
    }
    let ssu2_options = Mapping::from_entries(vec![
        ("host".to_owned(), ssu2_endpoint.ip().to_string()),
        ("port".to_owned(), ssu2_endpoint.port().to_string()),
        (
            "caps".to_owned(),
            if ssu2_endpoint.is_ipv4() { "4" } else { "6" }.to_owned(),
        ),
        ("mtu".to_owned(), i2pr_runtime::SSU2_DEFAULT_MTU.to_string()),
        ("v".to_owned(), "2".to_owned()),
        (
            "s".to_owned(),
            i2p_b64_encode(&ssu2_transport.static_key().public_bytes()),
        ),
        (
            "i".to_owned(),
            i2p_b64_encode(ssu2_transport.intro_key_bytes()),
        ),
    ])
    .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let ntcp2_options = Mapping::from_entries(vec![
        ("host".to_owned(), ntcp2_endpoint.ip().to_string()),
        ("port".to_owned(), ntcp2_endpoint.port().to_string()),
        ("v".to_owned(), "2".to_owned()),
        (
            "s".to_owned(),
            i2p_b64_encode(&ntcp2_transport.transport().key().public_bytes()),
        ),
        (
            "i".to_owned(),
            i2p_b64_encode(ntcp2_transport.transport().iv()),
        ),
    ])
    .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let expires = Date::from_millis(wall_secs().saturating_add(86_400).saturating_mul(1000));
    let ssu2_address = RouterAddress::new(10, expires, "SSU2".to_owned(), ssu2_options)
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let ntcp2_address = RouterAddress::new(10, expires, "NTCP2".to_owned(), ntcp2_options)
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let router_options = Mapping::from_entries(vec![
        (
            "router.version".to_owned(),
            CONTROLLED_ROUTER_VERSION.to_owned(),
        ),
        ("netId".to_owned(), network_id.to_string()),
    ])
    .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let published = Date::from_millis(wall_secs().saturating_mul(1000));
    let info = bundle
        .sign_router_info(
            published,
            vec![ssu2_address, ntcp2_address],
            Vec::new(),
            router_options,
        )
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let encoded = info
        .encode_to_vec(constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let decoded =
        i2pr_proto::RouterInfo::decode(&encoded, constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
            .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    i2pr_crypto::verify_router_info(&decoded).map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    if decoded.addresses().len() != 2
        || decoded
            .addresses()
            .iter()
            .find(|address| address.transport_style() == "SSU2")
            .and_then(|address| address.options().get("host"))
            != Some(ssu2_endpoint.ip().to_string().as_str())
        || decoded
            .addresses()
            .iter()
            .find(|address| address.transport_style() == "SSU2")
            .and_then(|address| address.options().get("port"))
            != Some(ssu2_endpoint.port().to_string().as_str())
        || decoded
            .addresses()
            .iter()
            .find(|address| address.transport_style() == "NTCP2")
            .and_then(|address| address.options().get("host"))
            != Some(ntcp2_endpoint.ip().to_string().as_str())
        || decoded
            .addresses()
            .iter()
            .find(|address| address.transport_style() == "NTCP2")
            .and_then(|address| address.options().get("port"))
            != Some(ntcp2_endpoint.port().to_string().as_str())
    {
        return Err(Ssu2ServiceError::InvalidIdentity);
    }
    Ok(encoded)
}

pub(crate) fn generate_private_lan_identity_with_keys(
    bundle: &RouterIdentityBundle,
    host: &str,
    port: u16,
    transport: &Ssu2TransportIdentityMaterial,
) -> Result<Ssu2IdentityMaterial, Ssu2ServiceError> {
    generate_identity_with_keys(bundle, host, port, transport, true)
}

fn generate_identity_with_keys(
    bundle: &RouterIdentityBundle,
    host: &str,
    port: u16,
    transport: &Ssu2TransportIdentityMaterial,
    private_lan: bool,
) -> Result<Ssu2IdentityMaterial, Ssu2ServiceError> {
    let host_ip: std::net::IpAddr = host.parse().map_err(|_| {
        Ssu2ServiceError::ControlledProfile("SSU2 identity host must be an IP literal".to_owned())
    })?;
    let allowed_host = host_ip.is_loopback()
        || (private_lan && matches!(host_ip, std::net::IpAddr::V4(ip) if ip.is_private()));
    if !allowed_host {
        return Err(Ssu2ServiceError::ControlledProfile(
            "SSU2 identity host is outside the selected qualification profile".to_owned(),
        ));
    }
    if port == 0 {
        return Err(Ssu2ServiceError::ControlledProfile(
            "SSU2 controlled identity requires a nonzero port".to_owned(),
        ));
    }
    let hash = bundle
        .identity()
        .hash()
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    if transport.router_hash() != hash.as_bytes() {
        return Err(Ssu2ServiceError::InvalidIdentity);
    }
    let static_key = transport.static_key();
    let static_bytes = *static_key.secret_bytes();
    let intro_bytes = *transport.intro_key_bytes();
    if intro_bytes.iter().all(|byte| *byte == 0) {
        return Err(Ssu2ServiceError::InvalidIdentity);
    }
    let intro = IntroKey::new(intro_bytes);
    let options = Mapping::from_entries(vec![
        ("host".to_string(), host_ip.to_string()),
        ("port".to_string(), port.to_string()),
        ("v".to_string(), "2".to_string()),
        ("s".to_string(), i2p_b64_encode(&static_key.public_bytes())),
        ("i".to_string(), i2p_b64_encode(&intro_bytes)),
    ])
    .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let address = RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let ri_options = controlled_router_options().map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let info = bundle
        .sign_router_info(
            Date::from_millis(wall_secs().saturating_mul(1000)),
            vec![address],
            Vec::new(),
            ri_options,
        )
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let router_info = info
        .encode_to_vec(constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    // Re-validate through the standard decoder so a signing defect
    // fails closed before any socket opens.
    i2pr_proto::RouterInfo::decode(&router_info, constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    Ok(Ssu2IdentityMaterial {
        router_hash: hash,
        static_secret_bytes: static_bytes,
        intro_key: intro,
        router_info,
    })
}

/// Verifies one out-of-band reference RouterInfo without importing
/// secrets.
///
/// Only public signed bytes cross this boundary; the caller retains
/// the file path for provenance. The decoded record must carry at
/// least one SSU2 address or the preflight fails closed.
pub fn verify_reference_router_info(
    bytes: &[u8],
) -> Result<(Hash, Ssu2RouterAddress), Ssu2ServiceError> {
    let info =
        i2pr_proto::RouterInfo::decode(bytes, constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
            .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    let hash = info
        .router_identity()
        .hash()
        .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
    for address in info.addresses() {
        if address.transport_style() == "SSU2" {
            let parsed =
                Ssu2RouterAddress::parse(address).map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
            return Ok((hash, parsed));
        }
    }
    Err(Ssu2ServiceError::InvalidIdentity)
}

/// One daemon-owned SSU2 service instance.
///
/// Construction validates the strict profile and maps daemon ceilings
/// to the runtime policy; [`Ssu2DaemonService::start`] binds only the
/// profile-permitted UDP sockets under the caller-owned [`ChildScope`]. The handle is
/// the only runtime owner in counted tests: no hidden standalone
/// runtime coexists with it.
#[derive(Clone, Debug)]
pub struct Ssu2DaemonService {
    service: Ssu2RuntimeService,
}

impl Ssu2DaemonService {
    /// Creates the daemon-owned runtime without opening a socket.
    pub fn new(
        config: &Ssu2Config,
        identity: Ssu2IdentityMaterial,
    ) -> Result<Self, Ssu2ServiceError> {
        Self::new_with_bandwidth(config, identity, None)
    }

    /// Creates the daemon-owned service with a shared process governor.
    pub fn new_with_bandwidth(
        config: &Ssu2Config,
        identity: Ssu2IdentityMaterial,
        bandwidth: Option<i2pr_runtime::BandwidthGovernor>,
    ) -> Result<Self, Ssu2ServiceError> {
        if !config.enabled {
            return Err(Ssu2ServiceError::ControlledProfile(
                "SSU2 service requires enabled = true".to_owned(),
            ));
        }
        if config.advertise || config.introducer_service {
            return Err(Ssu2ServiceError::ControlledProfile(
                "SSU2 controlled profile forbids advertise/introducer".to_owned(),
            ));
        }
        if config.qualification_profile == crate::config::Ssu2QualificationProfile::PrivateLan
            && !private_lan_bind_valid(config)
        {
            return Err(Ssu2ServiceError::ControlledProfile(
                "private-LAN SSU2 requires one RFC1918 IPv4 bind and an explicit non-privileged port"
                    .to_owned(),
            ));
        }
        if config.bind_ipv4.is_none() && config.bind_ipv6.is_none() {
            return Err(Ssu2ServiceError::NoSocket);
        }
        for bind in [config.bind_ipv4, config.bind_ipv6].into_iter().flatten() {
            let allowed = bind.is_loopback()
                || (config.qualification_profile
                    == crate::config::Ssu2QualificationProfile::PrivateLan
                    && matches!(bind, std::net::IpAddr::V4(ip) if ip.is_private()));
            if !allowed {
                return Err(Ssu2ServiceError::ControlledProfile(
                    "SSU2 bind is outside the selected qualification profile".to_owned(),
                ));
            }
        }
        let runtime = ssu2_runtime_config_from(config)?;
        let service = Ssu2RuntimeService::new_with_bandwidth(runtime, identity, bandwidth)
            .map_err(|_| Ssu2ServiceError::InvalidIdentity)?;
        Ok(Self { service })
    }

    /// Returns the owned runtime for dial/send/snapshot calls.
    pub const fn service(&self) -> &Ssu2RuntimeService {
        &self.service
    }

    /// Installs one controlled dual-family record only when its NTCP2
    /// endpoint matches the separately configured live loopback listener.
    pub fn install_controlled_dual_router_info(
        &self,
        encoded: Vec<u8>,
        wall_now_ms: u64,
        ntcp2_endpoint: SocketAddr,
    ) -> Result<(), i2pr_runtime::Ssu2PublicationUnavailable> {
        self.service
            .install_controlled_dual_router_info(encoded, wall_now_ms, ntcp2_endpoint)
    }

    /// Returns the narrow outbound delivery capability.
    pub fn delivery(&self) -> RouterDeliveryService {
        RouterDeliveryService::new(self.service.clone())
    }

    /// Binds profile-permitted sockets and starts supervised loop tasks.
    pub async fn start(
        &self,
        scope: &ChildScope,
        config: &Ssu2Config,
    ) -> Result<Ssu2DaemonHandle, Ssu2ServiceError> {
        let sockets = ssu2_socket_config_from(config)?;
        let handle = self.service.start(scope, sockets).await?;
        Ok(Ssu2DaemonHandle {
            service: self.service.clone(),
            handle,
            delivery: self.delivery(),
        })
    }
}

/// A started daemon-owned SSU2 service: bound addresses plus the
/// bounded authenticated-I2NP handoff and delivery seams.
pub struct Ssu2DaemonHandle {
    service: Ssu2RuntimeService,
    handle: Ssu2ServiceHandle,
    delivery: RouterDeliveryService,
}

impl std::fmt::Debug for Ssu2DaemonHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Ssu2DaemonHandle(..)")
    }
}

impl Ssu2DaemonHandle {
    /// Returns the owning runtime service.
    pub const fn service(&self) -> &Ssu2RuntimeService {
        &self.service
    }

    /// Returns the opt-in private qualification observer, if configured.
    pub fn qualification_observer(&self) -> Option<i2pr_runtime::Ssu2QualificationObserver> {
        self.service.qualification_observer()
    }

    /// Installs one controlled dual-family record only when its NTCP2
    /// endpoint matches the separately configured live loopback listener.
    pub fn install_controlled_dual_router_info(
        &self,
        encoded: Vec<u8>,
        wall_now_ms: u64,
        ntcp2_endpoint: SocketAddr,
    ) -> Result<(), i2pr_runtime::Ssu2PublicationUnavailable> {
        self.service
            .install_controlled_dual_router_info(encoded, wall_now_ms, ntcp2_endpoint)
    }

    /// Returns publication material derived by the runtime from its live bind and reachability
    /// owner. The daemon receives no private transport key bytes.
    pub fn publication_material(
        &self,
        wall_now_ms: u64,
    ) -> Result<i2pr_runtime::Ssu2PublicationMaterial, i2pr_runtime::Ssu2PublicationUnavailable>
    {
        self.service.publication_material(wall_now_ms)
    }

    /// Returns the narrow outbound delivery capability.
    pub const fn delivery(&self) -> &RouterDeliveryService {
        &self.delivery
    }

    /// Returns the bound IPv4 socket address, if any.
    pub const fn local_v4(&self) -> Option<SocketAddr> {
        self.handle.local_v4()
    }

    /// Returns the bound IPv6 socket address, if any.
    pub const fn local_v6(&self) -> Option<SocketAddr> {
        self.handle.local_v6()
    }

    /// Receives the next authenticated inbound I2NP message, or `None`
    /// after service shutdown drains the handoff queue.
    pub async fn next_inbound(&mut self) -> Option<i2pr_runtime::Ssu2InboundI2np> {
        self.handle.next_inbound().await
    }

    /// Receives the next inbound message and dispatches it through
    /// the central router-I2NP dispatcher, preserving authenticated
    /// peer identity. Returns `None` after shutdown drains the queue.
    pub async fn next_dispatched(
        &mut self,
        now_ms: u64,
    ) -> Option<Result<RouterI2npOutcome, RouterI2npError>> {
        let inbound = self.handle.next_inbound().await?;
        Some(dispatch_router_i2np(&inbound, now_ms))
    }

    /// Dials one validated SSU2 target to an authenticated session.
    pub async fn dial(
        &self,
        target: Ssu2DialTarget,
        timeout: Duration,
        cancellation: &CancellationToken,
    ) -> Result<Ssu2EstablishedLink, i2pr_runtime::Ssu2DialOutcome> {
        self.service.dial_ssu2(target, timeout, cancellation).await
    }

    /// Requests service shutdown. Loop tasks drain, release all
    /// handshakes/sessions/manager links, then exit for scope join.
    pub fn shutdown(&self) {
        self.service.shutdown();
    }

    /// Returns the privacy-safe runtime snapshot.
    pub fn snapshot(&self) -> i2pr_runtime::Ssu2Snapshot {
        self.service.snapshot()
    }
}

/// Builds the authenticated dial target for one verified reference
/// peer at its out-of-band loopback endpoint.
///
/// The peer hash must match the RouterInfo-derived hash; the address
/// must be loopback with a nonzero port. Returns the typed target or
/// a controlled-profile rejection without touching a socket.
pub fn daemon_dial_target(
    peer_hash: Hash,
    address: SocketAddr,
    responder_static: Ssu2PublicKey,
    responder_intro: IntroKey,
) -> Result<Ssu2DialTarget, Ssu2ServiceError> {
    let peer = PeerId::from_hash(peer_hash);
    Ssu2DialTarget::new(peer, peer_hash, address, responder_static, responder_intro)
        .map_err(|_| Ssu2ServiceError::ControlledProfile("invalid dial target".to_owned()))
}

/// Constructs a direct SSU2 dial target under the daemon's explicit socket
/// qualification profile. Private-LAN dials remain RFC1918 IPv4-only and are
/// reachable only when the operator selected `private_lan_qualification` in
/// configuration; the default loopback path keeps its existing clamp.
pub fn daemon_dial_target_for_profile(
    peer_hash: Hash,
    address: SocketAddr,
    responder_static: Ssu2PublicKey,
    responder_intro: IntroKey,
    profile: Ssu2QualificationProfile,
) -> Result<Ssu2DialTarget, Ssu2ServiceError> {
    let peer = PeerId::from_hash(peer_hash);
    let target = match profile {
        Ssu2QualificationProfile::ControlledLoopback => {
            Ssu2DialTarget::new(peer, peer_hash, address, responder_static, responder_intro)
        }
        Ssu2QualificationProfile::PrivateLan => Ssu2DialTarget::new_private_lan(
            peer,
            peer_hash,
            address,
            responder_static,
            responder_intro,
        ),
    };
    target.map_err(|_| Ssu2ServiceError::ControlledProfile("invalid dial target".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_proto::{
        DatabaseStoreData, DatabaseStoreMessage, Date, DeferredBuildRecords, DeferredPayload,
        DeliveryStatusMessage, I2npBody, I2npMessage, MessageType, OpaqueMessageBody,
    };
    use i2pr_transport::PeerId;

    const NOW_MS: u64 = 1_700_000_000_000;

    #[test]
    fn daemon_dial_target_requires_explicit_private_lan_profile() {
        let hash = Hash::from_bytes([0x44; 32]);
        let static_key = Ssu2PublicKey::new([0x55; 32]).expect("static key");
        let intro = IntroKey::new([0x66; 32]);
        let private: SocketAddr = "192.168.44.11:19002".parse().expect("address");
        assert!(daemon_dial_target(hash, private, static_key, intro).is_err());
        assert!(
            daemon_dial_target_for_profile(
                hash,
                private,
                static_key,
                intro,
                Ssu2QualificationProfile::ControlledLoopback,
            )
            .is_err()
        );
        assert!(
            daemon_dial_target_for_profile(
                hash,
                private,
                static_key,
                intro,
                Ssu2QualificationProfile::PrivateLan,
            )
            .is_ok()
        );
        let public: SocketAddr = "8.8.8.8:19002".parse().expect("address");
        assert!(
            daemon_dial_target_for_profile(
                hash,
                public,
                static_key,
                intro,
                Ssu2QualificationProfile::PrivateLan,
            )
            .is_err()
        );
    }

    fn test_peer() -> (PeerId, LinkId) {
        (
            PeerId::from_hash(Hash::from_bytes([0x11; 32])),
            LinkId::new(7).expect("link"),
        )
    }

    #[test]
    fn reply_token_probe_binds_signed_store_and_zero_tunnel_to_local_hash() {
        let mut rng = OsRng;
        let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
        let info = bundle
            .sign_router_info(
                Date::from_millis(NOW_MS),
                Vec::new(),
                Vec::new(),
                Mapping::empty(),
            )
            .expect("signed RouterInfo");
        let info_bytes = info
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("RouterInfo bytes");
        let (peer, bytes) = build_reply_token_router_info_probe(&info_bytes, 0x467_0101, NOW_MS)
            .expect("tokened store probe");
        let message = I2npMessage::decode_short_transport(&bytes, MAX_ROUTER_I2NP_BYTES)
            .expect("short-transport store");
        let I2npBody::DatabaseStore(store) = message.body() else {
            panic!("expected DatabaseStore");
        };
        assert_eq!(peer.hash(), store.key);
        assert_eq!(store.reply_token, 0x467_0101);
        assert_eq!(store.reply_tunnel_id, Some(0));
        assert_eq!(store.reply_gateway, Some(store.key));
        let i2pr_proto::DatabaseStoreData::RouterInfoCompressed(payload) = &store.data else {
            panic!("expected signed RouterInfo payload");
        };
        use std::io::Read as _;
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(payload.as_bytes())
            .read_to_end(&mut decoded)
            .expect("gzip RouterInfo");
        assert_eq!(decoded, info_bytes);
    }

    #[test]
    fn reply_token_probe_rejects_reserved_tokens_and_invalid_router_info() {
        let mut rng = OsRng;
        let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
        let info = bundle
            .sign_router_info(
                Date::from_millis(NOW_MS),
                Vec::new(),
                Vec::new(),
                Mapping::empty(),
            )
            .expect("signed RouterInfo");
        let info_bytes = info
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("RouterInfo bytes");
        for token in [0, u32::MAX] {
            assert_eq!(
                build_reply_token_router_info_probe(&info_bytes, token, NOW_MS),
                Err(RouterDeliveryError::InvalidControlledProbe)
            );
        }
        let mut invalid = info_bytes;
        *invalid.last_mut().expect("signature byte") ^= 1;
        assert_eq!(
            build_reply_token_router_info_probe(&invalid, 0x467_0102, NOW_MS),
            Err(RouterDeliveryError::InvalidControlledProbe)
        );
    }

    #[test]
    fn initiator_lookup_probe_uses_verified_local_identity_and_direct_route() {
        let mut rng = OsRng;
        let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
        let info = bundle
            .sign_router_info(
                Date::from_millis(NOW_MS),
                Vec::new(),
                Vec::new(),
                Mapping::empty(),
            )
            .expect("signed RouterInfo");
        let info_bytes = info
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("RouterInfo bytes");
        let key = Hash::from_bytes([0xA7; 32]);
        let bytes = build_initiator_database_lookup_probe(&info_bytes, key, NOW_MS)
            .expect("bounded lookup probe");
        let message = I2npMessage::decode_short_transport(&bytes, MAX_ROUTER_I2NP_BYTES)
            .expect("short-transport lookup");
        let I2npBody::DatabaseLookup(lookup) = message.body() else {
            panic!("expected DatabaseLookup");
        };
        assert_eq!(lookup.key, key);
        assert_eq!(
            lookup.from,
            router_identity_hash(info.router_identity()).expect("local hash")
        );
        assert!(!lookup.delivery_flag);
        assert_eq!(lookup.reply_tunnel_id, None);
        assert_eq!(lookup.lookup_type, 1);
        assert!(lookup.excluded_peers.is_empty());
        assert_eq!(lookup.reply_encryption, i2pr_proto::ReplyEncryption::None);
    }

    #[test]
    fn reply_token_status_rejects_wrong_reserved_and_stale_correlation() {
        assert!(reply_token_status_matches(
            0x467_0103, 0x467_0103, NOW_MS, NOW_MS
        ));
        assert!(!reply_token_status_matches(
            0x467_0103, 0x467_0104, NOW_MS, NOW_MS
        ));
        assert!(!reply_token_status_matches(0, 0, NOW_MS, NOW_MS));
        assert!(!reply_token_status_matches(
            u32::MAX,
            u32::MAX,
            NOW_MS,
            NOW_MS
        ));
        assert!(!reply_token_status_matches(
            0x467_0103,
            0x467_0103,
            NOW_MS - 300_001,
            NOW_MS
        ));
        assert!(!reply_token_status_matches(
            0x467_0103,
            0x467_0103,
            NOW_MS + 300_001,
            NOW_MS
        ));
    }

    #[tokio::test]
    async fn ntcp2_delivery_registry_is_peer_bound_generation_safe_and_bounded() {
        let registry = Ntcp2RouterDeliveryRegistry::default();
        let peer = PeerId::from_hash(Hash::from_bytes([0x33; 32]));
        let active = LinkId::new(11).expect("active link");
        let stale = LinkId::new(12).expect("stale link");
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        assert!(registry.register(peer, active, sender));
        assert!(registry.is_current(peer, active));
        assert!(!registry.is_current(peer, stale));
        let lookup_key = Hash::from_bytes([0x61; 32]);
        assert!(registry.register_lookup(peer, active, lookup_key, NOW_MS, NOW_MS + 10_000, 1,));
        assert!(!registry.correlate_lookup_response(peer, stale, lookup_key, NOW_MS + 1,));
        assert!(!registry.correlate_lookup_response(
            peer,
            active,
            Hash::from_bytes([0x62; 32]),
            NOW_MS + 1,
        ));
        assert!(registry.correlate_lookup_response(peer, active, lookup_key, NOW_MS + 1,));
        assert!(!registry.correlate_lookup_response(peer, active, lookup_key, NOW_MS + 2,));
        assert!(registry.register_lookup(peer, active, lookup_key, NOW_MS, NOW_MS + 10_000, 1,));
        assert!(!registry.correlate_lookup_response(peer, active, lookup_key, NOW_MS + 10_000,));
        assert!(registry.register_lookup(peer, active, lookup_key, NOW_MS, NOW_MS + 10_000, 1,));
        let (duplicate_sender, mut duplicate_receiver) = tokio::sync::mpsc::channel(1);
        assert!(registry.register(peer, stale, duplicate_sender));
        assert!(registry.is_current(peer, stale));
        assert_eq!(
            registry.deliver_with_link(peer, vec![0]),
            (RouterDeliveryOutcome::Accepted, Some(stale)),
            "the newest authenticated generation is selected deterministically"
        );
        assert_eq!(duplicate_receiver.recv().await.unwrap().bytes, vec![0]);
        registry.remove(peer, stale);
        assert!(registry.is_current(peer, active));

        assert_eq!(
            registry.deliver_with_link(peer, vec![1]).0,
            RouterDeliveryOutcome::Accepted
        );
        assert_eq!(
            registry.deliver_with_link(peer, vec![2]).0,
            RouterDeliveryOutcome::QueueFull
        );
        assert_eq!(receiver.recv().await.unwrap().bytes, vec![1]);

        assert_eq!(
            registry.deliver_with_link(peer, vec![3]).0,
            RouterDeliveryOutcome::Accepted,
            "a stale close must not remove a newer authenticated generation"
        );
        registry.remove(peer, active);
        assert!(!registry.is_current(peer, active));
        assert!(!registry.correlate_lookup_response(peer, active, lookup_key, NOW_MS + 3,));
        assert_eq!(
            registry.deliver_with_link(peer, vec![4]).0,
            RouterDeliveryOutcome::NoActiveSession
        );
    }

    #[tokio::test]
    async fn router_delivery_falls_back_to_the_authenticated_ntcp2_peer_queue() {
        let peer = PeerId::from_hash(Hash::from_bytes([0x44; 32]));
        let link_id = LinkId::new(21).expect("link id");
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        let registry = Ntcp2RouterDeliveryRegistry::default();
        assert!(registry.register(peer, link_id, sender));
        let bundle =
            i2pr_crypto::RouterIdentityBundle::generate(&mut OsRng).expect("router identity");
        let identity = generate_controlled_identity(&bundle, "127.0.0.1", 44_321)
            .expect("controlled SSU2 identity");
        let runtime = Ssu2RuntimeService::new(Ssu2RuntimeConfig::default(), identity)
            .expect("runtime-neutral SSU2 service");
        let delivery = RouterDeliveryService::new(runtime);
        assert!(delivery.attach_ntcp2_registry(registry));
        let bytes = delivery_status_short(7, (NOW_MS / 1000) as u32 + 60);
        let request = RouterDeliveryRequest::new(peer, bytes.clone(), Duration::from_secs(2))
            .expect("bounded router delivery request");

        assert_eq!(
            delivery.deliver_with_transport(request, &CancellationToken::new()),
            (
                RouterDeliveryOutcome::Accepted,
                Some(RouterDeliveryTransport::Ntcp2(link_id)),
            )
        );
        assert_eq!(receiver.recv().await.unwrap().bytes, bytes);
    }

    #[tokio::test]
    async fn router_delivery_miss_requests_a_bounded_ntcp2_dial() {
        let peer = PeerId::from_hash(Hash::from_bytes([0x4a; 32]));
        let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("router identity");
        let identity = generate_controlled_identity(&bundle, "127.0.0.1", 44_322)
            .expect("controlled SSU2 identity");
        let runtime = Ssu2RuntimeService::new(Ssu2RuntimeConfig::default(), identity)
            .expect("runtime-neutral SSU2 service");
        let registry = Ntcp2RouterDeliveryRegistry::default();
        let (dial_sender, mut dial_receiver) = tokio::sync::mpsc::channel(1);
        assert!(registry.attach_dial_request_sender(dial_sender));
        let delivery = RouterDeliveryService::new(runtime);
        assert!(delivery.attach_ntcp2_registry(registry.clone()));
        let cancellation = CancellationToken::new();
        let send = || {
            RouterDeliveryRequest::new(
                peer,
                delivery_status_short(0x461_2001, (NOW_MS / 1000) as u32 + 60),
                Duration::from_secs(2),
            )
            .expect("bounded request")
        };

        assert_eq!(
            delivery.deliver_with_transport(send(), &cancellation),
            (RouterDeliveryOutcome::NoActiveSession, None),
            "the caller still observes that its first request has not been sent"
        );
        assert_eq!(dial_receiver.recv().await, Some(peer));

        let link_id = LinkId::new(22).expect("link generation");
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        assert!(registry.register(peer, link_id, sender));
        assert_eq!(
            delivery.deliver_with_transport(send(), &cancellation),
            (
                RouterDeliveryOutcome::Accepted,
                Some(RouterDeliveryTransport::Ntcp2(link_id)),
            ),
            "a subsequent product request uses the newly admitted route"
        );
        assert!(dial_receiver.try_recv().is_err());
        assert!(receiver.recv().await.is_some());
    }

    #[tokio::test]
    async fn router_delivery_survives_each_single_family_route_loss() {
        let text = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n";
        let config = crate::config::Config::parse(text)
            .expect("controlled config")
            .ssu2;
        let bundle_a = RouterIdentityBundle::generate(&mut OsRng).expect("identity A");
        let bundle_b = RouterIdentityBundle::generate(&mut OsRng).expect("identity B");
        let identity_a =
            generate_controlled_identity(&bundle_a, "127.0.0.1", 44_011).expect("SSU2 identity A");
        let identity_b =
            generate_controlled_identity(&bundle_b, "127.0.0.1", 44_012).expect("SSU2 identity B");
        let peer_b = PeerId::from_hash(identity_b.router_hash);
        let router_hash_b = identity_b.router_hash;
        let static_secret_b =
            i2pr_crypto::X25519PrivateKey::from_bytes(identity_b.static_secret_bytes);
        let static_public_b = i2pr_runtime::Ssu2PublicKey::new(static_secret_b.public_bytes())
            .expect("SSU2 static public key");
        let intro_b = identity_b.intro_key;
        let token_a = CancellationToken::new();
        let token_b = CancellationToken::new();
        let scope_a = ChildScope::for_test(&token_a, i2pr_runtime::ChildFailurePolicy::FailParent);
        let scope_b = ChildScope::for_test(&token_b, i2pr_runtime::ChildFailurePolicy::FailParent);
        let service_a = Ssu2DaemonService::new(&config, identity_a).expect("service A");
        let service_b = Ssu2DaemonService::new(&config, identity_b).expect("service B");
        let handle_a = service_a
            .start(
                &scope_a,
                &crate::config::Ssu2Config {
                    port: 0,
                    ..config.clone()
                },
            )
            .await
            .expect("bind A");
        let mut handle_b = service_b
            .start(
                &scope_b,
                &crate::config::Ssu2Config {
                    port: 0,
                    ..config.clone()
                },
            )
            .await
            .expect("bind B");
        let target = i2pr_runtime::Ssu2DialTarget::new(
            peer_b,
            router_hash_b,
            handle_b.local_v4().expect("B bind"),
            static_public_b,
            intro_b,
        )
        .expect("validated loopback target");
        handle_a
            .dial(target, Duration::from_secs(5), &token_a)
            .await
            .expect("SSU2 session establishes");

        let ntcp2 = Ntcp2RouterDeliveryRegistry::default();
        let ntcp2_link = LinkId::new(90).expect("NTCP2 generation");
        let (ntcp2_sender, mut ntcp2_receiver) = tokio::sync::mpsc::channel(2);
        assert!(ntcp2.register(peer_b, ntcp2_link, ntcp2_sender));
        let delivery = handle_a.delivery().clone();
        assert!(delivery.attach_ntcp2_registry(ntcp2.clone()));
        let cancellation = CancellationToken::new();

        let send = |message_id| {
            let bytes = delivery_status_short(message_id, (NOW_MS / 1000) as u32 + 60);
            RouterDeliveryRequest::new(peer_b, bytes, Duration::from_secs(2))
                .expect("bounded request")
        };
        for message_id in [0x461_0001, 0x461_0002] {
            let (outcome, selected) =
                delivery.deliver_with_transport(send(message_id), &cancellation);
            assert_eq!(outcome, RouterDeliveryOutcome::Accepted);
            assert!(matches!(selected, Some(RouterDeliveryTransport::Ssu2(_))));
            let inbound = tokio::time::timeout(Duration::from_secs(5), handle_b.next_inbound())
                .await
                .expect("SSU2 inbound deadline")
                .expect("SSU2 inbound message");
            assert_eq!(inbound.bytes, send(message_id).message.as_bytes());
            assert!(ntcp2_receiver.try_recv().is_err());
            if message_id == 0x461_0001 {
                ntcp2.remove(peer_b, ntcp2_link);
            }
        }

        handle_a.shutdown();
        scope_a.shutdown().await;
        let (replacement_sender, _replacement_receiver) = tokio::sync::mpsc::channel(2);
        assert!(ntcp2.register(peer_b, ntcp2_link, replacement_sender));
        assert_eq!(
            delivery.deliver_with_transport(send(0x461_0003), &cancellation),
            (
                RouterDeliveryOutcome::Accepted,
                Some(RouterDeliveryTransport::Ntcp2(ntcp2_link)),
            ),
            "NTCP2 remains usable after the SSU2 owner is stopped"
        );
        assert!(ntcp2_receiver.try_recv().is_err());

        handle_b.shutdown();
        scope_b.shutdown().await;
        token_a.cancel(i2pr_runtime::CancellationReason::OperatorRequest);
        token_b.cancel(i2pr_runtime::CancellationReason::OperatorRequest);
    }

    #[tokio::test]
    async fn router_delivery_uses_real_ntcp2_link_after_ssu2_owner_stops() {
        use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
        use i2pr_runtime::{Ntcp2ResponderIdentity, Ntcp2RuntimeConfig, Ntcp2RuntimeService};
        use i2pr_storage::{IdentityStore, Ntcp2TransportIdentityStore};
        use std::net::IpAddr;

        let ssu2_config = crate::config::Config::parse(
            "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n",
        )
        .expect("controlled config")
        .ssu2;
        let bundle_a = RouterIdentityBundle::generate(&mut OsRng).expect("identity A");
        let bundle_b = RouterIdentityBundle::generate(&mut OsRng).expect("identity B");
        let hash_a = *bundle_a.identity().hash().expect("hash A").as_bytes();
        let hash_b = *bundle_b.identity().hash().expect("hash B").as_bytes();
        let temp = tempfile::tempdir().expect("temporary identity root");
        let data_root = temp.path().join("router-data");
        IdentityStore::prepare_directory(&data_root).expect("prepare identity root");
        let data_dir_a = data_root.join("a");
        let data_dir_b = data_root.join("b");
        let ntcp_a = Ntcp2TransportIdentityStore::in_data_dir(&data_dir_a)
            .load_or_generate(hash_a, &mut OsRng)
            .expect("NTCP2 material A");
        let ntcp_b = Ntcp2TransportIdentityStore::in_data_dir(&data_dir_b)
            .load_or_generate(hash_b, &mut OsRng)
            .expect("NTCP2 material B");
        let ntcp_runtime_a =
            Ntcp2RuntimeService::new(Ntcp2RuntimeConfig::default()).expect("NTCP2 runtime A");
        let ntcp_runtime_b =
            Ntcp2RuntimeService::new(Ntcp2RuntimeConfig::default()).expect("NTCP2 runtime B");
        let token_ntcp_a = CancellationToken::new();
        let token_ntcp_b = CancellationToken::new();
        let scope_ntcp_a = ntcp_runtime_a.child_scope(&token_ntcp_a);
        let scope_ntcp_b = ntcp_runtime_b.child_scope(&token_ntcp_b);
        let mut listener = ntcp_runtime_b
            .listen("127.0.0.1:0".parse().expect("loopback bind"), &scope_ntcp_b)
            .await
            .expect("NTCP2 listener");
        let endpoint_b = listener.local_addr();
        let router_info_a = generate_controlled_ntcp2_router_info(
            &bundle_a,
            IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            44_001,
            2,
            &ntcp_a,
        )
        .expect("signed RouterInfo A");
        let router_info_b = generate_controlled_ntcp2_router_info(
            &bundle_b,
            endpoint_b.ip(),
            endpoint_b.port(),
            2,
            &ntcp_b,
        )
        .expect("signed RouterInfo B");
        let parsed_router_info_b =
            i2pr_proto::RouterInfo::decode(&router_info_b, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
                .expect("decode signed RouterInfo B");
        let responder_hash = bundle_b.identity().hash().expect("responder hash");
        let responder_static = *ntcp_b.transport().key().secret_bytes();
        let responder_iv = *ntcp_b.transport().iv();
        let network_id = 2_u8;
        let accept = async {
            let inbound = listener.next().await.expect("accepted NTCP2 stream");
            let (inbound, handshake) = ntcp_runtime_b
                .authenticate_inbound(
                    inbound,
                    Ntcp2ResponderIdentity::new(
                        &responder_static,
                        *responder_hash.as_bytes(),
                        responder_iv,
                        network_id,
                        &router_info_b,
                    ),
                    &token_ntcp_b,
                )
                .await
                .expect("authenticate NTCP2 initiator");
            ntcp_runtime_b
                .promote_authenticated_inbound(&scope_ntcp_b, inbound, handshake, 92)
                .expect("promote NTCP2 inbound")
        };
        let connect = ntcp_runtime_a.dial_authenticated_loopback_peer(
            &scope_ntcp_a,
            &parsed_router_info_b,
            hash_b,
            X25519PrivateKey::from_bytes(*ntcp_a.transport().key().secret_bytes()),
            &router_info_a,
            network_id,
            91,
            &token_ntcp_a,
        );
        let (outbound, inbound) = tokio::join!(connect, accept);
        let outbound = outbound.expect("authenticated NTCP2 outbound link");

        let ssu_identity_a =
            generate_controlled_identity(&bundle_a, "127.0.0.1", 44_011).expect("SSU2 A");
        let ssu_identity_b =
            generate_controlled_identity(&bundle_b, "127.0.0.1", 44_012).expect("SSU2 B");
        let ssu_static_b = X25519PrivateKey::from_bytes(ssu_identity_b.static_secret_bytes);
        let ssu_public_b =
            Ssu2PublicKey::new(ssu_static_b.public_bytes()).expect("SSU2 static public key");
        let ssu_intro_b = ssu_identity_b.intro_key;
        let ssu_service_a =
            Ssu2DaemonService::new(&ssu2_config, ssu_identity_a).expect("SSU2 service A");
        let ssu_service_b =
            Ssu2DaemonService::new(&ssu2_config, ssu_identity_b).expect("SSU2 service B");
        let token_ssu_a = CancellationToken::new();
        let token_ssu_b = CancellationToken::new();
        let scope_ssu_a = i2pr_runtime::ChildScope::for_test(
            &token_ssu_a,
            i2pr_runtime::ChildFailurePolicy::FailParent,
        );
        let scope_ssu_b = i2pr_runtime::ChildScope::for_test(
            &token_ssu_b,
            i2pr_runtime::ChildFailurePolicy::FailParent,
        );
        let handle_a = ssu_service_a
            .start(
                &scope_ssu_a,
                &crate::config::Ssu2Config {
                    port: 0,
                    ..ssu2_config.clone()
                },
            )
            .await
            .expect("bind SSU2 A");
        let mut handle_b = ssu_service_b
            .start(
                &scope_ssu_b,
                &crate::config::Ssu2Config {
                    port: 0,
                    ..ssu2_config.clone()
                },
            )
            .await
            .expect("bind SSU2 B");
        let peer_b = PeerId::from_hash(Hash::from_bytes(hash_b));
        let peer_a = PeerId::from_hash(Hash::from_bytes(hash_a));
        let target = i2pr_runtime::Ssu2DialTarget::new(
            peer_b,
            Hash::from_bytes(hash_b),
            handle_b.local_v4().expect("SSU2 B endpoint"),
            ssu_public_b,
            ssu_intro_b,
        )
        .expect("validated SSU2 target");
        handle_a
            .dial(target, Duration::from_secs(5), &token_ssu_a)
            .await
            .expect("SSU2 session establishes");

        let registry = Ntcp2RouterDeliveryRegistry::default();
        let link_id = LinkId::new(91).expect("NTCP2 generation");
        let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
        assert!(registry.register(peer_b, link_id, sender.clone()));
        let delivery = handle_a.delivery().clone();
        assert!(delivery.attach_ntcp2_registry(registry.clone()));
        let cancellation = CancellationToken::new();
        let send = |message_id| {
            RouterDeliveryRequest::new(
                peer_b,
                delivery_status_short(message_id, (NOW_MS / 1000) as u32 + 60),
                Duration::from_secs(2),
            )
            .expect("bounded delivery")
        };

        let (outcome, selected) = delivery.deliver_with_transport(send(0x461_1001), &cancellation);
        assert_eq!(outcome, RouterDeliveryOutcome::Accepted);
        assert!(matches!(selected, Some(RouterDeliveryTransport::Ssu2(_))));
        let ssu_inbound = tokio::time::timeout(Duration::from_secs(5), handle_b.next_inbound())
            .await
            .expect("SSU2 receive deadline")
            .expect("SSU2 I2NP message");
        assert!(receiver.try_recv().is_err());

        registry.remove(peer_b, link_id);
        outbound.close();
        inbound.close();
        let (outcome, selected) = delivery.deliver_with_transport(send(0x461_1002), &cancellation);
        assert_eq!(outcome, RouterDeliveryOutcome::Accepted);
        assert!(matches!(selected, Some(RouterDeliveryTransport::Ssu2(_))));
        let ssu_inbound_after_ntcp2_loss =
            tokio::time::timeout(Duration::from_secs(5), handle_b.next_inbound())
                .await
                .expect("SSU2 receive deadline after NTCP2 loss")
                .expect("SSU2 I2NP after NTCP2 loss");
        assert_eq!(ssu_inbound_after_ntcp2_loss.peer, peer_a);

        let reconnect_accept = async {
            let inbound = listener
                .next()
                .await
                .expect("accepted replacement NTCP2 stream");
            let (inbound, handshake) = ntcp_runtime_b
                .authenticate_inbound(
                    inbound,
                    Ntcp2ResponderIdentity::new(
                        &responder_static,
                        *responder_hash.as_bytes(),
                        responder_iv,
                        network_id,
                        &router_info_b,
                    ),
                    &token_ntcp_b,
                )
                .await
                .expect("authenticate replacement NTCP2 initiator");
            ntcp_runtime_b
                .promote_authenticated_inbound(&scope_ntcp_b, inbound, handshake, 94)
                .expect("promote replacement NTCP2 inbound")
        };
        let reconnect = ntcp_runtime_a.dial_authenticated_loopback_peer(
            &scope_ntcp_a,
            &parsed_router_info_b,
            hash_b,
            X25519PrivateKey::from_bytes(*ntcp_a.transport().key().secret_bytes()),
            &router_info_a,
            network_id,
            93,
            &token_ntcp_a,
        );
        let (outbound_replacement, inbound_replacement) = tokio::join!(reconnect, reconnect_accept);
        let outbound_replacement =
            outbound_replacement.expect("replacement authenticated NTCP2 outbound link");
        let mut inbound_replacement = inbound_replacement;
        let replacement_link_id = LinkId::new(93).expect("replacement link generation");
        let (replacement_sender, mut replacement_receiver) = tokio::sync::mpsc::channel(2);
        assert!(registry.register(peer_b, replacement_link_id, replacement_sender));

        handle_a.shutdown();
        scope_ssu_a.shutdown().await;
        assert_eq!(
            delivery.deliver_with_transport(send(0x461_1003), &cancellation),
            (
                RouterDeliveryOutcome::Accepted,
                Some(RouterDeliveryTransport::Ntcp2(replacement_link_id))
            ),
            "the replacement NTCP2 route survives SSU2 owner shutdown"
        );
        let bytes = replacement_receiver
            .recv()
            .await
            .expect("replacement NTCP2 route queue")
            .bytes;
        outbound_replacement
            .send_i2np_message(bytes, Duration::from_secs(2), &cancellation)
            .await
            .expect("send over replacement authenticated NTCP2 link");
        let ntcp_inbound = tokio::time::timeout(
            Duration::from_secs(5),
            inbound_replacement.recv_i2np_messages(&token_ntcp_b),
        )
        .await
        .expect("NTCP2 receive deadline")
        .expect("NTCP2 receive succeeds")
        .expect("NTCP2 I2NP frame");
        assert_eq!(
            ntcp_inbound,
            vec![send(0x461_1003).message.as_bytes().to_vec()]
        );
        assert_eq!(ssu_inbound.peer, peer_a);

        outbound_replacement.close();
        inbound_replacement.close();
        handle_b.shutdown();
        scope_ssu_b.shutdown().await;
        token_ntcp_a.cancel(i2pr_runtime::CancellationReason::OperatorRequest);
        token_ntcp_b.cancel(i2pr_runtime::CancellationReason::OperatorRequest);
        scope_ntcp_a.shutdown().await;
        scope_ntcp_b.shutdown().await;
        token_ssu_a.cancel(i2pr_runtime::CancellationReason::OperatorRequest);
        token_ssu_b.cancel(i2pr_runtime::CancellationReason::OperatorRequest);
    }

    fn inbound_with(bytes: Vec<u8>) -> i2pr_runtime::Ssu2InboundI2np {
        let (peer, link_id) = test_peer();
        i2pr_runtime::Ssu2InboundI2np {
            link_id,
            peer,
            bytes,
        }
    }

    fn delivery_status_short(message_id: u32, expiration_secs: u32) -> Vec<u8> {
        let body = I2npBody::DeliveryStatus(DeliveryStatusMessage::new(
            0x51A4_2001,
            Date::from_millis(NOW_MS),
        ));
        I2npMessage::new_short_transport(message_id, expiration_secs, body)
            .expect("message")
            .encode_short_transport_to_vec(MAX_ROUTER_I2NP_BYTES)
            .expect("encode")
    }

    fn delivery_status_standard(message_id: u32, expiration_ms: u64) -> Vec<u8> {
        let body = I2npBody::DeliveryStatus(DeliveryStatusMessage::new(
            0x51A4_2001,
            Date::from_millis(NOW_MS),
        ));
        I2npMessage::new_standard(message_id, Date::from_millis(expiration_ms), body)
            .expect("message")
            .encode_standard_to_vec(MAX_ROUTER_I2NP_BYTES)
            .expect("encode")
    }

    fn now_secs() -> u64 {
        NOW_MS / 1000
    }

    #[test]
    fn dispatcher_classifies_standard_vs_short_delivery_status() {
        let short_exp = now_secs() as u32 + 60;
        let short_bytes = delivery_status_short(0xA1, short_exp);
        let outcome =
            dispatch_router_i2np(&inbound_with(short_bytes), NOW_MS).expect("short dispatches");
        assert!(matches!(
            outcome,
            RouterI2npOutcome::RouterControl {
                kind: RouterI2npKind::DeliveryStatus,
                header: RouterI2npHeaderKind::ShortTransport,
                ..
            }
        ));

        let standard_bytes = delivery_status_standard(0xA2, NOW_MS + 60_000);
        let outcome = dispatch_router_i2np(&inbound_with(standard_bytes), NOW_MS)
            .expect("standard dispatches");
        assert!(matches!(
            outcome,
            RouterI2npOutcome::RouterControl {
                kind: RouterI2npKind::DatabaseStore | RouterI2npKind::DeliveryStatus,
                header: RouterI2npHeaderKind::Standard,
                ..
            }
        ));
        assert!(matches!(
            outcome,
            RouterI2npOutcome::RouterControl {
                kind: RouterI2npKind::DeliveryStatus,
                ..
            }
        ));
    }

    #[test]
    fn dispatcher_preserves_authenticated_peer_identity() {
        let short_exp = now_secs() as u32 + 60;
        let bytes = delivery_status_short(0xB1, short_exp);
        let (peer, link) = test_peer();
        let outcome = dispatch_router_i2np(&inbound_with(bytes), NOW_MS).expect("dispatch");
        assert_eq!(outcome.peer(), peer);
        assert_eq!(outcome.link_id(), link);
    }

    #[test]
    fn dispatcher_rejects_expired_and_far_future() {
        // Expired: standard expiration at now.
        let expired = delivery_status_standard(0xC1, NOW_MS);
        assert_eq!(
            dispatch_router_i2np(&inbound_with(expired), NOW_MS),
            Err(RouterI2npError::Expired)
        );
        // Far future: standard expiration beyond the horizon.
        let far = delivery_status_standard(0xC2, NOW_MS + MAX_ROUTER_I2NP_FUTURE_MS + 1000);
        assert_eq!(
            dispatch_router_i2np(&inbound_with(far), NOW_MS),
            Err(RouterI2npError::FarFuture)
        );
        // Expired short-transport (seconds in the past).
        let past_secs = now_secs() as u32 - 10;
        let past = delivery_status_short(0xC3, past_secs);
        assert_eq!(
            dispatch_router_i2np(&inbound_with(past), NOW_MS),
            Err(RouterI2npError::Expired)
        );
    }

    #[test]
    fn dispatcher_rejects_oversized_and_empty() {
        assert_eq!(
            dispatch_router_i2np(&inbound_with(Vec::new()), NOW_MS),
            Err(RouterI2npError::Empty)
        );
        let oversized = vec![0x0A; MAX_ROUTER_I2NP_BYTES + 1];
        assert!(matches!(
            dispatch_router_i2np(&inbound_with(oversized), NOW_MS),
            Err(RouterI2npError::TooLarge { .. })
        ));
    }

    #[test]
    fn dispatcher_reports_unknown_and_unsupported_boundedly() {
        // Unknown type byte 99 never enters a decoder.
        let unknown = vec![99, 0, 1, 2, 3];
        let outcome =
            dispatch_router_i2np(&inbound_with(unknown.clone()), NOW_MS).expect("unsupported");
        assert!(matches!(
            outcome,
            RouterI2npOutcome::Unsupported { type_byte: 99, .. }
        ));
        assert_eq!(outcome.encoded_len(), unknown.len());

        // Known-but-unsupported Garlic body returns Unsupported, never an error.
        let garlic = I2npBody::Garlic(OpaqueMessageBody {
            payload: i2pr_proto::DeferredPayload::new(vec![1, 2, 3], 1024).expect("payload"),
        });
        let message = I2npMessage::new_standard(0xD1, Date::from_millis(NOW_MS + 60_000), garlic)
            .expect("garlic message");
        let bytes = message
            .encode_standard_to_vec(MAX_ROUTER_I2NP_BYTES)
            .expect("encode garlic");
        let outcome = dispatch_router_i2np(&inbound_with(bytes), NOW_MS).expect("garlic");
        assert!(matches!(
            outcome,
            RouterI2npOutcome::Unsupported { type_byte: 11, .. }
        ));
    }

    #[test]
    fn dispatcher_routes_database_store_and_tunnel_data() {
        // DatabaseStore control hook (short-transport, small RouterInfo-shaped payload is
        // unnecessary here: use a Deferred empty RouterInfo payload via the codec).
        let gzip: Vec<u8> = vec![0x1F, 0x8B, 0x08, 0x00, 0, 0, 0, 0, 0, 0xFF, 3, 0];
        let payload = DeferredPayload::new(gzip, usize::from(u16::MAX)).expect("deferred payload");
        let store = DatabaseStoreMessage {
            key: Hash::from_bytes([0x22; 32]),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::RouterInfoCompressed(payload),
        };
        let body = I2npBody::DatabaseStore(Box::new(store));
        let short_exp = now_secs() as u32 + 60;
        let message = I2npMessage::new_short_transport(0xE1, short_exp, body).expect("store");
        let bytes = message
            .encode_short_transport_to_vec(MAX_ROUTER_I2NP_BYTES)
            .expect("encode store");
        let outcome = dispatch_router_i2np(&inbound_with(bytes), NOW_MS).expect("store");
        assert!(matches!(
            outcome,
            RouterI2npOutcome::RouterControl {
                kind: RouterI2npKind::DatabaseStore,
                ..
            }
        ));

        // TunnelData hook preserves the tunnel id.
        let cell = i2pr_proto::TunnelDataMessage {
            tunnel_id: 0x1234_5678,
            data: [0xAB; 1024],
        };
        let body = I2npBody::TunnelData(Box::new(cell));
        let message = I2npMessage::new_standard(0xE2, Date::from_millis(NOW_MS + 60_000), body)
            .expect("tunnel data");
        let bytes = message
            .encode_standard_to_vec(MAX_ROUTER_I2NP_BYTES)
            .expect("encode cell");
        let outcome = dispatch_router_i2np(&inbound_with(bytes), NOW_MS).expect("cell");
        assert!(matches!(
            outcome,
            RouterI2npOutcome::TunnelData {
                tunnel_id: 0x1234_5678,
                ..
            }
        ));
    }

    #[test]
    fn garlic_handoff_carries_bounded_opaque() {
        // A Garlic message keeps the Unsupported outcome but the
        // transit handoff carries the bounded opaque payload so
        // the service can attempt a router-garlic unwrap.
        let garlic = I2npBody::Garlic(OpaqueMessageBody {
            payload: DeferredPayload::new(vec![0xCC_u8; 64], MAX_ROUTER_I2NP_BYTES)
                .expect("payload"),
        });
        let message = I2npMessage::new_standard(0xD2, Date::from_millis(NOW_MS + 60_000), garlic)
            .expect("garlic message");
        let bytes = message
            .encode_standard_to_vec(MAX_ROUTER_I2NP_BYTES)
            .expect("encode garlic");
        let inbound = inbound_with(bytes);
        let (outcome, bodies) =
            dispatch_router_i2np_with_transit_bodies(&inbound, NOW_MS).expect("handoff");
        assert!(matches!(outcome, RouterI2npOutcome::Unsupported { .. }));
        assert_eq!(
            bodies.garlic_opaque,
            Some(vec![0xCC_u8; 64]),
            "garlic opaque must travel the handoff verbatim"
        );
        assert!(bodies.short_build_body.is_none());
    }

    #[test]
    fn delivery_request_validates_bounds() {
        let peer = PeerId::from_hash(Hash::from_bytes([0x33; 32]));
        assert!(matches!(
            RouterDeliveryRequest::new(peer, vec![1, 2, 3], Duration::from_secs(0)),
            Err(RouterDeliveryError::ZeroTimeout)
        ));
        assert!(matches!(
            RouterDeliveryRequest::new(
                peer,
                vec![1, 2, 3],
                MAX_ROUTER_DELIVERY_TIMEOUT + Duration::from_secs(1)
            ),
            Err(RouterDeliveryError::TimeoutTooLong)
        ));
        assert!(matches!(
            RouterDeliveryRequest::new(peer, Vec::new(), Duration::from_secs(1)),
            Err(RouterDeliveryError::MessageTooLarge)
        ));
        let oversized = vec![0_u8; MAX_I2NP_MESSAGE_BYTES + 1];
        assert!(matches!(
            RouterDeliveryRequest::new(peer, oversized, Duration::from_secs(1)),
            Err(RouterDeliveryError::MessageTooLarge)
        ));
    }

    #[test]
    fn runtime_config_mapping_preserves_daemon_ceilings() {
        let text = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n";
        let config = crate::config::Config::parse(text).expect("strict profile");
        let runtime = ssu2_runtime_config_from(&config.ssu2).expect("mapping");
        assert_eq!(
            runtime.limits.max_pending_handshakes,
            config.ssu2.max_pending_handshakes
        );
        assert_eq!(
            runtime.limits.max_active_sessions,
            config.ssu2.max_active_sessions
        );
        assert_eq!(
            runtime.limits.max_outbound_datagrams,
            config.ssu2.max_datagram_queue_items
        );
        assert_eq!(runtime.deadlines.handshake, config.ssu2.handshake_timeout);
        assert!(runtime.deadlines.dial >= runtime.deadlines.handshake);
        runtime.validate().expect("runtime validates");
    }

    #[test]
    fn socket_config_rejects_disabled_and_broad_profiles() {
        let text = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n";
        let config = crate::config::Config::parse(text).expect("defaults");
        assert!(matches!(
            ssu2_socket_config_from(&config.ssu2),
            Err(Ssu2ServiceError::ControlledProfile(_))
        ));
        let text = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nbind_ipv4 = \"\"\nbind_ipv6 = \"::1\"\n";
        let config = crate::config::Config::parse(text).expect("ipv6 profile");
        let sockets = ssu2_socket_config_from(&config.ssu2).expect("ipv6 sockets");
        assert!(sockets.ipv4.is_none());
        assert!(sockets.ipv6.is_some());

        let text = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nqualification_profile = \"private_lan_qualification\"\nbind_ipv4 = \"192.168.10.25\"\nbind_ipv6 = \"\"\nport = 44123\n";
        let config = crate::config::Config::parse(text).expect("private-LAN profile");
        let sockets = ssu2_socket_config_from(&config.ssu2).expect("private IPv4 socket");
        assert_eq!(
            sockets.ipv4.expect("v4"),
            "192.168.10.25:44123".parse().unwrap()
        );
        assert!(sockets.ipv6.is_none());
    }

    #[test]
    fn controlled_identity_rejects_non_loopback_and_zero_port() {
        use i2pr_crypto::RouterIdentityBundle;
        use rand_core::OsRng;
        let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
        assert!(generate_controlled_identity(&bundle, "192.0.2.1", 44001).is_err());
        assert!(generate_controlled_identity(&bundle, "127.0.0.1", 0).is_err());
        let material =
            generate_controlled_identity(&bundle, "127.0.0.1", 44001).expect("controlled");
        assert!(!material.router_info.is_empty());
    }

    #[test]
    fn ntcp2_router_info_is_fresh_signed_loopback_and_identity_bound() {
        use i2pr_crypto::RouterIdentityBundle;
        use i2pr_storage::{IdentityStore, Ntcp2TransportIdentityStore};
        use rand_core::OsRng;
        use tempfile::tempdir;

        let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
        let router_hash = *bundle.identity().hash().expect("router hash").as_bytes();
        let directory = tempdir().expect("temporary data directory");
        let data_dir = directory.path().join("router-data");
        IdentityStore::prepare_directory(&data_dir).expect("private directory");
        let store = Ntcp2TransportIdentityStore::in_data_dir(&data_dir);
        let transport = store
            .load_or_generate(router_hash, &mut OsRng)
            .expect("identity-bound transport material");

        let encoded = generate_controlled_ntcp2_router_info(
            &bundle,
            "127.0.0.1".parse().expect("loopback literal"),
            44_321,
            2,
            &transport,
        )
        .expect("signed local NTCP2 RouterInfo");
        let router_info = i2pr_proto::RouterInfo::decode(
            &encoded,
            constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
        )
        .expect("RouterInfo decodes");
        i2pr_crypto::verify_router_info(&router_info).expect("RouterInfo signature");
        assert_eq!(
            router_info
                .router_identity()
                .hash()
                .expect("signed router hash"),
            bundle.identity().hash().expect("router hash")
        );
        let address = router_info
            .addresses()
            .iter()
            .find(|address| address.transport_style() == "NTCP2")
            .expect("one NTCP2 address");
        assert_eq!(address.options().get("host"), Some("127.0.0.1"));
        assert_eq!(address.options().get("port"), Some("44321"));
        assert_eq!(address.options().get("v"), Some("2"));
        assert_eq!(
            router_info.options().get("netId"),
            Some("2"),
            "controlled address uses the active test network ID"
        );

        assert!(
            generate_controlled_ntcp2_router_info(
                &bundle,
                "0.0.0.0".parse().expect("wildcard"),
                44_321,
                2,
                &transport,
            )
            .is_err()
        );
    }

    #[test]
    fn controlled_dual_transport_router_info_binds_both_live_loopback_keys() {
        use i2pr_crypto::RouterIdentityBundle;
        use i2pr_storage::{
            IdentityStore, Ntcp2TransportIdentityStore, Ssu2TransportIdentityStore,
        };
        use rand_core::OsRng;
        use tempfile::tempdir;

        let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
        let hash = *bundle.identity().hash().expect("router hash").as_bytes();
        let directory = tempdir().expect("temporary data directory");
        let data_dir = directory.path().join("router-data");
        IdentityStore::prepare_directory(&data_dir).expect("private directory");
        let ssu2 = Ssu2TransportIdentityStore::in_data_dir(&data_dir)
            .load_or_generate(hash, &mut OsRng)
            .expect("SSU2 material");
        let ntcp2 = Ntcp2TransportIdentityStore::in_data_dir(&data_dir)
            .load_or_generate(hash, &mut OsRng)
            .expect("NTCP2 material");
        let encoded = generate_controlled_dual_transport_router_info(
            &bundle,
            "127.0.0.1:40123".parse().expect("SSU2 endpoint"),
            &ssu2,
            "127.0.0.1:40124".parse().expect("NTCP2 endpoint"),
            &ntcp2,
            2,
        )
        .expect("dual loopback RouterInfo");
        let info = i2pr_proto::RouterInfo::decode(
            &encoded,
            constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
        )
        .expect("RouterInfo decodes");
        i2pr_crypto::verify_router_info(&info).expect("signature verifies");
        assert_eq!(info.addresses().len(), 2);
        assert_eq!(info.options().get("caps"), None);
        for (style, expected_port) in [("SSU2", "40123"), ("NTCP2", "40124")] {
            let address = info
                .addresses()
                .iter()
                .find(|address| address.transport_style() == style)
                .unwrap_or_else(|| panic!("missing {style} address"));
            assert_eq!(address.options().get("host"), Some("127.0.0.1"));
            assert_eq!(address.options().get("port"), Some(expected_port));
            assert_eq!(address.options().get("v"), Some("2"));
        }
        assert!(
            generate_controlled_dual_transport_router_info(
                &bundle,
                "127.0.0.1:40123".parse().unwrap(),
                &ssu2,
                "0.0.0.0:40124".parse().unwrap(),
                &ntcp2,
                2,
            )
            .is_err()
        );
    }

    #[test]
    fn controlled_identity_uses_router_bound_persistent_transport_keys() {
        use i2pr_crypto::RouterIdentityBundle;
        use rand_core::OsRng;

        let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
        let hash = *bundle.identity().hash().expect("router hash").as_bytes();
        let transport =
            Ssu2TransportIdentityMaterial::generate(hash, &mut OsRng).expect("transport keys");
        let generated =
            generate_controlled_identity_with_keys(&bundle, "127.0.0.1", 44001, &transport)
                .expect("signed controlled identity");
        assert_eq!(
            generated.router_hash,
            bundle.identity().hash().expect("hash")
        );
        assert_eq!(
            generated.static_secret_bytes,
            *transport.static_key().secret_bytes()
        );
        let signed = i2pr_proto::RouterInfo::decode(
            &generated.router_info,
            constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
        )
        .expect("signed RouterInfo decodes");
        let address = signed
            .addresses()
            .iter()
            .find(|address| address.transport_style() == "SSU2")
            .expect("signed SSU2 address");
        assert_eq!(
            address.options().get("s"),
            Some(i2p_b64_encode(&transport.static_key().public_bytes()).as_str())
        );
        assert_eq!(
            address.options().get("i"),
            Some(i2p_b64_encode(transport.intro_key_bytes()).as_str())
        );
        let wrong_router = RouterIdentityBundle::generate(&mut OsRng).expect("other identity");
        assert!(
            generate_controlled_identity_with_keys(&wrong_router, "127.0.0.1", 44001, &transport,)
                .is_err()
        );
    }

    #[test]
    fn private_lan_identity_is_signed_for_rfc1918_endpoint_only() {
        use i2pr_crypto::RouterIdentityBundle;
        use rand_core::OsRng;

        let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
        let hash = *bundle.identity().hash().expect("router hash").as_bytes();
        let transport =
            Ssu2TransportIdentityMaterial::generate(hash, &mut OsRng).expect("transport keys");
        let generated =
            generate_private_lan_identity_with_keys(&bundle, "192.168.10.25", 44123, &transport)
                .expect("signed private-LAN identity");
        let info = i2pr_proto::RouterInfo::decode(
            &generated.router_info,
            constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
        )
        .expect("RouterInfo decodes");
        i2pr_crypto::verify_router_info(&info).expect("signature verifies");
        assert_eq!(
            info.router_identity()
                .hash()
                .expect("router hash")
                .as_bytes(),
            &hash
        );
        let address = info
            .addresses()
            .iter()
            .find(|address| address.transport_style() == "SSU2")
            .expect("SSU2 address");
        assert_eq!(address.options().get("host"), Some("192.168.10.25"));
        assert_eq!(address.options().get("port"), Some("44123"));
        assert_eq!(
            address.options().get("s"),
            Some(i2p_b64_encode(&transport.static_key().public_bytes()).as_str())
        );
        assert_eq!(
            address.options().get("i"),
            Some(i2p_b64_encode(transport.intro_key_bytes()).as_str())
        );
        assert!(
            generate_controlled_identity_with_keys(&bundle, "192.168.10.25", 44123, &transport)
                .is_err()
        );
        assert!(
            generate_private_lan_identity_with_keys(&bundle, "198.51.100.2", 44123, &transport)
                .is_err()
        );
    }

    #[tokio::test]
    #[ignore = "requires an explicitly selected, locally assigned RFC1918 IPv4 interface"]
    async fn private_lan_profile_binds_only_the_inventory_endpoint() {
        use i2pr_crypto::RouterIdentityBundle;
        use rand_core::OsRng;

        let endpoint: SocketAddr = std::env::var("I2PR_PRIVATE_LAN_BIND")
            .expect("explicit invocation requires I2PR_PRIVATE_LAN_BIND inventory")
            .parse()
            .expect("inventory endpoint must be an IP:port literal");
        assert!(matches!(endpoint.ip(), std::net::IpAddr::V4(ip) if ip.is_private()));
        assert!(endpoint.port() >= 1024);
        let text = format!(
            "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nqualification_profile = \"private_lan_qualification\"\nbind_ipv4 = \"{}\"\nbind_ipv6 = \"\"\nport = {}\n",
            endpoint.ip(),
            endpoint.port()
        );
        let config = crate::config::Config::parse(&text)
            .expect("profile validates")
            .ssu2;
        let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("router identity");
        let hash = *bundle.identity().hash().expect("router hash").as_bytes();
        let transport =
            Ssu2TransportIdentityMaterial::generate(hash, &mut OsRng).expect("transport keys");
        let identity = generate_private_lan_identity_with_keys(
            &bundle,
            &endpoint.ip().to_string(),
            endpoint.port(),
            &transport,
        )
        .expect("private RouterInfo");
        let service = Ssu2DaemonService::new(&config, identity).expect("qualified service");
        let token = CancellationToken::new();
        let scope = ChildScope::for_test(&token, i2pr_runtime::ChildFailurePolicy::FailParent);
        let handle = service
            .start(&scope, &config)
            .await
            .expect("bind inventory endpoint");
        assert_eq!(handle.local_v4(), Some(endpoint));
        token.cancel(i2pr_runtime::CancellationReason::OperatorRequest);
        scope.shutdown().await;
    }

    #[test]
    fn verify_reference_rejects_bytes_without_ssu2() {
        assert!(matches!(
            verify_reference_router_info(b"not-a-routerinfo"),
            Err(Ssu2ServiceError::InvalidIdentity)
        ));
    }

    #[test]
    fn inner_short_build_extracts_body_and_msgid() {
        let records = DeferredBuildRecords::new(1, SHORT_BUILD_RECORD_SIZE, vec![0xAA_u8; 218])
            .expect("records");
        let bytes = I2npMessage::new_standard(
            0x1234_5678,
            Date::from_millis(1_800_000_000_000),
            I2npBody::ShortTunnelBuild(records),
        )
        .expect("message")
        .encode_standard_to_vec(MAX_ROUTER_I2NP_BYTES)
        .expect("encode");
        let (body, msgid) = extract_inner_short_build(&bytes).expect("extract");
        assert_eq!(msgid, 0x1234_5678);
        assert_eq!(body.len(), 1 + 218);
        assert_eq!(body[0], 1);
        assert!(body[1..].iter().all(|b| *b == 0xAA));
    }

    #[test]
    fn inner_non_build_returns_none() {
        let garlic = I2npMessage::new_standard(
            7,
            Date::from_millis(1_800_000_000_000),
            I2npBody::Garlic(OpaqueMessageBody {
                payload: DeferredPayload::new(vec![0xBB_u8; 32], MAX_ROUTER_I2NP_BYTES)
                    .expect("payload"),
            }),
        )
        .expect("message")
        .encode_standard_to_vec(MAX_ROUTER_I2NP_BYTES)
        .expect("encode");
        assert!(extract_inner_short_build(&garlic).is_none());
        // Message type must actually be garlic for the negative to be meaningful.
        assert_eq!(
            I2npMessage::decode_standard(&garlic, MAX_ROUTER_I2NP_BYTES)
                .expect("decode")
                .header()
                .message_type(),
            MessageType::Garlic
        );
    }

    #[test]
    fn truncated_inner_returns_none() {
        let records = DeferredBuildRecords::new(1, SHORT_BUILD_RECORD_SIZE, vec![0xAA_u8; 218])
            .expect("records");
        let bytes = I2npMessage::new_standard(
            9,
            Date::from_millis(1_800_000_000_000),
            I2npBody::ShortTunnelBuild(records),
        )
        .expect("message")
        .encode_standard_to_vec(MAX_ROUTER_I2NP_BYTES)
        .expect("encode");
        assert!(extract_inner_short_build(&bytes[..bytes.len() - 1]).is_none());
        assert!(extract_inner_short_build(&[]).is_none());
    }
}
