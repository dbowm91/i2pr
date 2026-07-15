//! Explicit link lifecycle transitions.

use std::fmt;

/// Finite lifecycle for one transport link instance.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkState {
    /// A locally allocated candidate has not started its handshake.
    Candidate,
    /// Handshake work is in progress under a pending-handshake lease.
    Handshaking,
    /// Authentication completed for this link instance.
    Authenticated,
    /// The link will not accept new work but is draining queued work.
    Draining,
    /// Link shutdown has started.
    Closing,
    /// Link shutdown completed.
    Closed,
    /// Link failed and cannot be authenticated again.
    Failed,
}

impl LinkState {
    /// Applies one allowed transition without performing any side effect.
    pub fn transition(self, next: Self) -> Result<Self, InvalidLinkTransition> {
        let valid = self == next
            || matches!(
                (self, next),
                (
                    Self::Candidate,
                    Self::Handshaking | Self::Closing | Self::Failed,
                ) | (
                    Self::Handshaking,
                    Self::Authenticated | Self::Closing | Self::Failed
                ) | (
                    Self::Authenticated,
                    Self::Draining | Self::Closing | Self::Failed
                ) | (Self::Draining, Self::Closing | Self::Closed | Self::Failed)
                    | (Self::Closing, Self::Closed)
                    | (Self::Failed, Self::Closed)
            );
        if valid {
            Ok(next)
        } else {
            Err(InvalidLinkTransition {
                from: self,
                to: next,
            })
        }
    }

    /// Returns whether authentication has completed for this instance.
    pub const fn is_authenticated(self) -> bool {
        matches!(self, Self::Authenticated | Self::Draining | Self::Closing)
    }

    /// Returns whether the state can own a live link entry.
    pub const fn is_live(self) -> bool {
        !matches!(self, Self::Closed | Self::Failed)
    }
}

/// An attempted link transition rejected by the finite lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidLinkTransition {
    /// State before the attempted transition.
    pub from: LinkState,
    /// Requested destination state.
    pub to: LinkState,
}

impl fmt::Display for InvalidLinkTransition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid link transition: {:?} -> {:?}",
            self.from, self.to
        )
    }
}

impl std::error::Error for InvalidLinkTransition {}
