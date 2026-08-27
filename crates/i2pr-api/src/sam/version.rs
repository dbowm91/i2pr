//! SAM v3.1 version negotiation.
//!
//! Plan 136 advertises exactly one supported version: **3.1**. The
//! parser rejects malformed, signed, overflowing, extra-component,
//! empty, or whitespace-contaminated version strings. Negotiation
//! returns the highest mutually supported version from the explicit
//! server support set; if no overlap exists, negotiation fails rather
//! than returning the nearest version.

use core::fmt;

const SUPPORTED_MAJOR: u16 = 3;
const SUPPORTED_MINOR: u16 = 1;

/// The minimum version the server advertises for the Milestone 7
/// baseline.
pub const MIN_SUPPORTED_VERSION: SamVersion = SamVersion::const_new(3, 1);
/// The maximum version the server advertises for the Milestone 7
/// baseline.
pub const MAX_SUPPORTED_VERSION: SamVersion = SamVersion::const_new(3, 1);

/// A typed SAM protocol version.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SamVersion {
    major: u16,
    minor: u16,
}

impl SamVersion {
    /// Constructs a new version. Intended for compile-time constants.
    pub const fn const_new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// Returns the major component.
    pub const fn major(self) -> u16 {
        self.major
    }

    /// Returns the minor component.
    pub const fn minor(self) -> u16 {
        self.minor
    }

    /// Renders the version as `MAJOR.MINOR`.
    pub fn render(self) -> alloc::string::String {
        alloc::format!("{}.{}", self.major, self.minor)
    }
}

impl fmt::Display for SamVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

/// Failure modes for version parsing and negotiation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamVersionParseError {
    /// The version string was empty.
    Empty,
    /// The version contained NUL or control characters.
    ContainsControlByte,
    /// The version had more than one `.` separator.
    ExtraComponents,
    /// The version was missing the `.` separator.
    MissingSeparator,
    /// One of the numeric components was empty or non-decimal.
    NonDecimalComponent,
    /// The major component was larger than `u16::MAX`.
    MajorOverflow,
    /// The minor component was larger than `u16::MAX`.
    MinorOverflow,
    /// The version contained leading/trailing whitespace or interior
    /// whitespace.
    SurroundedByWhitespace,
}

impl fmt::Display for SamVersionParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason: &'static str = match self {
            Self::Empty => "empty",
            Self::ContainsControlByte => "contains control byte",
            Self::ExtraComponents => "extra components",
            Self::MissingSeparator => "missing '.' separator",
            Self::NonDecimalComponent => "non-decimal component",
            Self::MajorOverflow => "major component overflow",
            Self::MinorOverflow => "minor component overflow",
            Self::SurroundedByWhitespace => "surrounded by whitespace",
        };
        formatter.write_str(reason)
    }
}

impl std::error::Error for SamVersionParseError {}

/// Parses a `MAJOR.MINOR` version string.
pub fn parse_version(input: &str) -> Result<SamVersion, SamVersionParseError> {
    if input.is_empty() {
        return Err(SamVersionParseError::Empty);
    }
    if input != input.trim() {
        return Err(SamVersionParseError::SurroundedByWhitespace);
    }
    if input.bytes().any(|byte| byte < 0x20 || byte == 0x7f) {
        return Err(SamVersionParseError::ContainsControlByte);
    }
    let (major_text, minor_text) = input
        .split_once('.')
        .ok_or(SamVersionParseError::MissingSeparator)?;
    if minor_text.contains('.') {
        return Err(SamVersionParseError::ExtraComponents);
    }
    let major = parse_decimal(major_text, SamVersionParseError::MajorOverflow)?;
    let minor = parse_decimal(minor_text, SamVersionParseError::MinorOverflow)?;
    Ok(SamVersion::const_new(major, minor))
}

fn parse_decimal(input: &str, overflow: SamVersionParseError) -> Result<u16, SamVersionParseError> {
    if input.is_empty() || !input.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(SamVersionParseError::NonDecimalComponent);
    }
    let value = input.parse::<u32>().map_err(|_| overflow)?;
    if value > u16::MAX as u32 {
        return Err(overflow);
    }
    Ok(value as u16)
}

/// The negotiated version after intersecting the client's
/// `MIN`/`MAX` range with the server's advertised support.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NegotiatedVersion {
    /// The intersection succeeded; the negotiated version is the
    /// highest version that satisfies both sides.
    Agreed(SamVersion),
    /// The client and server versions had no overlap.
    NoOverlap {
        /// Client's `MIN` (lowest acceptable).
        client_min: SamVersion,
        /// Client's `MAX` (highest acceptable).
        client_max: SamVersion,
        /// Server's lowest advertised version.
        server_min: SamVersion,
        /// Server's highest advertised version.
        server_max: SamVersion,
    },
}

impl NegotiatedVersion {
    /// Returns the agreed version, if any.
    pub const fn agreed(self) -> Option<SamVersion> {
        match self {
            Self::Agreed(version) => Some(version),
            Self::NoOverlap { .. } => None,
        }
    }
}

/// Negotiates a SAM version between the client's `MIN`/`MAX` range
/// and the server's advertised support. The server support set is
/// intentionally minimal: the exact range
/// `[MIN_SUPPORTED_VERSION, MAX_SUPPORTED_VERSION]`. A future plan may
/// widen the support set.
pub fn negotiate(client_min: SamVersion, client_max: SamVersion) -> NegotiatedVersion {
    let server_min = MIN_SUPPORTED_VERSION;
    let server_max = MAX_SUPPORTED_VERSION;
    if client_min.major() != client_max.major() {
        return NegotiatedVersion::NoOverlap {
            client_min,
            client_max,
            server_min,
            server_max,
        };
    }
    if client_max < server_min || client_min > server_max {
        return NegotiatedVersion::NoOverlap {
            client_min,
            client_max,
            server_min,
            server_max,
        };
    }
    let chosen_minor = if client_max.minor() <= server_max.minor() {
        client_max.minor()
    } else {
        server_max.minor()
    };
    let chosen_minor = if chosen_minor >= client_min.minor() {
        chosen_minor
    } else {
        client_min.minor()
    };
    if chosen_minor < client_min.minor() || chosen_minor > client_max.minor() {
        return NegotiatedVersion::NoOverlap {
            client_min,
            client_max,
            server_min,
            server_max,
        };
    }
    NegotiatedVersion::Agreed(SamVersion::const_new(server_max.major(), chosen_minor))
}

/// Whether the supplied version is within the server's advertised
/// range. Plan 136 advertises only the exact value `3.1`.
pub fn is_advertised(version: SamVersion) -> bool {
    version.major() == SUPPORTED_MAJOR
        && version.minor() == SUPPORTED_MINOR
        && version >= MIN_SUPPORTED_VERSION
        && version <= MAX_SUPPORTED_VERSION
}

extern crate alloc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_canonical_3_1() {
        assert_eq!(parse_version("3.1").unwrap(), SamVersion::const_new(3, 1));
        assert_eq!(parse_version("3.0").unwrap(), SamVersion::const_new(3, 0));
        assert_eq!(parse_version("3.10").unwrap(), SamVersion::const_new(3, 10));
    }

    #[test]
    fn parse_rejects_malformed_inputs() {
        for bad in [
            "", "3", "3.1.0", "3.", ".1", "3.1 ", " 3.1", "3. 1", "-1.0", "+3.1", "a.b", "3.x",
            "65536.0", "0.65536", "3.\0", "3.1\0",
        ] {
            assert!(
                parse_version(bad).is_err(),
                "expected error for {bad:?}, got {:?}",
                parse_version(bad)
            );
        }
    }

    #[test]
    fn negotiate_agrees_on_overlap() {
        let outcome = negotiate(SamVersion::const_new(3, 0), SamVersion::const_new(3, 1));
        assert_eq!(outcome.agreed(), Some(SamVersion::const_new(3, 1)));
    }

    #[test]
    fn negotiate_rejects_disjoint_ranges() {
        let outcome = negotiate(SamVersion::const_new(3, 0), SamVersion::const_new(3, 1));
        assert!(matches!(outcome, NegotiatedVersion::Agreed(_)));
        let empty = negotiate(SamVersion::const_new(4, 0), SamVersion::const_new(4, 5));
        assert!(matches!(empty, NegotiatedVersion::NoOverlap { .. }));
    }

    #[test]
    fn negotiate_rejects_major_mismatch() {
        let outcome = negotiate(SamVersion::const_new(2, 9), SamVersion::const_new(2, 9));
        assert!(matches!(outcome, NegotiatedVersion::NoOverlap { .. }));
    }

    #[test]
    fn advertised_versions_are_only_3_1() {
        assert!(is_advertised(SamVersion::const_new(3, 1)));
        assert!(!is_advertised(SamVersion::const_new(3, 0)));
        assert!(!is_advertised(SamVersion::const_new(3, 2)));
        assert!(!is_advertised(SamVersion::const_new(3, 3)));
    }
}
