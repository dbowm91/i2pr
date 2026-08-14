//! Pure routing primitives used by NetDB selection logic.
//!
//! Plan 103 §5 owns XOR distance, deterministic tie-breaking, floodfill
//! filtering, and bounded nearest-N selection. Plan 105 §1 adds the
//! daily routing-key derivation (`SHA256(search_key || UTC_yyyyMMdd)`)
//! and exposes it as a small, deterministic, timezone-explicit helper.

use i2pr_proto::Hash;
use sha2::{Digest, Sha256};

use crate::router_info::RouterHash;
use crate::store::RouterInfoStore;

/// Format an `i2pr_proto::Date` as the bounded 8-byte ASCII `yyyyMMdd`
/// representation used by the daily routing-key transform.
///
/// The helper does not read the wall clock; the caller is responsible
/// for supplying an I2P `Date` whose `now` was already adapted to
/// UTC. The result is exactly 8 ASCII bytes wide; `Date` values that
/// fall outside the I2P Date range (year 0..=9999) are rejected rather
/// than silently truncated.
pub fn format_daily_key(now: i2pr_proto::Date) -> Result<[u8; 8], RoutingKeyError> {
    let millis = now.as_millis();
    let days = millis / 86_400_000;
    if days > 1_977_337 {
        // 9999-12-31 in proleptic Gregorian days.
        return Err(RoutingKeyError::DateOutOfRange { days });
    }
    let (y, m, d) = civil_from_days(days);
    if !(0..=9999).contains(&y) {
        return Err(RoutingKeyError::DateOutOfRange { days });
    }
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return Err(RoutingKeyError::DateOutOfRange { days });
    }
    let year = y as u16;
    let mut out = [0u8; 8];
    write_two_digits(&mut out[0..2], year / 100)?;
    write_two_digits(&mut out[2..4], year % 100)?;
    write_two_digits(&mut out[4..6], u16::from(m as u8))?;
    write_two_digits(&mut out[6..8], u16::from(d as u8))?;
    Ok(out)
}

/// Compute the daily routing key for a 32-byte search key (typically a
/// RouterHash) and an explicit UTC date.
///
/// The transform is `SHA256(search_key || UTC_yyyyMMdd[8])` exactly as
/// specified by the current I2P NetDB rules. The search key is never
/// modified and the date is never widened to local time.
pub fn daily_routing_key(
    search_key: &RouterHash,
    now: i2pr_proto::Date,
) -> Result<Hash, RoutingKeyError> {
    let date = format_daily_key(now)?;
    let mut hasher = Sha256::new();
    hasher.update(search_key.as_bytes());
    hasher.update(date);
    Ok(Hash::from_bytes(hasher.finalize().into()))
}

/// Diagnostic failure for routing-key derivation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingKeyError {
    /// The supplied `Date` is outside the representable `yyyyMMdd` range.
    DateOutOfRange {
        /// Number of whole days since the Unix epoch.
        days: u64,
    },
}

impl core::fmt::Display for RoutingKeyError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DateOutOfRange { days } => {
                write!(formatter, "date {days} days since epoch is out of range")
            }
        }
    }
}

impl std::error::Error for RoutingKeyError {}

fn write_two_digits(out: &mut [u8], value: u16) -> Result<(), RoutingKeyError> {
    if value >= 100 {
        return Err(RoutingKeyError::DateOutOfRange { days: 0 });
    }
    out[0] = b'0' + (value / 10) as u8;
    out[1] = b'0' + (value % 10) as u8;
    Ok(())
}

/// Days-from-epoch to (year, month, day) using the proleptic Gregorian
/// calendar. The algorithm is the standard Howard Hinnant date
/// conversion that I2P's `yyyyMMdd` transform uses.
fn civil_from_days(days: u64) -> (i64, i64, i64) {
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as i64;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as i64;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Computes the XOR distance between two 32-byte hashes.
///
/// The result is itself a 32-byte value; the caller compares bytes
/// (lexicographically for the Kademlia convention). The function does
/// no allocation and is safe to call from tight loops.
pub fn xor_distance(left: &RouterHash, right: &RouterHash) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = left.as_bytes()[index] ^ right.as_bytes()[index];
    }
    out
}

/// Ordered pair of `(distance, candidate_key)` for nearest-N selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NearestSelection {
    /// The XOR distance to the target.
    pub distance: [u8; 32],
    /// The candidate's RouterHash.
    pub key: RouterHash,
}

impl Ord for NearestSelection {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // Sort by distance ascending; RouterHash ascending is the
        // deterministic tie-breaker.
        self.distance
            .cmp(&other.distance)
            .then(self.key.cmp(&other.key))
    }
}

impl PartialOrd for NearestSelection {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Returns the `n` nearest records to `target`, restricted to records
/// that advertise floodfill capability.
///
/// `n` is bounded by the number of records in `store`; the function
/// never returns more than `n` entries. When `n` is zero the result is
/// always empty.
#[allow(dead_code)]
pub fn nearest_floodfill(
    store: &RouterInfoStore,
    target: &RouterHash,
    n: usize,
) -> Vec<NearestSelection> {
    if n == 0 {
        return Vec::new();
    }
    let mut candidates: Vec<NearestSelection> = store
        .floodfill_advertisers()
        .map(|record| NearestSelection {
            distance: xor_distance(&record.key(), target),
            key: record.key(),
        })
        .collect();
    candidates.sort();
    candidates.truncate(n);
    candidates
}

/// Returns the `n` nearest records to `target` without filtering by
/// capability. The same distance + RouterHash tie-break applies.
#[allow(dead_code)]
pub fn nearest(store: &RouterInfoStore, target: &RouterHash, n: usize) -> Vec<NearestSelection> {
    if n == 0 {
        return Vec::new();
    }
    let mut candidates: Vec<NearestSelection> = store
        .iter()
        .map(|(key, _record)| NearestSelection {
            distance: xor_distance(key, target),
            key: *key,
        })
        .collect();
    candidates.sort();
    candidates.truncate(n);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router_info::{ValidatedRouterInfo, ValidationContext, router_hash};
    use crate::store::RouterInfoStore;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_proto::Mapping;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn validate(b: &RouterIdentityBundle, published_ms: u64) -> ValidatedRouterInfo {
        let info = b
            .sign_router_info(
                i2pr_proto::Date::from_millis(published_ms),
                Vec::new(),
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign");
        ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(i2pr_proto::Date::from_millis(published_ms)),
        )
        .expect("validate")
    }

    fn caps(b: &RouterIdentityBundle, published_ms: u64, caps: &str) -> ValidatedRouterInfo {
        let mut options = Mapping::builder();
        options.insert("caps".to_owned(), caps.to_owned()).unwrap();
        let info = b
            .sign_router_info(
                i2pr_proto::Date::from_millis(published_ms),
                Vec::new(),
                Vec::new(),
                options.build().unwrap(),
            )
            .expect("sign");
        ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(i2pr_proto::Date::from_millis(published_ms)),
        )
        .expect("validate")
    }

    #[test]
    fn xor_distance_is_symmetric_and_zero_for_identical_keys() {
        let key_a = RouterHash::from_bytes([0x11u8; 32]);
        let key_b = RouterHash::from_bytes([0x22u8; 32]);
        let ab = xor_distance(&key_a, &key_b);
        let ba = xor_distance(&key_b, &key_a);
        assert_eq!(ab, ba);
        let zero = xor_distance(&key_a, &key_a);
        assert_eq!(zero, [0u8; 32]);
    }

    #[test]
    fn nearest_returns_lexicographically_ordered_distances() {
        let signer_a = bundle(0x300);
        let signer_b = bundle(0x301);
        let signer_c = bundle(0x302);
        let mut store = RouterInfoStore::default();
        store.insert(validate(&signer_a, 1));
        store.insert(validate(&signer_b, 1));
        store.insert(validate(&signer_c, 1));
        let target = RouterHash::from_bytes([0u8; 32]);
        let nearest_keys = nearest(&store, &target, 3);
        assert_eq!(nearest_keys.len(), 3);
        for window in nearest_keys.windows(2) {
            assert!(window[0].distance <= window[1].distance);
        }
    }

    #[test]
    fn nearest_floodfill_filters_to_capability_advertisers() {
        let floodfill = bundle(0x303);
        let plain = bundle(0x304);
        let mut store = RouterInfoStore::default();
        store.insert(caps(&floodfill, 1, "f"));
        store.insert(caps(&plain, 1, "L"));
        let target = RouterHash::from_bytes([0u8; 32]);
        let advertisers = nearest_floodfill(&store, &target, 10);
        assert_eq!(advertisers.len(), 1);
        let expected = router_hash(floodfill.identity()).expect("floodfill hash");
        assert_eq!(advertisers[0].key, expected);
    }

    #[test]
    fn nearest_with_zero_count_is_empty() {
        let signer = bundle(0x305);
        let mut store = RouterInfoStore::default();
        store.insert(validate(&signer, 1));
        let target = RouterHash::from_bytes([0u8; 32]);
        assert!(nearest(&store, &target, 0).is_empty());
        assert!(nearest_floodfill(&store, &target, 0).is_empty());
    }

    #[test]
    fn nearest_respects_n_smaller_than_store() {
        let signer_a = bundle(0x306);
        let signer_b = bundle(0x307);
        let signer_c = bundle(0x308);
        let mut store = RouterInfoStore::default();
        store.insert(validate(&signer_a, 1));
        store.insert(validate(&signer_b, 1));
        store.insert(validate(&signer_c, 1));
        let target = RouterHash::from_bytes([0u8; 32]);
        assert_eq!(nearest(&store, &target, 2).len(), 2);
    }

    #[test]
    fn daily_key_format_includes_leading_zero_padding() {
        // 2024-01-02 00:00:00 UTC.
        let ms = 1_704_153_600_000;
        let formatted = format_daily_key(i2pr_proto::Date::from_millis(ms)).expect("date");
        assert_eq!(formatted, *b"20240102");
    }

    #[test]
    fn daily_key_format_handles_well_known_unix_epoch() {
        // 1970-01-01 is day 0.
        let formatted = format_daily_key(i2pr_proto::Date::from_millis(0)).expect("date");
        assert_eq!(formatted, *b"19700101");
    }

    #[test]
    fn daily_key_format_uses_utc_not_local() {
        // We never invoke a wall clock; the same `Date` always maps
        // to the same 8-byte representation regardless of the host
        // timezone. We assert that by computing the same date and
        // verifying the bytes are deterministic.
        let formatted = format_daily_key(i2pr_proto::Date::from_millis(0)).expect("date");
        assert_eq!(formatted, *b"19700101");
    }

    #[test]
    fn daily_key_changes_when_day_boundary_crosses() {
        let search_key = RouterHash::from_bytes([0xabu8; 32]);
        let day_a = daily_routing_key(&search_key, i2pr_proto::Date::from_millis(86_400_000))
            .expect("day a");
        let day_b = daily_routing_key(&search_key, i2pr_proto::Date::from_millis(86_400_000 + 1))
            .expect("day b");
        let next_day =
            daily_routing_key(&search_key, i2pr_proto::Date::from_millis(86_400_000 * 2))
                .expect("next day");
        assert_eq!(day_a, day_b);
        assert_ne!(day_a, next_day);
    }

    #[test]
    fn daily_key_depends_on_search_key_bytes() {
        let key_a = RouterHash::from_bytes([0x01u8; 32]);
        let key_b = RouterHash::from_bytes([0x02u8; 32]);
        let now = i2pr_proto::Date::from_millis(0);
        let routing_a = daily_routing_key(&key_a, now).expect("a");
        let routing_b = daily_routing_key(&key_b, now).expect("b");
        assert_ne!(routing_a, routing_b);
    }

    #[test]
    fn daily_key_does_not_mutate_search_key() {
        let search_key = RouterHash::from_bytes([0x77u8; 32]);
        let original = *search_key.as_bytes();
        let _ = daily_routing_key(&search_key, i2pr_proto::Date::from_millis(0)).expect("hash");
        assert_eq!(search_key.as_bytes(), &original);
    }

    #[test]
    fn daily_key_known_vector_for_well_known_search_key() {
        let search_key = RouterHash::from_bytes([0x33u8; 32]);
        let result =
            daily_routing_key(&search_key, i2pr_proto::Date::from_millis(0)).expect("hash");
        let expected = Hash::digest(&[search_key.as_bytes(), b"19700101".as_ref()].concat());
        assert_eq!(result, expected);
    }

    #[test]
    fn daily_key_format_rejects_out_of_range_dates() {
        // 10000-01-01 is past the maximum representable date.
        let ms = 253_402_300_800_000;
        let days = ms / 86_400_000;
        let error = format_daily_key(i2pr_proto::Date::from_millis(ms)).unwrap_err();
        assert_eq!(error, RoutingKeyError::DateOutOfRange { days });
    }
}
