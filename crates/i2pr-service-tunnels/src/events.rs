//! Typed service-tunnel events and snapshots.
//!
//! Events and snapshots carry counts, identifiers, and bounded
//! diagnostics only. They never carry socket handles, Tokio guards,
//! or secret material.

#![forbid(unsafe_code)]

/// Typed service-tunnel lifecycle event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceTunnelEvent {
    /// A service specification validated structurally.
    SpecValidated {
        /// Service identifier.
        id: String,
        /// Service kind spelling.
        kind: &'static str,
    },
    /// A service specification failed validation.
    SpecRejected {
        /// Service identifier (or `<unknown>`).
        id: String,
        /// Machine-readable reason.
        reason: &'static str,
    },
    /// A service-tunnel set validated as a whole.
    SetValidated {
        /// Number of services.
        service_count: usize,
    },
    /// A local connection was admitted within budget.
    ConnectionAdmitted {
        /// Service identifier.
        id: String,
        /// Active connections for the service after admission.
        active_for_service: usize,
        /// Aggregate active connections after admission.
        active_aggregate: usize,
    },
    /// A local connection closed and released budget.
    ConnectionClosed {
        /// Service identifier.
        id: String,
        /// Active connections for the service after release.
        active_for_service: usize,
        /// Aggregate active connections after release.
        active_aggregate: usize,
    },
}

/// Bounded snapshot of service-tunnel accounting.
///
/// All counters are point-in-time values for diagnostics. No queue
/// grows with memory as backpressure; saturation is signaled by
/// rejection, not by buffer growth.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ServiceTunnelSnapshot {
    /// Number of configured services.
    pub configured_services: usize,
    /// Number of enabled services.
    pub enabled_services: usize,
    /// Aggregate active connections.
    pub active_connections: usize,
    /// Number of connections rejected for budget exhaustion.
    pub rejected_connections: u64,
    /// Number of destination reference failures.
    pub destination_failures: u64,
}

impl ServiceTunnelSnapshot {
    /// Creates an empty snapshot.
    pub fn empty() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_is_empty_by_default() {
        let snapshot = ServiceTunnelSnapshot::empty();
        assert_eq!(snapshot.configured_services, 0);
        assert_eq!(snapshot.active_connections, 0);
    }

    #[test]
    fn events_carry_no_socket_handles() {
        // Compile-time shape check: events are cloneable value
        // types with no socket or guard fields.
        let event = ServiceTunnelEvent::ConnectionAdmitted {
            id: "alpha".to_owned(),
            active_for_service: 1,
            active_aggregate: 1,
        };
        let cloned = event.clone();
        assert_eq!(event, cloned);
    }
}
