//! Build-cryptography seam.
//!
//! Plan 107 §3.5 owns the typed seam over the ECIES-X25519 build
//! encryption primitive. The seam deliberately fails closed in
//! Plan 107 — the live primitive lands in Plan 008+. The seam
//! exposes:
//!
//! - [`BuildCryptography`] — a trait that the live primitive will
//!   implement;
//! - [`BuildCryptographyError`] — the typed error categories;
//! - [`LayerKeys`] — a `Zeroize`-derived non-cloneable wrapper for
//!   the per-hop layer keys a build needs.
//!
//! The seam never calls into a network or runtime. It is the
//! boundary at which a future plan plugs the live ECIES-X25519
//! primitive.

#![forbid(unsafe_code)]

use std::fmt;

use zeroize::{Zeroize, Zeroizing};

/// The fixed per-hop layer-key length (16 bytes for AES-256-CBC
/// half-keys plus 16 bytes for the IV seed). Plan 107 reserves the
/// length to keep the `LayerKeys` type stable; the live primitive
/// decides how the keys are derived.
pub const LAYER_KEY_LEN: usize = 32;

/// A zeroizing non-cloneable owner for a per-hop layer key.
///
/// The owner deliberately has no `Debug`, `Clone`, or serde
/// implementations. The only byte accessor borrows the secret for
/// the shortest practical lifetime.
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct LayerKeys([u8; LAYER_KEY_LEN]);

impl LayerKeys {
    /// Loads the supplied bytes without exposing them through any
    /// other API.
    pub const fn from_bytes(bytes: [u8; LAYER_KEY_LEN]) -> Self {
        Self(bytes)
    }

    /// Borrows the raw key bytes for the lifetime of the borrow.
    pub const fn as_bytes(&self) -> &[u8; LAYER_KEY_LEN] {
        &self.0
    }
}

/// A typed error returned by [`BuildCryptography::seal`] and the
/// related seam calls.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildCryptographyError {
    /// The seam has no live primitive yet.
    Unavailable,
    /// The supplied record layout is not supported by this seam.
    UnsupportedLayout(&'static str),
    /// The supplied key material failed to validate.
    InvalidKeyMaterial,
}

impl fmt::Display for BuildCryptographyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("build cryptography is unavailable"),
            Self::UnsupportedLayout(label) => {
                write!(
                    formatter,
                    "build cryptography does not support layout {label}"
                )
            }
            Self::InvalidKeyMaterial => formatter.write_str("invalid build key material"),
        }
    }
}

impl std::error::Error for BuildCryptographyError {}

/// The trait every build-cryptography implementation must satisfy.
///
/// Plan 107 ships the [`NoBuildCryptography`] implementation that
/// always returns [`BuildCryptographyError::Unavailable`]; Plan
/// 008+ will replace it with the live ECIES-X25519 primitive.
pub trait BuildCryptography {
    /// Seals the supplied records with the supplied layer keys and
    /// returns the resulting encrypted bytes. The implementation
    /// must validate every input before performing any work.
    fn seal(
        &self,
        layout: super::build::BuildRecordLayout,
        records: &[Zeroizing<Vec<u8>>],
        keys: &LayerKeys,
    ) -> Result<Vec<u8>, BuildCryptographyError>;

    /// Opens the supplied sealed records with the supplied layer
    /// keys. The implementation must validate every input before
    /// performing any work.
    fn open(
        &self,
        layout: super::build::BuildRecordLayout,
        sealed: &[u8],
        keys: &LayerKeys,
    ) -> Result<Vec<u8>, BuildCryptographyError>;
}

/// The Plan 107 default build-cryptography implementation: always
/// returns [`BuildCryptographyError::Unavailable`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoBuildCryptography;

impl BuildCryptography for NoBuildCryptography {
    fn seal(
        &self,
        _layout: super::build::BuildRecordLayout,
        _records: &[Zeroizing<Vec<u8>>],
        _keys: &LayerKeys,
    ) -> Result<Vec<u8>, BuildCryptographyError> {
        Err(BuildCryptographyError::Unavailable)
    }

    fn open(
        &self,
        _layout: super::build::BuildRecordLayout,
        _sealed: &[u8],
        _keys: &LayerKeys,
    ) -> Result<Vec<u8>, BuildCryptographyError> {
        Err(BuildCryptographyError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BuildRecordLayout;

    #[test]
    fn no_build_cryptography_seal_returns_unavailable() {
        let cryptography = NoBuildCryptography;
        let layout = BuildRecordLayout::Short;
        let records = vec![Zeroizing::new(vec![
            0_u8;
            BuildRecordLayout::Short.record_size()
        ])];
        let keys = LayerKeys::from_bytes([0u8; LAYER_KEY_LEN]);
        let outcome = cryptography.seal(layout, &records, &keys);
        assert_eq!(outcome, Err(BuildCryptographyError::Unavailable));
    }

    #[test]
    fn no_build_cryptography_open_returns_unavailable() {
        let cryptography = NoBuildCryptography;
        let layout = BuildRecordLayout::Short;
        let keys = LayerKeys::from_bytes([0u8; LAYER_KEY_LEN]);
        let outcome = cryptography.open(layout, &[0_u8; 64], &keys);
        assert_eq!(outcome, Err(BuildCryptographyError::Unavailable));
    }

    #[test]
    fn layer_keys_zeroize_on_drop() {
        // The Drop trait is auto-derived; this test merely
        // confirms the wrapper compiles and is constructed.
        let _ = LayerKeys::from_bytes([0xab_u8; LAYER_KEY_LEN]);
    }
}
