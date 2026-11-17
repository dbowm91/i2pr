//! Plan 489 §3-§5: the floodfill-indexed Kademlia routing table.
//!
//! The table replaces the flat, undifferentiated peer map with a
//! bounded, day-keyed Kademlia structure over the **floodfill set
//! only**. Three properties make it different from the Plan 103
//! [`RouterInfoStore`](crate::store::RouterInfoStore):
//!
//! 1. **Only floodfills are indexed.** A peer whose own signed
//!    `RouterInfo` does not advertise the `f` capability is rejected
//!    and counted, even when it is otherwise well connected. A peer
//!    that is *suggested* by a verified floodfill enters the table with
//!    weaker evidence and is promoted to signed evidence as soon as
//!    its own `RouterInfo` is resolved.
//! 2. **The keyspace is date-keyed.** Every peer's position in the
//!    keyspace is `SHA256(router_hash || UTC_yyyyMMdd)` — the same
//!    daily routing key the lookup path already uses. The table
//!    therefore rotates with the UTC day.
//! 3. **The table is bounded.** Total entries, per-bucket entries and
//!    the persisted projection all have hard ceilings. Eviction is
//!    deterministic: the peer furthest from the anchor leaves first,
//!    with the highest `RouterHash` breaking a distance tie.
//!
//! The table is **runtime-neutral**: it performs no I/O, owns no
//! socket and reads no clock. The caller supplies the anchor, the
//! date and — for persistence — the projection bytes.
//!
//! ## The anchor
//!
//! The persistent partition is anchored at the router's own position
//! in today's keyspace (`daily_routing_key(local_router_hash, date)`).
//! Bucket `i` holds the peers whose position shares exactly `i`
//! leading bits with the anchor. This is the libp2p-Kademlia shape:
//! a fixed per-day basis, buckets ordered by distance, and a
//! furthest-first eviction that keeps the nearest peers.
//!
//! Selection is *target-relative*: [`BucketSet::select`] returns the
//! peers closest to a supplied routing key, ordered by XOR distance.
//! The anchor only decides how the table is partitioned and which
//! peer is evicted first; it never decides which peer a lookup
//! prefers.

use std::collections::BTreeMap;
use std::fmt;

use i2pr_proto::{Date, Hash};

use crate::router_info::{RouterHash, ValidatedRouterInfo};
use crate::routing::daily_routing_key;
use crate::store::RouterInfoStore;

/// The number of buckets in the table.
///
/// The keyspace is 256 bits wide, so the bucket index is the count of
/// leading zero bits of the XOR distance between a peer's position
/// and the anchor, saturated at `BUCKET_COUNT - 1`.
pub const BUCKET_COUNT: usize = 256;

/// The default maximum number of entries in one bucket.
pub const MAX_BUCKET_SIZE: usize = 8;

/// The default maximum number of entries in the whole table.
pub const MAX_TABLE_SIZE: usize = 1_024;

/// The smallest legal per-bucket capacity. A capacity of zero would
/// make the table unable to hold anything, so it is refused.
pub const MIN_BUCKET_SIZE: usize = 1;

/// The largest legal per-bucket capacity.
pub const MAX_BUCKET_SIZE_LIMIT: usize = 64;

/// The largest legal total table capacity.
pub const MAX_TABLE_SIZE_LIMIT: usize = 16_384;

/// The maximum size of a persisted table projection, in bytes.
pub const MAX_PROJECTION_BYTES: usize = 128 * 1024;

/// The persisted projection format version.
pub const ROUTING_TABLE_PROJECTION_VERSION: u8 = 1;

/// The persisted projection magic prefix.
pub const ROUTING_TABLE_PROJECTION_MAGIC: &[u8; 11] = b"i2pr-rtable";

/// The number of bytes in a persisted projection header
/// (magic + version + day + entry count).
const PROJECTION_HEADER_LEN: usize = 11 + 1 + 8 + 4;

/// The number of bytes in one persisted projection record
/// (router hash + evidence label).
const PROJECTION_RECORD_LEN: usize = 32 + 1;

/// The number of bytes in the persisted projection trailer.
const PROJECTION_CHECKSUM_LEN: usize = 32;

/// The number of milliseconds in one UTC day.
const MILLIS_PER_DAY: u64 = 86_400_000;

/// The floodfill evidence held for one table entry.
///
/// The evidence is a *provenance* record, not a trust signal. A
/// `Signed` entry has been observed to publish its own `f` capability
/// in a validated `RouterInfo`. A `Suggested` entry was named as a
/// routing-table member by a floodfill whose own `f` capability was
/// observed; its own `RouterInfo` has not been resolved yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum FloodfillEvidence {
    /// The peer's own validated `RouterInfo` advertises `f`.
    Signed,
    /// A verified floodfill named this peer as a routing-table member.
    Suggested,
}

impl FloodfillEvidence {
    /// Returns the stable label used in receipts and projections.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Signed => "signed",
            Self::Suggested => "suggested",
        }
    }

    /// Returns the single-byte persisted encoding.
    pub const fn as_byte(self) -> u8 {
        match self {
            Self::Signed => 0,
            Self::Suggested => 1,
        }
    }

    /// Decodes a persisted evidence byte.
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Signed),
            1 => Some(Self::Suggested),
            _ => None,
        }
    }
}

/// One indexed peer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableEntry {
    /// The peer's canonical `RouterHash`.
    pub key: RouterHash,
    /// The peer's position in the current day's keyspace.
    pub position: RouterHash,
    /// The provenance of the peer's floodfill status.
    pub evidence: FloodfillEvidence,
}

/// A table entry selected for a lookup, with its distance to the
/// requested routing key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableCandidate {
    /// The peer's canonical `RouterHash`.
    pub key: RouterHash,
    /// The peer's position in the current day's keyspace.
    pub position: RouterHash,
    /// The XOR distance from the peer's position to the requested
    /// routing key.
    pub distance: [u8; 32],
    /// The provenance of the peer's floodfill status.
    pub evidence: FloodfillEvidence,
}

/// The outcome of an insertion attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableInsertOutcome {
    /// The peer was added to the table.
    Inserted,
    /// An existing entry was replaced by stronger evidence.
    Upgraded,
    /// The peer was already indexed with at least this evidence.
    Idempotent,
    /// The peer's own `RouterInfo` does not advertise `f`.
    RejectedNotFloodfill,
    /// The suggesting peer is not itself a verified floodfill.
    RejectedUnverifiedSource,
    /// The table is at its bound and the peer is not closer to the
    /// anchor than the peer that would have to leave.
    RejectedTableFull,
}

impl TableInsertOutcome {
    /// Returns `true` when the peer entered the table.
    pub const fn is_indexed(self) -> bool {
        matches!(self, Self::Inserted | Self::Upgraded)
    }

    /// Returns `true` when the attempt was refused.
    pub const fn is_rejected(self) -> bool {
        matches!(
            self,
            Self::RejectedNotFloodfill | Self::RejectedUnverifiedSource | Self::RejectedTableFull
        )
    }

    /// Returns the stable label used in receipts.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Inserted => "inserted",
            Self::Upgraded => "upgraded",
            Self::Idempotent => "idempotent",
            Self::RejectedNotFloodfill => "rejected_not_floodfill",
            Self::RejectedUnverifiedSource => "rejected_unverified_source",
            Self::RejectedTableFull => "rejected_table_full",
        }
    }
}

/// The bounded capacities of the table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BucketSetConfig {
    /// The maximum number of entries in the whole table.
    pub max_entries: usize,
    /// The maximum number of entries in one bucket.
    pub max_bucket_size: usize,
}

impl Default for BucketSetConfig {
    fn default() -> Self {
        Self {
            max_entries: MAX_TABLE_SIZE,
            max_bucket_size: MAX_BUCKET_SIZE,
        }
    }
}

/// The typed refusal of an invalid capacity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BucketSetConfigError {
    /// The offending field name.
    pub field: &'static str,
    /// The supplied value.
    pub value: usize,
    /// The inclusive lower bound, when there is one.
    pub minimum: Option<usize>,
    /// The inclusive upper bound, when there is one.
    pub maximum: Option<usize>,
}

impl fmt::Display for BucketSetConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "routing table {} = {} is out of range",
            self.field, self.value
        )?;
        if let Some(minimum) = self.minimum {
            write!(formatter, " (minimum {minimum})")?;
        }
        if let Some(maximum) = self.maximum {
            write!(formatter, " (maximum {maximum})")?;
        }
        Ok(())
    }
}

impl std::error::Error for BucketSetConfigError {}

impl BucketSetConfig {
    /// Validates the capacities.
    pub fn validate(self) -> Result<Self, BucketSetConfigError> {
        if self.max_bucket_size < MIN_BUCKET_SIZE {
            return Err(BucketSetConfigError {
                field: "max_bucket_size",
                value: self.max_bucket_size,
                minimum: Some(MIN_BUCKET_SIZE),
                maximum: Some(MAX_BUCKET_SIZE_LIMIT),
            });
        }
        if self.max_bucket_size > MAX_BUCKET_SIZE_LIMIT {
            return Err(BucketSetConfigError {
                field: "max_bucket_size",
                value: self.max_bucket_size,
                minimum: Some(MIN_BUCKET_SIZE),
                maximum: Some(MAX_BUCKET_SIZE_LIMIT),
            });
        }
        if self.max_entries == 0 {
            return Err(BucketSetConfigError {
                field: "max_entries",
                value: self.max_entries,
                minimum: Some(1),
                maximum: Some(MAX_TABLE_SIZE_LIMIT),
            });
        }
        if self.max_entries > MAX_TABLE_SIZE_LIMIT {
            return Err(BucketSetConfigError {
                field: "max_entries",
                value: self.max_entries,
                minimum: Some(1),
                maximum: Some(MAX_TABLE_SIZE_LIMIT),
            });
        }
        Ok(self)
    }
}

/// The result of a day rotation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RotationReport {
    /// The UTC day index before the rotation.
    pub previous_day: u64,
    /// The UTC day index after the rotation.
    pub current_day: u64,
    /// The number of peers whose position was recomputed.
    pub retained: usize,
    /// `true` when the rotation actually happened.
    pub rotated: bool,
}

/// The day-keyed, bounded, floodfill-only routing table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BucketSet {
    /// The bounded capacities.
    config: BucketSetConfig,
    /// The anchor position: the router's own daily routing key.
    anchor: RouterHash,
    /// The UTC day index the positions were computed for.
    day: u64,
    /// The indexed peers, keyed by `RouterHash` for deterministic order.
    entries: BTreeMap<RouterHash, TableEntry>,
    /// The bucket partition: bucket index -> peer keys, closest first.
    buckets: Vec<Vec<RouterHash>>,
}

impl BucketSet {
    /// Constructs an empty table anchored at `anchor` for the UTC day
    /// containing `now`.
    pub fn new(anchor: RouterHash, now: Date, config: BucketSetConfig) -> Self {
        Self {
            config,
            anchor,
            day: day_index(now),
            entries: BTreeMap::new(),
            buckets: vec![Vec::new(); BUCKET_COUNT],
        }
    }

    /// Returns the bounded capacities.
    pub fn config(&self) -> BucketSetConfig {
        self.config
    }

    /// Returns the anchor position.
    pub fn anchor(&self) -> RouterHash {
        self.anchor
    }

    /// Returns the UTC day index the positions were computed for.
    pub fn day(&self) -> u64 {
        self.day
    }

    /// Returns `true` when the table was built for an earlier UTC day.
    ///
    /// A persisted table whose day is not the current day is expired
    /// and must not be resurrected after a restart.
    pub fn is_expired(&self, now: Date) -> bool {
        self.day != day_index(now)
    }

    /// Returns the number of indexed peers.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when the table holds no peer.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the number of non-empty buckets.
    pub fn occupied_buckets(&self) -> usize {
        self.buckets
            .iter()
            .filter(|bucket| !bucket.is_empty())
            .count()
    }

    /// Returns the number of entries in one bucket.
    pub fn bucket_len(&self, index: usize) -> usize {
        self.buckets.get(index).map_or(0, Vec::len)
    }

    /// Returns the number of entries backed by each evidence class:
    /// `(signed, suggested)`.
    pub fn evidence_counts(&self) -> (usize, usize) {
        let mut signed = 0usize;
        let mut suggested = 0usize;
        for entry in self.entries.values() {
            match entry.evidence {
                FloodfillEvidence::Signed => signed += 1,
                FloodfillEvidence::Suggested => suggested += 1,
            }
        }
        (signed, suggested)
    }

    /// Borrows an indexed peer.
    pub fn get(&self, key: &RouterHash) -> Option<&TableEntry> {
        self.entries.get(key)
    }

    /// Returns `true` when the peer is indexed.
    pub fn contains(&self, key: &RouterHash) -> bool {
        self.entries.contains_key(key)
    }

    /// Returns the bucket index of a peer position against the anchor.
    pub fn bucket_index(&self, position: &RouterHash) -> usize {
        bucket_index(&distance(self.anchor, *position))
    }

    /// Indexes a peer whose own validated `RouterInfo` advertises `f`.
    ///
    /// A peer that does not advertise `f` is refused and counted; the
    /// table never indexes a non-floodfill on the strength of its
    /// connectivity alone.
    pub fn insert_signed(
        &mut self,
        validated: &ValidatedRouterInfo,
        now: Date,
    ) -> TableInsertOutcome {
        if !validated.advertises_floodfill() {
            return TableInsertOutcome::RejectedNotFloodfill;
        }
        let key = validated.key();
        let Some(position) = self.position_of(&key, now) else {
            return TableInsertOutcome::RejectedTableFull;
        };
        if let Some(existing) = self.entries.get(&key) {
            return match existing.evidence {
                FloodfillEvidence::Signed => TableInsertOutcome::Idempotent,
                FloodfillEvidence::Suggested => {
                    self.entries.insert(
                        key,
                        TableEntry {
                            key,
                            position,
                            evidence: FloodfillEvidence::Signed,
                        },
                    );
                    TableInsertOutcome::Upgraded
                }
            };
        }
        self.insert_new(key, position, FloodfillEvidence::Signed)
    }

    /// Indexes a peer a verified floodfill named as a routing-table
    /// member.
    ///
    /// The suggesting peer must itself be indexed with signed
    /// evidence: a peer that is not a verified floodfill can never
    /// introduce another peer into the table.
    pub fn insert_suggested(
        &mut self,
        key: RouterHash,
        from: &RouterHash,
        now: Date,
    ) -> TableInsertOutcome {
        match self.entries.get(from) {
            Some(entry) if entry.evidence == FloodfillEvidence::Signed => {}
            _ => return TableInsertOutcome::RejectedUnverifiedSource,
        }
        if self.entries.contains_key(&key) {
            return TableInsertOutcome::Idempotent;
        }
        let Some(position) = self.position_of(&key, now) else {
            return TableInsertOutcome::RejectedTableFull;
        };
        self.insert_new(key, position, FloodfillEvidence::Suggested)
    }

    /// Removes a peer from the table.
    ///
    /// The entry accounting and the bucket partition are released
    /// together, so a removed peer leaves no residue in either.
    pub fn remove(&mut self, key: &RouterHash) -> bool {
        let Some(entry) = self.entries.remove(key) else {
            return false;
        };
        let index = self.bucket_index(&entry.position);
        if let Some(bucket) = self.buckets.get_mut(index) {
            bucket.retain(|candidate| candidate != key);
        }
        true
    }

    /// Removes every peer whose resolved `RouterInfo` is known **not**
    /// to advertise `f`.
    ///
    /// A floodfill suggestion is not a licence to keep a peer that has
    /// since been observed to be a non-floodfill. The count of removed
    /// peers is returned so the caller can report it.
    pub fn retain_verified(&mut self, store: &RouterInfoStore) -> usize {
        let mut removed = Vec::new();
        for (key, entry) in &self.entries {
            if entry.evidence != FloodfillEvidence::Signed {
                continue;
            }
            if let Some(record) = store.get(key)
                && !record.advertises_floodfill()
            {
                removed.push(*key);
            }
        }
        for key in &removed {
            self.remove(key);
        }
        removed.len()
    }

    /// Selects the peers closest to `target`, closest first.
    ///
    /// The result is bounded by `count`. Ties on distance are broken
    /// by the lower `RouterHash`, so the order is deterministic and
    /// independent of the table's internal ordering.
    pub fn select(&self, target: &RouterHash, count: usize) -> Vec<TableCandidate> {
        if count == 0 {
            return Vec::new();
        }
        let mut candidates: Vec<TableCandidate> = self
            .entries
            .values()
            .map(|entry| TableCandidate {
                key: entry.key,
                position: entry.position,
                distance: distance(entry.position, *target),
                evidence: entry.evidence,
            })
            .collect();
        candidates.sort_by(|left, right| {
            left.distance
                .cmp(&right.distance)
                .then_with(|| left.key.cmp(&right.key))
        });
        candidates.truncate(count);
        candidates
    }

    /// Rotates the table onto the UTC day containing `now`.
    ///
    /// The peer set survives the rotation — the keyspace rotates, not
    /// the peers — but every position is recomputed against the new
    /// day and the new anchor. The entry count never grows across a
    /// rotation, so a clock jump cannot push the table past its
    /// bound.
    pub fn rotate(&mut self, now: Date, anchor: RouterHash) -> RotationReport {
        let current = day_index(now);
        let previous = self.day;
        if current == previous {
            return RotationReport {
                previous_day: previous,
                current_day: current,
                retained: self.entries.len(),
                rotated: false,
            };
        }
        self.anchor = anchor;
        self.day = current;
        self.buckets = vec![Vec::new(); BUCKET_COUNT];
        let keys: Vec<RouterHash> = self.entries.keys().copied().collect();
        for key in keys {
            let Some(position) = self.position_of(&key, now) else {
                continue;
            };
            if let Some(entry) = self.entries.get_mut(&key) {
                entry.position = position;
            }
            self.rebucket(key, position);
        }
        RotationReport {
            previous_day: previous,
            current_day: current,
            retained: self.entries.len(),
            rotated: true,
        }
    }

    /// Drops every peer and empties the partition.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.buckets = vec![Vec::new(); BUCKET_COUNT];
    }

    /// Encodes the table into a bounded, sanitized projection.
    ///
    /// The projection carries the day it was built for, so a restored
    /// table can be recognised as expired. The digest covers the body
    /// verbatim.
    pub fn encode_projection(&self) -> Vec<u8> {
        let capacity = PROJECTION_HEADER_LEN
            .saturating_add(self.entries.len().saturating_mul(PROJECTION_RECORD_LEN))
            .saturating_add(PROJECTION_CHECKSUM_LEN);
        let mut body = Vec::with_capacity(capacity);
        body.extend_from_slice(ROUTING_TABLE_PROJECTION_MAGIC);
        body.push(ROUTING_TABLE_PROJECTION_VERSION);
        body.extend_from_slice(&self.day.to_be_bytes());
        let count = u32::try_from(self.entries.len()).unwrap_or(u32::MAX);
        body.extend_from_slice(&count.to_be_bytes());
        for entry in self.entries.values() {
            body.extend_from_slice(entry.key.as_bytes());
            body.push(entry.evidence.as_byte());
        }
        let digest = Hash::digest(&body);
        body.extend_from_slice(digest.as_bytes());
        body
    }

    /// Decodes a persisted projection.
    ///
    /// A projection that is malformed, oversized, truncated or
    /// carries a mismatched digest is refused with a typed error; the
    /// caller discards it and counts the discard rather than failing
    /// startup.
    pub fn decode_projection(
        bytes: &[u8],
        now: Date,
        config: BucketSetConfig,
        anchor: RouterHash,
    ) -> Result<Self, RoutingTableProjectionError> {
        let config = config
            .validate()
            .map_err(RoutingTableProjectionError::Config)?;
        if bytes.len() > MAX_PROJECTION_BYTES {
            return Err(RoutingTableProjectionError::TooLarge {
                actual: bytes.len(),
                maximum: MAX_PROJECTION_BYTES,
            });
        }
        if bytes.len() < PROJECTION_HEADER_LEN + PROJECTION_CHECKSUM_LEN {
            return Err(RoutingTableProjectionError::Malformed {
                context: "projection is shorter than its minimum framing",
            });
        }
        if &bytes[..11] != ROUTING_TABLE_PROJECTION_MAGIC {
            return Err(RoutingTableProjectionError::Malformed {
                context: "projection magic does not match",
            });
        }
        if bytes[11] != ROUTING_TABLE_PROJECTION_VERSION {
            return Err(RoutingTableProjectionError::UnsupportedVersion(bytes[10]));
        }
        let mut day_bytes = [0u8; 8];
        day_bytes.copy_from_slice(&bytes[12..20]);
        let day = u64::from_be_bytes(day_bytes);
        let mut count_bytes = [0u8; 4];
        count_bytes.copy_from_slice(&bytes[20..24]);
        let count = u32::from_be_bytes(count_bytes) as usize;
        if count > config.max_entries {
            return Err(RoutingTableProjectionError::TooManyRecords {
                actual: count,
                maximum: config.max_entries,
            });
        }
        let Some(record_bytes) = count.checked_mul(PROJECTION_RECORD_LEN) else {
            return Err(RoutingTableProjectionError::Malformed {
                context: "projection record count overflows the length arithmetic",
            });
        };
        let Some(body_len) = PROJECTION_HEADER_LEN.checked_add(record_bytes) else {
            return Err(RoutingTableProjectionError::Malformed {
                context: "projection length overflows the length arithmetic",
            });
        };
        if bytes.len() != body_len + PROJECTION_CHECKSUM_LEN {
            return Err(RoutingTableProjectionError::Malformed {
                context: "projection length does not match its record count",
            });
        }
        let expected = Hash::digest(&bytes[..body_len]);
        if expected.as_bytes() != &bytes[body_len..body_len + PROJECTION_CHECKSUM_LEN] {
            return Err(RoutingTableProjectionError::DigestMismatch);
        }
        let mut table = BucketSet::new(anchor, now, config);
        table.day = day;
        let mut offset = PROJECTION_HEADER_LEN;
        for _ in 0..count {
            let mut key_bytes = [0u8; 32];
            key_bytes.copy_from_slice(&bytes[offset..offset + 32]);
            let key = RouterHash::from_bytes(key_bytes);
            let evidence = FloodfillEvidence::from_byte(bytes[offset + 32]).ok_or(
                RoutingTableProjectionError::UnknownEvidence(bytes[offset + 32]),
            )?;
            offset += PROJECTION_RECORD_LEN;
            if table.entries.contains_key(&key) {
                return Err(RoutingTableProjectionError::Malformed {
                    context: "projection contains a duplicate peer",
                });
            }
            let Some(position) = table.position_of(&key, now) else {
                return Err(RoutingTableProjectionError::Malformed {
                    context: "projection peer cannot be placed in the keyspace",
                });
            };
            table.entries.insert(
                key,
                TableEntry {
                    key,
                    position,
                    evidence,
                },
            );
            table.rebucket(key, position);
        }
        Ok(table)
    }

    /// Computes a peer's position for the table's current day.
    fn position_of(&self, key: &RouterHash, now: Date) -> Option<RouterHash> {
        let hash = daily_routing_key(key, now).ok()?;
        Some(RouterHash::from_hash(hash))
    }

    /// Inserts a peer that is not already indexed.
    fn insert_new(
        &mut self,
        key: RouterHash,
        position: RouterHash,
        evidence: FloodfillEvidence,
    ) -> TableInsertOutcome {
        let new_distance = distance(self.anchor, position);
        if self.entries.len() >= self.config.max_entries {
            // The table is full. The peer that leaves is the one
            // furthest from the anchor; the highest `RouterHash`
            // breaks a distance tie. A peer that is not strictly
            // closer than the departing one is refused instead of
            // displacing a better neighbour.
            let Some(worst) = self.worst_entry() else {
                return TableInsertOutcome::RejectedTableFull;
            };
            if new_distance >= worst.distance {
                return TableInsertOutcome::RejectedTableFull;
            }
            self.remove(&worst.key);
        }
        let index = self.bucket_index(&position);
        if let Some(bucket) = self.buckets.get(index)
            && bucket.len() >= self.config.max_bucket_size
        {
            let Some(worst) = self.bucket_worst(index) else {
                return TableInsertOutcome::RejectedTableFull;
            };
            let worst_distance = distance(self.anchor, worst.position);
            if new_distance >= worst_distance {
                return TableInsertOutcome::RejectedTableFull;
            }
            self.remove(&worst.key);
        }
        self.entries.insert(
            key,
            TableEntry {
                key,
                position,
                evidence,
            },
        );
        self.rebucket(key, position);
        TableInsertOutcome::Inserted
    }

    /// Places a peer in its bucket, keeping the bucket ordered by
    /// distance to the anchor.
    fn rebucket(&mut self, key: RouterHash, position: RouterHash) {
        let index = self.bucket_index(&position);
        let anchor = self.anchor;
        let Some(bucket) = self.buckets.get_mut(index) else {
            return;
        };
        if bucket.contains(&key) {
            return;
        }
        bucket.push(key);
        let entries = &self.entries;
        bucket.sort_by(|left, right| {
            let left_distance = entries
                .get(left)
                .map(|entry| distance(anchor, entry.position))
                .unwrap_or([0u8; 32]);
            let right_distance = entries
                .get(right)
                .map(|entry| distance(anchor, entry.position))
                .unwrap_or([0u8; 32]);
            left_distance
                .cmp(&right_distance)
                .then_with(|| left.cmp(right))
        });
    }

    /// Returns the entry furthest from the anchor.
    fn worst_entry(&self) -> Option<TableCandidate> {
        self.entries
            .values()
            .map(|entry| TableCandidate {
                key: entry.key,
                position: entry.position,
                distance: distance(self.anchor, entry.position),
                evidence: entry.evidence,
            })
            .max_by(|left, right| {
                left.distance
                    .cmp(&right.distance)
                    .then_with(|| right.key.cmp(&left.key))
            })
    }

    /// Returns the entry furthest from the anchor inside one bucket.
    fn bucket_worst(&self, index: usize) -> Option<TableCandidate> {
        self.buckets
            .get(index)?
            .iter()
            .filter_map(|key| self.entries.get(key))
            .map(|entry| TableCandidate {
                key: entry.key,
                position: entry.position,
                distance: distance(self.anchor, entry.position),
                evidence: entry.evidence,
            })
            .max_by(|left, right| {
                left.distance
                    .cmp(&right.distance)
                    .then_with(|| right.key.cmp(&left.key))
            })
    }
}

/// The typed refusal of an invalid persisted projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingTableProjectionError {
    /// The projection is larger than the permitted ceiling.
    TooLarge {
        /// The supplied size.
        actual: usize,
        /// The permitted ceiling.
        maximum: usize,
    },
    /// The projection framing is malformed.
    Malformed {
        /// A stable description of the defect.
        context: &'static str,
    },
    /// The projection carries an unsupported format version.
    UnsupportedVersion(u8),
    /// The projection digest does not match its body.
    DigestMismatch,
    /// The projection carries more records than the table may hold.
    TooManyRecords {
        /// The supplied record count.
        actual: usize,
        /// The permitted ceiling.
        maximum: usize,
    },
    /// The projection carries an unknown evidence label.
    UnknownEvidence(u8),
    /// The capacities supplied to the decoder are invalid.
    Config(BucketSetConfigError),
}

impl fmt::Display for RoutingTableProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { actual, maximum } => write!(
                formatter,
                "routing table projection is {actual} bytes, above the {maximum} byte ceiling"
            ),
            Self::Malformed { context } => {
                write!(formatter, "malformed routing table projection: {context}")
            }
            Self::UnsupportedVersion(version) => {
                write!(
                    formatter,
                    "unsupported routing table projection version {version}"
                )
            }
            Self::DigestMismatch => {
                formatter.write_str("routing table projection digest does not match its body")
            }
            Self::TooManyRecords { actual, maximum } => write!(
                formatter,
                "routing table projection carries {actual} records, above the {maximum} record ceiling"
            ),
            Self::UnknownEvidence(byte) => {
                write!(formatter, "unknown routing table evidence label {byte}")
            }
            Self::Config(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for RoutingTableProjectionError {}

/// Returns the UTC day index containing `now`.
fn day_index(now: Date) -> u64 {
    now.as_millis() / MILLIS_PER_DAY
}

/// Returns the XOR distance between two positions.
fn distance(left: RouterHash, right: RouterHash) -> [u8; 32] {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let mut out = [0u8; 32];
    for index in 0..32 {
        out[index] = left[index] ^ right[index];
    }
    out
}

/// Returns the number of leading zero bits of a distance, saturated at
/// `BUCKET_COUNT - 1`.
fn leading_zeros(distance: &[u8; 32]) -> usize {
    let mut count = 0usize;
    for byte in distance {
        if *byte == 0 {
            count += 8;
            continue;
        }
        count += byte.leading_zeros() as usize;
        break;
    }
    count.min(BUCKET_COUNT - 1)
}

/// Returns the bucket index of a distance.
fn bucket_index(distance: &[u8; 32]) -> usize {
    leading_zeros(distance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router_info::ValidationContext;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_proto::Mapping;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn day_at(days: u64) -> Date {
        Date::from_millis(days * MILLIS_PER_DAY + 3_600_000)
    }

    fn signer(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn validated(bundle: &RouterIdentityBundle, caps: &str) -> ValidatedRouterInfo {
        let mut options = Mapping::builder();
        options.insert("caps".to_owned(), caps.to_owned()).unwrap();
        let published = day_at(20_000);
        let info = bundle
            .sign_router_info(published, Vec::new(), Vec::new(), options.build().unwrap())
            .expect("sign");
        ValidatedRouterInfo::from_router_info(info, None, ValidationContext::new(published))
            .expect("validate")
    }

    fn floodfill(seed: u64) -> ValidatedRouterInfo {
        validated(&signer(seed), "Nf")
    }

    fn non_floodfill(seed: u64) -> ValidatedRouterInfo {
        validated(&signer(seed), "N")
    }

    fn anchor() -> RouterHash {
        floodfill(0x4890_0001).key()
    }

    fn new_table(config: BucketSetConfig) -> BucketSet {
        BucketSet::new(anchor(), day_at(20_000), config)
    }

    #[test]
    fn a_signed_floodfill_is_indexed() {
        let mut table = new_table(BucketSetConfig::default());
        let peer = floodfill(0x4890_0010);
        assert_eq!(
            table.insert_signed(&peer, day_at(20_000)),
            TableInsertOutcome::Inserted
        );
        assert_eq!(table.len(), 1);
        assert!(table.contains(&peer.key()));
        assert_eq!(
            table.get(&peer.key()).map(|entry| entry.evidence),
            Some(FloodfillEvidence::Signed)
        );
    }

    #[test]
    fn a_non_floodfill_is_rejected_and_counted() {
        let mut table = new_table(BucketSetConfig::default());
        let peer = non_floodfill(0x4890_0011);
        assert_eq!(
            table.insert_signed(&peer, day_at(20_000)),
            TableInsertOutcome::RejectedNotFloodfill
        );
        assert_eq!(table.len(), 0);
        assert!(table.buckets.iter().all(Vec::is_empty));
    }

    #[test]
    fn a_suggestion_needs_a_verified_source() {
        let mut table = new_table(BucketSetConfig::default());
        let source = floodfill(0x4890_0012);
        table.insert_signed(&source, day_at(20_000));
        let target = floodfill(0x4890_0013).key();

        let stranger = floodfill(0x4890_0014).key();
        assert_eq!(
            table.insert_suggested(target, &stranger, day_at(20_000)),
            TableInsertOutcome::RejectedUnverifiedSource
        );
        assert_eq!(table.len(), 1);

        assert_eq!(
            table.insert_suggested(target, &source.key(), day_at(20_000)),
            TableInsertOutcome::Inserted
        );
        assert_eq!(table.len(), 2);
        assert_eq!(
            table.get(&target).map(|entry| entry.evidence),
            Some(FloodfillEvidence::Suggested)
        );
    }

    #[test]
    fn a_suggestion_is_upgraded_by_its_own_signed_evidence() {
        let mut table = new_table(BucketSetConfig::default());
        let source = floodfill(0x4890_0015);
        table.insert_signed(&source, day_at(20_000));
        let peer = floodfill(0x4890_0016);
        table.insert_suggested(peer.key(), &source.key(), day_at(20_000));
        assert_eq!(
            table.insert_signed(&peer, day_at(20_000)),
            TableInsertOutcome::Upgraded
        );
        assert_eq!(
            table.get(&peer.key()).map(|entry| entry.evidence),
            Some(FloodfillEvidence::Signed)
        );
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn the_table_honours_its_total_bound() {
        let mut table = new_table(BucketSetConfig {
            max_entries: 4,
            max_bucket_size: 4,
        });
        for index in 0..4 {
            let peer = floodfill(0x4890_0020 + index);
            assert_eq!(
                table.insert_signed(&peer, day_at(20_000)),
                TableInsertOutcome::Inserted
            );
        }
        assert_eq!(table.len(), 4);
        let extra = floodfill(0x4890_0030);
        let outcome = table.insert_signed(&extra, day_at(20_000));
        assert!(outcome.is_rejected() || outcome.is_indexed());
        assert!(table.len() <= 4, "the table grew past its bound");
    }

    #[test]
    fn the_table_honours_its_bucket_bound() {
        let mut table = new_table(BucketSetConfig {
            max_entries: 64,
            max_bucket_size: 1,
        });
        table.insert_signed(&floodfill(0x4890_0040), day_at(20_000));
        table.insert_signed(&floodfill(0x4890_0041), day_at(20_000));
        assert!(table.len() <= 64);
        for index in 0..BUCKET_COUNT {
            assert!(table.bucket_len(index) <= 1, "bucket {index} overflowed");
        }
    }

    #[test]
    fn eviction_is_deterministic() {
        let peers: Vec<ValidatedRouterInfo> =
            (0..3).map(|index| floodfill(0x4890_0050 + index)).collect();
        let mut table = new_table(BucketSetConfig {
            max_entries: 2,
            max_bucket_size: 2,
        });
        for peer in &peers {
            table.insert_signed(peer, day_at(20_000));
        }
        let first_run: Vec<RouterHash> = table
            .select(&anchor(), 8)
            .into_iter()
            .map(|candidate| candidate.key)
            .collect();

        let mut replay = new_table(BucketSetConfig {
            max_entries: 2,
            max_bucket_size: 2,
        });
        for peer in &peers {
            replay.insert_signed(peer, day_at(20_000));
        }
        let second_run: Vec<RouterHash> = replay
            .select(&anchor(), 8)
            .into_iter()
            .map(|candidate| candidate.key)
            .collect();
        assert_eq!(first_run, second_run);
    }

    #[test]
    fn removal_releases_the_bucket_residue() {
        let mut table = new_table(BucketSetConfig::default());
        let peer = floodfill(0x4890_0060);
        table.insert_signed(&peer, day_at(20_000));
        assert_eq!(table.occupied_buckets(), 1);
        assert!(table.remove(&peer.key()));
        assert_eq!(table.len(), 0);
        assert_eq!(table.occupied_buckets(), 0);
        assert!(table.buckets.iter().all(Vec::is_empty));
        assert!(!table.remove(&peer.key()));
    }

    #[test]
    fn selection_is_closest_first_and_bounded() {
        let mut table = new_table(BucketSetConfig::default());
        let peers: Vec<ValidatedRouterInfo> =
            (0..6).map(|index| floodfill(0x4890_0070 + index)).collect();
        for peer in &peers {
            table.insert_signed(peer, day_at(20_000));
        }
        let target = peers[3].key();
        let selected = table.select(&target, 3);
        assert_eq!(selected.len(), 3);
        for window in selected.windows(2) {
            assert!(
                window[0].distance <= window[1].distance,
                "selection was not closest-first"
            );
        }
    }

    #[test]
    fn the_day_rotation_recomputes_positions_without_growing_the_table() {
        let mut table = new_table(BucketSetConfig::default());
        let peer = floodfill(0x4890_0080);
        table.insert_signed(&peer, day_at(20_000));
        let before = table.len();
        let position_before = table.get(&peer.key()).map(|entry| entry.position);
        let report = table.rotate(day_at(20_001), anchor());
        assert!(report.rotated);
        assert_eq!(report.retained, before);
        assert_eq!(table.len(), before);
        assert_eq!(table.day(), day_index(day_at(20_001)));
        assert_eq!(table.occupied_buckets(), 1);
        let position_after = table.get(&peer.key()).map(|entry| entry.position);
        assert_ne!(
            position_before, position_after,
            "the position must follow the daily key"
        );
    }

    #[test]
    fn the_day_rotation_is_a_no_op_inside_one_day() {
        let mut table = new_table(BucketSetConfig::default());
        let peer = floodfill(0x4890_0081);
        table.insert_signed(&peer, day_at(20_000));
        let report = table.rotate(day_at(20_000), anchor());
        assert!(!report.rotated);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn an_expired_table_is_recognised() {
        let table = new_table(BucketSetConfig::default());
        assert!(!table.is_expired(day_at(20_000)));
        assert!(table.is_expired(day_at(20_001)));
    }

    #[test]
    fn the_projection_round_trips() {
        let mut table = new_table(BucketSetConfig::default());
        let source = floodfill(0x4890_0090);
        table.insert_signed(&source, day_at(20_000));
        let suggested = floodfill(0x4890_0091).key();
        table.insert_suggested(suggested, &source.key(), day_at(20_000));

        let bytes = table.encode_projection();
        let restored = BucketSet::decode_projection(
            &bytes,
            day_at(20_000),
            BucketSetConfig::default(),
            anchor(),
        )
        .expect("projection decodes");
        assert_eq!(restored.len(), table.len());
        assert_eq!(restored.day(), table.day());
        for (key, entry) in &table.entries {
            let restored_entry = restored.get(key).expect("entry restored");
            assert_eq!(restored_entry.evidence, entry.evidence);
            assert_eq!(restored_entry.position, entry.position);
        }
    }

    #[test]
    fn a_corrupt_projection_is_refused() {
        let mut table = new_table(BucketSetConfig::default());
        table.insert_signed(&floodfill(0x4890_00a0), day_at(20_000));
        let mut bytes = table.encode_projection();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        assert_eq!(
            BucketSet::decode_projection(
                &bytes,
                day_at(20_000),
                BucketSetConfig::default(),
                anchor()
            ),
            Err(RoutingTableProjectionError::DigestMismatch)
        );
    }

    #[test]
    fn a_truncated_projection_is_refused() {
        let mut table = new_table(BucketSetConfig::default());
        table.insert_signed(&floodfill(0x4890_00b0), day_at(20_000));
        let bytes = table.encode_projection();
        let truncated = &bytes[..bytes.len() - 4];
        assert!(matches!(
            BucketSet::decode_projection(
                truncated,
                day_at(20_000),
                BucketSetConfig::default(),
                anchor()
            ),
            Err(RoutingTableProjectionError::Malformed { .. })
        ));
    }

    #[test]
    fn an_oversized_projection_is_refused() {
        let bytes = vec![0u8; MAX_PROJECTION_BYTES + 1];
        assert!(matches!(
            BucketSet::decode_projection(
                &bytes,
                day_at(20_000),
                BucketSetConfig::default(),
                anchor()
            ),
            Err(RoutingTableProjectionError::TooLarge { .. })
        ));
    }

    #[test]
    fn an_invalid_capacity_is_refused() {
        let mut table = new_table(BucketSetConfig::default());
        table.insert_signed(&floodfill(0x4890_00c0), day_at(20_000));
        let bytes = table.encode_projection();
        assert!(matches!(
            BucketSet::decode_projection(
                &bytes,
                day_at(20_000),
                BucketSetConfig {
                    max_entries: 0,
                    max_bucket_size: 1
                },
                anchor()
            ),
            Err(RoutingTableProjectionError::Config(_))
        ));
    }

    #[test]
    fn the_projection_is_bounded() {
        let mut table = new_table(BucketSetConfig::default());
        for index in 0..64 {
            table.insert_signed(&floodfill(0x4890_0100 + index), day_at(20_000));
        }
        let bytes = table.encode_projection();
        assert!(bytes.len() <= MAX_PROJECTION_BYTES);
    }

    #[test]
    fn a_verified_non_floodfill_is_evicted() {
        let mut table = new_table(BucketSetConfig::default());
        let peer = non_floodfill(0x4890_0200);
        // The peer cannot enter the table on its own evidence.
        assert_eq!(
            table.insert_signed(&peer, day_at(20_000)),
            TableInsertOutcome::RejectedNotFloodfill
        );
        // Nor can an unverified source introduce it.
        let stranger = floodfill(0x4890_0201).key();
        assert_eq!(
            table.insert_suggested(peer.key(), &stranger, day_at(20_000)),
            TableInsertOutcome::RejectedUnverifiedSource
        );
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn the_table_honours_a_capacity_of_one() {
        let mut table = new_table(BucketSetConfig {
            max_entries: 1,
            max_bucket_size: 1,
        });
        table.insert_signed(&floodfill(0x4890_0300), day_at(20_000));
        assert_eq!(table.len(), 1);
        table.insert_signed(&floodfill(0x4890_0301), day_at(20_000));
        assert_eq!(table.len(), 1, "a capacity of one must hold one peer");
        for index in 0..BUCKET_COUNT {
            assert!(table.bucket_len(index) <= 1);
        }
    }

    #[test]
    fn the_table_honours_the_exact_load() {
        let mut table = new_table(BucketSetConfig {
            max_entries: 3,
            max_bucket_size: 3,
        });
        for index in 0..3 {
            let peer = floodfill(0x4890_0310 + index);
            assert_eq!(
                table.insert_signed(&peer, day_at(20_000)),
                TableInsertOutcome::Inserted
            );
        }
        assert_eq!(table.len(), 3, "the exact load must fit");
    }

    #[test]
    fn the_table_refuses_the_max_plus_one() {
        let mut table = new_table(BucketSetConfig {
            max_entries: 3,
            max_bucket_size: 3,
        });
        for index in 0..3 {
            table.insert_signed(&floodfill(0x4890_0320 + index), day_at(20_000));
        }
        let before = table.len();
        let outcome = table.insert_signed(&floodfill(0x4890_0330), day_at(20_000));
        assert_eq!(
            table.len(),
            before,
            "the max+1 peer must never push the table past its bound"
        );
        if outcome.is_rejected() {
            assert_eq!(outcome, TableInsertOutcome::RejectedTableFull);
        }
    }

    #[test]
    fn the_bucket_residue_is_released_on_every_drop_path() {
        let mut table = new_table(BucketSetConfig::default());
        let peer = floodfill(0x4890_0340);
        table.insert_signed(&peer, day_at(20_000));
        assert_eq!(table.occupied_buckets(), 1);

        // Explicit removal.
        assert!(table.remove(&peer.key()));
        assert_eq!(table.occupied_buckets(), 0);
        assert!(table.buckets.iter().all(Vec::is_empty));

        // Rotation rebuilds the partition from scratch.
        table.insert_signed(&peer, day_at(20_000));
        assert_eq!(table.occupied_buckets(), 1);
        table.rotate(day_at(20_001), anchor());
        assert_eq!(table.occupied_buckets(), 1);
        assert_eq!(table.len(), 1, "a rotation keeps the peer");

        // A clear drops the peer and the partition together.
        table.clear();
        assert_eq!(table.len(), 0);
        assert_eq!(table.occupied_buckets(), 0);
        assert!(table.buckets.iter().all(Vec::is_empty));
    }

    #[test]
    fn the_projection_is_byte_identical_across_runs() {
        let mut first = new_table(BucketSetConfig::default());
        let mut second = new_table(BucketSetConfig::default());
        for index in 0..4 {
            let peer = floodfill(0x4890_0350 + index);
            first.insert_signed(&peer, day_at(20_000));
            second.insert_signed(&peer, day_at(20_000));
        }
        assert_eq!(
            first.encode_projection(),
            second.encode_projection(),
            "the same table must encode to the same bytes"
        );
    }

    #[test]
    fn the_bucket_index_saturates() {
        assert_eq!(bucket_index(&[0u8; 32]), BUCKET_COUNT - 1);
        assert_eq!(bucket_index(&[0xffu8; 32]), 0);
    }
}
