//! Plan 519 — the publication's **bounded** acknowledgement handoff.
//!
//! The router's own RouterInfo publication runs on its **own task**
//! (`run_router_info_publication_task` in the daemon's composition root),
//! and the SSU2 service's `select!` loop never awaits it. The loop still
//! sees the floodfill's inbound `DeliveryStatus`, so the acknowledgement
//! has to cross the boundary — and it crosses it over a **bounded**
//! channel that the loop never blocks on.
//!
//! The reference never blocks its transport's loop on a handshake: i2pd
//! 2.61.0 posts the send and ignores the returned future
//! (`libi2pd/Transports.cpp:483-496`), and Java I2P 2.13.0 runs the
//! publication as a job on its own pool (`PublishLocalRouterInfoJob`,
//! `JobQueue.java:163`) whose store is handed to a `FloodfillStoreJob`
//! with its own `PUBLISH_TIMEOUT`
//! (`FloodfillNetworkDatabaseFacade.java:174`).
//!
//! A full channel is **not** a dropped acknowledgement: it is counted as
//! **unmatched**, which is the refusal Plan 516 already defines for an
//! acknowledgement that names no live attempt. Nothing is ever dropped
//! silently, and the loop is never blocked.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::router_info_publication::RouterInfoPublicationOwner;

/// The capacity of the publication's acknowledgement handoff.
///
/// The channel is bounded: the transport's service loop hands the
/// acknowledgement over and moves on, so a publication that is not
/// draining its inbox can never make the loop wait.
pub const PUBLICATION_ACKNOWLEDGEMENT_CAPACITY: usize = 16;

/// The publication's ladder tick, in milliseconds.
///
/// Plan 519: the ladder's tick belongs to the **publication's** task. The
/// transport's service loop gains no timer: it carries none of the
/// publication's ticks, and the publication's ticks carry none of the
/// transport's work.
///
/// The period is the loop's own 100 millisecond cadence, which is how
/// quickly the reference's `ROUTER_INFO_CONFIRMATION_TIMEOUT` deadline
/// is noticed.
pub const PUBLICATION_LADDER_TICK_MS: u64 = 100;

/// The transport's service loop's end of the acknowledgement handoff.
///
/// The loop holds one of these and calls [`PublicationAcknowledgementHandoff::acknowledge`]
/// for every inbound `DeliveryStatus`. The call never blocks and never
/// awaits.
#[derive(Clone, Debug)]
pub struct PublicationAcknowledgementHandoff {
    sender: tokio::sync::mpsc::Sender<u32>,
    overflow: Arc<AtomicU64>,
}

impl PublicationAcknowledgementHandoff {
    /// Hands one inbound acknowledgement to the publication's task.
    ///
    /// Returns `true` when the acknowledgement was handed over, and
    /// `false` when it could not be: the channel is bounded and the loop
    /// is **never** blocked. A refused acknowledgement is counted as
    /// **unmatched**, and it is never dropped silently.
    pub fn acknowledge(&self, message_id: u32) -> bool {
        match self.sender.try_send(message_id) {
            Ok(()) => true,
            Err(_) => {
                self.overflow.fetch_add(1, Ordering::AcqRel);
                false
            }
        }
    }
}

/// The publication's task's end of the acknowledgement handoff.
#[derive(Debug)]
pub struct PublicationAcknowledgementInbox {
    receiver: tokio::sync::mpsc::Receiver<u32>,
    overflow: Arc<AtomicU64>,
}

impl PublicationAcknowledgementInbox {
    /// Receives the next inbound acknowledgement's message id.
    ///
    /// The receive is cancel-safe: the publication's task may drop it and
    /// come back to it without losing an acknowledgement.
    pub async fn recv(&mut self) -> Option<u32> {
        self.receiver.recv().await
    }

    /// Folds the acknowledgements the bounded channel could not take into
    /// the publication's counters, under the **unmatched** reason.
    ///
    /// The count is the number of acknowledgements the transport's loop
    /// could not hand over. They are refused exactly as an
    /// acknowledgement that names no live attempt is refused, so the two
    /// are never merged into one another.
    pub fn fold_overflow(&self, owner: &mut RouterInfoPublicationOwner) -> u64 {
        let count = self.overflow.swap(0, Ordering::AcqRel);
        owner.schedule_mut().record_unmatched_count(count);
        count
    }
}

/// Builds the publication's bounded acknowledgement handoff.
///
/// The channel's capacity is bounded and never zero: a zero-capacity
/// channel would refuse every acknowledgement, which is not a handoff.
pub fn publication_acknowledgement_channel(
    capacity: usize,
) -> (
    PublicationAcknowledgementHandoff,
    PublicationAcknowledgementInbox,
) {
    let (sender, receiver) = tokio::sync::mpsc::channel(capacity.max(1));
    let overflow = Arc::new(AtomicU64::new(0));
    (
        PublicationAcknowledgementHandoff {
            sender,
            overflow: Arc::clone(&overflow),
        },
        PublicationAcknowledgementInbox { receiver, overflow },
    )
}

/// The publication's task inputs.
///
/// Plan 519: the task owns its whole world — the owner, the transport
/// capability, the identity, the store handle and the inbox — so the
/// transport's service loop holds **none** of the publication's state and
/// there is exactly one owner of the cycle.
pub struct RouterInfoPublicationTask {
    /// The publication's owner: the schedule, the cycle, the correlator
    /// and the cycle's record.
    pub publication_owner: crate::router_info_publication::RouterInfoPublicationOwner,
    /// The narrow transport capability the publication's delivery uses.
    pub handle: crate::router_i2np::Ssu2PublicationHandle,
    /// The router's own identity, for the cycle's re-dating.
    pub bundle: Arc<i2pr_crypto::RouterIdentityBundle>,
    /// The router-wide store the selection reads.
    pub bootstrap: Option<Arc<Mutex<crate::bootstrap::Bootstrap>>>,
    /// The publication's message-id randomness.
    pub rng: rand_chacha::ChaCha8Rng,
    /// The bounded inbox of inbound acknowledgements.
    pub inbox: PublicationAcknowledgementInbox,
    /// The dial timeout the publication's own delivery uses.
    pub dial_timeout: std::time::Duration,
    /// The SSU2 qualification profile the dial is built under.
    pub qualification_profile: crate::config::Ssu2QualificationProfile,
}

impl std::fmt::Debug for RouterInfoPublicationTask {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Plan 519: the identity bundle carries private key material and
        // has no debug representation of its own, so this prints no field
        // at all.
        formatter
            .debug_struct("RouterInfoPublicationTask")
            .finish_non_exhaustive()
    }
}
