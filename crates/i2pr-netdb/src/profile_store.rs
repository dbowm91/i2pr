//! Plan 488 tiered peer profile store.
//!
//! The Plan 103 [`RouterInfoStore`] keeps a flat map of validated
//! RouterInfos: every retained peer is equal, nothing tracks whether a peer
//! answers, and nothing records which peers are worth building through.
//! This module layers a **profile** over that map and owns everything the
//! later participation phases need from it:
//!
//! - a **capacity tier** per peer, derived from the peer's signed `caps`
//!   option (self-asserted data, treated as a hint and never as trust);
//! - a **failure model** that distinguishes a peer that was never tried from
//!   a peer that was tried and did not answer, with bounded counters;
//! - a **bounded, explainable score** whose ordering rule is a Plan 488
//!   requirement: a peer that answered without holding the record ranks
//!   above a peer that did not answer at all;
//! - a **maintenance pass** that demotes an aged failure and then drops it,
//!   counting both transitions;
//! - a **bounded, sanitized projection** that survives a restart and is
//!   discarded — never fatal — when it is corrupt or oversized.
//!
//! Every bound is a named constant or a configured ceiling; every drop path
//! releases the peer's record and its byte accounting together. The store
//! is runtime-neutral: no socket, no timer, no file, no task.

use std::collections::BTreeMap;
use std::fmt;

use sha2::{Digest, Sha256};

use crate::profile::{
    MAX_ANSWERED_COUNT, MAX_CONSECUTIVE_FAILURES, MAX_TOTAL_FAILURES, PeerCapacityClass, Profile,
    ProfileScore, ProfileState, ProfileTier, SCORE_PENALTY_UNANSWERED,
};
use crate::router_info::{RouterHash, ValidatedRouterInfo};
use crate::store::{InsertOutcome, RouterInfoStore, RouterInfoStoreConfig};

/// The maximum number of profiles the store retains by default.
pub const DEFAULT_MAX_PROFILES: usize = 4_096;

/// The maximum number of high-capacity profiles the store retains by default.
pub const DEFAULT_MAX_HIGH_CAPACITY_PROFILES: usize = 1_024;

/// The maximum number of standard profiles the store retains by default.
pub const DEFAULT_MAX_STANDARD_PROFILES: usize = 2_048;

/// The maximum number of untracked profiles the store retains by default.
pub const DEFAULT_MAX_UNTRACKED_PROFILES: usize = 1_024;

/// The maximum encoded size of a persisted profile projection, in bytes.
pub const DEFAULT_MAX_PROJECTION_BYTES: usize = 256 * 1024;

/// The default age after which an aged failure is demoted one tier.
pub const DEFAULT_DEMOTE_AFTER_MS: u64 = 15 * 60 * 1_000;

/// The default age after which an aged or untried peer is dropped.
pub const DEFAULT_DROP_AFTER_MS: u64 = 60 * 60 * 1_000;

/// The default interval between two maintenance passes.
pub const DEFAULT_MAINTENANCE_INTERVAL_MS: u64 = 60 * 1_000;

/// The projection format version this implementation writes and accepts.
pub const PROFILE_PROJECTION_VERSION: u32 = 1;

/// The magic line that opens a profile projection.
pub const PROFILE_PROJECTION_MAGIC: &str = "i2pr-peer-profile";

/// Configuration for [`ProfileStore`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileStoreConfig {
    /// The configuration of the underlying validated-RouterInfo store.
    ///
    /// The Plan 103 ceilings keep their current defaults and semantics; the
    /// tiering is layered on top of them, never instead of them.
    pub router_info: RouterInfoStoreConfig,
    /// Maximum number of retained profiles.
    pub max_profiles: usize,
    /// Maximum number of retained high-capacity profiles.
    pub max_high_capacity: usize,
    /// Maximum number of retained standard profiles.
    pub max_standard: usize,
    /// Maximum number of retained untracked profiles.
    pub max_untracked: usize,
    /// Maximum encoded size of a persisted projection.
    pub max_projection_bytes: usize,
    /// Age after which an aged failure is demoted one tier.
    pub demote_after_ms: u64,
    /// Age after which an aged or untried peer is dropped.
    pub drop_after_ms: u64,
    /// Interval between two maintenance passes.
    pub maintenance_interval_ms: u64,
}

impl Default for ProfileStoreConfig {
    fn default() -> Self {
        Self {
            router_info: RouterInfoStoreConfig::default(),
            max_profiles: DEFAULT_MAX_PROFILES,
            max_high_capacity: DEFAULT_MAX_HIGH_CAPACITY_PROFILES,
            max_standard: DEFAULT_MAX_STANDARD_PROFILES,
            max_untracked: DEFAULT_MAX_UNTRACKED_PROFILES,
            max_projection_bytes: DEFAULT_MAX_PROJECTION_BYTES,
            demote_after_ms: DEFAULT_DEMOTE_AFTER_MS,
            drop_after_ms: DEFAULT_DROP_AFTER_MS,
            maintenance_interval_ms: DEFAULT_MAINTENANCE_INTERVAL_MS,
        }
    }
}

impl ProfileStoreConfig {
    /// Constructs a custom configuration.
    ///
    /// `max_profiles` is clamped to the underlying record ceiling so the
    /// profile map can never hold more peers than the store that owns their
    /// records.
    pub const fn new(
        router_info: RouterInfoStoreConfig,
        max_profiles: usize,
        max_high_capacity: usize,
        max_standard: usize,
        max_untracked: usize,
    ) -> Self {
        let clamped = if max_profiles > router_info.max_records {
            router_info.max_records
        } else {
            max_profiles
        };
        Self {
            router_info,
            max_profiles: clamped,
            max_high_capacity,
            max_standard,
            max_untracked,
            max_projection_bytes: DEFAULT_MAX_PROJECTION_BYTES,
            demote_after_ms: DEFAULT_DEMOTE_AFTER_MS,
            drop_after_ms: DEFAULT_DROP_AFTER_MS,
            maintenance_interval_ms: DEFAULT_MAINTENANCE_INTERVAL_MS,
        }
    }

    /// Returns the ceiling that applies to a given tier.
    pub const fn tier_ceiling(&self, tier: ProfileTier) -> usize {
        match tier {
            ProfileTier::HighCapacity => self.max_high_capacity,
            ProfileTier::Standard => self.max_standard,
            ProfileTier::Untracked => self.max_untracked,
        }
    }
}

/// The outcome of a [`ProfileStore::insert`] call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileInsertOutcome {
    /// The peer was inserted and a profile was created.
    Inserted,
    /// A byte-identical record was already present; nothing changed.
    Idempotent,
    /// A strictly newer published record replaced the previous one; the
    /// profile was re-classified and its observed state was preserved.
    Replaced,
    /// The record conflicted with an existing one; the existing record and
    /// profile were preserved.
    Conflict,
    /// A strictly older record was rejected.
    StaleReplacement,
    /// The underlying validated-RouterInfo store is at capacity.
    CapacityExceeded,
    /// The profile store refused the peer because its tier ceiling was
    /// reached and nothing could be evicted.
    ProfileCapacityExceeded,
}

/// The outcome of an observation event applied to a peer profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObserveOutcome {
    /// The observation was applied.
    Recorded {
        /// The state the peer was in before the observation.
        previous_state: ProfileState,
        /// The state the peer is in after the observation.
        state: ProfileState,
        /// The peer's score after the observation.
        score: u16,
    },
    /// The peer is not known to the store; the observation was ignored.
    UnknownPeer,
}

/// The result of one maintenance pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MaintenanceReport {
    /// Peers demoted one tier because their failure aged out.
    pub demoted: usize,
    /// Peers dropped because their failure or their untried state aged out.
    pub dropped: usize,
    /// Peers evicted to make room for a newly inserted peer.
    pub evicted: usize,
    /// Peers the pass left untouched.
    pub unchanged: usize,
}

/// The result of applying a persisted projection to the store.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RestoreReport {
    /// Profiles restored for peers the store still holds.
    pub restored: usize,
    /// Profiles discarded because the store no longer holds the peer.
    pub orphaned: usize,
    /// Peers the store holds that the projection did not mention; they keep
    /// their freshly classified profile.
    pub fresh: usize,
}

/// Privacy-safe aggregate statistics for the profile store.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProfileStoreStats {
    /// Number of validated RouterInfos the store retains.
    pub peer_count: usize,
    /// Number of retained profiles.
    pub profile_count: usize,
    /// Number of high-capacity profiles.
    pub high_capacity_count: usize,
    /// Number of standard profiles.
    pub standard_count: usize,
    /// Number of untracked profiles.
    pub untracked_count: usize,
    /// Number of peers that advertise the `f` capability.
    pub floodfill_count: usize,
    /// Number of peers in each state, indexed by [`ProfileState::label`].
    pub state_counts: BTreeMap<&'static str, usize>,
    /// Total encoded bytes retained by the underlying store.
    pub total_encoded_bytes: usize,
    /// Configured profile ceiling.
    pub max_profiles: usize,
}

/// A typed failure of the projection codec.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileProjectionError {
    /// The encoded projection exceeded the configured ceiling.
    TooLarge {
        /// The size that was refused.
        actual: usize,
        /// The inclusive ceiling.
        maximum: usize,
    },
    /// The projection did not parse.
    Malformed {
        /// A bounded, human-readable reason.
        reason: &'static str,
    },
    /// The body digest did not match the recorded digest.
    DigestMismatch,
    /// The projection carried more records than the store accepts.
    TooManyRecords {
        /// The number of records in the projection.
        actual: usize,
        /// The inclusive ceiling.
        maximum: usize,
    },
    /// A record carried a score outside the bounded range.
    ScoreOutOfRange {
        /// The rejected value.
        value: u16,
        /// The inclusive maximum.
        maximum: u16,
    },
    /// A record carried an unknown tier or state label.
    UnknownLabel {
        /// The rejected label.
        label: &'static str,
    },
}

impl fmt::Display for ProfileProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "projection of {actual} bytes exceeds {maximum}-byte limit"
                )
            }
            Self::Malformed { reason } => write!(formatter, "malformed projection: {reason}"),
            Self::DigestMismatch => formatter.write_str("projection body digest mismatch"),
            Self::TooManyRecords { actual, maximum } => {
                write!(
                    formatter,
                    "projection carries {actual} records, limit is {maximum}"
                )
            }
            Self::ScoreOutOfRange { value, maximum } => {
                write!(formatter, "projection score {value} exceeds {maximum}")
            }
            Self::UnknownLabel { label } => write!(formatter, "projection label {label:?} unknown"),
        }
    }
}

impl std::error::Error for ProfileProjectionError {}

/// One persisted profile record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileProjectionRecord {
    /// The peer's RouterHash.
    pub key: RouterHash,
    /// The peer's capacity tier.
    pub tier: ProfileTier,
    /// The peer's reachability state.
    pub state: ProfileState,
    /// The peer's bounded score.
    pub score: u16,
    /// Consecutive failures observed.
    pub consecutive_failures: u32,
    /// Total failures observed.
    pub total_failures: u32,
    /// Answered replies observed.
    pub answered_count: u32,
    /// Whether the peer advertises the `f` capability.
    pub floodfill: bool,
    /// Whether the peer declared a capacity class.
    pub capacity_evidence: bool,
    /// The millisecond timestamp of the most recent observation.
    pub last_observed_ms: u64,
    /// The millisecond timestamp of the most recent answered reply.
    pub last_answered_ms: Option<u64>,
}

/// A bounded, sanitized projection of the profile store.
///
/// The projection carries per-peer state only: a RouterHash (already a
/// digest), a tier, a state, bounded counters, and timestamps. It carries
/// no address, no key material, no session state, and no payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileProjection {
    /// The projection format version.
    pub version: u32,
    /// The persisted records, in canonical RouterHash order.
    pub records: Vec<ProfileProjectionRecord>,
}

impl ProfileProjection {
    /// Constructs an empty projection.
    pub fn empty() -> Self {
        Self {
            version: PROFILE_PROJECTION_VERSION,
            records: Vec::new(),
        }
    }

    /// Encodes the projection into its bounded textual form.
    ///
    /// The encoding is deterministic: the same records always produce the
    /// same bytes, and the body digest makes truncation or mutation
    /// detectable.
    pub fn encode(&self, maximum_bytes: usize) -> Result<Vec<u8>, ProfileProjectionError> {
        let mut body = String::new();
        for record in &self.records {
            body.push_str(&encode_hex(record.key.as_bytes()));
            body.push(' ');
            body.push_str(tier_label(record.tier));
            body.push(' ');
            body.push_str(state_label(record.state));
            body.push(' ');
            body.push_str(&record.score.to_string());
            body.push(' ');
            body.push_str(&record.consecutive_failures.to_string());
            body.push(' ');
            body.push_str(&record.total_failures.to_string());
            body.push(' ');
            body.push_str(&record.answered_count.to_string());
            body.push(' ');
            body.push_str(bool_label(record.floodfill));
            body.push(' ');
            body.push_str(bool_label(record.capacity_evidence));
            body.push(' ');
            body.push_str(&record.last_observed_ms.to_string());
            body.push(' ');
            match record.last_answered_ms {
                Some(value) => body.push_str(&value.to_string()),
                None => body.push('-'),
            }
            body.push('\n');
        }

        let digest = Sha256::digest(body.as_bytes());
        let mut out = String::new();
        out.push_str(PROFILE_PROJECTION_MAGIC);
        out.push('/');
        out.push_str(&self.version.to_string());
        out.push('\n');
        out.push_str(&self.records.len().to_string());
        out.push('\n');
        out.push_str(&encode_hex(&digest));
        out.push('\n');
        out.push_str(&body);

        let bytes = out.into_bytes();
        if bytes.len() > maximum_bytes {
            return Err(ProfileProjectionError::TooLarge {
                actual: bytes.len(),
                maximum: maximum_bytes,
            });
        }
        Ok(bytes)
    }

    /// Decodes a projection, refusing a malformed, corrupt, or oversized
    /// input.
    ///
    /// The caller is expected to treat every failure as "discard the
    /// projection and count it", never as a startup failure.
    pub fn decode(
        bytes: &[u8],
        maximum_bytes: usize,
        maximum_records: usize,
    ) -> Result<Self, ProfileProjectionError> {
        if bytes.len() > maximum_bytes {
            return Err(ProfileProjectionError::TooLarge {
                actual: bytes.len(),
                maximum: maximum_bytes,
            });
        }
        let text = core::str::from_utf8(bytes).map_err(|_| ProfileProjectionError::Malformed {
            reason: "not utf-8",
        })?;
        // The body is taken verbatim, trailing newline included, so the
        // digest covers exactly the bytes that were written.
        let mut parts = text.splitn(4, '\n');

        let header = parts
            .next()
            .ok_or(ProfileProjectionError::Malformed { reason: "empty" })?;
        let expected_header = format!("{PROFILE_PROJECTION_MAGIC}/{PROFILE_PROJECTION_VERSION}");
        if header != expected_header {
            return Err(ProfileProjectionError::Malformed {
                reason: "unexpected magic or version",
            });
        }
        let count_line = parts.next().ok_or(ProfileProjectionError::Malformed {
            reason: "missing record count",
        })?;
        let count: usize = count_line
            .parse()
            .map_err(|_| ProfileProjectionError::Malformed {
                reason: "record count is not a number",
            })?;
        if count > maximum_records {
            return Err(ProfileProjectionError::TooManyRecords {
                actual: count,
                maximum: maximum_records,
            });
        }
        let digest_line = parts.next().ok_or(ProfileProjectionError::Malformed {
            reason: "missing digest",
        })?;
        let expected_digest = decode_hex(digest_line).ok_or(ProfileProjectionError::Malformed {
            reason: "digest is not hex",
        })?;
        if expected_digest.len() != 32 {
            return Err(ProfileProjectionError::Malformed {
                reason: "digest is not 32 bytes",
            });
        }

        let body = parts.next().unwrap_or("");
        let actual_digest = Sha256::digest(body.as_bytes());
        if actual_digest.as_slice() != expected_digest.as_slice() {
            return Err(ProfileProjectionError::DigestMismatch);
        }

        let mut records = Vec::with_capacity(count.min(maximum_records));
        for line in body.lines() {
            if line.is_empty() {
                continue;
            }
            if records.len() >= maximum_records {
                return Err(ProfileProjectionError::TooManyRecords {
                    actual: records.len() + 1,
                    maximum: maximum_records,
                });
            }
            records.push(parse_record(line)?);
        }
        if records.len() != count {
            return Err(ProfileProjectionError::Malformed {
                reason: "record count mismatch",
            });
        }
        Ok(Self {
            version: PROFILE_PROJECTION_VERSION,
            records,
        })
    }
}

fn parse_record(line: &str) -> Result<ProfileProjectionRecord, ProfileProjectionError> {
    let fields: Vec<&str> = line.split(' ').collect();
    if fields.len() != 11 {
        return Err(ProfileProjectionError::Malformed {
            reason: "record has the wrong field count",
        });
    }
    let key_bytes = decode_hex(fields[0]).ok_or(ProfileProjectionError::Malformed {
        reason: "key is not hex",
    })?;
    if key_bytes.len() != 32 {
        return Err(ProfileProjectionError::Malformed {
            reason: "key is not 32 bytes",
        });
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&key_bytes);

    let tier = parse_tier(fields[1])?;
    let state = parse_state(fields[2])?;
    let score: u16 = fields[3]
        .parse()
        .map_err(|_| ProfileProjectionError::Malformed {
            reason: "score is not a number",
        })?;
    if score > ProfileScore::MAX.value() {
        return Err(ProfileProjectionError::ScoreOutOfRange {
            value: score,
            maximum: ProfileScore::MAX.value(),
        });
    }
    let consecutive_failures: u32 =
        fields[4]
            .parse()
            .map_err(|_| ProfileProjectionError::Malformed {
                reason: "consecutive failures is not a number",
            })?;
    let total_failures: u32 = fields[5]
        .parse()
        .map_err(|_| ProfileProjectionError::Malformed {
            reason: "total failures is not a number",
        })?;
    let answered_count: u32 = fields[6]
        .parse()
        .map_err(|_| ProfileProjectionError::Malformed {
            reason: "answered count is not a number",
        })?;
    if consecutive_failures > MAX_CONSECUTIVE_FAILURES
        || total_failures > MAX_TOTAL_FAILURES
        || answered_count > MAX_ANSWERED_COUNT
    {
        return Err(ProfileProjectionError::Malformed {
            reason: "counter exceeds its ceiling",
        });
    }
    let floodfill = parse_bool(fields[7])?;
    let capacity_evidence = parse_bool(fields[8])?;
    let last_observed_ms: u64 =
        fields[9]
            .parse()
            .map_err(|_| ProfileProjectionError::Malformed {
                reason: "last observed is not a number",
            })?;
    let last_answered_ms = if fields[10] == "-" {
        None
    } else {
        Some(
            fields[10]
                .parse()
                .map_err(|_| ProfileProjectionError::Malformed {
                    reason: "last answered is not a number",
                })?,
        )
    };

    Ok(ProfileProjectionRecord {
        key: RouterHash::from_bytes(key),
        tier,
        state,
        score,
        consecutive_failures,
        total_failures,
        answered_count,
        floodfill,
        capacity_evidence,
        last_observed_ms,
        last_answered_ms,
    })
}

const TIER_LABELS: [&str; 3] = ["high-capacity", "standard", "untracked"];
const STATE_LABELS: [&str; 7] = [
    "never-tried",
    "answered",
    "answered-empty",
    "refused",
    "misbehaving",
    "unanswered",
    "unreachable",
];

fn tier_label(tier: ProfileTier) -> &'static str {
    match tier {
        ProfileTier::HighCapacity => TIER_LABELS[0],
        ProfileTier::Standard => TIER_LABELS[1],
        ProfileTier::Untracked => TIER_LABELS[2],
    }
}

fn state_label(state: ProfileState) -> &'static str {
    match state {
        ProfileState::NeverTried => STATE_LABELS[0],
        ProfileState::Answered => STATE_LABELS[1],
        ProfileState::AnsweredEmpty => STATE_LABELS[2],
        ProfileState::Refused => STATE_LABELS[3],
        ProfileState::Misbehaving => STATE_LABELS[4],
        ProfileState::Unanswered => STATE_LABELS[5],
        ProfileState::Unreachable => STATE_LABELS[6],
    }
}

fn parse_tier(label: &str) -> Result<ProfileTier, ProfileProjectionError> {
    match label {
        "high-capacity" => Ok(ProfileTier::HighCapacity),
        "standard" => Ok(ProfileTier::Standard),
        "untracked" => Ok(ProfileTier::Untracked),
        _ => Err(ProfileProjectionError::UnknownLabel { label: "tier" }),
    }
}

fn parse_state(label: &str) -> Result<ProfileState, ProfileProjectionError> {
    match label {
        "never-tried" => Ok(ProfileState::NeverTried),
        "answered" => Ok(ProfileState::Answered),
        "answered-empty" => Ok(ProfileState::AnsweredEmpty),
        "refused" => Ok(ProfileState::Refused),
        "misbehaving" => Ok(ProfileState::Misbehaving),
        "unanswered" => Ok(ProfileState::Unanswered),
        "unreachable" => Ok(ProfileState::Unreachable),
        _ => Err(ProfileProjectionError::UnknownLabel { label: "state" }),
    }
}

fn bool_label(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}

fn parse_bool(value: &str) -> Result<bool, ProfileProjectionError> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(ProfileProjectionError::Malformed {
            reason: "boolean is not 0 or 1",
        }),
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(text.len() / 2);
    let mut index = 0;
    while index < bytes.len() {
        let high = hex_value(bytes[index])?;
        let low = hex_value(bytes[index + 1])?;
        out.push((high << 4) | low);
        index += 2;
    }
    Some(out)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// A bounded, tiered store of peer profiles layered over the Plan 103
/// validated-RouterInfo store.
#[derive(Debug)]
pub struct ProfileStore {
    peers: RouterInfoStore,
    profiles: BTreeMap<RouterHash, Profile>,
    config: ProfileStoreConfig,
}

impl Default for ProfileStore {
    fn default() -> Self {
        Self::new(ProfileStoreConfig::default())
    }
}

impl ProfileStore {
    /// Constructs an empty profile store.
    pub fn new(config: ProfileStoreConfig) -> Self {
        Self {
            peers: RouterInfoStore::with_config(config.router_info),
            profiles: BTreeMap::new(),
            config,
        }
    }

    /// Adopts an existing validated-RouterInfo store and classifies every
    /// retained peer.
    ///
    /// Adoption never trusts a peer that has no validated record: a peer is
    /// classified only because a validated RouterInfo for it exists. The
    /// adopted store keeps its own record and byte ceilings; the profile
    /// ceiling is clamped to that record ceiling.
    pub fn adopt(config: ProfileStoreConfig, store: RouterInfoStore, now_ms: u64) -> Self {
        let mut config = config;
        config.router_info = store.config();
        config.max_profiles = config.max_profiles.min(config.router_info.max_records);
        let mut adopted = Self::new(config);
        let keys: Vec<RouterHash> = store.iter().map(|(key, _record)| *key).collect();
        for key in keys {
            if let Some(record) = store.get(&key) {
                let mut profile = Profile::new(now_ms);
                classify(&mut profile, record);
                adopted.profiles.insert(key, profile);
            }
        }
        adopted.peers = store;
        adopted
    }

    /// Returns the underlying validated-RouterInfo store.
    pub const fn peers(&self) -> &RouterInfoStore {
        &self.peers
    }

    /// Returns the store configuration.
    pub const fn config(&self) -> ProfileStoreConfig {
        self.config
    }

    /// Returns the number of retained profiles.
    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    /// Returns whether the store holds no profile.
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }

    /// Returns the profile of one peer.
    pub fn profile(&self, key: &RouterHash) -> Option<&Profile> {
        self.profiles.get(key)
    }

    /// Iterates the profiles in canonical RouterHash order.
    pub fn iter(&self) -> impl Iterator<Item = (&RouterHash, &Profile)> {
        self.profiles.iter()
    }

    /// Inserts a validated peer and classifies it.
    ///
    /// The only insertion path consumes a [`ValidatedRouterInfo`]; an
    /// unvalidated record can never reach the profile store, and the Plan
    /// 103 `insert_unchecked` prohibition is preserved because the profile
    /// store exposes no such path.
    pub fn insert(&mut self, validated: ValidatedRouterInfo, now_ms: u64) -> ProfileInsertOutcome {
        let key = validated.key();
        let known = self.profiles.contains_key(&key);
        if known {
            let outcome = self.peers.insert(validated.clone());
            match outcome {
                InsertOutcome::Replaced => {
                    if let Some(profile) = self.profiles.get_mut(&key) {
                        classify(profile, &validated);
                    }
                    ProfileInsertOutcome::Replaced
                }
                InsertOutcome::Idempotent => ProfileInsertOutcome::Idempotent,
                InsertOutcome::Conflict => ProfileInsertOutcome::Conflict,
                InsertOutcome::StaleReplacement => ProfileInsertOutcome::StaleReplacement,
                InsertOutcome::CapacityExceeded => ProfileInsertOutcome::CapacityExceeded,
                InsertOutcome::Inserted => ProfileInsertOutcome::Inserted,
            }
        } else {
            let mut profile = Profile::new(now_ms);
            classify(&mut profile, &validated);
            let tier = profile.tier();

            // Make room before inserting so the underlying store's own
            // ceiling is never the reason a legitimate peer is refused.
            if self.profiles.len() >= self.config.max_profiles && !self.evict_worst(None) {
                return ProfileInsertOutcome::ProfileCapacityExceeded;
            }
            let tier_count = self.tier_count(tier);
            if tier_count >= self.config.tier_ceiling(tier) && !self.evict_worst(Some(tier)) {
                return ProfileInsertOutcome::ProfileCapacityExceeded;
            }
            match self.peers.insert(validated) {
                InsertOutcome::Inserted => {
                    self.profiles.insert(key, profile);
                    ProfileInsertOutcome::Inserted
                }
                other => match other {
                    InsertOutcome::CapacityExceeded => ProfileInsertOutcome::CapacityExceeded,
                    InsertOutcome::Idempotent => ProfileInsertOutcome::Idempotent,
                    InsertOutcome::Conflict => ProfileInsertOutcome::Conflict,
                    InsertOutcome::StaleReplacement => ProfileInsertOutcome::StaleReplacement,
                    InsertOutcome::Replaced => ProfileInsertOutcome::Replaced,
                    InsertOutcome::Inserted => unreachable!("inserted arm handled above"),
                },
            }
        }
    }

    /// Removes a peer and its profile together.
    ///
    /// The byte accounting is released with the record, so a removal never
    /// leaks capacity.
    pub fn remove(&mut self, key: &RouterHash) -> bool {
        let removed = self.peers.remove(key);
        self.profiles.remove(key);
        removed
    }

    /// Records that a peer answered with the requested data.
    pub fn observe_answered(&mut self, key: &RouterHash, now_ms: u64) -> ObserveOutcome {
        self.observe(key, |profile| profile.observe_answered(now_ms))
    }

    /// Records that a peer answered but does not hold the requested record.
    pub fn observe_answered_empty(&mut self, key: &RouterHash, now_ms: u64) -> ObserveOutcome {
        self.observe(key, |profile| profile.observe_answered_empty(now_ms))
    }

    /// Records that a peer refused or returned an error.
    pub fn observe_refused(&mut self, key: &RouterHash, now_ms: u64) -> ObserveOutcome {
        self.observe(key, |profile| profile.observe_refused(now_ms))
    }

    /// Records that a peer replied with data that does not match the
    /// requested key.
    pub fn observe_misbehaving(&mut self, key: &RouterHash, now_ms: u64) -> ObserveOutcome {
        self.observe(key, |profile| profile.observe_misbehaving(now_ms))
    }

    /// Records that a peer did not answer before the deadline.
    pub fn observe_unanswered(&mut self, key: &RouterHash, now_ms: u64) -> ObserveOutcome {
        self.observe(key, |profile| profile.observe_unanswered(now_ms))
    }

    /// Records that a peer could not be reached at the transport level.
    pub fn observe_unreachable(&mut self, key: &RouterHash, now_ms: u64) -> ObserveOutcome {
        self.observe(key, |profile| profile.observe_unreachable(now_ms))
    }

    fn observe(&mut self, key: &RouterHash, apply: impl FnOnce(&mut Profile)) -> ObserveOutcome {
        let Some(profile) = self.profiles.get_mut(key) else {
            return ObserveOutcome::UnknownPeer;
        };
        let previous_state = profile.state();
        apply(profile);
        ObserveOutcome::Recorded {
            previous_state,
            state: profile.state(),
            score: profile.score().value(),
        }
    }

    /// Runs the maintenance pass.
    ///
    /// The pass demotes a peer whose failure aged past the demotion
    /// threshold, and drops a peer whose failure — or whose untried state —
    /// aged past the drop threshold. Both transitions are counted, and the
    /// pass is idempotent: running it twice with the same clock changes
    /// nothing the second time.
    pub fn maintain(&mut self, now_ms: u64) -> MaintenanceReport {
        let mut report = MaintenanceReport::default();

        let demote_after = self.config.demote_after_ms;
        let drop_after = self.config.drop_after_ms;

        let mut demote: Vec<RouterHash> = Vec::new();
        let mut drop: Vec<RouterHash> = Vec::new();
        for (key, profile) in &self.profiles {
            let age = profile.age_ms(now_ms);
            if profile.state().is_aged_failure() {
                if age >= drop_after {
                    drop.push(*key);
                    continue;
                }
                if age >= demote_after && profile.tier() != ProfileTier::Untracked {
                    demote.push(*key);
                }
            } else if profile.state() == ProfileState::NeverTried && age >= drop_after {
                drop.push(*key);
            }
        }

        for key in demote {
            if let Some(profile) = self.profiles.get_mut(&key)
                && profile.demote()
            {
                profile.score = profile.score.debit(SCORE_PENALTY_UNANSWERED);
                report.demoted += 1;
            }
        }

        for key in drop {
            if self.remove(&key) {
                report.dropped += 1;
            }
        }

        report.unchanged = self.profiles.len();
        report
    }

    /// Returns whether a maintenance pass is due at the supplied clock.
    pub fn maintenance_due(&self, now_ms: u64, last_maintenance_ms: Option<u64>) -> bool {
        let Some(last) = last_maintenance_ms else {
            return true;
        };
        now_ms.saturating_sub(last) >= self.config.maintenance_interval_ms
    }

    /// Builds the bounded, sanitized projection of the current state.
    pub fn projection(&self) -> ProfileProjection {
        let records = self
            .profiles
            .iter()
            .map(|(key, profile)| ProfileProjectionRecord {
                key: *key,
                tier: profile.tier(),
                state: profile.state(),
                score: profile.score().value(),
                consecutive_failures: profile.consecutive_failures(),
                total_failures: profile.total_failures(),
                answered_count: profile.answered_count(),
                floodfill: profile.advertises_floodfill(),
                capacity_evidence: profile.has_capacity_evidence(),
                last_observed_ms: profile.last_observed_ms(),
                last_answered_ms: profile.last_answered_ms(),
            })
            .collect();
        ProfileProjection {
            version: PROFILE_PROJECTION_VERSION,
            records,
        }
    }

    /// Encodes the current projection, honouring the configured ceiling.
    pub fn encode_projection(&self) -> Result<Vec<u8>, ProfileProjectionError> {
        self.projection().encode(self.config.max_projection_bytes)
    }

    /// Applies a persisted projection to the current store.
    ///
    /// A profile whose peer is no longer held is counted as orphaned and
    /// discarded; a peer the projection does not mention keeps its freshly
    /// classified profile.
    pub fn restore(&mut self, projection: &ProfileProjection) -> RestoreReport {
        let mut report = RestoreReport::default();
        let mut restored = Vec::with_capacity(projection.records.len());
        for record in &projection.records {
            if !self.peers.contains(&record.key) {
                report.orphaned += 1;
                continue;
            }
            let Some(existing) = self.profiles.get(&record.key) else {
                report.fresh += 1;
                continue;
            };
            let mut profile = *existing;
            profile.restore_from(record);
            restored.push((record.key, profile));
        }
        for (key, profile) in restored {
            self.profiles.insert(key, profile);
            report.restored += 1;
        }
        report
    }

    /// Returns privacy-safe aggregate statistics.
    pub fn stats(&self) -> ProfileStoreStats {
        let mut state_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        for label in STATE_LABELS {
            state_counts.insert(label, 0);
        }
        let mut high_capacity_count = 0usize;
        let mut standard_count = 0usize;
        let mut untracked_count = 0usize;
        let mut floodfill_count = 0usize;
        for profile in self.profiles.values() {
            match profile.tier() {
                ProfileTier::HighCapacity => high_capacity_count += 1,
                ProfileTier::Standard => standard_count += 1,
                ProfileTier::Untracked => untracked_count += 1,
            }
            if profile.advertises_floodfill() {
                floodfill_count += 1;
            }
            *state_counts.entry(profile.state().label()).or_insert(0) += 1;
        }
        ProfileStoreStats {
            peer_count: self.peers.len(),
            profile_count: self.profiles.len(),
            high_capacity_count,
            standard_count,
            untracked_count,
            floodfill_count,
            state_counts,
            total_encoded_bytes: self.peers.encoded_bytes(),
            max_profiles: self.config.max_profiles,
        }
    }

    fn tier_count(&self, tier: ProfileTier) -> usize {
        self.profiles
            .values()
            .filter(|profile| profile.tier() == tier)
            .count()
    }

    /// Evicts the worst-ranked peer, optionally restricted to one tier.
    ///
    /// The eviction is deterministic: the lowest rank loses, and ties are
    /// broken by the highest RouterHash, so the same store always evicts
    /// the same peer.
    fn evict_worst(&mut self, tier: Option<ProfileTier>) -> bool {
        let mut victim: Option<(u16, RouterHash)> = None;
        for (key, profile) in &self.profiles {
            if let Some(tier) = tier
                && profile.tier() != tier
            {
                continue;
            }
            let rank = profile.rank();
            match victim {
                None => victim = Some((rank, *key)),
                Some((best_rank, best_key)) => {
                    if rank < best_rank || (rank == best_rank && *key > best_key) {
                        victim = Some((rank, *key));
                    }
                }
            }
        }
        match victim {
            Some((_, key)) => {
                self.remove(&key);
                true
            }
            None => false,
        }
    }
}

fn classify(profile: &mut Profile, record: &ValidatedRouterInfo) {
    let caps = match record.router_info().capabilities() {
        Ok(Some(caps)) => caps,
        _ => {
            profile.apply_capacity_class(None);
            profile.set_floodfill(record.advertises_floodfill());
            return;
        }
    };
    let class = PeerCapacityClass::from_capabilities(Some(caps.as_str()));
    profile.apply_capacity_class(class);
    profile.set_floodfill(record.advertises_floodfill());
}

impl Profile {
    /// Restores the observed state of a persisted record.
    ///
    /// Only the observed state is restored; the tier is re-derived from the
    /// stored capacity evidence so a restored profile cannot claim a tier
    /// the peer never declared.
    fn restore_from(&mut self, record: &ProfileProjectionRecord) {
        self.tier = match record.tier {
            ProfileTier::HighCapacity => {
                if record.capacity_evidence {
                    ProfileTier::HighCapacity
                } else {
                    ProfileTier::Untracked
                }
            }
            ProfileTier::Standard => {
                if record.capacity_evidence {
                    ProfileTier::Standard
                } else {
                    ProfileTier::Untracked
                }
            }
            ProfileTier::Untracked => ProfileTier::Untracked,
        };
        self.state = record.state;
        self.score = ProfileScore::new(record.score).unwrap_or(ProfileScore::MIN);
        self.consecutive_failures = record.consecutive_failures;
        self.total_failures = record.total_failures;
        self.answered_count = record.answered_count;
        self.floodfill = record.floodfill;
        self.capacity_evidence = record.capacity_evidence;
        self.last_observed_ms = record.last_observed_ms;
        self.last_answered_ms = record.last_answered_ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router_info::ValidationContext;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_proto::{Date, Mapping};
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn validate_with_options(
        signer: &RouterIdentityBundle,
        published_ms: u64,
        options: Mapping,
    ) -> ValidatedRouterInfo {
        let info = signer
            .sign_router_info(
                Date::from_millis(published_ms),
                Vec::new(),
                Vec::new(),
                options,
            )
            .expect("sign");
        ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(Date::from_millis(published_ms)),
        )
        .expect("validate")
    }

    fn validate(signer: &RouterIdentityBundle, published_ms: u64) -> ValidatedRouterInfo {
        validate_with_options(signer, published_ms, Mapping::empty())
    }

    fn caps_options(caps: &str) -> Mapping {
        let mut options = Mapping::builder();
        options.insert("caps".to_owned(), caps.to_owned()).unwrap();
        options.build().unwrap()
    }

    #[test]
    fn insert_classifies_the_tier_from_the_signed_caps() {
        let mut store = ProfileStore::default();
        let signer = bundle(0x900);
        let outcome = store.insert(validate_with_options(&signer, 1, caps_options("Nf")), 1);
        assert_eq!(outcome, ProfileInsertOutcome::Inserted);
        let profile = store.profile(&validate(&signer, 1).key()).expect("profile");
        assert_eq!(profile.tier(), ProfileTier::HighCapacity);
        assert!(profile.advertises_floodfill());
    }

    #[test]
    fn a_peer_without_a_capacity_class_is_untracked_not_downgraded() {
        let mut store = ProfileStore::default();
        let signer = bundle(0x901);
        store.insert(validate(&signer, 1), 1);
        let profile = store.profile(&validate(&signer, 1).key()).expect("profile");
        assert_eq!(profile.tier(), ProfileTier::Untracked);
        assert!(!profile.has_capacity_evidence());
    }

    #[test]
    fn never_tried_is_distinct_from_did_not_answer() {
        let mut store = ProfileStore::default();
        let silent = bundle(0x902);
        let untried = bundle(0x903);
        store.insert(validate(&silent, 1), 1);
        store.insert(validate(&untried, 1), 1);
        let silent_key = validate(&silent, 1).key();
        let untried_key = validate(&untried, 1).key();
        store.observe_unanswered(&silent_key, 2);
        assert_eq!(
            store.profile(&silent_key).expect("profile").state(),
            ProfileState::Unanswered
        );
        assert_eq!(
            store.profile(&untried_key).expect("profile").state(),
            ProfileState::NeverTried
        );
    }

    #[test]
    fn answering_outranks_not_answering() {
        let mut store = ProfileStore::default();
        let good = bundle(0x904);
        let bad = bundle(0x905);
        store.insert(validate(&good, 1), 1);
        store.insert(validate(&bad, 1), 1);
        let good_key = validate(&good, 1).key();
        let bad_key = validate(&bad, 1).key();
        store.observe_answered_empty(&good_key, 2);
        store.observe_unanswered(&bad_key, 2);
        assert!(
            store.profile(&good_key).expect("profile").rank()
                > store.profile(&bad_key).expect("profile").rank()
        );
    }

    #[test]
    fn maintenance_demotes_then_drops_an_aged_failure() {
        let mut store = ProfileStore::default();
        let signer = bundle(0x906);
        store.insert(validate_with_options(&signer, 1, caps_options("Nf")), 1);
        let key = validate(&signer, 1).key();
        store.observe_unanswered(&key, 10);
        let report = store.maintain(10 + DEFAULT_DEMOTE_AFTER_MS);
        assert_eq!(report.demoted, 1);
        assert_eq!(report.dropped, 0);
        assert_eq!(
            store.profile(&key).expect("profile").tier(),
            ProfileTier::Standard
        );
        let report = store.maintain(10 + DEFAULT_DROP_AFTER_MS);
        assert_eq!(report.dropped, 1);
        assert!(store.profile(&key).is_none());
        assert!(!store.peers().contains(&key));
    }

    #[test]
    fn maintenance_is_idempotent() {
        let mut store = ProfileStore::default();
        let signer = bundle(0x907);
        store.insert(validate(&signer, 1), 1);
        let key = validate(&signer, 1).key();
        store.observe_unanswered(&key, 10);
        let first = store.maintain(10 + DEFAULT_DROP_AFTER_MS);
        assert_eq!(first.dropped, 1);
        let second = store.maintain(10 + DEFAULT_DROP_AFTER_MS);
        assert_eq!(second.dropped, 0);
        assert_eq!(second.demoted, 0);
    }

    #[test]
    fn dropping_a_peer_releases_its_byte_accounting() {
        let mut store = ProfileStore::default();
        let signer = bundle(0x908);
        store.insert(validate(&signer, 1), 1);
        let key = validate(&signer, 1).key();
        let bytes = store.peers().encoded_bytes();
        assert!(bytes > 0);
        assert!(store.remove(&key));
        assert_eq!(store.peers().encoded_bytes(), 0);
        assert!(store.is_empty());
    }

    #[test]
    fn an_untried_peer_ages_out_without_ever_being_contacted() {
        let mut store = ProfileStore::default();
        let signer = bundle(0x909);
        store.insert(validate(&signer, 1), 1);
        let report = store.maintain(1 + DEFAULT_DROP_AFTER_MS);
        assert_eq!(report.dropped, 1);
        assert!(store.is_empty());
    }

    #[test]
    fn the_profile_ceiling_refuses_or_evicts_deterministically() {
        let config = ProfileStoreConfig {
            router_info: RouterInfoStoreConfig::new(2, usize::MAX),
            max_profiles: 2,
            max_high_capacity: 2,
            max_standard: 2,
            max_untracked: 2,
            ..ProfileStoreConfig::default()
        };
        let mut store = ProfileStore::new(config);
        let first = bundle(0x90a);
        let second = bundle(0x90b);
        let third = bundle(0x90c);
        store.insert(validate(&first, 1), 1);
        store.insert(validate(&second, 1), 1);
        // The third peer evicts the worst-ranked of the two.
        let outcome = store.insert(validate(&third, 1), 1);
        assert_eq!(outcome, ProfileInsertOutcome::Inserted);
        assert_eq!(store.len(), 2);
        assert!(store.profile(&validate(&third, 1).key()).is_some());
    }

    #[test]
    fn a_zero_ceiling_refuses_every_peer() {
        let config = ProfileStoreConfig {
            router_info: RouterInfoStoreConfig::new(0, 0),
            max_profiles: 0,
            max_high_capacity: 0,
            max_standard: 0,
            max_untracked: 0,
            ..ProfileStoreConfig::default()
        };
        let mut store = ProfileStore::new(config);
        let signer = bundle(0x90d);
        assert_eq!(
            store.insert(validate(&signer, 1), 1),
            ProfileInsertOutcome::ProfileCapacityExceeded
        );
        assert!(store.is_empty());
    }

    #[test]
    fn the_projection_round_trips() {
        let mut store = ProfileStore::default();
        let signer = bundle(0x90e);
        store.insert(validate_with_options(&signer, 1, caps_options("Nf")), 1);
        let key = validate(&signer, 1).key();
        store.observe_answered(&key, 5);
        let encoded = store.encode_projection().expect("encode");
        let decoded = ProfileProjection::decode(
            &encoded,
            store.config().max_projection_bytes,
            store.config().max_profiles,
        )
        .expect("decode");
        assert_eq!(decoded.records.len(), 1);
        assert_eq!(decoded.records[0].state, ProfileState::Answered);
        assert_eq!(decoded.records[0].tier, ProfileTier::HighCapacity);
        assert_eq!(decoded.records[0].key, key);
    }

    #[test]
    fn a_corrupt_projection_is_refused_with_a_typed_error() {
        let mut store = ProfileStore::default();
        let signer = bundle(0x90f);
        store.insert(validate(&signer, 1), 1);
        let mut encoded = store.encode_projection().expect("encode");
        let length = encoded.len();
        encoded[length - 2] = b'0';
        let error = ProfileProjection::decode(
            &encoded,
            store.config().max_projection_bytes,
            store.config().max_profiles,
        )
        .expect_err("corrupt projection must be refused");
        assert!(
            matches!(
                error,
                ProfileProjectionError::DigestMismatch | ProfileProjectionError::Malformed { .. }
            ),
            "unexpected error {error:?}"
        );
    }

    #[test]
    fn an_oversized_projection_is_refused() {
        let error = ProfileProjection::decode(b"i2pr-peer-profile/1\n", 4, 16)
            .expect_err("oversized projection must be refused");
        assert_eq!(
            error,
            ProfileProjectionError::TooLarge {
                actual: 20,
                maximum: 4
            }
        );
    }

    #[test]
    fn restore_reapplies_the_observed_state() {
        let mut store = ProfileStore::default();
        let signer = bundle(0x910);
        store.insert(validate(&signer, 1), 1);
        let key = validate(&signer, 1).key();
        store.observe_answered(&key, 5);
        let projection = store.projection();

        let mut restarted = ProfileStore::default();
        restarted.insert(validate(&signer, 1), 1);
        assert_eq!(
            restarted.profile(&key).expect("profile").state(),
            ProfileState::NeverTried
        );
        let report = restarted.restore(&projection);
        assert_eq!(report.restored, 1);
        assert_eq!(report.orphaned, 0);
        assert_eq!(
            restarted.profile(&key).expect("profile").state(),
            ProfileState::Answered
        );
    }

    #[test]
    fn restore_counts_a_projection_entry_for_an_absent_peer() {
        let mut store = ProfileStore::default();
        let signer = bundle(0x911);
        store.insert(validate(&signer, 1), 1);
        let projection = store.projection();
        let mut empty = ProfileStore::default();
        let report = empty.restore(&projection);
        assert_eq!(report.orphaned, 1);
        assert_eq!(report.restored, 0);
    }

    #[test]
    fn stats_are_privacy_safe_aggregates() {
        let mut store = ProfileStore::default();
        let floodfill = bundle(0x912);
        let plain = bundle(0x913);
        store.insert(validate_with_options(&floodfill, 1, caps_options("Nf")), 1);
        store.insert(validate_with_options(&plain, 1, caps_options("L")), 1);
        let stats = store.stats();
        assert_eq!(stats.profile_count, 2);
        assert_eq!(stats.peer_count, 2);
        assert_eq!(stats.high_capacity_count, 1);
        assert_eq!(stats.standard_count, 1);
        assert_eq!(stats.untracked_count, 0);
        assert_eq!(stats.floodfill_count, 1);
        assert_eq!(stats.state_counts.get("never-tried"), Some(&2));
    }

    #[test]
    fn observation_of_an_unknown_peer_is_reported_not_ignored() {
        let mut store = ProfileStore::default();
        let phantom = RouterHash::from_bytes([0xabu8; 32]);
        assert_eq!(
            store.observe_answered(&phantom, 1),
            ObserveOutcome::UnknownPeer
        );
    }

    #[test]
    fn maintenance_due_respects_the_configured_interval() {
        let store = ProfileStore::default();
        assert!(store.maintenance_due(1_000, None));
        assert!(!store.maintenance_due(1_000, Some(1_000)));
        assert!(!store.maintenance_due(1_000 + DEFAULT_MAINTENANCE_INTERVAL_MS - 1, Some(1_000)));
        assert!(store.maintenance_due(1_000 + DEFAULT_MAINTENANCE_INTERVAL_MS, Some(1_000)));
    }

    #[test]
    fn the_projection_carries_no_address_or_key_material() {
        let mut store = ProfileStore::default();
        let signer = bundle(0x914);
        store.insert(validate(&signer, 1), 1);
        let encoded = store.encode_projection().expect("encode");
        let text = String::from_utf8(encoded).expect("utf-8");
        assert!(text.starts_with(PROFILE_PROJECTION_MAGIC));
        // The fixture's public key bytes must not appear in the projection.
        let public_bytes = signer.identity().public_key().as_bytes();
        assert!(
            !text.contains(&encode_hex(public_bytes)),
            "the projection must not carry key material"
        );
    }

    // --- Plan 488 invariant probes -------------------------------------------------
    //
    // Each of the seven Plan 488 invariants needs an executable probe that
    // fails when the invariant is broken. The probes below cover the three
    // that are not already covered by a dedicated test.

    /// Invariant 3: a peer is never classified without a validated
    /// RouterInfo. Every profile has a peer record, and removing a peer
    /// removes its profile with it.
    #[test]
    fn invariant_every_profile_has_a_validated_peer_record() {
        let mut store = ProfileStore::default();
        let signer = bundle(0x915);
        store.insert(validate(&signer, 1), 1);
        let key = validate(&signer, 1).key();
        assert_eq!(store.len(), store.peers().len());
        assert!(store.peers().contains(&key));
        assert!(store.remove(&key));
        assert_eq!(store.len(), 0);
        assert_eq!(store.peers().len(), 0);
    }

    /// Invariant 4: the Plan 103 ceilings keep their current defaults and
    /// their current semantics; the tiering is layered on top.
    #[test]
    fn invariant_the_existing_store_ceilings_still_hold() {
        let defaults = ProfileStoreConfig::default();
        assert_eq!(
            defaults.router_info,
            RouterInfoStoreConfig::default(),
            "the Plan 103 record and byte ceilings must keep their defaults"
        );
        let store = ProfileStore::default();
        assert_eq!(store.peers().config(), RouterInfoStoreConfig::default());

        // A store configured with a single record never retains more than
        // one record: the new peer either evicts the old one or is refused.
        let mut bounded = ProfileStore::new(ProfileStoreConfig::new(
            RouterInfoStoreConfig::new(1, usize::MAX),
            1,
            1,
            1,
            1,
        ));
        bounded.insert(validate(&bundle(0x916), 1), 1);
        bounded.insert(validate(&bundle(0x917), 1), 1);
        assert_eq!(bounded.peers().len(), 1);
        assert_eq!(bounded.len(), 1);

        // The byte ceiling keeps its semantics too: a store with no byte
        // budget admits nothing.
        let mut no_bytes = ProfileStore::new(ProfileStoreConfig::new(
            RouterInfoStoreConfig::new(usize::MAX, 0),
            1,
            1,
            1,
            1,
        ));
        assert_eq!(
            no_bytes.insert(validate(&bundle(0x918), 1), 1),
            ProfileInsertOutcome::CapacityExceeded
        );
        assert!(no_bytes.is_empty());
    }

    /// Invariant 6: the profile store advertises nothing. It classifies a
    /// peer's signed capabilities; it never rewrites them, and it never
    /// touches the router's own declared version.
    #[test]
    fn invariant_the_profile_store_advertises_nothing() {
        let mut store = ProfileStore::default();
        let signer = bundle(0x919);
        let validated = validate_with_options(&signer, 1, caps_options("Nf"));
        let key = validated.key();
        store.insert(validated, 1);
        let stored = store.peers().get(&key).expect("stored record");
        let caps = stored
            .router_info()
            .capabilities()
            .expect("caps")
            .expect("caps present");
        assert_eq!(caps.as_str(), "Nf", "the record's caps must be untouched");
        assert_eq!(
            crate::local::CONTROLLED_ROUTER_VERSION,
            "0.9.62",
            "the declared router version must stay pinned"
        );
    }

    /// Adoption classifies every retained peer and keeps the adopted
    /// store's own record and byte ceilings.
    #[test]
    fn adoption_classifies_every_retained_peer() {
        let mut source = RouterInfoStore::with_config(RouterInfoStoreConfig::default());
        let high = bundle(0x930);
        let standard = bundle(0x931);
        source.insert(validate_with_options(&high, 1, caps_options("Nf")));
        source.insert(validate_with_options(&standard, 1, caps_options("L")));

        let adopted = ProfileStore::adopt(ProfileStoreConfig::default(), source, 5);
        assert_eq!(adopted.len(), 2);
        assert_eq!(adopted.peers().len(), 2);
        assert_eq!(
            adopted.peers().config(),
            RouterInfoStoreConfig::default(),
            "adoption keeps the adopted store's ceilings"
        );

        let high_key = validate_with_options(&high, 1, caps_options("Nf")).key();
        let high_profile = adopted.profile(&high_key).expect("profile");
        assert_eq!(high_profile.tier(), ProfileTier::HighCapacity);
        assert!(high_profile.advertises_floodfill());

        let standard_key = validate_with_options(&standard, 1, caps_options("L")).key();
        let standard_profile = adopted.profile(&standard_key).expect("profile");
        assert_eq!(standard_profile.tier(), ProfileTier::Standard);
        assert!(!standard_profile.advertises_floodfill());
    }

    /// A strictly newer record replaces the old one and re-classifies the
    /// peer, but the peer's observed state survives: the peer is the same
    /// identity, so what was learned about it still holds.
    #[test]
    fn a_replacement_reclassifies_the_peer_and_keeps_its_observed_state() {
        let mut store = ProfileStore::default();
        let signer = bundle(0x932);
        store.insert(validate_with_options(&signer, 1, caps_options("L")), 1);
        let key = validate_with_options(&signer, 1, caps_options("L")).key();
        store.observe_answered(&key, 2);
        let outcome = store.insert(validate_with_options(&signer, 100, caps_options("Nf")), 100);
        assert_eq!(outcome, ProfileInsertOutcome::Replaced);
        let profile = store.profile(&key).expect("profile");
        assert_eq!(profile.tier(), ProfileTier::HighCapacity);
        assert_eq!(
            profile.state(),
            ProfileState::Answered,
            "the observed state survives a replacement"
        );
    }

    /// Invariant 7: the same store contents produce the same bytes, not
    /// merely the same selection.
    #[test]
    fn invariant_the_projection_encoding_is_deterministic() {
        let mut store = ProfileStore::default();
        for index in 0..4u64 {
            store.insert(validate(&bundle(0x920 + index), 1), 1);
        }
        let first = store.encode_projection().expect("encode");
        let second = store.encode_projection().expect("encode");
        assert_eq!(first, second, "the encoding must be byte-for-byte stable");

        // The encoding is bound to the contents: a different store produces
        // different bytes.
        let mut other = ProfileStore::default();
        other.insert(validate(&bundle(0x924), 1), 1);
        assert_ne!(other.encode_projection().expect("encode"), first);
    }
}
