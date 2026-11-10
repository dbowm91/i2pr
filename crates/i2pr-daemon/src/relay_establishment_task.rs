//! Plan 533 — the relay establishment's **own** task.
//!
//! ## Why the establishment has a task
//!
//! A session establishment is an **await**. Plan 519 gave the router's own
//! publication its own task for exactly this reason, and Plan 522 gave
//! the outbound peer-session pool one: the daemon's `select!` loop must
//! never block on a handshake, and a blocked loop would stall the inbound
//! drain, the relay drain and every other service arm it carries.
//!
//! The relay tick is a 100 ms cadence, and a handshake is measured in
//! seconds. The establishment therefore runs on a task of its own, under
//! the service's child scope, so it is owned, cancelled and joined with
//! the service: there is no ownerless task and no detached one.
//!
//! ## The lock order
//!
//! The bootstrap's lock is taken to read the target's validated material
//! and is **released** before the dial. The dial is awaited with **no**
//! lock held, so a slow handshake can never hold the router's NetDB.

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use crate::relay_establishment::{
    RelayEstablishmentCounters, RelayEstablishmentOwner, prepare_relay_establishment,
};
use crate::relay_requester::RelayEstablishment;

/// The establishment's task.
///
/// The task is spawned under the service's own child scope, so it is
/// owned, cancelled and joined with the service: there is no ownerless
/// task and no detached one.
pub struct RelayEstablishmentTask {
    /// The establishment owner.
    pub owner: RelayEstablishmentOwner,
    /// The narrow transport capability the establishment dials through.
    ///
    /// The runtime's own service is the transport's capability: it owns
    /// the sockets, the handshake, the responder's token table and the
    /// transport's verification, and it grants nothing the daemon did
    /// not already hold.
    pub service: i2pr_runtime::Ssu2RuntimeService,
    /// The live bootstrap store the target's validated material is read
    /// from.
    pub bootstrap: Option<Arc<StdMutex<crate::bootstrap::Bootstrap>>>,
    /// The qualification profile the dial target is built under.
    pub qualification_profile: crate::config::Ssu2QualificationProfile,
    /// The bounded dial timeout.
    pub dial_timeout: Duration,
    /// The bounded handoff the requester owner publishes into.
    pub receiver: tokio::sync::mpsc::Receiver<RelayEstablishment>,
    /// The shared counter cell the owner publishes into.
    ///
    /// The cell holds counts only. It names no peer, no address, no key
    /// and no token.
    pub counters_cell: Arc<StdMutex<RelayEstablishmentCounters>>,
}

/// Publishes the owner's counters into the shared cell.
///
/// The cell holds counts only. It names no peer, no address, no key and
/// no token.
fn publish_counters(
    owner: &RelayEstablishmentOwner,
    cell: &Arc<StdMutex<RelayEstablishmentCounters>>,
) {
    if let Ok(mut guard) = cell.lock() {
        *guard = owner.counters();
    }
}

/// Runs the establishment's task until the cancellation arrives.
///
/// The task's work is driven by the handoff: the requester owner
/// publishes one establishment per completed exchange, and the task takes
/// them. A refusal never stops the router.
pub async fn run_relay_establishment_task(
    task: RelayEstablishmentTask,
    cancellation: i2pr_runtime::CancellationToken,
) -> Result<(), i2pr_runtime::ChildTaskFailure> {
    let RelayEstablishmentTask {
        mut owner,
        service,
        bootstrap,
        qualification_profile,
        dial_timeout,
        mut receiver,
        counters_cell,
    } = task;
    loop {
        tokio::select! {
            _ = cancellation.cancelled() => break,
            item = receiver.recv() => {
                let Some(establishment) = item else {
                    break;
                };
                let Some(bootstrap) = bootstrap.as_ref() else {
                    break;
                };
                // The target's validated material is read under the
                // bootstrap's lock and the lock is **released** before the
                // dial: the dial is awaited with no lock held, so a slow
                // handshake can never hold the router's NetDB.
                let prepared = {
                    let Ok(guard) = bootstrap.lock() else {
                        break;
                    };
                    prepare_relay_establishment(
                        guard.store(),
                        &establishment,
                        qualification_profile,
                    )
                };
                let target = match prepared {
                    Ok(target) => target,
                    Err(reason) => {
                        owner.refuse(reason);
                        continue;
                    }
                };
                let _ = crate::relay_establishment::drive_relay_establishment(
                    &mut owner,
                    target,
                    establishment.token,
                    &service,
                    dial_timeout,
                    &cancellation,
                )
                .await;
                publish_counters(&owner, &counters_cell);
            }
        }
    }
    publish_counters(&owner, &counters_cell);
    Ok(())
}
