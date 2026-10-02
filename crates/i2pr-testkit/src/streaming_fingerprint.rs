//! Bounded, identity-free observations for Streaming fingerprint tests.
//!
//! These types are test infrastructure only. They intentionally carry no
//! Destination hash, peer address, packet bytes, or application payload.

use std::fmt;

/// Maximum number of observations retained by one scenario.
pub const MAX_FINGERPRINT_EVENTS: usize = 4096;
/// Fingerprint traces use 10 ms relative-time buckets and stop at ten minutes.
pub const FINGERPRINT_TIME_BUCKET_MS: u64 = 10;
pub const FINGERPRINT_DEADLINE_MS: u64 = 600_000;
const MAX_TIME_BUCKET: u16 = (FINGERPRINT_DEADLINE_MS / FINGERPRINT_TIME_BUCKET_MS) as u16;

/// Fixed scenario vocabulary shared by all three implementations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FingerprintScenario {
    CleanHandshake,
    SmallPayload,
    SendWindowSaturation,
    DelayedFirstAck,
    AckWithheld,
    SingleLoss,
    DeterministicReorder,
    ConstrainedWindow,
    ChokeUnchoke,
    OrderlyClose,
    AbruptClose,
}

impl FingerprintScenario {
    const ALL: [Self; 11] = [
        Self::CleanHandshake,
        Self::SmallPayload,
        Self::SendWindowSaturation,
        Self::DelayedFirstAck,
        Self::AckWithheld,
        Self::SingleLoss,
        Self::DeterministicReorder,
        Self::ConstrainedWindow,
        Self::ChokeUnchoke,
        Self::OrderlyClose,
        Self::AbruptClose,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::CleanHandshake => "clean_handshake",
            Self::SmallPayload => "small_payload",
            Self::SendWindowSaturation => "send_window_saturation",
            Self::DelayedFirstAck => "delayed_first_ack",
            Self::AckWithheld => "ack_withheld",
            Self::SingleLoss => "single_loss",
            Self::DeterministicReorder => "deterministic_reorder",
            Self::ConstrainedWindow => "constrained_window",
            Self::ChokeUnchoke => "choke_unchoke",
            Self::OrderlyClose => "orderly_close",
            Self::AbruptClose => "abrupt_close",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|scenario| scenario.label() == value)
    }
}

/// Direction relative to the controlled hostile Destination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FingerprintDirection {
    ToDestination,
    FromDestination,
}

impl FingerprintDirection {
    const fn label(self) -> &'static str {
        match self {
            Self::ToDestination => "to_destination",
            Self::FromDestination => "from_destination",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "to_destination" => Some(Self::ToDestination),
            "from_destination" => Some(Self::FromDestination),
            _ => None,
        }
    }
}

/// Bounded terminal classification. Text from a peer or error is never kept.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FingerprintTerminal {
    Established,
    OrderlyClose,
    Reset,
    RetransmitLimit,
    Deadline,
    TransportFailure,
}

impl FingerprintTerminal {
    const fn label(self) -> &'static str {
        match self {
            Self::Established => "established",
            Self::OrderlyClose => "orderly_close",
            Self::Reset => "reset",
            Self::RetransmitLimit => "retransmit_limit",
            Self::Deadline => "deadline",
            Self::TransportFailure => "transport_failure",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "-" => None,
            "established" => Some(Self::Established),
            "orderly_close" => Some(Self::OrderlyClose),
            "reset" => Some(Self::Reset),
            "retransmit_limit" => Some(Self::RetransmitLimit),
            "deadline" => Some(Self::Deadline),
            "transport_failure" => Some(Self::TransportFailure),
            _ => return None,
        }
        .into()
    }
}

/// One normalized packet-level observation; sequence numbers are offsets from
/// each direction's first observed sequence origin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FingerprintEvent {
    pub index: u16,
    pub time_bucket_10ms: u16,
    pub direction: FingerprintDirection,
    pub flags: u16,
    pub payload_len: u16,
    pub sequence_delta: u32,
    pub acknowledgement_delta: u32,
    pub retransmission_ordinal: u8,
    pub max_payload: Option<u16>,
    pub advertised_window: Option<u16>,
    pub choked: Option<bool>,
    pub terminal: Option<FingerprintTerminal>,
}

/// A bounded trace for one scenario and one implementation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamingFingerprintTrace {
    scenario: FingerprintScenario,
    events: Vec<FingerprintEvent>,
}

impl StreamingFingerprintTrace {
    pub fn new(scenario: FingerprintScenario) -> Self {
        Self {
            scenario,
            events: Vec::new(),
        }
    }

    pub const fn scenario(&self) -> FingerprintScenario {
        self.scenario
    }

    pub fn events(&self) -> &[FingerprintEvent] {
        &self.events
    }

    /// Adds one event, enforcing count, ordering, and scenario deadline limits.
    pub fn push(&mut self, event: FingerprintEvent) -> Result<(), FingerprintTraceError> {
        if self.events.len() >= MAX_FINGERPRINT_EVENTS {
            return Err(FingerprintTraceError::EventLimit);
        }
        if event.index as usize != self.events.len() {
            return Err(FingerprintTraceError::IndexOrder);
        }
        if event.time_bucket_10ms > MAX_TIME_BUCKET {
            return Err(FingerprintTraceError::DeadlineExceeded);
        }
        if self
            .events
            .last()
            .is_some_and(|previous| event.time_bucket_10ms < previous.time_bucket_10ms)
        {
            return Err(FingerprintTraceError::TimeOrder);
        }
        self.events.push(event);
        Ok(())
    }

    /// Encodes a deterministic TSV trace with fixed columns and no free text.
    pub fn to_tsv(&self) -> String {
        let mut output = String::from(
            "scenario\tindex\ttime_10ms\tdirection\tflags\tpayload_len\tseq_delta\tack_delta\tretransmit_ordinal\tmax_payload\tadvertised_window\tchoked\tterminal\n",
        );
        for event in &self.events {
            use fmt::Write as _;
            let _ = writeln!(
                output,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                self.scenario.label(),
                event.index,
                event.time_bucket_10ms,
                event.direction.label(),
                event.flags,
                event.payload_len,
                event.sequence_delta,
                event.acknowledgement_delta,
                event.retransmission_ordinal,
                optional_u16(event.max_payload),
                optional_u16(event.advertised_window),
                optional_bool(event.choked),
                event.terminal.map_or("-", FingerprintTerminal::label),
            );
        }
        output
    }

    /// Parses only the canonical trace form emitted by [`Self::to_tsv`].
    pub fn from_tsv(input: &str) -> Result<Self, FingerprintTraceError> {
        let mut lines = input.lines();
        if lines.next() != Some(TRACE_HEADER) {
            return Err(FingerprintTraceError::BadHeader);
        }
        let mut trace: Option<Self> = None;
        for line in lines {
            if line.len() > 256 {
                return Err(FingerprintTraceError::BadRow);
            }
            let mut fields = line.split('\t');
            let scenario = FingerprintScenario::parse(next(&mut fields)?)
                .ok_or(FingerprintTraceError::BadRow)?;
            let event = FingerprintEvent {
                index: parse(next(&mut fields)?)?,
                time_bucket_10ms: parse(next(&mut fields)?)?,
                direction: FingerprintDirection::parse(next(&mut fields)?)
                    .ok_or(FingerprintTraceError::BadRow)?,
                flags: parse(next(&mut fields)?)?,
                payload_len: parse(next(&mut fields)?)?,
                sequence_delta: parse(next(&mut fields)?)?,
                acknowledgement_delta: parse(next(&mut fields)?)?,
                retransmission_ordinal: parse(next(&mut fields)?)?,
                max_payload: parse_optional_u16(next(&mut fields)?)?,
                advertised_window: parse_optional_u16(next(&mut fields)?)?,
                choked: parse_optional_bool(next(&mut fields)?)?,
                terminal: match next(&mut fields)? {
                    "-" => None,
                    value => Some(
                        FingerprintTerminal::parse(value).ok_or(FingerprintTraceError::BadRow)?,
                    ),
                },
            };
            if fields.next().is_some() {
                return Err(FingerprintTraceError::BadRow);
            }
            let current = trace.get_or_insert_with(|| Self::new(scenario));
            if current.scenario != scenario {
                return Err(FingerprintTraceError::MixedScenarios);
            }
            current.push(event)?;
        }
        trace.ok_or(FingerprintTraceError::EmptyTrace)
    }
}

const TRACE_HEADER: &str = "scenario\tindex\ttime_10ms\tdirection\tflags\tpayload_len\tseq_delta\tack_delta\tretransmit_ordinal\tmax_payload\tadvertised_window\tchoked\tterminal";

/// Removes random sequence origins while preserving their wrapping deltas.
pub const fn normalize_sequence(origin: u32, value: u32) -> u32 {
    value.wrapping_sub(origin)
}

/// Result of classifying one stable observable dimension across the families.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FingerprintClassification {
    CommonAcrossAll,
    I2prMatchesJava,
    I2prMatchesI2pd,
    ReferencesDifferI2prMatchesNeither,
    NotReliablyObservable,
    HarnessLimitation,
}

pub fn classify_dimension(
    i2pr: Option<u64>,
    java: Option<u64>,
    i2pd: Option<u64>,
) -> FingerprintClassification {
    let (Some(i2pr), Some(java), Some(i2pd)) = (i2pr, java, i2pd) else {
        return FingerprintClassification::NotReliablyObservable;
    };
    match (i2pr == java, i2pr == i2pd, java == i2pd) {
        (true, true, true) => FingerprintClassification::CommonAcrossAll,
        (true, false, false) => FingerprintClassification::I2prMatchesJava,
        (false, true, false) => FingerprintClassification::I2prMatchesI2pd,
        (false, false, false) => FingerprintClassification::ReferencesDifferI2prMatchesNeither,
        (false, false, true) => FingerprintClassification::HarnessLimitation,
        _ => FingerprintClassification::HarnessLimitation,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FingerprintTraceError {
    BadHeader,
    BadRow,
    EmptyTrace,
    MixedScenarios,
    EventLimit,
    IndexOrder,
    TimeOrder,
    DeadlineExceeded,
}

impl fmt::Display for FingerprintTraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadHeader => "fingerprint trace header is not canonical",
            Self::BadRow => "fingerprint trace row is malformed",
            Self::EmptyTrace => "fingerprint trace contains no rows",
            Self::MixedScenarios => "fingerprint trace mixes scenarios",
            Self::EventLimit => "fingerprint trace exceeds its event ceiling",
            Self::IndexOrder => "fingerprint event indices are not contiguous",
            Self::TimeOrder => "fingerprint event time buckets are not monotonic",
            Self::DeadlineExceeded => "fingerprint event exceeds the scenario deadline",
        })
    }
}

impl std::error::Error for FingerprintTraceError {}

fn optional_u16(value: Option<u16>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| value.to_string())
}

fn optional_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "-",
    }
}

fn parse_optional_u16(value: &str) -> Result<Option<u16>, FingerprintTraceError> {
    if value == "-" {
        Ok(None)
    } else {
        parse(value).map(Some)
    }
}

fn parse_optional_bool(value: &str) -> Result<Option<bool>, FingerprintTraceError> {
    match value {
        "-" => Ok(None),
        "true" => Ok(Some(true)),
        "false" => Ok(Some(false)),
        _ => Err(FingerprintTraceError::BadRow),
    }
}

fn next<'a>(fields: &mut impl Iterator<Item = &'a str>) -> Result<&'a str, FingerprintTraceError> {
    fields.next().ok_or(FingerprintTraceError::BadRow)
}

fn parse<T: std::str::FromStr>(value: &str) -> Result<T, FingerprintTraceError> {
    value.parse().map_err(|_| FingerprintTraceError::BadRow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(index: u16, time_bucket_10ms: u16) -> FingerprintEvent {
        FingerprintEvent {
            index,
            time_bucket_10ms,
            direction: FingerprintDirection::ToDestination,
            flags: 0x12,
            payload_len: 64,
            sequence_delta: 1,
            acknowledgement_delta: 0,
            retransmission_ordinal: 0,
            max_payload: Some(1730),
            advertised_window: Some(16),
            choked: Some(false),
            terminal: None,
        }
    }

    #[test]
    fn trace_round_trip_is_canonical_and_payload_free() {
        let mut trace = StreamingFingerprintTrace::new(FingerprintScenario::SmallPayload);
        trace.push(event(0, 2)).unwrap();
        let encoded = trace.to_tsv();
        assert!(!encoded.contains("payload_bytes"));
        assert_eq!(
            StreamingFingerprintTrace::from_tsv(&encoded).unwrap(),
            trace
        );
    }

    #[test]
    fn sequence_normalization_preserves_wrapping_delta() {
        assert_eq!(normalize_sequence(u32::MAX - 2, 1), 4);
        assert_eq!(normalize_sequence(91, 91), 0);
    }

    #[test]
    fn trace_rejects_index_time_deadline_and_mixed_scenario_errors() {
        let mut trace = StreamingFingerprintTrace::new(FingerprintScenario::SmallPayload);
        assert_eq!(
            trace.push(event(1, 0)),
            Err(FingerprintTraceError::IndexOrder)
        );
        trace.push(event(0, 4)).unwrap();
        assert_eq!(
            trace.push(event(1, 3)),
            Err(FingerprintTraceError::TimeOrder)
        );
        let late = FingerprintEvent {
            time_bucket_10ms: MAX_TIME_BUCKET + 1,
            ..event(1, 5)
        };
        assert_eq!(
            trace.push(late),
            Err(FingerprintTraceError::DeadlineExceeded)
        );
        let encoded = trace.to_tsv();
        let alternate = encoded
            .lines()
            .nth(1)
            .unwrap()
            .replace("small_payload\t0", "abrupt_close\t0");
        let mixed = format!("{encoded}{alternate}\n");
        assert_eq!(
            StreamingFingerprintTrace::from_tsv(&mixed),
            Err(FingerprintTraceError::MixedScenarios)
        );
    }

    #[test]
    fn seeded_family_dimensions_classify_without_collapsing_cases() {
        use FingerprintClassification as Class;
        assert_eq!(
            classify_dimension(Some(7), Some(7), Some(7)),
            Class::CommonAcrossAll
        );
        assert_eq!(
            classify_dimension(Some(7), Some(7), Some(9)),
            Class::I2prMatchesJava
        );
        assert_eq!(
            classify_dimension(Some(7), Some(9), Some(7)),
            Class::I2prMatchesI2pd
        );
        assert_eq!(
            classify_dimension(Some(7), Some(9), Some(11)),
            Class::ReferencesDifferI2prMatchesNeither
        );
        assert_eq!(
            classify_dimension(Some(7), Some(9), Some(9)),
            Class::HarnessLimitation
        );
        assert_eq!(
            classify_dimension(None, Some(9), Some(11)),
            Class::NotReliablyObservable
        );
    }
}
