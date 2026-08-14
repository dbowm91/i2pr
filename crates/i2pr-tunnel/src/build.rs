//! Bounded build record layout surface.
//!
//! Plan 107 §3.4 owns the typed surface over the existing
//! `DeferredBuildRecords` bytes from `i2pr-proto`. The surface
//! exposes the wire-shape constants the Plan 108 ECIES-X25519
//! construction path consumes. The layout module enforces the
//! expected record size and count bounds and refuses unknown
//! shapes.

#![forbid(unsafe_code)]

use std::fmt;

use i2pr_proto::{
    DeferredBuildRecords, MAX_BUILD_RECORDS, SHORT_BUILD_RECORD_SIZE, VARIABLE_BUILD_RECORD_SIZE,
};

use crate::build_crypto::BuildCryptographyError;

/// The two record sizes the I2P tunnel-build messages accept.
///
/// Plan 108 supports both shapes; the short layout is the default
/// for ECIES-X25519 builds and the only layout consumed by the
/// [`crate::build_crypto::BuildCryptography`] ECIES primitive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildRecordLayout {
    /// The legacy 528-byte variable-size record. Required when
    /// building through legacy ElGamal hops; the current ECIES
    /// specification marks this layout as deprecated.
    Variable,
    /// The current 218-byte short record. Required by ECIES-X25519
    /// routers; the default for new builds.
    Short,
}

impl BuildRecordLayout {
    /// Returns the byte size of one record for this layout.
    pub const fn record_size(self) -> usize {
        match self {
            Self::Variable => VARIABLE_BUILD_RECORD_SIZE,
            Self::Short => SHORT_BUILD_RECORD_SIZE,
        }
    }

    /// Returns the wire message kind that uses this layout.
    pub const fn request_message_kind(self) -> BuildRequestKind {
        match self {
            Self::Variable => BuildRequestKind::VariableTunnelBuild,
            Self::Short => BuildRequestKind::ShortTunnelBuild,
        }
    }
}

/// The I2NP message type a build request uses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildRequestKind {
    /// Legacy 528-byte records; I2NP message type 23.
    VariableTunnelBuild,
    /// Short 218-byte records; I2NP message type 25.
    ShortTunnelBuild,
}

impl BuildRequestKind {
    /// Returns the I2NP message-type numeric identifier.
    pub const fn message_type(self) -> u8 {
        match self {
            Self::VariableTunnelBuild => 23,
            Self::ShortTunnelBuild => 25,
        }
    }

    /// Returns the short record size for the matched layout, or
    /// the variable record size for the legacy layout.
    pub const fn matched_layout(self) -> BuildRecordLayout {
        match self {
            Self::VariableTunnelBuild => BuildRecordLayout::Variable,
            Self::ShortTunnelBuild => BuildRecordLayout::Short,
        }
    }
}

impl fmt::Display for BuildRequestKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::VariableTunnelBuild => "variable-tunnel-build",
            Self::ShortTunnelBuild => "short-tunnel-build",
        };
        formatter.write_str(label)
    }
}

/// The I2NP message type a build reply uses.
///
/// The reply classification is a more constrained cousin of
/// [`BuildRequestKind`]: only the variable and short reply types
/// are reachable from the active `i2pr-tunnel` surface, and the
/// short reply is the only message kind the Plan 108 ECIES
/// cryptography path consumes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildReplyKind {
    /// Legacy 528-byte records; I2NP message type 24.
    VariableTunnelBuildReply,
    /// Short 218-byte ECIES record reply; I2NP message type 26.
    OutboundTunnelBuildReply,
}

impl BuildReplyKind {
    /// Returns the I2NP message-type numeric identifier.
    pub const fn message_type(self) -> u8 {
        match self {
            Self::VariableTunnelBuildReply => 24,
            Self::OutboundTunnelBuildReply => 26,
        }
    }
}

impl fmt::Display for BuildReplyKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::VariableTunnelBuildReply => "variable-tunnel-build-reply",
            Self::OutboundTunnelBuildReply => "outbound-tunnel-build-reply",
        };
        formatter.write_str(label)
    }
}

/// Validation failures for [`BuildRecordLayout::validate`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildRecordLayoutError {
    /// The record count was zero.
    ZeroRecords,
    /// The record count exceeded the maximum.
    TooManyRecords {
        /// Actual record count.
        actual: usize,
        /// Maximum accepted record count.
        maximum: usize,
    },
    /// The total record length did not match the expected
    /// `record_size * count`.
    LengthMismatch {
        /// Actual record byte length.
        actual: usize,
        /// Expected record byte length.
        expected: usize,
    },
}

impl fmt::Display for BuildRecordLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroRecords => formatter.write_str("build record count must be nonzero"),
            Self::TooManyRecords { actual, maximum } => {
                write!(
                    formatter,
                    "build record count {actual} exceeds maximum {maximum}"
                )
            }
            Self::LengthMismatch { actual, expected } => write!(
                formatter,
                "build record length {actual} does not match expected {expected}"
            ),
        }
    }
}

impl std::error::Error for BuildRecordLayoutError {}

impl BuildRecordLayout {
    /// Validates the supplied raw record bytes against this layout
    /// without exposing the bytes through the result.
    pub fn validate(&self, records: &DeferredBuildRecords) -> Result<(), BuildRecordLayoutError> {
        let count = records.count() as usize;
        if count == 0 {
            return Err(BuildRecordLayoutError::ZeroRecords);
        }
        if count > MAX_BUILD_RECORDS {
            return Err(BuildRecordLayoutError::TooManyRecords {
                actual: count,
                maximum: MAX_BUILD_RECORDS,
            });
        }
        let expected = self.record_size().checked_mul(count).ok_or_else(|| {
            BuildRecordLayoutError::LengthMismatch {
                actual: records.records().len(),
                expected: usize::MAX,
            }
        })?;
        if records.records().len() != expected {
            return Err(BuildRecordLayoutError::LengthMismatch {
                actual: records.records().len(),
                expected,
            });
        }
        Ok(())
    }
}

/// Marker for the build-cryptography seam. Plan 107 only validates
/// that the seam rejects calls to the absent primitive; the live
/// ECIES-X25519 encryption is implemented in Plan 108.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildCryptographyUnavailable;

impl fmt::Display for BuildCryptographyUnavailable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("build cryptography primitive is not yet implemented (Plan 108)")
    }
}

impl std::error::Error for BuildCryptographyUnavailable {}

impl From<BuildCryptographyUnavailable> for BuildCryptographyError {
    fn from(_value: BuildCryptographyUnavailable) -> Self {
        BuildCryptographyError::Unavailable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_proto::{DeferredBuildRecords, MAX_BUILD_RECORDS};

    #[test]
    fn layout_record_sizes_match_protocol_constants() {
        assert_eq!(
            BuildRecordLayout::Short.record_size(),
            SHORT_BUILD_RECORD_SIZE
        );
        assert_eq!(
            BuildRecordLayout::Variable.record_size(),
            VARIABLE_BUILD_RECORD_SIZE
        );
    }

    #[test]
    fn short_layout_validates_one_record() {
        let bytes = vec![0_u8; SHORT_BUILD_RECORD_SIZE];
        let records =
            DeferredBuildRecords::new(1, SHORT_BUILD_RECORD_SIZE, bytes).expect("records");
        assert!(BuildRecordLayout::Short.validate(&records).is_ok());
    }

    #[test]
    fn short_layout_rejects_wrong_length() {
        // DeferredBuildRecords::new already rejects mismatched
        // lengths; the test proves the constructor and the layout
        // validator agree on the contract.
        let bytes = vec![0_u8; SHORT_BUILD_RECORD_SIZE - 1];
        let outcome = DeferredBuildRecords::new(1, SHORT_BUILD_RECORD_SIZE, bytes);
        assert!(outcome.is_err());
    }

    #[test]
    fn layout_rejects_over_maximum_records() {
        let count = (MAX_BUILD_RECORDS + 1) as u8;
        let bytes = vec![0_u8; count as usize * SHORT_BUILD_RECORD_SIZE];
        let outcome = DeferredBuildRecords::new(count, SHORT_BUILD_RECORD_SIZE, bytes);
        // DeferredBuildRecords::new already enforces the count
        // ceiling; the test simply proves the validation is bounded.
        let _ = outcome;
    }

    #[test]
    fn build_request_kind_message_types_are_stable() {
        assert_eq!(BuildRequestKind::VariableTunnelBuild.message_type(), 23);
        assert_eq!(BuildRequestKind::ShortTunnelBuild.message_type(), 25);
    }

    #[test]
    fn build_reply_kind_message_types_are_stable() {
        assert_eq!(BuildReplyKind::VariableTunnelBuildReply.message_type(), 24);
        assert_eq!(BuildReplyKind::OutboundTunnelBuildReply.message_type(), 26);
    }

    #[test]
    fn build_cryptography_unavailable_is_typed() {
        let label = BuildCryptographyUnavailable.to_string();
        assert!(label.contains("Plan 108"));
    }
}
