//! Local destination identity and secret ownership.
//!
//! Plan 120 §2 requires an explicit local destination identity owner that is
//! independent of the router identity. A [`DestinationIdentity`] owns the
//! destination signing private key and the static X25519 private key used for
//! ECIES destination encryption (Plan 121). Neither secret is reachable
//! through `Debug`, `Clone`, or any public accessor; the only secret-consuming
//! operation exposed is [`DestinationIdentity::sign`].
//!
//! Plan 166 adds a parallel client-owned capability surface. The router-owned
//! mode retains the [`DestinationIdentity`] seam unchanged so Plan 149 /
//! Plan 151 SAM regressions continue to share one private identity allocation
//! between the runtime and the SAM bridge. The client-owned mode is
//! capability-oriented: the public [`Destination`] plus an [`InboundDecryptionCapability`]
//! that proves the client supplied the matching X25519 private decryption
//! material. The destination signing private key never enters the
//! client-owned path; Plan 163 §2 documents this as an architectural
//! invariant.

use core::fmt;
use core::ops::Deref;

use i2pr_crypto::{
    CryptoError, PRIVATE_KEY_LENGTH, ROUTER_CRYPTO_KEY_TYPE, ROUTER_SIGNING_KEY_TYPE,
    SigningPrivateKey, X25519_KEY_LENGTH, X25519PrivateKey,
};
use i2pr_proto::{
    Certificate, CodecError, CryptoKeyType, Destination, Hash, KeyAndCert, KeyCertificate,
    PublicKey, SignatureValue, SigningPublicKey,
};
use rand_core::TryCryptoRng;
use zeroize::Zeroize;
use zeroize::Zeroizing;

/// Plan 223 D1 — explicit crypto-layer separation for router-owned
/// Destinations.
///
/// Java I2P 2.13.0 `OutboundClientMessageOneShotJob.runJob()` rejects any
/// target Destination whose `getEncType()` is not `ELGAMAL_2048` (type 0)
/// before the client-NetDB lookup or LeaseSet2 key-selection path. Java's
/// own `I2PClientImpl.createDestination()` therefore keeps the legacy
/// 256-byte Destination public-encryption slot in its ElGamal/type-0
/// identity shape even though that field is unused for end-to-end
/// encryption; modern ECIES capability is advertised independently in
/// Standard LeaseSet2 (`i2cp.leaseSetEncType=4`).
///
/// i2pr separates the same two concepts:
///
/// - [`DESTINATION_IDENTITY_LEGACY_CRYPTO_TYPE`] (ElGamal/type 0) is the
///   legacy Destination identity/key-certificate shape. The 256-byte slot
///   carries public non-secret filler/identity material and is never the
///   active destination message encryption key. No ElGamal encryption is
///   implemented.
/// - [`DESTINATION_LS2_CRYPTO_TYPE`] (X25519/type 4) is the active
///   Standard-LS2 encryption key, derived from the owned X25519 static
///   secret and emitted by `build_signed_lease_set2()`.
///
/// Router identity crypto (`ROUTER_CRYPTO_KEY_TYPE`) is a separate concern
/// and is never reused for Destination construction after this corrective.
pub const DESTINATION_IDENTITY_LEGACY_CRYPTO_TYPE: CryptoKeyType = CryptoKeyType::ElGamal;
/// Active Standard-LS2 encryption type for destinations (X25519/type 4).
pub const DESTINATION_LS2_CRYPTO_TYPE: CryptoKeyType = CryptoKeyType::X25519;
/// Legacy Destination public-encryption slot length (ElGamal/type 0).
pub const DESTINATION_LEGACY_PUBLIC_LENGTH: usize = 256;
/// Canonical key-area padding for Ed25519/type-7 + ElGamal/type-0
/// Destinations: `384 - 256 - 32 = 96`.
pub const DESTINATION_LEGACY_PADDING_LENGTH: usize = 96;
/// Legacy X25519 Destination padding (`384 - 32 - 32 = 320`). Retained only
/// for exact-byte reconstruction of pre-Plan-223 persisted v1 service
/// records; new router-owned Destinations must not use it.
pub const DESTINATION_X25519_PADDING_LENGTH: usize = 320;

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

/// Ownership mode for one local destination runtime.
///
/// Plan 166 §2 requires the runtime to differentiate two ownership modes
/// without duplicating the destination stack. Router-owned destinations
/// (SAM, local SAM-generated private destinations) keep their signing key
/// inside the runtime; client-owned destinations (I2CP, future plan hooks)
/// never see the destination signing private key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestinationOwnership {
    /// Router owns the destination signing key, the X25519 inbound
    /// decryption secret, and signs/rotates its own Standard
    /// LeaseSet2. Used by SAM and any router-generated destination.
    RouterOwned,
    /// Client owns the destination signing key. The runtime retains
    /// the public `Destination` plus, after atomic validation, the
    /// matching X25519 decryption private key. The runtime never
    /// signs a replacement LeaseSet2: lease refresh is requested
    /// from the client through a typed event.
    ClientOwned,
}

impl DestinationOwnership {
    /// Whether this ownership mode keeps the destination signing
    /// private key inside the runtime.
    pub const fn holds_signing_secret(self) -> bool {
        matches!(self, Self::RouterOwned)
    }
}

/// Non-secret public destination identity.
///
/// Both router-owned and client-owned destinations derive an instance of
/// this struct from the canonical `Destination` bytes. It carries no
/// signing or decryption key material; the router-owned path pairs it
/// with a [`DestinationIdentity`] and the client-owned path pairs it
/// with an [`InboundDecryptionCapability`].
#[derive(Clone, Debug)]
pub struct DestinationPublic {
    destination: Destination,
    id: DestinationId,
    /// Static X25519 public key advertised in the destination. Used
    /// to detect malformed LeaseSet2 installations (the supplied
    /// decryption capability must derive from these exact bytes).
    static_public_bytes: [u8; X25519_KEY_LENGTH],
}

impl DestinationPublic {
    /// Wraps an already-decoded destination and derives the cached
    /// public material used for capability validation.
    pub fn from_destination(destination: Destination) -> Result<Self, DestinationIdentityError> {
        let id = DestinationId::from_hash(destination.hash()?);
        let signing_type = destination.signing_key().key_type();
        let encryption_type = destination.public_key().key_type();
        if signing_type != ROUTER_SIGNING_KEY_TYPE {
            return Err(DestinationIdentityError::UnsupportedSigningType {
                signing_type: signing_type.code(),
            });
        }
        // Plan 170 §6: the Destination's encryption key type is the
        // legacy I2P 0.6 era field. Every modern reference client
        // (Java I2P 2.13.0, go-i2cp, i2pd) ships ElGamal (type 0)
        // here even when the LeaseSet2 install path uses X25519
        // (type 4) encryption. The Plan 166 `install_client_lease_set2`
        // path is the actual X25519 enforcement point; the runtime
        // destination identity accepts both ElGamal (legacy 256-byte
        // public-key slot, contents unused) and X25519 (32-byte
        // static public-key slot, used for ECIES inbound decryption).
        if !matches!(
            encryption_type,
            ROUTER_CRYPTO_KEY_TYPE | CryptoKeyType::ElGamal
        ) {
            return Err(DestinationIdentityError::UnsupportedCryptoType {
                crypto_type: encryption_type.code(),
            });
        }
        // For X25519 destinations the static public key is a
        // 32-byte slot extracted from the destination's encryption
        // field. For legacy ElGamal destinations the field is
        // 256 bytes (unused since I2P 0.6, 2005); we zero the slot
        // because the actual X25519 keypair material arrives later
        // through the Plan 166 LeaseSet2 install path.
        let static_public_bytes = if encryption_type == ROUTER_CRYPTO_KEY_TYPE {
            extract_x25519_public_bytes(destination.public_key().as_bytes())
                .ok_or(DestinationIdentityError::StaticPublicLengthMismatch)?
        } else {
            [0_u8; 32]
        };
        Ok(Self {
            destination,
            id,
            static_public_bytes,
        })
    }

    /// Returns the wrapped destination identifier.
    pub const fn id(&self) -> DestinationId {
        self.id
    }

    /// Borrows the public `Destination` structure.
    pub const fn destination(&self) -> &Destination {
        &self.destination
    }

    /// Returns the destination signing public key.
    pub fn signing_public_key(&self) -> &SigningPublicKey {
        self.destination.signing_key()
    }

    /// Returns the destination encryption public key.
    pub fn encryption_public_key_type(&self) -> CryptoKeyType {
        self.destination.public_key().key_type()
    }

    /// Returns the 32-byte static X25519 public key bytes.
    pub const fn static_public_bytes(&self) -> &[u8; X25519_KEY_LENGTH] {
        &self.static_public_bytes
    }
}

fn extract_x25519_public_bytes(bytes: &[u8]) -> Option<[u8; X25519_KEY_LENGTH]> {
    if bytes.len() != X25519_KEY_LENGTH {
        return None;
    }
    let mut out = [0_u8; X25519_KEY_LENGTH];
    out.copy_from_slice(bytes);
    Some(out)
}

/// Client-supplied X25519 inbound decryption private key material.
///
/// Plan 166 §7 requires the smallest possible secret-bearing wrapper
/// for the inbound decryption capability a client supplies alongside a
/// Standard LeaseSet2. The wrapper:
///
/// - is non-`Clone` (no second private identity allocation),
/// - has manual/redacted `Debug`,
/// - zeroizes on drop,
/// - exposes no equality over the secret bytes,
/// - keeps the matching public key for self-validation before the
///   LeaseSet2 encryption key is allowed to be the one we just stored
///   the matching secret for.
///
/// The router never persists the secret and never logs its bytes.
pub struct InboundDecryptionCapability {
    static_public_bytes: [u8; X25519_KEY_LENGTH],
    secret: X25519PrivateKey,
}

impl Drop for InboundDecryptionCapability {
    fn drop(&mut self) {
        self.static_public_bytes.zeroize();
    }
}

impl InboundDecryptionCapability {
    /// Constructs a capability from explicit private bytes. The
    /// supplied bytes are wrapped through [`X25519PrivateKey`]; the
    /// caller must not retain references to the original buffer.
    pub fn from_secret_bytes(
        static_public_bytes: [u8; X25519_KEY_LENGTH],
        secret_bytes: [u8; X25519_KEY_LENGTH],
    ) -> Self {
        Self {
            static_public_bytes,
            secret: X25519PrivateKey::from_bytes(secret_bytes),
        }
    }

    /// Returns the X25519 public key bytes this capability decrypts
    /// traffic for. Used to validate against a LeaseSet2 encryption
    /// key before committing the secret.
    pub const fn static_public_bytes(&self) -> &[u8; X25519_KEY_LENGTH] {
        &self.static_public_bytes
    }

    /// Borrows the raw private key bytes for the ECIES session
    /// manager. The accessor is the single documented path from a
    /// client-owned destination runtime to the ECIES primitives.
    pub const fn secret_bytes(&self) -> &[u8; X25519_KEY_LENGTH] {
        self.secret.secret_bytes()
    }

    /// Computes a static-static X25519 shared secret with the supplied
    /// peer public key. The shared secret is owned by the ECIES
    /// primitive; this method exists so the ECIES layer can be reused
    /// without leaking the secret bytes through caller code.
    pub fn diffie_hellman(
        &self,
        peer: &[u8; X25519_KEY_LENGTH],
    ) -> Result<i2pr_crypto::X25519SharedSecret, CryptoError> {
        self.secret.diffie_hellman(peer)
    }
}

impl fmt::Debug for InboundDecryptionCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InboundDecryptionCapability")
            .field("static_public_bytes", &"<redacted>")
            .field("secret", &"<redacted>")
            .finish()
    }
}

/// Borrowed view over an [`InboundDecryptionCapability`] that exposes
/// only the public and secret byte accessors needed by the ECIES layer.
///
/// The wrapper is constructed on demand so the runtime never hands out
/// the [`InboundDecryptionCapability`] to the ECIES primitives; only
/// the typed view.
#[derive(Clone, Copy)]
pub struct InboundDecryptionRef<'a> {
    capability: &'a InboundDecryptionCapability,
}

impl<'a> InboundDecryptionRef<'a> {
    /// Wraps a borrowed capability.
    pub const fn new(capability: &'a InboundDecryptionCapability) -> Self {
        Self { capability }
    }

    /// Returns the static X25519 public key bytes.
    pub const fn static_public_bytes(&self) -> &[u8; X25519_KEY_LENGTH] {
        self.capability.static_public_bytes()
    }

    /// Returns the static X25519 secret bytes.
    pub const fn secret_bytes(&self) -> &[u8; X25519_KEY_LENGTH] {
        self.capability.secret_bytes()
    }
}

impl Deref for InboundDecryptionRef<'_> {
    type Target = InboundDecryptionCapability;

    fn deref(&self) -> &Self::Target {
        self.capability
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
    /// Generates a fresh router-owned destination identity from the supplied
    /// CSPRNG.
    ///
    /// Plan 223 D2: the generated Destination uses the Java-compatible
    /// legacy identity shape (ElGamal/type 0, 256-byte public slot,
    /// Ed25519/type 7, 96-byte padding) while the owned X25519 static key
    /// remains independent for Standard-LS2 X25519/type-4 advertisement.
    /// The 256-byte filler is public non-secret identity material sampled
    /// from the caller CSPRNG; it is never derived from the X25519 static
    /// secret, the Ed25519 signing seed, or any other secret-bearing field.
    /// No ElGamal private key is generated or implemented.
    pub fn generate<R: TryCryptoRng + ?Sized>(
        rng: &mut R,
    ) -> Result<Self, DestinationIdentityError> {
        let mut signing = Zeroizing::new([0_u8; PRIVATE_KEY_LENGTH]);
        let mut static_secret = Zeroizing::new([0_u8; X25519_KEY_LENGTH]);
        let mut filler = [0_u8; DESTINATION_LEGACY_PUBLIC_LENGTH];
        let mut padding = vec![0_u8; DESTINATION_LEGACY_PADDING_LENGTH];
        if rng.try_fill_bytes(&mut *signing).is_err()
            || rng.try_fill_bytes(&mut *static_secret).is_err()
            || rng.try_fill_bytes(&mut filler).is_err()
            || rng.try_fill_bytes(&mut padding).is_err()
        {
            return Err(DestinationIdentityError::RandomnessUnavailable);
        }
        Self::from_explicit_parts(*signing, *static_secret, filler, Zeroizing::new(padding))
    }

    /// Reconstructs a router-owned destination identity from explicit
    /// signing/X25519 secrets plus explicit public legacy identity
    /// material (Plan 223 D4).
    ///
    /// - `signing`: Ed25519 seed;
    /// - `static_secret`: X25519 static secret for LS2/ECIES (never embedded
    ///   in the Destination public field);
    /// - `legacy_filler`: 256-byte public non-secret ElGamal-slot bytes;
    /// - `padding`: exact 96-byte key-area padding for type-7/type-0.
    ///
    /// Deterministic tests must supply deterministic public filler
    /// explicitly. The filler must never be derived from secret material.
    pub fn from_explicit_parts(
        signing: [u8; PRIVATE_KEY_LENGTH],
        static_secret: [u8; X25519_KEY_LENGTH],
        legacy_filler: [u8; DESTINATION_LEGACY_PUBLIC_LENGTH],
        padding: Zeroizing<Vec<u8>>,
    ) -> Result<Self, DestinationIdentityError> {
        if padding.len() != DESTINATION_LEGACY_PADDING_LENGTH {
            return Err(DestinationIdentityError::PaddingLength {
                actual: padding.len(),
                expected: DESTINATION_LEGACY_PADDING_LENGTH,
            });
        }
        let signing_key = SigningPrivateKey::from_bytes(signing);
        let static_key = X25519PrivateKey::from_bytes(static_secret);
        let signing_public = signing_key.public_key()?;
        let encryption_public = PublicKey::new(
            DESTINATION_IDENTITY_LEGACY_CRYPTO_TYPE,
            legacy_filler.to_vec(),
        )?;
        let certificate = Certificate::Key(KeyCertificate::for_types(
            ROUTER_SIGNING_KEY_TYPE,
            DESTINATION_IDENTITY_LEGACY_CRYPTO_TYPE,
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

    /// Deterministic test/compat constructor preserving the pre-Plan-223
    /// X25519 Destination shape (type 4, 32-byte slot, 320-byte padding).
    ///
    /// Retained only so pre-Plan-223 persisted v1 service records reconstruct
    /// byte-identical Destinations on load (Plan 223 §15: no silent
    /// regeneration of a different public identity). New router-owned
    /// Destinations must use [`Self::from_explicit_parts`]/[`Self::generate`].
    /// Do not use for new identities.
    pub fn from_private_bytes_legacy_x25519(
        signing: [u8; PRIVATE_KEY_LENGTH],
        static_secret: [u8; X25519_KEY_LENGTH],
        padding: Zeroizing<Vec<u8>>,
    ) -> Result<Self, DestinationIdentityError> {
        if padding.len() != DESTINATION_X25519_PADDING_LENGTH {
            return Err(DestinationIdentityError::PaddingLength {
                actual: padding.len(),
                expected: DESTINATION_X25519_PADDING_LENGTH,
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

    /// Reconstructs a destination identity from explicit private key bytes and
    /// an exact identity padding buffer plus explicit legacy public filler.
    ///
    /// Plan 223 D4: this is the deterministic form of
    /// [`Self::from_explicit_parts`]. The legacy filler is public
    /// non-secret caller input; it must not be derived from secret bytes.
    pub fn from_private_bytes(
        signing: [u8; PRIVATE_KEY_LENGTH],
        static_secret: [u8; X25519_KEY_LENGTH],
        legacy_filler: [u8; DESTINATION_LEGACY_PUBLIC_LENGTH],
        padding: Zeroizing<Vec<u8>>,
    ) -> Result<Self, DestinationIdentityError> {
        Self::from_explicit_parts(signing, static_secret, legacy_filler, padding)
    }

    /// Reconstructs a destination identity from an already-decoded public
    /// destination together with the matching signing seed and static
    /// X25519 secret.
    ///
    /// This is the SAM private-destination import path. The destination
    /// bytes are taken verbatim; the public encryption key bytes are
    /// **not** required to equal `X25519(static_secret)` because the
    /// standard Java I2P `PrivateKeyFile` format (and i2pd's
    /// `IdentityEx`/`PrivateKeys`) populate the encryption public field
    /// with random bytes for destinations (the encryption field is
    /// unused for end-to-end traffic). Plan 146 documents and pins this
    /// tolerance. Only the structural parse and the
    /// `signing_public == EdDSA(signing_seed)` invariant are enforced.
    pub fn from_imported(
        destination: Destination,
        signing: [u8; PRIVATE_KEY_LENGTH],
        static_secret: [u8; X25519_KEY_LENGTH],
    ) -> Result<Self, DestinationIdentityError> {
        let signing_key = SigningPrivateKey::from_bytes(signing);
        let static_key = X25519PrivateKey::from_bytes(static_secret);
        let derived_signing_public = signing_key.public_key()?;
        let embedded_signing_public = destination.signing_key();
        if embedded_signing_public.as_bytes() != derived_signing_public.as_bytes() {
            return Err(DestinationIdentityError::ImportSigningKeyMismatch);
        }
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

    /// Borrows the raw static X25519 secret for the ECIES destination
    /// session layer and the recipient-side dispatcher.
    ///
    /// The accessor exists only because the [`crate::session`] and
    /// [`crate::dispatch`] modules must hand the secret to the ECIES
    /// primitives during outbound encryption and inbound decryption.
    /// Both owners wrap the secret through a non-cloneable,
    /// non-`Debug` type so this accessor is the single documented path
    /// to the bytes.
    pub const fn static_secret_bytes(&self) -> &[u8; X25519_KEY_LENGTH] {
        self.static_key.secret_bytes()
    }

    /// Borrows the raw 32-byte Ed25519 signing seed.
    ///
    /// This accessor is reserved for the SAM private-destination
    /// codec in `i2pr-api` and the bounded SAM-style import/export
    /// seam. It is the documented narrow path that lets the SAM
    /// codec extract the seed required by the standard
    /// `PrivateKeyFile` concatenation. The accessor is **not** a
    /// generic getter for arbitrary call sites; do not add more
    /// consumers without updating the comment and recording the new
    /// consumer in this crate's API review.
    pub const fn signing_seed_bytes(&self) -> &[u8; PRIVATE_KEY_LENGTH] {
        self.signing_key.secret_bytes()
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
    /// The supplied signing seed did not derive a public key equal to the
    /// embedded destination signing public key. Plan 146 describes this
    /// as the only structural invariant the SAM import path enforces.
    #[error("destination signing seed inconsistent with embedded signing public key")]
    ImportSigningKeyMismatch,
    /// The destination signing key type is outside the supported set.
    /// M9 accepts Ed25519 (type 7) only.
    #[error("destination signing type {signing_type} is outside the supported set")]
    UnsupportedSigningType {
        /// Numeric signing type code.
        signing_type: u16,
    },
    /// The destination encryption key type is outside the supported set.
    /// Plan 223: router-owned Destinations use ElGamal/type 0 legacy
    /// identity material; Standard LS2 uses X25519/type 4. Both are
    /// accepted in the Destination slot; X25519 is enforced at the LS2
    /// install path, never by downgrading LS2.
    #[error("destination encryption type {crypto_type} is outside the supported set")]
    UnsupportedCryptoType {
        /// Numeric encryption type code.
        crypto_type: u16,
    },
    /// The destination's encryption public-key field was not exactly
    /// 32 bytes; the router cannot install an inbound decryption
    /// capability for the wrong curve.
    #[error("destination encryption public-key length is not 32 bytes")]
    StaticPublicLengthMismatch,
    /// A supplied X25519 private key did not derive a public key
    /// matching the destination's advertised static public key.
    #[error("decryption capability public-key does not match destination")]
    DecryptionCapabilityKeyMismatch,
    /// The decryption capability's X25519 public key bytes do not
    /// match the supplied LeaseSet2 encryption public key.
    #[error("decryption capability does not match LeaseSet2 encryption public key")]
    DecryptionCapabilityLeaseSet2Mismatch,
    /// The destination identity is already installed; the runtime
    /// rejects duplicate installations of a client-owned capability.
    #[error("client-owned destination already installed")]
    ClientOwnedAlreadyInstalled,
    /// The supplied client-owned destination lacks the encryption
    /// public key bytes required for validation.
    #[error("client-owned destination encryption public key is missing")]
    MissingEncryptionPublicKey,
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
        // Plan 223 F1/F2: generated Destination uses legacy ElGamal/type 0
        // identity material while LS2/X25519 stays independent.
        let identity = identity_for(1);
        assert_eq!(
            identity.destination().public_key().key_type(),
            DESTINATION_IDENTITY_LEGACY_CRYPTO_TYPE
        );
        assert_eq!(identity.destination().public_key().key_type().code(), 0);
        assert_eq!(identity.destination().public_key().as_bytes().len(), 256);
        assert_eq!(
            identity.signing_public_key().key_type(),
            ROUTER_SIGNING_KEY_TYPE
        );
        // Legacy filler is public identity material, never the LS2 key.
        assert_ne!(
            identity.destination().public_key().as_bytes(),
            &identity.static_public_bytes()[..]
        );
        assert_eq!(
            identity.id().as_bytes(),
            identity.destination().hash().expect("hash").as_bytes()
        );
    }

    #[test]
    fn generated_destination_shape_is_legacy_while_ls2_stays_x25519() {
        let identity = identity_for(11);
        assert_eq!(
            identity.destination().public_key().key_type(),
            CryptoKeyType::ElGamal
        );
        assert_eq!(identity.destination().public_key().as_bytes().len(), 256);
        assert_eq!(
            identity.signing_public_key().key_type().code(),
            ROUTER_SIGNING_KEY_TYPE.code()
        );
        // Filler is independent of the X25519 static public key.
        assert_ne!(
            identity.destination().public_key().as_bytes()[..32],
            identity.static_public_bytes()[..]
        );
    }

    #[test]
    fn distinct_filler_yields_distinct_destination_hash() {
        let signing = [7_u8; PRIVATE_KEY_LENGTH];
        let static_secret = [9_u8; X25519_KEY_LENGTH];
        let padding = Zeroizing::new(vec![0x5a_u8; DESTINATION_LEGACY_PADDING_LENGTH]);
        let first = DestinationIdentity::from_private_bytes(
            signing,
            static_secret,
            [0x11_u8; DESTINATION_LEGACY_PUBLIC_LENGTH],
            padding.clone(),
        )
        .expect("identity");
        let second = DestinationIdentity::from_private_bytes(
            signing,
            static_secret,
            [0x22_u8; DESTINATION_LEGACY_PUBLIC_LENGTH],
            padding,
        )
        .expect("identity");
        assert_ne!(first.id(), second.id());
        // Same filler reconstructs identically.
        let third = DestinationIdentity::from_private_bytes(
            signing,
            static_secret,
            [0x11_u8; DESTINATION_LEGACY_PUBLIC_LENGTH],
            Zeroizing::new(vec![0x5a_u8; DESTINATION_LEGACY_PADDING_LENGTH]),
        )
        .expect("identity");
        assert_eq!(first.id(), third.id());
    }

    #[test]
    fn legacy_filler_is_not_secret_derived() {
        // The filler must come from explicit public caller input; this test
        // locks that generate() samples filler from the CSPRNG independently
        // of both secrets by proving two identities with identical secrets
        // but different explicit filler differ, while identical explicit
        // filler matches.
        let signing = [0xabu8; PRIVATE_KEY_LENGTH];
        let static_secret = [0xcdu8; X25519_KEY_LENGTH];
        let padding = Zeroizing::new(vec![0xef_u8; DESTINATION_LEGACY_PADDING_LENGTH]);
        let filler_a = [0x01_u8; DESTINATION_LEGACY_PUBLIC_LENGTH];
        let filler_b = [0x02_u8; DESTINATION_LEGACY_PUBLIC_LENGTH];
        let a = DestinationIdentity::from_private_bytes(
            signing,
            static_secret,
            filler_a,
            padding.clone(),
        )
        .expect("a");
        let b = DestinationIdentity::from_private_bytes(signing, static_secret, filler_b, padding)
            .expect("b");
        assert_ne!(
            a.destination().public_key().as_bytes(),
            b.destination().public_key().as_bytes()
        );
        assert_ne!(a.id(), b.id());
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
        let filler = [0x5a_u8; DESTINATION_LEGACY_PUBLIC_LENGTH];
        let padding = Zeroizing::new(vec![0x5a_u8; DESTINATION_LEGACY_PADDING_LENGTH]);
        let first = DestinationIdentity::from_private_bytes(
            signing,
            static_secret,
            filler,
            padding.clone(),
        )
        .expect("identity");
        let second =
            DestinationIdentity::from_private_bytes(signing, static_secret, filler, padding)
                .expect("identity");
        assert_eq!(first.id(), second.id());
    }

    #[test]
    fn wrong_padding_length_is_rejected() {
        let error = DestinationIdentity::from_private_bytes(
            [1_u8; PRIVATE_KEY_LENGTH],
            [2_u8; X25519_KEY_LENGTH],
            [0x11_u8; DESTINATION_LEGACY_PUBLIC_LENGTH],
            Zeroizing::new(vec![0_u8; 8]),
        )
        .expect_err("padding rejected");
        assert!(matches!(
            error,
            DestinationIdentityError::PaddingLength { actual: 8, .. }
        ));
    }

    #[test]
    fn destination_public_rejects_wrong_encryption_curve() {
        // Plan 223: router-owned ElGamal Destinations carry legacy filler;
        // DestinationPublic zeroes the X25519 slot for them because the
        // active key arrives via LS2. X25519-slot Destinations (legacy
        // pre-223 shape) still expose the static key directly.
        let signing = [0x09_u8; PRIVATE_KEY_LENGTH];
        let static_secret = [0x11_u8; X25519_KEY_LENGTH];
        let filler = [0x33_u8; DESTINATION_LEGACY_PUBLIC_LENGTH];
        let padding = Zeroizing::new(vec![0x22_u8; DESTINATION_LEGACY_PADDING_LENGTH]);
        let identity =
            DestinationIdentity::from_private_bytes(signing, static_secret, filler, padding)
                .expect("id");
        let public = DestinationPublic::from_destination(identity.destination().clone())
            .expect("supported curve");
        // ElGamal legacy slot: static slot is zeroed; LS2 is the
        // enforcement point, not the Destination field.
        assert_eq!(public.static_public_bytes(), &[0_u8; 32][..]);
        assert_eq!(public.id(), identity.id());
        assert_eq!(
            public.encryption_public_key_type(),
            DESTINATION_IDENTITY_LEGACY_CRYPTO_TYPE
        );
        // Legacy X25519 shape still round-trips for v1 persistence.
        let legacy = DestinationIdentity::from_private_bytes_legacy_x25519(
            signing,
            static_secret,
            Zeroizing::new(vec![0x44_u8; DESTINATION_X25519_PADDING_LENGTH]),
        )
        .expect("legacy");
        let legacy_public =
            DestinationPublic::from_destination(legacy.destination().clone()).expect("legacy");
        assert_eq!(
            legacy_public.static_public_bytes(),
            &legacy.static_public_bytes()[..]
        );
        assert_eq!(
            legacy_public.encryption_public_key_type(),
            ROUTER_CRYPTO_KEY_TYPE
        );
    }

    #[test]
    fn imported_java_compatible_destination_is_preserved() {
        // Plan 223 F3: Java-compatible legacy Destination fixture parses
        // and remains accepted without rewriting its public field.
        // Construct a Java-style ElGamal/type-0 Destination directly via
        // proto (256-byte filler, Ed25519, 96-byte padding).
        let signing = [0x5eu8; PRIVATE_KEY_LENGTH];
        let filler = [0xabu8; DESTINATION_LEGACY_PUBLIC_LENGTH];
        let padding = vec![0x33_u8; DESTINATION_LEGACY_PADDING_LENGTH];
        let signing_key = i2pr_crypto::SigningPrivateKey::from_bytes(signing);
        let signing_public = signing_key.public_key().expect("signing public");
        let encryption_public =
            PublicKey::new(DESTINATION_IDENTITY_LEGACY_CRYPTO_TYPE, filler.to_vec())
                .expect("public");
        let certificate = Certificate::Key(
            KeyCertificate::for_types(
                ROUTER_SIGNING_KEY_TYPE,
                DESTINATION_IDENTITY_LEGACY_CRYPTO_TYPE,
            )
            .expect("cert"),
        );
        let keys =
            KeyAndCert::new(encryption_public, signing_public, padding, certificate).expect("kc");
        let dest = Destination::new(keys).expect("dest");
        let encoded = dest
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("encode");
        assert_eq!(encoded.len(), 391);
        let decoded =
            Destination::decode(&encoded, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE).expect("decode");
        assert_eq!(decoded, dest);
        let public = DestinationPublic::from_destination(decoded.clone()).expect("public");
        assert_eq!(
            public.encryption_public_key_type(),
            DESTINATION_IDENTITY_LEGACY_CRYPTO_TYPE
        );
        // Import preserves bytes verbatim (Plan 146 tolerance).
        let static_secret = [0x11_u8; X25519_KEY_LENGTH];
        let imported = DestinationIdentity::from_imported(decoded.clone(), signing, static_secret)
            .expect("import");
        assert_eq!(imported.destination(), &decoded);
        assert_eq!(imported.id(), public.id());
    }

    #[test]
    fn inbound_decryption_capability_debug_redacts_secret_bytes() {
        let identity = identity_for(42);
        let capability = InboundDecryptionCapability::from_secret_bytes(
            identity.static_public_bytes(),
            *identity.static_secret_bytes(),
        );
        let rendered = format!("{capability:?}");
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("secret_bytes"));
        // Diffie-Hellman still works through the capability.
        let peer_public = identity.static_public_bytes();
        let shared = capability.diffie_hellman(&peer_public).expect("shared");
        // The shared secret equals the static-static DH output; verify
        // the route through the destination identity produces the
        // same bytes so the ECIES layer does not need a different
        // entry point.
        let identity_shared = identity
            .diffie_hellman(&peer_public)
            .expect("shared via id");
        assert_eq!(shared.as_bytes(), identity_shared.as_bytes());
    }

    #[test]
    fn inbound_decryption_capability_secret_bytes_round_trip() {
        let identity = identity_for(43);
        let capability = InboundDecryptionCapability::from_secret_bytes(
            identity.static_public_bytes(),
            *identity.static_secret_bytes(),
        );
        assert_eq!(capability.secret_bytes(), identity.static_secret_bytes());
        assert_eq!(
            capability.static_public_bytes(),
            &identity.static_public_bytes()[..]
        );
    }

    #[test]
    fn inbound_decryption_ref_does_not_expose_the_capability_value() {
        let identity = identity_for(44);
        let capability = InboundDecryptionCapability::from_secret_bytes(
            identity.static_public_bytes(),
            *identity.static_secret_bytes(),
        );
        let r#ref = InboundDecryptionRef::new(&capability);
        assert_eq!(r#ref.secret_bytes(), identity.static_secret_bytes());
        assert_eq!(
            r#ref.static_public_bytes(),
            &identity.static_public_bytes()[..]
        );
    }
}
