//! Destination-owned tunnel pool policy.
//!
//! Plan 120 §5/§6 requires destination-specific pool ownership without forking
//! tunnel cryptography or the data plane. The pool below reuses the shared
//! bounded container [`BoundedTunnelPool`] from `i2pr-tunnel` (the exploratory
//! pool's slot lifetime, expiry, activation ownership, and failure accounting)
//! and layers destination policy on top. Only real one-shot
//! [`EstablishedMaterial`] is accepted; there is no production placeholder
//! path.

use i2pr_proto::Hash;
use i2pr_tunnel::{
    BoundedTunnelPool, EstablishedMaterial, RegisterError, RegisterOutcome, TunnelDirection,
    TunnelSlot, TunnelState,
};

use crate::config::{DestinationConfig, DestinationConfigError};

/// Public routing metadata for one usable destination inbound tunnel.
///
/// This is exactly the non-secret data a `Lease2` advertises. Secret tunnel
/// layer keys never reach this struct.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InboundLeaseSource {
    slot: TunnelSlot,
    gateway: Hash,
    gateway_receive_tunnel_id: u32,
    tunnel_expires_seconds: u64,
    advertised_expires_seconds: u64,
}

impl InboundLeaseSource {
    /// Pool slot that owns the tunnel.
    pub const fn slot(&self) -> TunnelSlot {
        self.slot
    }

    /// Inbound gateway router hash advertised in the `Lease2`.
    pub const fn gateway(&self) -> Hash {
        self.gateway
    }

    /// Inbound gateway receive tunnel id advertised in the `Lease2`.
    pub const fn gateway_receive_tunnel_id(&self) -> u32 {
        self.gateway_receive_tunnel_id
    }

    /// Real tunnel usability deadline in seconds.
    pub const fn tunnel_expires_seconds(&self) -> u64 {
        self.tunnel_expires_seconds
    }

    /// Advertised lease end date in seconds. Always strictly less than
    /// [`Self::tunnel_expires_seconds`] whenever the configured publication
    /// margin is nonzero, and never greater than it.
    pub const fn advertised_expires_seconds(&self) -> u64 {
        self.advertised_expires_seconds
    }
}

/// Disposition of a bounded destination build failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildFailureDisposition {
    /// The destination may schedule another bounded replacement attempt.
    RetryPermitted {
        /// Number of consecutive failures observed so far.
        consecutive_failures: u16,
    },
    /// The configured failure threshold was reached; replacement is paused
    /// until a successful registration resets the counter.
    Exhausted {
        /// Number of consecutive failures observed so far.
        consecutive_failures: u16,
    },
}

/// Bounded destination tunnel pool.
#[derive(Debug)]
pub struct DestinationTunnelPool {
    config: DestinationConfig,
    inner: BoundedTunnelPool,
    consecutive_failures: u16,
}

impl DestinationTunnelPool {
    /// Constructs a destination pool from the bounded destination policy.
    pub fn new(config: DestinationConfig) -> Result<Self, DestinationPoolError> {
        let pool_config = config
            .pool_config()
            .map_err(|error| DestinationPoolError::Configuration(Box::new(error)))?;
        Ok(Self {
            config,
            inner: BoundedTunnelPool::new(pool_config),
            consecutive_failures: 0,
        })
    }

    /// Returns the destination policy backing this pool.
    pub const fn config(&self) -> DestinationConfig {
        self.config
    }

    /// Registers real inbound established material.
    pub fn register_inbound(
        &mut self,
        established: EstablishedMaterial,
        now_seconds: u64,
    ) -> Result<TunnelSlot, DestinationPoolError> {
        if established.direction() != TunnelDirection::Inbound {
            return Err(DestinationPoolError::DirectionMismatch {
                expected: TunnelDirection::Inbound,
            });
        }
        let outcome = self
            .inner
            .register_inbound_with_material(established, now_seconds)
            .map_err(DestinationPoolError::from)?;
        self.consecutive_failures = 0;
        Ok(outcome.slot())
    }

    /// Registers real outbound established material.
    pub fn register_outbound(
        &mut self,
        established: EstablishedMaterial,
        now_seconds: u64,
    ) -> Result<TunnelSlot, DestinationPoolError> {
        if established.direction() != TunnelDirection::Outbound {
            return Err(DestinationPoolError::DirectionMismatch {
                expected: TunnelDirection::Outbound,
            });
        }
        let outcome = self
            .inner
            .register_outbound_with_material(established, now_seconds)
            .map_err(DestinationPoolError::from)?;
        self.consecutive_failures = 0;
        Ok(outcome.slot())
    }

    /// Advances the pool's deterministic view of time and returns the slots
    /// evicted because their tunnels expired.
    pub fn advance_time(&mut self, now_seconds: u64) -> Vec<TunnelSlot> {
        self.inner.advance_time(now_seconds)
    }

    /// Records a bounded destination build/replacement failure.
    pub fn note_build_failure(&mut self) -> BuildFailureDisposition {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.consecutive_failures >= self.config.failure_threshold() {
            BuildFailureDisposition::Exhausted {
                consecutive_failures: self.consecutive_failures,
            }
        } else {
            BuildFailureDisposition::RetryPermitted {
                consecutive_failures: self.consecutive_failures,
            }
        }
    }

    /// Number of consecutive build/replacement failures since the last
    /// successful registration.
    pub const fn consecutive_failures(&self) -> u16 {
        self.consecutive_failures
    }

    /// Whether bounded replacement is paused because the failure threshold was
    /// reached.
    pub const fn replacement_paused(&self) -> bool {
        self.consecutive_failures >= self.config.failure_threshold()
    }

    /// Marks a slot failed, removing it and incrementing the bounded failure
    /// counter.
    pub fn mark_failed(&mut self, slot: TunnelSlot) -> bool {
        let removed = self.inner.mark_failed(slot).is_some();
        if removed {
            let _ = self.note_build_failure();
        }
        removed
    }

    /// Removes a slot unconditionally, dropping (and zeroizing) its material.
    pub fn remove(&mut self, slot: TunnelSlot) -> bool {
        self.inner.remove(slot).is_some()
    }

    /// Number of registered inbound tunnels.
    pub fn inbound_len(&self) -> usize {
        self.inner.inbound_len()
    }

    /// Number of registered outbound tunnels.
    pub fn outbound_len(&self) -> usize {
        self.inner.outbound_len()
    }

    /// Total number of registered tunnels.
    pub fn len(&self) -> usize {
        self.inbound_len() + self.outbound_len()
    }

    /// Whether the pool holds no tunnels.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether the pool satisfies the configured minimum usable inbound count
    /// and holds at least one usable outbound tunnel.
    pub fn is_usable(&self, now_seconds: u64) -> bool {
        let inbound = self.inbound_lease_sources(now_seconds).len();
        inbound >= usize::from(self.config.minimum_usable_inbound()) && self.outbound_len() > 0
    }

    /// Returns the public routing metadata for every currently usable inbound
    /// tunnel, in deterministic slot order.
    ///
    /// A tunnel is excluded when it is not `Established`, when its real expiry
    /// has already passed, or when the configured publication margin would
    /// produce a lease that has already ended.
    pub fn inbound_lease_sources(&self, now_seconds: u64) -> Vec<InboundLeaseSource> {
        let lifetime = u64::from(self.config.tunnel_lifetime_seconds());
        let margin = u64::from(self.config.lease_publication_margin_seconds());
        let mut sources = Vec::new();
        for registration in self.inner.inbound_registrations() {
            if registration.state() != TunnelState::Established {
                continue;
            }
            let slot = registration.slot();
            let Some(routing) = self.inner.routing(slot) else {
                continue;
            };
            let tunnel_expires = registration.created_at_seconds().saturating_add(lifetime);
            if tunnel_expires <= now_seconds {
                continue;
            }
            let advertised = tunnel_expires.saturating_sub(margin);
            if advertised <= now_seconds {
                continue;
            }
            sources.push(InboundLeaseSource {
                slot,
                gateway: routing.first_hop_router(),
                gateway_receive_tunnel_id: routing.first_hop_receive_tunnel().get(),
                tunnel_expires_seconds: tunnel_expires,
                advertised_expires_seconds: advertised,
            });
        }
        sources.sort_by_key(|source| source.slot.get());
        sources
    }

    /// Releases every pool registration, returning the number of slots
    /// dropped. Established material is zeroized by its own `Drop` impl.
    pub fn release_all(&mut self) -> usize {
        let mut slots: Vec<TunnelSlot> = self
            .inner
            .inbound_registrations()
            .into_iter()
            .map(|registration| registration.slot())
            .collect();
        slots.extend(
            self.inner
                .outbound_registrations()
                .into_iter()
                .map(|registration| registration.slot()),
        );
        let mut released = 0;
        for slot in slots {
            if self.inner.remove(slot).is_some() {
                released += 1;
            }
        }
        self.consecutive_failures = 0;
        released
    }
}

/// Typed destination pool failures.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DestinationPoolError {
    /// The destination policy could not be projected onto the shared bounded
    /// pool configuration.
    #[error("destination pool configuration rejected: {0}")]
    Configuration(Box<i2pr_tunnel::ExploratoryConfigError>),
    /// The destination policy itself was rejected.
    #[error("destination configuration rejected: {0}")]
    Policy(#[from] DestinationConfigError),
    /// Material for the wrong direction was offered.
    #[error("established material direction mismatch, expected {expected:?}")]
    DirectionMismatch {
        /// The direction the call site required.
        expected: TunnelDirection,
    },
    /// The bounded pool rejected the registration.
    #[error("destination pool registration rejected: {0}")]
    Register(String),
}

impl From<RegisterError> for DestinationPoolError {
    fn from(error: RegisterError) -> Self {
        match error {
            RegisterError::Full { kind, .. } => Self::Register(format!("{kind}")),
            RegisterError::Invalid(inner) => Self::Register(format!("{inner}")),
        }
    }
}

/// Convenience accessor that mirrors [`RegisterOutcome::slot`] for callers that
/// only need the slot identifier.
pub const fn outcome_slot(outcome: &RegisterOutcome) -> TunnelSlot {
    match outcome {
        RegisterOutcome::Inserted { slot, .. } | RegisterOutcome::Duplicate { slot } => *slot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{established_inbound, established_outbound};

    fn pool() -> DestinationTunnelPool {
        DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool")
    }

    #[test]
    fn inbound_material_yields_gateway_and_receive_tunnel_id() {
        let mut pool = pool();
        let material = established_inbound(11);
        let expected_gateway = material.inbound_gateway().0.hash();
        let expected_tunnel = material.inbound_gateway().1.get();
        pool.register_inbound(material, 1_000).expect("register");
        let sources = pool.inbound_lease_sources(1_000);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].gateway(), expected_gateway);
        assert_eq!(sources[0].gateway_receive_tunnel_id(), expected_tunnel);
    }

    #[test]
    fn advertised_expiry_never_exceeds_tunnel_expiry() {
        let mut pool = pool();
        pool.register_inbound(established_inbound(12), 1_000)
            .expect("register");
        let source = pool.inbound_lease_sources(1_000)[0];
        let lifetime = u64::from(DestinationConfig::balanced().tunnel_lifetime_seconds());
        assert_eq!(source.tunnel_expires_seconds(), 1_000 + lifetime);
        assert!(source.advertised_expires_seconds() < source.tunnel_expires_seconds());
        assert_eq!(
            source.advertised_expires_seconds(),
            source.tunnel_expires_seconds()
                - u64::from(DestinationConfig::balanced().lease_publication_margin_seconds())
        );
    }

    #[test]
    fn expired_or_failed_tunnel_is_not_advertised() {
        let mut pool = pool();
        let slot = pool
            .register_inbound(established_inbound(13), 0)
            .expect("register");
        assert_eq!(pool.inbound_lease_sources(0).len(), 1);
        let lifetime = u64::from(DestinationConfig::balanced().tunnel_lifetime_seconds());
        // Inside the publication margin the lease is already withheld.
        assert!(pool.inbound_lease_sources(lifetime - 1).is_empty());
        assert_eq!(pool.advance_time(lifetime), vec![slot]);
        assert!(pool.inbound_lease_sources(lifetime).is_empty());

        let mut second = pool_with_one_inbound(14);
        let slot = second.inbound_lease_sources(0)[0].slot();
        assert!(second.mark_failed(slot));
        assert!(second.inbound_lease_sources(0).is_empty());
        assert_eq!(second.consecutive_failures(), 1);
    }

    fn pool_with_one_inbound(seed: u64) -> DestinationTunnelPool {
        let mut pool = pool();
        pool.register_inbound(established_inbound(seed), 0)
            .expect("register");
        pool
    }

    #[test]
    fn direction_mismatch_is_rejected() {
        let mut pool = pool();
        let error = pool
            .register_inbound(established_outbound(15), 0)
            .expect_err("mismatch");
        assert!(matches!(
            error,
            DestinationPoolError::DirectionMismatch {
                expected: TunnelDirection::Inbound
            }
        ));
        let error = pool
            .register_outbound(established_inbound(16), 0)
            .expect_err("mismatch");
        assert!(matches!(
            error,
            DestinationPoolError::DirectionMismatch {
                expected: TunnelDirection::Outbound
            }
        ));
    }

    #[test]
    fn build_failures_are_bounded_and_reset_on_success() {
        let config =
            DestinationConfig::try_new(2, 2, 1, 2, 600, 2, 2, 64, 1024, 60, 120).expect("config");
        let mut pool = DestinationTunnelPool::new(config).expect("pool");
        assert_eq!(
            pool.note_build_failure(),
            BuildFailureDisposition::RetryPermitted {
                consecutive_failures: 1
            }
        );
        assert_eq!(
            pool.note_build_failure(),
            BuildFailureDisposition::Exhausted {
                consecutive_failures: 2
            }
        );
        assert!(pool.replacement_paused());
        pool.register_inbound(established_inbound(17), 0)
            .expect("register");
        assert_eq!(pool.consecutive_failures(), 0);
        assert!(!pool.replacement_paused());
    }

    #[test]
    fn usability_requires_minimum_inbound_and_one_outbound() {
        let mut pool = pool();
        assert!(!pool.is_usable(0));
        pool.register_inbound(established_inbound(18), 0)
            .expect("register");
        assert!(!pool.is_usable(0));
        pool.register_outbound(established_outbound(19), 0)
            .expect("register");
        assert!(pool.is_usable(0));
    }

    #[test]
    fn release_all_drops_every_slot() {
        let mut pool = pool();
        pool.register_inbound(established_inbound(20), 0)
            .expect("register");
        pool.register_outbound(established_outbound(21), 0)
            .expect("register");
        assert_eq!(pool.len(), 2);
        assert_eq!(pool.release_all(), 2);
        assert!(pool.is_empty());
    }

    #[test]
    fn replacement_tunnel_replaces_the_advertised_lease_source() {
        let mut pool = pool();
        let first = pool
            .register_inbound(established_inbound(22), 0)
            .expect("register");
        let original = pool.inbound_lease_sources(0)[0];
        assert!(pool.remove(first));
        pool.register_inbound(established_inbound(23), 10)
            .expect("register");
        let replacement = pool.inbound_lease_sources(10)[0];
        assert_ne!(replacement.slot(), original.slot());
        assert_ne!(replacement.gateway(), original.gateway());
    }
}
