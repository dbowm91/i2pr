//! Bounded projection and selection of validated Destination build peers.
//!
//! RouterInfo records enter through NetDB validation. This module retains only
//! the build key and the Java-profile diversity facts needed for one bounded
//! selection; it never stores full RouterInfos or exposes peer details in
//! diagnostics.
//!
//! Plan 492 adds the **selection diversity** the router's own tunnel hops
//! were missing: the /16 network-prefix rule and the per-peer participation
//! cap, both taken from Plan 488's selection policy in `i2pr-netdb`. The
//! ordinary path is the daemon's build-peer projection, not
//! `i2pr-client`'s pool: `i2pr-client` owns destination tunnel *lifecycles*
//! and consumes peers that were already chosen here.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::net::IpAddr;

use i2pr_netdb::{
    DEFAULT_MAX_PARTICIPATION, DEFAULT_MAX_PER_PREFIX16, RouterHash, RouterInfoStore,
    ValidatedRouterInfo, network_prefix16, published_version_of, tunnel_peer_eligibility,
};
use i2pr_proto::{Hash, VersionEligibility};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use rand_core::{CryptoRng, RngCore};
use thiserror::Error;

/// Maximum number of canonical NetDB records projected for one selection.
pub const MAX_DESTINATION_CANDIDATES: usize = 256;
/// Exact number of remote routers in the qualified service profile.
pub const QUALIFIED_DESTINATION_HOPS: usize = 3;
const MAX_SELECTION_NODES: usize = 4_096;

/// The Plan 488 diversity rules the ordinary build-hop selection honours.
///
/// The values are Plan 488's defaults, taken from `i2pr-netdb` rather than
/// restated here: the router's own tunnel hops use the same policy the
/// profile-store selection uses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HopDiversityPolicy {
    /// The maximum number of peers from one /16 network prefix in one
    /// selection.
    pub max_per_prefix16: usize,
    /// The maximum number of selections one peer may accumulate.
    pub max_participation: u32,
}

impl Default for HopDiversityPolicy {
    fn default() -> Self {
        Self {
            max_per_prefix16: DEFAULT_MAX_PER_PREFIX16,
            max_participation: DEFAULT_MAX_PARTICIPATION,
        }
    }
}

/// A privacy-safe summary of one bounded candidate projection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CandidateProjectionSummary {
    /// Records considered from the canonical store prefix.
    pub scanned: usize,
    /// Records rejected because build/diversity metadata was incomplete.
    pub unqualified: usize,
    /// Records rejected because the peer publishes no API version at all.
    pub refused_unknown_version: usize,
    /// Records rejected because the peer publishes an uncomparable API
    /// version.
    pub refused_uncomparable_version: usize,
    /// Records rejected because the peer publishes an API version below the
    /// tunnel-build floor.
    pub refused_below_version_floor: usize,
    /// Records rejected because the peer reached the per-peer participation
    /// cap.
    pub capped_participation: usize,
}

impl CandidateProjectionSummary {
    /// The number of records the tunnel-build API version floor refused.
    pub const fn version_refusals(&self) -> usize {
        self.refused_unknown_version
            .saturating_add(self.refused_uncomparable_version)
            .saturating_add(self.refused_below_version_floor)
    }
}

/// The typed reason a validated record is not usable as a build peer.
///
/// The version reasons are Plan 491's: the specification's API-version table
/// attaches a minimum-peer obligation to the level this router declares, and
/// a peer that publishes no comparable version cannot satisfy it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum CandidateRefusal {
    /// The peer publishes no API version, so it cannot be compared against
    /// the floor.
    #[error("the peer publishes no API version")]
    UnknownVersion,
    /// The peer publishes an API version that cannot be compared.
    #[error("the peer publishes an uncomparable API version")]
    UncomparableVersion,
    /// The peer publishes an API version below the tunnel-build floor.
    #[error("the peer publishes an API version below the tunnel-build floor")]
    BelowVersionFloor,
    /// The record lacks the build/diversity metadata this profile requires.
    #[error("the record lacks the build metadata for this profile")]
    Unqualified,
}

fn version_refusal(eligibility: VersionEligibility) -> Option<CandidateRefusal> {
    match eligibility {
        VersionEligibility::Eligible => None,
        VersionEligibility::UnknownVersion => Some(CandidateRefusal::UnknownVersion),
        VersionEligibility::UncomparableVersion => Some(CandidateRefusal::UncomparableVersion),
        VersionEligibility::BelowFloor { .. } => Some(CandidateRefusal::BelowVersionFloor),
    }
}

/// Candidate facts extracted only from a validated RouterInfo.
#[derive(Clone)]
pub struct DestinationPeerCandidate {
    router_hash: Hash,
    static_encryption_key: [u8; 32],
    family: String,
    endpoints: Vec<EndpointBucket>,
    prefix16: u16,
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
    ///
    /// Plan 491: a record whose published API version is unknown,
    /// uncomparable, or below the tunnel-build floor is refused with the
    /// matching [`CandidateRefusal`] before any other fact is considered. The
    /// refusal is typed, so the caller can count it rather than skip it
    /// silently.
    pub fn from_validated(record: &ValidatedRouterInfo) -> Result<Self, CandidateRefusal> {
        let eligibility = tunnel_peer_eligibility(published_version_of(record));
        if let Some(refusal) = version_refusal(eligibility) {
            return Err(refusal);
        }
        let info = record.router_info();
        let Some(family) = info.options().get("family") else {
            return Err(CandidateRefusal::Unqualified);
        };
        if family.is_empty()
            || family.len() > 64
            || !family
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        {
            return Err(CandidateRefusal::Unqualified);
        }
        let public = info.router_identity().public_key().as_bytes();
        let Ok(static_encryption_key) = <[u8; 32]>::try_from(public) else {
            return Err(CandidateRefusal::Unqualified);
        };
        if static_encryption_key.iter().all(|byte| *byte == 0) {
            return Err(CandidateRefusal::Unqualified);
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
            return Err(CandidateRefusal::Unqualified);
        }
        Ok(Self {
            router_hash: Hash::from_bytes(*record.key().as_bytes()),
            static_encryption_key,
            family: family.to_ascii_lowercase(),
            endpoints,
            prefix16: network_prefix16(&RouterHash::from_hash(Hash::from_bytes(
                *record.key().as_bytes(),
            ))),
        })
    }

    /// Returns the RouterHash for internal build construction.
    pub const fn router_hash(&self) -> Hash {
        self.router_hash
    }

    /// Returns the peer's /16 network prefix, as Plan 488's diversity rule
    /// defines it: the leading two bytes of the peer's RouterHash.
    pub const fn prefix16(&self) -> u16 {
        self.prefix16
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
    /// Every scanned record was refused by the tunnel-build API version
    /// floor.
    #[error("no Destination build candidate meets the tunnel-build API version floor")]
    NoEligibleVersionPeers,
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
///
/// Plan 492: a peer whose participation counter reached the policy's
/// per-peer cap is not eligible for another hop selection. The refusal is
/// counted, so a capped peer is never a silent skip.
pub fn project_validated_store(
    store: &RouterInfoStore,
    participation: &BTreeMap<RouterHash, u32>,
    policy: &HopDiversityPolicy,
) -> (Vec<DestinationPeerCandidate>, CandidateProjectionSummary) {
    let mut candidates = Vec::new();
    let mut summary = CandidateProjectionSummary::default();
    for (_, record) in store.iter().take(MAX_DESTINATION_CANDIDATES) {
        summary.scanned += 1;
        match DestinationPeerCandidate::from_validated(record) {
            Ok(candidate) => {
                let key = RouterHash::from_hash(candidate.router_hash());
                let count = participation.get(&key).copied().unwrap_or(0);
                if policy.max_participation > 0 && count >= policy.max_participation {
                    summary.capped_participation += 1;
                    continue;
                }
                candidates.push(candidate);
            }
            Err(CandidateRefusal::UnknownVersion) => summary.refused_unknown_version += 1,
            Err(CandidateRefusal::UncomparableVersion) => {
                summary.refused_uncomparable_version += 1;
            }
            Err(CandidateRefusal::BelowVersionFloor) => summary.refused_below_version_floor += 1,
            Err(CandidateRefusal::Unqualified) => summary.unqualified += 1,
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
pub(crate) fn test_validated_record_with_version(
    seed: u64,
    family: Option<&str>,
    endpoint: Option<(&str, u16)>,
    version: Option<&str>,
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
    if let Some(version) = version {
        options
            .insert("router.version".to_owned(), version.to_owned())
            .expect("router.version");
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
    // Plan 491: a build peer must publish a comparable API version at or
    // above the tunnel-build minimum-version floor.
    options
        .insert(
            "router.version".to_owned(),
            i2pr_netdb::CONTROLLED_ROUTER_VERSION.to_owned(),
        )
        .expect("router.version");
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
///
/// Plan 492: the selection also honours Plan 488's diversity policy — at
/// most `max_per_prefix16` peers from one /16 network prefix, and the
/// per-peer participation cap applied at projection time. A store that
/// cannot satisfy the rules yields a typed terminal; the selector never
/// violates a rule to reach the requested count.
pub fn select_destination_path<R: CryptoRng + RngCore>(
    candidates: &[DestinationPeerCandidate],
    rng: &mut R,
) -> Result<Vec<DestinationPeerCandidate>, DestinationSelectionError> {
    select_destination_path_with_policy(candidates, rng, &HopDiversityPolicy::default())
}

/// Selects exactly three candidates under an explicit diversity policy.
pub fn select_destination_path_with_policy<R: CryptoRng + RngCore>(
    candidates: &[DestinationPeerCandidate],
    rng: &mut R,
    policy: &HopDiversityPolicy,
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
    let mut per_prefix: BTreeMap<u16, usize> = BTreeMap::new();
    if search_path(
        candidates,
        &order,
        0,
        &mut selected,
        &mut nodes,
        policy,
        &mut per_prefix,
    ) {
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

/// Selects exactly three build peers from a bounded projection, failing closed
/// with the reason the projection is empty.
///
/// When the projection refused every scanned record by the tunnel-build API
/// version floor, the terminal is
/// [`DestinationSelectionError::NoEligibleVersionPeers`] rather than a bare
/// "no candidates": the operator can see that the floor, not the store, was
/// the reason. The build is refused either way — the selector never loops
/// looking for a peer that is not allowed.
pub fn select_projected_destination_path<R: CryptoRng + RngCore>(
    candidates: &[DestinationPeerCandidate],
    summary: &CandidateProjectionSummary,
    rng: &mut R,
) -> Result<Vec<DestinationPeerCandidate>, DestinationSelectionError> {
    select_projected_destination_path_with_policy(
        candidates,
        summary,
        rng,
        &HopDiversityPolicy::default(),
    )
}

/// Selects exactly three build peers from a bounded projection under an
/// explicit diversity policy, failing closed with the reason the projection
/// is empty.
pub fn select_projected_destination_path_with_policy<R: CryptoRng + RngCore>(
    candidates: &[DestinationPeerCandidate],
    summary: &CandidateProjectionSummary,
    rng: &mut R,
    policy: &HopDiversityPolicy,
) -> Result<Vec<DestinationPeerCandidate>, DestinationSelectionError> {
    if candidates.is_empty() {
        return Err(if summary.version_refusals() > 0 {
            DestinationSelectionError::NoEligibleVersionPeers
        } else {
            DestinationSelectionError::NoCandidates
        });
    }
    select_destination_path_with_policy(candidates, rng, policy)
}

/// Production constructor for [`select_projected_destination_path`].
pub fn select_projected_destination_path_os(
    candidates: &[DestinationPeerCandidate],
    summary: &CandidateProjectionSummary,
) -> Result<Vec<DestinationPeerCandidate>, DestinationSelectionError> {
    let mut rng =
        ChaCha8Rng::try_from_os_rng().map_err(|_| DestinationSelectionError::EntropyUnavailable)?;
    select_projected_destination_path(candidates, summary, &mut rng)
}

/// Production constructor for
/// [`select_projected_destination_path_with_policy`].
pub fn select_projected_destination_path_os_with_policy(
    candidates: &[DestinationPeerCandidate],
    summary: &CandidateProjectionSummary,
    policy: &HopDiversityPolicy,
) -> Result<Vec<DestinationPeerCandidate>, DestinationSelectionError> {
    let mut rng =
        ChaCha8Rng::try_from_os_rng().map_err(|_| DestinationSelectionError::EntropyUnavailable)?;
    select_projected_destination_path_with_policy(candidates, summary, &mut rng, policy)
}

fn search_path(
    candidates: &[DestinationPeerCandidate],
    order: &[usize],
    start: usize,
    selected: &mut Vec<usize>,
    nodes: &mut usize,
    policy: &HopDiversityPolicy,
    per_prefix: &mut BTreeMap<u16, usize>,
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
        let candidate = &candidates[candidate_index];
        if selected
            .iter()
            .any(|selected_index| !diverse_from(candidate, &candidates[*selected_index]))
        {
            continue;
        }
        // Plan 492: Plan 488's /16 diversity rule. At most
        // `max_per_prefix16` peers from one /16 network prefix enter one
        // selection; the rule is never relaxed to reach the hop count.
        let prefix = candidate.prefix16;
        if policy.max_per_prefix16 > 0 {
            let used = per_prefix.entry(prefix).or_insert(0);
            if *used >= policy.max_per_prefix16 {
                continue;
            }
            *used += 1;
        }
        selected.push(candidate_index);
        if search_path(
            candidates,
            order,
            position + 1,
            selected,
            nodes,
            policy,
            per_prefix,
        ) {
            return true;
        }
        selected.pop();
        if policy.max_per_prefix16 > 0
            && let Some(used) = per_prefix.get_mut(&prefix)
        {
            *used = used.saturating_sub(1);
        }
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
        let (candidates, summary) =
            project_validated_store(&store, &BTreeMap::new(), &HopDiversityPolicy::default());
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
        let (candidates, summary) =
            project_validated_store(&store, &BTreeMap::new(), &HopDiversityPolicy::default());
        assert_eq!(summary.scanned, MAX_DESTINATION_CANDIDATES);
        assert_eq!(candidates.len(), MAX_DESTINATION_CANDIDATES);
        assert_eq!(summary.unqualified, 0);
    }

    /// Plan 491: a peer below the tunnel-build floor is refused with the
    /// matching typed reason, and the projection counts it.
    #[test]
    fn a_peer_below_the_tunnel_build_floor_is_refused_and_counted() {
        let mut store = RouterInfoStore::default();
        let below = test_validated_record_with_version(
            0xD001,
            Some("family-x"),
            Some(("10.0.0.1", 9_999)),
            Some("0.9.61"),
        );
        store.insert(below);
        let (candidates, summary) =
            project_validated_store(&store, &BTreeMap::new(), &HopDiversityPolicy::default());
        assert!(candidates.is_empty(), "{candidates:?}");
        assert_eq!(summary.refused_below_version_floor, 1);
        assert_eq!(summary.version_refusals(), 1);
        assert_eq!(summary.unqualified, 0);

        // The selection fails closed with the reason, not a bare "no
        // candidates", and it never loops looking for another peer.
        let error = select_projected_destination_path_os(&candidates, &summary)
            .expect_err("must fail closed");
        assert_eq!(error, DestinationSelectionError::NoEligibleVersionPeers);
    }

    /// Plan 491: a peer that publishes no comparable API version is refused
    /// before any other fact about it is considered.
    #[test]
    fn a_peer_without_a_comparable_version_is_refused() {
        let below = test_validated_record_with_version(
            0xD002,
            Some("family-y"),
            Some(("10.0.0.2", 9_998)),
            None,
        );
        let error = DestinationPeerCandidate::from_validated(&below).expect_err("must be refused");
        assert_eq!(error, CandidateRefusal::UnknownVersion);
    }

    /// A candidate with a controlled /16 prefix, so the diversity rule can
    /// be exercised without depending on a random hash.
    fn candidate_with_prefix(
        prefix: u16,
        discriminator: u8,
        family: &str,
        port: u16,
    ) -> DestinationPeerCandidate {
        let mut bytes = [0_u8; 32];
        bytes[0..2].copy_from_slice(&prefix.to_be_bytes());
        bytes[2] = discriminator;
        let router_hash = Hash::from_bytes(bytes);
        DestinationPeerCandidate {
            router_hash,
            static_encryption_key: [0x11; 32],
            family: family.to_owned(),
            endpoints: vec![EndpointBucket::new(
                IpAddr::from([10, 0, discriminator, 1]),
                port,
            )],
            prefix16: network_prefix16(&RouterHash::from_hash(router_hash)),
        }
    }

    /// Plan 492 WP4: at most one peer from one /16 network prefix enters one
    /// selection, even when the rest of the profile would allow both.
    #[test]
    fn the_prefix16_rule_excludes_two_peers_from_one_network_prefix() {
        let candidates = vec![
            candidate_with_prefix(0x1111, 1, "family-a", 1001),
            candidate_with_prefix(0x1111, 2, "family-b", 1002),
            candidate_with_prefix(0x2222, 3, "family-c", 1003),
            candidate_with_prefix(0x3333, 4, "family-d", 1004),
        ];
        let mut rng = ChaCha8Rng::seed_from_u64(0x4921);
        let selected = select_destination_path(&candidates, &mut rng).expect("selected path");
        assert_eq!(selected.len(), QUALIFIED_DESTINATION_HOPS);
        let mut prefixes: Vec<u16> = selected.iter().map(|c| c.prefix16()).collect();
        prefixes.sort_unstable();
        prefixes.dedup();
        assert_eq!(
            prefixes.len(),
            QUALIFIED_DESTINATION_HOPS,
            "one /16 network prefix may contribute at most one peer"
        );
    }

    /// Plan 492 WP4: the diversity policy is honoured, and a policy that
    /// allows two peers from one prefix is honoured too.
    #[test]
    fn the_prefix16_policy_is_honoured_explicitly() {
        let candidates = vec![
            candidate_with_prefix(0x1111, 1, "family-a", 1001),
            candidate_with_prefix(0x1111, 2, "family-b", 1002),
            candidate_with_prefix(0x2222, 3, "family-c", 1003),
            candidate_with_prefix(0x3333, 4, "family-d", 1004),
        ];
        let permissive = HopDiversityPolicy {
            max_per_prefix16: 2,
            max_participation: DEFAULT_MAX_PARTICIPATION,
        };
        let mut rng = ChaCha8Rng::seed_from_u64(0x4922);
        let selected = select_destination_path_with_policy(&candidates, &mut rng, &permissive)
            .expect("selected path");
        assert_eq!(selected.len(), QUALIFIED_DESTINATION_HOPS);
        let shared = selected
            .iter()
            .filter(|candidate| candidate.prefix16() == 0x1111)
            .count();
        assert!(
            shared <= 2,
            "the explicit per-prefix policy must bound the prefix share"
        );
    }

    /// Plan 492 WP4: the selection is deterministic for a fixed seed.
    #[test]
    fn the_selection_is_deterministic_for_a_fixed_seed() {
        let candidates = vec![
            candidate_with_prefix(0x1111, 1, "family-a", 1001),
            candidate_with_prefix(0x2222, 2, "family-b", 1002),
            candidate_with_prefix(0x3333, 3, "family-c", 1003),
            candidate_with_prefix(0x4444, 4, "family-d", 1004),
        ];
        let mut left_rng = ChaCha8Rng::seed_from_u64(0x4923);
        let mut right_rng = ChaCha8Rng::seed_from_u64(0x4923);
        let left = select_destination_path(&candidates, &mut left_rng).expect("left");
        let right = select_destination_path(&candidates, &mut right_rng).expect("right");
        assert_eq!(
            left.iter()
                .map(DestinationPeerCandidate::router_hash)
                .collect::<Vec<_>>(),
            right
                .iter()
                .map(DestinationPeerCandidate::router_hash)
                .collect::<Vec<_>>()
        );
    }

    /// Plan 492 WP4: a peer that reached the per-peer participation cap is
    /// not offered again, and the refusal is counted.
    #[test]
    fn the_participation_cap_removes_a_capped_peer_from_the_projection() {
        let mut store = RouterInfoStore::default();
        let capped = test_validated_record(0xE001, Some("family-a"), Some(("192.0.2.1", 1500)));
        let capped_key = capped.key();
        store.insert(capped);
        store.insert(test_validated_record(
            0xE002,
            Some("family-b"),
            Some(("192.0.2.2", 1501)),
        ));
        let mut participation = BTreeMap::new();
        participation.insert(capped_key, DEFAULT_MAX_PARTICIPATION);
        let policy = HopDiversityPolicy::default();
        let (candidates, summary) = project_validated_store(&store, &participation, &policy);
        assert_eq!(summary.capped_participation, 1);
        assert_eq!(candidates.len(), 1);
    }
}
