//! Plan 522 — the outbound peer-session pool's **own** task.
//!
//! ## Why the pool has a task
//!
//! A session establishment is an **await**. Plan 519 gave the router's own
//! publication its own task for exactly this reason, and the same rule
//! applies here: the daemon's `select!` loop must never block on a
//! handshake, and a blocked loop would stall the inbound drain, the relay
//! drain and every other service arm it carries.
//!
//! Neither pinned reference blocks its transport's loop on a handshake:
//! i2pd 2.61.0 posts the send and ignores the returned future
//! (`libi2pd/Transports.cpp:483-496`) and Java I2P 2.13.0 runs the
//! publication as a **job on its own pool** (`PublishLocalRouterInfoJob`,
//! `JobQueue.java:163`).
//!
//! The task owns the pool owner, the narrow transport capability, the
//! bootstrap's store, the local router hash, the dial timeout and the
//! counter cell. The daemon's `select!` loop holds **none** of it, and the
//! pool's task never blocks it.
//!
//! ## The lock order
//!
//! The bootstrap's lock is taken to read the population and is **released**
//! before any dial. The dial is awaited with **no** lock held, so a slow
//! handshake can never hold the router's NetDB.

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use i2pr_netdb::RouterHash;

use crate::peer_pool::{PeerPoolDialOutcome, PeerPoolOwner};

/// The pool's task.
///
/// The task is spawned under the service's own child scope, so it is
/// owned, cancelled and joined with the service: there is no ownerless
/// task and no detached one.
pub struct PeerPoolTask {
    /// The pool owner.
    pub owner: PeerPoolOwner,
    /// The narrow transport capability the pool dials through.
    pub handle: crate::router_i2np::Ssu2PublicationHandle,
    /// The live bootstrap store the pool reads its population from.
    pub bootstrap: Option<Arc<StdMutex<crate::bootstrap::Bootstrap>>>,
    /// The router's own hash, excluded from the population.
    pub local_hash: Option<RouterHash>,
    /// The bounded dial timeout.
    pub dial_timeout: Duration,
    /// The qualification profile the dial target is built under.
    pub qualification_profile: crate::config::Ssu2QualificationProfile,
    /// The live counter cell the composition publishes into the
    /// inspection plane.
    pub counters_cell: Arc<crate::i2pcontrol_inspection::PeerPoolCounterCell>,
}

/// Publishes the owner's counters into the shared cell.
///
/// The cell holds counts only. It names no peer, no address and no key.
fn publish_counters(
    owner: &PeerPoolOwner,
    cell: &Arc<crate::i2pcontrol_inspection::PeerPoolCounterCell>,
) {
    if let Ok(mut guard) = cell.lock() {
        *guard = *owner.counters();
    }
}

/// Runs the pool's task until the cancellation arrives.
///
/// The task's period is the pool's own cadence. Every outcome is counted
/// by reason, and a refusal never stops the router.
pub async fn run_peer_pool_task(
    task: PeerPoolTask,
    cancellation: i2pr_runtime::CancellationToken,
) -> Result<(), i2pr_runtime::ChildTaskFailure> {
    let PeerPoolTask {
        mut owner,
        handle,
        bootstrap,
        local_hash,
        dial_timeout,
        qualification_profile,
        counters_cell,
    } = task;
    let mut tick = tokio::time::interval(Duration::from_millis(crate::peer_pool::POOL_PERIOD_MS));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = cancellation.cancelled() => break,
            _ = tick.tick() => {
                let Some(bootstrap) = bootstrap.as_ref() else {
                    break;
                };
                let Some(local_hash) = local_hash else {
                    break;
                };
                // The population is read under the bootstrap's lock and the
                // lock is **released** before any dial: the dial is awaited
                // with no lock held, so a slow handshake can never hold the
                // router's NetDB.
                let planned = {
                    let Ok(guard) = bootstrap.lock() else {
                        break;
                    };
                    let established = handle.service().active_peer_hashes();
                    owner.plan(guard.store(), &local_hash, &established)
                };
                for peer in planned {
                    if cancellation.is_cancelled() {
                        break;
                    }
                    let record = {
                        let Ok(guard) = bootstrap.lock() else {
                            break;
                        };
                        guard.store().get(&peer).cloned()
                    };
                    let Some(record) = record else {
                        // The peer the selection named is no longer in the
                        // store. It is refused under its own reason and
                        // released, so it is retried on the next period.
                        owner.record_peer_absent(&peer);
                        continue;
                    };
                    let outcome = match crate::service_product::ensure_peer_session_for_record(
                        &record,
                        &handle,
                        *peer.as_hash(),
                        crate::service_product::DestinationDialPurpose::PeerPoolMaintenance,
                        dial_timeout,
                        qualification_profile,
                        &cancellation,
                    )
                    .await
                    {
                        Ok(()) => PeerPoolDialOutcome::Established,
                        Err(error) => {
                            if cancellation.is_cancelled() {
                                PeerPoolDialOutcome::Cancelled
                            } else if matches!(
                                error,
                                crate::service_product::ServiceProductError::ControlledProfile(_)
                            ) {
                                // The qualification profile refused the
                                // target. The transport's own scope is the
                                // authority on what may be dialled, and the
                                // pool never widens it.
                                PeerPoolDialOutcome::Refused
                            } else {
                                PeerPoolDialOutcome::Failed
                            }
                        }
                    };
                    owner.record_dial_outcome(&peer, outcome);
                }
                publish_counters(&owner, &counters_cell);
            }
        }
    }
    publish_counters(&owner, &counters_cell);
    Ok(())
}
