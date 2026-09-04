//! Bounded validated introducer records (Plan 160 §6).
//!
//! One owner holds the validated introducer set that feeds Plan 159's
//! publication builder. Records carry the v2 fields required for
//! contact: peer identity reference, relay tag, endpoint/family,
//! expiration, and authenticated provenance. Rules:
//!
//! - maximum published introducers is explicit and spec-compatible
//!   ([`MAX_PUBLISHED_INTRODUCERS`] == `MAX_SSU2_INTRODUCERS` == 3);
//! - only live/recent authenticated introducers are chosen;
//! - deterministic replacement/expiry (oldest-expiring evicted first,
//!   stable sort by peer hash for publication order);
//! - stale/failed records are never published;
//! - firewalled SSU2 RouterAddress output consumes this validated set;
//! - direct vs introducer-only publication follows the reachability
//!   state, not a caller boolean (the publication builder enforces
//!   that; this table only supplies validated inputs);
//! - introducer public service remains disabled unless configuration
//!   explicitly enables it (runtime/daemon concern; this table only
//!   stores records the caller authenticated).
//!
//! No sockets, no Tokio, no NetDB mutation, no async.

use std::collections::HashMap;

use i2pr_transport::AddressFamily;
use thiserror::Error;

use crate::address::{IntroKey, Ssu2Endpoint, Ssu2Introducer};
use crate::constants;

/// Maximum introducer records retained by one table.
pub const MAX_INTRODUCER_RECORDS: usize = 8;
/// Maximum introducers published in one RouterAddress (spec bound).
pub const MAX_PUBLISHED_INTRODUCERS: usize = constants::MAX_SSU2_INTRODUCERS;
/// Default introducer record lifetime in seconds.
pub const INTRODUCER_RECORD_LIFETIME_SECS: u64 = 600;

/// Where one introducer record came from (authenticated provenance).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntroducerProvenance {
    /// Direct authenticated session with the introducer.
    AuthenticatedDirect,
    /// Corroborated peer-test confirmation involving the introducer.
    PeerTestConfirmed,
    /// Successful relay use through the introducer.
    RelaySuccess,
}

/// Typed introducer-record failures.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum IntroducerError {
    /// The tag is zero (reserved).
    #[error("SSU2 introducer relay tag is zero")]
    ZeroTag,
    /// The table already retains its ceiling.
    #[error("SSU2 introducer table is full")]
    TableFull,
    /// No live record matches the key.
    #[error("SSU2 introducer record is unknown")]
    Unknown,
    /// The record expired before use.
    #[error("SSU2 introducer record expired")]
    Expired,
}

/// One bounded validated introducer record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntroducerRecord {
    peer_hash: [u8; 32],
    endpoint: Ssu2Endpoint,
    intro_key: IntroKey,
    relay_tag: u32,
    expires_secs: u64,
    provenance: IntroducerProvenance,
}

impl IntroducerRecord {
    /// Creates a record after validating the nonzero tag.
    pub fn new(
        peer_hash: [u8; 32],
        endpoint: Ssu2Endpoint,
        intro_key: IntroKey,
        relay_tag: u32,
        expires_secs: u64,
        provenance: IntroducerProvenance,
    ) -> Result<Self, IntroducerError> {
        if relay_tag == 0 {
            return Err(IntroducerError::ZeroTag);
        }
        Ok(Self {
            peer_hash,
            endpoint,
            intro_key,
            relay_tag,
            expires_secs,
            provenance,
        })
    }

    /// Returns the peer identity reference.
    pub const fn peer_hash(&self) -> &[u8; 32] {
        &self.peer_hash
    }

    /// Returns the introducer endpoint.
    pub const fn endpoint(&self) -> Ssu2Endpoint {
        self.endpoint
    }

    /// Returns the introducer intro key.
    pub const fn intro_key(&self) -> IntroKey {
        self.intro_key
    }

    /// Returns the nonzero relay tag.
    pub const fn relay_tag(&self) -> u32 {
        self.relay_tag
    }

    /// Returns the wall-clock expiry second.
    pub const fn expires_secs(&self) -> u64 {
        self.expires_secs
    }

    /// Returns the authenticated provenance.
    pub const fn provenance(&self) -> IntroducerProvenance {
        self.provenance
    }

    /// Returns the address family.
    pub const fn family(&self) -> AddressFamily {
        self.endpoint.family()
    }

    /// Returns whether the record expired at `now_secs`.
    pub const fn is_expired(&self, now_secs: u64) -> bool {
        now_secs >= self.expires_secs
    }

    fn key(&self) -> ([u8; 32], u32) {
        (self.peer_hash, self.relay_tag)
    }
}

/// The single bounded validated introducer-record owner.
///
/// `Debug` is redacted (counts only): records carry peer hashes,
/// endpoints, intro keys, and relay tags (Plan 160 §12).
#[derive(Clone, Default)]
pub struct IntroducerTable {
    records: HashMap<([u8; 32], u32), IntroducerRecord>,
}

impl std::fmt::Debug for IntroducerTable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IntroducerTable")
            .field("records", &self.records.len())
            .finish()
    }
}

impl IntroducerTable {
    /// Creates an empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of retained records (live + unreaped).
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns whether no records are retained.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Inserts or refreshes one authenticated record.
    ///
    /// Deterministic replacement: a full table evicts the
    /// oldest-expiring record (ties break by peer hash, then tag) so
    /// publication order stays stable for identical inputs.
    pub fn insert(
        &mut self,
        record: IntroducerRecord,
        now_secs: u64,
    ) -> Result<(), IntroducerError> {
        self.expire_locked(now_secs);
        let key = record.key();
        if let std::collections::hash_map::Entry::Occupied(mut e) = self.records.entry(key) {
            e.insert(record);
            return Ok(());
        }
        if self.records.len() >= MAX_INTRODUCER_RECORDS {
            let oldest = self
                .records
                .iter()
                .min_by(|left, right| {
                    left.1
                        .expires_secs()
                        .cmp(&right.1.expires_secs())
                        .then_with(|| left.0.0.cmp(&right.0.0))
                        .then_with(|| left.0.1.cmp(&right.0.1))
                })
                .map(|(key, _)| *key)
                .ok_or(IntroducerError::TableFull)?;
            self.records.remove(&oldest);
        }
        self.records.insert(key, record);
        Ok(())
    }

    /// Removes records for a failed introducer peer (never publish
    /// stale/failed records).
    pub fn remove_peer(&mut self, peer_hash: &[u8; 32]) -> usize {
        let keys: Vec<([u8; 32], u32)> = self
            .records
            .keys()
            .filter(|(hash, _)| hash == peer_hash)
            .copied()
            .collect();
        let removed = keys.len();
        for key in keys {
            self.records.remove(&key);
        }
        removed
    }

    /// Selects up to [`MAX_PUBLISHED_INTRODUCERS`] live records for
    /// publication, oldest-expiring first with deterministic tie-breaks.
    /// Expired records are reaped and never returned.
    pub fn select_live(&mut self, now_secs: u64) -> Vec<IntroducerRecord> {
        self.expire_locked(now_secs);
        let mut live: Vec<IntroducerRecord> = self.records.values().copied().collect();
        live.sort_by(|left, right| {
            left.expires_secs()
                .cmp(&right.expires_secs())
                .then_with(|| left.peer_hash.cmp(&right.peer_hash))
                .then_with(|| left.relay_tag.cmp(&right.relay_tag))
        });
        live.truncate(MAX_PUBLISHED_INTRODUCERS);
        live
    }

    /// Converts live records into validated [`Ssu2Introducer`] values
    /// for the Plan 159 publication builder.
    pub fn validated_introducers(&mut self, now_secs: u64) -> Vec<Ssu2Introducer> {
        self.select_live(now_secs)
            .into_iter()
            .filter_map(|record| {
                Ssu2Introducer::new(record.endpoint, record.intro_key, record.relay_tag).ok()
            })
            .collect()
    }

    /// Expires stale records, returning their peer hashes for
    /// diagnostics (counts only downstream).
    pub fn poll_expired(&mut self, now_secs: u64) -> Vec<[u8; 32]> {
        let expired: Vec<([u8; 32], u32)> = self
            .records
            .iter()
            .filter(|(_, record)| record.is_expired(now_secs))
            .map(|(key, _)| *key)
            .collect();
        let hashes: Vec<[u8; 32]> = expired.iter().map(|(hash, _)| *hash).collect();
        for key in &expired {
            self.records.remove(key);
        }
        hashes
    }

    /// Clears all records (shutdown baseline).
    pub fn clear(&mut self) {
        self.records.clear();
    }

    fn expire_locked(&mut self, now_secs: u64) {
        let expired: Vec<([u8; 32], u32)> = self
            .records
            .iter()
            .filter(|(_, record)| record.is_expired(now_secs))
            .map(|(key, _)| *key)
            .collect();
        for key in &expired {
            self.records.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::net::{IpAddr, Ipv4Addr};

    fn endpoint(last: u8, port: u16) -> Ssu2Endpoint {
        Ssu2Endpoint::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, last)), port).expect("endpoint")
    }

    fn key(byte: u8) -> IntroKey {
        IntroKey::new([byte; 32]).expect("test intro key")
    }

    fn record(peer: u8, tag: u32, expires: u64) -> IntroducerRecord {
        IntroducerRecord::new(
            [peer; 32],
            endpoint(peer, 10000 + u16::from(peer)),
            key(peer),
            tag,
            expires,
            IntroducerProvenance::AuthenticatedDirect,
        )
        .expect("record")
    }

    #[test]
    fn live_selection_is_bounded_deterministic_and_expiry_withdraws() {
        let mut table = IntroducerTable::new();
        for peer in 1..=5_u8 {
            table
                .insert(record(peer, 100 + u32::from(peer), 1000), 0)
                .expect("insert");
        }
        let live = table.select_live(100);
        assert_eq!(live.len(), MAX_PUBLISHED_INTRODUCERS);
        // Deterministic: identical inputs select identically.
        let mut second = table.clone();
        assert_eq!(second.select_live(100), live);
        // Expiry withdraws: past the lifetime nothing is published.
        assert!(table.select_live(1000).is_empty());
        assert!(table.is_empty());
    }

    #[test]
    fn failed_peers_never_publish() {
        let mut table = IntroducerTable::new();
        table.insert(record(1, 7, 1000), 0).expect("insert");
        table.insert(record(2, 8, 1000), 0).expect("insert");
        assert_eq!(table.remove_peer(&[1; 32]), 1);
        let live = table.select_live(100);
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].peer_hash(), &[2; 32]);
    }

    #[test]
    fn zero_tag_and_overflow_evict_oldest() {
        assert_eq!(
            IntroducerRecord::new(
                [1; 32],
                endpoint(1, 1000),
                key(1),
                0,
                1000,
                IntroducerProvenance::AuthenticatedDirect
            ),
            Err(IntroducerError::ZeroTag)
        );
        let mut table = IntroducerTable::new();
        for peer in 1..=MAX_INTRODUCER_RECORDS as u8 {
            table
                .insert(
                    record(peer, 100 + u32::from(peer), 1000 + u64::from(peer)),
                    0,
                )
                .expect("insert");
        }
        // One more evicts the oldest-expiring (peer 1).
        table
            .insert(record(0xFF, 999, 2000), 0)
            .expect("overflow insert");
        assert_eq!(table.len(), MAX_INTRODUCER_RECORDS);
        assert_eq!(table.remove_peer(&[1; 32]), 0);
    }

    #[test]
    fn validated_conversion_feeds_publication_shapes() {
        let mut table = IntroducerTable::new();
        table.insert(record(1, 7, 1000), 0).expect("insert");
        let validated = table.validated_introducers(100);
        assert_eq!(validated.len(), 1);
        assert_eq!(validated[0].relay_tag(), 7);
    }
}
