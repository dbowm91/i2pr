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
//! Nothing here advertises anything: `router.version` stays `0.9.62`, the
//! published RouterInfo's content is unchanged, and the inventory's
//! advertised count stays zero.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use i2pr_netdb::{
    LookupPolicy, RouterHash, RouterInfoStore, ValidatedRouterInfo, VersionGatedFloodfillSelection,
    select_version_gated_floodfills,
};
use i2pr_proto::Hash;
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
        /// The `DatabaseStore` that carries the router's own RouterInfo.
        store: i2pr_proto::DatabaseStoreMessage,
    },
    /// The publication is refused, with the reason.
    Refused(RouterInfoPublicationOutcome),
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
}

impl RouterInfoPublicationOwner {
    /// Constructs an owner for the router's own hash.
    pub fn new(local_hash: RouterHash, period_ms: u64) -> Self {
        Self {
            schedule: RouterInfoPublicationSchedule::new(period_ms),
            local_hash,
            policy: LookupPolicy::default(),
        }
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
        let Ok(store_message) =
            router_info_store_message(*self.local_hash.as_hash(), record.to_vec())
        else {
            return RouterInfoPublicationPlan::Refused(
                RouterInfoPublicationOutcome::RefusedNoLocalRecord,
            );
        };
        RouterInfoPublicationPlan::Publish {
            targets,
            store: store_message,
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
    owner: &RouterInfoPublicationOwner,
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
    let (targets, store_message, records) = {
        let Ok(guard) = bootstrap.lock() else {
            return RouterInfoPublicationOutcome::RefusedNoTarget;
        };
        let plan = owner.plan(guard.store(), Some(local_record.as_slice()));
        let RouterInfoPublicationPlan::Publish {
            targets,
            store: store_message,
        } = plan
        else {
            return RouterInfoPublicationOutcome::RefusedNoTarget;
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
        (targets, store_message, records)
    };
    publish_router_info(
        &targets,
        &store_message,
        &records,
        ssu2_handle,
        dial_timeout,
        qualification_profile,
        rng,
        cancellation,
    )
    .await
}

/// Builds the `DatabaseStore` that carries the router's own RouterInfo.
///
/// The payload is the compressed RouterInfo, bounded by the protocol's
/// maximum I2NP payload size. The store names our own hash and carries no
/// reply token: the publication is a one-way announcement.
pub fn router_info_store_message(
    local_hash: Hash,
    router_info: Vec<u8>,
) -> Result<i2pr_proto::DatabaseStoreMessage, RouterInfoPublicationError> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut encoder, &router_info)
        .map_err(|_| RouterInfoPublicationError::Compression)?;
    let compressed = encoder
        .finish()
        .map_err(|_| RouterInfoPublicationError::Compression)?;
    let payload = i2pr_proto::DeferredPayload::new(compressed, MAX_I2NP_PAYLOAD_SIZE)
        .map_err(|_| RouterInfoPublicationError::PayloadTooLarge)?;
    Ok(i2pr_proto::DatabaseStoreMessage {
        key: local_hash,
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: i2pr_proto::DatabaseStoreData::RouterInfoCompressed(payload),
    })
}

/// The bounded I2NP payload size, matching the protocol's own ceiling.
const MAX_I2NP_PAYLOAD_SIZE: usize = 32 * 1024;

/// The typed failure of building the publication's store.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterInfoPublicationError {
    /// The RouterInfo could not be compressed.
    Compression,
    /// The compressed RouterInfo exceeds the bounded store payload.
    PayloadTooLarge,
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
    targets: &[RouterHash],
    store_message: &i2pr_proto::DatabaseStoreMessage,
    records: &[(RouterHash, ValidatedRouterInfo)],
    ssu2_handle: &crate::router_i2np::Ssu2DaemonHandle,
    dial_timeout: Duration,
    qualification_profile: crate::config::Ssu2QualificationProfile,
    rng: &mut R,
    cancellation: &i2pr_runtime::CancellationToken,
) -> RouterInfoPublicationOutcome {
    let mut accepted = 0usize;
    for target in targets {
        if cancellation.is_cancelled() {
            return RouterInfoPublicationOutcome::Cancelled;
        }
        let peer_id = i2pr_transport::PeerId::from_hash(*target.as_hash());
        if ssu2_handle.service().session_peer_links(peer_id).is_empty() {
            let Some((_, record)) = records.iter().find(|(hash, _)| hash == target) else {
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
                continue;
            }
        }
        let Some(message_id) = fresh_message_id(rng) else {
            continue;
        };
        let Ok(wire) = crate::outbound_lookup::encode_store_envelope_short(
            store_message,
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
            continue;
        };
        if matches!(
            ssu2_handle.delivery().deliver(request, cancellation),
            crate::router_i2np::RouterDeliveryOutcome::Accepted
        ) {
            accepted = accepted.saturating_add(1);
        }
    }
    if accepted > 0 {
        RouterInfoPublicationOutcome::Published {
            targets: targets.len(),
            accepted,
        }
    } else {
        RouterInfoPublicationOutcome::DeliveryRefused {
            targets: targets.len(),
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
