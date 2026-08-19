//! Local Standard LeaseSet2 construction, signing, and lifecycle.
//!
//! Plan 120 §7/§8/§9 owns the client-side LeaseSet2 path. The destination
//! signing private key never leaves `i2pr-client`: this module builds the
//! canonical unsigned Plan 119 LeaseSet2, signs the `0x03 || signed_bytes`
//! preimage with the destination key, and then self-validates the finalized
//! record through the same `i2pr-netdb` validation used for received entries.

use core::fmt;

use i2pr_netdb::{
    DestinationHash, LeaseSet2ValidationContext, LeaseSet2ValidationError, ValidatedLeaseSet2,
};
use i2pr_proto::{
    CodecError, Date32, Hash, LEASE_SET2_SIGNATURE_DOMAIN_BYTE, Lease2, LeaseSet2,
    LeaseSet2BuildError, LeaseSet2EncryptionKey, LeaseSet2Flags, LeaseSet2Header,
    LeaseSet2HeaderError, Mapping, SignatureValue,
};

use crate::config::DestinationConfig;
use crate::identity::{DestinationIdentity, DestinationIdentityError};
use crate::pool::InboundLeaseSource;

/// Signature domain byte prepended to the LeaseSet2 signature preimage.
pub const LEASE_SET2_SIGNATURE_DOMAIN: u8 = LEASE_SET2_SIGNATURE_DOMAIN_BYTE;

/// A finalized, signed, self-validated local LeaseSet2 together with the
/// non-secret metadata used to decide when it must be replaced.
#[derive(Debug)]
pub struct LocalLeaseSet {
    validated: ValidatedLeaseSet2,
    published_seconds: u32,
    leases: Vec<InboundLeaseSource>,
}

impl LocalLeaseSet {
    /// Borrows the validated LeaseSet2.
    pub const fn validated(&self) -> &ValidatedLeaseSet2 {
        &self.validated
    }

    /// Borrows the signed LeaseSet2 record.
    pub const fn lease_set2(&self) -> &LeaseSet2 {
        self.validated.lease_set2()
    }

    /// Returns the NetDB destination key this LeaseSet2 is stored under.
    pub const fn key(&self) -> DestinationHash {
        self.validated.key()
    }

    /// Returns the `published` timestamp in seconds.
    pub const fn published_seconds(&self) -> u32 {
        self.published_seconds
    }

    /// Returns the record's absolute expiry in seconds.
    pub fn expires_seconds(&self) -> u32 {
        self.validated.lease_set2().expires_seconds()
    }

    /// Returns the pool sources that produced the advertised leases.
    pub fn lease_sources(&self) -> &[InboundLeaseSource] {
        &self.leases
    }

    /// Returns the earliest advertised lease end date in seconds.
    pub fn earliest_lease_expiry_seconds(&self) -> u64 {
        self.leases
            .iter()
            .map(InboundLeaseSource::advertised_expires_seconds)
            .min()
            .unwrap_or(0)
    }
}

/// Reason a local LeaseSet2 must be (re)generated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseSetRotationCause {
    /// The destination has no LeaseSet2 yet and the first usable inbound set
    /// is available.
    InitialGeneration,
    /// The usable inbound tunnel set changed.
    TunnelSetChanged,
    /// The earliest advertised lease is inside the configured rotation margin.
    ApproachingExpiry,
}

/// Outcome of evaluating the local LeaseSet2 lifecycle against the current
/// usable inbound tunnel set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseSetDecision {
    /// The destination has no usable inbound tunnels and is therefore not
    /// publishable. Any previously generated LeaseSet2 must not be treated as
    /// healthy.
    NotPublishable,
    /// The destination is stopping; no new LeaseSet2 may be generated.
    Stopping,
    /// The current LeaseSet2 remains valid; do not regenerate.
    Retain,
    /// A replacement LeaseSet2 is required for the stated reason.
    Regenerate(LeaseSetRotationCause),
}

/// Bounded local LeaseSet2 lifecycle owner.
///
/// The lifecycle never reads a wall clock: callers supply a deterministic
/// `now_seconds`.
#[derive(Debug)]
pub struct LeaseSetLifecycle {
    config: DestinationConfig,
    current: Option<LocalLeaseSet>,
    last_published_seconds: Option<u32>,
    generations: u32,
    publication_pending: bool,
    stopping: bool,
}

impl LeaseSetLifecycle {
    /// Constructs an empty lifecycle for the supplied destination policy.
    pub const fn new(config: DestinationConfig) -> Self {
        Self {
            config,
            current: None,
            last_published_seconds: None,
            generations: 0,
            publication_pending: false,
            stopping: false,
        }
    }

    /// Borrows the current LeaseSet2, when one has been generated.
    pub const fn current(&self) -> Option<&LocalLeaseSet> {
        self.current.as_ref()
    }

    /// Number of LeaseSet2 versions generated so far.
    pub const fn generations(&self) -> u32 {
        self.generations
    }

    /// Whether a publication has been requested and not yet acknowledged.
    /// Plan 122 owns the network composition that clears this flag.
    pub const fn publication_pending(&self) -> bool {
        self.publication_pending
    }

    /// Marks the pending publication acknowledged.
    pub fn acknowledge_publication(&mut self) {
        self.publication_pending = false;
    }

    /// Marks the lifecycle as stopping. No further LeaseSet2 is generated and
    /// the retained record is dropped so a stale lease set can never be
    /// advertised as healthy.
    pub fn begin_stopping(&mut self) {
        self.stopping = true;
        self.publication_pending = false;
        self.current = None;
    }

    /// Whether the lifecycle is stopping.
    pub const fn is_stopping(&self) -> bool {
        self.stopping
    }

    /// Evaluates whether a replacement LeaseSet2 is required.
    pub fn evaluate(&self, leases: &[InboundLeaseSource], now_seconds: u64) -> LeaseSetDecision {
        if self.stopping {
            return LeaseSetDecision::Stopping;
        }
        if leases.len() < usize::from(self.config.minimum_usable_inbound()) {
            return LeaseSetDecision::NotPublishable;
        }
        let Some(current) = self.current.as_ref() else {
            return LeaseSetDecision::Regenerate(LeaseSetRotationCause::InitialGeneration);
        };
        if current.leases.as_slice() != leases {
            return LeaseSetDecision::Regenerate(LeaseSetRotationCause::TunnelSetChanged);
        }
        let rotation_margin = u64::from(self.config.lease_rotation_margin_seconds());
        if current
            .earliest_lease_expiry_seconds()
            .saturating_sub(rotation_margin)
            <= now_seconds
        {
            return LeaseSetDecision::Regenerate(LeaseSetRotationCause::ApproachingExpiry);
        }
        LeaseSetDecision::Retain
    }

    /// Evaluates and, when required, generates a replacement LeaseSet2.
    ///
    /// Returns the decision that was acted upon. A `NotPublishable` decision
    /// clears any retained record.
    pub fn refresh(
        &mut self,
        identity: &DestinationIdentity,
        leases: &[InboundLeaseSource],
        now_seconds: u64,
    ) -> Result<LeaseSetDecision, LeaseSetError> {
        let decision = self.evaluate(leases, now_seconds);
        match decision {
            LeaseSetDecision::Stopping => {}
            LeaseSetDecision::NotPublishable => {
                self.current = None;
                self.publication_pending = false;
            }
            LeaseSetDecision::Retain => {}
            LeaseSetDecision::Regenerate(_) => {
                let generated = self.generate(identity, leases, now_seconds)?;
                self.current = Some(generated);
                self.publication_pending = true;
                self.generations = self.generations.saturating_add(1);
            }
        }
        Ok(decision)
    }

    fn generate(
        &mut self,
        identity: &DestinationIdentity,
        leases: &[InboundLeaseSource],
        now_seconds: u64,
    ) -> Result<LocalLeaseSet, LeaseSetError> {
        if leases.is_empty() {
            return Err(LeaseSetError::NoUsableInboundTunnels);
        }
        let now = u32::try_from(now_seconds).map_err(|_| LeaseSetError::TimestampOverflow)?;
        // Plan 120 §9: `published` has one-second resolution, so a replacement
        // generated inside the same second must still advance or NetDB
        // replacement would be refused.
        let published = match self.last_published_seconds {
            Some(previous) if previous >= now => previous
                .checked_add(1)
                .ok_or(LeaseSetError::TimestampOverflow)?,
            _ => now,
        };
        let signed = build_signed_lease_set2(identity, leases, published)?;
        let expected = identity.id().as_netdb_key();
        let context = LeaseSet2ValidationContext::new(now);
        let validated = ValidatedLeaseSet2::from_lease_set2(signed, Some(expected), context)?;
        self.last_published_seconds = Some(published);
        Ok(LocalLeaseSet {
            validated,
            published_seconds: published,
            leases: leases.to_vec(),
        })
    }
}

/// Builds and signs a canonical Standard LeaseSet2 for the supplied
/// destination and inbound lease sources.
///
/// The advertised expiry offset is derived from the latest advertised lease so
/// the record never outlives the destination's own tunnels.
pub fn build_signed_lease_set2(
    identity: &DestinationIdentity,
    leases: &[InboundLeaseSource],
    published_seconds: u32,
) -> Result<LeaseSet2, LeaseSetError> {
    if leases.is_empty() {
        return Err(LeaseSetError::NoUsableInboundTunnels);
    }
    let mut lease2 = Vec::with_capacity(leases.len());
    let mut latest_expiry = published_seconds;
    for source in leases {
        let end = u32::try_from(source.advertised_expires_seconds())
            .map_err(|_| LeaseSetError::TimestampOverflow)?;
        if end <= published_seconds {
            return Err(LeaseSetError::LeaseAlreadyExpired {
                published_seconds,
                lease_end_seconds: end,
            });
        }
        if source.advertised_expires_seconds() > source.tunnel_expires_seconds() {
            return Err(LeaseSetError::LeaseOutlivesTunnel {
                lease_end_seconds: source.advertised_expires_seconds(),
                tunnel_end_seconds: source.tunnel_expires_seconds(),
            });
        }
        latest_expiry = latest_expiry.max(end);
        lease2.push(Lease2::new(
            source.gateway(),
            source.gateway_receive_tunnel_id(),
            Date32::from_seconds(end),
        ));
    }
    let offset = u16::try_from(latest_expiry.saturating_sub(published_seconds))
        .map_err(|_| LeaseSetError::ExpirationOffsetOverflow)?;
    let encryption_keys = vec![
        LeaseSet2EncryptionKey::new(
            i2pr_crypto::ROUTER_CRYPTO_KEY_TYPE,
            identity.static_public_bytes().to_vec(),
        )
        .map_err(|_| LeaseSetError::EmptyEncryptionKey)?,
    ];
    let placeholder = SignatureValue::new(
        i2pr_crypto::ROUTER_SIGNING_KEY_TYPE,
        vec![0_u8; i2pr_crypto::SIGNATURE_LENGTH],
    )?;
    let header = LeaseSet2Header::new(
        identity.destination().clone(),
        published_seconds,
        offset,
        LeaseSet2Flags::from_raw(0),
    )?;
    let unsigned = LeaseSet2::new(
        header,
        Mapping::empty(),
        encryption_keys.clone(),
        lease2.clone(),
        placeholder,
    )?;
    let signature = identity.sign(&unsigned.signature_preimage())?;
    let header = LeaseSet2Header::new(
        identity.destination().clone(),
        published_seconds,
        offset,
        LeaseSet2Flags::from_raw(0),
    )?;
    Ok(LeaseSet2::new(
        header,
        Mapping::empty(),
        encryption_keys,
        lease2,
        signature,
    )?)
}

/// Typed local LeaseSet2 construction failures.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LeaseSetError {
    /// No usable inbound tunnel is available, so no lease can be advertised.
    #[error("destination has no usable inbound tunnels")]
    NoUsableInboundTunnels,
    /// A supplied lease had already expired relative to the published time.
    #[error("lease end {lease_end_seconds} is not after published {published_seconds}")]
    LeaseAlreadyExpired {
        /// The record's published timestamp.
        published_seconds: u32,
        /// The rejected lease end date.
        lease_end_seconds: u32,
    },
    /// A supplied lease would outlive its tunnel.
    #[error("lease end {lease_end_seconds} outlives tunnel end {tunnel_end_seconds}")]
    LeaseOutlivesTunnel {
        /// The rejected lease end date.
        lease_end_seconds: u64,
        /// The tunnel's real usability deadline.
        tunnel_end_seconds: u64,
    },
    /// A timestamp did not fit the LeaseSet2 32-bit second field.
    #[error("lease set timestamp does not fit the 32-bit seconds field")]
    TimestampOverflow,
    /// The derived expiration offset did not fit the 16-bit field.
    #[error("lease set expiration offset does not fit the 16-bit field")]
    ExpirationOffsetOverflow,
    /// The destination static X25519 public key was empty.
    #[error("destination static encryption key was empty")]
    EmptyEncryptionKey,
    /// The LeaseSet2 header rejected the published/expires pair.
    #[error("lease set header rejected: {0}")]
    Header(#[from] LeaseSet2HeaderError),
    /// The LeaseSet2 structure was rejected.
    #[error("lease set structure rejected: {0}")]
    Build(#[from] LeaseSet2BuildError),
    /// A common-structure codec rejected the record.
    #[error("lease set codec rejected: {0}")]
    Codec(#[from] CodecError),
    /// The destination signing operation failed.
    #[error("destination signing failed: {0}")]
    Identity(#[from] DestinationIdentityError),
    /// The finalized record failed the same validation used for received
    /// LeaseSet2 entries.
    #[error("local lease set failed self-validation: {0}")]
    SelfValidation(#[from] LeaseSet2ValidationError),
}

/// Non-secret summary of the local LeaseSet2 state, suitable for a handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeaseSetSummary {
    /// Whether a signed, self-validated LeaseSet2 is currently held.
    pub present: bool,
    /// Number of advertised leases in the current record.
    pub lease_count: usize,
    /// The current record's published timestamp.
    pub published_seconds: Option<u32>,
    /// The current record's absolute expiry.
    pub expires_seconds: Option<u32>,
    /// Number of generated versions.
    pub generations: u32,
    /// Whether a publication is pending.
    pub publication_pending: bool,
}

impl LeaseSetSummary {
    pub(crate) fn from_lifecycle(lifecycle: &LeaseSetLifecycle) -> Self {
        match lifecycle.current() {
            Some(current) => Self {
                present: true,
                lease_count: current.lease_sources().len(),
                published_seconds: Some(current.published_seconds()),
                expires_seconds: Some(current.expires_seconds()),
                generations: lifecycle.generations(),
                publication_pending: lifecycle.publication_pending(),
            },
            None => Self {
                present: false,
                lease_count: 0,
                published_seconds: None,
                expires_seconds: None,
                generations: lifecycle.generations(),
                publication_pending: lifecycle.publication_pending(),
            },
        }
    }
}

impl fmt::Display for LeaseSetSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "lease-set present={} leases={} generations={}",
            self.present, self.lease_count, self.generations
        )
    }
}

/// Returns the SHA-256 hash of the encoded LeaseSet2 record, for diagnostics.
pub fn encoded_hash(record: &LeaseSet2) -> Result<Hash, CodecError> {
    let encoded = record.encode_to_vec(i2pr_proto::MAX_LEASE_SET2_BYTES)?;
    Ok(i2pr_crypto::sha256(&encoded))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DestinationConfig;
    use crate::identity::DestinationIdentity;
    use crate::pool::DestinationTunnelPool;
    use crate::testing::{established_inbound, established_outbound};
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn identity(seed: u64) -> DestinationIdentity {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        DestinationIdentity::generate(&mut rng).expect("identity")
    }

    fn pool_with_inbound(seeds: &[u64], now: u64) -> DestinationTunnelPool {
        let mut pool = DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool");
        for seed in seeds {
            pool.register_inbound(established_inbound(*seed), now)
                .expect("inbound");
        }
        pool.register_outbound(established_outbound(9_000), now)
            .expect("outbound");
        pool
    }

    #[test]
    fn first_usable_inbound_set_generates_and_self_validates_ls2() {
        let identity = identity(101);
        let pool = pool_with_inbound(&[1], 1_000);
        let leases = pool.inbound_lease_sources(1_000);
        let mut lifecycle = LeaseSetLifecycle::new(DestinationConfig::balanced());
        let decision = lifecycle
            .refresh(&identity, &leases, 1_000)
            .expect("refresh");
        assert_eq!(
            decision,
            LeaseSetDecision::Regenerate(LeaseSetRotationCause::InitialGeneration)
        );
        let current = lifecycle.current().expect("record");
        assert_eq!(current.key(), identity.id().as_netdb_key());
        assert_eq!(current.lease_set2().leases().len(), 1);
        assert_eq!(
            current.lease_set2().leases()[0].tunnel_gateway(),
            leases[0].gateway()
        );
        assert_eq!(
            current.lease_set2().leases()[0].tunnel_id(),
            leases[0].gateway_receive_tunnel_id()
        );
        let key = current
            .lease_set2()
            .usable_x25519_key()
            .expect("x25519 key");
        assert_eq!(key.as_bytes(), &identity.static_public_bytes()[..]);
        assert_eq!(key.key_type().code(), 4);
        i2pr_crypto::verify_lease_set2(current.lease_set2()).expect("signature verifies");
        assert!(lifecycle.publication_pending());
    }

    #[test]
    fn same_state_does_not_regenerate() {
        let identity = identity(102);
        let pool = pool_with_inbound(&[2], 1_000);
        let leases = pool.inbound_lease_sources(1_000);
        let mut lifecycle = LeaseSetLifecycle::new(DestinationConfig::balanced());
        lifecycle
            .refresh(&identity, &leases, 1_000)
            .expect("initial");
        for _ in 0..8 {
            assert_eq!(
                lifecycle
                    .refresh(&identity, &leases, 1_001)
                    .expect("refresh"),
                LeaseSetDecision::Retain
            );
        }
        assert_eq!(lifecycle.generations(), 1);
    }

    #[test]
    fn replacement_has_monotonic_published_time() {
        let identity = identity(103);
        let first_pool = pool_with_inbound(&[3], 1_000);
        let second_pool = pool_with_inbound(&[4], 1_000);
        let mut lifecycle = LeaseSetLifecycle::new(DestinationConfig::balanced());
        lifecycle
            .refresh(&identity, &first_pool.inbound_lease_sources(1_000), 1_000)
            .expect("initial");
        let first_published = lifecycle.current().expect("record").published_seconds();
        // Same second, changed tunnel set: `published` must still advance.
        let decision = lifecycle
            .refresh(&identity, &second_pool.inbound_lease_sources(1_000), 1_000)
            .expect("replacement");
        assert_eq!(
            decision,
            LeaseSetDecision::Regenerate(LeaseSetRotationCause::TunnelSetChanged)
        );
        let second_published = lifecycle.current().expect("record").published_seconds();
        assert!(second_published > first_published);
        assert_eq!(lifecycle.generations(), 2);
    }

    #[test]
    fn approaching_expiry_rotates_the_lease_set() {
        let identity = identity(104);
        let pool = pool_with_inbound(&[5], 0);
        let leases = pool.inbound_lease_sources(0);
        let mut lifecycle = LeaseSetLifecycle::new(DestinationConfig::balanced());
        lifecycle.refresh(&identity, &leases, 0).expect("initial");
        let earliest = lifecycle
            .current()
            .expect("record")
            .earliest_lease_expiry_seconds();
        let margin = u64::from(DestinationConfig::balanced().lease_rotation_margin_seconds());
        assert_eq!(
            lifecycle.evaluate(&leases, earliest - margin - 1),
            LeaseSetDecision::Retain
        );
        assert_eq!(
            lifecycle.evaluate(&leases, earliest - margin),
            LeaseSetDecision::Regenerate(LeaseSetRotationCause::ApproachingExpiry)
        );
    }

    #[test]
    fn zero_usable_inbound_tunnels_is_not_publishable() {
        let identity = identity(105);
        let mut lifecycle = LeaseSetLifecycle::new(DestinationConfig::balanced());
        assert_eq!(
            lifecycle.refresh(&identity, &[], 1_000).expect("refresh"),
            LeaseSetDecision::NotPublishable
        );
        assert!(lifecycle.current().is_none());
        let pool = pool_with_inbound(&[6], 1_000);
        lifecycle
            .refresh(&identity, &pool.inbound_lease_sources(1_000), 1_000)
            .expect("initial");
        assert!(lifecycle.current().is_some());
        // Losing every tunnel clears the retained record; a stale lease set is
        // never advertised as healthy.
        assert_eq!(
            lifecycle.refresh(&identity, &[], 1_100).expect("refresh"),
            LeaseSetDecision::NotPublishable
        );
        assert!(lifecycle.current().is_none());
        assert!(!lifecycle.publication_pending());
    }

    #[test]
    fn stopping_lifecycle_never_generates() {
        let identity = identity(106);
        let pool = pool_with_inbound(&[7], 1_000);
        let leases = pool.inbound_lease_sources(1_000);
        let mut lifecycle = LeaseSetLifecycle::new(DestinationConfig::balanced());
        lifecycle
            .refresh(&identity, &leases, 1_000)
            .expect("initial");
        lifecycle.begin_stopping();
        assert!(lifecycle.current().is_none());
        assert_eq!(
            lifecycle.refresh(&identity, &leases, 1_001).expect("stop"),
            LeaseSetDecision::Stopping
        );
        assert!(lifecycle.current().is_none());
        assert_eq!(lifecycle.generations(), 1);
    }

    #[test]
    fn expired_lease_is_rejected_by_the_builder() {
        let identity = identity(107);
        let pool = pool_with_inbound(&[8], 0);
        let leases = pool.inbound_lease_sources(0);
        let end = u32::try_from(leases[0].advertised_expires_seconds()).expect("fits");
        let error = build_signed_lease_set2(&identity, &leases, end).expect_err("rejected");
        assert!(matches!(error, LeaseSetError::LeaseAlreadyExpired { .. }));
    }

    #[test]
    fn signature_preimage_uses_the_lease_set2_domain_byte() {
        let identity = identity(108);
        let pool = pool_with_inbound(&[9], 1_000);
        let record = build_signed_lease_set2(&identity, &pool.inbound_lease_sources(1_000), 1_000)
            .expect("record");
        let preimage = record.signature_preimage();
        assert_eq!(preimage[0], LEASE_SET2_SIGNATURE_DOMAIN);
        assert_eq!(&preimage[1..], record.signed_bytes());
        assert!(encoded_hash(&record).is_ok());
    }

    #[test]
    fn foreign_key_expectation_is_rejected_by_self_validation() {
        let local = identity(109);
        let other = identity(110);
        let pool = pool_with_inbound(&[10], 1_000);
        let record = build_signed_lease_set2(&local, &pool.inbound_lease_sources(1_000), 1_000)
            .expect("record");
        let error = ValidatedLeaseSet2::from_lease_set2(
            record,
            Some(other.id().as_netdb_key()),
            LeaseSet2ValidationContext::new(1_000),
        )
        .expect_err("mismatch");
        assert!(matches!(
            error,
            LeaseSet2ValidationError::DestinationMismatch
        ));
    }
}
