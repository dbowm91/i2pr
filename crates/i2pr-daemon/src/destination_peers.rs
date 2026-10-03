//! Bounded projection and selection of validated Destination build peers.
//!
//! RouterInfo records enter through NetDB validation. This module retains only
//! the build key and the Java-profile diversity facts needed for one bounded
//! selection; it never stores full RouterInfos or exposes peer details in
//! diagnostics.

#![forbid(unsafe_code)]

use std::net::IpAddr;

use i2pr_netdb::{RouterInfoStore, ValidatedRouterInfo};
use i2pr_proto::Hash;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use rand_core::{CryptoRng, RngCore};
use thiserror::Error;

/// Maximum number of canonical NetDB records projected for one selection.
pub const MAX_DESTINATION_CANDIDATES: usize = 256;
/// Exact number of remote routers in the qualified service profile.
pub const QUALIFIED_DESTINATION_HOPS: usize = 3;
const MAX_SELECTION_NODES: usize = 4_096;

/// A privacy-safe summary of one bounded candidate projection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CandidateProjectionSummary {
    /// Records considered from the canonical store prefix.
    pub scanned: usize,
    /// Records rejected because build/diversity metadata was incomplete.
    pub unqualified: usize,
}

/// Candidate facts extracted only from a validated RouterInfo.
#[derive(Clone)]
pub struct DestinationPeerCandidate {
    router_hash: Hash,
    static_encryption_key: [u8; 32],
    family: String,
    endpoints: Vec<EndpointBucket>,
}

impl std::fmt::Debug for DestinationPeerCandidate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DestinationPeerCandidate")
            .field("router_hash", &"<redacted>")
            .field("static_encryption_key", &"<redacted>")
            .field("family", &"<redacted>")
            .field("endpoint_count", &self.endpoints.len())
            .finish()
    }
}

impl DestinationPeerCandidate {
    /// Projects a candidate from a validated NetDB record. Missing family,
    /// supported public-key material, or address/port facts make the record
    /// unqualified for this profile.
    pub fn from_validated(record: &ValidatedRouterInfo) -> Option<Self> {
        let info = record.router_info();
        let family = info.options().get("family")?;
        if family.is_empty()
            || family.len() > 64
            || !family
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        {
            return None;
        }
        let public = info.router_identity().public_key().as_bytes();
        let static_encryption_key: [u8; 32] = public.try_into().ok()?;
        if static_encryption_key.iter().all(|byte| *byte == 0) {
            return None;
        }
        let mut endpoints = Vec::new();
        for address in info.addresses() {
            let Some(host) = address.options().get("host") else {
                continue;
            };
            let Some(port) = address.options().get("port") else {
                continue;
            };
            let Ok(port) = port.parse::<u16>() else {
                continue;
            };
            if port == 0 {
                continue;
            }
            let Ok(ip) = host.parse::<IpAddr>() else {
                continue;
            };
            endpoints.push(EndpointBucket::new(ip, port));
        }
        if endpoints.is_empty() {
            return None;
        }
        Some(Self {
            router_hash: Hash::from_bytes(*record.key().as_bytes()),
            static_encryption_key,
            family: family.to_ascii_lowercase(),
            endpoints,
        })
    }

    /// Returns the RouterHash for internal build construction.
    pub const fn router_hash(&self) -> Hash {
        self.router_hash
    }

    /// Returns the static build-encryption key for internal build construction.
    pub const fn static_encryption_key(&self) -> &[u8; 32] {
        &self.static_encryption_key
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EndpointBucket {
    network: NetworkBucket,
    port: u16,
}

impl EndpointBucket {
    fn new(ip: IpAddr, port: u16) -> Self {
        let network = match ip {
            IpAddr::V4(ip) => {
                let octets = ip.octets();
                NetworkBucket::V4([octets[0], octets[1]])
            }
            IpAddr::V6(ip) => {
                let octets = ip.octets();
                NetworkBucket::V6([octets[0], octets[1], octets[2], octets[3]])
            }
        };
        Self { network, port }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NetworkBucket {
    V4([u8; 2]),
    V6([u8; 4]),
}

/// Typed terminal from exact-three candidate selection.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum DestinationSelectionError {
    /// No candidates survived validated projection.
    #[error("no qualified Destination build candidates")]
    NoCandidates,
    /// Fewer than three mutually diverse peers could be selected.
    #[error("insufficient diverse Destination build candidates")]
    InsufficientDiversity,
    /// The bounded backtracking budget was exhausted.
    #[error("Destination candidate selection budget exhausted")]
    SelectionBudgetExhausted,
    /// The candidate projection exceeded its fixed ceiling.
    #[error("Destination candidate set exceeds its fixed limit")]
    CandidateLimitExceeded,
    /// The operating-system randomness provider could not seed the selector.
    #[error("operating-system randomness is unavailable")]
    EntropyUnavailable,
}

/// Projects at most [`MAX_DESTINATION_CANDIDATES`] canonical records.
pub fn project_validated_store(
    store: &RouterInfoStore,
) -> (Vec<DestinationPeerCandidate>, CandidateProjectionSummary) {
    let mut candidates = Vec::new();
    let mut summary = CandidateProjectionSummary::default();
    for (_, record) in store.iter().take(MAX_DESTINATION_CANDIDATES) {
        summary.scanned += 1;
        if let Some(candidate) = DestinationPeerCandidate::from_validated(record) {
            candidates.push(candidate);
        } else {
            summary.unqualified += 1;
        }
    }
    (candidates, summary)
}

#[cfg(test)]
pub(crate) fn test_candidate(
    seed: u64,
    family: &str,
    host: &str,
    port: u16,
) -> DestinationPeerCandidate {
    let validated = test_validated_record(seed, Some(family), Some((host, port)));
    DestinationPeerCandidate::from_validated(&validated).expect("projected candidate")
}

#[cfg(test)]
pub(crate) fn test_validated_record(
    seed: u64,
    family: Option<&str>,
    endpoint: Option<(&str, u16)>,
) -> ValidatedRouterInfo {
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_netdb::ValidationContext;
    use i2pr_proto::{Date, Mapping, RouterAddress};
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
    let mut options = Mapping::builder();
    if let Some(family) = family {
        options
            .insert("family".to_owned(), family.to_owned())
            .expect("family");
    }
    let addresses = endpoint
        .map(|(host, port)| {
            let address_options = Mapping::from_entries(vec![
                ("host".to_owned(), host.to_owned()),
                ("port".to_owned(), port.to_string()),
            ])
            .expect("address options");
            RouterAddress::new(1, Date::from_millis(0), "SSU2".to_owned(), address_options)
                .expect("address")
        })
        .into_iter()
        .collect();
    let info = bundle
        .sign_router_info(
            Date::from_millis(0),
            addresses,
            Vec::new(),
            options.build().expect("options"),
        )
        .expect("signed info");
    ValidatedRouterInfo::from_router_info(info, None, ValidationContext::new(Date::from_millis(0)))
        .expect("validated info")
}

/// Selects exactly three candidates under the retained Java family and
/// address-proximity profile. Candidate order is randomized before bounded
/// backtracking; no shorter path is returned.
pub fn select_destination_path<R: CryptoRng + RngCore>(
    candidates: &[DestinationPeerCandidate],
    rng: &mut R,
) -> Result<Vec<DestinationPeerCandidate>, DestinationSelectionError> {
    if candidates.is_empty() {
        return Err(DestinationSelectionError::NoCandidates);
    }
    if candidates.len() > MAX_DESTINATION_CANDIDATES {
        return Err(DestinationSelectionError::CandidateLimitExceeded);
    }
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    for upper in (1..order.len()).rev() {
        let selected = random_below(rng, upper + 1);
        order.swap(upper, selected);
    }
    let mut selected = Vec::with_capacity(QUALIFIED_DESTINATION_HOPS);
    let mut nodes = 0;
    if search_path(candidates, &order, 0, &mut selected, &mut nodes) {
        return Ok(selected
            .into_iter()
            .map(|index| candidates[index].clone())
            .collect());
    }
    if nodes >= MAX_SELECTION_NODES {
        Err(DestinationSelectionError::SelectionBudgetExhausted)
    } else {
        Err(DestinationSelectionError::InsufficientDiversity)
    }
}

/// Production selector constructor using the operating-system CSPRNG.
pub fn select_destination_path_os(
    candidates: &[DestinationPeerCandidate],
) -> Result<Vec<DestinationPeerCandidate>, DestinationSelectionError> {
    let mut rng =
        ChaCha8Rng::try_from_os_rng().map_err(|_| DestinationSelectionError::EntropyUnavailable)?;
    select_destination_path(candidates, &mut rng)
}

fn search_path(
    candidates: &[DestinationPeerCandidate],
    order: &[usize],
    start: usize,
    selected: &mut Vec<usize>,
    nodes: &mut usize,
) -> bool {
    if selected.len() == QUALIFIED_DESTINATION_HOPS {
        return true;
    }
    for position in start..order.len() {
        if *nodes >= MAX_SELECTION_NODES {
            return false;
        }
        *nodes += 1;
        let candidate_index = order[position];
        if selected.iter().any(|selected_index| {
            !diverse_from(&candidates[candidate_index], &candidates[*selected_index])
        }) {
            continue;
        }
        selected.push(candidate_index);
        if search_path(candidates, order, position + 1, selected, nodes) {
            return true;
        }
        selected.pop();
    }
    false
}

fn diverse_from(left: &DestinationPeerCandidate, right: &DestinationPeerCandidate) -> bool {
    left.router_hash != right.router_hash
        && !left.family.eq_ignore_ascii_case(&right.family)
        && !left
            .endpoints
            .iter()
            .any(|left_endpoint| right.endpoints.contains(left_endpoint))
}

fn random_below<R: RngCore>(rng: &mut R, upper_exclusive: usize) -> usize {
    let upper = upper_exclusive as u64;
    let zone = u64::MAX - u64::MAX % upper;
    loop {
        let value = rng.next_u64();
        if value < zone {
            return (value % upper) as usize;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn candidate(seed: u64, family: &str, host: &str, port: u16) -> DestinationPeerCandidate {
        test_candidate(seed, family, host, port)
    }

    #[test]
    fn exact_three_selection_is_seeded_and_excludes_same_family_and_network_port() {
        let candidates = vec![
            candidate(1, "fam-a", "10.1.1.1", 1234),
            candidate(2, "fam-a", "10.2.1.1", 1234),
            candidate(3, "fam-b", "10.1.99.2", 1234),
            candidate(4, "fam-c", "10.1.99.3", 1235),
            candidate(5, "fam-d", "2001:db8:1::1", 1234),
        ];
        let mut left_rng = ChaCha8Rng::seed_from_u64(99);
        let mut right_rng = ChaCha8Rng::seed_from_u64(99);
        let left = select_destination_path(&candidates, &mut left_rng).expect("selected path");
        let right = select_destination_path(&candidates, &mut right_rng).expect("selected path");
        assert_eq!(left.len(), 3);
        assert_eq!(
            left.iter()
                .map(DestinationPeerCandidate::router_hash)
                .collect::<Vec<_>>(),
            right
                .iter()
                .map(DestinationPeerCandidate::router_hash)
                .collect::<Vec<_>>()
        );
        for (index, candidate) in left.iter().enumerate() {
            assert!(
                left[..index]
                    .iter()
                    .all(|prior| diverse_from(candidate, prior))
            );
        }
    }

    #[test]
    fn production_os_random_constructor_returns_exactly_three_candidates() {
        let candidates = vec![
            candidate(6_001, "family-a", "10.1.1.1", 1001),
            candidate(6_002, "family-b", "10.2.1.1", 1002),
            candidate(6_003, "family-c", "10.3.1.1", 1003),
            candidate(6_004, "family-d", "10.4.1.1", 1004),
        ];
        let selected = select_destination_path_os(&candidates).expect("OS-seeded selection");
        assert_eq!(selected.len(), QUALIFIED_DESTINATION_HOPS);
    }

    #[test]
    fn scarcity_is_typed_and_never_returns_a_shorter_path() {
        let candidates = vec![
            candidate(11, "fam-a", "10.1.1.1", 1001),
            candidate(12, "fam-b", "10.1.1.2", 1001),
            candidate(13, "fam-c", "10.1.1.3", 1001),
        ];
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        assert!(matches!(
            select_destination_path(&candidates, &mut rng),
            Err(DestinationSelectionError::InsufficientDiversity)
        ));
    }

    #[test]
    fn empty_and_duplicate_candidates_have_typed_non_path_outcomes() {
        let mut rng = ChaCha8Rng::seed_from_u64(21);
        assert!(matches!(
            select_destination_path(&[], &mut rng),
            Err(DestinationSelectionError::NoCandidates)
        ));
        let first = candidate(22, "family-a", "10.1.1.1", 1001);
        let candidates = vec![
            first.clone(),
            first,
            candidate(23, "family-b", "10.2.1.1", 1002),
        ];
        assert!(matches!(
            select_destination_path(&candidates, &mut rng),
            Err(DestinationSelectionError::InsufficientDiversity)
        ));
    }

    #[test]
    fn ipv4_16_and_ipv6_32_with_matching_ports_are_excluded() {
        let ipv4 = vec![
            candidate(31, "v4-a", "198.51.100.1", 1500),
            candidate(32, "v4-b", "198.51.101.9", 1500),
            candidate(33, "v4-c", "203.0.113.1", 1500),
        ];
        let mut rng = ChaCha8Rng::seed_from_u64(34);
        assert!(matches!(
            select_destination_path(&ipv4, &mut rng),
            Err(DestinationSelectionError::InsufficientDiversity)
        ));

        let ipv6 = vec![
            candidate(35, "v6-a", "2001:db8:1234:1::1", 1501),
            candidate(36, "v6-b", "2001:db8:1234:ffff::2", 1501),
            candidate(37, "v6-c", "2001:db9::1", 1501),
        ];
        let mut rng = ChaCha8Rng::seed_from_u64(38);
        assert!(matches!(
            select_destination_path(&ipv6, &mut rng),
            Err(DestinationSelectionError::InsufficientDiversity)
        ));
    }

    #[test]
    fn validated_store_projection_rejects_missing_family_or_address_and_is_bounded() {
        let mut store = RouterInfoStore::default();
        assert_eq!(
            store.insert(test_validated_record(51, None, Some(("192.0.2.1", 1500)))),
            i2pr_netdb::InsertOutcome::Inserted
        );
        assert_eq!(
            store.insert(test_validated_record(52, Some("family-b"), None)),
            i2pr_netdb::InsertOutcome::Inserted
        );
        let (candidates, summary) = project_validated_store(&store);
        assert!(candidates.is_empty());
        assert_eq!(summary.scanned, 2);
        assert_eq!(summary.unqualified, 2);
        assert!(summary.scanned <= MAX_DESTINATION_CANDIDATES);
    }

    #[test]
    fn validated_store_projection_stops_at_candidate_capacity() {
        let mut store = RouterInfoStore::default();
        for index in 0..=MAX_DESTINATION_CANDIDATES {
            let seed = 10_000 + index as u64;
            let family = format!("family-{seed}");
            let host = format!("10.{}.{}.1", (index / 250) % 250, index % 250 + 1);
            let port = 10_000 + index as u16;
            let inserted = store.insert(test_validated_record(
                seed,
                Some(&family),
                Some((&host, port)),
            ));
            assert_eq!(inserted, i2pr_netdb::InsertOutcome::Inserted);
        }
        let (candidates, summary) = project_validated_store(&store);
        assert_eq!(summary.scanned, MAX_DESTINATION_CANDIDATES);
        assert_eq!(candidates.len(), MAX_DESTINATION_CANDIDATES);
        assert_eq!(summary.unqualified, 0);
    }
}
