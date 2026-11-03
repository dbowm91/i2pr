//! Plan 515 — the router's **own** RouterInfo publication.
//!
//! Until this plan the router published its own RouterInfo **once**, from
//! the service-tunnel provisioning path, and never again. A record that is
//! never refreshed ages out of the network's view, so no peer can find the
//! router however long it has been running — and a router with no service
//! tunnels never published at all.
//!
//! The reference republishes on a timer
//! (`ROUTER_INFO_PUBLISH_INTERVAL = 39 * 60` seconds,
//! `libi2pd/RouterContext.h:34`) to the **closest** floodfill it has not
//! already asked this cycle (`libi2pd/RouterContext.cpp:1502`).
//!
//! This module owns three things:
//!
//! 1. the **schedule** — a period, a due test, and the counted outcomes.
//!    It is runtime-neutral: it takes `now_ms` from its caller and never
//!    reads a clock, which is what makes it testable without a wall
//!    clock, a socket, or a sleep;
//! 2. the **selection** — the closest version-gated floodfills, which is
//!    the **same** rule the destination coordinator applies
//!    (`i2pr_netdb::select_version_gated_floodfills`), so Plan 503's
//!    API-version floor holds for every publication;
//! 3. the **delivery** — the one path that puts the store on the wire,
//!    shared by the service product's one-shot publication and by the
//!    router-level refresh, so there is one publication and one set of
//!    rules rather than two.
//!
//! Plan 516 adds the **acknowledgement**. The store message carries the
//! correlator's minted reply token, in the reference's direct-reply form,
//! so the floodfill answers with a `DeliveryStatus` that carries the token
//! back. The publication's outcome stays a **delivery** outcome; the
//! acknowledgement is a separate fact, counted separately, and it arrives
//! on the inbound path rather than inside the publication's own call.
//!
//! Nothing here advertises anything: `router.version` stays `0.9.62`, the
//! published RouterInfo's content is unchanged, and the inventory's
//! advertised count stays zero.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use i2pr_netdb::{
    LookupPolicy, PublicationCoordinator, PublicationLocalRecord, RouterHash, RouterInfoStore,
    ValidatedRouterInfo, VersionGatedFloodfillSelection, select_version_gated_floodfills,
};
use rand_core::RngCore;

/// The reference's publication interval: `39 * 60` seconds
/// (`libi2pd/RouterContext.h:34`).
///
/// The reference adds up to `105` seconds of variance to it
/// (`libi2pd/RouterContext.h:36`). This plan does **not**: a deterministic
/// period is testable, and the variance exists to de-synchronise a
/// floodfill with the network, which a router that is not a floodfill has
/// no need for.
pub const PUBLICATION_INTERVAL_MS: u64 = 39 * 60 * 1_000;

/// The number of nearest floodfills that receive one publication.
///
/// The reference asks one floodfill per publish and then, every
/// `ROUTER_INFO_CONFIRMATION_TIMEOUT` milliseconds, the next closest ones
/// — up to `ROUTER_INFO_MAX_PUBLISH_EXCLUDED_FLOODFILLS` (`15`,
/// `libi2pd/RouterContext.h:38`). This plan asks the closest few in one
/// pass, which is not weaker: it reaches several peers immediately where
/// the reference reaches one and then waits.
pub const PUBLICATION_FLOODFILLS: usize = 3;

/// The outcome of one publication attempt.
///
/// Every outcome is named and counted. A publication that was refused is
/// never counted as a publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterInfoPublicationOutcome {
    /// The store reached at least one target.
    Published {
        /// The number of targets the publication named.
        targets: usize,
        /// The number of targets that accepted the store.
        accepted: usize,
    },
    /// The router holds no local RouterInfo to publish.
    RefusedNoLocalRecord,
    /// No floodfill was eligible: the store held none that advertise the
    /// floodfill capability, or every one of them was refused by the
    /// API-version floor.
    RefusedNoTarget,
    /// The transport refused every delivery.
    DeliveryRefused {
        /// The number of targets the publication named.
        targets: usize,
    },
    /// The publication was cancelled before it completed.
    Cancelled,
}

impl RouterInfoPublicationOutcome {
    /// Whether the outcome is a publication that reached a peer.
    pub const fn is_published(&self) -> bool {
        matches!(self, Self::Published { .. })
    }
}

/// The counted publication outcomes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RouterInfoPublicationCounters {
    /// The number of publication attempts.
    pub attempts: u64,
    /// The number of attempts that reached a peer.
    pub published: u64,
    /// The number of attempts refused because the router held no local
    /// RouterInfo.
    pub refused_no_local_record: u64,
    /// The number of attempts refused because no target was eligible.
    pub refused_no_target: u64,
    /// The number of attempts refused by the transport.
    pub deliveries_refused: u64,
    /// The number of attempts cancelled before they completed.
    pub cancelled: u64,
    /// The number of publications the network acknowledged.
    ///
    /// Plan 516: an acknowledgement is a **separate** fact from a delivery.
    /// A store the transport accepted is not a record the network holds.
    pub confirmed: u64,
    /// The number of acknowledgements that matched no live attempt.
    ///
    /// Plan 516: an unmatched acknowledgement is refused and counted under
    /// its own reason. It never confirms anything and is never dropped
    /// silently.
    pub unmatched: u64,
}

/// The outcome of one inbound acknowledgement.
///
/// Plan 516.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationConfirmation {
    /// The acknowledgement matched a live attempt.
    Confirmed {
        /// The request identifier of the confirmed attempt.
        request_id: u64,
    },
    /// The acknowledgement matched no live attempt, and was refused.
    Unmatched,
}

/// Correlates one inbound `DeliveryStatus` with the live attempts.
///
/// Plan 516: this is the runtime-neutral seam between the inbound path
/// and the correlator. An acknowledgement whose token matches no live
/// attempt is **refused** — it confirms nothing, and it is reported as
/// [`PublicationConfirmation::Unmatched`] so the caller can count it under
/// its own reason rather than drop it.
pub fn confirm_publication(
    coordinator: &mut PublicationCoordinator,
    message_id: u32,
) -> PublicationConfirmation {
    match coordinator.correlate_delivery_status(message_id) {
        Ok(correlation) => PublicationConfirmation::Confirmed {
            request_id: correlation.request_id,
        },
        Err(_) => PublicationConfirmation::Unmatched,
    }
}

/// The runtime-neutral publication schedule.
///
/// The schedule owns the period, the last attempt's clock reading, and the
/// counters. It never reads a clock: the caller passes `now_ms`, so the
/// whole schedule is provable with an explicit time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouterInfoPublicationSchedule {
    period_ms: u64,
    last_attempt_ms: Option<u64>,
    counters: RouterInfoPublicationCounters,
}

impl RouterInfoPublicationSchedule {
    /// Constructs a schedule with the given period.
    pub fn new(period_ms: u64) -> Self {
        Self {
            period_ms,
            last_attempt_ms: None,
            counters: RouterInfoPublicationCounters::default(),
        }
    }

    /// The schedule's period, in milliseconds.
    pub const fn period_ms(&self) -> u64 {
        self.period_ms
    }

    /// Whether a publication is due at `now_ms`.
    ///
    /// The first publication is due at the schedule's own start: a router
    /// that has a record to publish should publish it, not sit on it for a
    /// period. After that, a publication is due exactly one period after
    /// the last attempt.
    pub fn is_due(&self, now_ms: u64) -> bool {
        match self.last_attempt_ms {
            None => true,
            Some(last) => now_ms.saturating_sub(last) >= self.period_ms,
        }
    }

    /// Records one inbound acknowledgement.
    ///
    /// Plan 516: a confirmed acknowledgement and an unmatched one are
    /// counted under their own reasons, and neither is counted as a
    /// publication.
    pub fn record_confirmation(&mut self, outcome: PublicationConfirmation) {
        match outcome {
            PublicationConfirmation::Confirmed { .. } => {
                self.counters.confirmed = self.counters.confirmed.saturating_add(1);
            }
            PublicationConfirmation::Unmatched => {
                self.counters.unmatched = self.counters.unmatched.saturating_add(1);
            }
        }
    }

    /// Records one attempt and its outcome.
    ///
    /// The attempt is counted whatever its outcome, so a refusal is
    /// visible rather than silent, and the next attempt is one period
    /// after this one.
    pub fn record(&mut self, now_ms: u64, outcome: RouterInfoPublicationOutcome) {
        self.last_attempt_ms = Some(now_ms);
        self.counters.attempts = self.counters.attempts.saturating_add(1);
        match outcome {
            RouterInfoPublicationOutcome::Published { .. } => {
                self.counters.published = self.counters.published.saturating_add(1);
            }
            RouterInfoPublicationOutcome::RefusedNoLocalRecord => {
                self.counters.refused_no_local_record =
                    self.counters.refused_no_local_record.saturating_add(1);
            }
            RouterInfoPublicationOutcome::RefusedNoTarget => {
                self.counters.refused_no_target = self.counters.refused_no_target.saturating_add(1);
            }
            RouterInfoPublicationOutcome::DeliveryRefused { .. } => {
                self.counters.deliveries_refused =
                    self.counters.deliveries_refused.saturating_add(1);
            }
            RouterInfoPublicationOutcome::Cancelled => {
                self.counters.cancelled = self.counters.cancelled.saturating_add(1);
            }
        }
    }

    /// The counted outcomes.
    pub const fn counters(&self) -> RouterInfoPublicationCounters {
        self.counters
    }

    /// The clock reading of the last attempt, if there was one.
    pub const fn last_attempt_ms(&self) -> Option<u64> {
        self.last_attempt_ms
    }
}

/// The closest floodfills that may receive one publication.
///
/// The rule is the reference's and the coordinator's: the routing key is
/// **our own** hash, our own hash is excluded, and every candidate goes
/// through Plan 503's API-version floor. The refusal counts travel with
/// the answer, so a publication that was refused by the version gate is
/// distinguishable from one that found no floodfill at all.
pub fn publication_targets(
    store: &RouterInfoStore,
    local_hash: &RouterHash,
    policy: &LookupPolicy,
) -> VersionGatedFloodfillSelection {
    select_version_gated_floodfills(store, local_hash, &[*local_hash], policy)
}

/// The plan for one publication: the targets and the store, or the refusal.
///
/// The plan is built **before** anything is sent, so a publication that
/// cannot happen is refused rather than half-performed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouterInfoPublicationPlan {
    /// The publication may go ahead.
    Publish {
        /// The targets, nearest first, bounded by [`PUBLICATION_FLOODFILLS`].
        targets: Vec<RouterHash>,
        /// The encoded RouterInfo the publication publishes.
        ///
        /// Plan 516: the plan names the **record**, not the store message.
        /// The store message is composed by the correlator, once per
        /// target, with that target's own minted reply token.
        record: Vec<u8>,
    },
    /// The publication is refused, with the reason.
    Refused(RouterInfoPublicationOutcome),
}

impl RouterInfoPublicationPlan {
    /// Builds the publication plan for a named target set and record.
    ///
    /// Plan 516: the plan names the **record**, not the store message. The
    /// store message is composed by the correlator, once per target, with
    /// that target's own minted reply token.
    pub fn publish(targets: Vec<RouterHash>, record: Vec<u8>) -> Self {
        Self::Publish { targets, record }
    }
}

impl RouterInfoPublicationPlan {
    /// The plan's refusal, if it is a refusal.
    pub const fn refusal(&self) -> Option<RouterInfoPublicationOutcome> {
        match self {
            Self::Publish { .. } => None,
            Self::Refused(outcome) => Some(*outcome),
        }
    }

    /// The plan's targets, if it may go ahead.
    pub fn targets(&self) -> &[RouterHash] {
        match self {
            Self::Publish { targets, .. } => targets,
            Self::Refused(_) => &[],
        }
    }
}

/// The router's own publication owner.
///
/// The owner holds the schedule, the router's own hash, and the policy
/// the selection uses. It builds a **plan**; putting the plan on the wire
/// is [`publish_router_info`]'s job, so the same owner serves the
/// service product's one-shot publication and the router-level refresh.
#[derive(Clone, Debug)]
pub struct RouterInfoPublicationOwner {
    schedule: RouterInfoPublicationSchedule,
    local_hash: RouterHash,
    policy: LookupPolicy,
    /// Plan 516: the correlator that mints the reply token, composes the
    /// store message and holds the bounded attempt book.
    coordinator: PublicationCoordinator,
}

impl RouterInfoPublicationOwner {
    /// Constructs an owner for the router's own hash.
    pub fn new(local_hash: RouterHash, period_ms: u64) -> Self {
        Self {
            schedule: RouterInfoPublicationSchedule::new(period_ms),
            local_hash,
            policy: LookupPolicy::default(),
            coordinator: PublicationCoordinator::new(LookupPolicy::default()),
        }
    }

    /// The owner's correlator, mutably.
    ///
    /// Plan 516: the correlator owns the minted tokens and the bounded
    /// attempt book, so the publication's delivery outcomes and the
    /// inbound acknowledgements are recorded in one place.
    pub fn coordinator_mut(&mut self) -> &mut PublicationCoordinator {
        &mut self.coordinator
    }

    /// Correlates one inbound acknowledgement with the live attempts.
    ///
    /// Plan 516: an acknowledgement that matches no live attempt is
    /// refused and counted under its own reason. It never confirms
    /// anything, and it is never dropped silently.
    pub fn confirm(&mut self, message_id: u32) -> PublicationConfirmation {
        let outcome = confirm_publication(&mut self.coordinator, message_id);
        self.schedule.record_confirmation(outcome);
        outcome
    }

    /// The owner's schedule.
    pub const fn schedule(&self) -> &RouterInfoPublicationSchedule {
        &self.schedule
    }

    /// The owner's schedule, mutably, so an outcome can be recorded.
    pub fn schedule_mut(&mut self) -> &mut RouterInfoPublicationSchedule {
        &mut self.schedule
    }

    /// The router's own hash.
    pub const fn local_hash(&self) -> &RouterHash {
        &self.local_hash
    }

    /// Whether a publication is due at `now_ms`.
    pub fn is_due(&self, now_ms: u64) -> bool {
        self.schedule.is_due(now_ms)
    }

    /// Plans one publication.
    ///
    /// `local_record` is the router's own installed RouterInfo. Without it
    /// there is nothing to publish, and the plan says so.
    pub fn plan(
        &self,
        store: &RouterInfoStore,
        local_record: Option<&[u8]>,
    ) -> RouterInfoPublicationPlan {
        let Some(record) = local_record else {
            return RouterInfoPublicationPlan::Refused(
                RouterInfoPublicationOutcome::RefusedNoLocalRecord,
            );
        };
        let selection = publication_targets(store, &self.local_hash, &self.policy);
        let targets: Vec<RouterHash> = selection
            .targets()
            .iter()
            .copied()
            .take(PUBLICATION_FLOODFILLS)
            .collect();
        if targets.is_empty() {
            return RouterInfoPublicationPlan::Refused(
                RouterInfoPublicationOutcome::RefusedNoTarget,
            );
        }
        RouterInfoPublicationPlan::Publish {
            targets,
            record: record.to_vec(),
        }
    }
}

/// Republishes the router's own RouterInfo, once, and records the outcome.
///
/// This is the router-level refresh: it does **not** depend on a service
/// tunnel being provisioned, so a router whose only job is to be a DHT
/// member is still findable.
///
/// The selection and the records are read under the bootstrap's lock and
/// the lock is **released** before anything is sent: a publication that
/// dials a peer can take seconds, and the bootstrap must not be held for
/// that long.
#[allow(clippy::too_many_arguments)]
pub async fn refresh_router_info<R: RngCore>(
    owner: &mut RouterInfoPublicationOwner,
    ssu2_handle: &crate::router_i2np::Ssu2DaemonHandle,
    bootstrap: Option<&Arc<Mutex<crate::bootstrap::Bootstrap>>>,
    local_record: Option<Vec<u8>>,
    dial_timeout: Duration,
    qualification_profile: crate::config::Ssu2QualificationProfile,
    rng: &mut R,
    cancellation: &i2pr_runtime::CancellationToken,
    now_ms: u64,
) -> RouterInfoPublicationOutcome {
    let outcome = publish_once(
        owner,
        ssu2_handle,
        bootstrap,
        local_record,
        dial_timeout,
        qualification_profile,
        rng,
        cancellation,
    )
    .await;
    owner.schedule_mut().record(now_ms, outcome);
    outcome
}

/// Publishes once, without touching the schedule.
#[allow(clippy::too_many_arguments)]
async fn publish_once<R: RngCore>(
    owner: &mut RouterInfoPublicationOwner,
    ssu2_handle: &crate::router_i2np::Ssu2DaemonHandle,
    bootstrap: Option<&Arc<Mutex<crate::bootstrap::Bootstrap>>>,
    local_record: Option<Vec<u8>>,
    dial_timeout: Duration,
    qualification_profile: crate::config::Ssu2QualificationProfile,
    rng: &mut R,
    cancellation: &i2pr_runtime::CancellationToken,
) -> RouterInfoPublicationOutcome {
    let Some(local_record) = local_record else {
        return RouterInfoPublicationOutcome::RefusedNoLocalRecord;
    };
    let Some(bootstrap) = bootstrap else {
        return RouterInfoPublicationOutcome::RefusedNoTarget;
    };
    let local_hash = *owner.local_hash();
    let (attempts, records) = {
        let Ok(guard) = bootstrap.lock() else {
            return RouterInfoPublicationOutcome::RefusedNoTarget;
        };
        let plan = owner.plan(guard.store(), Some(local_record.as_slice()));
        let RouterInfoPublicationPlan::Publish { targets, .. } = &plan else {
            return plan
                .refusal()
                .unwrap_or(RouterInfoPublicationOutcome::RefusedNoTarget);
        };
        let records = targets
            .iter()
            .filter_map(|hash| {
                guard
                    .store()
                    .get(hash)
                    .cloned()
                    .map(|record| (*hash, record))
            })
            .collect::<Vec<_>>();
        let attempts = begin_attempts(owner.coordinator_mut(), local_hash, &plan, guard.store());
        (attempts, records)
    };
    publish_router_info(
        owner.coordinator_mut(),
        &attempts,
        &records,
        ssu2_handle,
        dial_timeout,
        qualification_profile,
        rng,
        cancellation,
    )
    .await
}

/// The attempts of one publication, begun against the store.
///
/// Plan 516: the correlator composes each target's `DatabaseStore`, with
/// that target's own minted reply token on the wire. The attempts are
/// begun under the store's lock and sent outside it, because a publication
/// that dials a peer can take seconds and the store must not be held for
/// that long.
pub fn begin_attempts(
    coordinator: &mut PublicationCoordinator,
    local_hash: RouterHash,
    plan: &RouterInfoPublicationPlan,
    store: &RouterInfoStore,
) -> Vec<i2pr_netdb::PublicationAttemptRecord> {
    let RouterInfoPublicationPlan::Publish { targets, record } = plan else {
        return Vec::new();
    };
    // The plan's record is the record that is published, so the store
    // message's key, its reply gateway and its payload are one record by
    // construction.
    coordinator.register_record(PublicationLocalRecord::new(local_hash, record.clone()));
    targets
        .iter()
        .filter_map(|target| coordinator.begin_attempt(*target, store).ok())
        .collect()
}

/// Puts a publication on the wire.
///
/// This is the **one** delivery path: the service product's one-shot
/// publication and the router-level refresh both come through here, so
/// they cannot disagree about the envelope, the freshness, or the
/// counting.
///
/// `records` are the peers' own validated RouterInfos, paired with the
/// target they belong to — the coordinator's records for the one-shot
/// publication, the router-wide bootstrap's records for the refresh. A
/// peer that already has a session needs no record at all.
#[allow(clippy::too_many_arguments)]
pub async fn publish_router_info<R: RngCore>(
    coordinator: &mut PublicationCoordinator,
    attempts: &[i2pr_netdb::PublicationAttemptRecord],
    records: &[(RouterHash, ValidatedRouterInfo)],
    ssu2_handle: &crate::router_i2np::Ssu2DaemonHandle,
    dial_timeout: Duration,
    qualification_profile: crate::config::Ssu2QualificationProfile,
    rng: &mut R,
    cancellation: &i2pr_runtime::CancellationToken,
) -> RouterInfoPublicationOutcome {
    let mut accepted = 0usize;
    for attempt in attempts {
        if cancellation.is_cancelled() {
            return RouterInfoPublicationOutcome::Cancelled;
        }
        // Plan 516: the store message was composed by the correlator, so
        // the wire message carries this target's own minted reply token and
        // the attempt was already in the bounded book before anything was
        // sent.
        let target = &attempt.attempt.peer();
        let request_id = attempt.attempt.request_id();
        let peer_id = i2pr_transport::PeerId::from_hash(*target.as_hash());
        if ssu2_handle.service().session_peer_links(peer_id).is_empty() {
            let Some((_, record)) = records.iter().find(|(hash, _)| hash == target) else {
                let _ = coordinator.mark_delivery_failed(request_id);
                continue;
            };
            if crate::service_product::ensure_peer_session_for_record(
                record,
                ssu2_handle,
                *target.as_hash(),
                crate::service_product::DestinationDialPurpose::CreatorRouterInfoPublication,
                dial_timeout,
                qualification_profile,
                cancellation,
            )
            .await
            .is_err()
            {
                let _ = coordinator.mark_delivery_failed(request_id);
                continue;
            }
        }
        let Some(message_id) = fresh_message_id(rng) else {
            let _ = coordinator.mark_delivery_failed(request_id);
            continue;
        };
        let Ok(wire) = crate::outbound_lookup::encode_store_envelope_short(
            &attempt.store_message,
            message_id,
            // The same 60-second lifetime policy the one-shot publication
            // carries, in the seconds the direct short envelope holds.
            publication_expiry_seconds(),
        ) else {
            continue;
        };
        let Ok(request) =
            crate::router_i2np::RouterDeliveryRequest::new(peer_id, wire, dial_timeout)
        else {
            let _ = coordinator.mark_delivery_failed(request_id);
            continue;
        };
        if matches!(
            ssu2_handle.delivery().deliver(request, cancellation),
            crate::router_i2np::RouterDeliveryOutcome::Accepted
        ) {
            // A **delivery** admission, not an acknowledgement. The attempt
            // is delivered; only the network's `DeliveryStatus` makes it
            // acknowledged.
            let _ = coordinator.mark_delivered(request_id);
            accepted = accepted.saturating_add(1);
        } else {
            let _ = coordinator.mark_delivery_failed(request_id);
        }
    }
    if accepted > 0 {
        RouterInfoPublicationOutcome::Published {
            targets: attempts.len(),
            accepted,
        }
    } else {
        RouterInfoPublicationOutcome::DeliveryRefused {
            targets: attempts.len(),
        }
    }
}

/// The publication envelope's expiry, in seconds.
///
/// The envelope is a short-lived announcement; the *record* is what the
/// network keeps, and the record's freshness is the RouterInfo's own
/// publication date.
fn publication_expiry_seconds() -> u32 {
    let now = crate::service_product::wall_secs();
    now.saturating_add(60).min(u64::from(u32::MAX)) as u32
}

/// Allocates a fresh message id.
///
/// The id is never zero, and the search is bounded: a router that cannot
/// find one in 32 tries refuses the publication rather than looping.
fn fresh_message_id<R: RngCore>(rng: &mut R) -> Option<u32> {
    for _ in 0..32 {
        let candidate = rng.next_u32();
        if candidate != 0 {
            return Some(candidate);
        }
    }
    None
}
