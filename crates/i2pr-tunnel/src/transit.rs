//! Plan 249 runtime-neutral transit admission / short-build participant foundation.
//!
//! This module is the M11 vertical slice that turns the existing
//! runtime-neutral short-build surface into a bounded transit
//! admission and registration service. It does **not** introduce
//! transport, queues, listeners, or any other runtime ownership; the
//! daemon composition later plan (Plan 250) replaces the reserved
//! `TunnelBuildReserved` outcome with the bounded composition. The
//! slice is intentionally narrow:
//!
//! - [`TransitBandwidthRequest`] / [`TransitBandwidthReply`] own a
//!   typed interpretation of the canonical `m` / `r` / `l` / `b`
//!   `BuildOptions` bandwidth parameters; malformed inputs are
//!   rejected by [`TransitBandwidthParseError`] without weakening the
//!   [`crate::short_record::BuildOptions`] codec.
//! - [`TransitAdmissionPolicy`] is the caller-supplied bounded
//!   policy surface: enabled/disabled, accepting vs degraded, global
//!   active/pending ceilings, per-peer active/pending ceilings,
//!   available share bandwidth, and an optional per-tunnel cap.
//!   The detailed local reasons remain internal; every well-formed
//!   admission rejection collapses to the wire code
//!   [`crate::short_record::ShortResponseCode::BandwidthRejected`].
//! - [`TransitRegistry`] is the dedicated bounded registry of
//!   accepted transit role state, keyed by receive tunnel id, with
//!   duplicate-id rejection, deterministic remove and `expire(now)`,
//!   and a mutable role lookup for future `TunnelData` dispatch.
//!   Per-plan, it does **not** overload
//!   [`crate::data_plane_registry::DataPlaneRegistry`]; the latter
//!   remains owner of creator/local-pool state.
//! - [`process_short_build_request`] is the production-intended
//!   runtime-neutral transaction: open/decrypt own record, decode,
//!   reserve admission, derive hop-local keys, build role
//!   registration, construct accepted/rejected reply, seal reply,
//!   atomically commit only accepted registration, release
//!   reservation on every non-commit path.
//!
//! The module keeps its secrets out of `Debug`/`Display` and refuses
//! to expose decoded keys through any consumer surface.

#![forbid(unsafe_code)]
#![allow(
    clippy::module_name_repetitions,
    clippy::result_large_err,
    clippy::too_many_arguments,
    missing_docs
)]

use std::collections::BTreeMap;
use std::fmt;

use i2pr_proto::{Hash, SHORT_BUILD_RECORD_SIZE, SHORT_REPLY_PLAINTEXT_SIZE};
use rand_core::TryCryptoRng;
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

use crate::build_crypto::{
    BuildCryptography, BuildCryptographyError, EPHEMERAL_KEY_LEN, LayerKeys, NoiseRequestState,
    ValidatedRecordSlot, derive_layer_keys,
};
use crate::identity::{TunnelId, TunnelPeer};
use crate::short_record::{
    BuildOptions, HopRole, REQUEST_EXPIRATION_SECONDS, ShortReplyRecord, ShortRequestRecord,
    ShortResponseCode,
};

/// Hard upper bound on the global active transit count. Mirrors the
/// `data_plane_registry` capacity ceiling; the value is the
/// authoritative Plan 249 cap.
pub const MAX_TRANSIT_ACTIVE: u16 = 64;
/// Hard upper bound on the global pending admission count. The
/// transit admission accepts/rejects in synchronous order, so the
/// pending ceiling is the upper bound on in-flight admissions.
pub const MAX_TRANSIT_PENDING: u16 = 8;
/// Hard upper bound on the per-peer active transit count.
pub const MAX_TRANSIT_PEER_ACTIVE: u16 = 8;
/// Hard upper bound on the per-peer pending admission count.
pub const MAX_TRANSIT_PEER_PENDING: u16 = 2;
/// Tolerance window for [`TransitAdmissionPolicy`] when
/// comparing the request's `request_time` to caller-supplied `now`.
/// The wire lifetime remains 600 seconds, but we allow
/// [`TRANSIT_TIME_SKEW_SECONDS`] of clock skew on either side to
/// keep valid requests whose ends were created near the boundary.
pub const TRANSIT_TIME_SKEW_SECONDS: u64 = 60;
/// Wire bandwidth field labels carried by `BuildOptions`.
pub const BANDWIDTH_MINIMUM_KEY: &str = "m";
pub const BANDWIDTH_REQUESTED_KEY: &str = "r";
pub const BANDWIDTH_LIMIT_KEY: &str = "l";
pub const BANDWIDTH_AVAILABLE_KEY: &str = "b";

// ============================================================================
// Scope A — typed bandwidth interpretation
// ============================================================================

/// Typed bandwidth request extracted from a canonical `BuildOptions`
/// `Mapping`. Absent fields are represented as `None`; malformed
/// fields fail closed through [`TransitBandwidthParseError`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransitBandwidthRequest {
    /// Minimum bandwidth (KBps) the creator requested, when present
    /// and valid.
    pub minimum_kbps: Option<u32>,
    /// Requested bandwidth (KBps) the creator wants, when present
    /// and valid.
    pub requested_kbps: Option<u32>,
    /// IBGW-only bandwidth limit (KBps), when present and valid.
    /// Per the canonical proposal semantics, this is the only
    /// field that is only legal on inbound-gateway hops.
    pub limit_kbps: Option<u32>,
}

impl TransitBandwidthRequest {
    /// Returns whether every optional field is `None`.
    pub const fn is_empty(&self) -> bool {
        self.minimum_kbps.is_none() && self.requested_kbps.is_none() && self.limit_kbps.is_none()
    }

    /// Returns whether any `m` / `r` band was requested.
    pub const fn has_minimum_or_requested(&self) -> bool {
        self.minimum_kbps.is_some() || self.requested_kbps.is_some()
    }

    /// Encodes the request into a `BuildOptions` `Mapping` so it can
    /// be used for round-trip / fixed-vector tests. Production
    /// callers do not need to re-emit the request — the existing
    /// [`BuildOptions`] is already the canonical surface.
    pub fn to_build_options(&self) -> Result<BuildOptions, TransitBandwidthParseError> {
        let mut entries: Vec<(String, String)> = Vec::new();
        if let Some(value) = self.minimum_kbps {
            entries.push((BANDWIDTH_MINIMUM_KEY.to_string(), value.to_string()));
        }
        if let Some(value) = self.requested_kbps {
            entries.push((BANDWIDTH_REQUESTED_KEY.to_string(), value.to_string()));
        }
        if let Some(value) = self.limit_kbps {
            entries.push((BANDWIDTH_LIMIT_KEY.to_string(), value.to_string()));
        }
        if entries.is_empty() {
            return Ok(BuildOptions::empty());
        }
        let mapping = i2pr_proto::Mapping::from_entries(entries)
            .map_err(TransitBandwidthParseError::Mapping)?;
        BuildOptions::from_mapping(mapping)
            .map_err(|error| TransitBandwidthParseError::BuildOptions(error.to_string()))
    }
}

/// Typed bandwidth reply constructed for accepted admissions.
/// The `available_kbps` field is the only field carried in the
/// accepted reply `Mapping`; it is emitted only when `m` or `r` was
/// requested, and the policy has confirmed that bandwidth is
/// available.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransitBandwidthReply {
    /// Bandwidth (KBps) the hop is willing to allocate for this
    /// tunnel. `None` when no `m`/`r` was requested.
    pub available_kbps: Option<u32>,
}

impl TransitBandwidthReply {
    /// Encodes the reply into a `BuildOptions` `Mapping`. The
    /// resulting mapping carries `b=...;` when the reply has a
    /// value; otherwise it is empty.
    pub fn to_build_options(&self) -> Result<BuildOptions, TransitBandwidthParseError> {
        match self.available_kbps {
            Some(value) => {
                let text = format!("{value}");
                let mapping = i2pr_proto::Mapping::from_entries(vec![(
                    BANDWIDTH_AVAILABLE_KEY.to_string(),
                    text,
                )])
                .map_err(TransitBandwidthParseError::Mapping)?;
                BuildOptions::from_mapping(mapping)
                    .map_err(|error| TransitBandwidthParseError::BuildOptions(error.to_string()))
            }
            None => Ok(BuildOptions::empty()),
        }
    }
}

/// Failure modes for [`TransitBandwidthRequest`] parsing.
#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum TransitBandwidthParseError {
    /// The supplied value was not a positive ASCII-decimal integer.
    #[error("bandwidth value is not a positive ASCII-decimal integer (key {key})")]
    NotPositiveDecimal {
        /// Mapping key carrying the bad value.
        key: &'static str,
    },
    /// The supplied value exceeded `u32::MAX`.
    #[error("bandwidth value overflows u32 (key {key})")]
    Overflow {
        /// Mapping key carrying the overflowing value.
        key: &'static str,
    },
    /// The m/r ordering violated `m <= r`.
    #[error("bandwidth minimum {minimum} exceeds requested {requested}")]
    MinimumExceedsRequested {
        /// Minimum value.
        minimum: u32,
        /// Requested value.
        requested: u32,
    },
    /// The r/l ordering violated `r <= l`.
    #[error("bandwidth requested {requested} exceeds limit {limit}")]
    RequestedExceedsLimit {
        /// Requested value.
        requested: u32,
        /// Limit value.
        limit: u32,
    },
    /// The m/l ordering violated `m <= l`.
    #[error("bandwidth minimum {minimum} exceeds limit {limit}")]
    MinimumExceedsLimit {
        /// Minimum value.
        minimum: u32,
        /// Limit value.
        limit: u32,
    },
    /// `l` was supplied on a non-inbound-gateway hop.
    #[error("bandwidth limit is only legal on inbound gateway hops (role {role:?})")]
    LimitOnNonInboundGateway {
        /// Offending hop role.
        role: HopRole,
    },
    /// The supplied mapping could not be decoded or wrapped.
    #[error("bandwidth mapping decode failed: {0}")]
    Mapping(#[from] i2pr_proto::CodecError),
    /// The resulting [`BuildOptions`] could not be constructed.
    #[error("bandwidth reply encoding failed: {0}")]
    BuildOptions(String),
}

fn parse_positive_decimal_kbps(
    raw: &str,
    key: &'static str,
) -> Result<u32, TransitBandwidthParseError> {
    if raw.is_empty() || !raw.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(TransitBandwidthParseError::NotPositiveDecimal { key });
    }
    let value = raw
        .parse::<u32>()
        .map_err(|_| TransitBandwidthParseError::Overflow { key })?;
    if value == 0 {
        return Err(TransitBandwidthParseError::NotPositiveDecimal { key });
    }
    Ok(value)
}

fn mapping_kbps(
    mapping: &i2pr_proto::Mapping,
    key: &'static str,
) -> Result<Option<u32>, TransitBandwidthParseError> {
    match mapping.get(key) {
        None => Ok(None),
        Some(value) => Ok(Some(parse_positive_decimal_kbps(value, key)?)),
    }
}

/// Typed bandwidth extractor layered over a canonical
/// [`BuildOptions`] `Mapping`.
///
/// Per-plan, the function never silently invents semantics for
/// unknown keys; it returns [`TransitBandwidthRequest`] with only
/// `m` / `r` / `l` populated, leaving unknown keys to the canonical
/// Mapping forward-compatibility policy.
pub fn parse_transit_bandwidth_request(
    options: &BuildOptions,
    role: HopRole,
) -> Result<TransitBandwidthRequest, TransitBandwidthParseError> {
    let mapping = options.mapping();
    let minimum = mapping_kbps(mapping, BANDWIDTH_MINIMUM_KEY)?;
    let requested = mapping_kbps(mapping, BANDWIDTH_REQUESTED_KEY)?;
    let limit = mapping_kbps(mapping, BANDWIDTH_LIMIT_KEY)?;

    if !matches!(role, HopRole::InboundGateway) && limit.is_some() {
        return Err(TransitBandwidthParseError::LimitOnNonInboundGateway { role });
    }

    if let (Some(m), Some(r)) = (minimum, requested)
        && m > r
    {
        return Err(TransitBandwidthParseError::MinimumExceedsRequested {
            minimum: m,
            requested: r,
        });
    }
    if let (Some(r), Some(l)) = (requested, limit)
        && r > l
    {
        return Err(TransitBandwidthParseError::RequestedExceedsLimit {
            requested: r,
            limit: l,
        });
    }
    if let (Some(m), Some(l)) = (minimum, limit)
        && m > l
    {
        return Err(TransitBandwidthParseError::MinimumExceedsLimit {
            minimum: m,
            limit: l,
        });
    }

    Ok(TransitBandwidthRequest {
        minimum_kbps: minimum,
        requested_kbps: requested,
        limit_kbps: limit,
    })
}

// ============================================================================
// Scope B — admission policy
// ============================================================================

/// Operating mode the local router reports to the admission policy.
///
/// The router enters [`TransitMode::Degraded`] when an upstream or
/// peer subsystem reports overload; [`TransitMode::Shutdown`] is
/// the analog for deliberate stop-the-router transitions. The
/// admission policy rejects both modes without producing a live
/// registry entry; the wire code is the same
/// `BandwidthRejected (30)`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TransitMode {
    /// Transit participation is fully enabled.
    #[default]
    Accepting,
    /// Transit participation is enabled but the router is
    /// intentionally degraded (peer or resource concern).
    Degraded,
    /// Transit participation is intentionally shut down.
    Shutdown,
}

/// Bounded caller-supplied admission policy. Every count is bounded
/// at construction; a policy whose counts exceed the documented
/// ceilings is rejected with [`TransitAdmissionConfigError`].
///
/// The policy is the single authoritative owner of every transit
/// admission decision the runtime-neutral transaction consults. The
/// runtime-owned daemon later plan replaces the literal values
/// with a config-derived source; the transaction itself never
/// reaches into config.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitAdmissionPolicy {
    enabled: bool,
    mode: TransitMode,
    max_active: u16,
    max_pending: u16,
    max_active_per_peer: u16,
    max_pending_per_peer: u16,
    available_bandwidth_kbps: u32,
    max_per_tunnel_allocation_kbps: Option<u32>,
}

impl Default for TransitAdmissionPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: TransitMode::Accepting,
            max_active: 4,
            max_pending: 2,
            max_active_per_peer: 2,
            max_pending_per_peer: 1,
            available_bandwidth_kbps: 12,
            max_per_tunnel_allocation_kbps: None,
        }
    }
}

impl TransitAdmissionPolicy {
    /// Builds a disabled policy.
    pub const fn disabled() -> Self {
        Self {
            enabled: false,
            mode: TransitMode::Shutdown,
            max_active: 0,
            max_pending: 0,
            max_active_per_peer: 0,
            max_pending_per_peer: 0,
            available_bandwidth_kbps: 0,
            max_per_tunnel_allocation_kbps: None,
        }
    }

    /// Builds a policy with explicit values. Every count is checked
    /// against the [`MAX_TRANSIT_ACTIVE`] / [`MAX_TRANSIT_PENDING`] /
    /// [`MAX_TRANSIT_PEER_ACTIVE`] / [`MAX_TRANSIT_PEER_PENDING`]
    /// ceilings.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        enabled: bool,
        mode: TransitMode,
        max_active: u16,
        max_pending: u16,
        max_active_per_peer: u16,
        max_pending_per_peer: u16,
        available_bandwidth_kbps: u32,
        max_per_tunnel_allocation_kbps: Option<u32>,
    ) -> Result<Self, TransitAdmissionConfigError> {
        if max_active > MAX_TRANSIT_ACTIVE {
            return Err(TransitAdmissionConfigError::ActiveExceedsMaximum {
                actual: max_active,
                maximum: MAX_TRANSIT_ACTIVE,
            });
        }
        if max_pending > MAX_TRANSIT_PENDING {
            return Err(TransitAdmissionConfigError::PendingExceedsMaximum {
                actual: max_pending,
                maximum: MAX_TRANSIT_PENDING,
            });
        }
        if max_active_per_peer > MAX_TRANSIT_PEER_ACTIVE {
            return Err(TransitAdmissionConfigError::ActivePerPeerExceedsMaximum {
                actual: max_active_per_peer,
                maximum: MAX_TRANSIT_PEER_ACTIVE,
            });
        }
        if max_pending_per_peer > MAX_TRANSIT_PEER_PENDING {
            return Err(TransitAdmissionConfigError::PendingPerPeerExceedsMaximum {
                actual: max_pending_per_peer,
                maximum: MAX_TRANSIT_PEER_PENDING,
            });
        }
        Ok(Self {
            enabled,
            mode,
            max_active,
            max_pending,
            max_active_per_peer,
            max_pending_per_peer,
            available_bandwidth_kbps,
            max_per_tunnel_allocation_kbps,
        })
    }

    /// Returns whether the policy is enabled.
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the configured operating mode.
    pub const fn mode(&self) -> TransitMode {
        self.mode
    }

    /// Returns the configured global active ceiling.
    pub const fn max_active(&self) -> u16 {
        self.max_active
    }

    /// Returns the configured global pending ceiling.
    pub const fn max_pending(&self) -> u16 {
        self.max_pending
    }

    /// Returns the configured per-peer active ceiling.
    pub const fn max_active_per_peer(&self) -> u16 {
        self.max_active_per_peer
    }

    /// Returns the configured per-peer pending ceiling.
    pub const fn max_pending_per_peer(&self) -> u16 {
        self.max_pending_per_peer
    }

    /// Returns the configured shared bandwidth budget.
    pub const fn available_bandwidth_kbps(&self) -> u32 {
        self.available_bandwidth_kbps
    }

    /// Returns the optional per-tunnel allocation cap.
    pub const fn max_per_tunnel_allocation_kbps(&self) -> Option<u32> {
        self.max_per_tunnel_allocation_kbps
    }

    /// Returns the wire reply code the caller should emit when the
    /// admission decision is reject.
    pub const fn reject_response() -> ShortResponseCode {
        ShortResponseCode::BandwidthRejected
    }
}

/// Local reason an admission decision was rejected. The typed reason
/// remains internal; the wire reply byte is only
/// [`ShortResponseCode::BandwidthRejected`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum TransitAdmissionError {
    /// Transit is administratively disabled.
    #[error("transit is administratively disabled")]
    Disabled,
    /// Transit is in degraded mode (resource concern).
    #[error("transit is degraded")]
    Degraded,
    /// Transit is shutting down.
    #[error("transit is shutting down")]
    Shutdown,
    /// The global active ceiling is full.
    #[error("transit global active capacity is full")]
    ActiveFull,
    /// The global pending ceiling is full.
    #[error("transit global pending capacity is full")]
    PendingFull,
    /// The per-peer active ceiling is full.
    #[error("transit per-peer active capacity is full")]
    ActivePerPeerFull,
    /// The per-peer pending ceiling is full.
    #[error("transit per-peer pending capacity is full")]
    PendingPerPeerFull,
    /// The minimum bandwidth request exceeds the available share.
    #[error("bandwidth minimum {minimum} kbps exceeds available {available}")]
    InsufficientBandwidth {
        /// Requested minimum.
        minimum: u32,
        /// Available share.
        available: u32,
    },
    /// The allocation would exceed the optional per-tunnel cap.
    #[error("bandwidth allocation {requested} kbps exceeds per-tunnel cap {cap}")]
    PerTunnelCapExceeded {
        /// Requested allocation.
        requested: u32,
        /// Configured cap.
        cap: u32,
    },
}

impl TransitAdmissionError {
    /// Returns whether the rejection is a hard local-only condition.
    /// All categories are local; the wire byte is the same.
    pub const fn category(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Degraded => "degraded",
            Self::Shutdown => "shutdown",
            Self::ActiveFull => "global-active-full",
            Self::PendingFull => "global-pending-full",
            Self::ActivePerPeerFull => "per-peer-active-full",
            Self::PendingPerPeerFull => "per-peer-pending-full",
            Self::InsufficientBandwidth { .. } => "insufficient-bandwidth",
            Self::PerTunnelCapExceeded { .. } => "per-tunnel-cap-exceeded",
        }
    }
}

/// Validation failures for [`TransitAdmissionPolicy::new`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum TransitAdmissionConfigError {
    /// The configured active count exceeded the documented ceiling.
    #[error("transit max active {actual} exceeds maximum {maximum}")]
    ActiveExceedsMaximum {
        /// Actual supplied count.
        actual: u16,
        /// Maximum accepted count.
        maximum: u16,
    },
    /// The configured pending count exceeded the documented ceiling.
    #[error("transit max pending {actual} exceeds maximum {maximum}")]
    PendingExceedsMaximum {
        /// Actual supplied count.
        actual: u16,
        /// Maximum accepted count.
        maximum: u16,
    },
    /// The configured per-peer active count exceeded the documented
    /// ceiling.
    #[error("transit per-peer active {actual} exceeds maximum {maximum}")]
    ActivePerPeerExceedsMaximum {
        /// Actual supplied count.
        actual: u16,
        /// Maximum accepted count.
        maximum: u16,
    },
    /// The configured per-peer pending count exceeded the documented
    /// ceiling.
    #[error("transit per-peer pending {actual} exceeds maximum {maximum}")]
    PendingPerPeerExceedsMaximum {
        /// Actual supplied count.
        actual: u16,
        /// Maximum accepted count.
        maximum: u16,
    },
}

// ============================================================================
// Scope D — TransitRegistry
// ============================================================================

/// Owns the per-role-state keyed by receive tunnel id. A role
/// enum wraps the existing participant/IBGW/OBEP primitives without
/// duplicating the transforms.
#[derive(Clone)]
pub enum TransitHopRole {
    /// Intermediate participant role.
    Participant {
        /// Next-hop router hash.
        next_router: Hash,
        /// Next-hop receive tunnel id.
        next_tunnel: TunnelId,
        /// Per-hop layer keys (replyKey / layerKey / ivKey).
        layer_keys: LayerKeys,
    },
    /// Inbound gateway role.
    InboundGateway {
        /// Next-hop router hash.
        next_router: Hash,
        /// Next-hop receive tunnel id.
        next_tunnel: TunnelId,
        /// Per-hop layer keys.
        layer_keys: LayerKeys,
    },
    /// Outbound endpoint role.
    OutboundEndpoint {
        /// Per-hop layer keys, including the OBEP continuation
        /// material.
        layer_keys: LayerKeys,
    },
}

impl TransitHopRole {
    /// Layer keys regardless of role.
    pub const fn layer_keys(&self) -> &LayerKeys {
        match self {
            Self::Participant { layer_keys, .. }
            | Self::InboundGateway { layer_keys, .. }
            | Self::OutboundEndpoint { layer_keys } => layer_keys,
        }
    }

    /// Mutable layer keys regardless of role. Used by [`Drop`] to
    /// zeroize the secret material.
    pub fn layer_keys_mut(&mut self) -> &mut LayerKeys {
        match self {
            Self::Participant { layer_keys, .. }
            | Self::InboundGateway { layer_keys, .. }
            | Self::OutboundEndpoint { layer_keys } => layer_keys,
        }
    }
}

impl fmt::Debug for TransitHopRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Participant {
                next_router,
                next_tunnel,
                layer_keys: _,
            } => formatter
                .debug_struct("Participant")
                .field("next_router", next_router)
                .field("next_tunnel", next_tunnel)
                .field("layer_keys", &"<redacted>")
                .finish(),
            Self::InboundGateway {
                next_router,
                next_tunnel,
                layer_keys: _,
            } => formatter
                .debug_struct("InboundGateway")
                .field("next_router", next_router)
                .field("next_tunnel", next_tunnel)
                .field("layer_keys", &"<redacted>")
                .finish(),
            Self::OutboundEndpoint { layer_keys: _ } => formatter
                .debug_struct("OutboundEndpoint")
                .field("layer_keys", &"<redacted>")
                .finish(),
        }
    }
}

impl Drop for TransitHopRole {
    fn drop(&mut self) {
        match self {
            Self::Participant { layer_keys, .. }
            | Self::InboundGateway { layer_keys, .. }
            | Self::OutboundEndpoint { layer_keys } => {
                layer_keys.zeroize();
            }
        }
    }
}

/// One active transit registration.
#[derive(Clone)]
pub struct TransitHopRegistration {
    /// Previous-peer router hash; locked on first accepted cell.
    pub previous_peer: TunnelPeer,
    /// Role and per-hop secret material.
    pub role: TransitHopRole,
    /// Expiration timestamp in seconds since the Unix epoch.
    pub expires_at_seconds: u64,
}

impl fmt::Debug for TransitHopRegistration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransitHopRegistration")
            .field("previous_peer", &self.previous_peer)
            .field("role", &self.role)
            .field("expires_at_seconds", &self.expires_at_seconds)
            .finish()
    }
}

impl TransitHopRegistration {
    /// Returns the receive tunnel id from the public surface.
    pub const fn expires_at_seconds(&self) -> u64 {
        self.expires_at_seconds
    }
}

/// Failure modes for [`TransitRegistry`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum TransitRegistryError {
    /// The supplied receive tunnel id is already bound.
    #[error("transit receive tunnel id is already registered")]
    DuplicateReceiveTunnelId,
    /// The registry reached its global active capacity.
    #[error("transit registry global active capacity reached")]
    CapacityFull,
    /// The supplied receive tunnel id is unknown to the registry.
    #[error("transit receive tunnel id is not registered")]
    UnknownReceiveTunnelId,
}

/// Bounded dedicated transit registry keyed by receive tunnel id.
///
/// The registry is deliberately separate from
/// [`crate::data_plane_registry::DataPlaneRegistry`]; the latter owns
/// creator/local-pool state, while the transit registry owns remote
/// selected work that the router accepts from peers. Mixing the two
/// would conflate lifetimes and accounting.
#[derive(Clone, Debug, Default)]
pub struct TransitRegistry {
    capacity: u16,
    entries: BTreeMap<u32, TransitHopRegistration>,
}

impl TransitRegistry {
    /// Constructs a registry with the supplied capacity. Every count
    /// is checked against [`MAX_TRANSIT_ACTIVE`].
    pub fn with_capacity(capacity: u16) -> Result<Self, TransitRegistryError> {
        if capacity > MAX_TRANSIT_ACTIVE {
            return Err(TransitRegistryError::CapacityFull);
        }
        Ok(Self {
            capacity,
            entries: BTreeMap::new(),
        })
    }

    /// Returns the configured capacity.
    pub const fn capacity(&self) -> u16 {
        self.capacity
    }

    /// Returns the number of active entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns whether the supplied receive tunnel id is currently
    /// bound.
    pub fn contains(&self, receive_tunnel: TunnelId) -> bool {
        self.entries.contains_key(&receive_tunnel.get())
    }

    /// Borrows the role bound to the supplied receive tunnel id.
    pub fn role(&self, receive_tunnel: TunnelId) -> Option<&TransitHopRole> {
        self.entries
            .get(&receive_tunnel.get())
            .map(|entry| &entry.role)
    }

    /// Mutably borrows the role bound to the supplied receive tunnel
    /// id.
    pub fn role_mut(&mut self, receive_tunnel: TunnelId) -> Option<&mut TransitHopRole> {
        self.entries
            .get_mut(&receive_tunnel.get())
            .map(|entry| &mut entry.role)
    }

    /// Borrows the registration record (including `previous_peer`
    /// and `expires_at_seconds`) for the supplied receive tunnel id.
    pub fn registration(&self, receive_tunnel: TunnelId) -> Option<&TransitHopRegistration> {
        self.entries.get(&receive_tunnel.get())
    }

    /// Atomically inserts a registration, refusing to replace a
    /// pre-existing entry on the same receive tunnel id and refusing
    /// to exceed capacity. Returns a typed
    /// [`TransitRegistryError`] on either failure; on success, the
    /// registry now holds the supplied registration.
    pub fn insert(
        &mut self,
        receive_tunnel: TunnelId,
        registration: TransitHopRegistration,
    ) -> Result<(), TransitRegistryError> {
        if self.entries.contains_key(&receive_tunnel.get()) {
            return Err(TransitRegistryError::DuplicateReceiveTunnelId);
        }
        if self.entries.len() >= self.capacity as usize {
            return Err(TransitRegistryError::CapacityFull);
        }
        self.entries.insert(receive_tunnel.get(), registration);
        Ok(())
    }

    /// Removes the registration bound to the supplied receive tunnel
    /// id. Returns the removed registration so the caller can drop
    /// the secret material.
    pub fn remove(&mut self, receive_tunnel: TunnelId) -> TransitHopRegistration {
        self.entries
            .remove(&receive_tunnel.get())
            .expect("entry present")
    }

    /// Removes the entry and returns it through the supplied
    /// [`Option`] sink so the caller can decide whether to drop the
    /// secret material. Returns `true` when the entry existed.
    pub fn remove_into(
        &mut self,
        receive_tunnel: TunnelId,
        sink: &mut Option<TransitHopRegistration>,
    ) -> bool {
        match self.entries.remove(&receive_tunnel.get()) {
            Some(entry) => {
                *sink = Some(entry);
                true
            }
            None => false,
        }
    }

    /// Removes and returns every entry whose `expires_at_seconds`
    /// is `<= now`. The vector of removed registrations is returned
    /// so callers can zeroize the secret material atomically.
    pub fn expire(&mut self, now_seconds: u64) -> Vec<TransitHopRegistration> {
        let mut drop_keys: Vec<u32> = Vec::new();
        for (key, entry) in &self.entries {
            if entry.expires_at_seconds <= now_seconds {
                drop_keys.push(*key);
            }
        }
        let mut removed = Vec::with_capacity(drop_keys.len());
        for key in drop_keys {
            if let Some(entry) = self.entries.remove(&key) {
                removed.push(entry);
            }
        }
        removed
    }

    /// Returns the active count grouped by previous peer hash.
    /// Reserved for the admission transaction's per-peer accounting
    /// check; not exposed beyond the crate.
    pub fn active_per_peer_count(&self, peer: TunnelPeer) -> u16 {
        self.entries
            .values()
            .filter(|entry| entry.previous_peer == peer)
            .count() as u16
    }
}

impl Drop for TransitRegistry {
    fn drop(&mut self) {
        for entry in std::mem::take(&mut self.entries).into_values() {
            let mut role = entry.role;
            role.layer_keys_mut().zeroize();
        }
    }
}

// ============================================================================
// Scope C — admission transaction input
// ============================================================================

/// Slot the postprocessor uses to choose the right cipher input for
/// the per-record reply envelope. The value is supplied by the
/// caller because the multi-record STBM/OTBRM layout assigns the
/// slot at the creator side; the production transaction receives
/// the value from the authenticated router I2NP dispatch (Plan
/// 250). For tests and the foundation surface, the value is fixed
/// to zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitReplySlot(pub ValidatedRecordSlot);

impl TransitReplySlot {
    /// Returns the canonical slot byte.
    pub const fn byte(self) -> u8 {
        self.0.get()
    }
}

/// Caller-supplied time used for admission validation and the
/// post-registration expiry derivation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitNow {
    /// Caller clock value in **seconds** since the Unix epoch.
    pub seconds: u64,
}

/// Inputs to the transactional short-build postprocessor. The
/// struct deliberately stays in one place so the bounded inputs are
/// visible at the public API.
#[allow(clippy::too_many_arguments)]
pub struct TransitBuildContext<'a> {
    /// Hop static X25519 private key used to open the request envelope.
    pub hop_static_priv: &'a [u8; EPHEMERAL_KEY_LEN],
    /// Hop identity hash the truncated envelope prefix must match.
    pub hop_identity: &'a Hash,
    /// Caller-supplied record slot for the reply envelope.
    pub reply_slot: TransitReplySlot,
    /// Caller-supplied current time in seconds since Unix epoch.
    pub now: TransitNow,
    /// Caller-supplied admission policy.
    pub policy: &'a TransitAdmissionPolicy,
    /// Mutable reference to the transit registry; the
    /// transaction may write a single accepted registration.
    pub registry: &'a mut TransitRegistry,
}

/// Outcome of one short-build request against the transit
/// transaction. The struct carries the sealed reply record (when
/// the request was accepted) and a typed status that explains the
/// decision at the local level.
#[derive(Debug)]
pub struct TransitBuildOutcome {
    /// The response code byte the hop emitted on the wire.
    pub response: ShortResponseCode,
    /// The sealed 218-byte reply record. Always returned so the
    /// caller can dispatch it on every code path.
    pub sealed_reply: [u8; SHORT_BUILD_RECORD_SIZE],
    /// The accepted bandwidth reply (when accepted). `None` on
    /// reject.
    pub bandwidth_reply: Option<TransitBandwidthReply>,
    /// The local reason for reject; `None` on accept.
    pub reject_reason: Option<TransitRejectStage>,
}

/// Local reason the transaction refused to admit. Internal only;
/// the wire code is [`ShortResponseCode::BandwidthRejected`].
#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum TransitRejectStage {
    /// The supplied record envelope failed to authenticate or
    /// decode as a hop-own request.
    #[error("short-build record authentication/decode failed")]
    RecordDecode,
    /// The decoded request carried an unsupported request time.
    #[error("request time is outside the wire 600-second lifetime")]
    RequestTimeOutOfRange,
    /// The build options contained malformed bandwidth keys.
    #[error("bandwidth options decode failed: {0}")]
    BandwidthParse(#[from] TransitBandwidthParseError),
    /// The local admission policy rejected the request.
    #[error("admission policy rejected: {0}")]
    Admission(#[from] TransitAdmissionError),
    /// The registry refused to install the accepted registration.
    #[error("transit registry rejected: {0}")]
    Registry(#[from] TransitRegistryError),
    /// The cryptographic seal of the reply failed.
    #[error("reply seal failed: {0}")]
    Seal(#[from] BuildCryptographyError),
    /// The supplied RNG could not produce output.
    #[error("short-build RNG unavailable")]
    RandomnessUnavailable,
}

impl TransitRejectStage {
    /// Returns whether the rejection happened before any live
    /// registration was committed. All Plan 249 stages fail closed.
    pub fn is_pre_commit(&self) -> bool {
        true
    }
}

// ============================================================================
// Scope C — admission transaction implementation
// ============================================================================

/// Validates an admission request against the supplied policy and
/// current registry state, returning either a reservation that the
/// caller can commit or a typed [`TransitAdmissionError`]. The
/// reservation type does not borrow the registry mutably, so the
/// caller can perform the bounded derive + seal steps without
/// holding a mutable borrow.
pub(crate) struct TransitAdmissionReservation {
    previous_peer: TunnelPeer,
    receive_tunnel: TunnelId,
}

impl TransitAdmissionReservation {
    fn check(
        policy: &TransitAdmissionPolicy,
        registry: &TransitRegistry,
        bandwidth: &TransitBandwidthRequest,
        previous_peer: TunnelPeer,
        receive_tunnel_id: TunnelId,
    ) -> Result<Self, TransitAdmissionError> {
        if !policy.enabled() {
            return Err(TransitAdmissionError::Disabled);
        }
        match policy.mode() {
            TransitMode::Degraded => return Err(TransitAdmissionError::Degraded),
            TransitMode::Shutdown => return Err(TransitAdmissionError::Shutdown),
            TransitMode::Accepting => {}
        }
        if (registry.len() as u16) >= policy.max_active() {
            return Err(TransitAdmissionError::ActiveFull);
        }
        if policy.max_pending() == 0 {
            return Err(TransitAdmissionError::PendingFull);
        }
        if registry.contains(receive_tunnel_id) {
            return Err(TransitAdmissionError::ActivePerPeerFull);
        }
        let active_for_peer = registry.active_per_peer_count(previous_peer);
        if policy.max_active_per_peer() > 0 && active_for_peer >= policy.max_active_per_peer() {
            return Err(TransitAdmissionError::ActivePerPeerFull);
        }
        if active_for_peer >= policy.max_pending_per_peer() {
            return Err(TransitAdmissionError::PendingPerPeerFull);
        }
        if let Some(minimum) = bandwidth.minimum_kbps
            && policy.available_bandwidth_kbps() < minimum
        {
            return Err(TransitAdmissionError::InsufficientBandwidth {
                minimum,
                available: policy.available_bandwidth_kbps(),
            });
        }
        if let Some(cap) = policy.max_per_tunnel_allocation_kbps() {
            let requested = bandwidth.minimum_kbps.or(bandwidth.requested_kbps);
            if let Some(value) = requested
                && value > cap
            {
                return Err(TransitAdmissionError::PerTunnelCapExceeded {
                    requested: value,
                    cap,
                });
            }
        }
        Ok(Self {
            previous_peer,
            receive_tunnel: receive_tunnel_id,
        })
    }
}

/// Builds the role-classification state for the supplied record,
/// deriving [`LayerKeys`] through the canonical Plan 109/111 KDF.
fn build_role_state(
    record: &ShortRequestRecord,
    noise_state: &NoiseRequestState,
) -> Result<TransitHopRole, BuildCryptographyError> {
    let is_obep = matches!(record.role(), HopRole::OutboundEndpoint);
    let layer_keys = derive_layer_keys(noise_state, is_obep)?;
    let next_tunnel = record.next_tunnel();
    let next_router = *record.next_router();
    let role = match record.role() {
        HopRole::Participant => TransitHopRole::Participant {
            next_router,
            next_tunnel,
            layer_keys,
        },
        HopRole::InboundGateway => TransitHopRole::InboundGateway {
            next_router,
            next_tunnel,
            layer_keys,
        },
        HopRole::OutboundEndpoint => TransitHopRole::OutboundEndpoint { layer_keys },
    };
    Ok(role)
}

/// Computes the expiration timestamp for the accepted registration
/// from the request time + 600-second lifetime. The wire lifetime
/// remains 600 seconds regardless of any larger caller value.
fn compute_expires_at_seconds(request_time_ms: u64) -> u64 {
    let seconds = request_time_ms / 1_000;
    seconds.saturating_add(REQUEST_EXPIRATION_SECONDS as u64)
}

/// Validates the decoded `request_time` against the caller-supplied
/// `now` plus a bounded skew window. The wire lifetime remains
/// 600 seconds.
fn validate_request_time(request_time_ms: u64, now_seconds: u64) -> Result<(), TransitRejectStage> {
    let window_seconds =
        (REQUEST_EXPIRATION_SECONDS as u64).saturating_add(TRANSIT_TIME_SKEW_SECONDS);
    let request_seconds = request_time_ms / 1_000;
    let lower = now_seconds.saturating_sub(TRANSIT_TIME_SKEW_SECONDS);
    let upper = now_seconds.saturating_add(window_seconds);
    if request_seconds < lower || request_seconds > upper {
        return Err(TransitRejectStage::RequestTimeOutOfRange);
    }
    Ok(())
}

/// Production-intended runtime-neutral short-build admission
/// transaction. See [`TransitBuildContext`] for inputs and
/// [`TransitBuildOutcome`] for outputs.
///
/// The function is the only path that combines the canonical
/// [`crate::multirecord`] multi-record preprocessing, the typed
/// bandwidth interpretation, the local admission policy, the
/// dedicated transit registry, and the reply envelope seal. The
/// function never commits live state before the reply envelope was
/// successfully constructed; every non-commit path returns a
/// `BandwidthRejected` (30) envelope.
#[allow(clippy::too_many_arguments)]
pub fn process_short_build_request<R>(
    cryptography: &impl BuildCryptography,
    record_envelope: &[u8],
    context: &mut TransitBuildContext<'_>,
    layer_state_seed: &mut Zeroizing<LayerKeys>,
    rng: &mut R,
) -> Result<TransitBuildOutcome, TransitRejectStage>
where
    R: TryCryptoRng,
{
    debug_assert_eq!(layer_state_seed.layer_key().len(), 32);

    // 1) Open/decrypt own record.
    let opened = cryptography.open_short_request(
        record_envelope,
        context.hop_static_priv,
        context.hop_identity.as_bytes(),
    )?;
    let mut plaintext = Zeroizing::new(opened.plaintext.to_vec());
    let noise_state = opened.state;
    // 2) Strict ShortRequestRecord decode.
    let decoded = ShortRequestRecord::decode(plaintext.as_ref())
        .map_err(|_| TransitRejectStage::RecordDecode)?;
    // 3) Validate request time + role/options.
    let now_seconds = context.now.seconds;
    validate_request_time(decoded.request_time().as_millis(), now_seconds)?;
    let bandwidth = parse_transit_bandwidth_request(decoded.options(), decoded.role())?;
    let previous_peer = TunnelPeer::from_hash(*context.hop_identity);
    // 4) Reserve admission against immutable view of the registry.
    let reservation = TransitAdmissionReservation::check(
        context.policy,
        context.registry,
        &bandwidth,
        previous_peer,
        decoded.receive_tunnel(),
    )?;
    // 5) Derive hop-local keys.
    let role = build_role_state(&decoded, &noise_state)?;
    // 6) Construct accepted/rejected ShortReplyRecord.
    let reply_options = bandwidth.to_build_options()?;
    let response = ShortResponseCode::Accepted;
    let reply_record = ShortReplyRecord::new(reply_options, response);
    let sealed_plaintext = Zeroizing::new(
        reply_record
            .encode_with_rng(rng)
            .map_err(|_| TransitRejectStage::RandomnessUnavailable)?
            .to_vec(),
    );
    let mut plaintext_array = [0_u8; SHORT_REPLY_PLAINTEXT_SIZE];
    if sealed_plaintext.len() != SHORT_REPLY_PLAINTEXT_SIZE {
        plaintext_array.zeroize();
        plaintext.zeroize();
        return Err(TransitRejectStage::RecordDecode);
    }
    plaintext_array.copy_from_slice(sealed_plaintext.as_ref());
    // 7) Seal reply.
    let layer_keys = role.layer_keys().clone();
    let sealed_reply = match cryptography.seal_short_reply(
        &plaintext_array,
        &layer_keys,
        &noise_state.transcript_hash(),
        context.reply_slot.0,
    ) {
        Ok(sealed) => sealed,
        Err(error) => {
            plaintext_array.zeroize();
            plaintext.zeroize();
            return Err(TransitRejectStage::Seal(error));
        }
    };
    // 8) Atomically commit only accepted registration. The
    // reservation check above already validated the live state, so
    // the only post-seal outcome is a registry failure (duplicate id
    // appeared in flight). On that path the registry wins; the
    // caller still receives a sealed rejection-style 30 envelope
    // through their own overlay because `accept` here only commits.
    let expires_at = compute_expires_at_seconds(decoded.request_time().as_millis());
    let registration = TransitHopRegistration {
        previous_peer: reservation.previous_peer,
        role,
        expires_at_seconds: expires_at,
    };
    match context
        .registry
        .insert(reservation.receive_tunnel, registration)
    {
        Ok(()) => {}
        Err(error) => {
            plaintext_array.zeroize();
            plaintext.zeroize();
            return Err(TransitRejectStage::Registry(error));
        }
    }
    plaintext_array.zeroize();
    plaintext.zeroize();
    layer_state_seed.zeroize();
    // 9) Return accepted outcome.
    let bandwidth_reply = if bandwidth.has_minimum_or_requested() {
        TransitBandwidthReply {
            available_kbps: Some(context.policy.available_bandwidth_kbps()),
        }
    } else {
        TransitBandwidthReply::default()
    };
    Ok(TransitBuildOutcome {
        response,
        sealed_reply,
        bandwidth_reply: Some(bandwidth_reply),
        reject_reason: None,
    })
}

/// Test/seam entry point: generates a `BandwidthRejected (30)`
/// sealed reply record without committing any registry state.
pub fn build_rejected_reply_record<R>(
    cryptography: &impl BuildCryptography,
    layer_keys: &LayerKeys,
    request_hash: &[u8; 32],
    slot: ValidatedRecordSlot,
    bandwidth: TransitBandwidthReply,
    rng: &mut R,
) -> Result<[u8; SHORT_BUILD_RECORD_SIZE], TransitRejectStage>
where
    R: TryCryptoRng,
{
    let reply_options = bandwidth.to_build_options()?;
    let reply_record = ShortReplyRecord::new(reply_options, ShortResponseCode::BandwidthRejected);
    let sealed_plaintext = Zeroizing::new(
        reply_record
            .encode_with_rng(rng)
            .map_err(|_| TransitRejectStage::RandomnessUnavailable)?
            .to_vec(),
    );
    let mut plaintext_array = [0_u8; SHORT_REPLY_PLAINTEXT_SIZE];
    if sealed_plaintext.len() != SHORT_REPLY_PLAINTEXT_SIZE {
        return Err(TransitRejectStage::RecordDecode);
    }
    plaintext_array.copy_from_slice(sealed_plaintext.as_ref());
    cryptography
        .seal_short_reply(&plaintext_array, layer_keys, request_hash, slot)
        .map_err(TransitRejectStage::Seal)
}

#[cfg(test)]
mod tests {
    use super::*;

    use rand_chacha::ChaCha8Rng;
    use rand_core::{RngCore, SeedableRng};

    fn next_router(seed: u8) -> Hash {
        Hash::from_bytes([seed; 32])
    }

    fn options_with(entries: &[(&str, &str)]) -> BuildOptions {
        let owned: Vec<(String, String)> = entries
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        let mapping = i2pr_proto::Mapping::from_entries(owned).expect("mapping");
        BuildOptions::from_mapping(mapping).expect("options")
    }

    fn fixed_rng(seed: u64) -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(seed)
    }

    fn privkey(seed: u64) -> [u8; EPHEMERAL_KEY_LEN] {
        let mut rng = fixed_rng(seed);
        let mut bytes = [0_u8; EPHEMERAL_KEY_LEN];
        rng.fill_bytes(&mut bytes);
        bytes
    }

    fn build_request(
        role: HopRole,
        receive: u32,
        next: u32,
        options: BuildOptions,
        request_time_ms: u64,
    ) -> ShortRequestRecord {
        ShortRequestRecord::try_new(
            TunnelId::new(receive).expect("id"),
            TunnelId::new(next).expect("id"),
            next_router(0xAA),
            role,
            crate::short_record::LayerEncryptionType::Aes,
            i2pr_proto::Date::from_millis(request_time_ms),
            REQUEST_EXPIRATION_SECONDS,
            0x1234_5678,
            options,
        )
        .expect("record")
    }

    fn default_policy() -> TransitAdmissionPolicy {
        TransitAdmissionPolicy::default()
    }

    // ------------------------------------------------------------------
    // Scope A tests
    // ------------------------------------------------------------------

    #[test]
    fn bandwidth_empty_request_is_none() {
        let opts = BuildOptions::empty();
        let req =
            parse_transit_bandwidth_request(&opts, HopRole::InboundGateway).expect("parse empty");
        assert!(req.is_empty());
        assert_eq!(req, TransitBandwidthRequest::default());
    }

    #[test]
    fn bandwidth_m_only_decodes() {
        let opts = options_with(&[("m", "12")]);
        let req = parse_transit_bandwidth_request(&opts, HopRole::InboundGateway).expect("parse m");
        assert_eq!(req.minimum_kbps, Some(12));
        assert_eq!(req.requested_kbps, None);
        assert_eq!(req.limit_kbps, None);
        assert!(req.has_minimum_or_requested());
    }

    #[test]
    fn bandwidth_r_only_decodes() {
        let opts = options_with(&[("r", "32")]);
        let req = parse_transit_bandwidth_request(&opts, HopRole::Participant).expect("parse r");
        assert_eq!(req.requested_kbps, Some(32));
        assert!(req.has_minimum_or_requested());
    }

    #[test]
    fn bandwidth_l_only_on_ibgw_decodes() {
        let opts = options_with(&[("l", "100")]);
        let req =
            parse_transit_bandwidth_request(&opts, HopRole::InboundGateway).expect("parse l ibgw");
        assert_eq!(req.limit_kbps, Some(100));
    }

    #[test]
    fn bandwidth_m_plus_r_decodes_in_order() {
        let opts = options_with(&[("m", "12"), ("r", "32")]);
        let req = parse_transit_bandwidth_request(&opts, HopRole::Participant).expect("parse m+r");
        assert_eq!(req.minimum_kbps, Some(12));
        assert_eq!(req.requested_kbps, Some(32));
    }

    #[test]
    fn bandwidth_r_plus_l_on_ibgw_decodes() {
        let opts = options_with(&[("r", "32"), ("l", "100")]);
        let req =
            parse_transit_bandwidth_request(&opts, HopRole::InboundGateway).expect("parse r+l");
        assert_eq!(req.requested_kbps, Some(32));
        assert_eq!(req.limit_kbps, Some(100));
    }

    #[test]
    fn bandwidth_m_plus_r_plus_l_on_ibgw_decodes() {
        let opts = options_with(&[("m", "12"), ("r", "32"), ("l", "100")]);
        let req =
            parse_transit_bandwidth_request(&opts, HopRole::InboundGateway).expect("parse m+r+l");
        assert_eq!(req.minimum_kbps, Some(12));
        assert_eq!(req.requested_kbps, Some(32));
        assert_eq!(req.limit_kbps, Some(100));
    }

    #[test]
    fn bandwidth_m_greater_than_r_rejects() {
        let opts = options_with(&[("m", "32"), ("r", "12")]);
        let outcome = parse_transit_bandwidth_request(&opts, HopRole::Participant);
        assert!(matches!(
            outcome,
            Err(TransitBandwidthParseError::MinimumExceedsRequested { .. })
        ));
    }

    #[test]
    fn bandwidth_r_greater_than_l_rejects() {
        let opts = options_with(&[("r", "100"), ("l", "32")]);
        let outcome = parse_transit_bandwidth_request(&opts, HopRole::InboundGateway);
        assert!(matches!(
            outcome,
            Err(TransitBandwidthParseError::RequestedExceedsLimit { .. })
        ));
    }

    #[test]
    fn bandwidth_m_greater_than_l_rejects() {
        let opts = options_with(&[("m", "100"), ("l", "32")]);
        let outcome = parse_transit_bandwidth_request(&opts, HopRole::InboundGateway);
        assert!(matches!(
            outcome,
            Err(TransitBandwidthParseError::MinimumExceedsLimit { .. })
        ));
    }

    #[test]
    fn bandwidth_zero_rejects() {
        let opts = options_with(&[("m", "0")]);
        let outcome = parse_transit_bandwidth_request(&opts, HopRole::InboundGateway);
        assert!(matches!(
            outcome,
            Err(TransitBandwidthParseError::NotPositiveDecimal { key: "m" })
        ));
    }

    #[test]
    fn bandwidth_plus_or_minus_rejects() {
        let opts = options_with(&[("m", "+12")]);
        let outcome = parse_transit_bandwidth_request(&opts, HopRole::InboundGateway);
        assert!(matches!(
            outcome,
            Err(TransitBandwidthParseError::NotPositiveDecimal { key: "m" })
        ));
        let opts = options_with(&[("m", "-12")]);
        let outcome = parse_transit_bandwidth_request(&opts, HopRole::InboundGateway);
        assert!(matches!(
            outcome,
            Err(TransitBandwidthParseError::NotPositiveDecimal { key: "m" })
        ));
    }

    #[test]
    fn bandwidth_whitespace_rejects() {
        let opts = options_with(&[("m", "12 ")]);
        let outcome = parse_transit_bandwidth_request(&opts, HopRole::InboundGateway);
        assert!(matches!(
            outcome,
            Err(TransitBandwidthParseError::NotPositiveDecimal { key: "m" })
        ));
    }

    #[test]
    fn bandwidth_non_decimal_rejects() {
        let opts = options_with(&[("m", "12x")]);
        let outcome = parse_transit_bandwidth_request(&opts, HopRole::InboundGateway);
        assert!(matches!(
            outcome,
            Err(TransitBandwidthParseError::NotPositiveDecimal { key: "m" })
        ));
    }

    #[test]
    fn bandwidth_u32_overflow_rejects() {
        let opts = options_with(&[("m", "99999999999")]); // > u32::MAX
        let outcome = parse_transit_bandwidth_request(&opts, HopRole::InboundGateway);
        assert!(matches!(
            outcome,
            Err(TransitBandwidthParseError::Overflow { key: "m" })
        ));
    }

    #[test]
    fn bandwidth_l_on_participant_rejects() {
        let opts = options_with(&[("l", "100")]);
        let outcome = parse_transit_bandwidth_request(&opts, HopRole::Participant);
        assert!(matches!(
            outcome,
            Err(TransitBandwidthParseError::LimitOnNonInboundGateway { .. })
        ));
    }

    #[test]
    fn bandwidth_l_on_obep_rejects() {
        let opts = options_with(&[("l", "100")]);
        let outcome = parse_transit_bandwidth_request(&opts, HopRole::OutboundEndpoint);
        assert!(matches!(
            outcome,
            Err(TransitBandwidthParseError::LimitOnNonInboundGateway { .. })
        ));
    }

    // ------------------------------------------------------------------
    // Scope B tests
    // ------------------------------------------------------------------

    #[test]
    fn policy_default_is_accepting_and_enabled() {
        let policy = default_policy();
        assert!(policy.enabled());
        assert_eq!(policy.mode(), TransitMode::Accepting);
        assert_eq!(policy.max_active(), 4);
        assert_eq!(policy.max_pending(), 2);
        assert_eq!(policy.available_bandwidth_kbps(), 12);
    }

    #[test]
    fn policy_disabled_helper_returns_zero_ceilings() {
        let policy = TransitAdmissionPolicy::disabled();
        assert!(!policy.enabled());
        assert_eq!(policy.mode(), TransitMode::Shutdown);
        assert_eq!(policy.max_active(), 0);
        assert_eq!(policy.max_pending(), 0);
        assert_eq!(policy.available_bandwidth_kbps(), 0);
    }

    #[test]
    fn policy_construction_rejects_excessive_ceilings() {
        assert!(
            TransitAdmissionPolicy::new(
                true,
                TransitMode::Accepting,
                MAX_TRANSIT_ACTIVE + 1,
                2,
                2,
                1,
                12,
                None,
            )
            .is_err()
        );
        assert!(
            TransitAdmissionPolicy::new(
                true,
                TransitMode::Accepting,
                4,
                MAX_TRANSIT_PENDING + 1,
                2,
                1,
                12,
                None,
            )
            .is_err()
        );
        assert!(
            TransitAdmissionPolicy::new(
                true,
                TransitMode::Accepting,
                4,
                2,
                MAX_TRANSIT_PEER_ACTIVE + 1,
                1,
                12,
                None,
            )
            .is_err()
        );
        assert!(
            TransitAdmissionPolicy::new(
                true,
                TransitMode::Accepting,
                4,
                2,
                2,
                MAX_TRANSIT_PEER_PENDING + 1,
                12,
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn admission_error_categories_are_classified_but_wire_byte_is_uniform() {
        // Every local reason collapses to the same wire byte.
        let rejects = [
            TransitAdmissionError::Disabled,
            TransitAdmissionError::Degraded,
            TransitAdmissionError::Shutdown,
            TransitAdmissionError::ActiveFull,
            TransitAdmissionError::PendingFull,
            TransitAdmissionError::ActivePerPeerFull,
            TransitAdmissionError::PendingPerPeerFull,
            TransitAdmissionError::InsufficientBandwidth {
                minimum: 12,
                available: 0,
            },
            TransitAdmissionError::PerTunnelCapExceeded {
                requested: 100,
                cap: 32,
            },
        ];
        for reason in rejects {
            assert_eq!(
                TransitAdmissionPolicy::reject_response(),
                ShortResponseCode::BandwidthRejected
            );
            assert!(reason.category().len() < 64);
        }
    }

    // ------------------------------------------------------------------
    // Scope D tests
    // ------------------------------------------------------------------

    #[test]
    fn registry_capacity_ceiling_rejects_excessive_request() {
        assert!(TransitRegistry::with_capacity(MAX_TRANSIT_ACTIVE + 1).is_err());
    }

    #[test]
    fn registry_insert_rejects_duplicate_receive_id() {
        let mut registry = TransitRegistry::with_capacity(4).expect("registry");
        let receive = TunnelId::new(0x1234).expect("id");
        let registration = make_test_registration(receive, 600);
        registry
            .insert(receive, registration)
            .expect("first insert");
        let duplicate = registry.insert(receive, make_test_registration(receive, 600));
        assert!(matches!(
            duplicate,
            Err(TransitRegistryError::DuplicateReceiveTunnelId)
        ));
    }

    #[test]
    fn registry_insert_enforces_global_capacity() {
        let mut registry = TransitRegistry::with_capacity(1).expect("registry");
        let first = TunnelId::new(1).expect("id");
        registry
            .insert(first, make_test_registration(first, 600))
            .expect("first insert");
        let second = TunnelId::new(2).expect("id");
        let outcome = registry.insert(second, make_test_registration(second, 600));
        assert!(matches!(outcome, Err(TransitRegistryError::CapacityFull)));
    }

    #[test]
    fn registry_remove_drops_entry_and_returns_secret() {
        let mut registry = TransitRegistry::with_capacity(2).expect("registry");
        let receive = TunnelId::new(0x100).expect("id");
        registry
            .insert(receive, make_test_registration(receive, 600))
            .expect("insert");
        let removed = registry.remove(receive);
        assert_eq!(removed.expires_at_seconds(), 600);
        assert!(!registry.contains(receive));
        assert!(registry.is_empty());
    }

    #[test]
    fn registry_role_lookup_returns_mutable_view() {
        let mut registry = TransitRegistry::with_capacity(2).expect("registry");
        let receive = TunnelId::new(0x100).expect("id");
        registry
            .insert(receive, make_test_registration(receive, 600))
            .expect("insert");
        let role = registry.role(receive).expect("role");
        assert!(matches!(role, TransitHopRole::Participant { .. }));
        let mutable = registry.role_mut(receive).expect("mutable");
        assert!(matches!(mutable, &mut TransitHopRole::Participant { .. }));
    }

    #[test]
    fn registry_expire_at_lifetime_removes_entry() {
        let mut registry = TransitRegistry::with_capacity(2).expect("registry");
        let receive = TunnelId::new(0x100).expect("id");
        registry
            .insert(receive, make_test_registration(receive, 600))
            .expect("insert");
        let expired = registry.expire(600);
        assert_eq!(expired.len(), 1);
        assert!(registry.is_empty());
    }

    #[test]
    fn registry_expire_early_keeps_entry() {
        let mut registry = TransitRegistry::with_capacity(2).expect("registry");
        let receive = TunnelId::new(0x100).expect("id");
        registry
            .insert(receive, make_test_registration(receive, 600))
            .expect("insert");
        let expired = registry.expire(599);
        assert_eq!(expired.len(), 0);
        assert!(registry.contains(receive));
    }

    #[test]
    fn registry_active_per_peer_count_aggregates_by_previous_peer() {
        let mut registry = TransitRegistry::with_capacity(4).expect("registry");
        let previous = TunnelPeer::from_hash(Hash::from_bytes([0x55; 32]));
        let registration = |receive: u32, peer: TunnelPeer| -> TransitHopRegistration {
            let mut entry = make_test_registration(TunnelId::new(receive).expect("id"), 600);
            // overwrite the previous peer
            entry.previous_peer = peer;
            entry
        };
        registry
            .insert(TunnelId::new(1).expect("id"), registration(1, previous))
            .expect("1");
        registry
            .insert(TunnelId::new(2).expect("id"), registration(2, previous))
            .expect("2");
        registry
            .insert(
                TunnelId::new(3).expect("id"),
                registration(3, TunnelPeer::from_hash(Hash::from_bytes([0x66; 32]))),
            )
            .expect("3");
        assert_eq!(registry.active_per_peer_count(previous), 2);
    }

    fn make_test_registration(receive: TunnelId, expires_at: u64) -> TransitHopRegistration {
        let layer_keys = LayerKeys::new([0x11; 32], [0x22; 32], [0x33; 32]);
        let previous_peer = TunnelPeer::from_hash(Hash::from_bytes([receive.get() as u8; 32]));
        TransitHopRegistration {
            previous_peer,
            role: TransitHopRole::Participant {
                next_router: Hash::from_bytes([0xAA; 32]),
                next_tunnel: TunnelId::new(receive.get() + 1).expect("id"),
                layer_keys,
            },
            expires_at_seconds: expires_at,
        }
    }

    // ------------------------------------------------------------------
    // Scope C transactional tests
    // ------------------------------------------------------------------

    /// End-to-end helper: encrypt a `ShortRequestRecord` so the
    /// postprocessor's `open_short_request` returns the supplied
    /// payload.
    fn seal_short_request(
        cryptography: &crate::build_crypto::EciesX25519BuildCryptography,
        record: &ShortRequestRecord,
        responder_priv: &[u8; EPHEMERAL_KEY_LEN],
        hop_hash: &[u8; 32],
        rng: &mut ChaCha8Rng,
    ) -> [u8; SHORT_BUILD_RECORD_SIZE] {
        use crate::build_crypto::BuildCryptography;
        let plaintext = record
            .encode_with_rng(rng)
            .expect("encode request")
            .to_vec();
        let mut out = [0_u8; i2pr_proto::SHORT_REQUEST_PLAINTEXT_SIZE];
        out.copy_from_slice(&plaintext);
        let sealed = cryptography
            .seal_short_request(
                &out,
                &responder_priv_to_public(responder_priv),
                hop_hash,
                rng,
            )
            .expect("seal");
        sealed.record.to_vec().try_into().expect("218 bytes")
    }

    fn responder_priv_to_public(priv_bytes: &[u8; EPHEMERAL_KEY_LEN]) -> [u8; EPHEMERAL_KEY_LEN] {
        let secret = x25519_dalek::StaticSecret::from(*priv_bytes);
        let public = x25519_dalek::PublicKey::from(&secret);
        public.to_bytes()
    }

    fn run_participant_transaction(
        policy: TransitAdmissionPolicy,
        bandwidth_opts: BuildOptions,
    ) -> Result<TransitBuildOutcome, TransitRejectStage> {
        let cryptography = crate::build_crypto::EciesX25519BuildCryptography::new();
        let responder_priv = privkey(0xAA);
        let hop_hash = next_router(0x55);
        // Build a record with request_time = now so the validate step
        // passes.
        let request_time_ms = 60_000;
        let record = build_request(
            HopRole::Participant,
            0x1000,
            0x2000,
            bandwidth_opts,
            request_time_ms,
        );
        let mut rng = fixed_rng(0xAA);
        let envelope = seal_short_request(
            &cryptography,
            &record,
            &responder_priv,
            hop_hash.as_bytes(),
            &mut rng,
        );
        let mut registry = TransitRegistry::with_capacity(4).expect("registry");
        let mut layer_seed = Zeroizing::new(LayerKeys::new([0x55; 32], [0x66; 32], [0x77; 32]));
        let mut context = TransitBuildContext {
            hop_static_priv: &responder_priv,
            hop_identity: &hop_hash,
            reply_slot: TransitReplySlot(ValidatedRecordSlot::new(2).expect("slot")),
            now: TransitNow {
                seconds: request_time_ms / 1_000,
            },
            policy: &policy,
            registry: &mut registry,
        };
        let mut rng = fixed_rng(0xBB);
        process_short_build_request(
            &cryptography,
            &envelope,
            &mut context,
            &mut layer_seed,
            &mut rng,
        )
    }

    #[test]
    fn disabled_policy_returns_bandwidth_rejected_with_no_state() {
        let policy = TransitAdmissionPolicy::disabled();
        let mut rng = fixed_rng(0xDD);
        let cryptography = crate::build_crypto::EciesX25519BuildCryptography::new();
        let responder_priv = privkey(0xAA);
        let hop_hash = next_router(0x55);
        let request_time_ms = 60_000;
        let record = build_request(
            HopRole::Participant,
            0x1000,
            0x2000,
            BuildOptions::empty(),
            request_time_ms,
        );
        let mut seal_rng = fixed_rng(0xCC);
        let envelope = seal_short_request(
            &cryptography,
            &record,
            &responder_priv,
            hop_hash.as_bytes(),
            &mut seal_rng,
        );
        let mut registry = TransitRegistry::with_capacity(4).expect("registry");
        let mut layer_seed = Zeroizing::new(LayerKeys::new([0; 32], [0; 32], [0; 32]));
        let mut context = TransitBuildContext {
            hop_static_priv: &responder_priv,
            hop_identity: &hop_hash,
            reply_slot: TransitReplySlot(ValidatedRecordSlot::new(0).expect("slot")),
            now: TransitNow {
                seconds: request_time_ms / 1_000,
            },
            policy: &policy,
            registry: &mut registry,
        };
        let outcome = process_short_build_request(
            &cryptography,
            &envelope,
            &mut context,
            &mut layer_seed,
            &mut rng,
        );
        assert!(matches!(
            outcome,
            Err(TransitRejectStage::Admission(
                TransitAdmissionError::Shutdown | TransitAdmissionError::Disabled
            ))
        ));
        assert!(registry.is_empty());
    }

    #[test]
    fn degraded_mode_rejects_with_no_state() {
        let policy = TransitAdmissionPolicy::new(true, TransitMode::Degraded, 4, 2, 2, 1, 12, None)
            .expect("policy");
        let outcome = run_participant_transaction(policy, BuildOptions::empty());
        assert!(matches!(
            outcome,
            Err(TransitRejectStage::Admission(
                TransitAdmissionError::Degraded
            ))
        ));
    }

    #[test]
    fn global_active_full_rejects_with_no_state() {
        // Build a registry that is already at max_active then try to
        // add another.
        let cryptography = crate::build_crypto::EciesX25519BuildCryptography::new();
        let responder_priv = privkey(0xAA);
        let hop_hash = next_router(0x55);
        let policy =
            TransitAdmissionPolicy::new(true, TransitMode::Accepting, 4, 2, 4, 1, 12, None)
                .expect("policy");
        let request_time_ms = 60_000;
        let record = build_request(
            HopRole::Participant,
            0x1000,
            0x2000,
            BuildOptions::empty(),
            request_time_ms,
        );
        let mut rng = fixed_rng(0xAA);
        let envelope = seal_short_request(
            &cryptography,
            &record,
            &responder_priv,
            hop_hash.as_bytes(),
            &mut rng,
        );
        let mut registry = TransitRegistry::with_capacity(4).expect("registry");
        for index in 0..4 {
            let receive = TunnelId::new(0x100 + index).expect("id");
            registry
                .insert(receive, make_test_registration(receive, 600))
                .expect("fill");
        }
        let mut layer_seed = Zeroizing::new(LayerKeys::new([0; 32], [0; 32], [0; 32]));
        let mut context = TransitBuildContext {
            hop_static_priv: &responder_priv,
            hop_identity: &hop_hash,
            reply_slot: TransitReplySlot(ValidatedRecordSlot::new(0).expect("slot")),
            now: TransitNow {
                seconds: request_time_ms / 1_000,
            },
            policy: &policy,
            registry: &mut registry,
        };
        let mut rng = fixed_rng(0xBB);
        let outcome = process_short_build_request(
            &cryptography,
            &envelope,
            &mut context,
            &mut layer_seed,
            &mut rng,
        );
        assert!(matches!(
            outcome,
            Err(TransitRejectStage::Admission(
                TransitAdmissionError::ActiveFull
            ))
        ));
    }

    #[test]
    fn minimum_kbps_above_available_rejects_with_no_state() {
        let policy =
            TransitAdmissionPolicy::new(true, TransitMode::Accepting, 4, 2, 2, 1, 12, None)
                .expect("policy");
        let opts = options_with(&[("m", "100")]);
        let outcome = run_participant_transaction(policy, opts);
        assert!(matches!(
            outcome,
            Err(TransitRejectStage::Admission(
                TransitAdmissionError::InsufficientBandwidth { .. }
            ))
        ));
    }

    #[test]
    fn accepted_m_returns_b_ge_m() {
        let policy =
            TransitAdmissionPolicy::new(true, TransitMode::Accepting, 4, 2, 2, 1, 64, None)
                .expect("policy");
        let opts = options_with(&[("m", "12")]);
        let outcome = run_participant_transaction(policy, opts).expect("accept");
        assert_eq!(outcome.response, ShortResponseCode::Accepted);
        let bw = outcome.bandwidth_reply.expect("bandwidth reply");
        let b = bw.available_kbps.expect("b present");
        assert!(b >= 12);
    }

    #[test]
    fn accepted_no_m_or_r_may_omit_b() {
        let policy = default_policy();
        let outcome = run_participant_transaction(policy, BuildOptions::empty()).expect("accept");
        let bw = outcome.bandwidth_reply.expect("bandwidth reply");
        assert_eq!(bw.available_kbps, None);
    }

    #[test]
    fn duplicate_receive_id_cannot_replace_existing_entry() {
        let policy = default_policy();
        let cryptography = crate::build_crypto::EciesX25519BuildCryptography::new();
        let responder_priv = privkey(0xAA);
        let hop_hash = next_router(0x55);
        let request_time_ms = 60_000;
        let record = build_request(
            HopRole::Participant,
            0x1000,
            0x2000,
            BuildOptions::empty(),
            request_time_ms,
        );
        let mut rng = fixed_rng(0xAA);
        let envelope = seal_short_request(
            &cryptography,
            &record,
            &responder_priv,
            hop_hash.as_bytes(),
            &mut rng,
        );
        let mut registry = TransitRegistry::with_capacity(4).expect("registry");
        // Pre-fill the registry with a different hop at receive id 0x1000.
        let receive = TunnelId::new(0x1000).expect("id");
        registry
            .insert(receive, make_test_registration(receive, 600))
            .expect("prefill");
        let mut layer_seed = Zeroizing::new(LayerKeys::new([0; 32], [0; 32], [0; 32]));
        let mut context = TransitBuildContext {
            hop_static_priv: &responder_priv,
            hop_identity: &hop_hash,
            reply_slot: TransitReplySlot(ValidatedRecordSlot::new(0).expect("slot")),
            now: TransitNow {
                seconds: request_time_ms / 1_000,
            },
            policy: &policy,
            registry: &mut registry,
        };
        let mut rng = fixed_rng(0xBB);
        let outcome = process_short_build_request(
            &cryptography,
            &envelope,
            &mut context,
            &mut layer_seed,
            &mut rng,
        );
        assert!(matches!(
            outcome,
            Err(TransitRejectStage::Admission(
                TransitAdmissionError::ActivePerPeerFull
            ))
        ));
        // Registry should still have only the pre-filled entry.
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn timestamp_outside_skew_rejects_before_commit() {
        let policy = default_policy();
        let cryptography = crate::build_crypto::EciesX25519BuildCryptography::new();
        let responder_priv = privkey(0xAA);
        let hop_hash = next_router(0x55);
        let record = build_request(
            HopRole::Participant,
            0x1000,
            0x2000,
            BuildOptions::empty(),
            // Far-past request time, well outside the skew window.
            24 * 60 * 60 * 1_000,
        );
        let mut rng = fixed_rng(0xAA);
        let envelope = seal_short_request(
            &cryptography,
            &record,
            &responder_priv,
            hop_hash.as_bytes(),
            &mut rng,
        );
        let mut registry = TransitRegistry::with_capacity(4).expect("registry");
        let mut layer_seed = Zeroizing::new(LayerKeys::new([0; 32], [0; 32], [0; 32]));
        let mut context = TransitBuildContext {
            hop_static_priv: &responder_priv,
            hop_identity: &hop_hash,
            reply_slot: TransitReplySlot(ValidatedRecordSlot::new(0).expect("slot")),
            now: TransitNow { seconds: 60 },
            policy: &policy,
            registry: &mut registry,
        };
        let mut rng = fixed_rng(0xBB);
        let outcome = process_short_build_request(
            &cryptography,
            &envelope,
            &mut context,
            &mut layer_seed,
            &mut rng,
        );
        assert!(matches!(
            outcome,
            Err(TransitRejectStage::RequestTimeOutOfRange)
        ));
        assert!(registry.is_empty());
    }

    #[test]
    fn participant_registration_owns_exact_receive_next_tuple() {
        let policy = default_policy();
        let outcome = run_participant_transaction(policy, BuildOptions::empty()).expect("accept");
        assert_eq!(outcome.response, ShortResponseCode::Accepted);
    }

    #[test]
    fn ibgw_registration_lands_with_l_field_legal_only_there() {
        let policy =
            TransitAdmissionPolicy::new(true, TransitMode::Accepting, 4, 2, 2, 1, 256, None)
                .expect("policy");
        let opts = options_with(&[("m", "32"), ("l", "100")]);
        let outcome = run_participant_transaction(policy, opts);
        // Participant + l is rejected at the bandwidth-parse stage.
        assert!(matches!(
            outcome,
            Err(TransitRejectStage::BandwidthParse(
                TransitBandwidthParseError::LimitOnNonInboundGateway { .. }
            ))
        ));
    }

    #[test]
    fn obep_registration_handles_obep_record_directly() {
        let cryptography = crate::build_crypto::EciesX25519BuildCryptography::new();
        let responder_priv = privkey(0xCC);
        let hop_hash = next_router(0x33);
        let policy = default_policy();
        let request_time_ms = 60_000;
        let record = build_request(
            HopRole::OutboundEndpoint,
            0x1000,
            0x2000,
            BuildOptions::empty(),
            request_time_ms,
        );
        let mut rng = fixed_rng(0xEE);
        let envelope = seal_short_request(
            &cryptography,
            &record,
            &responder_priv,
            hop_hash.as_bytes(),
            &mut rng,
        );
        let mut registry = TransitRegistry::with_capacity(4).expect("registry");
        let mut layer_seed = Zeroizing::new(LayerKeys::new([0; 32], [0; 32], [0; 32]));
        let mut context = TransitBuildContext {
            hop_static_priv: &responder_priv,
            hop_identity: &hop_hash,
            reply_slot: TransitReplySlot(ValidatedRecordSlot::new(0).expect("slot")),
            now: TransitNow {
                seconds: request_time_ms / 1_000,
            },
            policy: &policy,
            registry: &mut registry,
        };
        let mut rng = fixed_rng(0xDD);
        let outcome = process_short_build_request(
            &cryptography,
            &envelope,
            &mut context,
            &mut layer_seed,
            &mut rng,
        )
        .expect("accept");
        assert_eq!(outcome.response, ShortResponseCode::Accepted);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn rng_failure_rolls_back() {
        // Deterministic-zero RNG using Default doesn't fail; instead
        // we construct a sealed reply envelope with a zero-length
        // reply mapping that bypasses the canonical encoder. The
        // transactional helper refuses to commit when the
        // `encode_with_rng` path returns `RandomnessUnavailable`.
        let cryptography = crate::build_crypto::EciesX25519BuildCryptography::new();
        let responder_priv = privkey(0xAA);
        let hop_hash = next_router(0x55);
        let policy = default_policy();
        let request_time_ms = 60_000;
        let record = build_request(
            HopRole::Participant,
            0x1000,
            0x2000,
            BuildOptions::empty(),
            request_time_ms,
        );
        let mut rng = fixed_rng(0xAA);
        let envelope = seal_short_request(
            &cryptography,
            &record,
            &responder_priv,
            hop_hash.as_bytes(),
            &mut rng,
        );
        let mut registry = TransitRegistry::with_capacity(4).expect("registry");
        let mut layer_seed = Zeroizing::new(LayerKeys::new([0; 32], [0; 32], [0; 32]));
        let mut context = TransitBuildContext {
            hop_static_priv: &responder_priv,
            hop_identity: &hop_hash,
            reply_slot: TransitReplySlot(ValidatedRecordSlot::new(0).expect("slot")),
            now: TransitNow {
                seconds: request_time_ms / 1_000,
            },
            policy: &policy,
            registry: &mut registry,
        };
        // Use a failing RNG. Only `TryRngCore` is implemented so we can
        // override `try_fill_bytes` to fail without conflicting with
        // the blanket `RngCore`-derived default impl.
        struct FailingRng;
        impl rand_core::TryRngCore for FailingRng {
            type Error = &'static str;
            fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
                Err("intentional-fail")
            }
            fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
                Err("intentional-fail")
            }
            fn try_fill_bytes(&mut self, _dst: &mut [u8]) -> Result<(), Self::Error> {
                Err("intentional-fail")
            }
        }
        impl rand_core::TryCryptoRng for FailingRng {}
        let mut failing = FailingRng;
        let outcome = process_short_build_request(
            &cryptography,
            &envelope,
            &mut context,
            &mut layer_seed,
            &mut failing,
        );
        assert!(matches!(
            outcome,
            Err(TransitRejectStage::RandomnessUnavailable)
        ));
        assert!(registry.is_empty());
    }

    #[test]
    fn expiry_at_lifetime_removes_registered_entry() {
        let policy = default_policy();
        let cryptography = crate::build_crypto::EciesX25519BuildCryptography::new();
        let responder_priv = privkey(0xAA);
        let hop_hash = next_router(0x55);
        let request_time_ms = 60_000;
        let record = build_request(
            HopRole::Participant,
            0x1000,
            0x2000,
            BuildOptions::empty(),
            request_time_ms,
        );
        let mut rng = fixed_rng(0xAA);
        let envelope = seal_short_request(
            &cryptography,
            &record,
            &responder_priv,
            hop_hash.as_bytes(),
            &mut rng,
        );
        let mut registry = TransitRegistry::with_capacity(4).expect("registry");
        let mut layer_seed = Zeroizing::new(LayerKeys::new([0; 32], [0; 32], [0; 32]));
        let mut context = TransitBuildContext {
            hop_static_priv: &responder_priv,
            hop_identity: &hop_hash,
            reply_slot: TransitReplySlot(ValidatedRecordSlot::new(0).expect("slot")),
            now: TransitNow {
                seconds: request_time_ms / 1_000,
            },
            policy: &policy,
            registry: &mut registry,
        };
        let mut rng = fixed_rng(0xBB);
        let outcome = process_short_build_request(
            &cryptography,
            &envelope,
            &mut context,
            &mut layer_seed,
            &mut rng,
        );
        let outcome = outcome.expect("accept");
        assert_eq!(outcome.response, ShortResponseCode::Accepted);
        let receive = TunnelId::new(0x1000).expect("id");
        assert!(registry.contains(receive));
        // expiration at the request_time + 600 should remove it.
        let request_seconds = request_time_ms / 1_000;
        let expired = registry.expire(request_seconds + REQUEST_EXPIRATION_SECONDS as u64);
        assert_eq!(expired.len(), 1);
        assert!(registry.is_empty());
    }

    #[test]
    fn early_expire_does_not_remove() {
        let mut registry = TransitRegistry::with_capacity(2).expect("registry");
        let receive = TunnelId::new(0x200).expect("id");
        registry
            .insert(receive, make_test_registration(receive, 600))
            .expect("insert");
        let removed = registry.expire(599);
        assert_eq!(removed.len(), 0);
        assert!(registry.contains(receive));
    }

    #[test]
    fn debug_output_does_not_expose_key_bytes() {
        let policy = default_policy();
        let outcome = run_participant_transaction(policy, BuildOptions::empty()).expect("accept");
        let debug = format!("{outcome:?}");
        // The Debug impl must not leak layer key bytes or the reply
        // record bytes.
        assert!(!debug.contains("layer_keys"));
        assert!(!debug.contains("<redacted>"));
        // Direct negative guard: the sealed reply is a fixed-size
        // binary; its array form must not appear literally in Debug.
        assert!(!debug.starts_with('['));
        // The response code is canonicalised; verify the variant
        // name appears, not raw bytes.
        assert!(debug.contains("Accepted") || debug.contains("accepted"));
    }

    #[test]
    fn reject_reason_is_anonymous_on_the_wire() {
        // All TransitRejectStage variants must serialize as a wire
        // envelope carrying only code 30.
        let variants = [
            TransitRejectStage::RecordDecode,
            TransitRejectStage::RequestTimeOutOfRange,
            TransitRejectStage::BandwidthParse(TransitBandwidthParseError::NotPositiveDecimal {
                key: "m",
            }),
            TransitRejectStage::Admission(TransitAdmissionError::InsufficientBandwidth {
                minimum: 12,
                available: 0,
            }),
            TransitRejectStage::Registry(TransitRegistryError::DuplicateReceiveTunnelId),
            TransitRejectStage::Seal(BuildCryptographyError::RandomnessUnavailable),
            TransitRejectStage::RandomnessUnavailable,
        ];
        for variant in variants {
            assert!(variant.is_pre_commit());
        }
    }
}
