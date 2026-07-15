//! Protocol-facing names, bounded codecs, and error categories for `i2pr`.
//!
//! The codec module provides primitive mechanics plus bounded common-structure
//! and initial I2NP models. It does not implement runtime integration,
//! filesystem I/O, routing, transport behavior, or router policy. I2NP values
//! requiring later cryptography or state machines remain explicitly deferred.

#![forbid(unsafe_code)]

mod codec;
mod common;
#[doc(hidden)]
mod common_impl;
mod i2np;
#[doc(hidden)]
mod i2np_impl;

pub use codec::{CodecError, DecodeCursor, EncodeBuffer, decode_exact, encode_to_vec};
pub use common::*;
pub use i2np::*;

/// Stable structural outcomes shared by protocol parsers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolErrorKind {
    /// The input is not valid for the selected protocol shape.
    Malformed,
    /// The input ended before a complete value was available.
    Truncated,
    /// A declared size, count, or nesting level exceeds a local limit.
    LimitExceeded,
    /// A field value is structurally invalid but not a size/format error.
    InvalidValue,
    /// A strict decoder found bytes outside the value boundary.
    TrailingBytes,
    /// A value is structurally valid but rejected by a local policy.
    PolicyRejected,
    /// The input uses a feature that this implementation does not support.
    Unsupported,
}

/// Protocol-facing namespaces owned by this crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Namespace {
    /// Common identity and encoding vocabulary.
    Common,
    /// I2NP message namespace and bounded structural codecs.
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
        assert_ne!(
            ProtocolErrorKind::PolicyRejected,
            ProtocolErrorKind::InvalidValue
        );
    }
}
