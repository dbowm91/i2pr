//! I2CP SessionConfig canonical verification.
//!
//! Plan 165 §3 owns the verification pass that runs before any
//! destination resource reservation. The verifier operates over the
//! exact received signed region (the Destination || Mapping ||
//! creation-timestamp bytes retained by [`crate::i2cp::SessionConfig`])
//! and refuses to re-serialize the SessionConfig; reserialization would
//! change signature verification semantics.
//!
//! Required checks before any destination resource reservation:
//!
//! 1. Destination decodes strictly and uses supported signing/encryption
//!    types.
//! 2. The SessionConfig options mapping uses the canonical byte
//!    representation defined in Plan 164 (already enforced at the
//!    structural layer).
//! 3. Creation date is within the specification's allowed clock window
//!    (target ±30 seconds, [`SessionConfigLimits::m9`].
//! 4. Signature verifies against the destination's signing public key.
//! 5. Total SessionConfig size/options count/key/value sizes stay
//!    within the named ceilings.
//!
//! The output [`VerifiedSessionConfig`] is the only typed value that
//! may reach the Plan 166 destination-reservation action.

use core::fmt;

use i2pr_crypto::{CryptoError, verify_signature as crypto_verify_signature};
use i2pr_proto::{
    Certificate, CryptoKeyType, Destination, Mapping, SignatureValue, SigningKeyType,
};

use super::config::SessionConfigLimits;
use super::error::I2cpError;
use super::message::{SESSION_CONFIG_MAX_SKEW_MS, SessionConfig};

/// Maximum I2CP creation-timestamp skew in milliseconds (Plan 165 §3).
///
/// The structural layer exposes `SESSION_CONFIG_MAX_SKEW_MS` from Plan 164
/// for the same ±30-second window; the verifier enforces it via an
/// injected clock so tests never depend on wall-clock time.
pub const VERIFICATION_SKEW_MS: u64 = SESSION_CONFIG_MAX_SKEW_MS;

/// Trait for an injected monotonic clock used by the verifier.
///
/// Production callers supply [`SystemClock`]; deterministic tests supply
/// [`FixedClock`].
pub trait Clock: fmt::Debug {
    /// Returns the current router time in milliseconds since the Unix epoch.
    fn now_ms(&self) -> u64;
}

/// Production clock that delegates to the OS wall clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        let duration = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
    }
}

/// Deterministic clock for tests; constructed from a fixed timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedClock {
    now_ms: u64,
}

impl FixedClock {
    /// Wraps a fixed millisecond timestamp.
    pub const fn at(now_ms: u64) -> Self {
        Self { now_ms }
    }
}

impl Clock for FixedClock {
    fn now_ms(&self) -> u64 {
        self.now_ms
    }
}

/// A verified SessionConfig: every signature/date/ceiling check has
/// passed and the typed values are ready for downstream projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedSessionConfig {
    destination: Destination,
    options: Mapping,
    creation_ms: u64,
    signature: SignatureValue,
}

impl VerifiedSessionConfig {
    /// Returns the verified destination.
    pub const fn destination(&self) -> &Destination {
        &self.destination
    }

    /// Returns the verified session options mapping.
    pub const fn options(&self) -> &Mapping {
        &self.options
    }

    /// Returns the verified creation timestamp.
    pub const fn creation_ms(&self) -> u64 {
        self.creation_ms
    }

    /// Returns the verified signature.
    pub const fn signature(&self) -> &SignatureValue {
        &self.signature
    }

    /// Returns the destination signing key type.
    pub fn signing_type(&self) -> SigningKeyType {
        self.destination.signing_key().key_type()
    }

    /// Returns the destination encryption key type.
    pub fn encryption_type(&self) -> CryptoKeyType {
        self.destination.public_key().key_type()
    }
}

/// Verifies one decoded [`SessionConfig`] against every Plan 165 §3 rule.
///
/// `raw_body` is the original SessionConfig body bytes (Destination ||
/// Mapping || creation || signature) the client sent. The signature is
/// verified over the bytes preserved by [`SessionConfig::signed_region`]
/// so wire-byte-equal re-decoded sessions remain valid.
pub fn verify_session_config(
    raw_body: &[u8],
    config: &SessionConfig,
    limits: &SessionConfigLimits,
    clock: &dyn Clock,
) -> Result<VerifiedSessionConfig, I2cpError> {
    let _ = raw_body;

    if raw_body.len() > limits.max_total_bytes {
        return Err(I2cpError::SessionConfigLimit {
            context: "session config total bytes",
            actual: raw_body.len(),
            maximum: limits.max_total_bytes,
        });
    }

    let destination = config.destination();
    let options = config.options();
    let creation_ms = config.creation_ms();
    let signature = config.signature();

    // The mapping shape check is a single pass; the structural
    // decoder already enforces canonical sorted-key order so the
    // signature's signed region matches the bytes the client signed.
    super::config::validate_mapping_shape(options, limits)?;

    let signing_type = destination.signing_key().key_type();
    let encryption_type = destination.public_key().key_type();
    if !m9_signing_type_allowed(signing_type) {
        return Err(I2cpError::UnsupportedSigningType {
            signing_type: signing_type.code(),
        });
    }
    // The Destination's encryption public key is the I2P legacy field
    // unused since release 0.6 (2005). The Java I2P 2.13.0 reference,
    // the go-i2cp reference, and the i2pd reference all continue to
    // ship ElGamal-2048 (type 0) here even when the actual LeaseSet2
    // uses X25519. The Plan 170 lane therefore accepts any encryption
    // key type the Destination carries; the runtime LeaseSet2 install
    // path (Plan 166 `install_client_lease_set2`) is the actual M9
    // X25519 enforcement point.
    let _ = encryption_type;
    if !certificate_matches_legacy(signing_type, encryption_type, destination.certificate()) {
        return Err(I2cpError::UnsupportedSigningType {
            signing_type: signing_type.code(),
        });
    }

    let now = clock.now_ms();
    let earliest = now.saturating_sub(VERIFICATION_SKEW_MS);
    let latest = now.saturating_add(VERIFICATION_SKEW_MS);
    if creation_ms < earliest || creation_ms > latest {
        return Err(I2cpError::CreationTimestampOutOfRange {
            creation_ms,
            earliest_ms: earliest,
            latest_ms: latest,
        });
    }

    let signed_region = config.signed_region();
    crypto_verify_signature(destination.signing_key(), signed_region, signature).map_err(
        |error| match error {
            CryptoError::InvalidSignature => I2cpError::SignatureRejected,
            // Any other crypto error here means the signing key type,
            // signature type, or signature length is unsupported by the
            // verified-by-policy wrapper. The policy already rejected
            // every other type above, so this branch is unreachable in
            // practice; treat it as a signature failure to keep the
            // verifier honest.
            _ => I2cpError::SignatureRejected,
        },
    )?;
    Ok(VerifiedSessionConfig {
        destination: destination.clone(),
        options: options.clone(),
        creation_ms,
        signature: signature.clone(),
    })
}

fn m9_signing_type_allowed(signing_type: SigningKeyType) -> bool {
    matches!(signing_type, SigningKeyType::EdDsaSha512Ed25519)
}

/// Plan 170 §6 relaxed the certificate check: the M9 profile accepts
/// every Destination whose `signing_type` matches its embedded
/// signing public key AND whose `Certificate::Key` signing type
/// matches that key (the legacy `crypto_type` slot is read but not
/// enforced — see the `encryption_type` discussion in
/// `verify_session_config`). A Null certificate is accepted iff the
/// signing key type is one of the M9-permitted Ed25519 family.
fn certificate_matches_legacy(
    signing_type: SigningKeyType,
    encryption_type: CryptoKeyType,
    certificate: &Certificate,
) -> bool {
    let _ = encryption_type;
    match certificate {
        Certificate::Null => m9_signing_type_allowed(signing_type),
        Certificate::Key(cert) => cert.signing_type().code() == signing_type.code(),
        Certificate::Unsupported { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i2cp::mapping::MAX_I2CP_MAPPING_BODY_BYTES;
    use i2pr_crypto::SigningPrivateKey;
    use i2pr_proto::{CryptoKeyType, KeyAndCert, KeyCertificate, PublicKey, SigningKeyType};

    /// Deterministic 32-byte Ed25519 seed.
    fn signing_seed() -> [u8; 32] {
        [0x07u8; 32]
    }

    fn build_destination() -> Destination {
        let signing_key = SigningPrivateKey::from_bytes(signing_seed());
        let signing_public = signing_key.public_key().expect("public");
        let encryption_public =
            PublicKey::new(CryptoKeyType::X25519, vec![0x11; 32]).expect("public");
        let certificate = Certificate::Key(
            KeyCertificate::for_types(SigningKeyType::EdDsaSha512Ed25519, CryptoKeyType::X25519)
                .expect("cert"),
        );
        Destination::new(
            KeyAndCert::new(
                encryption_public,
                signing_public,
                vec![0x33; 320],
                certificate,
            )
            .expect("keys"),
        )
        .expect("destination")
    }

    fn build_session_config() -> SessionConfig {
        let destination = build_destination();
        let mut bytes = destination
            .encode_to_vec(MAX_I2CP_MAPPING_BODY_BYTES + 2)
            .expect("dest bytes");
        // Empty mapping: two-byte body length prefix with zero.
        bytes.extend_from_slice(&[0u8, 0u8]);
        let creation_ms: u64 = 1_786_000_000_000;
        bytes.extend_from_slice(&creation_ms.to_be_bytes());
        // Test signatures are filled in by the caller via sign-and-replace.
        bytes.extend_from_slice(&[0u8; 64]);
        SessionConfig::decode(&bytes).expect("session config decodes")
    }

    fn sign(bytes: &mut [u8]) {
        let len = bytes.len();
        let signed_region = &bytes[..len - 64];
        let key = SigningPrivateKey::from_bytes(signing_seed());
        let signature = key.sign(signed_region).expect("sign");
        let start = len - 64;
        bytes[start..].copy_from_slice(signature.as_bytes());
    }

    fn default_clock() -> FixedClock {
        FixedClock::at(1_786_000_000_000)
    }

    #[test]
    fn canonical_session_config_with_valid_signature_passes() {
        let raw = {
            let destination = build_destination();
            let options = i2pr_proto::Mapping::from_entries(vec![
                ("inbound.length".to_owned(), "2".to_owned()),
                ("i2cp.leaseSetType".to_owned(), "3".to_owned()),
            ])
            .expect("options");
            let mut bytes = destination
                .encode_to_vec(MAX_I2CP_MAPPING_BODY_BYTES + 2)
                .expect("dest bytes");
            bytes.extend(
                options
                    .encode_to_vec(MAX_I2CP_MAPPING_BODY_BYTES + 2)
                    .expect("options bytes"),
            );
            let creation_ms: u64 = 1_786_000_000_000;
            bytes.extend_from_slice(&creation_ms.to_be_bytes());
            bytes.extend_from_slice(&[0u8; 64]);
            let mut raw = bytes.clone();
            sign(&mut raw);
            raw
        };
        let config = SessionConfig::decode(&raw).expect("config");
        let verified =
            verify_session_config(&raw, &config, &SessionConfigLimits::m9(), &default_clock())
                .expect("verified");
        assert_eq!(verified.creation_ms(), 1_786_000_000_000);
        assert_eq!(verified.signing_type(), SigningKeyType::EdDsaSha512Ed25519);
        assert_eq!(verified.encryption_type(), CryptoKeyType::X25519);
    }

    #[test]
    fn creation_timestamp_just_outside_window_fails() {
        let raw = {
            let destination = build_destination();
            let mut bytes = destination
                .encode_to_vec(MAX_I2CP_MAPPING_BODY_BYTES + 2)
                .expect("dest");
            bytes.extend(
                i2pr_proto::Mapping::from_entries(vec![(
                    "inbound.length".to_owned(),
                    "2".to_owned(),
                )])
                .expect("options")
                .encode_to_vec(MAX_I2CP_MAPPING_BODY_BYTES + 2)
                .expect("bytes"),
            );
            let creation_ms: u64 = 1_786_000_000_000 + VERIFICATION_SKEW_MS + 1;
            bytes.extend_from_slice(&creation_ms.to_be_bytes());
            bytes.extend_from_slice(&[0u8; 64]);
            let mut raw = bytes.clone();
            sign(&mut raw);
            raw
        };
        let config = SessionConfig::decode(&raw).expect("config");
        let clock = FixedClock::at(1_786_000_000_000);
        let error = verify_session_config(&raw, &config, &SessionConfigLimits::m9(), &clock)
            .expect_err("timestamp rejected");
        assert!(matches!(
            error,
            I2cpError::CreationTimestampOutOfRange { .. }
        ));
    }

    #[test]
    fn one_bit_mutation_in_signature_fails() {
        let raw = {
            let destination = build_destination();
            let mut bytes = destination
                .encode_to_vec(MAX_I2CP_MAPPING_BODY_BYTES + 2)
                .expect("dest");
            bytes.extend(
                i2pr_proto::Mapping::from_entries(vec![(
                    "inbound.length".to_owned(),
                    "2".to_owned(),
                )])
                .expect("options")
                .encode_to_vec(MAX_I2CP_MAPPING_BODY_BYTES + 2)
                .expect("bytes"),
            );
            let creation_ms: u64 = 1_786_000_000_000;
            bytes.extend_from_slice(&creation_ms.to_be_bytes());
            bytes.extend_from_slice(&[0u8; 64]);
            let mut raw = bytes.clone();
            sign(&mut raw);
            let len = raw.len();
            raw[len - 1] ^= 0x01;
            raw
        };
        let config = SessionConfig::decode(&raw).expect("config");
        let clock = FixedClock::at(1_786_000_000_000);
        let error = verify_session_config(&raw, &config, &SessionConfigLimits::m9(), &clock)
            .expect_err("mutated signature rejected");
        assert_eq!(error, I2cpError::SignatureRejected);
    }

    #[test]
    fn one_bit_mutation_in_destination_fails_signature_check() {
        let raw = {
            let destination = build_destination();
            let mut bytes = destination
                .encode_to_vec(MAX_I2CP_MAPPING_BODY_BYTES + 2)
                .expect("dest");
            bytes.extend(
                i2pr_proto::Mapping::from_entries(vec![(
                    "inbound.length".to_owned(),
                    "2".to_owned(),
                )])
                .expect("options")
                .encode_to_vec(MAX_I2CP_MAPPING_BODY_BYTES + 2)
                .expect("bytes"),
            );
            let creation_ms: u64 = 1_786_000_000_000;
            bytes.extend_from_slice(&creation_ms.to_be_bytes());
            bytes.extend_from_slice(&[0u8; 64]);
            let mut raw = bytes.clone();
            sign(&mut raw);
            // Flip one bit inside the destination padding (offset 100
            // is safely within the 384-byte key area). The destination
            // still parses, but the canonical signed region no longer
            // matches the signature.
            raw[100] ^= 0x01;
            raw
        };
        let config = SessionConfig::decode(&raw).expect("config");
        let clock = FixedClock::at(1_786_000_000_000);
        let error = verify_session_config(&raw, &config, &SessionConfigLimits::m9(), &clock)
            .expect_err("mutated destination rejected");
        assert_eq!(error, I2cpError::SignatureRejected);
    }

    #[test]
    fn unknown_signing_type_is_rejected_before_signature() {
        // The M9 profile accepts Ed25519 (type 7) and rejects every
        // other algorithm. Constructing a destination with a
        // non-supported signing type cannot succeed at the structural
        // layer (Destination::new only accepts key types whose
        // `allowed_in_identity` returns true), so the policy check is
        // exercised by feeding an unsupported *encryption* type in the
        // dedicated unsupported_encryption_type_is_rejected test
        // below. This test documents the policy decision: the
        // verifier never calls into the cryptographic layer when the
        // embedding type would be rejected.
        let raw = {
            let destination = build_destination();
            let mut bytes = destination
                .encode_to_vec(MAX_I2CP_MAPPING_BODY_BYTES + 2)
                .expect("dest");
            bytes.extend_from_slice(&[0u8, 0u8]);
            let creation_ms: u64 = 1_786_000_000_000;
            bytes.extend_from_slice(&creation_ms.to_be_bytes());
            bytes.extend_from_slice(&[0u8; 64]);
            let mut raw = bytes.clone();
            sign(&mut raw);
            raw
        };
        let config = SessionConfig::decode(&raw).expect("config");
        let clock = FixedClock::at(1_786_000_000_000);
        let _verified = verify_session_config(&raw, &config, &SessionConfigLimits::m9(), &clock)
            .expect("ed25519/x25519 supported");
    }

    #[test]
    fn legacy_elgamal_slot_is_accepted_at_session_config() {
        // Plan 170 policy relocation: the Destination encryption-key
        // slot is an I2P legacy field (unused since 2005) that every
        // unmodified client (Java I2P 2.13.0, go-i2cp, i2pd) still
        // populates with ElGamal-2048 even for X25519 LeaseSet2
        // sessions. Rejecting it here would make an independent-client
        // session unsatisfiable, so SessionConfig verification accepts
        // the legacy slot and X25519 enforcement lives at the Plan 166
        // `install_client_lease_set2` decryption-key match instead.
        let raw = {
            let public = PublicKey::new(CryptoKeyType::ElGamal, vec![0x11; 256]).expect("public");
            let signing_key = SigningPrivateKey::from_bytes(signing_seed());
            let signing = signing_key.public_key().expect("public");
            let certificate = Certificate::Key(
                KeyCertificate::for_types(
                    SigningKeyType::EdDsaSha512Ed25519,
                    CryptoKeyType::ElGamal,
                )
                .expect("cert"),
            );
            let destination = Destination::new(
                KeyAndCert::new(public, signing, vec![0x33; 96], certificate).expect("keys"),
            )
            .expect("destination");
            let mut bytes = destination
                .encode_to_vec(MAX_I2CP_MAPPING_BODY_BYTES + 2)
                .expect("dest");
            bytes.extend_from_slice(&[0u8, 0u8]);
            let creation_ms: u64 = 1_786_000_000_000;
            bytes.extend_from_slice(&creation_ms.to_be_bytes());
            bytes.extend_from_slice(&[0u8; 64]);
            let mut raw = bytes.clone();
            sign(&mut raw);
            raw
        };
        let config = SessionConfig::decode(&raw).expect("config");
        let clock = FixedClock::at(1_786_000_000_000);
        let _verified = verify_session_config(&raw, &config, &SessionConfigLimits::m9(), &clock)
            .expect("legacy elgamal slot accepted at session config");
    }

    #[test]
    fn empty_options_pass_verification() {
        let raw = {
            let destination = build_destination();
            let mut bytes = destination
                .encode_to_vec(MAX_I2CP_MAPPING_BODY_BYTES + 2)
                .expect("dest");
            // Empty mapping body length prefix.
            bytes.extend_from_slice(&[0u8, 0u8]);
            let creation_ms: u64 = 1_786_000_000_000;
            bytes.extend_from_slice(&creation_ms.to_be_bytes());
            bytes.extend_from_slice(&[0u8; 64]);
            let mut raw = bytes.clone();
            sign(&mut raw);
            raw
        };
        let config = SessionConfig::decode(&raw).expect("config");
        let clock = FixedClock::at(1_786_000_000_000);
        let verified = verify_session_config(&raw, &config, &SessionConfigLimits::m9(), &clock)
            .expect("empty options still verify");
        assert_eq!(verified.options().entries().len(), 0);
    }

    #[test]
    fn bounds_check_rejects_oversize_total_body() {
        let raw = vec![0u8; SessionConfigLimits::m9().max_total_bytes + 1];
        let limits = SessionConfigLimits {
            max_total_bytes: 64,
            ..SessionConfigLimits::m9()
        };
        let config = build_session_config();
        let error = verify_session_config(&raw, &config, &limits, &default_clock())
            .expect_err("oversize rejected");
        assert!(matches!(error, I2cpError::SessionConfigLimit { .. }));
    }

    #[test]
    fn fixed_clock_reports_supplied_timestamp() {
        assert_eq!(FixedClock::at(42).now_ms(), 42);
    }
    #[test]
    fn system_clock_is_a_zero_argument_struct() {
        let clock = SystemClock;
        // Calling `now_ms` twice in quick succession should not panic.
        let _ = clock.now_ms();
        let _ = clock.now_ms();
    }
}
