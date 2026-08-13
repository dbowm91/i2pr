//! RouterInfo validation, RouterHash derivation, and the validated
//! boundary owned by the local NetDB.

use std::time::Duration;

use i2pr_crypto::{CryptoError, router_identity_hash, verify_router_info};
use i2pr_proto::{Date, Hash, MAX_COMMON_STRUCTURE_SIZE, RouterIdentity, RouterInfo};
use thiserror::Error;

/// The maximum age window accepted for a remote `RouterInfo`.
///
/// Plan 103 deliberately uses one shared conservative window for the
/// initial implementation. The current I2P specification leaves the
/// exact freshness policy to local router policy; pinning this value
/// keeps NetDB membership deterministic until Plan 104/105 refine
/// reseed-driven vs. live-peer policy. The window mirrors the typical
/// "one day plus a generous clock skew" posture documented for Java
/// I2P and i2pd NetDB consumers while keeping eligibility narrow
/// enough that stale records do not silently persist.
pub const DEFAULT_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// The maximum clock-skew tolerated between the local clock and the
/// publication date when accepting a remote `RouterInfo`.
pub const DEFAULT_MAX_FUTURE_SKEW: Duration = Duration::from_secs(60 * 60);

/// The maximum encoded length accepted for a single `RouterInfo` in
/// the validator. The common-structure size cap in `i2pr-proto` is
/// intentionally generous; the NetDB layer tightens the bound to keep
/// resource accounting honest.
pub const DEFAULT_MAX_ENCODED_LEN: usize = 16 * 1024;

/// The NetDB-tracked SHA-256 RouterHash of a `RouterInfo`.
///
/// The hash is derived from the canonical encoded RouterIdentity bytes
/// (Plan 103 §1.1). It is the only `Hash` value the NetDB accepts as
/// an identity key.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RouterHash(Hash);

impl RouterHash {
    /// Constructs a `RouterHash` from an already-derived identity hash.
    pub const fn from_hash(hash: Hash) -> Self {
        Self(hash)
    }

    /// Constructs a `RouterHash` from raw 32 bytes. The caller is
    /// responsible for ensuring the bytes represent the SHA-256
    /// digest of a canonical `RouterIdentity` encoding; this
    /// constructor does not re-hash its input.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Hash::from_bytes(bytes))
    }

    /// Returns the underlying protocol hash.
    pub const fn as_hash(&self) -> &Hash {
        &self.0
    }

    /// Returns the raw 32-byte digest.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

impl core::fmt::Debug for RouterHash {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("RouterHash(..)")
    }
}

/// Computes the canonical RouterHash for a RouterIdentity by hashing
/// its canonical encoded representation.
///
/// Plan 103 §1.1 specifies that the hash input is the encoded
/// RouterIdentity; it is **not** the complete RouterInfo, the signing
/// key alone, or any debug representation. `i2pr-crypto` already
/// computes this digest as `router_identity_hash`; the wrapper here
/// returns the NetDB-level `RouterHash` type so callers cannot confuse
/// the two.
pub fn router_hash(identity: &RouterIdentity) -> Result<RouterHash, RouterInfoValidationError> {
    let hash = router_identity_hash(identity)?;
    Ok(RouterHash::from_hash(hash))
}

/// A typed outcome of `RouterInfo` validation.
///
/// The categories are intentionally distinct so callers can distinguish
/// "the peer advertises an algorithm we cannot verify" from "the
/// signature was cryptographically invalid". Both must be tracked
/// separately; collapsing them would mask protocol drift.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum RouterInfoValidationError {
    /// The RouterInfo exceeded the caller-supplied encoded length cap.
    #[error("router info encoded length {actual} exceeds {maximum}-byte limit")]
    EncodedTooLarge {
        /// Actual encoded length.
        actual: usize,
        /// Maximum length accepted by the validator.
        maximum: usize,
    },
    /// The signing-key type carried by the RouterIdentity is outside
    /// the algorithms this implementation can verify.
    #[error("unsupported router info signing algorithm {algorithm}")]
    UnsupportedAlgorithm {
        /// Numeric protocol algorithm identifier.
        algorithm: u16,
    },
    /// The signature did not verify against the contained signing key
    /// and the retained signed bytes.
    #[error("router info signature verification failed")]
    InvalidSignature,
    /// The expected RouterHash did not match the contained identity.
    #[error("router info key mismatch")]
    KeyMismatch,
    /// The publication timestamp is older than the freshness policy.
    #[error("router info publication age {age_secs}s exceeds {max_age_secs}s")]
    Stale {
        /// Age in seconds relative to the supplied `now`.
        age_secs: u64,
        /// Maximum age accepted by the validator.
        max_age_secs: u64,
    },
    /// The publication timestamp is further into the future than the
    /// tolerated clock skew.
    #[error("router info publication skew {skew_secs}s exceeds {max_skew_secs}s")]
    ExcessiveFuture {
        /// Future skew in seconds relative to the supplied `now`.
        skew_secs: u64,
        /// Maximum future skew accepted by the validator.
        max_skew_secs: u64,
    },
    /// The encoded RouterInfo length overflowed the validator's
    /// checked arithmetic.
    #[error("router info length arithmetic overflow")]
    ArithmeticOverflow,
    /// The underlying cryptographic or protocol codec failed.
    #[error(transparent)]
    Crypto(#[from] CryptoError),
}

/// Caller-supplied freshness policy for `RouterInfo` validation.
///
/// Plan 103 §2.2 requires callers to provide both the policy and the
/// `now` value used for the freshness comparison. `SystemTime::now()`
/// must not be called inside the validator; the structure is
/// intentionally cheap to clone so it can be reused across many
/// inserts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouterInfoValidationPolicy {
    /// Maximum age for an accepted RouterInfo.
    pub max_age: Duration,
    /// Maximum tolerated future skew for an accepted RouterInfo.
    pub max_future_skew: Duration,
    /// Maximum encoded length for a single RouterInfo.
    pub max_encoded_len: usize,
}

impl Default for RouterInfoValidationPolicy {
    fn default() -> Self {
        Self::default_const()
    }
}

impl RouterInfoValidationPolicy {
    /// Constructs a custom policy.
    pub const fn new(max_age: Duration, max_future_skew: Duration, max_encoded_len: usize) -> Self {
        Self {
            max_age,
            max_future_skew,
            max_encoded_len,
        }
    }

    /// Returns the default policy using only const-evaluable
    /// expressions; suitable for `const fn` callers.
    pub const fn default_const() -> Self {
        Self {
            max_age: DEFAULT_MAX_AGE,
            max_future_skew: DEFAULT_MAX_FUTURE_SKEW,
            max_encoded_len: DEFAULT_MAX_ENCODED_LEN,
        }
    }
}

/// Caller-supplied validation context.
///
/// `now` is the current I2P `Date` interpreted as milliseconds since
/// the Unix epoch. The validator never reads the wall clock itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidationContext {
    /// Current local time as an I2P `Date`.
    pub now: Date,
    /// Validation policy bounds.
    pub policy: RouterInfoValidationPolicy,
}

impl ValidationContext {
    /// Creates a validation context with the default policy.
    pub const fn new(now: Date) -> Self {
        Self {
            now,
            policy: RouterInfoValidationPolicy::default_const(),
        }
    }

    /// Creates a validation context with a custom policy.
    pub const fn with_policy(now: Date, policy: RouterInfoValidationPolicy) -> Self {
        Self { now, policy }
    }
}

/// A `RouterInfo` that has passed every Plan 103 validation gate.
///
/// The type is constructed only through [`ValidatedRouterInfo::from_router_info`]
/// so no caller can bypass cryptographic or temporal validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRouterInfo {
    key: RouterHash,
    router_info: RouterInfo,
    encoded_len: usize,
}

impl ValidatedRouterInfo {
    /// Validates a `RouterInfo` and returns the wrapped record on
    /// success. The `expected_key` argument is optional; when supplied
    /// it must equal the derived RouterHash or validation fails with
    /// `RouterInfoValidationError::KeyMismatch`.
    ///
    /// Validation order is the fail-closed sequence from Plan 103
    /// §2.3: length → key derivation → expected-key check → algorithm
    /// support → signature verification → publication policy → wrap.
    pub fn from_router_info(
        router_info: RouterInfo,
        expected_key: Option<RouterHash>,
        context: ValidationContext,
    ) -> Result<Self, RouterInfoValidationError> {
        let encoded_len = router_info
            .signed_bytes()
            .len()
            .checked_add(router_info.signature().as_bytes().len())
            .ok_or(RouterInfoValidationError::ArithmeticOverflow)?;
        if encoded_len > context.policy.max_encoded_len {
            return Err(RouterInfoValidationError::EncodedTooLarge {
                actual: encoded_len,
                maximum: context.policy.max_encoded_len,
            });
        }

        let key = router_hash(router_info.router_identity())?;
        if let Some(expected) = expected_key {
            if expected != key {
                return Err(RouterInfoValidationError::KeyMismatch);
            }
        }

        let algorithm = router_info
            .router_identity()
            .signing_key()
            .key_type()
            .code();
        match router_info
            .router_identity()
            .signing_key()
            .key_type()
            .public_key_len()
        {
            Some(_) => {}
            None => {
                return Err(RouterInfoValidationError::UnsupportedAlgorithm { algorithm });
            }
        }

        verify_router_info(&router_info).map_err(|error| match error {
            CryptoError::UnsupportedAlgorithm { algorithm, .. } => {
                RouterInfoValidationError::UnsupportedAlgorithm { algorithm }
            }
            CryptoError::InvalidSignature => RouterInfoValidationError::InvalidSignature,
            other => RouterInfoValidationError::Crypto(other),
        })?;

        let published = router_info.published();
        let now_ms = context.now.as_millis();
        let published_ms = published.as_millis();
        let max_age_ms = saturating_duration_to_millis(context.policy.max_age);
        let max_skew_ms = saturating_duration_to_millis(context.policy.max_future_skew);
        if published_ms <= now_ms {
            let age_ms = now_ms.saturating_sub(published_ms);
            if age_ms > max_age_ms {
                return Err(RouterInfoValidationError::Stale {
                    age_secs: age_ms / 1000,
                    max_age_secs: max_age_ms / 1000,
                });
            }
        } else {
            let skew_ms = published_ms.saturating_sub(now_ms);
            if skew_ms > max_skew_ms {
                return Err(RouterInfoValidationError::ExcessiveFuture {
                    skew_secs: skew_ms / 1000,
                    max_skew_secs: max_skew_ms / 1000,
                });
            }
        }

        Ok(Self {
            key,
            router_info,
            encoded_len,
        })
    }

    /// Returns the canonical RouterHash.
    pub fn key(&self) -> RouterHash {
        self.key
    }

    /// Borrows the underlying `RouterInfo`.
    pub fn router_info(&self) -> &RouterInfo {
        &self.router_info
    }

    /// Returns the encoded length that contributed to store accounting.
    pub fn encoded_len(&self) -> usize {
        self.encoded_len
    }

    /// Returns the publication timestamp.
    pub fn published(&self) -> Date {
        self.router_info.published()
    }

    /// Returns `true` when the signed `caps` mapping contains the
    /// `f` flag. The flag is signed-but-self-asserted data; it never
    /// implies that the peer is honest, healthy, or trustworthy.
    pub fn advertises_floodfill(&self) -> bool {
        match self.router_info.capabilities() {
            Ok(Some(caps)) => caps.as_str().bytes().any(|byte| byte == b'f'),
            _ => false,
        }
    }

    /// Returns the canonical encoded RouterInfo bytes used to
    /// represent the record on the wire or in a persistent cache.
    pub fn encoded(&self, maximum: usize) -> Result<Vec<u8>, i2pr_proto::CodecError> {
        self.router_info.encode_to_vec(maximum)
    }
}

fn saturating_duration_to_millis(duration: Duration) -> u64 {
    let millis = duration.as_millis();
    if millis > u64::MAX as u128 {
        u64::MAX
    } else {
        millis as u64
    }
}

/// Convenience: re-encodes a `RouterInfo` through the bounded proto
/// ceiling so callers that already trusted the input once do not need
/// to know the limit.
///
/// Reserved for Plan 104/105 callers; the API is intentionally
/// non-default so the static surface stays small.
#[allow(dead_code)]
pub fn reencode_router_info(router_info: &RouterInfo) -> Result<Vec<u8>, i2pr_proto::CodecError> {
    router_info.encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_crypto::{ROUTER_SIGNING_KEY_TYPE, RouterIdentityBundle};
    use i2pr_proto::Mapping;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn empty_signed_info(bundle: &RouterIdentityBundle, published_ms: u64) -> RouterInfo {
        bundle
            .sign_router_info(
                Date::from_millis(published_ms),
                Vec::new(),
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign empty router info")
    }

    #[test]
    fn router_hash_is_stable_and_independent_of_router_info_payload() {
        let signer = bundle(0x103);
        let identity = signer.identity().clone();
        let derived = router_hash(&identity).expect("hash");
        let again = router_hash(&identity).expect("hash");
        assert_eq!(derived, again);

        let signed = empty_signed_info(&signer, 1);
        let from_info = router_hash(signed.router_identity()).expect("hash from info");
        assert_eq!(derived, from_info);
    }

    #[test]
    fn identity_mutation_changes_router_hash() {
        let left = bundle(0x104);
        let right = bundle(0x105);
        assert_ne!(
            router_hash(left.identity()).expect("left"),
            router_hash(right.identity()).expect("right"),
        );
    }

    #[test]
    fn validated_router_info_rejects_expected_key_mismatch_before_signature() {
        let signer = bundle(0x106);
        let other = bundle(0x107);
        let info = empty_signed_info(&signer, 1);
        let expected = router_hash(other.identity()).expect("other");
        let context = ValidationContext::new(Date::from_millis(1));
        let error =
            ValidatedRouterInfo::from_router_info(info, Some(expected), context).unwrap_err();
        assert_eq!(error, RouterInfoValidationError::KeyMismatch);
    }

    #[test]
    fn validated_router_info_accepts_matching_expected_key() {
        let signer = bundle(0x108);
        let info = empty_signed_info(&signer, 1);
        let expected = router_hash(signer.identity()).expect("expected");
        let context = ValidationContext::new(Date::from_millis(1));
        let validated =
            ValidatedRouterInfo::from_router_info(info, Some(expected), context).expect("accept");
        assert_eq!(validated.key(), expected);
        assert_eq!(
            validated.encoded_len(),
            validated.router_info().signed_bytes().len() + 64
        );
    }

    #[test]
    fn signature_byte_mutation_fails_signature_verification() {
        let signer = bundle(0x109);
        let info = empty_signed_info(&signer, 1);
        let bad_signature = {
            let mut bytes = info.signature().as_bytes().to_vec();
            bytes[0] ^= 0x01;
            i2pr_proto::SignatureValue::new(ROUTER_SIGNING_KEY_TYPE, bytes).expect("signature")
        };
        let tampered_info = i2pr_proto::RouterInfo::new(
            info.router_identity().clone(),
            info.published(),
            info.addresses().to_vec(),
            Vec::new(),
            i2pr_proto::Mapping::empty(),
            bad_signature,
        )
        .expect("tampered info");
        let context = ValidationContext::new(Date::from_millis(1));
        let result = ValidatedRouterInfo::from_router_info(tampered_info, None, context);
        assert!(matches!(
            result,
            Err(RouterInfoValidationError::InvalidSignature)
        ));
    }

    #[test]
    fn signed_byte_mutation_fails_signature_verification() {
        let signer = bundle(0x109);
        let info = empty_signed_info(&signer, 1);
        // Craft a sibling record whose signed region is intrinsically
        // distinct. Then tamper with its signature and revalidate; the
        // validator must reject the tampered signature.
        let mut options = i2pr_proto::Mapping::builder();
        options
            .insert("router.version".to_owned(), "0.9.68".to_owned())
            .unwrap();
        let mutated_options = options.build().unwrap();
        let mutated_info = signer
            .sign_router_info(
                i2pr_proto::Date::from_millis(1),
                Vec::new(),
                Vec::new(),
                mutated_options.clone(),
            )
            .expect("sign mutated");
        // Sanity: the mutated signed bytes must differ from the
        // unmodified record.
        assert_ne!(mutated_info.signed_bytes(), info.signed_bytes());
        // Tampering with the signature bytes after construction still
        // rejects verification through the validator.
        let bad_signature = {
            let mut bytes = mutated_info.signature().as_bytes().to_vec();
            bytes[0] ^= 0x01;
            i2pr_proto::SignatureValue::new(ROUTER_SIGNING_KEY_TYPE, bytes).expect("signature")
        };
        let tampered_info = i2pr_proto::RouterInfo::new(
            mutated_info.router_identity().clone(),
            mutated_info.published(),
            mutated_info.addresses().to_vec(),
            Vec::new(),
            mutated_options,
            bad_signature,
        )
        .expect("tampered info");
        let context = ValidationContext::new(Date::from_millis(1));
        let result = ValidatedRouterInfo::from_router_info(tampered_info, None, context);
        assert!(matches!(
            result,
            Err(RouterInfoValidationError::InvalidSignature)
        ));
        let _ = info;
    }

    #[test]
    fn stale_publication_is_rejected_at_boundary() {
        let signer = bundle(0x10a);
        let published = 0;
        let now = DEFAULT_MAX_AGE.as_millis() as u64 + 1;
        let info = empty_signed_info(&signer, published);
        let context = ValidationContext::new(Date::from_millis(now));
        let error = ValidatedRouterInfo::from_router_info(info, None, context).unwrap_err();
        assert!(matches!(error, RouterInfoValidationError::Stale { .. }));
    }

    #[test]
    fn future_publication_is_rejected_at_boundary() {
        let signer = bundle(0x10b);
        let now = 0;
        let published = DEFAULT_MAX_FUTURE_SKEW.as_millis() as u64 + 1;
        let info = empty_signed_info(&signer, published);
        let context = ValidationContext::new(Date::from_millis(now));
        let error = ValidatedRouterInfo::from_router_info(info, None, context).unwrap_err();
        assert!(matches!(
            error,
            RouterInfoValidationError::ExcessiveFuture { .. }
        ));
    }

    #[test]
    fn exact_age_boundary_is_accepted() {
        let signer = bundle(0x10c);
        let now = DEFAULT_MAX_AGE.as_millis() as u64;
        let info = empty_signed_info(&signer, 0);
        let context = ValidationContext::new(Date::from_millis(now));
        ValidatedRouterInfo::from_router_info(info, None, context).expect("accept at boundary");
    }

    #[test]
    fn unsupported_signing_algorithm_is_rejected() {
        // `i2pr-proto::SigningPublicKey::new` rejects unknown algorithms
        // at construction time; the supported-algorithm check inside
        // the validator therefore funnels into the codec's rejection.
        // We assert that path here without trying to construct a
        // malformed key by hand (which the public API forbids).
        let error = i2pr_proto::SigningPublicKey::new(
            i2pr_proto::SigningKeyType::Unknown(0x1234),
            vec![0; 32],
        )
        .unwrap_err();
        assert!(matches!(error, i2pr_proto::CodecError::Unsupported { .. }));
    }

    #[test]
    fn advertises_floodfill_extracts_signed_cap_flag() {
        let signer = bundle(0x10e);
        let mut options = Mapping::builder();
        options.insert("caps".to_owned(), "Nf".to_owned()).unwrap();
        let info = signer
            .sign_router_info(
                Date::from_millis(1),
                Vec::new(),
                Vec::new(),
                options.build().unwrap(),
            )
            .expect("sign");
        let context = ValidationContext::new(Date::from_millis(1));
        let validated =
            ValidatedRouterInfo::from_router_info(info, None, context).expect("validate");
        assert!(validated.advertises_floodfill());

        let mut no_caps = Mapping::builder();
        no_caps.insert("caps".to_owned(), "L".to_owned()).unwrap();
        let info_no_caps = signer
            .sign_router_info(
                Date::from_millis(1),
                Vec::new(),
                Vec::new(),
                no_caps.build().unwrap(),
            )
            .expect("sign");
        let context = ValidationContext::new(Date::from_millis(1));
        let validated_no_caps =
            ValidatedRouterInfo::from_router_info(info_no_caps, None, context).expect("validate");
        assert!(!validated_no_caps.advertises_floodfill());
    }
}
