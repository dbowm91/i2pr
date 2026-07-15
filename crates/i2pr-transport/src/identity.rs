//! Redacted peer references used as map keys by transport state.

use std::fmt;

use i2pr_proto::Hash;

/// A router-hash reference with intentionally redacted default diagnostics.
///
/// `PeerId` carries no mutable profile, address, or routing policy. The raw
/// hash is accepted at this boundary for map identity, but no public accessor
/// exposes it back to logs or snapshots.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PeerId(Hash);

impl PeerId {
    /// Creates a peer reference from an existing protocol hash.
    pub const fn from_hash(hash: Hash) -> Self {
        Self(hash)
    }

    /// Creates a deterministic synthetic peer reference for local tests.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Hash::from_bytes(bytes))
    }
}

impl fmt::Debug for PeerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PeerId(..)")
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("peer")
    }
}
