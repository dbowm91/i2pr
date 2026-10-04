//! Bounded, runtime-neutral SU3 framing and signature verification.
//!
//! Content-specific policy (such as reseed ZIP parsing or NEWS XML
//! handling) belongs to the consuming subsystem. This crate validates
//! the common SU3 envelope and verifies signatures against an explicit
//! caller-provided RSA key.

#![forbid(unsafe_code)]

use sad_rsa::pkcs1v15::{Signature, VerifyingKey};
use sad_rsa::sha2::Sha512;
use sad_rsa::signature::Verifier;
use thiserror::Error;

pub const DEFAULT_MAX_SU3_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SIGNER_ID_BYTES: usize = 256;
pub const MAX_VERSION_BYTES: usize = 64;
const HEADER_PREFIX: usize = 25;

/// Caller-owned ceilings applied before any content is exposed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Su3Limits {
    pub max_file_bytes: usize,
    pub max_content_bytes: usize,
    pub max_signer_id_bytes: usize,
    pub max_version_bytes: usize,
}

impl Default for Su3Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: DEFAULT_MAX_SU3_BYTES,
            max_content_bytes: DEFAULT_MAX_SU3_BYTES,
            max_signer_id_bytes: MAX_SIGNER_ID_BYTES,
            max_version_bytes: MAX_VERSION_BYTES,
        }
    }
}

/// Validated metadata and offsets for one complete SU3 container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Su3Header {
    pub signature_type: u16,
    pub signature_length: usize,
    pub content_length: usize,
    pub file_type: u8,
    pub content_type: u8,
    pub version: String,
    pub signer_id: String,
    content_offset: usize,
    signature_offset: usize,
    total_length: usize,
}

impl Su3Header {
    /// Returns the exact bytes covered by the SU3 signature.
    pub fn signed_bytes<'a>(&self, input: &'a [u8]) -> Result<&'a [u8], Su3Error> {
        input
            .get(..self.signature_offset)
            .ok_or(Su3Error::InputChanged)
    }

    /// Returns the content bytes after framing validation.
    pub fn content<'a>(&self, input: &'a [u8]) -> Result<&'a [u8], Su3Error> {
        input
            .get(self.content_offset..self.signature_offset)
            .ok_or(Su3Error::InputChanged)
    }

    /// Returns the signature bytes after framing validation.
    pub fn signature<'a>(&self, input: &'a [u8]) -> Result<&'a [u8], Su3Error> {
        input
            .get(self.signature_offset..self.total_length)
            .ok_or(Su3Error::InputChanged)
    }
}

/// Explicit RSA signer material. Certificate parsing and trust-anchor
/// policy are deliberately left to the caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RsaSha512Signer {
    pub signer_id: String,
    pub modulus: Vec<u8>,
    pub exponent: Vec<u8>,
    pub not_before: u64,
    pub not_after: u64,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum Su3Error {
    #[error("SU3 file exceeds configured byte limit")]
    FileTooLarge,
    #[error("SU3 magic does not match")]
    MagicMismatch,
    #[error("SU3 format version is unsupported")]
    UnsupportedFormatVersion,
    #[error("SU3 reserved bytes must be zero")]
    NonZeroReserved,
    #[error("SU3 input is truncated or has trailing bytes")]
    InvalidLength,
    #[error("SU3 content exceeds configured byte limit")]
    ContentTooLarge,
    #[error("SU3 signer identifier is invalid")]
    InvalidSignerId,
    #[error("SU3 version field is invalid")]
    InvalidVersion,
    #[error("SU3 signature algorithm {0} is unsupported")]
    UnsupportedSignatureType(u16),
    #[error("SU3 signature length does not match signer key")]
    SignatureLength,
    #[error("SU3 signer is not trusted")]
    UntrustedSigner,
    #[error("SU3 signer certificate is outside its validity interval")]
    ExpiredSigner,
    #[error("SU3 signature verification failed")]
    InvalidSignature,
    #[error("input no longer matches parsed SU3 framing")]
    InputChanged,
    #[error("trusted SU3 signer certificate is not valid DER X.509")]
    CertificateParse,
    #[error("trusted SU3 signer certificate uses unsupported public key algorithm {0}")]
    UnsupportedKeyType(String),
    #[error("trusted SU3 signer certificate has an invalid validity interval")]
    CertificateValidity,
}

/// Parses an operator-pinned DER X.509 certificate into explicit RSA
/// verification material. This does not build or validate a CA chain:
/// the caller's configured certificate is the trust anchor.
pub fn rsa_signer_from_certificate(
    signer_id: &str,
    certificate_der: &[u8],
) -> Result<RsaSha512Signer, Su3Error> {
    if signer_id.is_empty()
        || signer_id.len() > MAX_SIGNER_ID_BYTES
        || signer_id
            .bytes()
            .any(|byte| byte == 0 || byte.is_ascii_control())
    {
        return Err(Su3Error::InvalidSignerId);
    }
    use x509_parser::prelude::{FromDer, X509Certificate};
    use x509_parser::public_key::PublicKey;

    let (_, certificate) =
        X509Certificate::from_der(certificate_der).map_err(|_| Su3Error::CertificateParse)?;
    let algorithm_oid = certificate
        .tbs_certificate
        .subject_pki
        .algorithm
        .algorithm
        .to_id_string();
    let public_key = certificate
        .tbs_certificate
        .subject_pki
        .parsed()
        .map_err(|_| Su3Error::UnsupportedKeyType(algorithm_oid.clone()))?;
    let rsa = match public_key {
        PublicKey::RSA(key) => key,
        _ => return Err(Su3Error::UnsupportedKeyType(algorithm_oid)),
    };
    let not_before = u64::try_from(certificate.validity.not_before.timestamp())
        .map_err(|_| Su3Error::CertificateValidity)?;
    let not_after = u64::try_from(certificate.validity.not_after.timestamp())
        .map_err(|_| Su3Error::CertificateValidity)?;
    if not_after < not_before {
        return Err(Su3Error::CertificateValidity);
    }
    Ok(RsaSha512Signer {
        signer_id: signer_id.to_owned(),
        modulus: unsigned_big_endian(rsa.modulus),
        exponent: unsigned_big_endian(rsa.exponent),
        not_before,
        not_after,
    })
}

fn unsigned_big_endian(integer: &[u8]) -> Vec<u8> {
    let significant = integer
        .iter()
        .position(|byte| *byte != 0)
        .map(|offset| &integer[offset..])
        .unwrap_or(&integer[integer.len().saturating_sub(1)..]);
    significant.to_vec()
}

/// Validates SU3 framing without imposing a content/file type policy.
pub fn parse(input: &[u8], limits: Su3Limits) -> Result<Su3Header, Su3Error> {
    if input.len() > limits.max_file_bytes {
        return Err(Su3Error::FileTooLarge);
    }
    if input.len() < HEADER_PREFIX {
        return Err(Su3Error::InvalidLength);
    }
    if input.get(..6) != Some(b"I2Psu3") {
        return Err(Su3Error::MagicMismatch);
    }
    if input[6] != 1 {
        return Err(Su3Error::UnsupportedFormatVersion);
    }
    if input[7..10].iter().any(|byte| *byte != 0) || input[20..23].iter().any(|byte| *byte != 0) {
        return Err(Su3Error::NonZeroReserved);
    }
    let signature_type = u16::from_le_bytes([input[10], input[11]]);
    let signature_length = usize::from(u16::from_le_bytes([input[12], input[13]]));
    let content_length = usize::try_from(u32::from_le_bytes([
        input[14], input[15], input[16], input[17],
    ]))
    .map_err(|_| Su3Error::InvalidLength)?;
    if content_length > limits.max_content_bytes {
        return Err(Su3Error::ContentTooLarge);
    }
    let file_type = input[18];
    let content_type = input[19];
    let version_length = usize::from(u16::from_le_bytes([input[23], input[24]]));
    if version_length == 0 || version_length > limits.max_version_bytes {
        return Err(Su3Error::InvalidVersion);
    }
    let version_end = HEADER_PREFIX
        .checked_add(version_length)
        .ok_or(Su3Error::InvalidLength)?;
    let version_bytes = input
        .get(HEADER_PREFIX..version_end)
        .ok_or(Su3Error::InvalidLength)?;
    if !version_bytes.is_ascii()
        || version_bytes
            .iter()
            .any(|byte| *byte < 0x20 || *byte > 0x7e)
    {
        return Err(Su3Error::InvalidVersion);
    }
    let signer_length_bytes = input
        .get(version_end..version_end.checked_add(2).ok_or(Su3Error::InvalidLength)?)
        .ok_or(Su3Error::InvalidLength)?;
    let signer_length = usize::from(u16::from_le_bytes([
        signer_length_bytes[0],
        signer_length_bytes[1],
    ]));
    if signer_length == 0 || signer_length > limits.max_signer_id_bytes {
        return Err(Su3Error::InvalidSignerId);
    }
    let signer_start = version_end + 2;
    let content_offset = signer_start
        .checked_add(signer_length)
        .ok_or(Su3Error::InvalidLength)?;
    let signer_id = std::str::from_utf8(
        input
            .get(signer_start..content_offset)
            .ok_or(Su3Error::InvalidLength)?,
    )
    .map_err(|_| Su3Error::InvalidSignerId)?;
    if signer_id
        .bytes()
        .any(|byte| byte == 0 || byte.is_ascii_control())
    {
        return Err(Su3Error::InvalidSignerId);
    }
    let signature_offset = content_offset
        .checked_add(content_length)
        .ok_or(Su3Error::InvalidLength)?;
    let total_length = signature_offset
        .checked_add(signature_length)
        .ok_or(Su3Error::InvalidLength)?;
    if total_length != input.len() {
        return Err(Su3Error::InvalidLength);
    }
    Ok(Su3Header {
        signature_type,
        signature_length,
        content_length,
        file_type,
        content_type,
        version: std::str::from_utf8(version_bytes)
            .map_err(|_| Su3Error::InvalidVersion)?
            .to_owned(),
        signer_id: signer_id.to_owned(),
        content_offset,
        signature_offset,
        total_length,
    })
}

/// Verifies the currently supported SU3 RSA-SHA512 signature type (6)
/// over the exact header and content bytes.
pub fn verify_rsa_sha512(
    input: &[u8],
    header: &Su3Header,
    signer: &RsaSha512Signer,
    now_seconds: u64,
) -> Result<(), Su3Error> {
    let reparsed = parse(
        input,
        Su3Limits {
            max_file_bytes: input.len(),
            max_content_bytes: header.content_length,
            max_signer_id_bytes: header.signer_id.len(),
            max_version_bytes: header.version.len(),
        },
    )?;
    if &reparsed != header {
        return Err(Su3Error::InputChanged);
    }
    if header.signature_type != 6 {
        return Err(Su3Error::UnsupportedSignatureType(header.signature_type));
    }
    if signer.signer_id != header.signer_id {
        return Err(Su3Error::UntrustedSigner);
    }
    if now_seconds < signer.not_before || now_seconds > signer.not_after {
        return Err(Su3Error::ExpiredSigner);
    }
    let signature = header.signature(input)?;
    if signature.len() != signer.modulus.len() {
        return Err(Su3Error::SignatureLength);
    }
    let modulus_bits = u32::try_from(signer.modulus.len())
        .ok()
        .and_then(|length| length.checked_mul(8))
        .ok_or(Su3Error::InvalidSignature)?;
    let exponent_bits = u32::try_from(signer.exponent.len())
        .ok()
        .and_then(|length| length.checked_mul(8))
        .ok_or(Su3Error::InvalidSignature)?;
    let modulus = sad_rsa::BoxedUint::from_be_slice(&signer.modulus, modulus_bits)
        .map_err(|_| Su3Error::InvalidSignature)?;
    let exponent = sad_rsa::BoxedUint::from_be_slice(&signer.exponent, exponent_bits)
        .map_err(|_| Su3Error::InvalidSignature)?;
    let public_key =
        sad_rsa::RsaPublicKey::new(modulus, exponent).map_err(|_| Su3Error::InvalidSignature)?;
    let signature = Signature::try_from(signature).map_err(|_| Su3Error::InvalidSignature)?;
    VerifyingKey::<Sha512>::new(public_key)
        .verify(header.signed_bytes(input)?, &signature)
        .map_err(|_| Su3Error::InvalidSignature)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let version = b"20261004";
        let signer = b"trusted";
        let content = b"payload";
        let mut bytes = b"I2Psu3\x01\0\0\0".to_vec();
        bytes.extend_from_slice(&6_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&(content.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&[1, 4, 0, 0, 0]);
        bytes.extend_from_slice(&(version.len() as u16).to_le_bytes());
        bytes.extend_from_slice(version);
        bytes.extend_from_slice(&(signer.len() as u16).to_le_bytes());
        bytes.extend_from_slice(signer);
        bytes.extend_from_slice(content);
        bytes
    }

    #[test]
    fn validates_generic_framing_and_exposes_exact_content() {
        let bytes = fixture();
        let header = parse(&bytes, Su3Limits::default()).unwrap();
        assert_eq!(header.file_type, 1);
        assert_eq!(header.content_type, 4);
        assert_eq!(header.signer_id, "trusted");
        assert_eq!(header.content(&bytes).unwrap(), b"payload");
        assert_eq!(header.signed_bytes(&bytes).unwrap(), &bytes[..]);
        assert_eq!(header.signature(&bytes).unwrap(), b"");
    }

    #[test]
    fn rejects_content_over_limit_and_trailing_bytes() {
        let bytes = fixture();
        let limits = Su3Limits {
            max_content_bytes: 6,
            ..Su3Limits::default()
        };
        assert_eq!(parse(&bytes, limits), Err(Su3Error::ContentTooLarge));
        let mut trailing = bytes;
        trailing.push(0);
        assert_eq!(
            parse(&trailing, Su3Limits::default()),
            Err(Su3Error::InvalidLength)
        );
    }

    #[test]
    fn verifier_rejects_header_changed_after_framing_parse() {
        let mut bytes = fixture();
        let header = parse(&bytes, Su3Limits::default()).unwrap();
        bytes[18] = 2;
        let signer = RsaSha512Signer {
            signer_id: "trusted".to_owned(),
            modulus: vec![1; 256],
            exponent: vec![1],
            not_before: 0,
            not_after: u64::MAX,
        };
        assert_eq!(
            verify_rsa_sha512(&bytes, &header, &signer, 1),
            Err(Su3Error::InputChanged)
        );
    }
}
