//! Bounded subscription-refresh composition (runtime-neutral half).
//!
//! The queue owns the capacity discipline: at most one active refresh
//! plus one newest pending subscription set. A newer `SetSubscriptions`
//! commit coalesces over a waiting pending set; it never queues
//! unbounded work and never disturbs the active fetch. The daemon owns
//! timers, the (currently absent) downloader, ingestion commit, and the
//! diagnostic artifact; this module owns the state machine plus the
//! outcome vocabulary the artifact records.

use crate::subscription::SubscriptionSet;

/// Why a refresh was requested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefreshReason {
    /// A `SetSubscriptions` commit.
    SubscriptionsReplaced,
    /// The refresh-interval cadence elapsed.
    IntervalElapsed,
    /// Operator/diagnostic retry.
    Manual,
}

/// Outcome of one refresh attempt (recorded to the artifact, bounded).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefreshOutcome {
    /// A body was ingested and derived entries were committed.
    Committed {
        /// Ingested entry count.
        ingested: usize,
        /// Whether derived entries changed.
        changed: bool,
    },
    /// No downloader owner exists for the fetch.
    DownloaderUnavailable,
    /// A fetched body failed ingestion (whole body rejected).
    IngestRejected,
    /// The fetch itself failed (bounded detail, no URLs echoed).
    FetchFailed,
}

/// Bounded refresh queue: one active set plus one coalesced pending set.
#[derive(Debug, Default)]
pub struct RefreshQueue {
    active: Option<SubscriptionSet>,
    pending: Option<SubscriptionSet>,
    last_reason: Option<RefreshReason>,
}

impl RefreshQueue {
    /// Empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests a refresh of `set`. Returns `true` when the caller
    /// should start fetching immediately (queue was idle); `false`
    /// when the set coalesced into pending behind an active fetch.
    pub fn push(&mut self, set: SubscriptionSet, reason: RefreshReason) -> bool {
        if self.active.is_none() {
            self.active = Some(set);
            self.last_reason = Some(reason);
            true
        } else {
            self.pending = Some(set);
            self.last_reason = Some(reason);
            false
        }
    }

    /// The set the active fetch is working on, if any.
    pub fn active(&self) -> Option<&SubscriptionSet> {
        self.active.as_ref()
    }

    /// Completes the active fetch. A coalesced pending set promotes to
    /// active and is returned for immediate fetching; otherwise the
    /// queue is idle and `None` is returned.
    pub fn finish_active(&mut self) -> Option<SubscriptionSet> {
        self.active = None;
        if let Some(next) = self.pending.take() {
            self.active = Some(next.clone());
            Some(next)
        } else {
            None
        }
    }

    /// Whether no fetch is active and none is pending.
    pub fn is_idle(&self) -> bool {
        self.active.is_none() && self.pending.is_none()
    }

    /// Reason of the most recent push, if any.
    pub fn last_reason(&self) -> Option<RefreshReason> {
        self.last_reason
    }
}

/// One artifact diagnostic line's structured content. The daemon
/// timestamps and bounds the rendered lines; this vocabulary keeps
/// URLs and bodies out of the artifact by construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefreshDiagnostic {
    /// What triggered the refresh.
    pub reason: RefreshReason,
    /// What happened.
    pub outcome: RefreshOutcome,
    /// Subscription URL count (never the URLs).
    pub url_count: usize,
}

impl RefreshDiagnostic {
    /// Renders the bounded artifact line (no timestamp; the daemon
    /// prefixes one).
    pub fn line(&self) -> String {
        let reason = match self.reason {
            RefreshReason::SubscriptionsReplaced => "subscriptions-replaced",
            RefreshReason::IntervalElapsed => "interval-elapsed",
            RefreshReason::Manual => "manual",
        };
        let outcome = match &self.outcome {
            RefreshOutcome::Committed { ingested, changed } => {
                format!("committed ingested={ingested} changed={changed}")
            }
            RefreshOutcome::DownloaderUnavailable => "downloader-unavailable".to_owned(),
            RefreshOutcome::IngestRejected => "ingest-rejected".to_owned(),
            RefreshOutcome::FetchFailed => "fetch-failed".to_owned(),
        };
        format!("refresh reason={reason} urls={} {outcome}", self.url_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(urls: &[&str]) -> SubscriptionSet {
        SubscriptionSet::checked(
            &urls.iter().map(|url| (*url).to_owned()).collect::<Vec<String>>(),
        )
        .expect("valid set")
    }

    #[test]
    fn queue_coalesces_to_newest_pending() {
        let mut queue = RefreshQueue::new();
        assert!(queue.is_idle());
        assert!(queue.push(set(&["http://a.i2p/h"]), RefreshReason::Manual));
        // Active fetch running: two more commits coalesce to the newest.
        assert!(!queue.push(set(&["http://b.i2p/h"]), RefreshReason::SubscriptionsReplaced));
        assert!(!queue.push(
            set(&["http://c.i2p/h", "http://d.i2p/h"]),
            RefreshReason::SubscriptionsReplaced
        ));
        assert_eq!(queue.active().expect("active").urls().len(), 1);
        // Finishing promotes exactly the newest pending set.
        let next = queue.finish_active().expect("promoted");
        assert_eq!(
            next.urls(),
            &["http://c.i2p/h".to_owned(), "http://d.i2p/h".to_owned()]
        );
        assert_eq!(queue.last_reason(), Some(RefreshReason::SubscriptionsReplaced));
        // No further pending: finishing idles the queue.
        assert!(queue.finish_active().is_none());
        assert!(queue.is_idle());
    }

    #[test]
    fn diagnostic_lines_carry_no_urls() {
        let diagnostic = RefreshDiagnostic {
            reason: RefreshReason::IntervalElapsed,
            outcome: RefreshOutcome::Committed {
                ingested: 12,
                changed: true,
            },
            url_count: 3,
        };
        let line = diagnostic.line();
        assert!(line.contains("interval-elapsed"));
        assert!(line.contains("ingested=12"));
        assert!(!line.contains("http"));
        let unavailable = RefreshDiagnostic {
            reason: RefreshReason::SubscriptionsReplaced,
            outcome: RefreshOutcome::DownloaderUnavailable,
            url_count: 1,
        };
        assert!(unavailable.line().contains("downloader-unavailable"));
    }
}
