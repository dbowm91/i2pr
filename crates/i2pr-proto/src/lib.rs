//! Protocol-facing names and error categories for `i2pr`.
//!
//! This crate deliberately contains no wire codecs, runtime integration, or
//! router policy.  It establishes only stable vocabulary that later protocol
//! plans can refine without pulling daemon concerns into lower layers.

#![forbid(unsafe_code)]

/// Broad structural outcomes shared by future protocol parsers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolErrorKind {
    /// The input is not valid for the selected protocol shape.
    Malformed,
    /// A declared size, count, or nesting level exceeds a local limit.
    LimitExceeded,
    /// The input uses a feature that this implementation does not support.
    Unsupported,
}

/// Namespaces reserved for protocol-facing crates.
///
/// These names do not indicate that any corresponding protocol is currently
/// implemented.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Namespace {
    /// Common identity and encoding vocabulary.
    Common,
    /// I2NP message namespace, reserved for a later detailed plan.
    I2np,
}

impl Namespace {
    /// Returns the stable documentation name of this namespace.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Common => "common",
            Self::I2np => "i2np",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Namespace, ProtocolErrorKind};

    #[test]
    fn namespace_names_are_explicit() {
        assert_eq!(Namespace::Common.as_str(), "common");
        assert_eq!(Namespace::I2np.as_str(), "i2np");
    }

    #[test]
    fn error_categories_are_distinct() {
        assert_ne!(ProtocolErrorKind::Malformed, ProtocolErrorKind::Unsupported);
        assert_ne!(
            ProtocolErrorKind::Malformed,
            ProtocolErrorKind::LimitExceeded
        );
    }
}
