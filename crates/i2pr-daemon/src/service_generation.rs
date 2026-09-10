//! Plan 180 §3 committed-generation bookkeeping.
//!
//! `ServiceTunnelGeneration` is the daemon-side committed-generation
//! record that the [`ServiceTunnelManager`](crate::service_tunnels::ServiceTunnelManager) owns
//! exactly one of at a time. The struct holds:
//!
//! - the monotonically increasing generation id;
//! - the committed [`ServiceTunnelSpec`](i2pr_service_tunnels::ServiceTunnelSpec) set in
//!   configuration order;
//! - the per-spec runtime map (`Arc<ServiceRuntime>`);
//! - the per-spec destination bridge map;
//! - the per-spec destination runtime registry;
//! - a generation-scoped snapshot the daemon uses for diagnostics
//!   and the Plan 180 §9 resource accounting matrix.
//!
//! The struct never owns secret bytes; secrets live inside the
//! `Arc<DestinationIdentity>` shared with the SAM bridge, and the
//! `Debug` impl prints counts and identifiers only. Two generations
//! may co-exist transiently during a reconcile commit (the old
//! generation continues to drain its existing connections while the
//! new one accepts new traffic), but the manager commits exactly
//! one authoritative generation per moment.
//!
//! Plan 180 §3/§8/§9 requirements:
//!
//! - one generation authoritative at a time;
//! - the manager must be able to stage a replacement while the
//!   current generation continues serving existing connections;
//! - drain accounting must distinguish `active_current_generation`,
//!   `active_draining_generation`, and `forced_drain_closes_total`;
//! - counters are bounded/sanitized and must not carry destination
//!   names or application data.
//!
//! This module is runtime-owned (daemon-only); it carries
//! `Arc<...>` handles and never enters the runtime-neutral crate.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use i2pr_client::{DestinationId, DestinationRegistry};
use i2pr_runtime::CancellationToken;
use i2pr_service_tunnels::ServiceTunnelSet;

use crate::sam::streams::{SamDestinationHandle, SamDestinations};

use crate::service_tunnels::ServiceRuntime;

/// Bounded counter set the daemon exposes for the Plan 180 §9
/// unified resource accounting. Counters are per-generation and
/// the manager sums active generations in the cross-generation
/// snapshot path.
#[derive(Debug)]
pub struct GenerationCounters {
    /// Active connections attributed to this generation's services
    /// after the commit moment.
    pub active_current_generation: usize,
    /// Active connections still draining on this generation after
    /// it has been replaced by a newer committed generation.
    pub active_draining_generation: usize,
    /// Cumulative count of connections forcibly closed by the drain
    /// deadline (never reset for the lifetime of the manager).
    pub forced_drain_closes_total: AtomicU64,
}

impl Default for GenerationCounters {
    fn default() -> Self {
        Self {
            active_current_generation: 0,
            active_draining_generation: 0,
            forced_drain_closes_total: AtomicU64::new(0),
        }
    }
}

impl Clone for GenerationCounters {
    fn clone(&self) -> Self {
        Self {
            active_current_generation: self.active_current_generation,
            active_draining_generation: self.active_draining_generation,
            forced_drain_closes_total: AtomicU64::new(
                self.forced_drain_closes_total.load(Ordering::Relaxed),
            ),
        }
    }
}

impl GenerationCounters {
    /// Atomically increments the forced-drain counter and returns
    /// the previous value.
    pub fn increment_forced(&self) -> u64 {
        self.forced_drain_closes_total
            .fetch_add(1, Ordering::Relaxed)
    }

    /// Returns the current forced-drain counter value.
    pub fn forced(&self) -> u64 {
        self.forced_drain_closes_total.load(Ordering::Relaxed)
    }
}

/// One committed generation. Lives under the manager's
/// `committed_generation` slot for the authoritative period, then
/// is moved into the `draining` slot until its connections close
/// or the deadline forces termination.
pub struct ServiceTunnelGeneration {
    /// Monotonic generation id.
    pub generation_id: u64,
    /// The committed service-tunnel set (used as the diff baseline
    /// for the next reconcile).
    pub committed_specs: Arc<ServiceTunnelSet>,
    /// Per-spec-id runtime map.
    pub runtimes: HashMap<String, Arc<ServiceRuntime>>,
    /// Per-destination-id SAM bridge map. Held by the generation so
    /// the cross-tunnel local delivery path can resolve against the
    /// generation's own service destinations.
    pub sam_destinations: SamDestinations,
    /// Per-generation destination runtime registry.
    pub destination_registry: DestinationRegistry,
    /// Per-generation resource counters.
    pub counters: GenerationCounters,
}

impl std::fmt::Debug for ServiceTunnelGeneration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServiceTunnelGeneration")
            .field("generation_id", &self.generation_id)
            .field("committed_specs", &self.committed_specs)
            .field("runtimes", &self.runtimes.len())
            .field("sam_destinations", &self.sam_destinations.len())
            .field("counters", &self.counters)
            .finish_non_exhaustive()
    }
}

/// One generation currently in the drain phase. Owned by the
/// manager's `draining` list; released back to the caller when its
/// `active_draining_generation` counter reaches zero or the
/// deadline fires.
pub struct DrainingGeneration {
    /// The generation being drained.
    pub generation: ServiceTunnelGeneration,
    /// The original committed generation id (preserved even if the
    /// authoritative id has since moved on).
    pub generation_id: u64,
    /// Deadline instant after which the manager forcibly cancels
    /// residual connections.
    pub drain_deadline: std::time::Instant,
    /// Cancellation token the manager uses to force-close residual
    /// connections once the deadline expires.
    pub cancellation: CancellationToken,
    /// Total number of destinations in this draining generation.
    pub destination_count: usize,
}

impl std::fmt::Debug for DrainingGeneration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DrainingGeneration")
            .field("generation_id", &self.generation_id)
            .field("destination_count", &self.destination_count)
            .field(
                "active_draining_generation",
                &self.generation.counters.active_draining_generation,
            )
            .field(
                "forced_drain_closes_total",
                &self.generation.counters.forced_drain_closes_total,
            )
            .finish_non_exhaustive()
    }
}

/// Atomic counter pair used by the manager to assign generation
/// ids. Lives behind a single shared atomic so even concurrent
/// reconcile attempts observe a stable, monotonic id.
#[derive(Debug, Default)]
pub struct GenerationIdAllocator {
    next: AtomicU64,
}

impl GenerationIdAllocator {
    /// Allocates and returns the next generation id.
    pub fn allocate(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
}

/// One destination lookup result. The cross-tunnel local-delivery
/// path uses this so both committed and draining generations can
/// resolve their own destinations without a global registry.
#[derive(Clone, Debug)]
pub struct DestinationResolution {
    /// Destination identifier that owns the resolved bridge.
    pub destination_id: DestinationId,
    /// Bridge handle that owns the resolved destination.
    pub bridge: SamDestinationHandle,
    /// Whether the destination belongs to a draining generation
    /// (the caller must continue serving it but may not stage new
    /// services against it).
    pub draining: bool,
}
