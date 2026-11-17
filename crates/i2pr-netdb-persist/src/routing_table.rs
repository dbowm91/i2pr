//! Plan 489: optional persistence of the floodfill-indexed routing table.
//!
//! The routing table is a *derived* structure: it is rebuilt from the
//! floodfills the router knows, and it rotates with the UTC day. Its
//! persistence is therefore **optional** — a router that cannot read
//! the projection simply starts with an empty table and rebuilds it
//! through exploration.
//!
//! The persisted artifact is the same bounded, sanitized projection
//! the [`i2pr_netdb`] routing table encodes: the day the table was
//! built for, the peers' `RouterHash`es and their floodfill evidence,
//! under a SHA-256 digest over the body. It carries no address, no key
//! material and no payload.
//!
//! A projection that is missing, corrupt, oversized or expired is
//! **discarded and counted** by the caller. It never fails the
//! router's startup.

use i2pr_netdb::{BucketSet, BucketSetConfig, RouterHash, RoutingTableProjectionError};
use i2pr_proto::Date;
use i2pr_storage::cache_seam::{ByteCache, CacheError};

/// The cache filename of the persisted routing table.
///
/// The byte cache accepts only a 64-character lowercase hex filename.
/// The value is the SHA-256 of `i2pr-netdb-routing-table-v1`, so the
/// name is stable, opaque and carries no operator-supplied data.
pub const ROUTING_TABLE_CACHE_NAME: &str =
    "0c48a8be55244e8eb40b6b961a6e9bbf59530ab3001c656421d10bb6e063b81c";

/// The maximum size of the persisted routing table, in bytes.
pub const MAX_ROUTING_TABLE_BYTES: usize = i2pr_netdb::MAX_PROJECTION_BYTES;

/// The typed errors of the routing-table persistence.
#[derive(Debug, thiserror::Error)]
pub enum RoutingTablePersistError {
    /// The underlying byte cache refused the operation.
    #[error("routing table cache operation failed: {0}")]
    Cache(#[from] CacheError),
    /// The projection exceeded the permitted ceiling.
    #[error("routing table projection exceeds its {maximum} byte ceiling")]
    TooLarge {
        /// The permitted ceiling.
        maximum: usize,
    },
}

/// The outcome of loading the persisted routing table.
#[derive(Debug)]
pub enum RoutingTableLoad {
    /// The projection was decoded and applied.
    Restored(Box<BucketSet>),
    /// No projection is present. The router starts with an empty table.
    Absent,
    /// The projection was present but unusable. The caller counts the
    /// discard and starts with an empty table.
    Discarded {
        /// A bounded, human-readable reason.
        reason: &'static str,
    },
}

/// Byte-cache composition for the routing table.
///
/// The caller supplies the anchor — the router's own position in
/// today's keyspace — because the anchor is the router's identity,
/// not the table's.
#[derive(Clone, Debug)]
pub struct RoutingTableStore {
    cache: ByteCache,
    config: BucketSetConfig,
    max_bytes: usize,
}

impl RoutingTableStore {
    /// Constructs a store over the supplied byte cache.
    pub fn new(cache: ByteCache, config: BucketSetConfig) -> Self {
        Self {
            cache,
            config,
            max_bytes: MAX_ROUTING_TABLE_BYTES,
        }
    }

    /// Returns the bounded capacities of the table.
    pub fn config(&self) -> BucketSetConfig {
        self.config
    }

    /// Writes the table's projection.
    ///
    /// The projection is bounded before it reaches the cache, so a
    /// table that grew past its ceiling is refused rather than written.
    pub fn save(&self, table: &BucketSet) -> Result<(), RoutingTablePersistError> {
        let bytes = table.encode_projection();
        if bytes.len() > self.max_bytes {
            return Err(RoutingTablePersistError::TooLarge {
                maximum: self.max_bytes,
            });
        }
        self.cache.replace(ROUTING_TABLE_CACHE_NAME, &bytes)?;
        Ok(())
    }

    /// Reads the table's projection.
    ///
    /// A missing projection is [`RoutingTableLoad::Absent`]. A corrupt,
    /// oversized or malformed projection is
    /// [`RoutingTableLoad::Discarded`]. The projection carries the day
    /// it was built for, so a caller can recognise a table that has
    /// expired against the current UTC day.
    pub fn load(
        &self,
        now: Date,
        anchor: RouterHash,
    ) -> Result<RoutingTableLoad, RoutingTablePersistError> {
        let Some(bytes) = self.cache.read(ROUTING_TABLE_CACHE_NAME)? else {
            return Ok(RoutingTableLoad::Absent);
        };
        if bytes.len() > self.max_bytes {
            return Ok(RoutingTableLoad::Discarded {
                reason: "too_large",
            });
        }
        match BucketSet::decode_projection(&bytes, now, self.config, anchor) {
            Ok(table) => Ok(RoutingTableLoad::Restored(Box::new(table))),
            Err(error) => Ok(RoutingTableLoad::Discarded {
                reason: discard_reason(&error),
            }),
        }
    }

    /// Removes the persisted projection.
    pub fn remove(&self) -> Result<bool, RoutingTablePersistError> {
        Ok(self.cache.remove(ROUTING_TABLE_CACHE_NAME)?)
    }
}

/// Returns a bounded, human-readable reason for a discarded projection.
fn discard_reason(error: &RoutingTableProjectionError) -> &'static str {
    match error {
        RoutingTableProjectionError::TooLarge { .. } => "too_large",
        RoutingTableProjectionError::Malformed { .. } => "malformed",
        RoutingTableProjectionError::UnsupportedVersion(_) => "unsupported_version",
        RoutingTableProjectionError::DigestMismatch => "digest_mismatch",
        RoutingTableProjectionError::TooManyRecords { .. } => "too_many_records",
        RoutingTableProjectionError::UnknownEvidence(_) => "unknown_evidence",
        RoutingTableProjectionError::Config(_) => "invalid_config",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_netdb::{ValidatedRouterInfo, ValidationContext};
    use i2pr_proto::Mapping;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;
    use std::path::Path;
    use tempfile::tempdir;

    const T0: u64 = 1_000;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn validated(signer: &RouterIdentityBundle, caps: &str) -> ValidatedRouterInfo {
        let mut options = Mapping::builder();
        options.insert("caps".to_owned(), caps.to_owned()).unwrap();
        let published = Date::from_millis(T0);
        let info = signer
            .sign_router_info(published, Vec::new(), Vec::new(), options.build().unwrap())
            .expect("sign");
        ValidatedRouterInfo::from_router_info(info, None, ValidationContext::new(published))
            .expect("validate")
    }

    fn table_with(peer: &ValidatedRouterInfo) -> BucketSet {
        let anchor = peer.key();
        let mut table = BucketSet::new(anchor, Date::from_millis(T0), BucketSetConfig::default());
        table.insert_signed(peer, Date::from_millis(T0));
        table
    }

    #[test]
    fn the_projection_round_trips_through_the_cache() {
        let directory = tempdir().expect("temp directory");
        // The byte cache creates its own `netdb/routers` tree with
        // `0o700`; the `netdb` parent is created here so the cache
        // does not have to widen its own parent checks.
        std::fs::create_dir_all(directory.path().join("netdb")).expect("netdb parent");
        let cache = ByteCache::in_data_dir(directory.path());
        let store = RoutingTableStore::new(cache, BucketSetConfig::default());
        let peer = validated(&bundle(0x4893_0001), "Nf");
        let table = table_with(&peer);
        store.save(&table).expect("save");

        let loaded = store.load(Date::from_millis(T0), peer.key()).expect("load");
        let RoutingTableLoad::Restored(restored) = loaded else {
            panic!("the projection must be restored");
        };
        assert_eq!(restored.len(), 1);
        assert!(restored.contains(&peer.key()));
    }

    #[test]
    fn an_absent_projection_is_not_an_error() {
        let directory = tempdir().expect("temp directory");
        // The byte cache creates its own `netdb/routers` tree with
        // `0o700`; the `netdb` parent is created here so the cache
        // does not have to widen its own parent checks.
        std::fs::create_dir_all(directory.path().join("netdb")).expect("netdb parent");
        let cache = ByteCache::in_data_dir(directory.path());
        let store = RoutingTableStore::new(cache, BucketSetConfig::default());
        let loaded = store
            .load(Date::from_millis(T0), RouterHash::from_bytes([1; 32]))
            .expect("load");
        assert!(matches!(loaded, RoutingTableLoad::Absent));
    }

    #[test]
    fn a_corrupt_projection_is_discarded() {
        let directory = tempdir().expect("temp directory");
        // The byte cache creates its own `netdb/routers` tree with
        // `0o700`; the `netdb` parent is created here so the cache
        // does not have to widen its own parent checks.
        std::fs::create_dir_all(directory.path().join("netdb")).expect("netdb parent");
        let cache = ByteCache::in_data_dir(directory.path());
        let store = RoutingTableStore::new(cache, BucketSetConfig::default());
        let peer = validated(&bundle(0x4893_0002), "Nf");
        let table = table_with(&peer);
        store.save(&table).expect("save");

        // Corrupt the persisted bytes in place.
        let path = Path::new(directory.path())
            .join(i2pr_storage::cache_seam::ROUTERS_SUBDIR)
            .join(ROUTING_TABLE_CACHE_NAME);
        let mut bytes = std::fs::read(&path).expect("read");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        std::fs::write(&path, &bytes).expect("write");

        let loaded = store.load(Date::from_millis(T0), peer.key()).expect("load");
        assert!(matches!(
            loaded,
            RoutingTableLoad::Discarded {
                reason: "digest_mismatch"
            }
        ));
    }

    #[test]
    fn the_projection_can_be_removed() {
        let directory = tempdir().expect("temp directory");
        // The byte cache creates its own `netdb/routers` tree with
        // `0o700`; the `netdb` parent is created here so the cache
        // does not have to widen its own parent checks.
        std::fs::create_dir_all(directory.path().join("netdb")).expect("netdb parent");
        let cache = ByteCache::in_data_dir(directory.path());
        let store = RoutingTableStore::new(cache, BucketSetConfig::default());
        let peer = validated(&bundle(0x4893_0003), "Nf");
        let table = table_with(&peer);
        store.save(&table).expect("save");
        assert!(store.remove().expect("remove"));
        let loaded = store.load(Date::from_millis(T0), peer.key()).expect("load");
        assert!(matches!(loaded, RoutingTableLoad::Absent));
    }
}
