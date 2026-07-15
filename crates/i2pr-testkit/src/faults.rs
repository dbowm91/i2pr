use std::fmt;
use std::time::Duration;

use crate::rng::ReproducibilitySeed;

/// Maximum number of executable fault rules in one script.
pub const MAX_FAULT_RULES: usize = 64;
/// Maximum extra units one duplicate action may generate.
pub const MAX_DUPLICATE_UNITS: usize = 8;

/// Synthetic link identifier. It is never an IP address or a peer-derived channel name.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinkId(u32);

impl LinkId {
    /// Creates a nonzero synthetic link identifier.
    pub const fn new(value: u32) -> Result<Self, FaultError> {
        if value == 0 {
            Err(FaultError::ZeroValue)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the stable numeric identifier.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Direction of a unit across a synthetic link.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkDirection {
    /// First endpoint to second endpoint.
    AtoB,
    /// Second endpoint to first endpoint.
    BtoA,
}

/// Unit category used by bounded fault matching.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FaultUnitKind {
    /// One scheduled stream segment.
    Stream,
    /// One complete datagram.
    Datagram,
}

/// Executable fault action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultAction {
    /// Discard the affected unit.
    Drop,
    /// Add a deterministic delay to the affected unit.
    Delay(Duration),
    /// Deliver the original plus the requested number of copies.
    Duplicate { copies: u8 },
    /// Reverse ordering in bounded groups of this size.
    Reorder { window: u16 },
    /// Retain no more than this many bytes. Stream truncation discards the
    /// remainder of the scheduled segment; datagram truncation delivers a
    /// shorter datagram.
    Truncate { max_bytes: u32 },
    /// Graceful disconnect: queued bytes drain, then readers observe EOF.
    Disconnect,
    /// Reset: queued bytes are discarded and readers observe reset.
    Reset,
}

impl FaultAction {
    /// Creates a duplicate action with at least one extra copy.
    pub const fn duplicate(copies: u8) -> Result<Self, FaultError> {
        if copies == 0 || copies as usize > MAX_DUPLICATE_UNITS {
            Err(FaultError::ExpansionLimit)
        } else {
            Ok(Self::Duplicate { copies })
        }
    }

    /// Creates a bounded reorder action.
    pub const fn reorder(window: u16) -> Result<Self, FaultError> {
        if window == 0 {
            Err(FaultError::ZeroValue)
        } else {
            Ok(Self::Reorder { window })
        }
    }

    /// Creates a nonzero truncation action.
    pub const fn truncate(max_bytes: u32) -> Result<Self, FaultError> {
        if max_bytes == 0 {
            Err(FaultError::ZeroValue)
        } else {
            Ok(Self::Truncate { max_bytes })
        }
    }
}

/// Errors returned while validating a fault script.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultError {
    /// A count, window, or byte bound was zero.
    ZeroValue,
    /// A duplicate action could exceed the hard expansion bound.
    ExpansionLimit,
    /// A delay would overflow the monotonic timeline.
    DelayOverflow,
    /// A rule count exceeded the hard bound.
    TooManyRules,
    /// A probability was outside 0..=1,000,000 parts per million.
    ProbabilityOutOfRange,
}

impl fmt::Display for FaultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroValue => formatter.write_str("fault action value must be nonzero"),
            Self::ExpansionLimit => formatter.write_str("fault duplicate expansion exceeds bound"),
            Self::DelayOverflow => formatter.write_str("fault delay overflows monotonic time"),
            Self::TooManyRules => formatter.write_str("fault script exceeds its rule bound"),
            Self::ProbabilityOutOfRange => formatter.write_str("fault probability is out of range"),
        }
    }
}

impl std::error::Error for FaultError {}

/// Bounded rule matcher. Unset fields match any corresponding unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FaultMatcher {
    link: Option<LinkId>,
    direction: Option<LinkDirection>,
    kind: Option<FaultUnitKind>,
    sequence: Option<u64>,
    range: Option<(u64, u64)>,
    every: Option<u64>,
    probability_ppm: Option<u32>,
}

impl FaultMatcher {
    /// Creates a matcher that matches every unit.
    pub const fn any() -> Self {
        Self {
            link: None,
            direction: None,
            kind: None,
            sequence: None,
            range: None,
            every: None,
            probability_ppm: None,
        }
    }

    /// Restricts the matcher to a link.
    pub const fn link(mut self, link: LinkId) -> Self {
        self.link = Some(link);
        self
    }

    /// Restricts the matcher to a direction.
    pub const fn direction(mut self, direction: LinkDirection) -> Self {
        self.direction = Some(direction);
        self
    }

    /// Restricts the matcher to stream or datagram units.
    pub const fn kind(mut self, kind: FaultUnitKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Restricts the matcher to one exact sequence number.
    pub const fn sequence(mut self, sequence: u64) -> Self {
        self.sequence = Some(sequence);
        self
    }

    /// Restricts the matcher to an inclusive sequence range.
    pub const fn sequence_range(mut self, start: u64, end: u64) -> Result<Self, FaultError> {
        if start > end {
            Err(FaultError::ZeroValue)
        } else {
            self.range = Some((start, end));
            Ok(self)
        }
    }

    /// Matches every `n`th unit, starting with sequence zero.
    pub const fn every(mut self, n: u64) -> Result<Self, FaultError> {
        if n == 0 {
            Err(FaultError::ZeroValue)
        } else {
            self.every = Some(n);
            Ok(self)
        }
    }

    /// Applies a deterministic per-unit probability in parts per million.
    pub const fn probability_ppm(mut self, probability: u32) -> Result<Self, FaultError> {
        if probability > 1_000_000 {
            Err(FaultError::ProbabilityOutOfRange)
        } else {
            self.probability_ppm = Some(probability);
            Ok(self)
        }
    }

    pub(crate) fn matches(
        &self,
        seed: ReproducibilitySeed,
        rule_id: u16,
        link: LinkId,
        direction: LinkDirection,
        kind: FaultUnitKind,
        sequence: u64,
    ) -> bool {
        if self.link.is_some_and(|value| value != link)
            || self.direction.is_some_and(|value| value != direction)
            || self.kind.is_some_and(|value| value != kind)
            || self.sequence.is_some_and(|value| value != sequence)
            || self
                .range
                .is_some_and(|(start, end)| sequence < start || sequence > end)
            || self.every.is_some_and(|value| sequence % value != 0)
        {
            return false;
        }
        let Some(probability) = self.probability_ppm else {
            return true;
        };
        let mut label = [0_u8; 16];
        label[..4].copy_from_slice(&link.get().to_be_bytes());
        label[4] = match direction {
            LinkDirection::AtoB => 0,
            LinkDirection::BtoA => 1,
        };
        label[5] = match kind {
            FaultUnitKind::Stream => 0,
            FaultUnitKind::Datagram => 1,
        };
        label[6..8].copy_from_slice(&rule_id.to_be_bytes());
        label[8..].copy_from_slice(&sequence.to_be_bytes());
        let derived = ReproducibilitySeed::derive_bytes(seed, &label);
        u32::from_be_bytes(
            derived.as_bytes()[..4]
                .try_into()
                .expect("fixed seed slice"),
        ) % 1_000_000
            < probability
    }
}

/// One bounded executable fault rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FaultRule {
    id: u16,
    matcher: FaultMatcher,
    action: FaultAction,
}

impl FaultRule {
    /// Creates a rule with a stable diagnostic identifier.
    pub const fn new(id: u16, matcher: FaultMatcher, action: FaultAction) -> Self {
        Self {
            id,
            matcher,
            action,
        }
    }

    /// Returns the rule identifier.
    pub const fn id(&self) -> u16 {
        self.id
    }

    /// Returns the rule action.
    pub const fn action(&self) -> FaultAction {
        self.action
    }
}

/// A validated, bounded list of deterministic fault rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FaultScript {
    seed: ReproducibilitySeed,
    rules: Vec<FaultRule>,
}

impl FaultScript {
    /// Creates a script and rejects excessive rule or duplicate expansion.
    pub fn new(seed: ReproducibilitySeed, rules: Vec<FaultRule>) -> Result<Self, FaultError> {
        if rules.len() > MAX_FAULT_RULES {
            return Err(FaultError::TooManyRules);
        }
        for rule in &rules {
            if let FaultAction::Duplicate { copies } = rule.action {
                if copies == 0 || copies as usize > MAX_DUPLICATE_UNITS {
                    return Err(FaultError::ExpansionLimit);
                }
            }
        }
        Ok(Self { seed, rules })
    }

    /// Creates an empty script.
    pub fn empty(seed: ReproducibilitySeed) -> Self {
        Self {
            seed,
            rules: Vec::new(),
        }
    }

    /// Returns the root seed used for probability decisions.
    pub const fn seed(&self) -> ReproducibilitySeed {
        self.seed
    }

    /// Returns rules in declaration order.
    pub fn rules(&self) -> &[FaultRule] {
        &self.rules
    }

    pub(crate) fn apply(
        &self,
        link: LinkId,
        direction: LinkDirection,
        kind: FaultUnitKind,
        sequence: u64,
        payload: Vec<u8>,
    ) -> Result<FaultPlan, FaultError> {
        let mut units = vec![PlannedFaultUnit {
            payload,
            delay: Duration::ZERO,
            order_sequence: sequence,
            duplicate_index: 0,
        }];
        let mut applied = Vec::new();
        let mut terminal = None;
        for rule in &self.rules {
            if !rule
                .matcher
                .matches(self.seed, rule.id, link, direction, kind, sequence)
            {
                continue;
            }
            applied.push(rule.id);
            match rule.action {
                FaultAction::Drop => {
                    units.clear();
                    terminal = Some(FaultTerminal::Drop);
                    break;
                }
                FaultAction::Delay(delay) => {
                    for unit in &mut units {
                        unit.delay = unit
                            .delay
                            .checked_add(delay)
                            .ok_or(FaultError::DelayOverflow)?;
                    }
                }
                FaultAction::Duplicate { copies } => {
                    let copies = usize::from(copies);
                    if units.len().checked_mul(copies.saturating_add(1)).is_none()
                        || units.len() * copies.saturating_add(1) > MAX_DUPLICATE_UNITS + 1
                    {
                        return Err(FaultError::ExpansionLimit);
                    }
                    let original = units.clone();
                    for duplicate_index in 1..=copies {
                        units.extend(original.iter().cloned().map(|mut unit| {
                            unit.duplicate_index = duplicate_index as u8;
                            unit
                        }));
                    }
                }
                FaultAction::Reorder { window } => {
                    let window = u64::from(window);
                    for unit in &mut units {
                        let group = sequence / window * window;
                        let offset = sequence % window;
                        unit.order_sequence = group + window.saturating_sub(offset + 1);
                    }
                }
                FaultAction::Truncate { max_bytes } => {
                    let maximum = max_bytes as usize;
                    for unit in &mut units {
                        unit.payload.truncate(maximum);
                    }
                }
                FaultAction::Disconnect => terminal = Some(FaultTerminal::Disconnect),
                FaultAction::Reset => {
                    units.clear();
                    terminal = Some(FaultTerminal::Reset);
                    break;
                }
            }
        }
        Ok(FaultPlan {
            units,
            applied,
            terminal,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FaultTerminal {
    Drop,
    Disconnect,
    Reset,
}

#[derive(Clone, Debug)]
pub(crate) struct PlannedFaultUnit {
    pub(crate) payload: Vec<u8>,
    pub(crate) delay: Duration,
    pub(crate) order_sequence: u64,
    pub(crate) duplicate_index: u8,
}

#[derive(Clone, Debug)]
pub(crate) struct FaultPlan {
    pub(crate) units: Vec<PlannedFaultUnit>,
    pub(crate) applied: Vec<u16>,
    pub(crate) terminal: Option<FaultTerminal>,
}
