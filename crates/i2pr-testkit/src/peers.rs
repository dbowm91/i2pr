use std::fmt;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_proto::{Date, Hash, Mapping, RouterInfo};

use crate::network::{DatagramConfig, DatagramLink, NetworkScheduler, StreamConfig, StreamLink};
use crate::rng::{DeterministicRng, ReproducibilitySeed};
use crate::{FaultScript, LinkId};

/// Maximum peers created by one deterministic factory.
pub const MAX_TEST_PEERS: usize = 128;

/// Stable synthetic peer identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PeerId(u32);
impl PeerId {
    /// Returns the numeric identifier.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Synthetic service identifier for local test topology metadata.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SyntheticServiceId(u32);

impl SyntheticServiceId {
    /// Creates a nonzero bounded service identifier.
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    /// Returns the stable numeric identifier.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Public summary of an ephemeral peer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeerSummary {
    /// Synthetic identifier.
    pub id: PeerId,
    /// Public identity hash, redacted by its own safe `Debug` implementation.
    pub identity_hash: Hash,
}

/// An ephemeral peer. Private key material remains memory-only and is not Debug.
pub struct TestPeer {
    id: PeerId,
    identity: RouterIdentityBundle,
    summary: PeerSummary,
}

impl fmt::Debug for TestPeer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TestPeer")
            .field("id", &self.id)
            .finish()
    }
}

impl TestPeer {
    /// Returns the public peer summary.
    pub fn summary(&self) -> &PeerSummary {
        &self.summary
    }
    /// Returns the synthetic identifier.
    pub const fn id(&self) -> PeerId {
        self.id
    }
    /// Borrows the public RouterIdentity.
    pub fn identity(&self) -> &i2pr_proto::RouterIdentity {
        self.identity.identity()
    }
    /// Builds a no-capability, structurally signed RouterInfo in memory.
    pub fn router_info(
        &self,
        published_millis: u64,
    ) -> Result<RouterInfo, i2pr_crypto::CryptoError> {
        self.identity.sign_router_info(
            Date::from_millis(published_millis),
            Vec::new(),
            Vec::new(),
            Mapping::empty(),
        )
    }
}

/// Errors returned by deterministic peer factories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerFactoryError {
    /// The requested number or index exceeds the explicit bound.
    PeerLimit,
    /// Identity construction failed structurally.
    Identity,
    /// A topology link identifier was invalid.
    Link,
}
impl fmt::Display for PeerFactoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PeerLimit => formatter.write_str("test peer limit reached"),
            Self::Identity => formatter.write_str("test identity construction failed"),
            Self::Link => formatter.write_str("test topology link is invalid"),
        }
    }
}
impl std::error::Error for PeerFactoryError {}

/// Domain-separated factory for ephemeral deterministic identities.
#[derive(Clone, Debug)]
pub struct PeerFactory {
    seed: ReproducibilitySeed,
    maximum: usize,
}

impl PeerFactory {
    /// Creates a factory with an explicit maximum peer count.
    pub fn new(seed: ReproducibilitySeed, maximum: usize) -> Result<Self, PeerFactoryError> {
        if maximum == 0 || maximum > MAX_TEST_PEERS {
            return Err(PeerFactoryError::PeerLimit);
        }
        Ok(Self { seed, maximum })
    }
    /// Creates a factory using the maximum supported count.
    pub fn bounded(seed: ReproducibilitySeed) -> Self {
        Self {
            seed,
            maximum: MAX_TEST_PEERS,
        }
    }
    /// Creates one peer at a stable zero-based index.
    pub fn peer(&self, index: usize) -> Result<TestPeer, PeerFactoryError> {
        if index >= self.maximum {
            return Err(PeerFactoryError::PeerLimit);
        }
        let label = format!("identity/{index}");
        let seed = self
            .seed
            .derive(&label)
            .map_err(|_| PeerFactoryError::Identity)?;
        let mut rng = DeterministicRng::new(seed);
        let mut signing = [0_u8; 32];
        let mut encryption = [0_u8; 32];
        rng.fill_bytes(&mut signing);
        rng.fill_bytes(&mut encryption);
        let identity = RouterIdentityBundle::from_private_bytes(signing, encryption, &mut rng)
            .map_err(|_| PeerFactoryError::Identity)?;
        let identity_hash = identity
            .identity()
            .hash()
            .map_err(|_| PeerFactoryError::Identity)?;
        let id = PeerId(u32::try_from(index + 1).map_err(|_| PeerFactoryError::PeerLimit)?);
        Ok(TestPeer {
            id,
            identity,
            summary: PeerSummary { id, identity_hash },
        })
    }
    /// Creates a deterministic vector of peers.
    pub fn peers(&self, count: usize) -> Result<Vec<TestPeer>, PeerFactoryError> {
        if count > self.maximum {
            return Err(PeerFactoryError::PeerLimit);
        }
        (0..count).map(|index| self.peer(index)).collect()
    }
    /// Returns the configured peer maximum.
    pub const fn maximum(&self) -> usize {
        self.maximum
    }

    /// Returns a deterministic synthetic service identifier.
    pub fn service_id(&self, index: usize) -> Result<SyntheticServiceId, PeerFactoryError> {
        if index >= self.maximum {
            return Err(PeerFactoryError::PeerLimit);
        }
        SyntheticServiceId::new(u32::try_from(index + 1).map_err(|_| PeerFactoryError::PeerLimit)?)
            .ok_or(PeerFactoryError::PeerLimit)
    }
}

/// Topology shape used by deterministic tests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TopologyKind {
    /// Connect peers in a line.
    Linear,
    /// Connect peer zero to every other peer.
    Star,
    /// Connect peers in a closed ring.
    Ring,
    /// Use explicit zero-based edges.
    Arbitrary(Vec<(usize, usize)>),
}

/// Errors returned by topology construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TopologyError {
    PeerLimit,
    InvalidEdge,
    DuplicateEdge,
}
impl fmt::Display for TopologyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PeerLimit => formatter.write_str("topology peer limit reached"),
            Self::InvalidEdge => formatter.write_str("topology edge is invalid"),
            Self::DuplicateEdge => formatter.write_str("topology edge is duplicated"),
        }
    }
}
impl std::error::Error for TopologyError {}

/// Public deterministic topology summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Topology {
    peers: Vec<PeerSummary>,
    edges: Vec<(PeerId, PeerId)>,
}

impl Topology {
    /// Builds a bounded topology summary.
    pub fn build(
        factory: &PeerFactory,
        count: usize,
        kind: TopologyKind,
    ) -> Result<Self, TopologyError> {
        let peers = factory
            .peers(count)
            .map_err(|_| TopologyError::PeerLimit)?
            .into_iter()
            .map(|peer| peer.summary().clone())
            .collect::<Vec<_>>();
        let mut raw = match kind {
            TopologyKind::Linear => (1..count).map(|index| (index - 1, index)).collect(),
            TopologyKind::Star => (1..count).map(|index| (0, index)).collect(),
            TopologyKind::Ring => {
                if count > 1 {
                    (0..count)
                        .map(|index| (index, (index + 1) % count))
                        .collect()
                } else {
                    Vec::new()
                }
            }
            TopologyKind::Arbitrary(edges) => edges,
        };
        raw.sort_unstable();
        let mut edges = Vec::with_capacity(raw.len());
        for (left, right) in raw {
            if left >= count || right >= count || left == right {
                return Err(TopologyError::InvalidEdge);
            }
            let edge = (PeerId((left + 1) as u32), PeerId((right + 1) as u32));
            if edges.contains(&edge) || edges.contains(&(edge.1, edge.0)) {
                return Err(TopologyError::DuplicateEdge);
            }
            edges.push(edge);
        }
        Ok(Self { peers, edges })
    }
    /// Returns public peer summaries.
    pub fn peers(&self) -> &[PeerSummary] {
        &self.peers
    }
    /// Returns public edges.
    pub fn edges(&self) -> &[(PeerId, PeerId)] {
        &self.edges
    }
}

/// A helper for building links matching a topology summary.
impl Topology {
    /// Builds stream links in edge order using stable link identifiers.
    pub fn stream_links(
        &self,
        scheduler: &NetworkScheduler,
        config: StreamConfig,
        faults: FaultScript,
    ) -> Result<Vec<StreamLink>, crate::SchedulerError> {
        self.edges
            .iter()
            .enumerate()
            .map(|(index, _)| {
                scheduler.stream_link(
                    LinkId::new((index + 1) as u32)
                        .map_err(|_| crate::SchedulerError::InvalidLimit)?,
                    config,
                    faults.clone(),
                )
            })
            .collect()
    }
    /// Builds datagram links in edge order using stable link identifiers.
    pub fn datagram_links(
        &self,
        scheduler: &NetworkScheduler,
        config: DatagramConfig,
        faults: FaultScript,
    ) -> Result<Vec<DatagramLink>, crate::SchedulerError> {
        self.edges
            .iter()
            .enumerate()
            .map(|(index, _)| {
                scheduler.datagram_link(
                    LinkId::new((index + 1) as u32)
                        .map_err(|_| crate::SchedulerError::InvalidLimit)?,
                    config,
                    faults.clone(),
                )
            })
            .collect()
    }
}
