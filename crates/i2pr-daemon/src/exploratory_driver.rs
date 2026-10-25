//! Plan 505 — the exploratory build driver in the product composition.
//!
//! The mechanism that builds a one-hop exploratory tunnel already exists:
//! [`ExploratoryBuildCoordinator::rebuild_exploratory`] is a complete
//! first-build driver with the peer-selection step removed. What was missing
//! was the *input* — a peer, that peer's public build material, and the
//! observation of the outcome. This module owns exactly that input and
//! nothing else:
//!
//! * the peer is chosen through Plan 491's version-gated
//!   [`PeerParticipationDriver::select_tunnel_build_peers`](crate::netdb_participation::PeerParticipationDriver::select_tunnel_build_peers),
//!   so a peer below the floor is refused and counted by reason, never
//!   silently skipped;
//! * the peer's public build material is resolved from the validated
//!   RouterInfo store through the [`PeerBuildIdentityResolver`] seam, so a
//!   peer with no validated record — or with an identity key of the wrong
//!   length — is refused and counted, never substituted;
//! * the build is submitted through the **existing**
//!   [`ExploratoryBuildCoordinator::rebuild_exploratory`], so there is
//!   exactly one code path that builds an exploratory tunnel and the first
//!   build cannot drift away from the liveness rebuild;
//! * every terminal build outcome the driver owns produces exactly **one**
//!   peer-level observation in the profile store.
//!
//! Properties:
//!
//! - the driver owns no task, no timer, no listener and no socket; it is a
//!   plain value the product tick advances;
//! - the driver's own state is bounded: at most one in-flight attempt and at
//!   most one retained pair peer;
//! - the driver never builds, registers, removes or rebuilds a destination
//!   tunnel; it is exploratory-only;
//! - the counters carry counts only — no peer identity, no address, no key
//!   material.

#![forbid(unsafe_code)]

use std::fmt;
use std::sync::Arc;

use i2pr_netdb::{RouterHash, ValidatedRouterInfo};
use i2pr_proto::Hash;
use i2pr_tunnel::bridge::ShortBuildI2npBridge;
use i2pr_tunnel::build_crypto::EPHEMERAL_KEY_LEN;
use i2pr_tunnel::short::BuildAttemptId;
use rand_core::{CryptoRng, RngCore};

use crate::exploratory_build::{
    BuildCoordinatorOutcome, BuildDirection, ExploratoryBuildCoordinator, ExploratoryRebuildPeer,
    SubmitResult,
};
use crate::netdb_participation::PeerParticipationHandle;
use crate::router_i2np::RouterDeliveryService;

/// The public build identity of one peer.
///
/// The static encryption key is the peer's **RouterInfo identity public
/// key** — `router_info().router_identity().public_key()` — exactly as the
/// product's own `peer_static_encryption_key` seam derives it. It is *not*
/// the SSU2 address `s` option: that key belongs to the transport session,
/// not to the peer's build identity.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct PeerBuildIdentity {
    /// The peer's router hash.
    pub router_hash: Hash,
    /// The peer's public static encryption key.
    pub static_encryption_key: [u8; EPHEMERAL_KEY_LEN],
}

impl fmt::Debug for PeerBuildIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The key is public peer material, but the house style redacts peer
        // identity in every derived representation.
        formatter
            .debug_struct("PeerBuildIdentity")
            .field("router_hash", &"<redacted>")
            .field("static_encryption_key", &"<redacted>")
            .finish()
    }
}

/// Why a peer's build material could not be resolved.
///
/// Every arm is a refusal. The driver never substitutes a key, a peer or an
/// address for a peer it could not resolve.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerMaterialError {
    /// The authoritative store holds no validated record for the peer.
    UnknownPeer,
    /// The peer's identity public key is not a 32-byte X25519 key.
    InvalidKeyLength {
        /// The observed key length.
        actual: usize,
    },
    /// The peer-profile store could not be locked.
    StoreUnavailable,
}

impl fmt::Display for PeerMaterialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPeer => write!(formatter, "the peer has no validated RouterInfo"),
            Self::InvalidKeyLength { actual } => {
                write!(formatter, "the peer identity key is {actual} bytes, not 32")
            }
            Self::StoreUnavailable => write!(formatter, "the peer-profile store is unavailable"),
        }
    }
}

/// The seam that maps a selected peer key to its public build identity.
///
/// The driver never reaches into a store directly: it asks the resolver. A
/// later work package may supply a different resolver — a cache, a
/// per-tick snapshot — without touching the driver's state machine.
pub trait PeerBuildIdentityResolver: Send + Sync {
    /// Resolves the peer's public build identity.
    ///
    /// A peer the resolver cannot resolve is refused, never substituted.
    fn resolve(&self, peer: &RouterHash) -> Result<PeerBuildIdentity, PeerMaterialError>;
}

/// The production resolver: the peer-profile store's own validated records.
///
/// The profile store *adopts* the validated-RouterInfo store, so the
/// resolver reads the very record the selection was drawn from. The store
/// and the resolver therefore cannot disagree about which peer exists.
#[derive(Clone, Debug)]
pub struct ProfileStorePeerResolver {
    profiles: PeerParticipationHandle,
}

impl ProfileStorePeerResolver {
    /// Constructs a resolver over the shared peer-profile handle.
    pub fn new(profiles: PeerParticipationHandle) -> Self {
        Self { profiles }
    }
}

impl PeerBuildIdentityResolver for ProfileStorePeerResolver {
    fn resolve(&self, peer: &RouterHash) -> Result<PeerBuildIdentity, PeerMaterialError> {
        match self.profiles.with(|profiles| {
            let record = profiles
                .store()
                .peers()
                .get(peer)
                .ok_or(PeerMaterialError::UnknownPeer)?;
            identity_from_record(record)
        }) {
            Some(resolved) => resolved,
            None => Err(PeerMaterialError::StoreUnavailable),
        }
    }
}

/// Derives the public build identity from one validated RouterInfo.
///
/// The key is the identity's public key, never the SSU2 address `s` option.
fn identity_from_record(
    record: &ValidatedRouterInfo,
) -> Result<PeerBuildIdentity, PeerMaterialError> {
    let bytes = record
        .router_info()
        .router_identity()
        .public_key()
        .as_bytes();
    if bytes.len() != EPHEMERAL_KEY_LEN {
        return Err(PeerMaterialError::InvalidKeyLength {
            actual: bytes.len(),
        });
    }
    let mut static_encryption_key = [0_u8; EPHEMERAL_KEY_LEN];
    static_encryption_key.copy_from_slice(bytes);
    Ok(PeerBuildIdentity {
        router_hash: *record.key().as_hash(),
        static_encryption_key,
    })
}

/// The peer-level observation one terminal build outcome produces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerObservation {
    /// The peer answered the build request.
    Answered,
    /// The peer did not answer before the deadline.
    Unanswered,
    /// The peer refused the build request.
    Refused,
    /// The peer could not be reached.
    Unreachable,
    /// The peer answered in a way the coordinator did not accept.
    Misbehaving,
    /// The terminal outcome was **this router's own** condition — the local
    /// pool refused the registration, the local registration was invalid,
    /// the coordinator refused the submission, the path was invalid, or the
    /// attempt was cancelled locally.
    ///
    /// The peer is **not** at fault, so this variant produces **no**
    /// peer-level observation: the router must never write a peer's profile
    /// a statement the peer did not earn. The outcome is counted by the
    /// driver's own counters and the peer's profile is left untouched.
    Local,
}

impl PeerObservation {
    /// Returns the stable label used in counters and receipts.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Answered => "answered",
            Self::Unanswered => "unanswered",
            Self::Refused => "refused",
            Self::Unreachable => "unreachable",
            Self::Misbehaving => "misbehaving",
            Self::Local => "local",
        }
    }
}

/// Maps one terminal build outcome to the peer-level observation it
/// produces.
///
/// The mapping is total over [`BuildCoordinatorOutcome`]: no terminal
/// outcome is silently dropped. The outcome's own variants decide the
/// observation — the driver never guesses.
///
/// | Outcome | Observation |
/// | --- | --- |
/// | `Installed` | `Answered` |
/// | `TimedOut` | `Unanswered` |
/// | `HopRejected` | `Refused` |
/// | `DeliveryFailed` | `Unreachable` |
/// | `InvalidReply` | `Misbehaving` |
/// | `Cancelled`, `PoolRejected`, `InvalidRegistration`, `InvalidPath`, `CoordinatorRejected` | `Local` — the router's own condition, so **no** peer-level observation is written |
/// | the four destination-scoped variants | never attributed to the driver |
///
/// The destination-scoped variants are excluded because the driver is
/// exploratory-only: it never owns a destination attempt, so such an
/// outcome is never the driver's outcome.
pub fn observation_for_outcome(outcome: &BuildCoordinatorOutcome) -> PeerObservation {
    match outcome {
        BuildCoordinatorOutcome::Installed { .. } => PeerObservation::Answered,
        BuildCoordinatorOutcome::TimedOut { .. } => PeerObservation::Unanswered,
        BuildCoordinatorOutcome::HopRejected { .. } => PeerObservation::Refused,
        BuildCoordinatorOutcome::DeliveryFailed { .. } => PeerObservation::Unreachable,
        BuildCoordinatorOutcome::InvalidReply { .. } => PeerObservation::Misbehaving,
        // These five are **this router's** condition, not the peer's: the
        // local pool refused the registration, the local registration was
        // invalid, the coordinator refused the submission locally, the path
        // was invalid, or the attempt was cancelled locally. Writing any of
        // them into the peer's profile would blame the peer for a local
        // fault, so they produce no peer-level observation.
        BuildCoordinatorOutcome::Cancelled { .. }
        | BuildCoordinatorOutcome::PoolRejected { .. }
        | BuildCoordinatorOutcome::InvalidRegistration { .. }
        | BuildCoordinatorOutcome::InvalidPath { .. }
        | BuildCoordinatorOutcome::CoordinatorRejected { .. } => PeerObservation::Local,
        BuildCoordinatorOutcome::DestinationEstablished { .. }
        | BuildCoordinatorOutcome::DestinationHandoffRejected { .. }
        | BuildCoordinatorOutcome::DestinationBuildFailed { .. }
        | BuildCoordinatorOutcome::DestinationBuildCancelled { .. } => PeerObservation::Misbehaving,
    }
}

/// Returns whether one outcome belongs to an exploratory attempt.
///
/// The driver owns no destination attempt, so a destination-scoped outcome
/// is never attributed to the driver's peer.
fn is_exploratory_scoped(outcome: &BuildCoordinatorOutcome) -> bool {
    !matches!(
        outcome,
        BuildCoordinatorOutcome::DestinationEstablished { .. }
            | BuildCoordinatorOutcome::DestinationHandoffRejected { .. }
            | BuildCoordinatorOutcome::DestinationBuildFailed { .. }
            | BuildCoordinatorOutcome::DestinationBuildCancelled { .. }
    )
}

/// Returns the attempt id one outcome carries, when it carries one.
fn outcome_attempt_id(outcome: &BuildCoordinatorOutcome) -> Option<BuildAttemptId> {
    match outcome {
        BuildCoordinatorOutcome::InvalidReply { attempt_id, .. }
        | BuildCoordinatorOutcome::DestinationEstablished { attempt_id, .. } => Some(*attempt_id),
        _ => None,
    }
}

/// Bounded, privacy-safe counters for the exploratory build driver.
///
/// The counters carry counts only: no peer identity, no address, no key
/// material and no payload.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExploratoryDriverCounters {
    /// Peer selections the driver drew.
    pub selections: u64,
    /// Build attempts the coordinator accepted.
    pub attempts: u64,
    /// Terminal outcomes the driver observed.
    pub outcomes_observed: u64,
    /// Terminal outcomes that installed a tunnel in the pool and the registry.
    pub installed: u64,
    /// Peer observations recorded as "answered".
    pub answered: u64,
    /// Peer observations recorded as "unanswered".
    pub unanswered: u64,
    /// Peer observations recorded as "refused".
    pub refused: u64,
    /// Peer observations recorded as "unreachable".
    pub unreachable: u64,
    /// Peer observations recorded as "misbehaving".
    pub misbehaving: u64,
    /// Terminal outcomes that were **this router's own** condition. They are
    /// counted here and produce **no** peer-level observation.
    pub local_outcomes: u64,
    /// Observations the profile store refused because it no longer holds the peer.
    pub observations_unknown_peer: u64,
    /// Ticks the peer-profile store could not be locked.
    pub store_unavailable: u64,
    /// Outcomes the driver could not attribute to its own attempt.
    pub orphan_outcomes: u64,
    /// Ticks refused because the operator has not enabled the driver.
    pub disabled: u64,
    /// Ticks refused because the coordinator is paused.
    pub coordinator_paused: u64,
    /// Ticks refused because a build was already outstanding.
    pub pending_busy: u64,
    /// Ticks refused because the pool already holds a usable exploratory pair.
    pub pool_satisfied: u64,
    /// Ticks refused because no peer was eligible.
    pub no_eligible_peer: u64,
    /// Ticks refused because the peer's build material could not be resolved.
    pub peer_material_absent: u64,
    /// Submissions the narrow delivery seam refused.
    pub peer_not_reachable: u64,
    /// Submissions the coordinator refused for a local reason.
    pub coordinator_rejected: u64,
    /// Peer selections the version gate refused for an unknown version.
    pub refusals_unknown_version: u64,
    /// Peer selections the version gate refused for an uncomparable version.
    pub refusals_uncomparable_version: u64,
    /// Peer selections the version gate refused for a version below the floor.
    pub refusals_below_floor: u64,
    /// Pairs the driver completed — both directions installed toward one peer.
    pub pairs_completed: u64,
}

/// What one driver tick did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExploratoryDriverAction {
    /// The operator has not enabled the driver.
    Disabled,
    /// The coordinator is paused after repeated failures.
    CoordinatorPaused,
    /// A build the driver submitted is still outstanding.
    AwaitingReply,
    /// The coordinator's pending table is not empty.
    PendingBusy,
    /// The pool already holds a usable exploratory pair.
    PoolSatisfied,
    /// No peer was eligible.
    NoEligiblePeer,
    /// The selected peer's build material could not be resolved.
    PeerMaterialAbsent,
    /// The driver submitted the outbound build of a pair.
    SubmittedOutbound,
    /// The driver submitted the inbound build of a pair.
    SubmittedInbound,
    /// The narrow delivery seam refused the build request.
    PeerNotReachable,
    /// The coordinator refused the submission for a local reason.
    CoordinatorRejected,
}

/// One peer the driver chose for a pair.
///
/// The peer is kept as a **key**, not as material: the build material is
/// resolved from the authoritative store at build time, so a peer that left
/// the store between the two halves of a pair is refused rather than built
/// toward with stale material.
#[derive(Clone, Copy, Eq, PartialEq)]
struct SelectedPeer {
    /// The peer's key in the peer-profile store.
    key: RouterHash,
}

impl fmt::Debug for SelectedPeer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SelectedPeer")
            .field("key", &"<redacted>")
            .finish()
    }
}

/// The driver's single bounded in-flight build record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InFlightBuild {
    /// The direction the attempt is for.
    direction: BuildDirection,
    /// The peer the attempt is against.
    peer: SelectedPeer,
    /// The coordinator's attempt id.
    attempt_id: BuildAttemptId,
}

/// The driver's privacy-safe state summary.
///
/// The driver's own state carries a peer identity, so the public surface
/// publishes the summary instead: no peer, no address, no key material.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExploratoryDriverState {
    /// No pair is in progress; the next tick chooses a peer.
    Idle,
    /// One build is outstanding.
    InFlight,
    /// The outbound half of the pair is installed; the inbound half toward
    /// the same peer is next.
    InboundPending,
}

/// The driver's bounded state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DriverState {
    /// No pair is in progress; the next tick chooses a peer.
    Idle,
    /// One build is outstanding.
    InFlight(InFlightBuild),
    /// The outbound tunnel is installed; the inbound build toward the same
    /// peer is next.
    InboundPending(SelectedPeer),
}

impl DriverState {
    /// Returns the privacy-safe summary of the state.
    const fn summary(self) -> ExploratoryDriverState {
        match self {
            Self::Idle => ExploratoryDriverState::Idle,
            Self::InFlight(_) => ExploratoryDriverState::InFlight,
            Self::InboundPending(_) => ExploratoryDriverState::InboundPending,
        }
    }
}

/// The Plan 505 exploratory build driver.
///
/// The driver owns the *input* to the existing exploratory build path: it
/// names a peer, submits the build through
/// [`ExploratoryBuildCoordinator::rebuild_exploratory`], and records one
/// peer-level observation for every terminal outcome it owns.
pub struct ExploratoryDriver {
    /// Whether the operator has enabled the feature.
    enabled: bool,
    /// The shared peer-profile handle the driver selects from and writes to.
    profiles: PeerParticipationHandle,
    /// The seam that resolves a selected peer to its public build material.
    resolver: Arc<dyn PeerBuildIdentityResolver + Send + Sync>,
    /// The driver's bounded state.
    state: DriverState,
    /// The driver's bounded counters.
    counters: ExploratoryDriverCounters,
    /// The driver's clock, advanced by [`Self::drive`].
    now_ms: u64,
}

impl fmt::Debug for ExploratoryDriver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExploratoryDriver")
            .field("enabled", &self.enabled)
            .field("resolver", &"<opaque>")
            .field("state", &self.state)
            .field("counters", &self.counters)
            .field("now_ms", &self.now_ms)
            .finish()
    }
}

impl ExploratoryDriver {
    /// Constructs a driver.
    ///
    /// `enabled` is the operator opt-in. The configuration wiring is a later
    /// work package; the driver takes the flag directly so the product can
    /// own the wiring without the driver owning configuration.
    pub fn new(
        enabled: bool,
        profiles: PeerParticipationHandle,
        resolver: Arc<dyn PeerBuildIdentityResolver + Send + Sync>,
    ) -> Self {
        Self {
            enabled,
            profiles,
            resolver,
            state: DriverState::Idle,
            counters: ExploratoryDriverCounters::default(),
            now_ms: 0,
        }
    }

    /// Returns the driver's bounded counters.
    pub const fn counters(&self) -> ExploratoryDriverCounters {
        self.counters
    }

    /// Returns whether the operator has enabled the driver.
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the driver's privacy-safe state summary.
    pub const fn state(&self) -> ExploratoryDriverState {
        self.state.summary()
    }

    /// Advances the driver one tick.
    ///
    /// The driver submits only when every one of the plan's conditions
    /// holds: the operator has enabled the feature, the coordinator is not
    /// paused, the pending table is empty, the pool has no usable
    /// exploratory pair, and a peer was selected and its material resolved.
    /// Every refusal is counted by reason.
    pub fn drive<R>(
        &mut self,
        now_ms: u64,
        coordinator: &mut ExploratoryBuildCoordinator,
        delivery: &RouterDeliveryService,
        bridge: &ShortBuildI2npBridge,
        local_router_hash: Option<Hash>,
        rng: &mut R,
    ) -> ExploratoryDriverAction
    where
        R: CryptoRng + RngCore,
    {
        self.now_ms = now_ms;
        if !self.enabled {
            self.counters.disabled = self.counters.disabled.saturating_add(1);
            return ExploratoryDriverAction::Disabled;
        }
        if coordinator.is_paused() {
            self.counters.coordinator_paused = self.counters.coordinator_paused.saturating_add(1);
            return ExploratoryDriverAction::CoordinatorPaused;
        }
        if !coordinator.exploratory_liveness_pairs().is_empty() {
            self.counters.pool_satisfied = self.counters.pool_satisfied.saturating_add(1);
            return ExploratoryDriverAction::PoolSatisfied;
        }
        // The peer is chosen **once** per pair: the outbound build names it
        // and the inbound build reuses the very same peer.
        let (direction, peer) = match self.state {
            DriverState::InFlight(_) => {
                self.counters.pending_busy = self.counters.pending_busy.saturating_add(1);
                return ExploratoryDriverAction::AwaitingReply;
            }
            DriverState::Idle => {
                let peer = match self.select_peer(now_ms) {
                    Some(peer) => peer,
                    None => {
                        self.counters.no_eligible_peer =
                            self.counters.no_eligible_peer.saturating_add(1);
                        return ExploratoryDriverAction::NoEligiblePeer;
                    }
                };
                (BuildDirection::Outbound, peer)
            }
            DriverState::InboundPending(peer) => (BuildDirection::Inbound, peer),
        };
        if coordinator.pending_len() != 0 {
            self.counters.pending_busy = self.counters.pending_busy.saturating_add(1);
            return ExploratoryDriverAction::PendingBusy;
        }
        // The material is resolved outside the peer-profile lock: the
        // resolver takes the same lock itself.
        let material = match self.resolver.resolve(&peer.key) {
            Ok(material) => material,
            Err(_) => {
                self.counters.peer_material_absent =
                    self.counters.peer_material_absent.saturating_add(1);
                // The pair attempt cannot proceed: the driver abandons it
                // rather than spinning on a peer it cannot resolve.
                self.state = DriverState::Idle;
                return ExploratoryDriverAction::PeerMaterialAbsent;
            }
        };
        if material.router_hash != *peer.key.as_hash() {
            // The resolver and the selection disagree about the peer. The
            // driver refuses rather than substituting one for the other.
            self.counters.peer_material_absent =
                self.counters.peer_material_absent.saturating_add(1);
            self.state = DriverState::Idle;
            return ExploratoryDriverAction::PeerMaterialAbsent;
        }
        let rebuild_peer = ExploratoryRebuildPeer {
            router_hash: material.router_hash,
            static_encryption_key: material.static_encryption_key,
        };
        match coordinator.rebuild_exploratory(
            direction,
            rebuild_peer,
            local_router_hash,
            delivery,
            bridge,
            rng,
        ) {
            Ok(SubmitResult::Submitted { attempt_id, .. }) => {
                self.counters.attempts = self.counters.attempts.saturating_add(1);
                self.state = DriverState::InFlight(InFlightBuild {
                    direction,
                    peer,
                    attempt_id,
                });
                match direction {
                    BuildDirection::Outbound => ExploratoryDriverAction::SubmittedOutbound,
                    BuildDirection::Inbound => ExploratoryDriverAction::SubmittedInbound,
                }
            }
            Ok(SubmitResult::Rejected { .. }) => {
                // The narrow outbound seam refused the request: the peer
                // could not be reached at the transport level.
                self.counters.peer_not_reachable =
                    self.counters.peer_not_reachable.saturating_add(1);
                self.record_observation(peer.key, PeerObservation::Unreachable);
                self.state = DriverState::Idle;
                ExploratoryDriverAction::PeerNotReachable
            }
            Err(_error) => {
                // A local coordinator refusal. The peer never saw the
                // request, so the peer is not at fault and the driver
                // records no observation.
                self.counters.coordinator_rejected =
                    self.counters.coordinator_rejected.saturating_add(1);
                self.state = DriverState::Idle;
                ExploratoryDriverAction::CoordinatorRejected
            }
        }
    }

    /// Records the peer-level observation for one terminal build outcome.
    ///
    /// Returns `true` when the outcome was attributed to the driver's own
    /// attempt — that is, when it produced exactly one observation — and
    /// `false` when the outcome was an orphan. An orphan is an outcome the
    /// driver cannot attribute: it has no in-flight record, it belongs to
    /// another direction, it carries another attempt id, or it belongs to a
    /// destination attempt the driver never owned. An orphan is counted and
    /// produces no observation.
    pub fn observe_outcome(&mut self, outcome: &BuildCoordinatorOutcome) -> bool {
        let Some(in_flight) = self.in_flight() else {
            self.counters.orphan_outcomes = self.counters.orphan_outcomes.saturating_add(1);
            return false;
        };
        if !is_exploratory_scoped(outcome) {
            self.counters.orphan_outcomes = self.counters.orphan_outcomes.saturating_add(1);
            return false;
        }
        if outcome.direction() != in_flight.direction {
            self.counters.orphan_outcomes = self.counters.orphan_outcomes.saturating_add(1);
            return false;
        }
        if let Some(attempt_id) = outcome_attempt_id(outcome)
            && attempt_id != in_flight.attempt_id
        {
            self.counters.orphan_outcomes = self.counters.orphan_outcomes.saturating_add(1);
            return false;
        }
        let observation = observation_for_outcome(outcome);
        self.counters.outcomes_observed = self.counters.outcomes_observed.saturating_add(1);
        if matches!(outcome, BuildCoordinatorOutcome::Installed { .. }) {
            self.counters.installed = self.counters.installed.saturating_add(1);
        }
        self.record_observation(in_flight.peer.key, observation);
        self.state = match (observation, in_flight.direction) {
            (PeerObservation::Answered, BuildDirection::Outbound) => {
                // The outbound half of the pair is installed: the inbound
                // half toward the **same** peer is next.
                DriverState::InboundPending(in_flight.peer)
            }
            (PeerObservation::Answered, BuildDirection::Inbound) => {
                self.counters.pairs_completed = self.counters.pairs_completed.saturating_add(1);
                DriverState::Idle
            }
            _ => DriverState::Idle,
        };
        true
    }

    /// Records the peer-level observation for every outcome in a batch.
    ///
    /// Returns the number of outcomes that were attributed to the driver's
    /// own attempt.
    pub fn observe_outcomes(&mut self, outcomes: &[BuildCoordinatorOutcome]) -> usize {
        let mut attributed = 0_usize;
        for outcome in outcomes {
            if self.observe_outcome(outcome) {
                attributed = attributed.saturating_add(1);
            }
        }
        attributed
    }

    /// Returns the driver's in-flight record, when it has one.
    fn in_flight(&self) -> Option<InFlightBuild> {
        match self.state {
            DriverState::InFlight(record) => Some(record),
            _ => None,
        }
    }

    /// Selects one peer through Plan 491's version-gated selection.
    ///
    /// The refusals the gate counted are added to the driver's own counters
    /// by reason, so a peer below the floor is never a silent skip.
    fn select_peer(&mut self, now_ms: u64) -> Option<SelectedPeer> {
        let seed = self.selection_seed(now_ms);
        let Some(selection) = self.profiles.with(|profiles| {
            profiles.advance_time(now_ms);
            profiles.select_tunnel_build_peers(1, seed)
        }) else {
            self.counters.store_unavailable = self.counters.store_unavailable.saturating_add(1);
            return None;
        };
        let refusals = selection.refusals;
        self.counters.refusals_unknown_version = self
            .counters
            .refusals_unknown_version
            .saturating_add(u64::from(refusals.unknown_version));
        self.counters.refusals_uncomparable_version = self
            .counters
            .refusals_uncomparable_version
            .saturating_add(u64::from(refusals.uncomparable_version));
        self.counters.refusals_below_floor = self
            .counters
            .refusals_below_floor
            .saturating_add(u64::from(refusals.below_floor));
        let candidate = selection.candidates.into_iter().next()?;
        self.counters.selections = self.counters.selections.saturating_add(1);
        Some(SelectedPeer { key: candidate.key })
    }

    /// Derives the deterministic selection seed for one tick.
    ///
    /// The seed is a mixed value, not a counter, so it is derived with
    /// wrapping arithmetic.
    fn selection_seed(&self, now_ms: u64) -> u64 {
        let mut seed = now_ms ^ self.counters.selections.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        seed ^= seed >> 33;
        seed
    }

    /// Records exactly one peer-level observation in the profile store.
    fn record_observation(&mut self, key: RouterHash, observation: PeerObservation) {
        let now_ms = self.now_ms;
        // A local condition is **not** a peer-level fact. The router's own
        // failure is counted by the driver's counters and the peer's profile
        // is left untouched — the router must never blame a peer for a
        // condition of its own.
        if matches!(observation, PeerObservation::Local) {
            self.counters.local_outcomes = self.counters.local_outcomes.saturating_add(1);
            return;
        }
        let outcome = self.profiles.with(|profiles| {
            profiles.advance_time(now_ms);
            match observation {
                PeerObservation::Answered => profiles.observe_answered(&key),
                PeerObservation::Unanswered => profiles.observe_unanswered(&key),
                PeerObservation::Refused => profiles.observe_refused(&key),
                PeerObservation::Unreachable => profiles.observe_unreachable(&key),
                PeerObservation::Misbehaving => profiles.observe_misbehaving(&key),
                PeerObservation::Local => unreachable!("the local case returns above"),
            }
        });
        match outcome {
            Some(i2pr_netdb::ObserveOutcome::Recorded { .. }) => {
                let counter = match observation {
                    PeerObservation::Answered => &mut self.counters.answered,
                    PeerObservation::Unanswered => &mut self.counters.unanswered,
                    PeerObservation::Refused => &mut self.counters.refused,
                    PeerObservation::Unreachable => &mut self.counters.unreachable,
                    PeerObservation::Misbehaving => &mut self.counters.misbehaving,
                    PeerObservation::Local => unreachable!("the local case returns above"),
                };
                *counter = counter.saturating_add(1);
            }
            Some(i2pr_netdb::ObserveOutcome::UnknownPeer) => {
                self.counters.observations_unknown_peer =
                    self.counters.observations_unknown_peer.saturating_add(1);
            }
            None => {
                self.counters.store_unavailable = self.counters.store_unavailable.saturating_add(1);
            }
        }
    }
}

#[cfg(test)]
mod exploratory_driver_tests {
    //! Plan 505 WP1 — the feasibility artefact.
    //!
    //! The artefact answers one question by running code: can the driver,
    //! against the **real** [`ExploratoryBuildCoordinator`], the **real**
    //! peer-profile store and the **real** [`RouterDeliveryService`], name a
    //! peer, build a one-hop exploratory tunnel toward it, install it into
    //! the pool and the registry, and record the outcome — with no
    //! wire-format change and no new dependency?
    //!
    //! The controlled mesh is the Plan 185 shape: two real SSU2 services on
    //! loopback, one real dial, and a responder that applies the canonical
    //! [`MessageHopProcessor`] to the inbound short tunnel build.
    //!
    //! The responder uses the peer's **identity** encryption key on both
    //! sides, exactly as the product's `peer_static_encryption_key` seam
    //! derives it. The Plan 185 harness used the SSU2 static key on both
    //! sides, which is self-consistent but does not prove the production
    //! key derivation.

    use std::sync::Arc;
    use std::time::Duration;

    use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
    use i2pr_netdb::{
        MIN_PEER_VERSION_FOR_TUNNEL_BUILD, ProfileState, ProfileStoreConfig, RouterInfoStore,
        RouterInfoStoreConfig, ValidatedRouterInfo,
    };
    use i2pr_proto::{Date, Hash, I2npBody, I2npMessage, Mapping, RouterAddress};
    use i2pr_runtime::{
        CancellationToken, ChildFailurePolicy, ChildScope, Ssu2IdentityMaterial, Ssu2InboundI2np,
    };
    use i2pr_transport::{LinkId, MAX_I2NP_MESSAGE_BYTES, PeerId};
    use i2pr_tunnel::bridge::ShortBuildI2npBridge;
    use i2pr_tunnel::build_crypto::{BuildCryptography, EciesX25519BuildCryptography};
    use i2pr_tunnel::config::ExploratoryPoolConfig;
    use i2pr_tunnel::multirecord::MessageHopProcessor;
    use i2pr_tunnel::short::BuildAttemptId;
    use i2pr_tunnel::short_record::{ShortRequestRecord, ShortResponseCode};
    use rand_chacha::ChaCha8Rng;
    use rand_core::{OsRng, SeedableRng, TryRngCore};

    use crate::exploratory_build::{
        BuildCoordinatorOutcome, BuildDirection, ExploratoryBuildCoordinator,
    };
    use crate::netdb_participation::{PeerParticipationDriver, PeerParticipationHandle};
    use crate::router_i2np::{
        RouterDeliveryRequest, RouterDeliveryService, RouterI2npKind, RouterI2npOutcome,
        Ssu2DaemonHandle, Ssu2DaemonService, dispatch_router_i2np,
    };
    use crate::tunnel_liveness::{LivenessConfig, TunnelLivenessScheduler};

    use super::{
        ExploratoryDriver, ExploratoryDriverAction, PeerBuildIdentityResolver,
        ProfileStorePeerResolver,
    };

    const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
    const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
    const POLL_INTERVAL: Duration = Duration::from_millis(250);

    /// The peer's published API version, read from the floor the policy
    /// derives. The lane never restates the floor as a literal, so it
    /// cannot drift from the product.
    fn at_floor_build_version() -> String {
        MIN_PEER_VERSION_FOR_TUNNEL_BUILD.to_string()
    }

    fn wall_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or(1_700_000_000)
    }

    fn wall_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(1_700_000_000_000)
    }

    fn i2p_b64_encode(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
        let mut output = String::new();
        for chunk in bytes.chunks(3) {
            let mut value: u32 = 0;
            for byte in chunk {
                value = (value << 8) | u32::from(*byte);
            }
            value <<= 8 * (3 - chunk.len());
            let digits = match chunk.len() {
                1 => 2,
                2 => 3,
                _ => 4,
            };
            for index in 0..digits {
                output.push(ALPHABET[((value >> (18 - 6 * index)) & 0x3f) as usize] as char);
            }
            for _ in digits..4 {
                output.push('=');
            }
        }
        output
    }

    /// One controlled router's identity material.
    struct LocalKeys {
        /// The router identity bundle. The bundle owns the **identity**
        /// encryption key the production key derivation uses.
        bundle: RouterIdentityBundle,
        /// The router hash.
        hash: Hash,
        /// The SSU2 static secret — the transport key, never the build key.
        static_bytes: [u8; 32],
        /// The SSU2 static public key.
        static_public: i2pr_runtime::Ssu2PublicKey,
        /// The SSU2 intro key.
        intro: i2pr_runtime::IntroKey,
        /// The signed RouterInfo.
        router_info: i2pr_proto::RouterInfo,
        /// The encoded RouterInfo.
        router_info_bytes: Vec<u8>,
    }

    fn make_local_keys(port_placeholder: u16) -> LocalKeys {
        let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
        let hash = bundle.identity().hash().expect("hash");
        let static_secret = X25519PrivateKey::generate(&mut OsRng).expect("static");
        let static_bytes = *static_secret.secret_bytes();
        let static_public =
            i2pr_runtime::Ssu2PublicKey::new(static_secret.public_bytes()).expect("static public");
        let mut intro_bytes = [0_u8; 32];
        loop {
            OsRng.try_fill_bytes(&mut intro_bytes).expect("rng");
            if intro_bytes.iter().any(|byte| *byte != 0) {
                break;
            }
        }
        let intro = i2pr_runtime::IntroKey::new(intro_bytes);
        let options = Mapping::from_entries(vec![
            ("host".to_string(), "127.0.0.1".to_string()),
            ("port".to_string(), port_placeholder.to_string()),
            ("v".to_string(), "2".to_string()),
            (
                "s".to_string(),
                i2p_b64_encode(&static_secret.public_bytes()),
            ),
            ("i".to_string(), i2p_b64_encode(&intro_bytes)),
        ])
        .expect("options");
        let address = RouterAddress::new(
            10,
            Date::from_millis(9_999_999_999_999),
            "SSU2".to_string(),
            options,
        )
        .expect("address");
        let mut ri_options = Mapping::builder();
        // The peer publishes the API version the tunnel-build floor
        // demands, so the version-gated selection accepts it.
        ri_options
            .insert("router.version".to_string(), at_floor_build_version())
            .expect("router.version");
        ri_options
            .insert("netId".to_string(), "2".to_string())
            .expect("netId");
        let router_info = bundle
            .sign_router_info(
                Date::from_millis(wall_secs().saturating_mul(1000)),
                vec![address],
                Vec::new(),
                ri_options.build().expect("router info options"),
            )
            .expect("sign");
        let router_info_bytes = router_info
            .encode_to_vec(i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
            .expect("encode");
        LocalKeys {
            bundle,
            hash,
            static_bytes,
            static_public,
            intro,
            router_info,
            router_info_bytes,
        }
    }

    fn daemon_ssu2_config() -> crate::config::Ssu2Config {
        let text = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n";
        crate::config::Config::parse(text)
            .expect("strict profile")
            .ssu2
    }

    async fn start_daemon_pair() -> (
        Ssu2DaemonHandle,
        Ssu2DaemonHandle,
        ChildScope,
        ChildScope,
        LocalKeys,
        LocalKeys,
    ) {
        let keys_a = make_local_keys(44011);
        let keys_b = make_local_keys(44012);
        let config = daemon_ssu2_config();
        let service_a = Ssu2DaemonService::new(
            &config,
            Ssu2IdentityMaterial {
                router_hash: keys_a.hash,
                static_secret_bytes: keys_a.static_bytes,
                intro_key: keys_a.intro,
                router_info: keys_a.router_info_bytes.clone(),
            },
        )
        .expect("daemon service a");
        let service_b = Ssu2DaemonService::new(
            &config,
            Ssu2IdentityMaterial {
                router_hash: keys_b.hash,
                static_secret_bytes: keys_b.static_bytes,
                intro_key: keys_b.intro,
                router_info: keys_b.router_info_bytes.clone(),
            },
        )
        .expect("daemon service b");
        let token_a = CancellationToken::new();
        let token_b = CancellationToken::new();
        let scope_a = ChildScope::for_test(&token_a, ChildFailurePolicy::FailParent);
        let scope_b = ChildScope::for_test(&token_b, ChildFailurePolicy::FailParent);
        let handle_a = service_a
            .start(
                &scope_a,
                &crate::config::Ssu2Config {
                    port: 0,
                    ..config.clone()
                },
            )
            .await
            .expect("bind a");
        let handle_b = service_b
            .start(
                &scope_b,
                &crate::config::Ssu2Config {
                    port: 0,
                    ..config.clone()
                },
            )
            .await
            .expect("bind b");
        let _ = handle_a.local_v4().expect("a bound");
        let _ = handle_b.local_v4().expect("b bound");
        (handle_a, handle_b, scope_a, scope_b, keys_a, keys_b)
    }

    async fn wait_for_active(handle: &Ssu2DaemonHandle, expected: usize) {
        let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
        loop {
            if handle.snapshot().active_sessions == expected {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "active sessions did not reach {expected}"
            );
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    async fn next_inbound_with_timeout(
        handle: &mut Ssu2DaemonHandle,
        timeout: Duration,
    ) -> Option<Ssu2InboundI2np> {
        tokio::time::timeout(timeout, handle.next_inbound())
            .await
            .unwrap_or_default()
    }

    /// The controlled peer that answers a short tunnel build.
    ///
    /// Plan 505: the hop is processed with the peer's **identity**
    /// encryption key — the key the production `peer_static_encryption_key`
    /// seam publishes as the peer's public static encryption key.
    struct BuildResponder {
        /// The peer's identity encryption secret.
        identity_secret: [u8; 32],
        /// The peer's router hash.
        hash: Hash,
    }

    impl BuildResponder {
        /// Returns the message id the reply must carry.
        ///
        /// The reply is correlated by the *inner* message id the creator
        /// reserved, which the request record carries as its next message
        /// id. The responder therefore opens its own record and reads the
        /// id, instead of guessing one.
        fn reply_message_id(&self, payload: &[u8]) -> Option<u32> {
            let count = *payload.first()?;
            let records = payload.get(1..)?;
            let crypto = EciesX25519BuildCryptography::new();
            for index in 0..usize::from(count) {
                let start = index.checked_mul(i2pr_proto::SHORT_BUILD_RECORD_SIZE)?;
                let record =
                    records.get(start..start.checked_add(i2pr_proto::SHORT_BUILD_RECORD_SIZE)?)?;
                if record[..i2pr_tunnel::build_crypto::HASH_PREFIX_LEN]
                    != self.hash.as_bytes()[..i2pr_tunnel::build_crypto::HASH_PREFIX_LEN]
                {
                    continue;
                }
                let opened = crypto
                    .open_short_request(record, &self.identity_secret, self.hash.as_bytes())
                    .ok()?;
                let decoded = ShortRequestRecord::decode(opened.plaintext.as_ref()).ok()?;
                return Some(decoded.next_message_id());
            }
            None
        }

        /// Processes one inbound build and returns the reply wire.
        fn process_build(&self, request_payload: &[u8]) -> Option<Vec<u8>> {
            let message_id = self.reply_message_id(request_payload)?;
            let crypto = EciesX25519BuildCryptography::new();
            let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(wall_secs());
            let (reply_payload, _result) = MessageHopProcessor::process_hop(
                &crypto,
                request_payload,
                &self.identity_secret,
                &self.hash,
                ShortResponseCode::Accepted,
                &mut rng,
            )
            .ok()?;
            let count = *reply_payload.first()?;
            let records = reply_payload.get(1..)?.to_vec();
            let deferred = i2pr_proto::DeferredBuildRecords::new(
                count,
                i2pr_proto::SHORT_BUILD_RECORD_SIZE,
                records,
            )
            .ok()?;
            let body = I2npBody::OutboundTunnelBuildReply(deferred);
            let expiration_secs = wall_secs().saturating_add(60) as u32;
            let message =
                I2npMessage::new_short_transport(message_id, expiration_secs, body).expect("OTBRM");
            message
                .encode_short_transport_to_vec(MAX_I2NP_MESSAGE_BYTES)
                .ok()
        }
    }

    async fn pump_responder_loop(
        mut handle: Ssu2DaemonHandle,
        delivery: crate::router_i2np::RouterDeliveryService,
        responder: BuildResponder,
        shutdown: CancellationToken,
    ) {
        loop {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => return,
                inbound = handle.next_inbound() => {
                    let Some(inbound) = inbound else {
                        return;
                    };
                    let now_ms = wall_ms();
                    let dispatch = match dispatch_router_i2np(&inbound, now_ms) {
                        Ok(value) => value,
                        Err(_) => continue,
                    };
                    if !matches!(
                        dispatch,
                        RouterI2npOutcome::TunnelBuildReserved {
                            kind: RouterI2npKind::ShortTunnelBuild,
                            ..
                        }
                    ) {
                        continue;
                    }
                    let message =
                        match I2npMessage::decode_short_transport(&inbound.bytes, MAX_I2NP_MESSAGE_BYTES)
                        {
                            Ok(message) => message,
                            Err(_) => continue,
                        };
                    let body = match message.body() {
                        I2npBody::ShortTunnelBuild(records) => records,
                        _ => continue,
                    };
                    let expected = usize::from(body.count())
                        .saturating_mul(usize::from(body.record_size()));
                    if body.records().len() != expected {
                        continue;
                    }
                    let mut payload = Vec::with_capacity(1 + expected);
                    payload.push(body.count());
                    payload.extend_from_slice(body.records());
                    let Some(reply_wire) = responder.process_build(&payload) else {
                        continue;
                    };
                    let request = match RouterDeliveryRequest::new(
                        inbound.peer,
                        reply_wire,
                        Duration::from_secs(5),
                    ) {
                        Ok(request) => request,
                        Err(_) => continue,
                    };
                    let _ = delivery.deliver(request, &CancellationToken::new());
                }
            }
        }
    }

    /// Drains the inbound path until the coordinator produces an outcome.
    async fn drain_build_outcomes(
        handle: &mut Ssu2DaemonHandle,
        coordinator: &mut ExploratoryBuildCoordinator,
        peer_hash: Hash,
    ) -> Vec<BuildCoordinatorOutcome> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        while tokio::time::Instant::now() < deadline {
            let Some(inbound) = next_inbound_with_timeout(handle, POLL_INTERVAL * 4).await else {
                continue;
            };
            let now_ms = wall_ms();
            let dispatch = match dispatch_router_i2np(&inbound, now_ms) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if !matches!(
                dispatch,
                RouterI2npOutcome::TunnelBuildReserved {
                    kind: RouterI2npKind::OutboundTunnelBuildReply,
                    ..
                }
            ) {
                continue;
            }
            let wrapped = Ssu2InboundI2np {
                link_id: LinkId::new(1).expect("link"),
                peer: PeerId::from_hash(peer_hash),
                bytes: inbound.bytes.clone(),
            };
            let routed = coordinator
                .route_inbound_i2np(&wrapped, now_ms)
                .expect("dispatcher outcome");
            if !routed.coordinator.is_empty() {
                return routed.coordinator;
            }
        }
        Vec::new()
    }

    /// Builds one installed registration, so the `Installed` variant can be
    /// constructed in the mapping test.
    fn registration() -> i2pr_tunnel::pool::TunnelRegistration {
        i2pr_tunnel::pool::TunnelRegistration::new(
            i2pr_tunnel::pool::TunnelSlot::from_raw(1),
            i2pr_tunnel::identity::TunnelId::new(0x1234).expect("tunnel id"),
            i2pr_tunnel::identity::TunnelDirection::Outbound,
            i2pr_tunnel::identity::TunnelState::Established,
            vec![i2pr_tunnel::identity::TunnelPeer::from_hash(
                Hash::from_bytes([0x11_u8; 32]),
            )],
            0,
        )
        .expect("registration")
    }

    /// One representative of every [`BuildCoordinatorOutcome`] variant.
    fn every_outcome() -> Vec<(&'static str, BuildCoordinatorOutcome)> {
        let destination_id = i2pr_client::DestinationId::from_hash(Hash::from_bytes([0x22_u8; 32]));
        vec![
            (
                "Installed",
                BuildCoordinatorOutcome::Installed {
                    slot: i2pr_tunnel::pool::TunnelSlot::from_raw(1),
                    registration: registration(),
                    hop_count: 1,
                    direction: BuildDirection::Outbound,
                },
            ),
            (
                "DestinationEstablished",
                BuildCoordinatorOutcome::DestinationEstablished {
                    attempt_id: BuildAttemptId::new(1),
                    destination_id,
                    hop_count: 1,
                    direction: BuildDirection::Outbound,
                },
            ),
            (
                "DestinationHandoffRejected",
                BuildCoordinatorOutcome::DestinationHandoffRejected {
                    direction: BuildDirection::Outbound,
                },
            ),
            (
                "DestinationBuildFailed",
                BuildCoordinatorOutcome::DestinationBuildFailed {
                    destination_id,
                    direction: BuildDirection::Outbound,
                },
            ),
            (
                "DestinationBuildCancelled",
                BuildCoordinatorOutcome::DestinationBuildCancelled {
                    destination_id,
                    direction: BuildDirection::Outbound,
                },
            ),
            (
                "PoolRejected",
                BuildCoordinatorOutcome::PoolRejected {
                    direction: BuildDirection::Outbound,
                    kind: i2pr_tunnel::pool::PoolFullError::OutboundPoolFull,
                },
            ),
            (
                "InvalidRegistration",
                BuildCoordinatorOutcome::InvalidRegistration {
                    direction: BuildDirection::Outbound,
                    reason: "invalid material",
                },
            ),
            (
                "HopRejected",
                BuildCoordinatorOutcome::HopRejected {
                    direction: BuildDirection::Outbound,
                    hop_index: 0,
                    response_code: 0,
                },
            ),
            (
                "TimedOut",
                BuildCoordinatorOutcome::TimedOut {
                    direction: BuildDirection::Outbound,
                },
            ),
            (
                "Cancelled",
                BuildCoordinatorOutcome::Cancelled {
                    direction: BuildDirection::Outbound,
                },
            ),
            (
                "InvalidReply",
                BuildCoordinatorOutcome::InvalidReply {
                    attempt_id: BuildAttemptId::new(1),
                    direction: BuildDirection::Outbound,
                    reason: "malformed",
                },
            ),
            (
                "DeliveryFailed",
                BuildCoordinatorOutcome::DeliveryFailed {
                    direction: BuildDirection::Outbound,
                    delivery: crate::router_i2np::RouterDeliveryOutcome::NoActiveSession,
                },
            ),
            (
                "InvalidPath",
                BuildCoordinatorOutcome::InvalidPath {
                    direction: BuildDirection::Outbound,
                    reason: "invalid path",
                },
            ),
            (
                "CoordinatorRejected",
                BuildCoordinatorOutcome::CoordinatorRejected {
                    direction: BuildDirection::Outbound,
                    reason: "too many pending builds",
                },
            ),
        ]
    }

    /// The mapping is total: every variant of the coordinator's outcome
    /// enum produces a peer observation, and no variant is dropped.
    #[test]
    fn the_outcome_mapping_is_total() {
        let outcomes = every_outcome();
        assert_eq!(
            outcomes.len(),
            14,
            "the coordinator's outcome enum has fourteen variants"
        );
        for (label, outcome) in &outcomes {
            let observation = super::observation_for_outcome(outcome);
            assert!(
                !observation.label().is_empty(),
                "{label} must map to a labelled observation"
            );
        }
        // The intended mapping, pinned per variant.
        //
        // The five local-condition variants map to `Local`, **not** to
        // `Misbehaving`: the local pool refusing a registration, an invalid
        // local registration, a local coordinator rejection, an invalid path
        // and a local cancellation are **this router's** faults. Writing any
        // of them into the peer's profile would blame the peer for a
        // condition the peer did not cause.
        let expected = [
            ("Installed", super::PeerObservation::Answered),
            (
                "DestinationEstablished",
                super::PeerObservation::Misbehaving,
            ),
            (
                "DestinationHandoffRejected",
                super::PeerObservation::Misbehaving,
            ),
            (
                "DestinationBuildFailed",
                super::PeerObservation::Misbehaving,
            ),
            (
                "DestinationBuildCancelled",
                super::PeerObservation::Misbehaving,
            ),
            ("PoolRejected", super::PeerObservation::Local),
            ("InvalidRegistration", super::PeerObservation::Local),
            ("HopRejected", super::PeerObservation::Refused),
            ("TimedOut", super::PeerObservation::Unanswered),
            ("Cancelled", super::PeerObservation::Local),
            ("InvalidReply", super::PeerObservation::Misbehaving),
            ("DeliveryFailed", super::PeerObservation::Unreachable),
            ("InvalidPath", super::PeerObservation::Local),
            ("CoordinatorRejected", super::PeerObservation::Local),
        ];
        assert_eq!(outcomes.len(), expected.len());
        for ((label, outcome), (expected_label, expected_observation)) in
            outcomes.iter().zip(expected.iter())
        {
            assert_eq!(*label, *expected_label);
            assert_eq!(
                super::observation_for_outcome(outcome),
                *expected_observation
            );
        }
    }

    /// A destination-scoped outcome is never the driver's own outcome.
    #[test]
    fn a_destination_outcome_is_never_attributed_to_the_driver() {
        let (_, outcome) = every_outcome()
            .into_iter()
            .find(|(label, _)| *label == "DestinationBuildFailed")
            .expect("the destination outcome");
        assert!(!super::is_exploratory_scoped(&outcome));
    }

    /// **Plan 505's safety property on the peer profile.** A terminal outcome
    /// that is *this router's* condition — the local pool refused the
    /// registration, the local registration was invalid, the coordinator
    /// refused the submission, the path was invalid, or the attempt was
    /// cancelled locally — must be **counted** and must **not** be written
    /// into the peer's profile. The router must never blame a peer for a
    /// fault of its own.
    ///
    /// The contrast is asserted in the same test: a genuine peer fault *does*
    /// write an observation, so the local case is a deliberate omission
    /// rather than a broken recorder.
    #[test]
    fn a_local_condition_never_writes_the_peers_profile() {
        let profiles = PeerParticipationHandle::new(PeerParticipationDriver::new(
            ProfileStoreConfig::default(),
        ));
        let resolver: Arc<dyn PeerBuildIdentityResolver + Send + Sync> =
            Arc::new(ProfileStorePeerResolver::new(profiles.clone()));
        let mut driver = ExploratoryDriver::new(true, profiles.clone(), resolver);

        // A real peer the store holds, so a write would be recorded.
        let keys = make_local_keys(44017);
        let validated = ValidatedRouterInfo::from_router_info(
            keys.router_info.clone(),
            None,
            i2pr_netdb::ValidationContext::new(Date::from_millis(wall_ms())),
        )
        .expect("the peer RouterInfo must validate");
        let peer_key = validated.key();
        profiles.with(|store| {
            store.advance_time(wall_ms());
            store.learn(validated);
        });
        let receipts_before = profiles.with(|store| store.receipt_count()).unwrap_or(0);

        // The router's own condition: counted, and **no** profile write.
        driver.record_observation(peer_key, super::PeerObservation::Local);
        assert_eq!(driver.counters().local_outcomes, 1);
        assert_eq!(driver.counters().misbehaving, 0);
        assert_eq!(driver.counters().answered, 0);
        assert_eq!(
            profiles.with(|store| store.receipt_count()).unwrap_or(0),
            receipts_before,
            "a local condition must not write an observation receipt"
        );

        // The contrast: a genuine peer fault **does** write one.
        driver.record_observation(peer_key, super::PeerObservation::Misbehaving);
        assert_eq!(driver.counters().misbehaving, 1);
        assert_eq!(
            profiles.with(|store| store.receipt_count()).unwrap_or(0),
            receipts_before + 1,
            "a peer fault must write an observation receipt"
        );
    }

    /// An outcome with no in-flight record is an orphan: it is counted and
    /// produces no observation.
    #[test]
    fn an_outcome_without_an_in_flight_record_is_an_orphan() {
        let profiles = PeerParticipationHandle::new(PeerParticipationDriver::new(
            ProfileStoreConfig::default(),
        ));
        let resolver: Arc<dyn PeerBuildIdentityResolver + Send + Sync> =
            Arc::new(ProfileStorePeerResolver::new(profiles.clone()));
        let mut driver = ExploratoryDriver::new(true, profiles, resolver);
        let (_, outcome) = every_outcome()
            .into_iter()
            .find(|(label, _)| *label == "TimedOut")
            .expect("the timeout outcome");
        assert!(!driver.observe_outcome(&outcome));
        assert_eq!(driver.counters().orphan_outcomes, 1);
        assert_eq!(driver.counters().outcomes_observed, 0);
        assert_eq!(driver.counters().unanswered, 0);
    }

    /// A disabled driver never selects, never builds and never observes.
    #[test]
    fn a_disabled_driver_refuses_every_tick() {
        let profiles = PeerParticipationHandle::new(PeerParticipationDriver::new(
            ProfileStoreConfig::default(),
        ));
        let resolver: Arc<dyn PeerBuildIdentityResolver + Send + Sync> =
            Arc::new(ProfileStorePeerResolver::new(profiles.clone()));
        let mut driver = ExploratoryDriver::new(false, profiles, resolver);
        let mut coordinator = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
        coordinator.advance_time(wall_ms());
        let bridge = ShortBuildI2npBridge::new();
        // The delivery seam is never reached: the driver refuses first. The
        // service is still a real one, so the refusal cannot be an artefact
        // of a stub.
        let keys = make_local_keys(44013);
        let runtime = i2pr_runtime::Ssu2RuntimeService::new(
            crate::router_i2np::ssu2_runtime_config_from(&daemon_ssu2_config())
                .expect("runtime config"),
            Ssu2IdentityMaterial {
                router_hash: keys.hash,
                static_secret_bytes: keys.static_bytes,
                intro_key: keys.intro,
                router_info: keys.router_info_bytes.clone(),
            },
        )
        .expect("runtime service");
        let delivery = RouterDeliveryService::new(runtime);
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let action = driver.drive(
            wall_ms(),
            &mut coordinator,
            &delivery,
            &bridge,
            None,
            &mut rng,
        );
        assert_eq!(action, ExploratoryDriverAction::Disabled);
        assert_eq!(driver.counters().disabled, 1);
        assert_eq!(driver.counters().selections, 0);
        assert_eq!(driver.counters().attempts, 0);
    }

    /// Plan 505 WP1 — the feasibility gate.
    ///
    /// The whole loop runs against the real coordinator, the real
    /// peer-profile store and the real delivery seam.
    #[tokio::test]
    async fn exploratory_driver_builds_installs_and_records_a_liveness_pair() {
        let (mut handle_a, handle_b, scope_a, scope_b, keys_a, keys_b) = start_daemon_pair().await;
        let addr_b = handle_b.local_v4().expect("b addr");
        let target = i2pr_runtime::Ssu2DialTarget::new(
            PeerId::from_hash(keys_b.hash),
            keys_b.hash,
            addr_b,
            keys_b.static_public,
            keys_b.intro,
        )
        .expect("dial target");
        let _established = handle_a
            .service()
            .dial_ssu2(target, DIAL_TIMEOUT, &CancellationToken::new())
            .await
            .expect("dial establishes");
        wait_for_active(&handle_a, 1).await;
        wait_for_active(&handle_b, 1).await;

        // The responder answers with the peer's **identity** encryption key.
        let responder = BuildResponder {
            identity_secret: *keys_b.bundle.encryption_key().secret_bytes(),
            hash: keys_b.hash,
        };
        let responder_delivery = handle_b.delivery().clone();
        let responder_token = CancellationToken::new();
        let responder_task = tokio::spawn(pump_responder_loop(
            handle_b,
            responder_delivery,
            responder,
            responder_token.clone(),
        ));

        // The **real** validated RouterInfo store holds the peer.
        let mut validated_store =
            RouterInfoStore::with_config(RouterInfoStoreConfig::new(64, 1 << 20));
        let validated = ValidatedRouterInfo::from_router_info(
            keys_b.router_info.clone(),
            None,
            i2pr_netdb::ValidationContext::new(Date::from_millis(wall_ms())),
        )
        .expect("the peer RouterInfo must validate");
        let peer_key = validated.key();
        validated_store.insert(validated);

        // The **real** peer-profile driver adopts that store, so the
        // selection and the resolver can never disagree about the peer.
        let participation = PeerParticipationDriver::adopt(
            ProfileStoreConfig::default(),
            validated_store,
            wall_ms(),
        );
        let profiles = PeerParticipationHandle::new(participation);

        // The resolver is the production one: the peer-profile store's own
        // validated records.
        let resolver: Arc<dyn PeerBuildIdentityResolver + Send + Sync> =
            Arc::new(ProfileStorePeerResolver::new(profiles.clone()));
        let resolver_for_assert = Arc::clone(&resolver);

        // The driver under test.
        let mut driver = ExploratoryDriver::new(true, profiles.clone(), resolver);

        let mut coordinator = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
        coordinator.advance_time(wall_ms());
        let bridge = ShortBuildI2npBridge::new();
        let delivery = handle_a.delivery().clone();
        let mut rng = ChaCha8Rng::seed_from_u64(wall_secs().wrapping_add(23));

        // The selection resolves the peer's **identity** public key.
        let material = resolver_for_assert
            .resolve(&peer_key)
            .expect("the peer material must resolve");
        assert_eq!(material.router_hash, keys_b.hash);
        assert_eq!(
            material.static_encryption_key.as_slice(),
            keys_b
                .bundle
                .encryption_key()
                .public_key()
                .expect("identity public key")
                .as_bytes(),
            "the driver must use the RouterInfo identity public key"
        );
        assert_ne!(
            material.static_encryption_key.as_slice(),
            keys_b.static_public.as_bytes(),
            "the production key is not the SSU2 address key"
        );

        // The first tick selects the peer and submits the outbound build.
        let action = driver.drive(
            wall_ms(),
            &mut coordinator,
            &delivery,
            &bridge,
            Some(keys_a.hash),
            &mut rng,
        );
        assert_eq!(action, ExploratoryDriverAction::SubmittedOutbound);
        assert_eq!(coordinator.pending_len(), 1);
        assert_eq!(driver.counters().attempts, 1);
        assert_eq!(driver.counters().selections, 1);

        // The real inbound path classifies the reply and the coordinator
        // installs the tunnel.
        let outcomes = drain_build_outcomes(&mut handle_a, &mut coordinator, keys_b.hash).await;
        assert!(
            outcomes
                .iter()
                .any(|outcome| matches!(outcome, BuildCoordinatorOutcome::Installed { .. })),
            "the outbound build must install"
        );
        assert_eq!(driver.observe_outcomes(&outcomes), 1);
        assert_eq!(coordinator.outbound_pool_len(), 1);
        assert_eq!(coordinator.inbound_pool_len(), 0);
        assert_eq!(driver.counters().installed, 1);
        assert_eq!(driver.counters().answered, 1);

        // The second tick builds the inbound half toward the **same** peer.
        let action = driver.drive(
            wall_ms(),
            &mut coordinator,
            &delivery,
            &bridge,
            Some(keys_a.hash),
            &mut rng,
        );
        assert_eq!(action, ExploratoryDriverAction::SubmittedInbound);
        assert_eq!(
            driver.counters().selections,
            1,
            "the peer is chosen once per pair"
        );

        let outcomes = drain_build_outcomes(&mut handle_a, &mut coordinator, keys_b.hash).await;
        assert!(
            outcomes
                .iter()
                .any(|outcome| matches!(outcome, BuildCoordinatorOutcome::Installed { .. })),
            "the inbound build must install"
        );
        assert_eq!(driver.observe_outcomes(&outcomes), 1);
        assert_eq!(coordinator.outbound_pool_len(), 1);
        assert_eq!(coordinator.inbound_pool_len(), 1);
        assert_eq!(driver.counters().installed, 2);
        assert_eq!(driver.counters().pairs_completed, 1);

        // The pool and the registry own the installed material.
        let pairs = coordinator.exploratory_liveness_pairs();
        assert_eq!(
            pairs.len(),
            1,
            "the coordinator must report one liveness pair"
        );
        let (outbound_slot, inbound_slot) = pairs[0];
        let first_hop = coordinator
            .registry_outbound_first_hop(outbound_slot)
            .expect("outbound first-hop metadata");
        assert_eq!(first_hop.0, keys_b.hash);

        // The liveness scheduler sees the pair.
        let mut scheduler = TunnelLivenessScheduler::new(LivenessConfig::default());
        scheduler.advance_time(wall_ms());
        scheduler
            .register_pair(outbound_slot, inbound_slot)
            .expect("the pair must register");
        assert_eq!(scheduler.active_pairs(), 1);

        // The profile store received the observation.
        let profile = profiles
            .with(|participation| participation.store().profile(&peer_key).cloned())
            .expect("the store lock");
        let profile = profile.expect("the peer profile");
        assert_eq!(profile.state(), ProfileState::Answered);
        assert!(profile.answered_count() >= 1);

        // The counters moved.
        let counters = driver.counters();
        assert_eq!(counters.attempts, 2);
        assert_eq!(counters.outcomes_observed, 2);
        assert_eq!(counters.answered, 2);
        assert_eq!(counters.orphan_outcomes, 0);
        assert_eq!(counters.no_eligible_peer, 0);
        assert_eq!(counters.peer_material_absent, 0);
        assert_eq!(counters.peer_not_reachable, 0);
        assert_eq!(counters.refusals_unknown_version, 0);
        assert_eq!(counters.refusals_below_floor, 0);

        // A third tick refuses: the pool already holds a usable pair.
        let action = driver.drive(
            wall_ms(),
            &mut coordinator,
            &delivery,
            &bridge,
            Some(keys_a.hash),
            &mut rng,
        );
        assert_eq!(action, ExploratoryDriverAction::PoolSatisfied);
        assert_eq!(driver.counters().pool_satisfied, 1);

        responder_token.cancel(i2pr_runtime::CancellationReason::OperatorRequest);
        let _ = responder_task.await;
        let _ = scope_a.shutdown().await;
        let _ = scope_b.shutdown().await;
    }
}
