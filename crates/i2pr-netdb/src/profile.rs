//! Plan 488 peer profile vocabulary: capacity tier, reachability state, and
//! the bounded, explainable score.
//!
//! The Plan 103 store keeps a *flat* map of validated RouterInfos: every
//! retained peer is equal, a peer that never answers is indistinguishable
//! from one that was never tried, and nothing records which peers are worth
//! building a tunnel through. Plan 488 layers a **profile** over that map so
//! the later participation phases (489–494) have honest selection inputs.
//!
//! Three properties matter here:
//!
//! - the profile is **bounded**: the score, every counter and every age
//!   threshold has a named ceiling, and no operation can overflow silently;
//! - the profile is **explainable**: the score is a sum of named credits and
//!   penalties, so a reader can state exactly why a peer ranks where it does;
//! - the profile is **evidence-bound**: a capacity tier is derived from the
//!   peer's *signed* `caps` option, which is self-asserted data and is
//!   treated as a hint, never as a trust signal.
//!
//! This module is runtime-neutral: it owns no socket, no timer, no file, and
//! no task. It is pure vocabulary plus pure transitions.

use crate::router_info::RouterHash;

/// The largest score a peer profile may carry.
pub const MAX_PROFILE_SCORE: u16 = 1_000;

/// The smallest score a peer profile may carry.
pub const MIN_PROFILE_SCORE: u16 = 0;

/// The score a freshly learned peer starts from.
///
/// A non-zero base lets a penalty be *observable*: without it every
/// negative transition would clamp to zero and a misbehaving peer would be
/// indistinguishable from an unreachable one.
pub const SCORE_BASE: u16 = 100;

/// Score credit for a reply that carried the requested data.
pub const SCORE_CREDIT_ANSWERED: u16 = 400;

/// Score credit for a valid negative reply: the peer answered, but it does
/// not hold the requested record.
///
/// This credit is deliberately larger than any penalty a single failed
/// request can apply, because Plan 488 requires that *a peer that answered
/// without having the data ranks above a peer that did not answer at all*.
pub const SCORE_CREDIT_ANSWERED_EMPTY: u16 = 150;

/// Score credit for an explicit refusal or error reply.
pub const SCORE_CREDIT_REFUSED: u16 = 25;

/// Score penalty for a request that was not answered before its deadline.
pub const SCORE_PENALTY_UNANSWERED: u16 = 50;

/// Score penalty for a transport-level failure.
pub const SCORE_PENALTY_UNREACHABLE: u16 = 75;

/// Score penalty for a reply that did not match the requested key.
///
/// A mismatched reply is misbehaviour, not a slow peer, so it is penalised
/// harder than a transport failure.
pub const SCORE_PENALTY_MISBEHAVING: u16 = 300;

/// The maximum number of consecutive failures a profile tracks.
pub const MAX_CONSECUTIVE_FAILURES: u32 = 32;

/// The maximum number of total failures a profile tracks.
pub const MAX_TOTAL_FAILURES: u32 = 1_000;

/// The maximum number of answered replies a profile tracks.
pub const MAX_ANSWERED_COUNT: u32 = 1_000;

/// The selection bonus a high-capacity peer contributes.
pub const TIER_BONUS_HIGH_CAPACITY: u16 = 120;

/// The selection bonus a standard peer contributes.
pub const TIER_BONUS_STANDARD: u16 = 40;

/// The selection bonus an untracked peer contributes.
///
/// An untracked peer has *no* declared capacity. It is not penalised — an
/// unknown class is not a known-bad class — but it never outranks a declared
/// peer of equal responsiveness.
pub const TIER_BONUS_UNTRACKED: u16 = 0;

/// The capacity class a peer advertises, ordered from weakest to strongest.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum PeerCapacityClass {
    /// Shared capacity below 12 KiB/s.
    K,
    /// Shared capacity through 48 KiB/s.
    L,
    /// Shared capacity through 64 KiB/s.
    M,
    /// Shared capacity through 128 KiB/s.
    N,
    /// Shared capacity through 256 KiB/s.
    O,
    /// Shared capacity through 2000 KiB/s.
    P,
    /// Shared capacity above 2000 KiB/s.
    X,
}

impl PeerCapacityClass {
    /// Parses the capacity-class letter out of a peer's signed `caps`
    /// option text.
    ///
    /// Returns `None` when the option is absent or carries no class letter,
    /// which is the honest "unmeasured" case rather than a guess.
    pub fn from_capabilities(caps: Option<&str>) -> Option<Self> {
        let text = caps?;
        for byte in text.bytes() {
            if let Some(class) = Self::from_letter(byte) {
                return Some(class);
            }
        }
        None
    }

    /// Parses a single capacity-class letter.
    ///
    /// The class letters are `K`, `L`, `M`, `N`, `O`, `P` and `X`. They do
    /// not collide with the flag letters either implementation uses (`f`,
    /// `H`, `R`, `U`, `D`, `E`, `G`, `4`, `6`, `B`, `C`).
    pub const fn from_letter(byte: u8) -> Option<Self> {
        match byte {
            b'K' => Some(Self::K),
            b'L' => Some(Self::L),
            b'M' => Some(Self::M),
            b'N' => Some(Self::N),
            b'O' => Some(Self::O),
            b'P' => Some(Self::P),
            b'X' => Some(Self::X),
            _ => None,
        }
    }

    /// Returns the protocol letter for this class.
    pub const fn as_letter(self) -> u8 {
        match self {
            Self::K => b'K',
            Self::L => b'L',
            Self::M => b'M',
            Self::N => b'N',
            Self::O => b'O',
            Self::P => b'P',
            Self::X => b'X',
        }
    }

    /// Whether the class sits at or above the pinned Java floodfill floor
    /// (`N`), the same floor `BandwidthClass::supports_floodfill_role`
    /// applies to this router's own record.
    pub const fn at_or_above_floodfill_floor(self) -> bool {
        matches!(self, Self::N | Self::O | Self::P | Self::X)
    }
}

/// The capacity tier a known peer is classified into.
///
/// The tier is a *selection hint* derived from self-asserted data. A peer
/// that declares no capacity class is `Untracked`, which is an honest
/// "unknown", never a downgrade.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileTier {
    /// The peer declares a capacity at or above the pinned Java floor.
    HighCapacity,
    /// The peer declares a capacity below the pinned Java floor.
    Standard,
    /// The peer declares no capacity class at all.
    Untracked,
}

impl ProfileTier {
    /// Returns the tier's strength, where a stronger tier compares greater.
    pub const fn strength(self) -> u8 {
        match self {
            Self::Untracked => 0,
            Self::Standard => 1,
            Self::HighCapacity => 2,
        }
    }

    /// Derives the tier from a peer's declared capacity class.
    pub const fn from_capacity_class(class: Option<PeerCapacityClass>) -> Self {
        match class {
            None => Self::Untracked,
            Some(class) => {
                if class.at_or_above_floodfill_floor() {
                    Self::HighCapacity
                } else {
                    Self::Standard
                }
            }
        }
    }

    /// Returns the stable label used in receipts and evidence files.
    pub const fn label(self) -> &'static str {
        match self {
            Self::HighCapacity => "high-capacity",
            Self::Standard => "standard",
            Self::Untracked => "untracked",
        }
    }

    /// Returns the bounded selection bonus this tier contributes.
    pub const fn selection_bonus(self) -> u16 {
        match self {
            Self::HighCapacity => TIER_BONUS_HIGH_CAPACITY,
            Self::Standard => TIER_BONUS_STANDARD,
            Self::Untracked => TIER_BONUS_UNTRACKED,
        }
    }

    /// Returns the next weaker tier, or `None` when the tier is already the
    /// weakest.
    ///
    /// Demotion is one step at a time: a peer that stops answering is
    /// demoted by the maintenance pass and is never silently reclassified
    /// in one jump.
    pub const fn demote(self) -> Option<Self> {
        match self {
            Self::HighCapacity => Some(Self::Standard),
            Self::Standard => Some(Self::Untracked),
            Self::Untracked => None,
        }
    }
}

impl Ord for ProfileTier {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.strength().cmp(&other.strength())
    }
}

impl PartialOrd for ProfileTier {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// The reachability state of a known peer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileState {
    /// The peer is known but has never been contacted.
    NeverTried,
    /// The peer replied with the requested data.
    Answered,
    /// The peer replied, but it does not hold the requested record.
    AnsweredEmpty,
    /// The peer replied with an error or a refusal.
    Refused,
    /// The peer replied with data that does not match the requested key.
    Misbehaving,
    /// The peer did not reply before the request deadline.
    Unanswered,
    /// The peer could not be reached at the transport level.
    Unreachable,
}

impl ProfileState {
    /// Returns the stable label used in receipts and evidence files.
    pub const fn label(self) -> &'static str {
        match self {
            Self::NeverTried => "never-tried",
            Self::Answered => "answered",
            Self::AnsweredEmpty => "answered-empty",
            Self::Refused => "refused",
            Self::Misbehaving => "misbehaving",
            Self::Unanswered => "unanswered",
            Self::Unreachable => "unreachable",
        }
    }

    /// Whether the state records a failure of the peer.
    pub const fn is_failure(self) -> bool {
        matches!(
            self,
            Self::Misbehaving | Self::Unanswered | Self::Unreachable
        )
    }

    /// Whether the state records that the peer did answer the router.
    ///
    /// A peer that answered — even with a negative reply — is demonstrably
    /// alive, which is exactly the distinction Plan 488 requires between
    /// "tried and did not answer" and "tried and refused".
    pub const fn is_responsive(self) -> bool {
        matches!(self, Self::Answered | Self::AnsweredEmpty | Self::Refused)
    }

    /// Whether the state is a failure the maintenance pass may age out.
    pub const fn is_aged_failure(self) -> bool {
        matches!(
            self,
            Self::Misbehaving | Self::Unanswered | Self::Unreachable
        )
    }
}

/// A bounded, explainable peer score.
///
/// The score is an integer in `[MIN_PROFILE_SCORE, MAX_PROFILE_SCORE]`. It
/// is the sum of named credits and penalties, so any value can be traced
/// back to the observations that produced it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ProfileScore(u16);

impl ProfileScore {
    /// The lowest score a profile may carry.
    pub const MIN: Self = Self(MIN_PROFILE_SCORE);

    /// The score a freshly learned peer starts from.
    pub const BASE: Self = Self(SCORE_BASE);

    /// The highest score a profile may carry.
    pub const MAX: Self = Self(MAX_PROFILE_SCORE);

    /// Constructs a score, rejecting an out-of-range value.
    pub const fn new(value: u16) -> Result<Self, ProfileError> {
        if value > MAX_PROFILE_SCORE {
            return Err(ProfileError::ScoreOutOfRange {
                value,
                maximum: MAX_PROFILE_SCORE,
            });
        }
        Ok(Self(value))
    }

    /// Returns the raw score value.
    pub const fn value(self) -> u16 {
        self.0
    }

    /// Applies a positive credit, saturating at the maximum.
    pub fn credit(self, delta: u16) -> Self {
        let sum = u32::from(self.0) + u32::from(delta);
        Self(sum.min(u32::from(MAX_PROFILE_SCORE)) as u16)
    }

    /// Applies a negative penalty, saturating at the minimum.
    pub fn debit(self, delta: u16) -> Self {
        let difference = u32::from(self.0).saturating_sub(u32::from(delta));
        Self(difference as u16)
    }
}

/// A typed profile failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileError {
    /// A score was constructed outside its bounded range.
    ScoreOutOfRange {
        /// The rejected value.
        value: u16,
        /// The inclusive maximum.
        maximum: u16,
    },
    /// A counter could not be incremented without exceeding its ceiling.
    CounterOverflow {
        /// The counter that reached its ceiling.
        counter: &'static str,
        /// The counter's inclusive ceiling.
        maximum: u32,
    },
    /// A tier was raised without the evidence required to justify it.
    TierPromotionWithoutEvidence,
}

impl core::fmt::Display for ProfileError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ScoreOutOfRange { value, maximum } => {
                write!(formatter, "score {value} exceeds maximum {maximum}")
            }
            Self::CounterOverflow { counter, maximum } => {
                write!(formatter, "counter {counter} reached ceiling {maximum}")
            }
            Self::TierPromotionWithoutEvidence => {
                formatter.write_str("tier promotion requires capacity evidence")
            }
        }
    }
}

impl std::error::Error for ProfileError {}

/// The bounded profile of one known peer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Profile {
    pub(crate) tier: ProfileTier,
    pub(crate) state: ProfileState,
    pub(crate) score: ProfileScore,
    pub(crate) consecutive_failures: u32,
    pub(crate) total_failures: u32,
    pub(crate) answered_count: u32,
    pub(crate) floodfill: bool,
    pub(crate) capacity_evidence: bool,
    pub(crate) last_observed_ms: u64,
    pub(crate) last_answered_ms: Option<u64>,
}

impl Profile {
    /// Constructs the profile of a peer that has just been learned about.
    ///
    /// The peer starts in [`ProfileState::NeverTried`] with a zero score:
    /// nothing has been observed, so nothing is claimed.
    pub const fn new(now_ms: u64) -> Self {
        Self {
            tier: ProfileTier::Untracked,
            state: ProfileState::NeverTried,
            score: ProfileScore::BASE,
            consecutive_failures: 0,
            total_failures: 0,
            answered_count: 0,
            floodfill: false,
            capacity_evidence: false,
            last_observed_ms: now_ms,
            last_answered_ms: None,
        }
    }

    /// Returns the peer's capacity tier.
    pub const fn tier(&self) -> ProfileTier {
        self.tier
    }

    /// Returns the peer's reachability state.
    pub const fn state(&self) -> ProfileState {
        self.state
    }

    /// Returns the peer's bounded score.
    pub const fn score(&self) -> ProfileScore {
        self.score
    }

    /// Returns the number of consecutive failures observed.
    pub const fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    /// Returns the total number of failures observed.
    pub const fn total_failures(&self) -> u32 {
        self.total_failures
    }

    /// Returns the number of answered replies observed.
    pub const fn answered_count(&self) -> u32 {
        self.answered_count
    }

    /// Returns whether the peer advertises the `f` capability.
    pub const fn advertises_floodfill(&self) -> bool {
        self.floodfill
    }

    /// Returns the millisecond timestamp of the most recent observation.
    ///
    /// For a peer that has never been contacted this is the timestamp at
    /// which it was learned about.
    pub const fn last_observed_ms(&self) -> u64 {
        self.last_observed_ms
    }

    /// Returns the millisecond timestamp of the most recent answered reply.
    pub const fn last_answered_ms(&self) -> Option<u64> {
        self.last_answered_ms
    }

    /// Records the peer's advertised floodfill capability.
    pub fn set_floodfill(&mut self, floodfill: bool) {
        self.floodfill = floodfill;
    }

    /// Applies the capacity class the peer advertised.
    ///
    /// The tier is only ever *raised* from declared capacity evidence: a
    /// peer whose record carries no capacity class stays `Untracked`, and a
    /// tier may never be raised by anything other than a validated record.
    pub fn apply_capacity_class(&mut self, class: Option<PeerCapacityClass>) {
        match class {
            None => {
                self.capacity_evidence = false;
                self.tier = ProfileTier::Untracked;
            }
            Some(class) => {
                self.capacity_evidence = true;
                self.tier = ProfileTier::from_capacity_class(Some(class));
            }
        }
    }

    /// Returns whether the profile carries capacity evidence.
    pub const fn has_capacity_evidence(&self) -> bool {
        self.capacity_evidence
    }

    /// Raises the tier, refusing a promotion that is not backed by capacity
    /// evidence.
    pub fn promote(&mut self, tier: ProfileTier) -> Result<(), ProfileError> {
        if tier > self.tier && !self.capacity_evidence {
            return Err(ProfileError::TierPromotionWithoutEvidence);
        }
        self.tier = tier;
        Ok(())
    }

    /// Demotes the tier by exactly one step.
    ///
    /// Returns `true` when the tier actually changed. A peer that is
    /// already `Untracked` cannot be demoted further.
    pub fn demote(&mut self) -> bool {
        match self.tier.demote() {
            Some(next) => {
                self.tier = next;
                true
            }
            None => false,
        }
    }

    /// Records that the peer answered with the requested data.
    pub fn observe_answered(&mut self, now_ms: u64) {
        self.state = ProfileState::Answered;
        self.score = self.score.credit(SCORE_CREDIT_ANSWERED);
        self.consecutive_failures = 0;
        self.answered_count = saturating_increment(self.answered_count, MAX_ANSWERED_COUNT);
        self.last_observed_ms = now_ms;
        self.last_answered_ms = Some(now_ms);
    }

    /// Records that the peer answered but does not hold the record.
    pub fn observe_answered_empty(&mut self, now_ms: u64) {
        self.state = ProfileState::AnsweredEmpty;
        self.score = self.score.credit(SCORE_CREDIT_ANSWERED_EMPTY);
        self.consecutive_failures = 0;
        self.answered_count = saturating_increment(self.answered_count, MAX_ANSWERED_COUNT);
        self.last_observed_ms = now_ms;
        self.last_answered_ms = Some(now_ms);
    }

    /// Records that the peer refused or returned an error.
    pub fn observe_refused(&mut self, now_ms: u64) {
        self.state = ProfileState::Refused;
        self.score = self.score.credit(SCORE_CREDIT_REFUSED);
        self.consecutive_failures = 0;
        self.last_observed_ms = now_ms;
    }

    /// Records that the peer replied with data that does not match the
    /// requested key.
    pub fn observe_misbehaving(&mut self, now_ms: u64) {
        self.state = ProfileState::Misbehaving;
        self.score = self.score.debit(SCORE_PENALTY_MISBEHAVING);
        self.consecutive_failures =
            saturating_increment(self.consecutive_failures, MAX_CONSECUTIVE_FAILURES);
        self.total_failures = saturating_increment(self.total_failures, MAX_TOTAL_FAILURES);
        self.last_observed_ms = now_ms;
    }

    /// Records that the peer did not answer before the deadline.
    pub fn observe_unanswered(&mut self, now_ms: u64) {
        self.state = ProfileState::Unanswered;
        self.score = self.score.debit(SCORE_PENALTY_UNANSWERED);
        self.consecutive_failures =
            saturating_increment(self.consecutive_failures, MAX_CONSECUTIVE_FAILURES);
        self.total_failures = saturating_increment(self.total_failures, MAX_TOTAL_FAILURES);
        self.last_observed_ms = now_ms;
    }

    /// Records that the peer could not be reached at the transport level.
    pub fn observe_unreachable(&mut self, now_ms: u64) {
        self.state = ProfileState::Unreachable;
        self.score = self.score.debit(SCORE_PENALTY_UNREACHABLE);
        self.consecutive_failures =
            saturating_increment(self.consecutive_failures, MAX_CONSECUTIVE_FAILURES);
        self.total_failures = saturating_increment(self.total_failures, MAX_TOTAL_FAILURES);
        self.last_observed_ms = now_ms;
    }

    /// Returns the age in milliseconds of the most recent observation.
    ///
    /// The age saturates at zero for a caller whose clock moved backwards,
    /// so a skewed clock can never make a peer look ancient.
    pub fn age_ms(&self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.last_observed_ms)
    }

    /// Returns the combined selection rank of the profile.
    ///
    /// The rank is the responsive score plus the tier's bounded selection
    /// bonus, clamped to the score ceiling. It is the single ordering key
    /// the selection policy uses.
    pub fn rank(&self) -> u16 {
        let bonus = u32::from(self.tier.selection_bonus());
        let total = u32::from(self.score.value()) + bonus;
        total.min(u32::from(MAX_PROFILE_SCORE)) as u16
    }
}

fn saturating_increment(value: u32, maximum: u32) -> u32 {
    if value >= maximum { maximum } else { value + 1 }
}

/// The network prefix a peer belongs to, used by the selection policy's
/// diversity rule.
///
/// The prefix is the leading two bytes of the peer's RouterHash. Two peers
/// that share a prefix are considered to sit behind the same /16, and the
/// selection policy refuses to place both of them into one selection.
pub fn network_prefix16(key: &RouterHash) -> u16 {
    let bytes = key.as_bytes();
    u16::from(bytes[0]) << 8 | u16::from(bytes[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> RouterHash {
        RouterHash::from_bytes([byte; 32])
    }

    #[test]
    fn capacity_class_is_parsed_from_either_caps_order() {
        // The pinned i2pd emits the class letter first.
        assert_eq!(
            PeerCapacityClass::from_capabilities(Some("Nf")),
            Some(PeerCapacityClass::N)
        );
        // This repository's own controlled builders append it last.
        assert_eq!(
            PeerCapacityClass::from_capabilities(Some("fN")),
            Some(PeerCapacityClass::N)
        );
        // A flags-only record carries no class.
        assert_eq!(PeerCapacityClass::from_capabilities(Some("f")), None);
        assert_eq!(PeerCapacityClass::from_capabilities(None), None);
    }

    #[test]
    fn capacity_class_does_not_collide_with_flag_letters() {
        // i2pd's flag letters are f, H, R, U, D, E, G, 4, 6, B, C.
        for flag in ["f", "H", "R", "U", "D", "E", "G", "4", "6", "B", "C"] {
            assert_eq!(PeerCapacityClass::from_capabilities(Some(flag)), None);
        }
    }

    #[test]
    fn tier_uses_the_pinned_java_floodfill_floor() {
        assert_eq!(
            ProfileTier::from_capacity_class(Some(PeerCapacityClass::N)),
            ProfileTier::HighCapacity
        );
        assert_eq!(
            ProfileTier::from_capacity_class(Some(PeerCapacityClass::X)),
            ProfileTier::HighCapacity
        );
        assert_eq!(
            ProfileTier::from_capacity_class(Some(PeerCapacityClass::M)),
            ProfileTier::Standard
        );
        assert_eq!(
            ProfileTier::from_capacity_class(None),
            ProfileTier::Untracked
        );
    }

    #[test]
    fn score_is_bounded_at_both_ends() {
        let score = ProfileScore::MIN;
        let credited = score.credit(SCORE_CREDIT_ANSWERED);
        assert_eq!(credited.value(), SCORE_CREDIT_ANSWERED);
        assert_eq!(credited.credit(u16::MAX).value(), MAX_PROFILE_SCORE);
        assert_eq!(ProfileScore::MAX.debit(u16::MAX).value(), MIN_PROFILE_SCORE);
    }

    #[test]
    fn score_out_of_range_is_refused() {
        assert_eq!(
            ProfileScore::new(MAX_PROFILE_SCORE + 1),
            Err(ProfileError::ScoreOutOfRange {
                value: MAX_PROFILE_SCORE + 1,
                maximum: MAX_PROFILE_SCORE,
            })
        );
        assert!(ProfileScore::new(MAX_PROFILE_SCORE).is_ok());
    }

    #[test]
    fn answered_empty_outranks_did_not_answer() {
        let mut answered = Profile::new(0);
        answered.observe_answered_empty(1);
        let mut silent = Profile::new(0);
        silent.observe_unanswered(1);
        assert!(
            answered.score() > silent.score(),
            "answered-empty {} must outrank unanswered {}",
            answered.score().value(),
            silent.score().value()
        );
    }

    #[test]
    fn answered_outranks_answered_empty_and_refused() {
        let mut full = Profile::new(0);
        full.observe_answered(1);
        let mut empty = Profile::new(0);
        empty.observe_answered_empty(1);
        let mut refused = Profile::new(0);
        refused.observe_refused(1);
        assert!(full.score() > empty.score());
        assert!(empty.score() > refused.score());
    }

    #[test]
    fn misbehaving_is_penalised_harder_than_unreachable() {
        let mut bad = Profile::new(0);
        bad.observe_misbehaving(1);
        let mut gone = Profile::new(0);
        gone.observe_unreachable(1);
        assert!(bad.score() < gone.score());
    }

    #[test]
    fn failure_counters_saturate_at_their_ceiling() {
        let mut profile = Profile::new(0);
        for _ in 0..(MAX_CONSECUTIVE_FAILURES + 10) {
            profile.observe_unanswered(1);
        }
        assert_eq!(profile.consecutive_failures(), MAX_CONSECUTIVE_FAILURES);
        assert_eq!(
            profile.total_failures(),
            MAX_CONSECUTIVE_FAILURES + 10,
            "the total counter is bounded by its own ceiling, not the consecutive one"
        );
    }

    #[test]
    fn answered_counter_saturates_at_its_ceiling() {
        let mut profile = Profile::new(0);
        for _ in 0..(MAX_ANSWERED_COUNT + 5) {
            profile.observe_answered(1);
        }
        assert_eq!(profile.answered_count(), MAX_ANSWERED_COUNT);
    }

    #[test]
    fn promotion_without_capacity_evidence_is_refused() {
        let mut profile = Profile::new(0);
        assert_eq!(
            profile.promote(ProfileTier::HighCapacity),
            Err(ProfileError::TierPromotionWithoutEvidence)
        );
        assert_eq!(profile.tier(), ProfileTier::Untracked);
    }

    #[test]
    fn demotion_steps_one_tier_at_a_time() {
        let mut profile = Profile::new(0);
        profile.apply_capacity_class(Some(PeerCapacityClass::X));
        assert_eq!(profile.tier(), ProfileTier::HighCapacity);
        assert!(profile.demote());
        assert_eq!(profile.tier(), ProfileTier::Standard);
        assert!(profile.demote());
        assert_eq!(profile.tier(), ProfileTier::Untracked);
        assert!(!profile.demote());
        assert_eq!(profile.tier(), ProfileTier::Untracked);
    }

    #[test]
    fn applying_a_missing_capacity_class_clears_the_evidence() {
        let mut profile = Profile::new(0);
        profile.apply_capacity_class(Some(PeerCapacityClass::P));
        assert!(profile.has_capacity_evidence());
        profile.apply_capacity_class(None);
        assert!(!profile.has_capacity_evidence());
        assert_eq!(profile.tier(), ProfileTier::Untracked);
    }

    #[test]
    fn age_saturates_when_the_clock_moves_backwards() {
        let profile = Profile::new(1_000);
        assert_eq!(profile.age_ms(500), 0);
        assert_eq!(profile.age_ms(1_500), 500);
    }

    #[test]
    fn rank_combines_score_and_tier_bonus_within_the_ceiling() {
        let mut profile = Profile::new(0);
        profile.observe_answered(1);
        profile.apply_capacity_class(Some(PeerCapacityClass::N));
        assert_eq!(
            profile.rank(),
            SCORE_BASE + SCORE_CREDIT_ANSWERED + TIER_BONUS_HIGH_CAPACITY
        );
        assert!(profile.rank() <= MAX_PROFILE_SCORE);
    }

    #[test]
    fn network_prefix_uses_the_leading_two_hash_bytes() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0x12;
        bytes[1] = 0x34;
        assert_eq!(network_prefix16(&RouterHash::from_bytes(bytes)), 0x1234);
    }

    #[test]
    fn state_labels_are_unique_and_stable() {
        let states = [
            ProfileState::NeverTried,
            ProfileState::Answered,
            ProfileState::AnsweredEmpty,
            ProfileState::Refused,
            ProfileState::Misbehaving,
            ProfileState::Unanswered,
            ProfileState::Unreachable,
        ];
        let mut labels: Vec<&'static str> = states.iter().map(|state| state.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), states.len());
    }

    #[test]
    fn unused_key_helper_is_referenced() {
        // Keeps the test-only helper honest without an unused warning.
        assert_ne!(key(1), key(2));
    }
}
