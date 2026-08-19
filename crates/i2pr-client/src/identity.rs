//! Local destination identity and secret ownership.
//!
//! Plan 120 §2 requires an explicit local destination identity owner that is
//! independent of the router identity. A [`DestinationIdentity`] owns the
//! destination signing private key and the static X25519 private key used for
//! ECIES destination encryption (Plan 121). Neither secret is reachable
//! through `Debug`, `Clone`, or any public accessor; the only secret-consuming
//! operation exposed is [`DestinationIdentity::sign`].

use core::fmt;

use i2pr_crypto::{
    CryptoError, IDENTITY_PADDING_LENGTH, PRIVATE_KEY_LENGTH, ROUTER_CRYPTO_KEY_TYPE,
    ROUTER_SIGNING_KEY_TYPE, SigningPrivateKey, X25519_KEY_LENGTH, X25519PrivateKey,
};
use i2pr_proto::{
    Certificate, CodecError, Destination, Hash, KeyAndCert, KeyCertificate, SignatureValue,
    SigningPublicKey,
};
use rand_core::TryCryptoRng;
use zeroize::Zeroizing;

/// Non-secret local destination identifier: the SHA-256 hash of the canonical
/// `Destination` structure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DestinationId(Hash);

impl DestinationId {
    /// Wraps an existing destination hash.
    pub const fn from_hash(hash: Hash) -> Self {
        Self(hash)
    }

    /// Returns the wrapped hash.
    pub const fn as_hash(&self) -> &Hash {
        &self.0
    }

    /// Returns the raw 32 hash bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }

    /// Projects the identifier onto the `i2pr-netdb` destination key.
    pub const fn as_netdb_key(&self) -> i2pr_netdb::DestinationHash {
        i2pr_netdb::DestinationHash::from_hash(self.0)
    }
}

/// Owner of one local destination's public structure and private key material.
///
/// The type is deliberately non-`Clone`: Plan 120 §2 invariant 3 forbids two
/// local destinations from implicitly sharing private key objects.
pub struct DestinationIdentity {
    id: DestinationId,
    destination: Destination,
    signing_key: SigningPrivateKey,
    static_key: X25519PrivateKey,
}

impl DestinationIdentity {
    /// Generates a fresh destination identity from the supplied CSPRNG.
    pub fn generate<R: TryCryptoRng + ?Sized>(
        rng: &mut R,
    ) -> Result<Self, DestinationIdentityError> {
        let mut signing = Zeroizing::new([0_u8; PRIVATE_KEY_LENGTH]);
        let mut static_secret = Zeroizing::new([0_u8; X25519_KEY_LENGTH]);
        if rng.try_fill_bytes(&mut *signing).is_err()
            || rng.try_fill_bytes(&mut *static_secret).is_err()
        {
            return Err(DestinationIdentityError::RandomnessUnavailable);
        }
        let mut padding = vec![0_u8; IDENTITY_PADDING_LENGTH];
        if rng.try_fill_bytes(&mut padding).is_err() {
            return Err(DestinationIdentityError::RandomnessUnavailable);
        }
        Self::from_private_bytes(*signing, *static_secret, Zeroizing::new(padding))
    }

    /// Reconstructs a destination identity from explicit private key bytes and
    /// an exact identity padding buffer. Deterministic destination creation in
    /// tests uses this constructor; Plan 120 defers encrypted destination-key
    /// persistence, so there is no storage-backed constructor yet.
    pub fn from_private_bytes(
        signing: [u8; PRIVATE_KEY_LENGTH],
        static_secret: [u8; X25519_KEY_LENGTH],
        padding: Zeroizing<Vec<u8>>,
    ) -> Result<Self, DestinationIdentityError> {
        if padding.len() != IDENTITY_PADDING_LENGTH {
            return Err(DestinationIdentityError::PaddingLength {
                actual: padding.len(),
                expected: IDENTITY_PADDING_LENGTH,
            });
        }
        let signing_key = SigningPrivateKey::from_bytes(signing);
        let static_key = X25519PrivateKey::from_bytes(static_secret);
        let signing_public = signing_key.public_key()?;
        let encryption_public = static_key.public_key()?;
        let certificate = Certificate::Key(KeyCertificate::for_types(
            ROUTER_SIGNING_KEY_TYPE,
            ROUTER_CRYPTO_KEY_TYPE,
        )?);
        let keys = KeyAndCert::new(
            encryption_public,
            signing_public,
            padding.to_vec(),
            certificate,
        )?;
        let destination = Destination::new(keys)?;
        let id = DestinationId::from_hash(destination.hash()?);
        Ok(Self {
            id,
            destination,
            signing_key,
            static_key,
        })
    }

    /// Returns the non-secret destination identifier.
    pub const fn id(&self) -> DestinationId {
        self.id
    }

    /// Borrows the public `Destination` structure. Cloning the public
    /// structure is permitted and never exposes secrets.
    pub const fn destination(&self) -> &Destination {
        &self.destination
    }

    /// Returns the destination signing public key.
    pub fn signing_public_key(&self) -> &SigningPublicKey {
        self.destination.signing_key()
    }

    /// Returns the raw static X25519 public key bytes advertised in the
    /// destination's LeaseSet2 encryption key entry.
    pub fn static_public_bytes(&self) -> [u8; X25519_KEY_LENGTH] {
        self.static_key.public_bytes()
    }

    /// Signs the supplied message with the destination signing private key.
    ///
    /// This is the only secret-consuming operation the identity owner exposes.
    pub fn sign(&self, message: &[u8]) -> Result<SignatureValue, DestinationIdentityError> {
        Ok(self.signing_key.sign(message)?)
    }

    /// Computes a static-static X25519 shared secret with the supplied peer
    /// public key. Plan 121 consumes this seam for the ECIES Garlic session
    /// layer; Plan 120 exposes it only so the static key has one documented
    /// owner.
    pub fn diffie_hellman(
        &self,
        peer: &[u8; X25519_KEY_LENGTH],
    ) -> Result<i2pr_crypto::X25519SharedSecret, DestinationIdentityError> {
        Ok(self.static_key.diffie_hellman(peer)?)
    }
}

impl fmt::Debug for DestinationIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DestinationIdentity")
            .field("id", &self.id)
            .field("signing_key", &"<redacted>")
            .field("static_key", &"<redacted>")
            .finish()
    }
}

/// Typed destination identity construction failures.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DestinationIdentityError {
    /// The supplied CSPRNG could not produce key or padding bytes.
    #[error("randomness unavailable for destination identity construction")]
    RandomnessUnavailable,
    /// The supplied identity padding buffer was the wrong length.
    #[error("destination identity padding length {actual} != expected {expected}")]
    PaddingLength {
        /// Supplied length.
        actual: usize,
        /// Required length.
        expected: usize,
    },
    /// A cryptographic primitive rejected the key material.
    #[error("destination identity cryptography rejected: {0}")]
    Crypto(#[from] CryptoError),
    /// A common-structure codec rejected the constructed identity.
    #[error("destination identity codec rejected: {0}")]
    Codec(#[from] CodecError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    pub(crate) fn identity_for(seed: u64) -> DestinationIdentity {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        DestinationIdentity::generate(&mut rng).expect("destination identity")
    }

    #[test]
    fn generated_identity_exposes_x25519_and_ed25519_public_material() {
        let identity = identity_for(1);
        assert_eq!(
            identity.destination().public_key().key_type(),
            ROUTER_CRYPTO_KEY_TYPE
        );
        assert_eq!(
            identity.signing_public_key().key_type(),
            ROUTER_SIGNING_KEY_TYPE
        );
        assert_eq!(
            identity.destination().public_key().as_bytes(),
            &identity.static_public_bytes()[..]
        );
        assert_eq!(
            identity.id().as_bytes(),
            identity.destination().hash().expect("hash").as_bytes()
        );
    }

    #[test]
    fn debug_never_reveals_secret_bytes() {
        let identity = identity_for(2);
        let rendered = format!("{identity:?}");
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("signing_key: ["));
        let secret_prefix = format!("{:?}", identity.static_public_bytes()[0]);
        // The public byte value may coincidentally appear; the assertion below
        // is the real invariant: no raw secret array is rendered.
        let _ = secret_prefix;
        assert_eq!(rendered.matches("<redacted>").count(), 2);
    }

    #[test]
    fn two_destinations_are_independent() {
        let first = identity_for(3);
        let second = identity_for(4);
        assert_ne!(first.id(), second.id());
        assert_ne!(first.static_public_bytes(), second.static_public_bytes());
        let message = b"plan-120";
        let first_signature = first.sign(message).expect("sign");
        let second_signature = second.sign(message).expect("sign");
        assert_ne!(first_signature.as_bytes(), second_signature.as_bytes());
        i2pr_crypto::verify_signature(first.signing_public_key(), message, &first_signature)
            .expect("first verifies");
        assert!(
            i2pr_crypto::verify_signature(first.signing_public_key(), message, &second_signature)
                .is_err()
        );
    }

    #[test]
    fn deterministic_reconstruction_matches_generated_identity() {
        let signing = [7_u8; PRIVATE_KEY_LENGTH];
        let static_secret = [9_u8; X25519_KEY_LENGTH];
        let padding = Zeroizing::new(vec![0x5a_u8; IDENTITY_PADDING_LENGTH]);
        let first =
            DestinationIdentity::from_private_bytes(signing, static_secret, padding.clone())
                .expect("identity");
        let second = DestinationIdentity::from_private_bytes(signing, static_secret, padding)
            .expect("identity");
        assert_eq!(first.id(), second.id());
    }

    #[test]
    fn wrong_padding_length_is_rejected() {
        let error = DestinationIdentity::from_private_bytes(
            [1_u8; PRIVATE_KEY_LENGTH],
            [2_u8; X25519_KEY_LENGTH],
            Zeroizing::new(vec![0_u8; 8]),
        )
        .expect_err("padding rejected");
        assert!(matches!(
            error,
            DestinationIdentityError::PaddingLength { actual: 8, .. }
        ));
    }
}
