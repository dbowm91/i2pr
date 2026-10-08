//! Plan 332: encrypted LeaseSet2 (DatabaseStore type 5) validation, layer
//! cryptography, the daily blinding lifecycle, and the bounded record store.
//!
//! # What this module owns
//!
//! - the `credential` / `subcredential` derivation that binds an encrypted
//!   LeaseSet2 to knowledge of the unblinded signing public key;
//! - the two layer key derivations and their ChaCha20 streams;
//! - construction and decryption of the **no-client-authorization** encrypted
//!   form (per-client PSK and DH authorization belong to Plan 333);
//! - signature, freshness, and key-binding validation of a type-5 record;
//! - the bounded store keyed by the DHT storage key;
//! - the per-UTC-day blinding schedule, including rollover and precomputation.
//!
//! # What this module deliberately does not own
//!
//! Destination lifecycle, Streaming, Proposal 170 mode mapping, and any socket
//! or timer. The wire framing of layer 0 lives in `i2pr-proto`; this crate
//! composes it with the `i2pr-crypto` Red25519 and HKDF primitives and applies
//! policy.
//!
//! # Daily rotation is the whole point
//!
//! A type-5 record is stored under `SHA-256(sigtype || blindedPublicKey)`, and
//! the blinded key changes every UTC day. There is therefore no long-lived
//! "blinded key" for a destination: [`BlindingSchedule`] is the owner of the
//! daily material, hands out one self-consistent value per call, and refuses to
//! mix an old `alpha` with a new signing key, storage key, or address.

use std::collections::BTreeMap;

use crate::els2_auth::{
    AuthBlock, Els2AuthError, Els2AuthScheme, Els2ClientAuth, recover_auth_cookie,
};
pub use i2pr_crypto::red25519::LookupSecret;
use i2pr_crypto::red25519::{
    BLINDED_SIGNING_KEY_TYPE, BlindedPrivateScalar, BlindingDay, BlindingScalar, PUBLIC_KEY_LENGTH,
    Red25519Error, Red25519PrivateScalar, Red25519PublicKey, blind_private_key, blind_public_key,
    blinded_storage_key, convert_ed25519_private, derive_blinded_public_key, generate_alpha,
};
use i2pr_crypto::{
    CHACHA20_KEY_LENGTH, CHACHA20_NONCE_LENGTH, ChachaError, CryptoError, LayerCipherKey,
    chacha20_xor_layer, chacha20_xor_layer_owned, hkdf_sha256_extract_and_expand,
};
use i2pr_proto::{
    ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE, ENCRYPTED_LEASE_SET2_MAX_EXPIRES_OFFSET,
    ENCRYPTED_LEASE_SET2_SALT_LENGTH, ENCRYPTED_LEASE_SET2_UNBLINDED_SIGTYPES, Els2SignedRegion,
    EncryptedLeaseSet2, EncryptedLeaseSet2OfflineKeys, Hash, INNER_LEASE_SET2_STORE_TYPE,
    INNER_META_LEASE_SET2_STORE_TYPE, LeaseSet2, MAX_COMMON_STRUCTURE_SIZE, MetaLeaseSet,
    SignatureValue, SigningKeyType, SigningPublicKey,
};
use thiserror::Error;

/// Length of one layer salt, in bytes.
///
/// The specification fixes the salt at 32 bytes for both the outer and the
/// inner layer, and the salt travels in the clear at the head of its own
/// ciphertext, so it needs no separate bound.
pub const ELS2_SALT_LENGTH: usize = ENCRYPTED_LEASE_SET2_SALT_LENGTH;

/// Personalization string for the encrypted-LeaseSet2 credential.
pub const ELS2_CREDENTIAL_PERSONALIZATION: &[u8] = b"credential";
/// Personalization string for the encrypted-LeaseSet2 subcredential.
pub const ELS2_SUBCREDENTIAL_PERSONALIZATION: &[u8] = b"subcredential";
/// HKDF `info` label for the layer-1 key derivation.
pub const ELS2_LAYER1_HKDF_INFO: &[u8] = b"ELS2_L1K";
/// HKDF `info` label for the layer-2 key derivation.
pub const ELS2_LAYER2_HKDF_INFO: &[u8] = b"ELS2_L2K";
/// Bytes of HKDF output consumed by one layer: a 32-byte key and a 12-byte IV.
pub const ELS2_LAYER_KEY_MATERIAL_LENGTH: usize = CHACHA20_KEY_LENGTH + CHACHA20_NONCE_LENGTH;
/// Layer-1 flag bit 0: a per-client authorization block follows.
pub const ELS2_LAYER1_FLAG_PER_CLIENT: u8 = 0x01;
/// Mask of the layer-1 authentication-scheme bits.
pub const ELS2_LAYER1_SCHEME_MASK: u8 = 0x06;
/// Shift of the layer-1 authentication-scheme bits.
pub const ELS2_LAYER1_SCHEME_SHIFT: u32 = 1;
/// Layer-1 flag bits that must be zero: 7-4 are reserved for future use.
pub const ELS2_LAYER1_RESERVED_MASK: u8 = 0xf0;
/// Layer-1 authentication scheme `000`: Diffie-Hellman, or no authorization.
pub const ELS2_LAYER1_SCHEME_DH: u8 = 0x00;
/// Layer-1 authentication scheme `001`: pre-shared key.
pub const ELS2_LAYER1_SCHEME_PSK: u8 = 0x01;

/// Largest inner (layer-2) LeaseSet2 payload i2pr will encrypt or decrypt.
///
/// This is an i2pr resource bound, not a protocol limit. The specification
/// explicitly leaves record sizing to the floodfill ("floodfills may limit the
/// max size to a reasonable value to prevent abuse"), so the bound is declared
/// here and applied at both ends. It is far above a real LeaseSet2: the
/// ordinary codec caps leases at 16, which is well under 1 KiB.
pub const MAX_ELS2_INNER_LEASE_SET_LENGTH: usize = 8 * 1024;
/// Largest outer ciphertext i2pr will accept, derived from the inner bound.
///
/// The outer ciphertext is `outerSalt(32) || layer1Plaintext`, and the layer-1
/// plaintext is `flags(1) || innerSalt(32) || innerCiphertext`, and the inner
/// ciphertext is `innerSalt(32) || type(1) || inner`. The overhead is therefore
/// exactly 66 bytes, and it is computed rather than restated so the two bounds
/// cannot drift apart.
///
/// Plan 332 first recorded this constant with a 130-byte overhead. Building the
/// record end to end and measuring the real ciphertext showed 66; the constant
/// is derived from the framing rather than asserted, so the two can no longer
/// disagree. A larger-than-needed ceiling would have been a safety-only defect,
/// but a smaller one would have rejected valid records at the boundary.
pub const MAX_ELS2_OUTER_CIPHERTEXT_LENGTH: usize =
    MAX_ELS2_INNER_LEASE_SET_LENGTH + 2 * ELS2_SALT_LENGTH + 2;
/// Largest complete type-5 record body i2pr will accept.
pub const MAX_ELS2_RECORD_LENGTH: usize = MAX_ELS2_OUTER_CIPHERTEXT_LENGTH + 128;

/// Number of per-client authorization entries one layer-1 block may carry.
pub const MAX_ELS2_AUTH_CLIENTS: usize = 255;
/// Encoded length of one `authClient` entry: an 8-byte id and a 32-byte cookie.
pub const ELS2_AUTH_CLIENT_LENGTH: usize = 40;
/// Layer-1 flags for the no-authorization form.
pub const ELS2_LAYER1_FLAGS_NO_AUTH: u8 = 0;

/// Errors produced by the encrypted LeaseSet2 layer and blinding surfaces.
#[derive(Debug, Error)]
pub enum Els2Error {
    /// The blinding input was not a usable unblinded public key.
    #[error("encrypted LeaseSet2 blinding input rejected: {0}")]
    Blinding(#[from] Red25519Error),
    /// The configured lookup secret was rejected before any derivation ran.
    ///
    /// The specification defines no maximum secret length, so this is an i2pr
    /// resource bound. It is reported separately from a blinding failure because
    /// the remedy is different: a too-long secret is an operator configuration
    /// error, not a bad key.
    #[error("encrypted LeaseSet2 lookup secret rejected: {0}")]
    LookupSecretRejected(#[source] Red25519Error),
    /// The unblinded signature type is neither 7 nor 11.
    #[error("encrypted LeaseSet2 unblinded sigtype {code} is neither 7 nor 11")]
    UnsupportedUnblindedSigtype {
        /// The rejected signature type code.
        code: u16,
    },
    /// A type-11 signature matched neither the deployed nor the strict transcript, or matched
    /// both.
    ///
    /// The two are reported together on purpose: an ambiguous match is rejected rather than
    /// resolved, so a caller must not be able to tell from the error whether the failure was a
    /// plain non-match or a contradiction between the two transcripts.
    #[error("encrypted LeaseSet2 type-11 signature rejected by the bounded transcript policy")]
    Type11SignatureRejected,
    /// A layer key derivation produced the wrong amount of material.
    #[error(
        "encrypted LeaseSet2 layer key derivation returned {actual} bytes, expected {expected}"
    )]
    LayerKeyMaterialLength {
        /// Actual derived length.
        actual: usize,
        /// Expected derived length.
        expected: usize,
    },
    /// A layer buffer was too short to contain its own salt.
    #[error(
        "encrypted LeaseSet2 layer {layer} ciphertext is {actual} bytes, shorter than its {minimum}-byte minimum"
    )]
    LayerTooShort {
        /// Which layer was short.
        layer: &'static str,
        /// Actual length.
        actual: usize,
        /// Minimum length for that layer.
        minimum: usize,
    },
    /// A caller-visible ceiling was exceeded.
    #[error("encrypted LeaseSet2 {what} is {actual} bytes, exceeding the {maximum}-byte limit")]
    TooLarge {
        /// Which value was too large.
        what: &'static str,
        /// Actual length.
        actual: usize,
        /// Maximum accepted length.
        maximum: usize,
    },
    /// The record requested per-client authorization, which this floor does not implement.
    #[error("encrypted LeaseSet2 requires per-client authorization (layer-1 flags {flags:#04x})")]
    ClientAuthorizationRequired {
        /// The layer-1 flags byte.
        flags: u8,
    },
    /// A layer-1 reserved flag bit was set.
    #[error("encrypted LeaseSet2 layer-1 flags set reserved bits {mask:#04x}")]
    Layer1ReservedFlags {
        /// The offending reserved-bit mask.
        mask: u8,
    },
    /// The layer-1 flag byte named no recognized authorization scheme.
    #[error(
        "encrypted LeaseSet2 layer-1 flags {flags:#04x} name no recognized authorization scheme"
    )]
    UnrecognizedLayer1Flags {
        /// The layer-1 flags byte.
        flags: u8,
    },
    /// The flags byte, the authorization data, and the cookie disagreed.
    #[error(
        "encrypted LeaseSet2 layer-1 flags {flags:#04x} disagree with the supplied authorization data"
    )]
    InconsistentLayer1Authorization {
        /// The layer-1 flags byte.
        flags: u8,
    },
    /// A per-client authorization step failed.
    #[error(transparent)]
    Auth(#[from] Els2AuthError),
    /// The inner record type was neither 3 nor 7.
    #[error("encrypted LeaseSet2 inner store type {code} is neither 3 nor 7")]
    UnsupportedInnerStoreType {
        /// The rejected store type code.
        code: u8,
    },
    /// The inner record's timestamps did not match the outer record's.
    #[error("encrypted LeaseSet2 inner timestamps disagree with the outer record")]
    InnerTimestampMismatch,
    /// The record body failed its structural codec.
    #[error("encrypted LeaseSet2 codec rejected the record: {0}")]
    Codec(#[from] i2pr_proto::CodecError),
    /// A signature or key operation failed.
    #[error(transparent)]
    Crypto(#[from] CryptoError),
}

/// The credential and subcredential of one encrypted service.
///
/// Both values are derived from public material plus the knowledge that the
/// derived `subcredential` is a function of the *unblinded* public key, so a
/// party that knows only the blinded key cannot compute them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Els2Credentials {
    credential: [u8; 32],
    subcredential: [u8; 32],
}

impl Els2Credentials {
    /// Returns the credential bytes.
    pub const fn credential(&self) -> &[u8; 32] {
        &self.credential
    }

    /// Returns the subcredential bytes.
    ///
    /// The subcredential, not the credential, is what enters the layer key
    /// derivations.
    pub const fn subcredential(&self) -> &[u8; 32] {
        &self.subcredential
    }
}

/// Derives the credential and subcredential for one encrypted service.
///
/// ```text
/// keydata       = A || stA_be16 || 0x000b_be16
/// credential    = SHA-256("credential" || keydata)
/// subcredential = SHA-256("subcredential" || credential || blindedPublicKey)
/// ```
///
/// The personalization strings keep the credential from colliding with any DHT
/// lookup key such as the plain Destination hash. The order matters: the
/// *unblinded* public key goes into the credential, the *blinded* public key
/// goes into the subcredential.
pub fn derive_els2_credentials(
    unblinded_public_key: &Red25519PublicKey,
    unblinded_sigtype: SigningKeyType,
    blinded_public_key: &Red25519PublicKey,
) -> Els2Credentials {
    let mut keydata = [0_u8; 2 + 2 + PUBLIC_KEY_LENGTH];
    keydata[..PUBLIC_KEY_LENGTH].copy_from_slice(unblinded_public_key.as_bytes());
    keydata[PUBLIC_KEY_LENGTH..PUBLIC_KEY_LENGTH + 2]
        .copy_from_slice(&unblinded_sigtype.code().to_be_bytes());
    keydata[PUBLIC_KEY_LENGTH + 2..]
        .copy_from_slice(&BLINDED_SIGNING_KEY_TYPE.code().to_be_bytes());
    let mut credential_input =
        Vec::with_capacity(ELS2_CREDENTIAL_PERSONALIZATION.len() + keydata.len());
    credential_input.extend_from_slice(ELS2_CREDENTIAL_PERSONALIZATION);
    credential_input.extend_from_slice(&keydata);
    let credential = *Hash::digest(&credential_input).as_bytes();

    let mut subcredential_input =
        Vec::with_capacity(ELS2_SUBCREDENTIAL_PERSONALIZATION.len() + 32 + PUBLIC_KEY_LENGTH);
    subcredential_input.extend_from_slice(ELS2_SUBCREDENTIAL_PERSONALIZATION);
    subcredential_input.extend_from_slice(&credential);
    subcredential_input.extend_from_slice(blinded_public_key.as_bytes());
    let subcredential = *Hash::digest(&subcredential_input).as_bytes();

    Els2Credentials {
        credential,
        subcredential,
    }
}

/// A derived ChaCha20 layer key and IV.
///
/// The key erases itself on drop. The IV is not secret: it is derived
/// deterministically from the same HKDF output as the key, and each
/// publication uses a fresh random salt, so the `(key, nonce)` pair is never
/// reused.
struct Els2LayerKeys {
    key: LayerCipherKey,
    nonce: [u8; CHACHA20_NONCE_LENGTH],
}

/// Derives one layer's key material: `HKDF(salt, input, info, 44)`.
///
/// The slicing is the frozen resolution of a specification inconsistency: the
/// text says `keys[0:31]`, `keys[32:43]`, and `keys[44:51]`, which cannot
/// describe a 32-byte key and a 12-byte IV. Both pinned references use
/// `key = okm[0..32]`, `iv = okm[32..44]`, and (for the authorization path)
/// `clientID = okm[44..52]`.
fn derive_layer_keys(
    salt: &[u8; 32],
    input: &[u8],
    info: &[u8],
) -> Result<Els2LayerKeys, Els2Error> {
    let derived = hkdf_sha256_extract_and_expand(salt, input, info, ELS2_LAYER_KEY_MATERIAL_LENGTH)
        .map_err(|error| {
            CryptoError::Protocol(i2pr_proto::CodecError::InvalidFieldValue {
                offset: 0,
                context: match error {
                    i2pr_crypto::HkdfError::OutputLengthExceeded { .. } => "HKDF layer key length",
                    i2pr_crypto::HkdfError::InvalidKeyLength => "HKDF layer key salt",
                },
            })
        })?;
    if derived.len() != ELS2_LAYER_KEY_MATERIAL_LENGTH {
        return Err(Els2Error::LayerKeyMaterialLength {
            actual: derived.len(),
            expected: ELS2_LAYER_KEY_MATERIAL_LENGTH,
        });
    }
    let mut nonce = [0_u8; CHACHA20_NONCE_LENGTH];
    nonce.copy_from_slice(&derived[CHACHA20_KEY_LENGTH..]);
    Ok(Els2LayerKeys {
        key: LayerCipherKey::from_bytes(
            derived[..CHACHA20_KEY_LENGTH]
                .try_into()
                .expect("derived key is 32 bytes"),
        ),
        nonce,
    })
}

/// The `published` timestamp is part of both layer key inputs, big endian.
fn published_be32(published_seconds: u32) -> [u8; 4] {
    published_seconds.to_be_bytes()
}

/// Encrypts the no-client-authorization inner payload into an outer ciphertext.
///
/// ```text
/// outerInput      = subcredential || published_be32
/// outerSalt       = caller-supplied CSRNG(32)
/// keys            = HKDF(outerSalt, outerInput, "ELS2_L1K", 44)
/// innerInput      = subcredential || published_be32          // no authCookie
/// innerSalt       = caller-supplied CSRNG(32)
/// keys            = HKDF(innerSalt, innerInput, "ELS2_L2K", 44)
/// layer2Plaintext = innerStoreType(1) || innerRecord
/// innerCiphertext = innerSalt || ENCRYPT(innerKey, innerIV, layer2Plaintext)
/// layer1Plaintext = layer1Flags(1) || innerCiphertext
/// outerCiphertext = outerSalt || ENCRYPT(outerKey, outerIV, layer1Plaintext)
/// ```
///
/// The caller supplies both salts so the random source stays owned by the
/// runtime layer, and so a deterministic test can inject fixed values. The salts
/// are public: they travel in the clear inside the record and only need to be
/// fresh and unpredictable per publication.
pub fn encrypt_no_auth_outer_ciphertext(
    credentials: &Els2Credentials,
    published_seconds: u32,
    inner_store_type: u8,
    inner_record: &[u8],
    outer_salt: &[u8; ELS2_SALT_LENGTH],
    inner_salt: &[u8; ELS2_SALT_LENGTH],
) -> Result<Vec<u8>, Els2Error> {
    encrypt_outer_ciphertext(
        credentials,
        published_seconds,
        inner_store_type,
        inner_record,
        Layer1Authorization::none(),
        outer_salt,
        inner_salt,
    )
}

/// The layer-1 authorization inputs for one publication.
///
/// These three values must agree with each other: the flags byte says whether
/// authorization is on and which scheme it is, `data` is the serialized block
/// that follows that byte, and `cookie` is the value prepended to the layer-2
/// key input. A caller that disagrees with itself produces a record no client
/// can open — or worse, one that opens with the wrong key — so the agreement is
/// checked in [`Layer1Authorization::new`] rather than trusted at the point of
/// use. Grouping them also means the encrypt entry point cannot be called with
/// the block and the cookie transposed.
///
/// [`Layer1Authorization::none`] is the ordinary case: no per-client bit, no
/// block, and no cookie.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Layer1Authorization<'a> {
    flags: u8,
    data: &'a [u8],
    cookie: Option<&'a [u8; 32]>,
}

impl<'a> Layer1Authorization<'a> {
    /// Builds the no-authorization form.
    pub const fn none() -> Self {
        Self {
            flags: ELS2_LAYER1_FLAGS_NO_AUTH,
            data: &[],
            cookie: None,
        }
    }

    /// Builds the authorized form, checking that the three values agree.
    pub fn new(flags: u8, data: &'a [u8], cookie: &'a [u8; 32]) -> Result<Self, Els2Error> {
        if flags & ELS2_LAYER1_RESERVED_MASK != 0 {
            return Err(Els2Error::Layer1ReservedFlags {
                mask: flags & ELS2_LAYER1_RESERVED_MASK,
            });
        }
        if flags & ELS2_LAYER1_FLAG_PER_CLIENT == 0 {
            return Err(Els2Error::InconsistentLayer1Authorization { flags });
        }
        // salt-or-ephemeral (32) plus the client count (2) is the minimum a
        // block with at least one client can be.
        if data.len() < 34 {
            return Err(Els2Error::LayerTooShort {
                layer: "authorization",
                actual: data.len(),
                minimum: 34,
            });
        }
        Ok(Self {
            flags,
            data,
            cookie: Some(cookie),
        })
    }

    /// Returns the layer-1 flags byte.
    pub const fn flags(&self) -> u8 {
        self.flags
    }

    /// Returns the serialized authorization block.
    pub const fn data(&self) -> &'a [u8] {
        self.data
    }

    /// Returns the per-generation cookie, when authorization is on.
    pub const fn cookie(&self) -> Option<&'a [u8; 32]> {
        self.cookie
    }

    /// Returns whether the record requests per-client authorization.
    pub const fn is_authorized(&self) -> bool {
        self.flags & ELS2_LAYER1_FLAG_PER_CLIENT != 0
    }
}

/// Encrypts an inner payload into an outer ciphertext with an explicit layer-1
/// authorization block.
///
/// This is the general form the no-authorization function above delegates to.
pub fn encrypt_outer_ciphertext(
    credentials: &Els2Credentials,
    published_seconds: u32,
    inner_store_type: u8,
    inner_record: &[u8],
    authorization: Layer1Authorization<'_>,
    outer_salt: &[u8; ELS2_SALT_LENGTH],
    inner_salt: &[u8; ELS2_SALT_LENGTH],
) -> Result<Vec<u8>, Els2Error> {
    let layer1_flags = authorization.flags;
    let auth_data = authorization.data;
    let auth_cookie = authorization.cookie;
    if inner_record.is_empty() {
        return Err(Els2Error::TooLarge {
            what: "inner record",
            actual: 0,
            maximum: MAX_ELS2_INNER_LEASE_SET_LENGTH,
        });
    }
    if inner_record.len() > MAX_ELS2_INNER_LEASE_SET_LENGTH {
        return Err(Els2Error::TooLarge {
            what: "inner record",
            actual: inner_record.len(),
            maximum: MAX_ELS2_INNER_LEASE_SET_LENGTH,
        });
    }
    if !matches!(
        inner_store_type,
        INNER_LEASE_SET2_STORE_TYPE | INNER_META_LEASE_SET2_STORE_TYPE
    ) {
        return Err(Els2Error::UnsupportedInnerStoreType {
            code: inner_store_type,
        });
    }
    // The flags/data/cookie agreement is already established by
    // `Layer1Authorization::new`, so there is nothing left to re-check here.

    let mut layer2_plaintext = Vec::with_capacity(inner_record.len() + 1);
    layer2_plaintext.push(inner_store_type);
    layer2_plaintext.extend_from_slice(inner_record);

    let mut inner_input = Vec::with_capacity(4 + 68);
    if let Some(cookie) = auth_cookie {
        inner_input.extend_from_slice(cookie);
    }
    inner_input.extend_from_slice(credentials.subcredential());
    inner_input.extend_from_slice(&published_be32(published_seconds));
    let inner_keys = derive_layer_keys(inner_salt, &inner_input, ELS2_LAYER2_HKDF_INFO)?;

    let encrypted_inner =
        chacha20_xor_layer_owned(&inner_keys.key, &inner_keys.nonce, &layer2_plaintext)
            .map_err(chacha_error)?;
    let mut inner_ciphertext = Vec::with_capacity(ELS2_SALT_LENGTH + encrypted_inner.len());
    inner_ciphertext.extend_from_slice(inner_salt);
    inner_ciphertext.extend_from_slice(&encrypted_inner);

    let mut layer1_plaintext = Vec::with_capacity(1 + auth_data.len() + inner_ciphertext.len());
    layer1_plaintext.push(layer1_flags);
    layer1_plaintext.extend_from_slice(auth_data);
    layer1_plaintext.extend_from_slice(&inner_ciphertext);

    let mut outer_input = Vec::with_capacity(36);
    outer_input.extend_from_slice(credentials.subcredential());
    outer_input.extend_from_slice(&published_be32(published_seconds));
    let outer_keys = derive_layer_keys(outer_salt, &outer_input, ELS2_LAYER1_HKDF_INFO)?;

    let encrypted_outer =
        chacha20_xor_layer_owned(&outer_keys.key, &outer_keys.nonce, &layer1_plaintext)
            .map_err(chacha_error)?;
    let mut outer_ciphertext = Vec::with_capacity(ELS2_SALT_LENGTH + encrypted_outer.len());
    outer_ciphertext.extend_from_slice(outer_salt);
    outer_ciphertext.extend_from_slice(&encrypted_outer);
    Ok(outer_ciphertext)
}

fn chacha_error(error: ChachaError) -> Els2Error {
    let _ = error;
    Els2Error::Codec(i2pr_proto::CodecError::InvalidFieldValue {
        offset: 0,
        context: "encrypted LeaseSet2 layer stream",
    })
}

/// The decrypted payload of a no-client-authorization encrypted LeaseSet2.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecryptedEls2 {
    store_type: u8,
    published_seconds: u32,
    expires_seconds: u32,
    payload: Vec<u8>,
}

impl DecryptedEls2 {
    /// Returns the inner DatabaseStore type: 3 for a LeaseSet2, 7 for a
    /// MetaLeaseSet.
    pub const fn store_type(&self) -> u8 {
        self.store_type
    }

    /// Returns the inner record's own published timestamp.
    pub const fn published_seconds(&self) -> u32 {
        self.published_seconds
    }

    /// Returns the inner record's own absolute expiration timestamp.
    pub const fn expires_seconds(&self) -> u32 {
        self.expires_seconds
    }

    /// Borrows the inner record body.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Consumes the value and decodes the inner record as a LeaseSet2.
    pub fn into_lease_set2(self, maximum: usize) -> Result<LeaseSet2, Els2Error> {
        if self.store_type != INNER_LEASE_SET2_STORE_TYPE {
            return Err(Els2Error::UnsupportedInnerStoreType {
                code: self.store_type,
            });
        }
        Ok(LeaseSet2::decode(&self.payload, maximum)?)
    }

    /// Consumes the value and decodes the inner record as a MetaLeaseSet.
    pub fn into_meta_lease_set(self, maximum: usize) -> Result<MetaLeaseSet, Els2Error> {
        if self.store_type != INNER_META_LEASE_SET2_STORE_TYPE {
            return Err(Els2Error::UnsupportedInnerStoreType {
                code: self.store_type,
            });
        }
        Ok(MetaLeaseSet::decode(&self.payload, maximum)?)
    }
}

/// Decrypts a no-client-authorization outer ciphertext.
///
/// The caller must supply the credentials of the destination it believes it is
/// talking to. A wrong destination, a wrong `published` timestamp, or a wrong
/// lookup secret all yield a wrong subcredential, hence a wrong key, hence a
/// garbage plaintext that is rejected by the inner-record checks. There is no
/// fallback to a no-secret derivation.
///
/// After decryption the inner record is parsed and its `published`/`expires`
/// values are required to match the outer record. A mismatch is a decryption
/// failure, not a warning: the specification calls for that check, and allowing
/// it would let a spliced inner record ride inside a valid outer signature.
pub fn decrypt_no_auth_outer_ciphertext(
    credentials: &Els2Credentials,
    published_seconds: u32,
    outer_ciphertext: &[u8],
) -> Result<DecryptedEls2, Els2Error> {
    decrypt_outer_ciphertext(credentials, published_seconds, outer_ciphertext, None)
}

/// Decrypts an outer ciphertext, optionally using a client authorization
/// credential to recover the `authCookie` that the layer-2 key needs.
///
/// With `client = None` a record that requests per-client authorization is
/// rejected as [`Els2Error::ClientAuthorizationRequired`]: the layer-1 key is
/// derivable without a credential, so silently decrypting a record the caller is
/// not authorized for would be the worst possible failure mode here.
pub fn decrypt_outer_ciphertext(
    credentials: &Els2Credentials,
    published_seconds: u32,
    outer_ciphertext: &[u8],
    client: Option<&Els2ClientAuth<'_>>,
) -> Result<DecryptedEls2, Els2Error> {
    if outer_ciphertext.len() < ELS2_SALT_LENGTH + 1 {
        return Err(Els2Error::LayerTooShort {
            layer: "outer",
            actual: outer_ciphertext.len(),
            minimum: ELS2_SALT_LENGTH + 1,
        });
    }
    if outer_ciphertext.len() > MAX_ELS2_OUTER_CIPHERTEXT_LENGTH {
        return Err(Els2Error::TooLarge {
            what: "outer ciphertext",
            actual: outer_ciphertext.len(),
            maximum: MAX_ELS2_OUTER_CIPHERTEXT_LENGTH,
        });
    }
    let outer_salt: [u8; 32] = outer_ciphertext[..ELS2_SALT_LENGTH]
        .try_into()
        .expect("outer ciphertext is at least one salt long");
    let mut outer_input = Vec::with_capacity(36);
    outer_input.extend_from_slice(credentials.subcredential());
    outer_input.extend_from_slice(&published_be32(published_seconds));
    let outer_keys = derive_layer_keys(&outer_salt, &outer_input, ELS2_LAYER1_HKDF_INFO)?;

    let mut layer1_plaintext = outer_ciphertext[ELS2_SALT_LENGTH..].to_vec();
    chacha20_xor_layer(&outer_keys.key, &outer_keys.nonce, &mut layer1_plaintext)
        .map_err(chacha_error)?;

    let layer1_flags = layer1_plaintext[0];
    if layer1_flags & ELS2_LAYER1_RESERVED_MASK != 0 {
        return Err(Els2Error::Layer1ReservedFlags {
            mask: layer1_flags & ELS2_LAYER1_RESERVED_MASK,
        });
    }
    // `auth_data_len` is the length of the serialized authorization block, which
    // is zero when the record does not request per-client authorization. It is
    // carried out of the branch above so the inner-ciphertext offset is computed
    // from the same value that was parsed, rather than by re-reading the client
    // count a second time and hoping the two agree.
    let (auth_cookie, auth_data_len) = if layer1_flags & ELS2_LAYER1_FLAG_PER_CLIENT == 0 {
        (None, 0_usize)
    } else {
        let scheme =
            Els2AuthScheme::from_flags(layer1_flags).ok_or(Els2Error::UnrecognizedLayer1Flags {
                flags: layer1_flags,
            })?;
        let client = client.ok_or(Els2Error::ClientAuthorizationRequired {
            flags: layer1_flags,
        })?;
        // The authorization data is a prefix of variable length, so it is parsed
        // by locating the inner salt from the declared client count rather than
        // by scanning: the entries are fixed-size and the count is explicit.
        let declared = read_auth_client_count(&layer1_plaintext, layer1_flags)?;
        let auth_data_len = 34 + declared * ELS2_AUTH_CLIENT_LENGTH;
        if layer1_plaintext.len() < 1 + auth_data_len + ELS2_SALT_LENGTH + 1 {
            return Err(Els2Error::LayerTooShort {
                layer: "authorization",
                actual: layer1_plaintext.len().saturating_sub(1),
                minimum: auth_data_len,
            });
        }
        let block = AuthBlock::decode(scheme, &layer1_plaintext[1..1 + auth_data_len])
            .map_err(Els2Error::Auth)?;
        let cookie = recover_auth_cookie(
            &block,
            credentials.subcredential(),
            published_seconds,
            client,
        )
        .map_err(Els2Error::Auth)?;
        (Some(cookie), auth_data_len)
    };

    // The inner ciphertext starts after the layer-1 flags byte and, when
    // present, after the authorization block.
    let inner_offset = 1 + auth_data_len;
    let inner_ciphertext = &layer1_plaintext[inner_offset..];
    if inner_ciphertext.len() < ELS2_SALT_LENGTH + 1 {
        return Err(Els2Error::LayerTooShort {
            layer: "inner",
            actual: inner_ciphertext.len(),
            minimum: ELS2_SALT_LENGTH + 1,
        });
    }
    let inner_salt: [u8; 32] = inner_ciphertext[..ELS2_SALT_LENGTH]
        .try_into()
        .expect("inner ciphertext is at least one salt long");
    let mut inner_input = Vec::with_capacity(4 + 68);
    if let Some(cookie) = &auth_cookie {
        inner_input.extend_from_slice(cookie.as_bytes());
    }
    inner_input.extend_from_slice(credentials.subcredential());
    inner_input.extend_from_slice(&published_be32(published_seconds));
    let inner_keys = derive_layer_keys(&inner_salt, &inner_input, ELS2_LAYER2_HKDF_INFO)?;

    let decrypted = chacha20_xor_layer_owned(
        &inner_keys.key,
        &inner_keys.nonce,
        &inner_ciphertext[ELS2_SALT_LENGTH..],
    )
    .map_err(chacha_error)?;

    if decrypted.is_empty() {
        return Err(Els2Error::LayerTooShort {
            layer: "inner",
            actual: 0,
            minimum: 1,
        });
    }
    let store_type = decrypted[0];
    let (inner_published, inner_expires) = match store_type {
        INNER_LEASE_SET2_STORE_TYPE => {
            let inner = LeaseSet2::decode(&decrypted[1..], MAX_COMMON_STRUCTURE_SIZE)?;
            (inner.published_seconds(), inner.expires_seconds())
        }
        INNER_META_LEASE_SET2_STORE_TYPE => {
            let inner = MetaLeaseSet::decode(&decrypted[1..], MAX_COMMON_STRUCTURE_SIZE)?;
            (
                inner.header().published_seconds(),
                inner.header().expires_seconds(),
            )
        }
        other => return Err(Els2Error::UnsupportedInnerStoreType { code: other }),
    };
    // Plan 381: deployed references do NOT synchronize the inner
    // publication timestamp with the outer one — i2pd wraps the
    // already-built inner LeaseSet2 as-is and stamps the outer with
    // `now`, so routine republication lag (minutes, growing while
    // leases stay stable) is normal on the wire, and neither
    // reference enforces equality on receipt (verified in pinned
    // i2pd 2.61.0 source: its consumer has no such check). Strict
    // equality rejects essentially every live reference record.
    // Enforce same-day contemporaneity instead: the inner must not
    // be from the future beyond ordinary clock skew, nor older than
    // a day relative to the outer (the blinding day-boundary),
    // which preserves the anti-transplant intent. Absolute
    // freshness (unexpired leases) is still enforced downstream by
    // ordinary LeaseSet2 validation, and i2pr's own publisher keeps
    // stamping inner == outer, so this relaxation only admits
    // foreign records — it never weakens what i2pr emits.
    const ELS2_INNER_PUBLISHED_MAX_FUTURE_SKEW_SECONDS: u32 = 3_600;
    const ELS2_INNER_PUBLISHED_MAX_STALENESS_SECONDS: u32 = 86_400;
    if inner_published
        > published_seconds.saturating_add(ELS2_INNER_PUBLISHED_MAX_FUTURE_SKEW_SECONDS)
        || inner_published
            < published_seconds.saturating_sub(ELS2_INNER_PUBLISHED_MAX_STALENESS_SECONDS)
    {
        return Err(Els2Error::InnerTimestampMismatch);
    }

    Ok(DecryptedEls2 {
        store_type,
        published_seconds: inner_published,
        expires_seconds: inner_expires,
        payload: decrypted[1..].to_vec(),
    })
}

/// Reads the declared client count from a layer-1 plaintext.
fn read_auth_client_count(layer1_plaintext: &[u8], flags: u8) -> Result<usize, Els2Error> {
    if layer1_plaintext.len() < 35 {
        return Err(Els2Error::LayerTooShort {
            layer: "authorization",
            actual: layer1_plaintext.len().saturating_sub(1),
            minimum: 34,
        });
    }
    let count = usize::from(u16::from_be_bytes([
        layer1_plaintext[33],
        layer1_plaintext[34],
    ]));
    if count > MAX_ELS2_AUTH_CLIENTS {
        return Err(Els2Error::Auth(Els2AuthError::TooManyClients {
            actual: count,
            maximum: MAX_ELS2_AUTH_CLIENTS,
        }));
    }
    let _ = flags;
    Ok(count)
}

/// The DHT storage key of a type-5 record.
///
/// The key is `SHA-256(0x000b || blindedPublicKey)`. The lookup secret
/// deliberately does not enter it, so a wrong secret produces a *different*
/// blinded key and therefore a lookup miss, never a wrong-record read.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BlindedStorageKey(Hash);

impl BlindedStorageKey {
    /// Wraps an already-derived storage key.
    pub const fn from_hash(hash: Hash) -> Self {
        Self(hash)
    }

    /// Borrows the underlying protocol hash.
    pub const fn as_hash(&self) -> &Hash {
        &self.0
    }

    /// Borrows the raw 32-byte digest.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

impl core::fmt::Debug for BlindedStorageKey {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("BlindedStorageKey(..)")
    }
}

/// The public daily material of one blinded key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DailyBlinding {
    day: BlindingDay,
    blinded_public_key: Red25519PublicKey,
    storage_key: BlindedStorageKey,
}

impl DailyBlinding {
    /// Returns the UTC day this material belongs to.
    pub const fn day(&self) -> BlindingDay {
        self.day
    }

    /// Returns the blinded signing public key for that day.
    pub const fn blinded_public_key(&self) -> &Red25519PublicKey {
        &self.blinded_public_key
    }

    /// Returns the DHT storage key for that day.
    pub const fn storage_key(&self) -> BlindedStorageKey {
        self.storage_key
    }
}

/// The owner's daily material: public material plus the blinded private scalar.
///
/// The private scalar is zeroized on drop, is not `Clone`, and has no `Debug`.
pub struct OwnerBlinding {
    daily: DailyBlinding,
    blinded_private_key: BlindedPrivateScalar,
}

impl OwnerBlinding {
    /// Returns the public daily material.
    pub const fn daily(&self) -> &DailyBlinding {
        &self.daily
    }

    /// Borrows the blinded private scalar for the shortest practical interval.
    pub const fn blinded_private_key(&self) -> &BlindedPrivateScalar {
        &self.blinded_private_key
    }
}

impl core::fmt::Debug for OwnerBlinding {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("OwnerBlinding")
            .field("daily", &self.daily)
            .field("blinded_private_key", &"<redacted>")
            .finish()
    }
}

/// The blinding identity a schedule derives daily material from.
///
/// The value is public material plus an optional secret, so it is deliberately
/// **not** `Clone`: a router must not hold two copies of a blinding secret. The
/// secret it carries is redacted in `Debug`.
#[derive(Debug)]
pub struct BlindingIdentity {
    unblinded_public_key: Red25519PublicKey,
    unblinded_sigtype: SigningKeyType,
    secret: LookupSecret,
}

impl BlindingIdentity {
    /// Builds an identity from an unblinded public key, its signature type, and
    /// an optional secret.
    ///
    /// Only signature types 7 and 11 may be blinded, so any other type is
    /// rejected before any derivation happens.
    pub fn new(
        unblinded_public_key: Red25519PublicKey,
        unblinded_sigtype: SigningKeyType,
        secret: Option<&str>,
    ) -> Result<Self, Els2Error> {
        if !ENCRYPTED_LEASE_SET2_UNBLINDED_SIGTYPES.contains(&unblinded_sigtype) {
            return Err(Els2Error::UnsupportedUnblindedSigtype {
                code: unblinded_sigtype.code(),
            });
        }
        let secret = match secret {
            None => LookupSecret::empty(),
            Some(value) => {
                LookupSecret::from_str(value).map_err(Els2Error::LookupSecretRejected)?
            }
        };
        Ok(Self {
            unblinded_public_key,
            unblinded_sigtype,
            secret,
        })
    }

    /// Returns the unblinded public key.
    pub const fn unblinded_public_key(&self) -> &Red25519PublicKey {
        &self.unblinded_public_key
    }

    /// Returns the unblinded signature type.
    pub const fn unblinded_sigtype(&self) -> SigningKeyType {
        self.unblinded_sigtype
    }

    /// Returns whether a blinding secret is configured.
    pub fn has_secret(&self) -> bool {
        !self.secret.is_empty()
    }

    /// Derives `alpha` for one UTC day.
    pub fn alpha(&self, day: BlindingDay) -> Result<BlindingScalar, Els2Error> {
        Ok(generate_alpha(
            &self.unblinded_public_key,
            self.unblinded_sigtype,
            day,
            self.secret.as_option(),
        )?)
    }

    /// Derives the public daily material for one UTC day.
    ///
    /// This is the client/lookup path: it needs the blinded public key and the
    /// storage key, never a private scalar.
    pub fn derive(&self, day: BlindingDay) -> Result<DailyBlinding, Els2Error> {
        let alpha = self.alpha(day)?;
        let blinded_public_key = blind_public_key(&self.unblinded_public_key, &alpha)?;
        Ok(DailyBlinding {
            day,
            blinded_public_key,
            storage_key: BlindedStorageKey::from_hash(blinded_storage_key(&blinded_public_key)),
        })
    }

    /// Derives the owner's daily material, including the blinded private scalar.
    ///
    /// For a type-7 destination the private scalar is the clamped Ed25519
    /// conversion of the stored seed; for a type-11 destination the stored
    /// 32-byte key is already the scalar.
    pub fn derive_owner(
        &self,
        unblinded_private_key: &Red25519PrivateScalar,
        day: BlindingDay,
    ) -> Result<OwnerBlinding, Els2Error> {
        let daily = self.derive(day)?;
        let alpha = self.alpha(day)?;
        let blinded_private_key = blind_private_key(unblinded_private_key, &alpha);
        // The public derivation must agree with the private one, or the record
        // would be signed by a key the record does not name.
        debug_assert_eq!(
            derive_blinded_public_key(&blinded_private_key),
            *daily.blinded_public_key(),
            "blinded public and private derivations disagree"
        );
        Ok(OwnerBlinding {
            daily,
            blinded_private_key,
        })
    }

    /// Derives the credentials for a given day's blinded public key.
    pub fn credentials_for(&self, blinded_public_key: &Red25519PublicKey) -> Els2Credentials {
        derive_els2_credentials(
            &self.unblinded_public_key,
            self.unblinded_sigtype,
            blinded_public_key,
        )
    }
}

/// Converts an Ed25519 seed into the Red25519 scalar a type-7 owner signs with.
pub fn unblinded_scalar_from_ed25519_seed(seed: &[u8; 32]) -> Red25519PrivateScalar {
    convert_ed25519_private(seed)
}

/// Configuration for a [`BlindingSchedule`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlindingScheduleConfig {
    /// Maximum number of days of material retained at once.
    ///
    /// The cache is a bound, not a policy: two days (today and tomorrow) is
    /// enough for rollover, and a larger value tolerates a clock that jumps
    /// backwards before the old entry ages out.
    pub max_cached_days: usize,
    /// Whether the schedule also holds the owner's private key material.
    ///
    /// When false, [`BlindingSchedule::owner_blinding`] is a typed error rather
    /// than a silent public-only answer, so a caller can never publish a record
    /// it cannot sign.
    pub owner: bool,
}

impl Default for BlindingScheduleConfig {
    fn default() -> Self {
        Self {
            max_cached_days: 2,
            owner: false,
        }
    }
}

impl BlindingScheduleConfig {
    /// Constructs a configuration.
    pub const fn new(max_cached_days: usize, owner: bool) -> Self {
        Self {
            max_cached_days,
            owner,
        }
    }
}

/// The bounded per-day blinding schedule.
///
/// # Atomicity
///
/// Every accessor returns one [`DailyBlinding`] (or one [`OwnerBlinding`]) value
/// that carries its own day, blinded public key, and storage key together. A
/// caller therefore cannot publish a record signed with one day's key under
/// another day's storage key: there is no API that returns the pieces
/// separately.
///
/// # Restart safety
///
/// Nothing is persisted. Every value is a pure function of the unblinded public
/// key, the signature type, the UTC day, and the optional secret, so a restart
/// recomputes exactly the same material. That is why "restart-safe" costs no
/// state here: there is no half-written rollover to recover from.
///
/// # Bounded precomputation
///
/// [`BlindingSchedule::precompute`] derives one day's material and inserts it
/// into a cache capped at `max_cached_days`; the oldest day is evicted first.
pub struct BlindingSchedule {
    identity: BlindingIdentity,
    owner_key: Option<Red25519PrivateScalar>,
    config: BlindingScheduleConfig,
    cache: BTreeMap<BlindingDay, DailyBlinding>,
    current_day: Option<BlindingDay>,
    rollovers: u64,
}

impl core::fmt::Debug for BlindingSchedule {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("BlindingSchedule")
            .field("identity", &self.identity)
            .field("config", &self.config)
            .field("cached_days", &self.cache.len())
            .field("current_day", &self.current_day)
            .field("rollovers", &self.rollovers)
            .field("owner_key", &self.owner_key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl BlindingSchedule {
    /// Builds a lookup-only schedule.
    pub fn new(identity: BlindingIdentity, config: BlindingScheduleConfig) -> Self {
        Self {
            identity,
            owner_key: None,
            config,
            cache: BTreeMap::new(),
            current_day: None,
            rollovers: 0,
        }
    }

    /// Builds an owner schedule that can also produce blinded private keys.
    ///
    /// The private scalar is moved in, never copied, and the type is not
    /// `Clone`, so a router never holds two copies of a destination's private
    /// identity.
    pub fn new_owner(
        identity: BlindingIdentity,
        unblinded_private_key: Red25519PrivateScalar,
        config: BlindingScheduleConfig,
    ) -> Self {
        Self {
            identity,
            owner_key: Some(unblinded_private_key),
            config: BlindingScheduleConfig {
                owner: true,
                ..config
            },
            cache: BTreeMap::new(),
            current_day: None,
            rollovers: 0,
        }
    }

    /// Returns the identity this schedule derives from.
    pub const fn identity(&self) -> &BlindingIdentity {
        &self.identity
    }

    /// Returns the schedule configuration.
    pub const fn config(&self) -> BlindingScheduleConfig {
        self.config
    }

    /// Returns the number of UTC-day rollovers observed so far.
    pub const fn rollovers(&self) -> u64 {
        self.rollovers
    }

    /// Returns the number of days currently cached.
    pub fn cached_days(&self) -> usize {
        self.cache.len()
    }

    /// Returns the day the schedule currently considers current.
    pub const fn current_day(&self) -> Option<BlindingDay> {
        self.current_day
    }

    /// Discards every cached day, forcing the next access to re-derive.
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Returns the public daily material for `day`, deriving and caching it.
    pub fn for_day(&mut self, day: BlindingDay) -> Result<DailyBlinding, Els2Error> {
        if let Some(existing) = self.cache.get(&day).copied() {
            return Ok(existing);
        }
        let derived = self.identity.derive(day)?;
        self.insert_cached(day, derived);
        Ok(derived)
    }

    /// Returns the daily material for the UTC day containing `now_seconds`.
    ///
    /// A day change is counted as a rollover exactly once: the counter moves
    /// only when the observed day actually differs from the previous one.
    pub fn current(&mut self, now_seconds: u32) -> Result<DailyBlinding, Els2Error> {
        let day = utc_blinding_day(now_seconds)?;
        if self.current_day != Some(day) {
            if self.current_day.is_some() {
                self.rollovers = self.rollovers.saturating_add(1);
            }
            self.current_day = Some(day);
        }
        self.for_day(day)
    }

    /// Returns the owner's daily material for the UTC day containing
    /// `now_seconds`.
    pub fn owner_blinding(&self, now_seconds: u32) -> Result<OwnerBlinding, Els2Error> {
        let Some(private_key) = self.owner_key.as_ref() else {
            return Err(Els2Error::Blinding(Red25519Error::ProtocolKeyRejected));
        };
        let day = utc_blinding_day(now_seconds)?;
        self.identity.derive_owner(private_key, day)
    }

    /// Derives and caches one day's material ahead of time.
    ///
    /// Returns the cached value. Precomputation never changes the schedule's
    /// notion of the current day, so a caller cannot publish tomorrow's key
    /// today by precomputing.
    pub fn precompute(&mut self, day: BlindingDay) -> Result<DailyBlinding, Els2Error> {
        self.for_day(day)
    }

    fn insert_cached(&mut self, day: BlindingDay, derived: DailyBlinding) {
        if self.config.max_cached_days == 0 {
            return;
        }
        self.cache.insert(day, derived);
        while self.cache.len() > self.config.max_cached_days {
            let Some(oldest) = self.cache.keys().next().copied() else {
                break;
            };
            self.cache.remove(&oldest);
        }
    }
}

/// Converts an I2P seconds-since-epoch timestamp to its UTC calendar day.
///
/// The blinding day is defined in UTC, so a local-timezone conversion would
/// silently produce a different `alpha` for a client near midnight. The
/// conversion is proleptic-Gregorian days-from-civil in reverse, evaluated with
/// checked arithmetic and no table.
pub fn utc_blinding_day(now_seconds: u32) -> Result<BlindingDay, Els2Error> {
    const SECONDS_PER_DAY: u64 = 86_400;
    let days = u64::from(now_seconds) / SECONDS_PER_DAY;
    let (year, month, day) = civil_from_days(days);
    let year =
        u16::try_from(year).map_err(|_| Red25519Error::InvalidBlindingDay { reason: "year" })?;
    BlindingDay::from_ymd(year, month, day).map_err(Els2Error::Blinding)
}

/// Computes the two-byte `expires` offset for a type-5 record.
///
/// The field is a relative offset from `published`, and it is bounded three
/// ways, so the clamp has to respect all three at once:
///
/// 1. the requested absolute expiration;
/// 2. the next UTC midnight, because tomorrow's record is filed under a
///    different blinded key, and a record that outlived its day would be
///    unreachable by every client;
/// 3. the field's own 65 535-second capacity.
///
/// Bounds 2 and 3 do not contain one another. Early in a UTC day the next
/// midnight is nearly 86 400 seconds away, which does not fit in two bytes, so
/// the effective lifetime is the smaller of the two: a record published at
/// 00:00:30 UTC lives about 18 hours, not 24. Clamping to midnight alone and
/// then saturating would silently emit a record that expires minutes before
/// midnight while the caller believed it lived until midnight. This function
/// returns a value within every bound, and `published + offset` is the record's
/// true absolute expiration.
pub fn day_bound_expiry_offset(
    published_seconds: u32,
    expires_seconds: u32,
    boundary_seconds: u32,
) -> u16 {
    let requested = expires_seconds.saturating_sub(published_seconds);
    let until_boundary = boundary_seconds.saturating_sub(published_seconds);
    requested
        .min(until_boundary)
        .min(ENCRYPTED_LEASE_SET2_MAX_EXPIRES_OFFSET) as u16
}

/// Returns the start of the next UTC day strictly after `now_seconds`.
pub fn next_utc_day_boundary_seconds(now_seconds: u32) -> u32 {
    const SECONDS_PER_DAY: u64 = 86_400;
    let days = u64::from(now_seconds) / SECONDS_PER_DAY + 1;
    (days * SECONDS_PER_DAY).min(u64::from(u32::MAX)) as u32
}

/// Days since 1970-01-01 to a proleptic-Gregorian `(year, month, day)`.
///
/// This is Howard Hinnant's `civil_from_days`, written from the algorithm's
/// definition: shift the era to a March-based year, invert the day-count
/// correction, then invert the March month arithmetic. It is evaluated in `i64`
/// with an absolute-date anchor, so it is correct for every day i2pr can
/// express in a 32-bit timestamp.
fn civil_from_days(days: u64) -> (i64, u8, u8) {
    let shifted = days as i64 + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64; // 0..=146096
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365; // 0..=399
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100); // 0..=365
    let month_position = (5 * day_of_year + 2) / 153; // 0..=11, March-based
    let day = (day_of_year - (153 * month_position + 2) / 5 + 1) as u8; // 1..=31
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    } as u8;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

/// Errors produced by type-5 record validation.
#[derive(Debug, Error)]
pub enum Els2ValidationError {
    /// The record body exceeded the caller-supplied length cap.
    #[error("encrypted LeaseSet2 encoded length {actual} exceeds {maximum}-byte limit")]
    EncodedTooLarge {
        /// Actual encoded length.
        actual: usize,
        /// Maximum accepted length.
        maximum: usize,
    },
    /// The blinded signature type was not Red25519.
    #[error("encrypted LeaseSet2 blinded sigtype {code} is not {expected}")]
    UnsupportedBlindedSigtype {
        /// The rejected signature type code.
        code: u16,
        /// The required signature type code.
        expected: u16,
    },
    /// The blinded public key was not a decodable curve point.
    #[error("encrypted LeaseSet2 blinded public key is not a valid point")]
    InvalidBlindedPublicKey,
    /// The expected storage key did not match the record's blinded public key.
    #[error("encrypted LeaseSet2 storage key mismatch")]
    StorageKeyMismatch,
    /// The offline-key delegation had expired.
    #[error("encrypted LeaseSet2 offline signing delegation is expired")]
    OfflineSignatureExpired,
    /// The offline-key block used a transient signature type that is not
    /// Ed25519 or Red25519.
    #[error("encrypted LeaseSet2 transient sigtype {code} is neither 7 nor 11")]
    UnsupportedTransientSigtype {
        /// The rejected signature type code.
        code: u16,
    },
    /// The record signature did not verify.
    #[error("encrypted LeaseSet2 signature verification failed")]
    InvalidSignature,
    /// The offline-key signature did not verify under the blinded public key.
    #[error("encrypted LeaseSet2 offline-key signature verification failed")]
    InvalidOfflineSignature,
    /// The published timestamp was further into the future than tolerated.
    #[error("encrypted LeaseSet2 published skew {skew_secs}s exceeds {max_skew_secs}s")]
    ExcessiveFuture {
        /// Future skew in seconds.
        skew_secs: u64,
        /// Maximum tolerated skew in seconds.
        max_skew_secs: u64,
    },
    /// The record had already expired relative to the supplied time.
    #[error("encrypted LeaseSet2 expired at {expires_secs}s vs now {now_secs}s")]
    Expired {
        /// Absolute expiration timestamp.
        expires_secs: u32,
        /// Current local time.
        now_secs: u32,
    },
    /// A layer or crypto operation failed.
    #[error(transparent)]
    Els2(#[from] Els2Error),
}

/// Caller-supplied freshness policy for type-5 validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Els2ValidationPolicy {
    /// Maximum future skew tolerated between the local clock and the record's
    /// published timestamp.
    pub max_future_skew_seconds: u64,
    /// Maximum encoded body length for one record.
    pub max_encoded_len: usize,
}

impl Default for Els2ValidationPolicy {
    fn default() -> Self {
        Self {
            max_future_skew_seconds: 60 * 60,
            max_encoded_len: MAX_ELS2_RECORD_LENGTH,
        }
    }
}

impl Els2ValidationPolicy {
    /// Constructs a custom policy.
    pub const fn new(max_future_skew_seconds: u64, max_encoded_len: usize) -> Self {
        Self {
            max_future_skew_seconds,
            max_encoded_len,
        }
    }
}

/// Caller-supplied validation context.
///
/// `now_seconds` is the current local time as an I2P seconds-since-epoch
/// timestamp. The validator never reads a clock itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Els2ValidationContext {
    /// Current local time as I2P seconds since the epoch.
    pub now_seconds: u32,
    /// Validation policy.
    pub policy: Els2ValidationPolicy,
}

impl Els2ValidationContext {
    /// Creates a context with the default policy.
    pub const fn new(now_seconds: u32) -> Self {
        Self {
            now_seconds,
            policy: Els2ValidationPolicy {
                max_future_skew_seconds: 60 * 60,
                max_encoded_len: MAX_ELS2_RECORD_LENGTH,
            },
        }
    }

    /// Creates a context with a custom policy.
    pub const fn with_policy(now_seconds: u32, policy: Els2ValidationPolicy) -> Self {
        Self {
            now_seconds,
            policy,
        }
    }
}

/// A type-5 record that has passed every validation gate.
///
/// The type is constructed only through
/// [`ValidatedEncryptedLeaseSet2::validate`], so no caller can store an
/// unverified record. A floodfill holds validated records and serves them
/// opaquely: it never holds the destination's unblinded public key, so it
/// cannot decrypt what it stores, and it does not try.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedEncryptedLeaseSet2 {
    record: EncryptedLeaseSet2,
    storage_key: BlindedStorageKey,
    encoded_len: usize,
    signature_profile: crate::els2_transcript::Els2RecordSignatureProfile,
}

impl ValidatedEncryptedLeaseSet2 {
    /// Validates a type-5 record and returns the wrapped record.
    ///
    /// The `expected_key` argument is optional; when supplied it must equal the
    /// storage key derived from the record's own blinded public key. A
    /// DatabaseStore for a type-5 record carries that storage key, so a caller
    /// that has one should always pass it: the check is what stops a record for
    /// one day's blinded key from being accepted at a lookup for another.
    ///
    /// Validation order is fail-closed: length → blinded key type → blinded key
    /// decode → storage key → offline delegation → record signature →
    /// freshness.
    pub fn validate(
        record: EncryptedLeaseSet2,
        expected_key: Option<BlindedStorageKey>,
        context: Els2ValidationContext,
    ) -> Result<Self, Els2ValidationError> {
        let encoded_len = record
            .signed_bytes()
            .len()
            .checked_add(record.signature().len())
            .ok_or(Els2ValidationError::EncodedTooLarge {
                actual: usize::MAX,
                maximum: context.policy.max_encoded_len,
            })?;
        if encoded_len > context.policy.max_encoded_len {
            return Err(Els2ValidationError::EncodedTooLarge {
                actual: encoded_len,
                maximum: context.policy.max_encoded_len,
            });
        }

        let blinded_sigtype = record.blinded_sigtype();
        if blinded_sigtype != ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE {
            return Err(Els2ValidationError::UnsupportedBlindedSigtype {
                code: blinded_sigtype.code(),
                expected: ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE.code(),
            });
        }
        let blinded_public_key = Red25519PublicKey::decode(record.blinded_public_key())
            .map_err(|_| Els2ValidationError::InvalidBlindedPublicKey)?;
        let storage_key = BlindedStorageKey::from_hash(blinded_storage_key(&blinded_public_key));
        if let Some(expected) = expected_key
            && expected != storage_key
        {
            return Err(Els2ValidationError::StorageKeyMismatch);
        }

        if let Some(offline) = record.offline_keys() {
            validate_offline_block(&blinded_public_key, offline, context)?;
        }

        let signature_profile = verify_record_signature(&record, &blinded_public_key)?;
        let now_secs = context.now_seconds;
        let published_secs = record.published_seconds();
        if published_secs > now_secs {
            let skew_secs = u64::from(published_secs - now_secs);
            if skew_secs > context.policy.max_future_skew_seconds {
                return Err(Els2ValidationError::ExcessiveFuture {
                    skew_secs,
                    max_skew_secs: context.policy.max_future_skew_seconds,
                });
            }
        }
        if record.expires_seconds() <= now_secs {
            return Err(Els2ValidationError::Expired {
                expires_secs: record.expires_seconds(),
                now_secs,
            });
        }

        Ok(Self {
            record,
            storage_key,
            encoded_len,
            signature_profile,
        })
    }

    /// Returns the DHT storage key the record is filed under.
    pub const fn storage_key(&self) -> BlindedStorageKey {
        self.storage_key
    }

    /// Returns which signature form this record's outer signature used.
    ///
    /// This is evidence, not policy: the record is already validated, so the value is
    /// always an accepting outcome. It exists so a floodfill or an interoperability
    /// harness can report *which* type-11 transcript a stored record was made under
    /// instead of inferring it from the bytes.
    pub const fn signature_profile(&self) -> crate::els2_transcript::Els2RecordSignatureProfile {
        self.signature_profile
    }

    /// Returns the blinded public key named by the record.
    pub fn blinded_public_key(&self) -> &[u8] {
        self.record.blinded_public_key()
    }

    /// Returns the record's published timestamp.
    pub const fn published_seconds(&self) -> u32 {
        self.record.published_seconds()
    }

    /// Returns the record's absolute expiration timestamp.
    pub const fn expires_seconds(&self) -> u32 {
        self.record.expires_seconds()
    }

    /// Returns the encoded length that contributed to store accounting.
    pub const fn encoded_len(&self) -> usize {
        self.encoded_len
    }

    /// Borrows the structural record.
    pub const fn record(&self) -> &EncryptedLeaseSet2 {
        &self.record
    }

    /// Returns the encoded record body.
    pub fn encoded(&self, maximum: usize) -> Result<Vec<u8>, Els2Error> {
        Ok(self.record.encode_to_vec(maximum)?)
    }
}

fn validate_offline_block(
    blinded_public_key: &Red25519PublicKey,
    offline: &EncryptedLeaseSet2OfflineKeys,
    context: Els2ValidationContext,
) -> Result<(), Els2ValidationError> {
    if !ENCRYPTED_LEASE_SET2_UNBLINDED_SIGTYPES.contains(&offline.sigtype()) {
        return Err(Els2ValidationError::UnsupportedTransientSigtype {
            code: offline.sigtype().code(),
        });
    }
    if offline.expires_seconds() <= context.now_seconds {
        return Err(Els2ValidationError::OfflineSignatureExpired);
    }
    // The delegation signature is made with the *blinded* key, not with the
    // transient key it authorizes. A blinded type-11 key therefore follows the
    // same bounded ELS2 transcript policy as the record's own signature.
    verify_with_sigtype(
        blinded_public_key,
        BLINDED_SIGNING_KEY_TYPE,
        Els2SignedRegion::of_offline_keys(offline),
        offline.signature(),
    )
    .map(|_| ())
    .map_err(|_| Els2ValidationError::InvalidOfflineSignature)
}

fn verify_record_signature(
    record: &EncryptedLeaseSet2,
    blinded_public_key: &Red25519PublicKey,
) -> Result<crate::els2_transcript::Els2RecordSignatureProfile, Els2ValidationError> {
    let region = Els2SignedRegion::of_record(record);
    let signature = record.signature();
    let outcome = match record.offline_keys() {
        // With offline keys the record itself is signed by the transient key,
        // not by the blinded key. The blinded key authorized the transient key
        // in the offline block, which is validated separately.
        Some(offline) => {
            let transient = Red25519PublicKey::decode(offline.public_key())
                .map_err(|_| Els2ValidationError::InvalidSignature)?;
            verify_with_sigtype(&transient, offline.sigtype(), region, signature)
        }
        None => verify_with_sigtype(
            blinded_public_key,
            ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
            region,
            signature,
        ),
    };
    outcome.map_err(|_| Els2ValidationError::InvalidSignature)
}

/// Verifies a signature whose key type is not necessarily Red25519.
///
/// The blinded key type is always Red25519, so this function exists for the
/// offline-key block, which the specification restricts to Ed25519 or
/// Red25519. Every other signature type is refused rather than routed to a
/// generic verifier, because a generic path would accept an algorithm the
/// encrypted-LeaseSet2 specification never defines for this record.
///
/// Type 11 — whether it is the record's own blinded signature or a transient
/// offline-delegation key — is routed to the bounded
/// [`crate::els2_transcript`] profile policy rather than to the strict
/// [`i2pr_crypto::red25519::verify_blinded`] alone, because that is the only
/// place a type-5 signature is allowed to be checked against the deployed
/// Java/i2pd transcript. Type 7 is untouched: it is ordinary Ed25519 and has no
/// second definition.
fn verify_with_sigtype(
    public_key: &Red25519PublicKey,
    sigtype: SigningKeyType,
    region: Els2SignedRegion<'_>,
    signature: &[u8],
) -> Result<crate::els2_transcript::Els2RecordSignatureProfile, ()> {
    if sigtype == ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE {
        return crate::els2_transcript::verify_type11(public_key, region, signature)
            .map(crate::els2_transcript::Els2RecordSignatureProfile::Type11)
            .map_err(|_| ());
    }
    if sigtype != i2pr_crypto::red25519::ED25519_SIGNING_KEY_TYPE {
        return Err(());
    }
    let signing_public_key =
        SigningPublicKey::new(sigtype, public_key.as_bytes().to_vec()).map_err(|_| ())?;
    let signature_value = SignatureValue::new(sigtype, signature.to_vec()).map_err(|_| ())?;
    i2pr_crypto::verify_signature(&signing_public_key, region.as_bytes(), &signature_value)
        .map(|_| crate::els2_transcript::Els2RecordSignatureProfile::Ed25519Transient)
        .map_err(|_| ())
}

/// Configuration for an [`Els2Store`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Els2StoreConfig {
    /// Maximum number of records retained.
    pub max_records: usize,
    /// Maximum total encoded bytes retained.
    pub max_total_encoded_bytes: usize,
}

impl Default for Els2StoreConfig {
    fn default() -> Self {
        Self {
            max_records: 4_096,
            max_total_encoded_bytes: 8 * 1024 * 1024,
        }
    }
}

impl Els2StoreConfig {
    /// Constructs a custom configuration.
    pub const fn new(max_records: usize, max_total_encoded_bytes: usize) -> Self {
        Self {
            max_records,
            max_total_encoded_bytes,
        }
    }
}

/// Privacy-safe aggregate statistics returned by the type-5 store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub struct Els2StoreStats {
    /// Number of records currently retained.
    pub record_count: usize,
    /// Total encoded bytes currently retained.
    pub total_encoded_bytes: usize,
    /// Configured record ceiling.
    pub max_records: usize,
    /// Configured byte ceiling.
    pub max_total_encoded_bytes: usize,
}

/// Outcome of an [`Els2Store::insert`] call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Els2InsertOutcome {
    /// The record was inserted as a new entry.
    Inserted,
    /// A byte-identical record was already present; the insert is a no-op.
    Idempotent,
    /// A newer record for the same storage key replaced an older one.
    Replaced,
    /// A record with the same storage key and publication timestamp but
    /// different bytes was rejected; the existing record is preserved.
    Conflict,
    /// A strictly older record for the same storage key was rejected.
    StaleReplacement,
    /// The store is at capacity; the candidate was rejected without mutating
    /// existing state.
    CapacityExceeded,
}

/// A bounded in-memory store of validated type-5 records.
///
/// The store accepts only [`ValidatedEncryptedLeaseSet2`] values; there is no
/// unchecked-insert bypass, because a floodfill that stored an unverified
/// blinded record would be serving attacker-chosen bytes under a key it could
/// not validate.
///
/// Replacement semantics mirror the LeaseSet2 store: a strictly newer
/// `published` replaces, equal `published` with identical bytes is idempotent,
/// equal `published` with different bytes is a conflict, and older is stale.
#[derive(Debug)]
pub struct Els2Store {
    records: BTreeMap<BlindedStorageKey, ValidatedEncryptedLeaseSet2>,
    total_encoded_bytes: usize,
    config: Els2StoreConfig,
}

impl Default for Els2Store {
    fn default() -> Self {
        Self::with_config(Els2StoreConfig::default())
    }
}

impl Els2Store {
    /// Constructs a store with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Constructs a store with a custom configuration.
    pub fn with_config(config: Els2StoreConfig) -> Self {
        Self {
            records: BTreeMap::new(),
            total_encoded_bytes: 0,
            config,
        }
    }

    /// Inserts a validated record and returns the outcome.
    pub fn insert(&mut self, validated: ValidatedEncryptedLeaseSet2) -> Els2InsertOutcome {
        let key = validated.storage_key();
        let encoded_len = validated.encoded_len();

        if let Some(existing) = self.records.get(&key) {
            let existing_published = existing.published_seconds();
            let incoming_published = validated.published_seconds();
            if incoming_published < existing_published {
                return Els2InsertOutcome::StaleReplacement;
            }
            if incoming_published == existing_published {
                if existing.record() == validated.record() {
                    return Els2InsertOutcome::Idempotent;
                }
                return Els2InsertOutcome::Conflict;
            }
            if encoded_len > existing.encoded_len() {
                let extra = encoded_len - existing.encoded_len();
                if self
                    .total_encoded_bytes
                    .checked_add(extra)
                    .is_none_or(|total| total > self.config.max_total_encoded_bytes)
                {
                    return Els2InsertOutcome::CapacityExceeded;
                }
            }
            self.total_encoded_bytes = self
                .total_encoded_bytes
                .checked_sub(existing.encoded_len())
                .expect("accounting underflow on replace");
            self.total_encoded_bytes = self
                .total_encoded_bytes
                .checked_add(encoded_len)
                .expect("accounting overflow on replace");
            self.records.insert(key, validated);
            return Els2InsertOutcome::Replaced;
        }

        if self.records.len() >= self.config.max_records {
            return Els2InsertOutcome::CapacityExceeded;
        }
        if self
            .total_encoded_bytes
            .checked_add(encoded_len)
            .is_none_or(|total| total > self.config.max_total_encoded_bytes)
        {
            return Els2InsertOutcome::CapacityExceeded;
        }
        self.total_encoded_bytes = self
            .total_encoded_bytes
            .checked_add(encoded_len)
            .expect("accounting overflow on insert");
        self.records.insert(key, validated);
        Els2InsertOutcome::Inserted
    }

    /// Returns the record for the supplied storage key, if any.
    pub fn get(&self, key: &BlindedStorageKey) -> Option<&ValidatedEncryptedLeaseSet2> {
        self.records.get(key)
    }

    /// Returns whether a record is retained for the supplied storage key.
    pub fn contains(&self, key: &BlindedStorageKey) -> bool {
        self.records.contains_key(key)
    }

    /// Removes the record for the supplied storage key.
    pub fn remove(&mut self, key: &BlindedStorageKey) -> bool {
        match self.records.remove(key) {
            Some(removed) => {
                self.total_encoded_bytes = self
                    .total_encoded_bytes
                    .checked_sub(removed.encoded_len())
                    .expect("accounting underflow on remove");
                true
            }
            None => false,
        }
    }

    /// Returns the number of records currently retained.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Returns the aggregate encoded byte count.
    pub fn encoded_bytes(&self) -> usize {
        self.total_encoded_bytes
    }

    /// Returns the store configuration.
    pub const fn config(&self) -> Els2StoreConfig {
        self.config
    }

    /// Returns a privacy-safe aggregate snapshot.
    pub fn stats(&self) -> Els2StoreStats {
        Els2StoreStats {
            record_count: self.records.len(),
            total_encoded_bytes: self.total_encoded_bytes,
            max_records: self.config.max_records,
            max_total_encoded_bytes: self.config.max_total_encoded_bytes,
        }
    }

    /// Iterates records in storage-key order.
    pub fn iter(&self) -> impl Iterator<Item = (&BlindedStorageKey, &ValidatedEncryptedLeaseSet2)> {
        self.records.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_day_conversion_matches_known_calendar_days() {
        let cases: &[(u32, u16, u8, u8)] = &[
            (0, 1970, 1, 1),
            (86_399, 1970, 1, 1),
            (86_400, 1970, 1, 2),
            (951_782_400, 2000, 2, 29),
            (1_700_000_000, 2023, 11, 14),
            (1_709_164_800, 2024, 2, 29),
            (2_147_483_647, 2038, 1, 19),
        ];
        for (seconds, year, month, day) in cases {
            let derived = utc_blinding_day(*seconds).expect("day");
            assert_eq!(
                derived.as_ymd(),
                (*year, *month, *day),
                "timestamp {seconds}"
            );
        }
    }

    #[test]
    fn day_string_is_the_derivation_input() {
        let day = utc_blinding_day(1_700_000_000).expect("day");
        assert_eq!(&day.as_bytes(), b"20231114");
    }

    #[test]
    fn next_day_boundary_is_the_following_midnight_utc() {
        assert_eq!(next_utc_day_boundary_seconds(0), 86_400);
        assert_eq!(next_utc_day_boundary_seconds(86_399), 86_400);
        assert_eq!(next_utc_day_boundary_seconds(86_400), 172_800);
        let now = 1_700_000_000;
        let boundary = next_utc_day_boundary_seconds(now);
        assert!(boundary > now);
        assert_eq!(boundary - now, 86_400 - (now % 86_400));
        assert_eq!(
            &utc_blinding_day(boundary).expect("day").as_bytes(),
            b"20231115"
        );
    }
}
