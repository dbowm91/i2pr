//! Plan 492: the operator-opted, bounded, revocable transit participation
//! posture.
//!
//! This module owns the *posture*, not the protocol. The transit state
//! machine ([`i2pr_tunnel::transit`]) and the transit ingress owner
//! ([`crate::transit_owner`]) are closed surfaces; what was missing was a
//! policy that answers four questions:
//!
//! 1. **Who may turn participation on?** Only the operator, through the
//!    [`crate::config::TransitConfig`] surface. The posture is **off by
//!    default** and the default configuration does not change.
//! 2. **How much?** A bounded share of the router's *enforced* bandwidth
//!    share, a bounded number of participating tunnels, and a bounded
//!    per-peer participation cap.
//! 3. **For how long?** Until the operator changes the posture, the
//!    governor's permit is withdrawn, or health is lost. Each stop is
//!    prompt and recorded.
//! 4. **What happened?** The receipts carry what was relayed, how much, the
//!    refusals by reason, and when participation stopped.
//!
//! ## Enforcement, not advice
//!
//! The share limit is enforced against the governor's permitted share. The
//! owner holds the [`BandwidthEnforcementPermit`] that proves the share is
//! enforced and **revalidates it at the point of use**: a stale, withdrawn,
//! or unhealthy permit revokes participation rather than letting it grow.
//! Without an enforced permit there is no share, so every request is
//! refused with [`TransitRefusalReason::NoEnforcedPermit`].
//!
//! ## The counts are the data plane's, not a second ledger
//!
//! The participating-tunnel count and the per-peer count are **supplied by
//! the transit service**, which owns the authoritative registration
//! registry. The owner therefore never keeps a second, diverging count of
//! live tunnels: a tunnel that expires, a session that closes, and a
//! registration that is rolled back are all reflected immediately, because
//! the number the owner reasons about *is* the registry's number.
//!
//! ## What is *not* here
//!
//! - No socket, no timer, no task. The owner is a synchronous, single-writer
//!   accounting surface; the runtime owner supplies the clock.
//! - No peer identity in a receipt. The per-peer admission record is held
//!   in memory to bound the router's participation breadth, and the receipts
//!   carry counts only — no router hash, no address, no tunnel id, no key,
//!   no payload.
//! - No persistence. A restart re-derives the accounting from zero and
//!   starts from the configured posture; it never resumes a share counter
//!   mid-window.
//!
//! ```text
//! policy change ─┐
//! permit withdrawn ─┼─> revoke(reason) ─> stop accepting ─> receipt(Stopped)
//! health loss ─┘
//!
//! admit(peer, active, active_for_peer) ─> refused(reason) ─> counted, receipt(Refused)
//! admit(peer, active, active_for_peer) ─> admitted        ─> counted, receipt(Admitted)
//! charge(bytes)                        ─> over share      ─> refused, receipt(Refused)
//! ```

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::collections::VecDeque;

use i2pr_proto::Hash;
use i2pr_runtime::{BandwidthEnforcementPermit, BandwidthGovernor};
use thiserror::Error;

use crate::config::{
    DEFAULT_TRANSIT_PARTICIPATING_TUNNELS, DEFAULT_TRANSIT_PEER_PARTICIPATION_CAP,
    DEFAULT_TRANSIT_SHARE_LIMIT, MAX_TRANSIT_PARTICIPATING_TUNNELS,
    MAX_TRANSIT_PEER_PARTICIPATION_CAP, MAX_TRANSIT_SHARE_LIMIT_PERCENT, TransitConfig,
};

/// The maximum number of participation receipts the owner retains.
///
/// The log is bounded: the oldest receipt is dropped when the bound is
/// reached. A receipt is evidence of a decision, not a durable log.
pub const MAX_TRANSIT_PARTICIPATION_RECEIPTS: usize = 256;

/// The maximum number of peers the participation record tracks.
///
/// The bound keeps the record from growing with the peer set. The
/// least-recently-admitted peer is evicted when the bound is reached.
pub const MAX_TRANSIT_PARTICIPATION_TRACKED_PEERS: usize = 4_096;

/// The length of the share accounting window, in seconds.
///
/// The window matches the governor's per-second rate accounting: a window
/// may spend at most the permitted share, and the next window starts from
/// zero.
pub const TRANSIT_SHARE_WINDOW_SECONDS: u64 = 1;

/// Why one transit request was refused.
///
/// Every refusal is counted by reason. A refused request is never a silent
/// skip: the caller sees the typed reason and the receipts carry the count.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TransitRefusalReason {
    /// Participation is not enabled by the operator's posture.
    ParticipationDisabled,
    /// Participation was revoked and the posture has not been re-enabled.
    Revoked,
    /// The router-wide participating-tunnel count is at its limit.
    ParticipatingTunnelLimit,
    /// The peer already holds the maximum number of participating tunnels.
    PeerParticipationCap,
    /// The share budget for the current window is exhausted.
    ShareLimitExceeded,
    /// No enforced bandwidth permit is held, so the share limit could not
    /// be enforced.
    NoEnforcedPermit,
}

impl TransitRefusalReason {
    /// A stable, non-sensitive label for receipts and evidence files.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::ParticipationDisabled => "participation-disabled",
            Self::Revoked => "revoked",
            Self::ParticipatingTunnelLimit => "participating-tunnel-limit",
            Self::PeerParticipationCap => "peer-participation-cap",
            Self::ShareLimitExceeded => "share-limit-exceeded",
            Self::NoEnforcedPermit => "no-enforced-permit",
        }
    }
}

/// Why participation stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitRevocationReason {
    /// The operator changed the posture.
    PolicyChanged,
    /// The governor's permit was withdrawn, replaced, or became stale.
    PermitWithdrawn,
    /// The owner lost health, so the enforced share could not be trusted.
    HealthLost,
}

impl TransitRevocationReason {
    /// A stable, non-sensitive label for receipts and evidence files.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::PolicyChanged => "policy-changed",
            Self::PermitWithdrawn => "permit-withdrawn",
            Self::HealthLost => "health-lost",
        }
    }
}

/// The bounded counters of the counted refusals.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransitRefusalCounts {
    /// Requests refused because participation is not enabled.
    pub participation_disabled: u64,
    /// Requests refused because participation was revoked.
    pub revoked: u64,
    /// Requests refused because the participating-tunnel count is at its
    /// limit.
    pub participating_tunnel_limit: u64,
    /// Requests refused because the peer's participation cap is reached.
    pub peer_participation_cap: u64,
    /// Relays refused because the share budget was exhausted.
    pub share_limit_exceeded: u64,
    /// Requests refused because no enforced permit was held.
    pub no_enforced_permit: u64,
}

impl TransitRefusalCounts {
    /// Records one refusal by reason.
    pub fn record(&mut self, reason: TransitRefusalReason) {
        let counter = match reason {
            TransitRefusalReason::ParticipationDisabled => &mut self.participation_disabled,
            TransitRefusalReason::Revoked => &mut self.revoked,
            TransitRefusalReason::ParticipatingTunnelLimit => &mut self.participating_tunnel_limit,
            TransitRefusalReason::PeerParticipationCap => &mut self.peer_participation_cap,
            TransitRefusalReason::ShareLimitExceeded => &mut self.share_limit_exceeded,
            TransitRefusalReason::NoEnforcedPermit => &mut self.no_enforced_permit,
        };
        *counter = counter.saturating_add(1);
    }

    /// Returns the count recorded for one reason.
    pub const fn count(&self, reason: TransitRefusalReason) -> u64 {
        match reason {
            TransitRefusalReason::ParticipationDisabled => self.participation_disabled,
            TransitRefusalReason::Revoked => self.revoked,
            TransitRefusalReason::ParticipatingTunnelLimit => self.participating_tunnel_limit,
            TransitRefusalReason::PeerParticipationCap => self.peer_participation_cap,
            TransitRefusalReason::ShareLimitExceeded => self.share_limit_exceeded,
            TransitRefusalReason::NoEnforcedPermit => self.no_enforced_permit,
        }
    }

    /// Returns the total number of counted refusals.
    pub const fn total(&self) -> u64 {
        self.participation_disabled
            .saturating_add(self.revoked)
            .saturating_add(self.participating_tunnel_limit)
            .saturating_add(self.peer_participation_cap)
            .saturating_add(self.share_limit_exceeded)
            .saturating_add(self.no_enforced_permit)
    }
}

/// The kind of event one receipt records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitReceiptKind {
    /// A transit request was admitted.
    Admitted,
    /// A transit request or relay was refused.
    Refused,
    /// Relayed bytes were charged against the share budget.
    Relayed,
    /// Participation stopped.
    Stopped,
}

impl TransitReceiptKind {
    /// A stable, non-sensitive label for receipts and evidence files.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::Refused => "refused",
            Self::Relayed => "relayed",
            Self::Stopped => "stopped",
        }
    }
}

/// One participation receipt.
///
/// The receipt carries **counts only**. It never names a peer, a router
/// hash, an address, a tunnel id, a key, or a payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitParticipationReceipt {
    /// The kind of event.
    pub kind: TransitReceiptKind,
    /// The refusal reason, when the event is a refusal.
    pub reason: Option<TransitRefusalReason>,
    /// The number of tunnels this router participates in after the event.
    pub participating_tunnels: usize,
    /// The number of distinct peers this router has admitted.
    pub peers_participating: usize,
    /// The bytes relayed so far.
    pub bytes_relayed: u64,
    /// The counted refusals by reason.
    pub refusals: TransitRefusalCounts,
    /// The number of times participation stopped.
    pub stops: u64,
}

/// The bounded, privacy-safe snapshot of the participation owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitParticipationSnapshot {
    /// Whether the operator's posture enables participation.
    pub enabled: bool,
    /// Whether participation was revoked and not re-enabled.
    pub revoked: bool,
    /// The reason participation stopped, when it did.
    pub revocation: Option<TransitRevocationReason>,
    /// The number of tunnels this router participates in.
    pub participating_tunnels: usize,
    /// The number of distinct peers this router has admitted.
    pub peers_participating: usize,
    /// The bytes relayed so far.
    pub bytes_relayed: u64,
    /// The number of admitted requests.
    pub admitted: u64,
    /// The number of times participation stopped.
    pub stops: u64,
    /// The counted refusals by reason.
    pub refusals: TransitRefusalCounts,
    /// The configured share limit, in percent of the enforced share.
    pub share_limit_percent: u8,
    /// The effective share budget in bytes per second. Zero when no
    /// enforced permit is held.
    pub share_bytes_per_second: u64,
}

/// The policy the participation owner enforces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitParticipationPolicy {
    /// The percentage of the enforced share the posture may consume.
    pub share_limit_percent: u8,
    /// The maximum number of tunnels this router participates in.
    pub participating_tunnels: usize,
    /// The maximum number of participating tunnels one peer may hold.
    pub peer_participation_cap: usize,
}

impl Default for TransitParticipationPolicy {
    fn default() -> Self {
        Self {
            share_limit_percent: DEFAULT_TRANSIT_SHARE_LIMIT,
            participating_tunnels: DEFAULT_TRANSIT_PARTICIPATING_TUNNELS,
            peer_participation_cap: DEFAULT_TRANSIT_PEER_PARTICIPATION_CAP,
        }
    }
}

/// The policy is outside the supported bounds.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum TransitParticipationError {
    /// The share limit is zero or above the ceiling.
    #[error("the transit share limit must be between 1 and 100 percent")]
    InvalidShareLimit,
    /// The participating-tunnel count is zero or above the ceiling.
    #[error("the transit participating-tunnel count must be positive and bounded")]
    InvalidParticipatingTunnels,
    /// The per-peer participation cap is zero, above the ceiling, or larger
    /// than the router-wide count.
    #[error(
        "the transit per-peer participation cap must be positive, bounded, and no larger than the participating-tunnel count"
    )]
    InvalidPeerParticipationCap,
}

impl TransitParticipationPolicy {
    /// Builds a policy from explicit values, rejecting anything outside the
    /// named bounds.
    pub const fn new(
        share_limit_percent: u8,
        participating_tunnels: usize,
        peer_participation_cap: usize,
    ) -> Result<Self, TransitParticipationError> {
        if share_limit_percent == 0 || share_limit_percent > MAX_TRANSIT_SHARE_LIMIT_PERCENT {
            return Err(TransitParticipationError::InvalidShareLimit);
        }
        if participating_tunnels == 0 || participating_tunnels > MAX_TRANSIT_PARTICIPATING_TUNNELS {
            return Err(TransitParticipationError::InvalidParticipatingTunnels);
        }
        if peer_participation_cap == 0
            || peer_participation_cap > MAX_TRANSIT_PEER_PARTICIPATION_CAP
            || peer_participation_cap > participating_tunnels
        {
            return Err(TransitParticipationError::InvalidPeerParticipationCap);
        }
        Ok(Self {
            share_limit_percent,
            participating_tunnels,
            peer_participation_cap,
        })
    }

    /// Builds a policy from the normalized operator configuration.
    pub const fn from_config(config: &TransitConfig) -> Result<Self, TransitParticipationError> {
        Self::new(
            config.share_limit,
            config.participating_tunnels,
            config.peer_participation_cap,
        )
    }
}

/// The daemon-side owner of the transit participation posture.
///
/// The owner is a synchronous, single-writer accounting surface. It owns no
/// socket, no timer, and no task; the runtime owner supplies the clock and
/// calls the admission and charge methods at the point of use.
pub struct TransitParticipationOwner {
    policy: TransitParticipationPolicy,
    enabled: bool,
    revoked: Option<TransitRevocationReason>,
    permit: Option<BandwidthEnforcementPermit>,
    share_bytes_per_second: u64,
    window_started_at_seconds: u64,
    window_bytes: u64,
    participating_tunnels: usize,
    admitted_peers: BTreeMap<Hash, u64>,
    admitted: u64,
    stops: u64,
    bytes_relayed: u64,
    refusals: TransitRefusalCounts,
    receipts: VecDeque<TransitParticipationReceipt>,
}

impl std::fmt::Debug for TransitParticipationOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TransitParticipationOwner")
            .field("policy", &self.policy)
            .field("enabled", &self.enabled)
            .field("revoked", &self.revoked)
            .field("has_permit", &self.permit.is_some())
            .field("share_bytes_per_second", &self.share_bytes_per_second)
            .field("participating_tunnels", &self.participating_tunnels)
            .field("peers_participating", &self.admitted_peers.len())
            .field("bytes_relayed", &self.bytes_relayed)
            .field("admitted", &self.admitted)
            .field("stops", &self.stops)
            .field("refusals", &self.refusals)
            .field("receipts", &self.receipts.len())
            .finish_non_exhaustive()
    }
}

impl TransitParticipationOwner {
    /// Constructs the **disabled** owner: the default posture.
    ///
    /// The disabled owner refuses every request with
    /// [`TransitRefusalReason::ParticipationDisabled`] and holds no permit,
    /// no counters beyond the refusal count, and no state.
    pub fn disabled() -> Self {
        Self::disabled_with_policy(TransitParticipationPolicy::default())
    }

    /// Constructs the **disabled** owner while retaining the operator's
    /// configured limits, so a disabled posture still reports the limits it
    /// would apply.
    pub fn disabled_with_policy(policy: TransitParticipationPolicy) -> Self {
        Self {
            policy,
            enabled: false,
            revoked: None,
            permit: None,
            share_bytes_per_second: 0,
            window_started_at_seconds: 0,
            window_bytes: 0,
            participating_tunnels: 0,
            admitted_peers: BTreeMap::new(),
            admitted: 0,
            stops: 0,
            bytes_relayed: 0,
            refusals: TransitRefusalCounts::default(),
            receipts: VecDeque::new(),
        }
    }

    /// Constructs the owner from the operator's normalized configuration.
    ///
    /// The configuration is the single source of the posture: when
    /// `participation` is off — the default — the owner is the disabled
    /// owner, and the configured limits are retained for diagnostics only.
    pub fn from_config(
        config: &TransitConfig,
        governor: Option<&BandwidthGovernor>,
        operator_share_percent: u8,
    ) -> Result<Self, TransitParticipationError> {
        let policy = TransitParticipationPolicy::from_config(config)?;
        if !config.participation {
            return Ok(Self::disabled_with_policy(policy));
        }
        Self::new(policy, governor, operator_share_percent)
    }

    /// Constructs the owner from the operator's posture and the enforced
    /// bandwidth share.
    ///
    /// `governor` and `operator_share_percent` describe the share the
    /// router is *allowed* to spend on the network. The transit posture may
    /// use `policy.share_limit_percent` of it. When the governor is absent,
    /// unhealthy, or the operator's share is zero, **no permit is issued**:
    /// the owner then refuses every request rather than treating the share
    /// limit as advisory.
    pub fn new(
        policy: TransitParticipationPolicy,
        governor: Option<&BandwidthGovernor>,
        operator_share_percent: u8,
    ) -> Result<Self, TransitParticipationError> {
        // Re-validate the policy through the same constructor the
        // configuration path uses, so a policy built by hand cannot smuggle
        // an unbounded limit past the named constants.
        TransitParticipationPolicy::new(
            policy.share_limit_percent,
            policy.participating_tunnels,
            policy.peer_participation_cap,
        )?;
        let (permit, share_bytes_per_second) = match governor {
            Some(governor) => match governor.capacity_permit(operator_share_percent) {
                Some(permit) => {
                    let (_, outbound, share) = permit.configured_limits();
                    let enforced = outbound
                        .saturating_mul(u64::from(share))
                        .saturating_div(100);
                    let transit = enforced
                        .saturating_mul(u64::from(policy.share_limit_percent))
                        .saturating_div(100);
                    (Some(permit), transit)
                }
                None => (None, 0),
            },
            None => (None, 0),
        };
        Ok(Self {
            policy,
            enabled: true,
            revoked: None,
            permit,
            share_bytes_per_second,
            window_started_at_seconds: 0,
            window_bytes: 0,
            participating_tunnels: 0,
            admitted_peers: BTreeMap::new(),
            admitted: 0,
            stops: 0,
            bytes_relayed: 0,
            refusals: TransitRefusalCounts::default(),
            receipts: VecDeque::new(),
        })
    }

    /// Returns the enforced policy.
    pub const fn policy(&self) -> &TransitParticipationPolicy {
        &self.policy
    }

    /// Returns whether the operator's posture enables participation.
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns whether participation was revoked and not re-enabled.
    pub const fn is_revoked(&self) -> bool {
        self.revoked.is_some()
    }

    /// Returns the reason participation stopped, when it did.
    pub const fn revocation(&self) -> Option<TransitRevocationReason> {
        self.revoked
    }

    /// Returns the effective share budget in bytes per second.
    pub const fn share_bytes_per_second(&self) -> u64 {
        self.share_bytes_per_second
    }

    /// Returns the number of tunnels this router participates in.
    pub const fn participating_tunnels(&self) -> usize {
        self.participating_tunnels
    }

    /// Returns the number of distinct peers this router has admitted.
    pub fn peers_participating(&self) -> usize {
        self.admitted_peers.len()
    }

    /// Returns the number of times participation stopped.
    pub const fn stops(&self) -> u64 {
        self.stops
    }

    /// Returns the counted refusals by reason.
    pub const fn refusals(&self) -> &TransitRefusalCounts {
        &self.refusals
    }

    /// Returns the bytes relayed so far.
    pub const fn bytes_relayed(&self) -> u64 {
        self.bytes_relayed
    }

    /// Returns the number of bytes charged in the current window.
    pub const fn window_bytes(&self) -> u64 {
        self.window_bytes
    }

    /// Returns the bounded, privacy-safe snapshot.
    pub fn snapshot(&self) -> TransitParticipationSnapshot {
        TransitParticipationSnapshot {
            enabled: self.enabled,
            revoked: self.revoked.is_some(),
            revocation: self.revoked,
            participating_tunnels: self.participating_tunnels,
            peers_participating: self.admitted_peers.len(),
            bytes_relayed: self.bytes_relayed,
            admitted: self.admitted,
            stops: self.stops,
            refusals: self.refusals,
            share_limit_percent: self.policy.share_limit_percent,
            share_bytes_per_second: self.share_bytes_per_second,
        }
    }

    /// Returns the retained receipts, oldest first.
    pub fn receipts(&self) -> Vec<TransitParticipationReceipt> {
        self.receipts.iter().copied().collect()
    }

    /// Returns the number of retained receipts.
    pub fn receipt_count(&self) -> usize {
        self.receipts.len()
    }

    /// Returns the refusal reason the current posture implies, when the
    /// posture cannot admit work.
    ///
    /// The order is the enforcement order: a disabled posture, a revoked
    /// posture, and a missing permit are three different facts and the
    /// caller is told which one applies.
    pub const fn current_refusal(&self) -> Option<TransitRefusalReason> {
        if !self.enabled {
            return Some(TransitRefusalReason::ParticipationDisabled);
        }
        if self.revoked.is_some() {
            return Some(TransitRefusalReason::Revoked);
        }
        if self.permit.is_none() {
            return Some(TransitRefusalReason::NoEnforcedPermit);
        }
        None
    }

    /// Applies an operator policy change.
    ///
    /// Turning participation off, or changing a limit, stops the current
    /// participation: the participating tunnels are released, the stop is
    /// counted, and the stop is recorded. The accounting is **not** reset by
    /// a policy change — only a restart re-derives it from zero.
    ///
    /// Returns `true` when the posture changed.
    pub fn apply_config(
        &mut self,
        config: &TransitConfig,
        governor: Option<&BandwidthGovernor>,
        operator_share_percent: u8,
    ) -> bool {
        let Ok(mut next) = Self::from_config(config, governor, operator_share_percent) else {
            // An invalid posture is refused rather than applied: the
            // current posture stays in force.
            return false;
        };
        let previous = (
            self.policy,
            self.enabled,
            self.revoked,
            self.share_bytes_per_second,
        );
        let next_posture = (
            next.policy,
            next.enabled,
            next.revoked,
            next.share_bytes_per_second,
        );
        // A policy change stops the current participation: the operator's
        // new posture is the authority, and the tunnels the old posture
        // admitted are released so the router does not keep serving under a
        // posture that no longer applies.
        let was_active = self.enabled || self.participating_tunnels > 0;
        next.admitted = self.admitted;
        next.stops = self.stops;
        next.bytes_relayed = self.bytes_relayed;
        next.refusals = self.refusals;
        next.receipts = std::mem::take(&mut self.receipts);
        next.window_started_at_seconds = self.window_started_at_seconds;
        next.window_bytes = self.window_bytes;
        *self = next;
        if was_active {
            self.stop(TransitRevocationReason::PolicyChanged);
        }
        previous != next_posture
    }

    /// Revokes participation for a typed reason.
    ///
    /// Revocation is prompt: the participating tunnels are released, the
    /// stop is counted, and the stop is recorded. It is idempotent — a
    /// second revocation of the same reason does not double-count.
    pub fn revoke(&mut self, reason: TransitRevocationReason) {
        if self.revoked == Some(reason) {
            return;
        }
        self.revoked = Some(reason);
        self.stop(reason);
    }

    /// Revalidates the governor's permit at the point of use.
    ///
    /// `governor` is the **live** governor the runtime owns. The permit was
    /// issued by one governor instance; when the live governor is a
    /// different instance the permit is stale, and when the issuing governor
    /// was dropped the permit is unhealthy. Either case revokes
    /// participation rather than letting the share grow.
    ///
    /// Returns `true` when participation may continue.
    pub fn revalidate(&mut self, governor: Option<&BandwidthGovernor>) -> bool {
        if !self.enabled {
            return false;
        }
        if self.revoked.is_some() {
            return false;
        }
        let Some(permit) = &self.permit else {
            self.revoke(TransitRevocationReason::PermitWithdrawn);
            return false;
        };
        // Health first: a dropped issuing governor is a health loss, not a
        // withdrawal, and the two are recorded differently.
        if !permit.is_healthy() {
            self.revoke(TransitRevocationReason::HealthLost);
            return false;
        }
        let Some(governor) = governor else {
            self.revoke(TransitRevocationReason::PermitWithdrawn);
            return false;
        };
        if !permit.is_current_for(governor) {
            self.revoke(TransitRevocationReason::PermitWithdrawn);
            return false;
        }
        true
    }

    /// Records one refusal by reason.
    ///
    /// The owner counts every refusal it reports, including the ones the
    /// caller reports after the permit revalidation itself refused: a
    /// refusal that is not counted is a silent skip.
    pub fn record_refusal(&mut self, reason: TransitRefusalReason) {
        self.refusals.record(reason);
        self.receipt(TransitReceiptKind::Refused, Some(reason));
    }

    /// Admits one transit request.
    ///
    /// `active_tunnels` and `active_for_peer` are the **authoritative**
    /// registration counts the transit service owns: the router-wide
    /// active registrations and the active registrations held by this peer.
    /// The owner never keeps a second count of live tunnels, so an expired,
    /// closed, or rolled-back registration is reflected immediately.
    ///
    /// The decision is fail-closed: an unknown, revoked, or exhausted
    /// posture refuses the request with a typed reason rather than letting
    /// the request through unaccounted.
    pub fn admit(
        &mut self,
        peer: &Hash,
        active_tunnels: usize,
        active_for_peer: usize,
        now_seconds: u64,
    ) -> Result<(), TransitRefusalReason> {
        if let Some(reason) = self.current_refusal() {
            return self.refuse(reason);
        }
        if active_tunnels >= self.policy.participating_tunnels {
            return self.refuse(TransitRefusalReason::ParticipatingTunnelLimit);
        }
        if active_for_peer >= self.policy.peer_participation_cap {
            return self.refuse(TransitRefusalReason::PeerParticipationCap);
        }
        self.roll_window(now_seconds);
        self.participating_tunnels = active_tunnels.saturating_add(1);
        self.track_peer(peer);
        self.admitted = self.admitted.saturating_add(1);
        self.receipt(TransitReceiptKind::Admitted, None);
        Ok(())
    }

    /// Charges relayed bytes against the share budget.
    ///
    /// The window is the governor's per-second accounting window: a window
    /// may spend at most the permitted share, and the next window starts
    /// from zero. Exceeding the share **refuses** the relay; it never grows
    /// the budget.
    pub fn charge_relay(
        &mut self,
        bytes: u64,
        now_seconds: u64,
    ) -> Result<(), TransitRefusalReason> {
        if let Some(reason) = self.current_refusal() {
            return self.refuse(reason);
        }
        self.roll_window(now_seconds);
        let charged = self.window_bytes.saturating_add(bytes);
        if charged > self.share_bytes_per_second {
            return self.refuse(TransitRefusalReason::ShareLimitExceeded);
        }
        self.window_bytes = charged;
        self.bytes_relayed = self.bytes_relayed.saturating_add(bytes);
        self.receipt(TransitReceiptKind::Relayed, None);
        Ok(())
    }

    fn track_peer(&mut self, peer: &Hash) {
        if self.admitted_peers.len() >= MAX_TRANSIT_PARTICIPATION_TRACKED_PEERS
            && !self.admitted_peers.contains_key(peer)
        {
            let oldest = self
                .admitted_peers
                .iter()
                .min_by_key(|(_, stamp)| **stamp)
                .map(|(key, _)| *key);
            if let Some(key) = oldest {
                self.admitted_peers.remove(&key);
            }
        }
        let stamp = self.admitted;
        self.admitted_peers.insert(*peer, stamp);
    }

    fn roll_window(&mut self, now_seconds: u64) {
        if now_seconds < self.window_started_at_seconds
            || now_seconds - self.window_started_at_seconds >= TRANSIT_SHARE_WINDOW_SECONDS
        {
            self.window_started_at_seconds = now_seconds;
            self.window_bytes = 0;
        }
    }

    fn refuse(&mut self, reason: TransitRefusalReason) -> Result<(), TransitRefusalReason> {
        self.record_refusal(reason);
        Err(reason)
    }

    fn stop(&mut self, reason: TransitRevocationReason) {
        self.participating_tunnels = 0;
        self.stops = self.stops.saturating_add(1);
        self.receipt(TransitReceiptKind::Stopped, None);
        let _ = reason;
    }

    fn receipt(&mut self, kind: TransitReceiptKind, reason: Option<TransitRefusalReason>) {
        let receipt = TransitParticipationReceipt {
            kind,
            reason,
            participating_tunnels: self.participating_tunnels,
            peers_participating: self.admitted_peers.len(),
            bytes_relayed: self.bytes_relayed,
            refusals: self.refusals,
            stops: self.stops,
        };
        if self.receipts.len() >= MAX_TRANSIT_PARTICIPATION_RECEIPTS {
            self.receipts.pop_front();
        }
        self.receipts.push_back(receipt);
    }
}

/// The participation posture a transit owner enforces.
///
/// The binding keeps the governor that issued the permit alive, so the
/// permit stays healthy while the owner is installed, and exposes that
/// governor as the **live** governor the owner revalidates against.
pub struct TransitParticipationBinding {
    owner: TransitParticipationOwner,
    governor: Option<BandwidthGovernor>,
}

impl std::fmt::Debug for TransitParticipationBinding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TransitParticipationBinding")
            .field("owner", &self.owner)
            .field("has_governor", &self.governor.is_some())
            .finish_non_exhaustive()
    }
}

impl TransitParticipationBinding {
    /// Binds a participation owner to the governor that issued its permit.
    pub fn new(owner: TransitParticipationOwner, governor: Option<BandwidthGovernor>) -> Self {
        Self { owner, governor }
    }

    /// The **disabled** binding: the default posture.
    pub fn disabled() -> Self {
        Self::new(TransitParticipationOwner::disabled(), None)
    }

    /// Returns the participation owner.
    pub const fn owner(&self) -> &TransitParticipationOwner {
        &self.owner
    }

    /// Returns the participation owner for mutation.
    pub fn owner_mut(&mut self) -> &mut TransitParticipationOwner {
        &mut self.owner
    }

    /// Returns the live governor the owner revalidates against.
    pub fn governor(&self) -> Option<&BandwidthGovernor> {
        self.governor.as_ref()
    }

    /// Revalidates the governor's permit at the point of use.
    pub fn revalidate(&mut self) -> bool {
        let governor = self.governor.as_ref();
        self.owner.revalidate(governor)
    }

    /// Returns the retained receipts, oldest first.
    pub fn receipts(&self) -> Vec<TransitParticipationReceipt> {
        self.owner.receipts()
    }

    /// Returns the bounded, privacy-safe snapshot.
    pub fn snapshot(&self) -> TransitParticipationSnapshot {
        self.owner.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_runtime::BandwidthGovernorConfig;

    fn governor(outbound: u64) -> BandwidthGovernor {
        BandwidthGovernor::new(BandwidthGovernorConfig {
            inbound_bytes_per_second: outbound,
            outbound_bytes_per_second: outbound,
            burst_bytes: 64 * 1024,
            control_reserve_percent: 10,
            max_pending_requests: 16,
            max_pending_per_peer: 2,
        })
        .expect("valid governor")
    }

    fn peer(byte: u8) -> Hash {
        Hash::from_bytes([byte; 32])
    }

    fn owner(share_limit: u8, tunnels: usize, peer_cap: usize) -> TransitParticipationOwner {
        let policy = TransitParticipationPolicy::new(share_limit, tunnels, peer_cap)
            .expect("bounded policy");
        let governor = governor(1024 * 1024);
        TransitParticipationOwner::new(policy, Some(&governor), 50).expect("owner")
    }

    #[test]
    fn the_default_posture_is_disabled_and_refuses_everything() {
        let mut owner = TransitParticipationOwner::disabled();
        assert!(!owner.is_enabled());
        assert_eq!(
            owner.admit(&peer(1), 0, 0, 0),
            Err(TransitRefusalReason::ParticipationDisabled)
        );
        assert_eq!(
            owner.charge_relay(10, 0),
            Err(TransitRefusalReason::ParticipationDisabled)
        );
        assert_eq!(owner.refusals.total(), 2);
        assert_eq!(owner.bytes_relayed, 0);
    }

    #[test]
    fn the_share_budget_is_the_policy_share_of_the_enforced_share() {
        // 1 MiB/s outbound at a 50% operator share is 512 KiB/s; a 25%
        // transit posture may use a quarter of that.
        let owner = owner(25, 4, 2);
        assert_eq!(owner.share_bytes_per_second(), 128 * 1024);
    }

    #[test]
    fn without_an_enforced_permit_every_request_is_refused() {
        let policy = TransitParticipationPolicy::new(25, 4, 2).expect("policy");
        let mut owner = TransitParticipationOwner::new(policy, None, 50).expect("owner");
        assert_eq!(
            owner.admit(&peer(1), 0, 0, 0),
            Err(TransitRefusalReason::NoEnforcedPermit)
        );
        assert_eq!(owner.share_bytes_per_second(), 0);
    }

    #[test]
    fn the_participating_tunnel_count_is_enforced_and_counted() {
        let mut owner = owner(25, 2, 2);
        assert!(owner.admit(&peer(1), 0, 0, 0).is_ok());
        assert!(owner.admit(&peer(2), 1, 0, 0).is_ok());
        assert_eq!(
            owner.admit(&peer(3), 2, 0, 0),
            Err(TransitRefusalReason::ParticipatingTunnelLimit)
        );
        assert_eq!(
            owner
                .refusals
                .count(TransitRefusalReason::ParticipatingTunnelLimit),
            1
        );
        assert_eq!(owner.participating_tunnels(), 2);
    }

    #[test]
    fn the_per_peer_cap_is_enforced_and_counted() {
        let mut owner = owner(25, 8, 1);
        assert!(owner.admit(&peer(1), 0, 0, 0).is_ok());
        assert_eq!(
            owner.admit(&peer(1), 1, 1, 0),
            Err(TransitRefusalReason::PeerParticipationCap)
        );
        assert_eq!(
            owner
                .refusals
                .count(TransitRefusalReason::PeerParticipationCap),
            1
        );
        assert!(owner.admit(&peer(2), 1, 0, 0).is_ok());
    }

    #[test]
    fn exceeding_the_share_refuses_rather_than_grows() {
        let mut owner = owner(25, 8, 8);
        let budget = owner.share_bytes_per_second();
        assert!(owner.charge_relay(budget, 0).is_ok());
        assert_eq!(
            owner.charge_relay(1, 0),
            Err(TransitRefusalReason::ShareLimitExceeded)
        );
        assert_eq!(owner.bytes_relayed(), budget);
        assert_eq!(
            owner
                .refusals
                .count(TransitRefusalReason::ShareLimitExceeded),
            1
        );
        // The next window starts from zero.
        assert!(
            owner
                .charge_relay(budget, TRANSIT_SHARE_WINDOW_SECONDS)
                .is_ok()
        );
    }

    #[test]
    fn a_policy_change_to_off_stops_participation_and_is_recorded() {
        let mut owner = owner(25, 4, 2);
        assert!(owner.admit(&peer(1), 0, 0, 0).is_ok());
        let off = TransitConfig {
            participation: false,
            share_limit: 25,
            participating_tunnels: 4,
            peer_participation_cap: 2,
        };
        assert!(owner.apply_config(&off, None, 0));
        assert!(!owner.is_enabled());
        assert_eq!(owner.participating_tunnels(), 0);
        assert_eq!(owner.stops(), 1);
        assert_eq!(
            owner.admit(&peer(2), 0, 0, 0),
            Err(TransitRefusalReason::ParticipationDisabled)
        );
        let receipts = owner.receipts();
        assert!(
            receipts
                .iter()
                .any(|receipt| receipt.kind == TransitReceiptKind::Stopped)
        );
    }

    #[test]
    fn revocation_by_permit_withdrawal_stops_participation() {
        let policy = TransitParticipationPolicy::new(25, 4, 2).expect("policy");
        let issuing = governor(1024 * 1024);
        let mut owner = TransitParticipationOwner::new(policy, Some(&issuing), 50).expect("owner");
        assert!(owner.admit(&peer(1), 0, 0, 0).is_ok());
        // A different governor instance makes the permit stale.
        let replacement = governor(1024 * 1024);
        assert!(!owner.revalidate(Some(&replacement)));
        assert!(owner.is_revoked());
        assert_eq!(
            owner.revocation(),
            Some(TransitRevocationReason::PermitWithdrawn)
        );
        assert_eq!(owner.participating_tunnels(), 0);
        assert_eq!(owner.stops(), 1);
        assert_eq!(
            owner.admit(&peer(2), 0, 0, 0),
            Err(TransitRefusalReason::Revoked)
        );
    }

    #[test]
    fn revocation_by_health_loss_stops_participation() {
        let policy = TransitParticipationPolicy::new(25, 4, 2).expect("policy");
        let issuing = governor(1024 * 1024);
        let mut owner = TransitParticipationOwner::new(policy, Some(&issuing), 50).expect("owner");
        assert!(owner.admit(&peer(1), 0, 0, 0).is_ok());
        // The issuing governor is dropped: the permit can no longer be
        // revalidated and the owner loses health.
        drop(issuing);
        assert!(!owner.revalidate(None));
        assert_eq!(
            owner.revocation(),
            Some(TransitRevocationReason::HealthLost)
        );
        assert_eq!(owner.participating_tunnels(), 0);
    }

    #[test]
    fn a_restart_re_derives_the_accounting_from_zero() {
        let mut owner = owner(25, 4, 2);
        assert!(owner.admit(&peer(1), 0, 0, 0).is_ok());
        assert!(owner.charge_relay(1024, 0).is_ok());
        assert_eq!(owner.bytes_relayed(), 1024);

        // A restart constructs a fresh owner from the same posture.
        let policy = TransitParticipationPolicy::new(25, 4, 2).expect("policy");
        let governor = governor(1024 * 1024);
        let restarted = TransitParticipationOwner::new(policy, Some(&governor), 50).expect("owner");
        assert_eq!(restarted.bytes_relayed(), 0);
        assert_eq!(restarted.window_bytes(), 0);
        assert_eq!(restarted.participating_tunnels(), 0);
        assert_eq!(restarted.snapshot().admitted, 0);
    }

    #[test]
    fn the_receipts_are_bounded_and_carry_counts_only() {
        let mut owner = owner(25, 4, 2);
        for index in 0..(MAX_TRANSIT_PARTICIPATION_RECEIPTS + 10) {
            assert!(owner.admit(&peer(index as u8), 0, 0, 0).is_ok());
        }
        assert_eq!(owner.receipt_count(), MAX_TRANSIT_PARTICIPATION_RECEIPTS);
        let receipts = owner.receipts();
        for receipt in &receipts {
            // Every receipt field is a bounded count: no router hash, no
            // address, no tunnel id, no key, and no payload can appear in
            // one. The static guard in the Plan 492 lane test proves the
            // receipt type itself carries no such field.
            assert!(receipt.participating_tunnels <= 4);
            assert!(receipt.peers_participating <= MAX_TRANSIT_PARTICIPATION_TRACKED_PEERS);
            assert!(receipt.bytes_relayed <= owner.share_bytes_per_second());
        }
    }

    #[test]
    fn the_admitted_peer_record_is_bounded() {
        let mut owner = owner(25, 8, 8);
        for index in 0..(MAX_TRANSIT_PARTICIPATION_TRACKED_PEERS + 10) {
            let mut bytes = [0u8; 32];
            bytes[0..8].copy_from_slice(&(index as u64).to_be_bytes());
            assert!(owner.admit(&Hash::from_bytes(bytes), 0, 0, 0).is_ok());
        }
        assert!(owner.peers_participating() <= MAX_TRANSIT_PARTICIPATION_TRACKED_PEERS);
    }
}
