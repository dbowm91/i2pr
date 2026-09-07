//! Typed I2CP actions emitted by the connection/session state machine.
//!
//! Plan 165 §8 defines the narrow action vocabulary the
//! connection/session state machine emits. The state machine owns no
//! sockets, timers, or destination secret material; it produces
//! actions the Plan 167 daemon projects into `i2pr-client`.
//!
//! Every action payload carries verified/typed values, not raw client
//! strings. A `VerifiedSessionConfig` is the only typed value that may
//! reach a `ReserveClientDestination` action; a `ProjectedPolicy` is
//! the only typed value that may drive a destination activation.

use i2pr_proto::Hash;

use super::config::ProjectedPolicy;
use super::ids::SessionId;
use super::verify::VerifiedSessionConfig;

/// One typed action emitted by the Plan 165 connection/session state
/// machine.
#[derive(Debug)]
#[non_exhaustive]
pub enum I2cpAction {
    /// Reserve a client destination for a freshly verified session.
    ReserveClientDestination {
        /// Verified session configuration (signature, date, options).
        verified_session: VerifiedSessionConfig,
        /// Destination policy projected from the SessionConfig options.
        projected_config: ProjectedPolicy,
        /// Connection capability the session is bound to.
        connection: u32,
    },
    /// Reconfigure an already-active session.
    ReconfigureClientDestination {
        /// Verified reconfiguration (caller must have validated every
        /// mutable/immutable classification).
        verified_session: VerifiedSessionConfig,
        /// Replacement destination policy.
        projected_config: ProjectedPolicy,
        /// Connection capability the session is bound to.
        connection: u32,
    },
    /// Destroy an already-active session and release every reservation.
    DestroyClientDestination {
        /// Connection capability the session is bound to.
        connection: u32,
        /// Session identifier that must be torn down.
        session: SessionId,
        /// Verified destination hash bound to the session.
        destination_hash: Hash,
    },
    /// Request a router bandwidth snapshot for the current connection.
    RequestBandwidthSnapshot {
        /// Connection capability that issued the request.
        connection: u32,
    },
    /// Request a destination lookup by hash or hostname.
    RequestDestinationLookup {
        /// Connection capability that issued the request.
        connection: u32,
        /// Lookup key: either a destination hash or a hostname.
        key: DestinationLookupKey,
    },
}

/// One destination lookup key carried in an action.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DestinationLookupKey {
    /// Hash lookup.
    Hash(Hash),
    /// Hostname lookup (client-supplied; resolution policy lives in
    /// Plan 168).
    Hostname(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_variants_are_constructible_from_typed_values() {
        // This is a compile-only check: every action variant must be
        // constructible from typed values, never from raw client bytes.
        let _action = I2cpAction::DestroyClientDestination {
            connection: 1,
            session: SessionId::new(7),
            destination_hash: Hash::from_bytes([0u8; 32]),
        };
        let _action = I2cpAction::RequestBandwidthSnapshot { connection: 1 };
        let _action = I2cpAction::RequestDestinationLookup {
            connection: 1,
            key: DestinationLookupKey::Hash(Hash::from_bytes([1u8; 32])),
        };
    }
}
