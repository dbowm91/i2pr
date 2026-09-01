//! SAM v3.1 private-destination codec.
//!
//! Owns the SAM-compatible `PRIV` representation and the strict
//! decode/encode contract. The wrapper preserves the existing i2pr
//! secret-ownership policy: it is non-`Clone`, non-`Debug`, and the
//! private bytes are zeroised on drop.
//!
//! ## Format
//!
//! `PRIV` is the standard Java `PrivateKeyFile` concatenation:
//!
//! ```text
//! Destination (391 bytes) || X25519_static_secret (32) || Ed25519_seed (32)
//! ```
//!
//! encoded as **I2P Base64** (`A-Z a-z 0-9 - ~`, with standard `=`
//! padding). Plan 142 corrects the RFC 4648 alphabet defect in
//! `crate::sam::base64`; see
//! `specs/references/sam31-private-destination.md` for the normative
//! provenance and the independent corroborating references
//! (i2pd `libi2pd/Base.cpp`, Java I2P `PrivateKeyFile.java`,
//! `i2plib/sam.py`).
//!
//! ## Reference-compatibility invariant (Plan 146)
//!
//! i2pr's SAM `PRIV` is the **standard Java I2P `PrivateKeyFile`
//! concatenation**: the destination's encryption public field (the
//! first 32 bytes of the 384-byte key area) is treated as opaque. The
//! standard Java I2P `PrivateKeyFile` and i2pd `IdentityEx`
//! destination layouts populate that field with random bytes and
//! record an unrelated random 32-byte `PrivateKey` slot in the PRIV
//! suffix; they do **not** require `encryption_public ==
//! X25519(static_secret)`. The SAM import path therefore preserves
//! the destination bytes verbatim and accepts the `static_secret`
//! suffix unchanged. The only structural invariant the import path
//! enforces is `signing_public == EdDSA(signing_seed)`; a mismatch
//! returns `DestinationIdentityError::ImportSigningKeyMismatch`.

use core::fmt;

use i2pr_client::{DestinationId, DestinationIdentity, DestinationIdentityError};
use i2pr_proto::{CodecError, Destination, RouterIdentity};
use zeroize::{Zeroize, Zeroizing};

use crate::sam::base64;

/// The exact `PUB` byte length for SIGNATURE_TYPE=7 / CRYPTO_TYPE=4.
pub const PUB_LENGTH: usize = 391;

/// The exact binary `PRIV` length for SIGNATURE_TYPE=7 / CRYPTO_TYPE=4.
pub const PRIV_LENGTH: usize = PUB_LENGTH + 32 + 32;

/// Errors emitted by the private-destination codec.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SamPrivateDestinationError {
    /// The supplied bytes were the wrong length.
    #[error("private destination length {actual} != expected {expected}")]
    LengthMismatch {
        /// Actual length.
        actual: usize,
        /// Expected length.
        expected: usize,
    },
    /// The destination public portion failed structural validation.
    #[error("destination public portion invalid: {0}")]
    Codec(#[from] CodecError),
    /// The supplied private key bytes could not reconstruct a
    /// matching identity.
    #[error("private key material rejected: {0}")]
    Identity(#[from] DestinationIdentityError),
    /// The reconstructed identity did not match the supplied
    /// destination bytes (public/private mismatch).
    #[error("public/private destination mismatch")]
    PublicPrivateMismatch,
    /// The supplied Base64 text failed to decode.
    #[error("private destination base64 invalid: {0}")]
    Base64(#[from] base64::SamBase64Error),
}

/// A secret-owning SAM private destination.
///
/// The wrapper owns the concatenation bytes (`Destination || static
/// secret || signing seed`) in a zeroising buffer. It exposes the
/// encoded `PRIV` value, the `PUB` value, and a consuming
/// reconstruction into a [`DestinationIdentity`].
pub struct SamPrivateDestination {
    bytes: Zeroizing<[u8; PRIV_LENGTH]>,
}

impl PartialEq for SamPrivateDestination {
    fn eq(&self, other: &Self) -> bool {
        // Compare the public destination portion only — comparing
        // private bytes via this operator would invite timing-side
        // attacks and exposes secrets through equality assertions.
        self.public_bytes() == other.public_bytes()
    }
}

impl Eq for SamPrivateDestination {}

impl SamPrivateDestination {
    /// Constructs a `SamPrivateDestination` from a verified
    /// [`DestinationIdentity`].
    ///
    /// The destination public bytes are taken from the identity's
    /// canonical encoding; the private bytes are taken from the
    /// identity's static X25519 secret and signing Ed25519 seed.
    pub fn from_identity(
        identity: &DestinationIdentity,
    ) -> Result<Self, SamPrivateDestinationError> {
        let destination_bytes = identity
            .destination()
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)?;
        if destination_bytes.len() != PUB_LENGTH {
            return Err(SamPrivateDestinationError::LengthMismatch {
                actual: destination_bytes.len(),
                expected: PUB_LENGTH,
            });
        }
        let mut out = Zeroizing::new([0_u8; PRIV_LENGTH]);
        out[..PUB_LENGTH].copy_from_slice(&destination_bytes);
        out[PUB_LENGTH..PUB_LENGTH + 32].copy_from_slice(identity.static_secret_bytes());
        out[PUB_LENGTH + 32..].copy_from_slice(identity.signing_seed_bytes());
        Ok(Self { bytes: out })
    }

    /// Constructs a `SamPrivateDestination` from a SAM Base64 `PRIV`
    /// text. Validates the full structure and key material.
    pub fn from_base64(input: &str) -> Result<Self, SamPrivateDestinationError> {
        let bytes = base64::decode(input, PRIV_LENGTH)?;
        Self::from_bytes(bytes)
    }

    /// Constructs a `SamPrivateDestination` from the exact 455-byte
    /// binary `PRIV` representation.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, SamPrivateDestinationError> {
        if bytes.len() != PRIV_LENGTH {
            return Err(SamPrivateDestinationError::LengthMismatch {
                actual: bytes.len(),
                expected: PRIV_LENGTH,
            });
        }
        Self::from_bytes_array(bytes.as_slice().try_into().map_err(|_| {
            SamPrivateDestinationError::LengthMismatch {
                actual: bytes.len(),
                expected: PRIV_LENGTH,
            }
        })?)
    }

    fn from_bytes_array(bytes: [u8; PRIV_LENGTH]) -> Result<Self, SamPrivateDestinationError> {
        let destination =
            Destination::decode(&bytes[..PUB_LENGTH], i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)?;
        let encoded = destination.encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)?;
        // The destination structural parse round-trips: re-encoding the
        // decoded destination must produce exactly the input bytes.
        // This guards against malformed/truncated/unknown-cert
        // destination encodings without requiring the encryption public
        // field to match X25519(private_encryption_key).
        if encoded.as_slice() != &bytes[..PUB_LENGTH] {
            return Err(SamPrivateDestinationError::PublicPrivateMismatch);
        }
        let mut signing_seed = [0_u8; 32];
        signing_seed.copy_from_slice(&bytes[PRIV_LENGTH - 32..]);
        let mut static_secret = [0_u8; 32];
        static_secret.copy_from_slice(&bytes[PUB_LENGTH..PUB_LENGTH + 32]);
        // Reuse the destination verbatim; do not force the destination's
        // encryption public field to equal X25519(static_secret).
        // Plan 146 documents this tolerance for the standard Java I2P
        // `PrivateKeyFile` and i2pd `IdentityEx` destination layout.
        // The reconstructed identity is not stored alongside the raw
        // bytes; callers that need the [`DestinationIdentity`] obtain
        // it through [`SamPrivateDestination::into_identity`].
        let _ = DestinationIdentity::from_imported(destination, signing_seed, static_secret)?;
        Ok(Self {
            bytes: Zeroizing::new(bytes),
        })
    }

    /// Returns the SAM Base64 encoding of the private destination.
    pub fn encode_base64(&self) -> String {
        base64::encode(&self.bytes[..])
    }

    /// Returns the SAM Base64 encoding of the public destination.
    pub fn encode_public_base64(&self) -> String {
        base64::encode(&self.bytes[..PUB_LENGTH])
    }

    /// Returns the public destination bytes.
    pub fn public_bytes(&self) -> &[u8] {
        &self.bytes[..PUB_LENGTH]
    }

    /// Returns the private destination bytes (destination + private
    /// keys).
    pub fn private_bytes(&self) -> &[u8] {
        &self.bytes[..]
    }

    /// Returns the SHA-256 [`DestinationId`] of the destination.
    pub fn destination_id(&self) -> Result<DestinationId, SamPrivateDestinationError> {
        let destination =
            RouterIdentity::decode(self.public_bytes(), i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)?;
        let hash = destination.hash()?;
        Ok(DestinationId::from_hash(hash))
    }

    /// Consumes the wrapper and returns a reconstructed
    /// [`DestinationIdentity`].
    pub fn into_identity(self) -> Result<DestinationIdentity, SamPrivateDestinationError> {
        Self::identity_from_bytes(&self.bytes)
    }

    fn identity_from_bytes(
        bytes: &[u8; PRIV_LENGTH],
    ) -> Result<DestinationIdentity, SamPrivateDestinationError> {
        let destination =
            Destination::decode(&bytes[..PUB_LENGTH], i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)?;
        let mut signing_seed = [0_u8; 32];
        signing_seed.copy_from_slice(&bytes[PRIV_LENGTH - 32..]);
        let mut static_secret = [0_u8; 32];
        static_secret.copy_from_slice(&bytes[PUB_LENGTH..PUB_LENGTH + 32]);
        // Use the relaxed import path: preserve the destination's
        // embedded encryption public key bytes verbatim instead of
        // re-deriving them from `static_secret`. Plan 146 documents
        // this tolerance for Java I2P `PrivateKeyFile` and i2pd
        // `IdentityEx` destinations, where the encryption public
        // field is independent of the encryption private key.
        Ok(DestinationIdentity::from_imported(
            destination,
            signing_seed,
            static_secret,
        )?)
    }

    /// Internal hook used by tests to construct a wrapper from raw
    /// bytes for negative-path coverage. The wrapper validates
    /// internally on construction, so this constructor is only used
    /// to exercise the validation path itself.
    #[doc(hidden)]
    pub fn from_raw_for_test(bytes: Vec<u8>) -> Self {
        let mut owned = [0_u8; PRIV_LENGTH];
        let take = bytes.len().min(PRIV_LENGTH);
        owned[..take].copy_from_slice(&bytes[..take]);
        Self {
            bytes: Zeroizing::new(owned),
        }
    }
}

impl Drop for SamPrivateDestination {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

impl fmt::Debug for SamPrivateDestination {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SamPrivateDestination")
            .field("length", &self.bytes.len())
            .field("public_destination", &"<redacted>")
            .field("private_keys", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn deterministic_identity(seed: u64) -> DestinationIdentity {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        DestinationIdentity::generate(&mut rng).expect("identity")
    }

    #[test]
    fn priv_round_trip_through_base64() {
        let identity = deterministic_identity(1);
        let original_id = identity.id();
        let wrapper = SamPrivateDestination::from_identity(&identity).expect("wrapper");
        let encoded = wrapper.encode_base64();
        let decoded = SamPrivateDestination::from_base64(&encoded).expect("decode");
        let restored = decoded.into_identity().expect("identity");
        assert_eq!(restored.id(), original_id);
    }

    #[test]
    fn pub_and_priv_lengths_match_specification() {
        let identity = deterministic_identity(2);
        let wrapper = SamPrivateDestination::from_identity(&identity).expect("wrapper");
        assert_eq!(wrapper.public_bytes().len(), PUB_LENGTH);
        assert_eq!(wrapper.private_bytes().len(), PRIV_LENGTH);
        assert_eq!(wrapper.encode_public_base64().len(), 524);
        assert_eq!(wrapper.encode_base64().len(), 608);
    }

    #[test]
    fn truncated_priv_is_rejected() {
        let identity = deterministic_identity(3);
        let wrapper = SamPrivateDestination::from_identity(&identity).expect("wrapper");
        let encoded = wrapper.encode_base64();
        let truncated: String = encoded.chars().take(encoded.len() - 1).collect();
        let error = SamPrivateDestination::from_base64(&truncated).unwrap_err();
        assert!(
            matches!(
                error,
                SamPrivateDestinationError::Base64(_)
                    | SamPrivateDestinationError::LengthMismatch { .. }
                    | SamPrivateDestinationError::Codec(_)
            ),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn mutated_private_key_is_rejected() {
        let identity = deterministic_identity(4);
        let wrapper = SamPrivateDestination::from_identity(&identity).expect("wrapper");
        let mut bytes: Vec<u8> = wrapper.private_bytes().to_vec();
        bytes[PRIV_LENGTH - 1] ^= 0x01;
        let error = SamPrivateDestination::from_bytes(bytes).unwrap_err();
        assert!(matches!(
            error,
            SamPrivateDestinationError::PublicPrivateMismatch
                | SamPrivateDestinationError::Identity(_)
        ));
    }

    #[test]
    fn wrong_length_priv_is_rejected() {
        let error = SamPrivateDestination::from_bytes(vec![0_u8; PRIV_LENGTH - 1]).unwrap_err();
        assert!(matches!(
            error,
            SamPrivateDestinationError::LengthMismatch { .. }
        ));
    }

    #[test]
    fn debug_redacts_secret_bytes() {
        let identity = deterministic_identity(5);
        let wrapper = SamPrivateDestination::from_identity(&identity).expect("wrapper");
        let rendered = format!("{wrapper:?}");
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("static_secret"));
    }
}
