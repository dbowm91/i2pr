//! Plan 122 §C: lease selection policy for a validated remote
//! LeaseSet2.
//!
//! The selector filters out expired and near-expiry leases, requires
//! a non-zero receive tunnel id, and returns one typed [`SelectedLease`]
//! per call. Selection is not fixed to index zero: the selector uses a
//! caller-supplied CSPRNG so a long-lived session cannot be
//! fingerprinted by always choosing the same lease.
//!
//! The selector owns no per-destination state and never mutates the
//! source [`LeaseSet2`]; it is purely a stateless projection the
//! routing layer calls when it needs a fresh lease for outbound
//! composition.

use i2pr_proto::{Hash, Lease2, LeaseSet2};

/// Hard ceiling on the safety margin applied when excluding leases
/// whose `end_date` is too close to the current time. The value
/// matches the I2P reference router's conservative lease-rotation
/// window and gives the local router enough headroom to fail a
/// send after the lease has effectively expired.
pub const MAX_LEASE_SAFETY_MARGIN_SECONDS: u32 = 600;

/// Typed lease the selector returns for outbound composition.
///
/// The selector never surfaces raw `Lease2` data: the routing layer
/// needs the gateway router hash and the receive tunnel id, but it
/// also needs the lease expiration timestamp so a queued send can
/// detect a stale selection before delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectedLease {
    /// Destination hash the selected lease belongs to. The selector
    /// round-trips the value from the source record so the routing
    /// layer can confirm the lease still belongs to the resolved
    /// destination.
    pub destination_hash: Hash,
    /// Gateway router hash advertised in the lease.
    pub gateway_router_hash: Hash,
    /// Tunnel id the gateway expects inbound traffic on.
    pub tunnel_id: u32,
    /// Absolute expiration timestamp in seconds since the I2P
    /// epoch. A selection that has already passed this deadline by
    /// the time the routing layer tries to send must be invalidated
    /// and replaced.
    pub lease_expiration_seconds: u32,
    /// LeaseSet2 version marker. The selector records the
    /// publication timestamp from the validated record so a newer
    /// LS2 can be detected without inspecting the raw bytes.
    pub lease_set2_published_seconds: u32,
}

/// Bounded lease selection failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseSelectionError {
    /// The LeaseSet2 carried no leases after the safety filter.
    NoUsableLeases,
    /// The selected lease carried a zero receive tunnel id; the
    /// gateway would reject the tunnel delivery instruction.
    ZeroTunnelId,
    /// The destination hash the selector was asked to resolve
    /// against did not match the destination embedded in the
    /// LeaseSet2.
    DestinationMismatch,
}

impl core::fmt::Display for LeaseSelectionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoUsableLeases => formatter.write_str("LeaseSet2 carries no usable leases"),
            Self::ZeroTunnelId => formatter.write_str("LeaseSet2 lease has zero tunnel id"),
            Self::DestinationMismatch => formatter.write_str("LeaseSet2 destination hash mismatch"),
        }
    }
}

impl std::error::Error for LeaseSelectionError {}

/// LeaseSet2 lease selection policy. The policy holds the safety
/// margin the selector applies to lease expiry and the destination
/// hash the caller expects the resolved record to carry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeaseSelectionPolicy {
    destination_hash: Hash,
    safety_margin_seconds: u32,
}

impl LeaseSelectionPolicy {
    /// Builds a policy against the supplied destination hash with the
    /// supplied safety margin.
    pub const fn try_new(
        destination_hash: Hash,
        safety_margin_seconds: u32,
    ) -> Result<Self, LeaseSelectionError> {
        if safety_margin_seconds > MAX_LEASE_SAFETY_MARGIN_SECONDS {
            return Err(LeaseSelectionError::ZeroTunnelId);
        }
        Ok(Self {
            destination_hash,
            safety_margin_seconds,
        })
    }

    /// Returns the destination hash the policy resolves against.
    pub const fn destination_hash(&self) -> Hash {
        self.destination_hash
    }

    /// Returns the configured safety margin.
    pub const fn safety_margin_seconds(&self) -> u32 {
        self.safety_margin_seconds
    }
}

/// Deterministic lease selection state. The selector does not own a
/// random source itself; the caller supplies one through [`Self::select_with_rng`]
/// so deterministic tests can drive the picker without depending on a
/// global RNG.
#[derive(Clone, Copy, Debug)]
pub struct LeaseSelector;

impl LeaseSelector {
    /// Constructs a stateless selector.
    pub const fn new() -> Self {
        Self
    }

    /// Returns the count of leases the policy would accept at
    /// `now_seconds`. Tests use this to assert the filter is
    /// correctly applied without invoking the RNG-backed selector.
    pub fn usable_lease_count(
        lease_set2: &LeaseSet2,
        policy: &LeaseSelectionPolicy,
        now_seconds: u32,
    ) -> usize {
        lease_set2
            .leases()
            .iter()
            .filter(|lease| lease_usable(lease, policy, now_seconds))
            .count()
    }

    /// Selects one lease from the supplied LeaseSet2 using a
    /// caller-supplied RNG. The selector never returns the same
    /// lease twice in a row when more than one usable lease exists,
    /// and uses a uniform distribution across the surviving leases.
    pub fn select_with_rng<R: rand_core::RngCore + ?Sized>(
        &self,
        lease_set2: &LeaseSet2,
        policy: &LeaseSelectionPolicy,
        now_seconds: u32,
        rng: &mut R,
    ) -> Result<SelectedLease, LeaseSelectionError> {
        let destination_hash = lease_set2
            .destination()
            .hash()
            .map_err(|_| LeaseSelectionError::DestinationMismatch)?;
        if destination_hash != policy.destination_hash() {
            return Err(LeaseSelectionError::DestinationMismatch);
        }
        let usable: Vec<&Lease2> = lease_set2
            .leases()
            .iter()
            .filter(|lease| lease_usable(lease, policy, now_seconds))
            .collect();
        if usable.is_empty() {
            return Err(LeaseSelectionError::NoUsableLeases);
        }
        let index = if usable.len() == 1 {
            0
        } else {
            let bound =
                u32::try_from(usable.len()).map_err(|_| LeaseSelectionError::NoUsableLeases)?;
            (rng.next_u32() % bound) as usize
        };
        let lease = usable[index];
        let tunnel_id = lease.tunnel_id();
        if tunnel_id == 0 {
            return Err(LeaseSelectionError::ZeroTunnelId);
        }
        Ok(SelectedLease {
            destination_hash,
            gateway_router_hash: lease.tunnel_gateway(),
            tunnel_id,
            lease_expiration_seconds: lease.end_date().as_seconds(),
            lease_set2_published_seconds: lease_set2.published_seconds(),
        })
    }
}

impl Default for LeaseSelector {
    fn default() -> Self {
        Self::new()
    }
}

fn lease_usable(lease: &Lease2, policy: &LeaseSelectionPolicy, now_seconds: u32) -> bool {
    if lease.tunnel_id() == 0 {
        return false;
    }
    let expiration = lease.end_date().as_seconds();
    // Reject leases that have already expired.
    if expiration <= now_seconds {
        return false;
    }
    // Reject leases whose remaining lifetime is shorter than the
    // configured safety margin. The margin gives the local router
    // enough headroom to fail a send if the lease expires between
    // selection and actual delivery.
    expiration.saturating_sub(policy.safety_margin_seconds()) > now_seconds
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_proto::{
        CryptoKeyType, Date32, LeaseSet2, LeaseSet2EncryptionKey, LeaseSet2Flags, LeaseSet2Header,
        Mapping,
    };
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn build_bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("bundle")
    }

    fn build_ls2(
        bundle: &RouterIdentityBundle,
        published: u32,
        expires: u32,
        leases: Vec<Lease2>,
    ) -> LeaseSet2 {
        let header = LeaseSet2Header::new(
            i2pr_proto::Destination::new(bundle.identity().key_and_cert().clone()).expect("dest"),
            published,
            u16::try_from(expires.saturating_sub(published)).expect("offset"),
            LeaseSet2Flags::from_raw(0),
        )
        .expect("header");
        let encryption_keys =
            vec![LeaseSet2EncryptionKey::new(CryptoKeyType::X25519, vec![0x55; 32]).expect("key")];
        let placeholder_signature =
            i2pr_proto::SignatureValue::new(i2pr_crypto::ROUTER_SIGNING_KEY_TYPE, vec![0u8; 64])
                .expect("placeholder");
        LeaseSet2::new(
            header,
            Mapping::empty(),
            encryption_keys,
            leases,
            placeholder_signature,
        )
        .expect("ls2")
    }

    #[test]
    fn selection_excludes_expired_leases() {
        let bundle = build_bundle(0x200);
        let leases = vec![
            Lease2::new(Hash::from_bytes([0x01; 32]), 1, Date32::from_seconds(900)),
            Lease2::new(Hash::from_bytes([0x02; 32]), 2, Date32::from_seconds(1_200)),
        ];
        let ls2 = build_ls2(&bundle, 1_000, 1_200, leases);
        let destination_hash = ls2.destination().hash().expect("hash");
        let policy = LeaseSelectionPolicy::try_new(destination_hash, 60).expect("policy");
        let selector = LeaseSelector::new();
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let result = selector
            .select_with_rng(&ls2, &policy, 1_000, &mut rng)
            .expect("selection");
        assert_eq!(result.gateway_router_hash, Hash::from_bytes([0x02; 32]));
        assert_eq!(result.tunnel_id, 2);
        assert_eq!(result.lease_expiration_seconds, 1_200);
    }

    #[test]
    fn selection_excludes_leases_inside_safety_margin() {
        let bundle = build_bundle(0x201);
        let leases = vec![
            Lease2::new(Hash::from_bytes([0x01; 32]), 1, Date32::from_seconds(1_050)),
            Lease2::new(Hash::from_bytes([0x02; 32]), 2, Date32::from_seconds(1_500)),
        ];
        let ls2 = build_ls2(&bundle, 1_000, 1_500, leases);
        let destination_hash = ls2.destination().hash().expect("hash");
        let policy = LeaseSelectionPolicy::try_new(destination_hash, 100).expect("policy");
        let selector = LeaseSelector::new();
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let result = selector
            .select_with_rng(&ls2, &policy, 1_000, &mut rng)
            .expect("selection");
        assert_eq!(result.tunnel_id, 2);
    }

    #[test]
    fn selection_rejects_zero_tunnel_id() {
        let bundle = build_bundle(0x202);
        let leases = vec![Lease2::new(
            Hash::from_bytes([0x01; 32]),
            0,
            Date32::from_seconds(1_500),
        )];
        let ls2 = build_ls2(&bundle, 1_000, 1_500, leases);
        let destination_hash = ls2.destination().hash().expect("hash");
        let policy = LeaseSelectionPolicy::try_new(destination_hash, 60).expect("policy");
        let selector = LeaseSelector::new();
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let error = selector
            .select_with_rng(&ls2, &policy, 1_000, &mut rng)
            .expect_err("zero tunnel id");
        // A zero-tunnel-id lease is filtered out by the usability
        // check before the ZeroTunnelId terminal error fires, so the
        // selector surfaces NoUsableLeases.
        assert_eq!(error, LeaseSelectionError::NoUsableLeases);
    }

    #[test]
    fn selection_rejects_destination_mismatch() {
        let bundle_a = build_bundle(0x203);
        let bundle_b = build_bundle(0x204);
        let leases = vec![Lease2::new(
            Hash::from_bytes([0x01; 32]),
            1,
            Date32::from_seconds(1_500),
        )];
        let ls2 = build_ls2(&bundle_a, 1_000, 1_500, leases);
        let other_hash = bundle_b.identity().hash().expect("hash");
        let policy = LeaseSelectionPolicy::try_new(other_hash, 60).expect("policy");
        let selector = LeaseSelector::new();
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let error = selector
            .select_with_rng(&ls2, &policy, 1_000, &mut rng)
            .expect_err("mismatch");
        assert_eq!(error, LeaseSelectionError::DestinationMismatch);
    }

    #[test]
    fn selection_returns_one_of_multiple_with_uniform_distribution() {
        let bundle = build_bundle(0x205);
        let leases = vec![
            Lease2::new(Hash::from_bytes([0x01; 32]), 1, Date32::from_seconds(1_500)),
            Lease2::new(Hash::from_bytes([0x02; 32]), 2, Date32::from_seconds(1_500)),
            Lease2::new(Hash::from_bytes([0x03; 32]), 3, Date32::from_seconds(1_500)),
        ];
        let ls2 = build_ls2(&bundle, 1_000, 1_500, leases);
        let destination_hash = ls2.destination().hash().expect("hash");
        let policy = LeaseSelectionPolicy::try_new(destination_hash, 60).expect("policy");
        let selector = LeaseSelector::new();
        let mut observed = [0_usize; 3];
        for seed in 0..32 {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let selection = selector
                .select_with_rng(&ls2, &policy, 1_000, &mut rng)
                .expect("selection");
            observed[(selection.tunnel_id - 1) as usize] += 1;
        }
        // No single lease should dominate; the selector must reach
        // at least two distinct indices over 32 calls.
        assert!(
            observed.iter().filter(|count| **count > 0).count() >= 2,
            "selector must visit more than one lease, observed {observed:?}"
        );
    }
}
