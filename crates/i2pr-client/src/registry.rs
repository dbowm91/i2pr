//! Destination runtime, non-secret handle, and bounded router-local registry.
//!
//! Plan 120 §3/§4/§11 owns the deterministic destination lifecycle. The runtime
//! composes the destination identity owner, the destination tunnel pool, the
//! LeaseSet2 lifecycle, and the bounded payload queues. Readiness is derived
//! from real usable tunnels: a destination is never `Usable` merely because its
//! keys exist.
//!
//! Plan 166 preserves the router-owned path unchanged and adds a
//! capability-oriented client-owned mode that shares one destination
//! runtime. The router-owned seam still owns the destination signing
//! private key and at most one `Arc<DestinationIdentity>` allocation; the
//! client-owned seam stores the public destination, an optional
//! [`InboundDecryptionCapability`] once a LeaseSet2 commits, and never
//! sees the destination signing private key. Both modes drive the same
//! destination tunnel pool, registry, ECIES session manager, and
//! dispatcher.

use std::collections::BTreeMap;
use std::sync::Arc;

use i2pr_core::{DegradationCode, HealthState};
use i2pr_netdb::DestinationHash;
use i2pr_proto::{Hash, LeaseSet2};
use i2pr_tunnel::{EstablishedMaterial, TunnelSlot};

use crate::config::{DestinationConfig, RegistryConfig};
use crate::identity::{
    DestinationId, DestinationIdentity, DestinationIdentityError, DestinationOwnership,
    DestinationPublic, InboundDecryptionCapability,
};
use crate::leaseset::{
    LeaseRequest, LeaseRequestLease, LeaseSetDecision, LeaseSetError, LeaseSetLifecycle,
    LeaseSetSummary, LocalLeaseSet,
};
use crate::message::{
    BoundedPayloadQueue, DestinationPayload, PayloadError, QueuedOutbound, RoutingUnavailable,
};
use crate::pool::{
    BuildFailureDisposition, DestinationPoolError, DestinationTunnelPool, InboundLeaseSource,
};

/// Deterministic destination lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestinationState {
    /// Keys exist; no tunnel work has been admitted yet.
    Initializing,
    /// At least one tunnel has been admitted but the configured minimum usable
    /// inbound count and one usable outbound tunnel are not both satisfied.
    BuildingTunnels,
    /// Enough usable tunnels exist and a signed, self-validated LeaseSet2 is
    /// held.
    Usable,
    /// Tunnels or the LeaseSet2 regressed after the destination was usable.
    Degraded,
    /// Shutdown was requested; no new LeaseSet2 or payload is accepted.
    Stopping,
    /// Every destination-owned resource has been released.
    Stopped,
}

impl DestinationState {
    /// Projects the destination state onto the shared `i2pr-core` health
    /// vocabulary.
    pub const fn health(self) -> HealthState {
        match self {
            Self::Initializing | Self::BuildingTunnels => HealthState::Starting,
            Self::Usable => HealthState::Ready,
            Self::Degraded => HealthState::Degraded(DegradationCode::DependencyUnavailable),
            Self::Stopping => HealthState::Stopping,
            Self::Stopped => HealthState::Failed,
        }
    }

    /// Whether the destination may accept new application payloads.
    pub const fn accepts_payloads(self) -> bool {
        matches!(
            self,
            Self::Initializing | Self::BuildingTunnels | Self::Usable | Self::Degraded
        )
    }

    /// Whether the destination is publication-ready.
    pub const fn is_publishable(self) -> bool {
        matches!(self, Self::Usable)
    }
}

/// Bounded command vocabulary a caller may address to a destination. Plan 122
/// owns the asynchronous transport of these commands; Plan 120 defines the
/// contract so the runtime surface is stable.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DestinationCommand {
    /// Enqueue an outbound application payload.
    EnqueueOutbound(DestinationPayload),
    /// Re-evaluate the LeaseSet2 lifecycle at the supplied deterministic time.
    RefreshLeaseSet {
        /// Deterministic caller-supplied clock reading.
        now_seconds: u64,
    },
    /// Request graceful shutdown.
    Shutdown,
}

/// Bounded event vocabulary a destination emits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DestinationEvent {
    /// The destination lifecycle state changed.
    StateChanged {
        /// Previous state.
        previous: DestinationState,
        /// New state.
        current: DestinationState,
    },
    /// A new LeaseSet2 version was generated and self-validated.
    LeaseSetGenerated {
        /// The generated record's published timestamp.
        published_seconds: u32,
        /// Number of advertised leases.
        lease_count: usize,
    },
    /// The destination lost every usable inbound tunnel and is no longer
    /// publishable.
    LeaseSetWithdrawn,
    /// A bounded tunnel build/replacement failed.
    BuildFailed {
        /// Disposition of the bounded failure.
        disposition: BuildFailureDisposition,
    },
    /// Every destination-owned resource was released.
    Stopped {
        /// Pool slots released.
        released_pool_slots: usize,
        /// Pending payloads dropped.
        released_payloads: usize,
    },
}

/// One local destination runtime.
#[derive(Debug)]
pub struct DestinationRuntime {
    ownership: DestinationOwnership,
    /// Router-owned destinations own their signing + X25519 secrets
    /// through this `Arc`. Client-owned destinations leave this field
    /// `None`; the destination signing private key never enters the
    /// runtime in that mode.
    identity: Option<Arc<DestinationIdentity>>,
    /// Public destination material for client-owned mode. For
    /// router-owned mode the public view is derived from
    /// `identity`; the field is `Some` for both so the ECIES
    /// dispatcher and the Plan 167 daemon can read the public
    /// destination without branching.
    public: DestinationPublic,
    /// Client-owned inbound decryption capability. `None` until a
    /// LeaseSet2 is installed and validated; `Some` once the
    /// destination is usable for inbound ECIES traffic.
    decryption: Option<InboundDecryptionCapability>,
    config: DestinationConfig,
    pool: DestinationTunnelPool,
    lease_sets: LeaseSetLifecycle,
    outbound: BoundedPayloadQueue,
    inbound: BoundedPayloadQueue,
    state: DestinationState,
}

impl DestinationRuntime {
    /// Constructs a router-owned destination runtime around an owned
    /// identity.
    ///
    /// The identity is moved into a single `Arc<DestinationIdentity>` allocation.
    /// Callers that already hold a shared `Arc<DestinationIdentity>` (e.g. a
    /// SAM product bridge that must share one secret allocation with the
    /// runtime) should use [`Self::with_shared_identity`] instead.
    pub fn new(
        identity: DestinationIdentity,
        config: DestinationConfig,
    ) -> Result<Self, DestinationRuntimeError> {
        Self::with_shared_identity(Arc::new(identity), config)
    }

    /// Constructs a router-owned destination runtime that shares an
    /// existing `Arc<DestinationIdentity>` allocation with another owner
    /// (e.g. the SAM product bridge). Plan 149 §3 Option A preserves the
    /// "one logical destination -> one private identity allocation" invariant
    /// by routing both owners through the same `Arc`.
    pub fn with_shared_identity(
        identity: Arc<DestinationIdentity>,
        config: DestinationConfig,
    ) -> Result<Self, DestinationRuntimeError> {
        let public = DestinationPublic::from_destination(identity.destination().clone())?;
        Self::assemble_router_owned(identity, public, config)
    }

    fn assemble_router_owned(
        identity: Arc<DestinationIdentity>,
        public: DestinationPublic,
        config: DestinationConfig,
    ) -> Result<Self, DestinationRuntimeError> {
        Ok(Self {
            ownership: DestinationOwnership::RouterOwned,
            identity: Some(identity),
            public,
            decryption: None,
            config,
            pool: DestinationTunnelPool::new(config)?,
            lease_sets: LeaseSetLifecycle::new(config),
            outbound: BoundedPayloadQueue::new(
                config.max_pending_messages(),
                config.max_pending_bytes(),
            ),
            inbound: BoundedPayloadQueue::new(
                config.max_pending_messages(),
                config.max_pending_bytes(),
            ),
            state: DestinationState::Initializing,
        })
    }

    /// Constructs a client-owned destination runtime from a verified
    /// SessionConfig-derived public destination and the projected
    /// destination policy. The runtime never holds the client's
    /// signing private key. Inbound traffic is rejected until
    /// [`Self::install_client_lease_set2`] commits a matched
    /// [`InboundDecryptionCapability`] alongside a validated Standard
    /// LeaseSet2.
    pub fn new_client_owned(
        public: DestinationPublic,
        config: DestinationConfig,
    ) -> Result<Self, DestinationPoolError> {
        Ok(Self {
            ownership: DestinationOwnership::ClientOwned,
            identity: None,
            public,
            decryption: None,
            config,
            pool: DestinationTunnelPool::new(config)?,
            lease_sets: LeaseSetLifecycle::new(config),
            outbound: BoundedPayloadQueue::new(
                config.max_pending_messages(),
                config.max_pending_bytes(),
            ),
            inbound: BoundedPayloadQueue::new(
                config.max_pending_messages(),
                config.max_pending_bytes(),
            ),
            state: DestinationState::BuildingTunnels,
        })
    }

    /// Returns the destination ownership mode.
    pub const fn ownership(&self) -> DestinationOwnership {
        self.ownership
    }

    /// Returns the non-secret destination identifier.
    pub fn id(&self) -> DestinationId {
        self.public.id()
    }

    /// Borrows the destination identity owner. Only the runtime itself uses
    /// this; the public [`DestinationHandle`] deliberately does not expose it.
    /// Returns `None` for client-owned destinations; the router never
    /// receives the destination signing private key in that mode.
    pub fn identity(&self) -> Option<&DestinationIdentity> {
        self.identity.as_deref()
    }

    /// Returns a clone of the shared `Arc<DestinationIdentity>` capability.
    /// Plan 149 §3 Option A lets the SAM product bridge and the runtime share
    /// one secret allocation: callers that need long-lived identity access
    /// (e.g. the SAM STREAM bridge) clone this `Arc`, never the underlying
    /// `DestinationIdentity`. Returns `None` for client-owned destinations.
    pub fn identity_arc(&self) -> Option<Arc<DestinationIdentity>> {
        self.identity.as_ref().map(Arc::clone)
    }

    /// Borrows the public destination (signing key bytes included). Both
    /// router-owned and client-owned runtimes expose this view so callers
    /// never need to branch on ownership to read the public side.
    pub const fn public(&self) -> &DestinationPublic {
        &self.public
    }

    /// Returns a borrowed view of the installed inbound decryption
    /// capability. `None` for router-owned destinations (the
    /// [`DestinationIdentity`] owns the secret directly) or until a
    /// client-owned destination installs a LeaseSet2.
    pub fn decryption_capability(&self) -> Option<&InboundDecryptionCapability> {
        self.decryption.as_ref()
    }

    /// Returns the static X25519 public-key bytes the runtime uses to
    /// decrypt inbound ECIES traffic. Both ownership modes expose the
    /// same lookup so the ECIES primitives do not need to branch.
    pub fn static_public_bytes(&self) -> &[u8; i2pr_crypto::X25519_KEY_LENGTH] {
        // Both ownership modes cache the destination's static X25519
        // public key inside the [`DestinationPublic`] field; routing
        // through the cached copy avoids a second copy out of the
        // router-owned identity.
        self.public.static_public_bytes()
    }

    /// Returns the static X25519 secret bytes the runtime uses to
    /// decrypt inbound ECIES traffic. Router-owned destinations hand
    /// the secret out from [`DestinationIdentity`]; client-owned
    /// destinations hand it out from the installed capability.
    pub fn static_secret_bytes(&self) -> Option<&[u8; i2pr_crypto::X25519_KEY_LENGTH]> {
        if let Some(identity) = self.identity.as_ref() {
            Some(identity.static_secret_bytes())
        } else {
            self.decryption
                .as_ref()
                .map(|capability| capability.secret_bytes())
        }
    }

    /// Returns the NetDB destination key.
    pub fn netdb_key(&self) -> DestinationHash {
        self.public.id().as_netdb_key()
    }

    /// Returns the destination hash by value.
    pub fn destination_hash(&self) -> Hash {
        self.public.id().as_hash().copy()
    }

    /// Returns the current lifecycle state.
    pub const fn state(&self) -> DestinationState {
        self.state
    }

    /// Returns the destination policy.
    pub const fn config(&self) -> DestinationConfig {
        self.config
    }

    /// Borrows the destination tunnel pool.
    pub const fn pool(&self) -> &DestinationTunnelPool {
        &self.pool
    }

    /// Mutably borrows the destination tunnel pool. Production
    /// callers use the typed `admit_*` accessors; this seam exists
    /// for fixtures and Plan 166 tests that need to drop a tunnel
    /// out of band to simulate expiry races.
    pub fn pool_mut(&mut self) -> &mut DestinationTunnelPool {
        &mut self.pool
    }

    /// Borrows the current router-signed LeaseSet2, when one is held.
    /// Router-owned destinations always read from here; client-owned
    /// destinations read from [`Self::client_lease_set`] instead.
    pub const fn lease_set(&self) -> Option<&LocalLeaseSet> {
        self.lease_sets.current()
    }

    /// Borrows the current client-supplied LeaseSet2, when one has been
    /// installed. Router-owned destinations always return `None` here.
    pub const fn client_lease_set(&self) -> Option<&crate::leaseset::ClientOwnedLeaseSet> {
        self.lease_sets.client_current()
    }

    /// Returns a non-secret LeaseSet2 summary that covers whichever
    /// ownership mode is active.
    pub fn lease_set_summary(&self) -> LeaseSetSummary {
        LeaseSetSummary::from_lifecycle(&self.lease_sets, self.ownership)
    }

    /// Returns the current outbound LeaseSet2 the routing layer should
    /// bundle into fresh bound New Session messages: the router-signed
    /// lease set for router-owned destinations, the client-installed
    /// lease set for client-owned destinations. `None` when neither is
    /// installed yet.
    pub fn outbound_lease_set(&self) -> Option<&i2pr_proto::LeaseSet2> {
        match self.ownership {
            DestinationOwnership::RouterOwned => {
                self.lease_sets.current().map(|local| local.lease_set2())
            }
            DestinationOwnership::ClientOwned => self
                .lease_sets
                .client_current()
                .map(|client| client.lease_set2()),
        }
    }

    /// Returns a non-secret handle view over the destination.
    pub fn handle(&self) -> DestinationHandle<'_> {
        DestinationHandle { runtime: self }
    }

    /// Atomically validates, stores, and stages publication for a
    /// client-supplied Standard LeaseSet2 paired with the matching
    /// inbound decryption capability. Router-owned destinations
    /// reject this call.
    ///
    /// `install_client_lease_set2` is the single transaction that
    /// wires a client-owned destination into the inbound ECIES path:
    /// on success the validated LeaseSet2 and the decryption
    /// capability are both installed and the destination becomes
    /// publishable; on failure no state is mutated.
    pub fn install_client_lease_set2(
        &mut self,
        lease_set2: LeaseSet2,
        capability: InboundDecryptionCapability,
        now_seconds: u64,
    ) -> Result<(), LeaseSetError> {
        if !matches!(self.ownership, DestinationOwnership::ClientOwned) {
            return Err(LeaseSetError::InstallWhileStopping);
        }
        if self.state == DestinationState::Stopping || self.state == DestinationState::Stopped {
            return Err(LeaseSetError::InstallWhileStopping);
        }
        // Cross-check the supplied capability public key against the
        // destination's embedded encryption public key BEFORE we
        // even look at the LeaseSet2. Plan 172 §11: legacy ElGamal-slot
        // destinations (every modern reference client ships ElGamal
        // type 0 in the Destination slot even for X25519 LS2 sessions;
        // the slot has been unused since 2005) carry a zeroed static
        // slot, so the check is skipped for them. The X25519-vs-LS2
        // match inside `install_external` remains fail-closed for all
        // destinations. X25519-slot destinations keep the strict match.
        if self.public.encryption_public_key_type() == i2pr_crypto::ROUTER_CRYPTO_KEY_TYPE
            && capability.static_public_bytes() != self.public.static_public_bytes()
        {
            return Err(DestinationIdentityError::DecryptionCapabilityKeyMismatch.into());
        }
        // Validate against the actual inbound pool owned by this
        // destination runtime. Foreign / expired / duplicate leases
        // are rejected before the LeaseSet2 signature is re-checked.
        let leases = self.pool.inbound_lease_sources(now_seconds);
        let capability_public = *capability.static_public_bytes();
        self.lease_sets.install_external(
            &self.public,
            lease_set2,
            &leases,
            &capability_public,
            now_seconds,
        )?;
        self.decryption = Some(capability);
        self.recompute_state(now_seconds);
        Ok(())
    }

    /// Builds the lease request material the Plan 167 daemon sends
    /// to the client when leases approach expiry. The request is
    /// sourced from the destination's real inbound pool; the router
    /// never synthesizes replacement leases for client-owned mode.
    ///
    /// Returns `None` when no refresh is currently pending.
    pub fn take_client_refresh_request(
        &mut self,
        now_seconds: u64,
    ) -> Result<Option<LeaseRequest>, LeaseSetError> {
        if !matches!(self.ownership, DestinationOwnership::ClientOwned) {
            return Ok(None);
        }
        // Drive the client refresh decision so the pending flag is up
        // to date; if leases are still healthy the flag stays false
        // and `take_client_refresh_request` returns `None`.
        let leases = self.pool.inbound_lease_sources(now_seconds);
        let decision = self.lease_sets.refresh_client(&leases, now_seconds);
        if !matches!(
            decision,
            LeaseSetDecision::RequestClientRefresh(_) | LeaseSetDecision::Retain
        ) {
            return Ok(None);
        }
        let request_leases: Vec<LeaseRequestLease> =
            leases.iter().map(LeaseRequestLease::from).collect();
        let hash = self.public.id().as_hash().copy();
        Ok(self
            .lease_sets
            .take_client_refresh_request(hash, request_leases))
    }

    /// Advances the client-owned LeaseSet2 refresh decision without
    /// popping the request. The Plan 167 daemon calls this once per
    /// poll cycle to keep `client_refresh_pending` in sync with
    /// `now_seconds`.
    pub fn poll_client_refresh(&mut self, now_seconds: u64) {
        if !matches!(self.ownership, DestinationOwnership::ClientOwned) {
            return;
        }
        let leases = self.pool.inbound_lease_sources(now_seconds);
        self.lease_sets.refresh_client(&leases, now_seconds);
    }

    /// Admits real one-shot inbound established material into the destination
    /// pool.
    pub fn admit_inbound(
        &mut self,
        established: EstablishedMaterial,
        now_seconds: u64,
    ) -> Result<TunnelSlot, DestinationRuntimeError> {
        self.reject_when_stopping()?;
        let slot = self.pool.register_inbound(established, now_seconds)?;
        self.transition_from_tunnels(now_seconds);
        Ok(slot)
    }

    /// Admits real one-shot outbound established material into the destination
    /// pool.
    pub fn admit_outbound(
        &mut self,
        established: EstablishedMaterial,
        now_seconds: u64,
    ) -> Result<TunnelSlot, DestinationRuntimeError> {
        self.reject_when_stopping()?;
        let slot = self.pool.register_outbound(established, now_seconds)?;
        self.transition_from_tunnels(now_seconds);
        Ok(slot)
    }

    /// Registers the Plan 172 local zero-hop inbound/outbound pair for a
    /// client-owned localhost destination.
    ///
    /// The gateway is `ctx.local_router_hash` (the actual local router
    /// identity hash, never synthesized per session). Both entries share
    /// the supplied bounded lifetime. On success the pool becomes usable
    /// once the minimum inbound count and one outbound route hold.
    pub fn ensure_local_zero_hop(
        &mut self,
        ctx: crate::config::LocalRouterContext,
        inbound_tunnel_id: i2pr_tunnel::TunnelId,
        outbound_route_id: i2pr_tunnel::TunnelId,
        now_seconds: u64,
        lifetime_seconds: u32,
    ) -> Result<(TunnelSlot, TunnelSlot), DestinationRuntimeError> {
        self.reject_when_stopping()?;
        if !matches!(
            self.ownership,
            crate::identity::DestinationOwnership::ClientOwned
        ) {
            return Err(DestinationRuntimeError::Pool(
                DestinationPoolError::Register(
                    "zero-hop routes require a client-owned destination".to_owned(),
                ),
            ));
        }
        let inbound = i2pr_tunnel::LocalZeroHopInbound::new(
            ctx.local_router_hash,
            inbound_tunnel_id,
            now_seconds,
            lifetime_seconds,
        )
        .map_err(|error| {
            DestinationRuntimeError::Pool(DestinationPoolError::Register(error.to_string()))
        })?;
        let outbound = i2pr_tunnel::LocalZeroHopOutbound::new(
            outbound_route_id,
            now_seconds,
            lifetime_seconds,
        )
        .map_err(|error| {
            DestinationRuntimeError::Pool(DestinationPoolError::Register(error.to_string()))
        })?;
        let inbound_slot = self
            .pool
            .register_local_zero_hop_inbound(inbound, now_seconds)?;
        let outbound_slot = match self
            .pool
            .register_local_zero_hop_outbound(outbound, now_seconds)
        {
            Ok(slot) => slot,
            Err(error) => {
                let _ = self.pool.remove(inbound_slot);
                return Err(DestinationRuntimeError::Pool(error));
            }
        };
        self.transition_from_tunnels(now_seconds);
        Ok((inbound_slot, outbound_slot))
    }

    /// Records a bounded tunnel build/replacement failure. A build failure
    /// degrades only this destination; it never panics or fails the router.
    pub fn note_build_failure(&mut self) -> BuildFailureDisposition {
        let disposition = self.pool.note_build_failure();
        if self.state == DestinationState::Usable {
            self.state = DestinationState::Degraded;
        }
        disposition
    }

    /// Returns the currently usable inbound lease sources.
    pub fn inbound_lease_sources(&self, now_seconds: u64) -> Vec<InboundLeaseSource> {
        self.pool.inbound_lease_sources(now_seconds)
    }

    /// Advances the destination's deterministic view of time: expires tunnels,
    /// re-evaluates the LeaseSet2 lifecycle, and recomputes readiness.
    pub fn advance_time(
        &mut self,
        now_seconds: u64,
    ) -> Result<DestinationProgress, DestinationRuntimeError> {
        let evicted = self.pool.advance_time(now_seconds);
        let decision = self.refresh_lease_set(now_seconds)?;
        Ok(DestinationProgress {
            evicted_slots: evicted,
            lease_set_decision: decision,
            state: self.state,
        })
    }

    /// Re-evaluates the LeaseSet2 lifecycle and recomputes readiness.
    pub fn refresh_lease_set(
        &mut self,
        now_seconds: u64,
    ) -> Result<LeaseSetDecision, DestinationRuntimeError> {
        if matches!(
            self.state,
            DestinationState::Stopping | DestinationState::Stopped
        ) {
            return Ok(LeaseSetDecision::Stopping);
        }
        let leases = self.pool.inbound_lease_sources(now_seconds);
        let decision = match self.ownership {
            DestinationOwnership::RouterOwned => {
                let identity = self
                    .identity
                    .as_ref()
                    .expect("router-owned runtime always carries identity");
                self.lease_sets.refresh(identity, &leases, now_seconds)?
            }
            DestinationOwnership::ClientOwned => {
                self.lease_sets.refresh_client(&leases, now_seconds)
            }
        };
        self.recompute_state(now_seconds);
        Ok(decision)
    }

    /// Enqueues an outbound application payload into the bounded queue.
    ///
    /// The payload is accepted for later routing but reported as not routable:
    /// Plan 120 explicitly forbids injecting plaintext into tunnel delivery as
    /// a shortcut around the Plan 121 Garlic session layer.
    pub fn enqueue_outbound(
        &mut self,
        payload: DestinationPayload,
    ) -> Result<QueuedOutbound, PayloadError> {
        if !self.state.accepts_payloads() {
            return Err(PayloadError::Stopping);
        }
        self.outbound.push(payload)?;
        Ok(QueuedOutbound {
            queued_messages: self.outbound.len(),
            queued_bytes: self.outbound.queued_bytes(),
            routing: RoutingUnavailable::AwaitingGarlicSessionLayer,
        })
    }

    /// Accepts a locally delivered inbound payload into the bounded queue.
    pub fn accept_inbound_payload(
        &mut self,
        payload: DestinationPayload,
    ) -> Result<usize, PayloadError> {
        if !self.state.accepts_payloads() {
            return Err(PayloadError::Stopping);
        }
        self.inbound.push(payload)?;
        Ok(self.inbound.len())
    }

    /// Pops the next locally delivered inbound payload.
    pub fn next_inbound_payload(&mut self) -> Option<DestinationPayload> {
        self.inbound.pop()
    }

    /// Number of queued outbound payloads.
    pub fn pending_outbound(&self) -> usize {
        self.outbound.len()
    }

    /// Number of queued inbound payloads.
    pub fn pending_inbound(&self) -> usize {
        self.inbound.len()
    }

    /// Requests graceful shutdown: the LeaseSet2 lifecycle stops generating and
    /// the retained record is dropped immediately.
    pub fn request_shutdown(&mut self) {
        if matches!(self.state, DestinationState::Stopped) {
            return;
        }
        self.lease_sets.begin_stopping();
        self.state = DestinationState::Stopping;
    }

    /// Releases every destination-owned resource and returns the shutdown
    /// accounting. Idempotent.
    pub fn shutdown(&mut self) -> DestinationShutdown {
        self.lease_sets.begin_stopping();
        let released_pool_slots = self.pool.release_all();
        let released_outbound = self.outbound.release_all();
        let released_inbound = self.inbound.release_all();
        // Drop the client-owned inbound decryption capability last so
        // its bytes are zeroized immediately at shutdown.
        self.decryption = None;
        self.state = DestinationState::Stopped;
        DestinationShutdown {
            released_pool_slots,
            released_outbound,
            released_inbound,
        }
    }

    fn reject_when_stopping(&self) -> Result<(), DestinationRuntimeError> {
        if matches!(
            self.state,
            DestinationState::Stopping | DestinationState::Stopped
        ) {
            return Err(DestinationRuntimeError::Stopping);
        }
        Ok(())
    }

    fn transition_from_tunnels(&mut self, now_seconds: u64) {
        if self.state == DestinationState::Initializing {
            self.state = DestinationState::BuildingTunnels;
        }
        self.recompute_state(now_seconds);
    }

    fn recompute_state(&mut self, now_seconds: u64) {
        if matches!(
            self.state,
            DestinationState::Stopping | DestinationState::Stopped
        ) {
            return;
        }
        let tunnels_usable = self.pool.is_usable(now_seconds);
        let lease_set_ready = self.lease_sets.current().is_some();
        self.state = match (tunnels_usable, lease_set_ready) {
            (true, true) => DestinationState::Usable,
            (_, _) if self.pool.is_empty() && self.state == DestinationState::Initializing => {
                DestinationState::Initializing
            }
            (false, _) if self.state == DestinationState::Usable => DestinationState::Degraded,
            (_, _) if self.state == DestinationState::Degraded => DestinationState::Degraded,
            _ => DestinationState::BuildingTunnels,
        };
    }
}

/// Result of advancing a destination's deterministic clock.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestinationProgress {
    /// Pool slots evicted because their tunnels expired.
    pub evicted_slots: Vec<TunnelSlot>,
    /// The LeaseSet2 lifecycle decision that was acted upon.
    pub lease_set_decision: LeaseSetDecision,
    /// The destination state after the advance.
    pub state: DestinationState,
}

/// Shutdown accounting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DestinationShutdown {
    /// Pool slots released.
    pub released_pool_slots: usize,
    /// Pending outbound payloads dropped.
    pub released_outbound: usize,
    /// Pending inbound payloads dropped.
    pub released_inbound: usize,
}

/// Non-secret borrowed view over a destination runtime.
///
/// The handle deliberately exposes no private key reference, no tunnel layer
/// key, and no mutable pool internals.
#[derive(Clone, Copy, Debug)]
pub struct DestinationHandle<'a> {
    runtime: &'a DestinationRuntime,
}

impl DestinationHandle<'_> {
    /// Returns the destination identifier.
    pub fn id(&self) -> DestinationId {
        self.runtime.id()
    }

    /// Returns the destination hash by value.
    pub fn destination_hash(&self) -> Hash {
        self.runtime.destination_hash()
    }

    /// Returns the current lifecycle state.
    pub const fn state(&self) -> DestinationState {
        self.runtime.state()
    }

    /// Returns the shared `i2pr-core` health projection.
    pub const fn health(&self) -> HealthState {
        self.runtime.state().health()
    }

    /// Returns the non-secret LeaseSet2 summary.
    pub fn lease_set_summary(&self) -> LeaseSetSummary {
        self.runtime.lease_set_summary()
    }

    /// Number of queued outbound payloads.
    pub fn pending_outbound(&self) -> usize {
        self.runtime.pending_outbound()
    }

    /// Number of queued inbound payloads.
    pub fn pending_inbound(&self) -> usize {
        self.runtime.pending_inbound()
    }

    /// Number of registered inbound tunnels.
    pub fn inbound_tunnels(&self) -> usize {
        self.runtime.pool().inbound_len()
    }

    /// Number of registered outbound tunnels.
    pub fn outbound_tunnels(&self) -> usize {
        self.runtime.pool().outbound_len()
    }
}

/// Bounded router-local destination registry.
#[derive(Debug)]
pub struct DestinationRegistry {
    config: RegistryConfig,
    destinations: BTreeMap<DestinationId, DestinationRuntime>,
    queued_commands: u32,
}

impl DestinationRegistry {
    /// Constructs an empty bounded registry.
    pub const fn new(config: RegistryConfig) -> Self {
        Self {
            config,
            destinations: BTreeMap::new(),
            queued_commands: 0,
        }
    }

    /// Returns the registry configuration.
    pub const fn config(&self) -> RegistryConfig {
        self.config
    }

    /// Inserts a destination runtime, rejecting duplicates and capacity
    /// overflow.
    pub fn insert(&mut self, runtime: DestinationRuntime) -> Result<DestinationId, RegistryError> {
        let id = runtime.id();
        if self.destinations.contains_key(&id) {
            return Err(RegistryError::DuplicateDestination { id });
        }
        if self.destinations.len() >= usize::from(self.config.max_destinations()) {
            return Err(RegistryError::CapacityExceeded {
                maximum: self.config.max_destinations(),
            });
        }
        self.destinations.insert(id, runtime);
        Ok(id)
    }

    /// Borrows a destination runtime.
    pub fn get(&self, id: &DestinationId) -> Option<&DestinationRuntime> {
        self.destinations.get(id)
    }

    /// Mutably borrows a destination runtime.
    pub fn get_mut(&mut self, id: &DestinationId) -> Option<&mut DestinationRuntime> {
        self.destinations.get_mut(id)
    }

    /// Removes a destination, shutting it down first so every destination-owned
    /// resource is released.
    pub fn remove(&mut self, id: &DestinationId) -> Option<DestinationShutdown> {
        let mut runtime = self.destinations.remove(id)?;
        let shutdown = runtime.shutdown();
        drop(runtime);
        Some(shutdown)
    }

    /// Returns the registered destination identifiers in deterministic order.
    pub fn ids(&self) -> Vec<DestinationId> {
        self.destinations.keys().copied().collect()
    }

    /// Returns non-secret handles for every registered destination.
    pub fn handles(&self) -> Vec<DestinationHandle<'_>> {
        self.destinations
            .values()
            .map(DestinationRuntime::handle)
            .collect()
    }

    /// Number of registered destinations.
    pub fn len(&self) -> usize {
        self.destinations.len()
    }

    /// Whether the registry holds no destinations.
    pub fn is_empty(&self) -> bool {
        self.destinations.is_empty()
    }

    /// Reserves aggregate command-queue depth, rejecting overflow.
    pub fn reserve_command_slot(&mut self) -> Result<u32, RegistryError> {
        let projected = self.queued_commands.saturating_add(1);
        if projected > self.config.max_aggregate_command_queue_depth() {
            return Err(RegistryError::CommandQueueFull {
                maximum: self.config.max_aggregate_command_queue_depth(),
            });
        }
        self.queued_commands = projected;
        Ok(projected)
    }

    /// Releases one reserved aggregate command-queue slot.
    pub fn release_command_slot(&mut self) {
        self.queued_commands = self.queued_commands.saturating_sub(1);
    }

    /// Current aggregate reserved command-queue depth.
    pub const fn queued_commands(&self) -> u32 {
        self.queued_commands
    }
}

/// Typed registry failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RegistryError {
    /// A destination with the same hash is already registered.
    #[error("destination already registered")]
    DuplicateDestination {
        /// The rejected destination identifier.
        id: DestinationId,
    },
    /// The registry destination ceiling was reached.
    #[error("destination registry capacity {maximum} exceeded")]
    CapacityExceeded {
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The aggregate command-queue ceiling was reached.
    #[error("destination registry aggregate command queue depth {maximum} exceeded")]
    CommandQueueFull {
        /// Accepted ceiling.
        maximum: u32,
    },
}

/// Typed destination runtime failures.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DestinationRuntimeError {
    /// The destination is stopping or stopped.
    #[error("destination is stopping and no longer admits tunnels")]
    Stopping,
    /// The destination tunnel pool rejected the operation.
    #[error("destination pool rejected: {0}")]
    Pool(#[from] DestinationPoolError),
    /// The LeaseSet2 lifecycle rejected the operation.
    #[error("destination lease set rejected: {0}")]
    LeaseSet(#[from] LeaseSetError),
    /// The destination identity / public material was rejected.
    #[error("destination identity rejected: {0}")]
    Identity(#[from] DestinationIdentityError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::DestinationIdentity;
    use crate::testing::{established_inbound, established_outbound};
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn runtime(seed: u64) -> DestinationRuntime {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let identity = DestinationIdentity::generate(&mut rng).expect("identity");
        DestinationRuntime::new(identity, DestinationConfig::balanced()).expect("runtime")
    }

    fn usable_runtime(seed: u64, now: u64) -> DestinationRuntime {
        let mut runtime = runtime(seed);
        runtime
            .admit_inbound(established_inbound(seed.wrapping_mul(3) + 1), now)
            .expect("inbound");
        runtime
            .admit_outbound(established_outbound(seed.wrapping_mul(3) + 2), now)
            .expect("outbound");
        runtime.refresh_lease_set(now).expect("refresh");
        runtime
    }

    #[test]
    fn keys_alone_do_not_make_a_destination_usable() {
        let runtime = runtime(1);
        assert_eq!(runtime.state(), DestinationState::Initializing);
        assert!(!runtime.state().is_publishable());
        assert_eq!(runtime.handle().health(), HealthState::Starting);
        assert!(runtime.lease_set().is_none());
    }

    #[test]
    fn real_tunnels_and_lease_set_make_the_destination_usable() {
        let runtime = usable_runtime(2, 1_000);
        assert_eq!(runtime.state(), DestinationState::Usable);
        assert_eq!(runtime.handle().health(), HealthState::Ready);
        assert_eq!(runtime.handle().inbound_tunnels(), 1);
        assert_eq!(runtime.handle().outbound_tunnels(), 1);
        let summary = runtime.lease_set_summary();
        assert!(summary.present);
        assert_eq!(summary.lease_count, 1);
        assert_eq!(summary.generations, 1);
    }

    #[test]
    fn inbound_only_destination_stays_building() {
        let mut runtime = runtime(3);
        runtime
            .admit_inbound(established_inbound(31), 0)
            .expect("inbound");
        assert_eq!(runtime.state(), DestinationState::BuildingTunnels);
        runtime.refresh_lease_set(0).expect("refresh");
        assert_eq!(runtime.state(), DestinationState::BuildingTunnels);
    }

    #[test]
    fn tunnel_expiry_degrades_and_withdraws_the_lease_set() {
        let mut runtime = usable_runtime(4, 0);
        let lifetime = u64::from(DestinationConfig::balanced().tunnel_lifetime_seconds());
        let progress = runtime.advance_time(lifetime).expect("advance");
        assert_eq!(
            progress.lease_set_decision,
            LeaseSetDecision::NotPublishable
        );
        assert_eq!(runtime.state(), DestinationState::Degraded);
        assert!(runtime.lease_set().is_none());
        assert!(!runtime.state().is_publishable());
    }

    #[test]
    fn build_failure_degrades_not_panics_and_is_bounded() {
        let config =
            DestinationConfig::try_new(2, 2, 1, 2, 600, 2, 2, 64, 1024, 60, 120).expect("config");
        let mut rng = ChaCha8Rng::seed_from_u64(5);
        let identity = DestinationIdentity::generate(&mut rng).expect("identity");
        let mut runtime = DestinationRuntime::new(identity, config).expect("runtime");
        runtime
            .admit_inbound(established_inbound(51), 0)
            .expect("inbound");
        runtime
            .admit_outbound(established_outbound(52), 0)
            .expect("outbound");
        runtime.refresh_lease_set(0).expect("refresh");
        assert_eq!(runtime.state(), DestinationState::Usable);
        assert_eq!(
            runtime.note_build_failure(),
            BuildFailureDisposition::RetryPermitted {
                consecutive_failures: 1
            }
        );
        assert_eq!(runtime.state(), DestinationState::Degraded);
        assert_eq!(
            runtime.note_build_failure(),
            BuildFailureDisposition::Exhausted {
                consecutive_failures: 2
            }
        );
        assert!(runtime.pool().replacement_paused());
    }

    #[test]
    fn outbound_payloads_are_queued_but_never_routable() {
        let mut runtime = usable_runtime(6, 1_000);
        let payload = DestinationPayload::new(6, vec![7; 32]).expect("payload");
        let queued = runtime.enqueue_outbound(payload).expect("queued");
        assert_eq!(queued.queued_messages, 1);
        assert_eq!(queued.queued_bytes, 32);
        assert_eq!(
            queued.routing,
            RoutingUnavailable::AwaitingGarlicSessionLayer
        );
        assert_eq!(runtime.pending_outbound(), 1);
    }

    #[test]
    fn shutdown_releases_pool_registry_and_message_state() {
        let mut runtime = usable_runtime(7, 1_000);
        runtime
            .enqueue_outbound(DestinationPayload::new(6, vec![1; 8]).expect("payload"))
            .expect("queued");
        runtime
            .accept_inbound_payload(DestinationPayload::new(6, vec![2; 8]).expect("payload"))
            .expect("queued");
        runtime.request_shutdown();
        assert_eq!(runtime.state(), DestinationState::Stopping);
        assert!(runtime.lease_set().is_none());
        assert!(matches!(
            runtime.admit_inbound(established_inbound(71), 1_000),
            Err(DestinationRuntimeError::Stopping)
        ));
        assert_eq!(
            runtime.enqueue_outbound(DestinationPayload::new(6, vec![3; 8]).expect("payload")),
            Err(PayloadError::Stopping)
        );
        let shutdown = runtime.shutdown();
        assert_eq!(shutdown.released_pool_slots, 2);
        assert_eq!(shutdown.released_outbound, 1);
        assert_eq!(shutdown.released_inbound, 1);
        assert_eq!(runtime.state(), DestinationState::Stopped);
        assert!(runtime.pool().is_empty());
        // Idempotent.
        let again = runtime.shutdown();
        assert_eq!(again.released_pool_slots, 0);
    }

    #[test]
    fn registry_is_bounded_and_rejects_duplicates() {
        let config = RegistryConfig::try_new(2, 4).expect("config");
        let mut registry = DestinationRegistry::new(config);
        let first = registry.insert(runtime(8)).expect("first");
        let second = registry.insert(runtime(9)).expect("second");
        assert_ne!(first, second);
        assert_eq!(registry.len(), 2);
        assert!(matches!(
            registry.insert(runtime(10)),
            Err(RegistryError::CapacityExceeded { maximum: 2 })
        ));
        let mut expected = [first, second];
        expected.sort();
        assert_eq!(registry.ids(), expected);
    }

    #[test]
    fn duplicate_destination_hash_is_rejected() {
        let mut registry = DestinationRegistry::new(RegistryConfig::default());
        let id = registry.insert(runtime(11)).expect("first");
        // The same deterministic seed reproduces the same destination hash.
        let error = registry.insert(runtime(11)).expect_err("duplicate");
        assert_eq!(error, RegistryError::DuplicateDestination { id });
    }

    #[test]
    fn registry_removal_drops_destination_state() {
        let mut registry = DestinationRegistry::new(RegistryConfig::default());
        let mut runtime = runtime(12);
        runtime
            .admit_inbound(established_inbound(121), 0)
            .expect("inbound");
        runtime
            .admit_outbound(established_outbound(122), 0)
            .expect("outbound");
        let id = registry.insert(runtime).expect("insert");
        let shutdown = registry.remove(&id).expect("removed");
        assert_eq!(shutdown.released_pool_slots, 2);
        assert!(registry.get(&id).is_none());
        assert!(registry.is_empty());
        assert!(registry.remove(&id).is_none());
    }

    #[test]
    fn one_destination_failure_does_not_mutate_the_other() {
        let mut registry = DestinationRegistry::new(RegistryConfig::default());
        let first = registry.insert(usable_runtime(13, 1_000)).expect("first");
        let second = registry.insert(usable_runtime(14, 1_000)).expect("second");
        assert_ne!(first, second);
        let first_summary = registry.get(&first).expect("first").lease_set_summary();
        let second_summary = registry.get(&second).expect("second").lease_set_summary();
        assert!(first_summary.present && second_summary.present);

        registry
            .get_mut(&second)
            .expect("second")
            .note_build_failure();
        registry
            .get_mut(&second)
            .expect("second")
            .request_shutdown();

        let first_after = registry.get(&first).expect("first");
        assert_eq!(first_after.state(), DestinationState::Usable);
        assert_eq!(first_after.lease_set_summary(), first_summary);
        assert_eq!(
            registry.get(&second).expect("second").state(),
            DestinationState::Stopping
        );
    }

    #[test]
    fn aggregate_command_queue_depth_is_bounded() {
        let mut registry = DestinationRegistry::new(RegistryConfig::try_new(2, 2).expect("config"));
        assert_eq!(registry.reserve_command_slot().expect("first"), 1);
        assert_eq!(registry.reserve_command_slot().expect("second"), 2);
        assert!(matches!(
            registry.reserve_command_slot(),
            Err(RegistryError::CommandQueueFull { maximum: 2 })
        ));
        registry.release_command_slot();
        assert_eq!(registry.queued_commands(), 1);
    }

    #[test]
    fn command_and_event_vocabularies_are_bounded() {
        let payload = DestinationPayload::new(6, vec![1; 4]).expect("payload");
        let command = DestinationCommand::EnqueueOutbound(payload);
        assert!(matches!(command, DestinationCommand::EnqueueOutbound(_)));
        let event = DestinationEvent::StateChanged {
            previous: DestinationState::BuildingTunnels,
            current: DestinationState::Usable,
        };
        assert!(matches!(event, DestinationEvent::StateChanged { .. }));
        assert_eq!(DestinationState::Stopping.health(), HealthState::Stopping);
    }
}
