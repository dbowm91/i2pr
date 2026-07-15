//! Protocol-facing names, bounded codecs, and error categories for `i2pr`.
//!
//! The codec module provides only primitive mechanics: a borrowed read cursor,
//! checked network-order integers, caller-bounded length-prefixed fields, and
//! a bounded encoder. It does not implement RouterIdentity, Destination,
//! RouterInfo, I2NP, runtime integration, filesystem I/O, or router policy.

#![forbid(unsafe_code)]

mod codec;

pub use codec::{CodecError, DecodeCursor, EncodeBuffer, decode_exact, encode_to_vec};

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
