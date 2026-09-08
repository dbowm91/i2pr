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
//!
//! Plan 166 adds the [`I2cpAction::RequestVariableLeaseSet`] variant so
//! the Plan 167 daemon can ship a typed lease-request to the client
//! whenever the destination needs a fresh signed Standard LeaseSet2.
//!
//! Plan 168 re-exports the data-plane action vocabulary through
//! the `I2cpDataPlaneAction` enum from the `data_plane` module.
//! The `I2cpAction` enum remains the connection/session vocabulary;
//! the data-plane actions are deliberately separated so the
//! connection state machine and the payload dispatcher can evolve
//! independently.

use i2pr_proto::Hash;

use super::config::ProjectedPolicy;
use super::ids::SessionId;
use super::verify::VerifiedSessionConfig;

/// One typed action emitted by the Plan 165 connection/session state
/// machine and the Plan 166 destination-runtime polling layer.
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
    /// Ask the I2CP client to supply a fresh Standard LeaseSet2 for
    /// a client-owned destination (Plan 166 §5 / §9). The lease
    /// material is sourced from the destination's real inbound tunnel
    /// pool; the router never synthesizes replacement leases.
    RequestVariableLeaseSet {
        /// Connection capability the destination is bound to.
        connection: u32,
        /// Session identifier the destination belongs to.
        session: SessionId,
        /// Verified destination hash the lease request is bound to.
        destination_hash: Hash,
        /// Reason the lease set must be refreshed.
        cause: LeaseRefreshCause,
        /// Lease material the client should include in its next
        /// signed Standard LeaseSet2. The router never modifies the
        /// list after this action is emitted.
        leases: Vec<LeaseRequestLease>,
    },
}

/// Reason the destination must request a fresh LeaseSet2 from the
/// I2CP client. Mirrors the typed `ClientRefreshCause` vocabulary in
/// `i2pr-client` so the daemon never has to translate an integer
/// status code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum LeaseRefreshCause {
    /// The destination has no LeaseSet2 yet but is otherwise ready to
    /// publish.
    InitialGeneration,
    /// The previously installed LeaseSet2 is approaching the
    /// configured rotation margin and must be replaced before its
    /// leases expire.
    ApproachingExpiry,
}

/// One lease entry inside a [`I2cpAction::RequestVariableLeaseSet`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeaseRequestLease {
    /// Inbound gateway router hash.
    pub gateway: Hash,
    /// Tunnel identifier the gateway must accept delivery on.
    pub tunnel_id: u32,
    /// Absolute lease end-date in seconds.
    pub end_date_seconds: u32,
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
        let _action = I2cpAction::RequestVariableLeaseSet {
            connection: 1,
            session: SessionId::new(3),
            destination_hash: Hash::from_bytes([2u8; 32]),
            cause: LeaseRefreshCause::InitialGeneration,
            leases: Vec::new(),
        };
    }
}
