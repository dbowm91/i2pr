//! Wakeable, hierarchical cancellation for runtime-owned work.

use std::sync::{Arc, Mutex};

use i2pr_core::CancellationReason;
use tokio_util::sync::CancellationToken as TokioCancellationToken;

#[derive(Debug)]
struct CancellationInner {
    token: TokioCancellationToken,
    reason: Mutex<Option<CancellationReason>>,
    parent: Option<Arc<CancellationInner>>,
}

/// A wakeable cancellation scope with first-reason-wins semantics.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    inner: Arc<CancellationInner>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancellationToken {
    /// Creates a root cancellation scope.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(CancellationInner {
                token: TokioCancellationToken::new(),
                reason: Mutex::new(None),
                parent: None,
            }),
        }
    }

    /// Creates an independent child that inherits cancellation from this scope.
    pub fn child_token(&self) -> Self {
        Self {
            inner: Arc::new(CancellationInner {
                token: self.inner.token.child_token(),
                reason: Mutex::new(None),
                parent: Some(Arc::clone(&self.inner)),
            }),
        }
    }

    /// Cancels this scope and records the first bounded reason.
    ///
    /// Returns `true` only for the caller that recorded the reason. A child
    /// whose parent is already cancelled cannot replace the inherited reason.
    pub fn cancel(&self, reason: CancellationReason) -> bool {
        if self.is_cancelled() {
            return false;
        }
        let Ok(mut recorded) = self.inner.reason.lock() else {
            return false;
        };
        if recorded.is_some() || self.parent_reason().is_some() {
            return false;
        }
        *recorded = Some(reason);
        drop(recorded);
        self.inner.token.cancel();
        true
    }

    /// Returns whether this scope has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.inner.token.is_cancelled()
    }

    /// Returns the first local or inherited reason, if cancellation occurred.
    pub fn reason(&self) -> Option<CancellationReason> {
        self.inner
            .reason
            .lock()
            .ok()
            .and_then(|reason| *reason)
            .or_else(|| self.parent_reason())
    }

    fn parent_reason(&self) -> Option<CancellationReason> {
        self.inner.parent.as_ref().and_then(|parent| {
            parent
                .reason
                .lock()
                .ok()
                .and_then(|reason| *reason)
                .or_else(|| {
                    let parent = Self {
                        inner: Arc::clone(parent),
                    };
                    parent.parent_reason()
                })
        })
    }

    /// Waits until cancellation. Tokio's cancellation primitive registers the
    /// waiter atomically, so cancellation before or during registration cannot
    /// lose a wakeup.
    pub async fn cancelled(&self) {
        self.inner.token.cancelled().await;
    }

    /// Waits until cancellation and returns its bounded reason.
    pub async fn cancelled_reason(&self) -> CancellationReason {
        self.cancelled().await;
        self.reason().unwrap_or(CancellationReason::ParentScope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_before_wait_is_immediate() {
        let token = CancellationToken::new();
        assert!(token.cancel(CancellationReason::OperatorRequest));
        token.cancelled().await;
        assert_eq!(token.reason(), Some(CancellationReason::OperatorRequest));
    }

    #[tokio::test]
    async fn child_inherits_without_cancelling_parent() {
        let parent = CancellationToken::new();
        let child = parent.child_token();
        assert!(child.cancel(CancellationReason::TestHarnessTeardown));
        assert!(!parent.is_cancelled());
        assert_eq!(
            child.reason(),
            Some(CancellationReason::TestHarnessTeardown)
        );

        parent.cancel(CancellationReason::OperatorRequest);
        assert!(parent.is_cancelled());
    }

    #[tokio::test]
    async fn parent_reason_is_visible_to_child() {
        let parent = CancellationToken::new();
        let child = parent.child_token();
        parent.cancel(CancellationReason::EssentialServiceFailure);
        child.cancelled().await;
        assert_eq!(
            child.reason(),
            Some(CancellationReason::EssentialServiceFailure)
        );
    }

    #[tokio::test]
    async fn all_waiters_wake() {
        let token = CancellationToken::new();
        let mut waiters = Vec::new();
        for _ in 0..16 {
            let waiter = token.clone();
            waiters.push(tokio::spawn(async move {
                waiter.cancelled().await;
            }));
        }
        token.cancel(CancellationReason::OperatorRequest);
        for waiter in waiters {
            waiter.await.expect("waiter joined");
        }
    }
}
