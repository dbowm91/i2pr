//! Protocol-specific cryptographic wrappers for the Milestone 1 router
//! identity.
//!
//! The generated identity uses I2P signature type 7 (Ed25519) and encryption
//! type 4 (X25519). Secret operations are deliberately kept here rather than
//! in `i2pr-proto`, whose key types remain public structural representations.
//! Randomness is supplied by the caller and must implement
//! [`rand_core::TryCryptoRng`].

#![forbid(unsafe_code)]

use std::convert::TryInto;

use ed25519_dalek::Signer;
use i2pr_proto::{
    Certificate, CryptoKeyType, Date, Hash, KeyAndCert, KeyCertificate, Mapping, PublicKey,
    RouterAddress, RouterIdentity, RouterInfo, SignatureValue, SigningKeyType, SigningPublicKey,
};
use rand_core::TryCryptoRng;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
use zeroize::Zeroize;

/// Operating-system-backed randomness for explicit production injection.
pub use rand_core::OsRng;

/// The generated I2P signature algorithm: EdDSA over Ed25519 (type 7).
pub const ROUTER_SIGNING_KEY_TYPE: SigningKeyType = SigningKeyType::EdDsaSha512Ed25519;
/// The generated I2P router encryption algorithm: X25519 (type 4).
pub const ROUTER_CRYPTO_KEY_TYPE: CryptoKeyType = CryptoKeyType::X25519;
/// The raw private-key length for both selected algorithms.
pub const PRIVATE_KEY_LENGTH: usize = 32;
/// The fixed Ed25519 signature length.
pub const SIGNATURE_LENGTH: usize = 64;
/// The common-structure key-area padding for the selected key certificate.
pub const IDENTITY_PADDING_LENGTH: usize = 384 - PRIVATE_KEY_LENGTH - PRIVATE_KEY_LENGTH;

/// Errors returned by the protocol-specific cryptographic wrappers.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum CryptoError {
    /// The injected cryptographic random source failed.
    #[error("cryptographic randomness unavailable")]
    RandomnessUnavailable,
    /// A key or signature algorithm was outside this concrete wrapper's scope.
    #[error("unsupported cryptographic algorithm {algorithm} for {context}")]
    UnsupportedAlgorithm {
        /// Numeric protocol algorithm identifier.
        algorithm: u16,
        /// Static operation category.
        context: &'static str,
    },
    /// A public or private key had an invalid representation.
    #[error("invalid {context} key material")]
    InvalidKey {
        /// Static key category.
        context: &'static str,
    },
    /// A signature did not verify against the supplied message and key.
    #[error("signature verification failed")]
    InvalidSignature,
    /// A structural protocol type could not be constructed.
    #[error("protocol structure rejected: {0}")]
    Protocol(#[from] i2pr_proto::CodecError),
}

/// A private Ed25519 signing seed.
///
/// This type intentionally has no `Debug`, `Display`, `Clone`, or serde
/// implementations. The only byte accessor is explicitly named for private
/// storage code and borrows the secret for the shortest practical lifetime.
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct SigningPrivateKey([u8; PRIVATE_KEY_LENGTH]);

impl SigningPrivateKey {
    /// Loads a raw 32-byte Ed25519 seed.
    pub const fn from_bytes(bytes: [u8; PRIVATE_KEY_LENGTH]) -> Self {
        Self(bytes)
    }

    /// Borrows the raw seed for explicit private-format storage.
    pub const fn secret_bytes(&self) -> &[u8; PRIVATE_KEY_LENGTH] {
        &self.0
    }

    /// Derives the public Ed25519 key in the protocol representation.
    pub fn public_key(&self) -> Result<SigningPublicKey, CryptoError> {
        let key = ed25519_dalek::SigningKey::from_bytes(&self.0);
        SigningPublicKey::new(
            ROUTER_SIGNING_KEY_TYPE,
            key.verifying_key().to_bytes().to_vec(),
        )
        .map_err(CryptoError::Protocol)
    }

    /// Signs exactly the supplied message bytes using Ed25519.
    pub fn sign(&self, message: &[u8]) -> Result<SignatureValue, CryptoError> {
        let key = ed25519_dalek::SigningKey::from_bytes(&self.0);
        let signature = key.sign(message);
        SignatureValue::new(ROUTER_SIGNING_KEY_TYPE, signature.to_bytes().to_vec())
            .map_err(CryptoError::Protocol)
    }
}

/// A private X25519 static secret.
///
/// Like [`SigningPrivateKey`], this type does not reveal itself through
/// formatting, cloning, or serialization.
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct EncryptionPrivateKey([u8; PRIVATE_KEY_LENGTH]);

impl EncryptionPrivateKey {
    /// Loads a raw 32-byte X25519 secret.
    pub const fn from_bytes(bytes: [u8; PRIVATE_KEY_LENGTH]) -> Self {
        Self(bytes)
    }

    /// Borrows the raw secret for explicit private-format storage.
    pub const fn secret_bytes(&self) -> &[u8; PRIVATE_KEY_LENGTH] {
        &self.0
    }

    /// Derives the little-endian X25519 public key in the protocol representation.
    pub fn public_key(&self) -> Result<PublicKey, CryptoError> {
        let secret = StaticSecret::from(self.0);
        let public = X25519PublicKey::from(&secret);
        PublicKey::new(ROUTER_CRYPTO_KEY_TYPE, public.to_bytes().to_vec())
            .map_err(CryptoError::Protocol)
    }
}

/// A complete generated router identity and its private key material.
///
/// The public [`RouterIdentity`] can be borrowed freely. Private material is
/// kept in separate non-cloneable, zeroizing wrappers and is never included in
/// a derived debug representation because this bundle intentionally has none.
pub struct RouterIdentityBundle {
    identity: RouterIdentity,
    signing_key: SigningPrivateKey,
    encryption_key: EncryptionPrivateKey,
}

impl RouterIdentityBundle {
    /// Generates a new identity from an injected cryptographic random source.
    pub fn generate<R: TryCryptoRng + ?Sized>(rng: &mut R) -> Result<Self, CryptoError> {
        let mut signing = [0_u8; PRIVATE_KEY_LENGTH];
        let mut encryption = [0_u8; PRIVATE_KEY_LENGTH];
        if rng.try_fill_bytes(&mut signing).is_err() || rng.try_fill_bytes(&mut encryption).is_err()
        {
            signing.zeroize();
            encryption.zeroize();
            return Err(CryptoError::RandomnessUnavailable);
        }
        Self::from_private_bytes(signing, encryption)
    }

    /// Reconstructs an identity from explicit private key bytes.
    pub fn from_private_bytes(
        signing: [u8; PRIVATE_KEY_LENGTH],
        encryption: [u8; PRIVATE_KEY_LENGTH],
    ) -> Result<Self, CryptoError> {
        let signing_key = SigningPrivateKey::from_bytes(signing);
        let encryption_key = EncryptionPrivateKey::from_bytes(encryption);
        let identity = build_router_identity(&signing_key, &encryption_key)?;
        Ok(Self {
            identity,
            signing_key,
            encryption_key,
        })
    }

    /// Returns the public RouterIdentity.
    pub const fn identity(&self) -> &RouterIdentity {
        &self.identity
    }

    /// Returns the private signing wrapper for signing or private storage.
    pub const fn signing_key(&self) -> &SigningPrivateKey {
        &self.signing_key
    }

    /// Returns the private encryption wrapper for private storage.
    pub const fn encryption_key(&self) -> &EncryptionPrivateKey {
        &self.encryption_key
    }

    /// Signs a canonical RouterInfo signed region and returns its full record.
    ///
    /// A zero signature is used only as a temporary structural placeholder so
    /// `i2pr-proto` can construct the exact unsigned region. The final record
    /// is rebuilt from the same semantic fields after signing that retained
    /// byte slice.
    pub fn sign_router_info(
        &self,
        published: Date,
        addresses: Vec<RouterAddress>,
        peers: Vec<Hash>,
        options: Mapping,
    ) -> Result<RouterInfo, CryptoError> {
        let placeholder =
            SignatureValue::new(ROUTER_SIGNING_KEY_TYPE, vec![0_u8; SIGNATURE_LENGTH])?;
        let unsigned = RouterInfo::new(
            self.identity.clone(),
            published,
            addresses.clone(),
            peers.clone(),
            options.clone(),
            placeholder,
        )?;
        let signature = self.signing_key.sign(unsigned.signed_bytes())?;
        RouterInfo::new(
            self.identity.clone(),
            published,
            addresses,
            peers,
            options,
            signature,
        )
        .map_err(CryptoError::Protocol)
    }
}

fn build_router_identity(
    signing_key: &SigningPrivateKey,
    encryption_key: &EncryptionPrivateKey,
) -> Result<RouterIdentity, CryptoError> {
    let public_key = encryption_key.public_key()?;
    let signing_public_key = signing_key.public_key()?;
    let certificate = Certificate::Key(KeyCertificate::for_types(
        ROUTER_SIGNING_KEY_TYPE,
        ROUTER_CRYPTO_KEY_TYPE,
    )?);
    let keys = KeyAndCert::new(
        public_key,
        signing_public_key,
        vec![0_u8; IDENTITY_PADDING_LENGTH],
        certificate,
    )?;
    RouterIdentity::new(keys).map_err(CryptoError::Protocol)
}

/// Verifies an Ed25519 signature against an explicitly supplied message region.
pub fn verify_signature(
    public_key: &SigningPublicKey,
    message: &[u8],
    signature: &SignatureValue,
) -> Result<(), CryptoError> {
    if public_key.key_type() != ROUTER_SIGNING_KEY_TYPE {
        return Err(CryptoError::UnsupportedAlgorithm {
            algorithm: public_key.key_type().code(),
            context: "signature verification",
        });
    }
    if signature.key_type() != ROUTER_SIGNING_KEY_TYPE {
        return Err(CryptoError::UnsupportedAlgorithm {
            algorithm: signature.key_type().code(),
            context: "signature verification",
        });
    }
    let public_bytes: [u8; PRIVATE_KEY_LENGTH] =
        public_key
            .as_bytes()
            .try_into()
            .map_err(|_| CryptoError::InvalidKey {
                context: "signing public",
            })?;
    let public = ed25519_dalek::VerifyingKey::from_bytes(&public_bytes).map_err(|_| {
        CryptoError::InvalidKey {
            context: "signing public",
        }
    })?;
    let signature_bytes: [u8; SIGNATURE_LENGTH] = signature
        .as_bytes()
        .try_into()
        .map_err(|_| CryptoError::InvalidSignature)?;
    let signature = ed25519_dalek::Signature::from_bytes(&signature_bytes);
    public
        .verify_strict(message, &signature)
        .map_err(|_| CryptoError::InvalidSignature)
}

/// Verifies the exact signed byte region retained by a RouterInfo.
pub fn verify_router_info(info: &RouterInfo) -> Result<(), CryptoError> {
    verify_signature(
        info.router_identity().signing_key(),
        info.signed_bytes(),
        info.signature(),
    )
}

/// Computes a SHA-256 digest as the protocol's fixed-size hash type.
pub fn sha256(input: &[u8]) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(input);
    Hash::from_bytes(hasher.finalize().into())
}

/// Computes the hash of a canonical RouterIdentity encoding.
pub fn router_identity_hash(identity: &RouterIdentity) -> Result<Hash, CryptoError> {
    identity.hash().map_err(CryptoError::Protocol)
}

/// Compares two public or integrity values without early exit on equal-length bytes.
pub fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && bool::from(left.ct_eq(right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    #[test]
    fn deterministic_generation_is_reproducible_only_with_injected_rng() {
        let left = bundle(7);
        let right = bundle(7);
        assert_eq!(left.identity(), right.identity());
        assert_eq!(
            left.signing_key().secret_bytes(),
            right.signing_key().secret_bytes()
        );
        assert_eq!(
            left.encryption_key().secret_bytes(),
            right.encryption_key().secret_bytes()
        );
        assert_eq!(
            left.identity().public_key().key_type(),
            ROUTER_CRYPTO_KEY_TYPE
        );
        assert_eq!(
            left.identity().signing_key().key_type(),
            ROUTER_SIGNING_KEY_TYPE
        );
    }

    #[test]
    fn signature_vectors_reject_message_signature_and_key_mutations() {
        let signer = bundle(11);
        let other = bundle(12);
        let message = b"the exact signed region";
        let signature = signer.signing_key().sign(message).expect("sign");
        verify_signature(signer.identity().signing_key(), message, &signature).expect("verify");

        let mut changed_message = message.to_vec();
        changed_message[0] ^= 1;
        assert_eq!(
            verify_signature(
                signer.identity().signing_key(),
                &changed_message,
                &signature
            ),
            Err(CryptoError::InvalidSignature)
        );

        let mut changed_signature = signature.as_bytes().to_vec();
        changed_signature[0] ^= 1;
        let changed_signature =
            SignatureValue::new(ROUTER_SIGNING_KEY_TYPE, changed_signature).expect("signature");
        assert_eq!(
            verify_signature(signer.identity().signing_key(), message, &changed_signature),
            Err(CryptoError::InvalidSignature)
        );
        assert_eq!(
            verify_signature(other.identity().signing_key(), message, &signature),
            Err(CryptoError::InvalidSignature)
        );
    }

    #[test]
    fn hash_and_constant_time_helpers_are_stable() {
        assert_eq!(sha256(b"abc"), Hash::digest(b"abc"));
        assert!(constant_time_eq(b"same", b"same"));
        assert!(!constant_time_eq(b"same", b"different"));
        assert!(!constant_time_eq(b"same", b"same\0"));
    }

    #[test]
    fn router_info_signing_uses_retained_signed_bytes() {
        let signer = bundle(13);
        let info = signer
            .sign_router_info(
                Date::from_millis(1),
                Vec::new(),
                Vec::new(),
                Mapping::empty(),
            )
            .expect("router info");
        verify_router_info(&info).expect("router info verifies");
        let encoded = info
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("encode");
        let decoded =
            RouterInfo::decode(&encoded, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE).expect("decode");
        verify_router_info(&decoded).expect("reloaded router info verifies");
        assert_eq!(decoded.signed_bytes(), info.signed_bytes());
    }
}
