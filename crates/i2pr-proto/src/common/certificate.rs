//! Certificate and key-certificate structural validation.

use super::*;

/// Certificate container kinds.
#[derive(Clone, Eq, PartialEq)]
pub enum Certificate {
    /// The three-byte null certificate.
    Null,
    /// A validated key certificate.
    Key(KeyCertificate),
    /// A bounded but unsupported/deferred certificate payload.
    Unsupported { type_code: u8, payload: Vec<u8> },
}

impl Certificate {
    /// Decodes a complete certificate.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    pub(super) fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        let type_code = cursor.read_u8()?;
        let payload_len = usize::from(cursor.read_u16()?);
        let payload = cursor.take(payload_len)?.to_vec();
        match type_code {
            0 if payload.is_empty() => Ok(Self::Null),
            0 => Err(invalid(0, "null certificate payload")),
            2 if payload.is_empty() => Ok(Self::Unsupported { type_code, payload }),
            3 if payload.len() == 40 || payload.len() == 72 => {
                Ok(Self::Unsupported { type_code, payload })
            }
            5 => Ok(Self::Key(KeyCertificate::decode_payload(&payload)?)),
            1 | 4 => Ok(Self::Unsupported { type_code, payload }),
            other => Ok(Self::Unsupported {
                type_code: other,
                payload,
            }),
        }
    }

    pub(super) fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        match self {
            Self::Null => {
                encoder.write_u8(0)?;
                encoder.write_u16(0)
            }
            Self::Key(key) => {
                let payload = key.encode_payload()?;
                encoder.write_u8(5)?;
                encoder.write_u16(
                    u16::try_from(payload.len()).map_err(|_| invalid(0, "key certificate"))?,
                )?;
                encoder.write_raw(&payload)
            }
            Self::Unsupported { type_code, payload } => {
                encoder.write_u8(*type_code)?;
                encoder.write_u16(
                    u16::try_from(payload.len()).map_err(|_| invalid(0, "certificate payload"))?,
                )?;
                encoder.write_raw(payload)
            }
        }
    }

    /// Encodes a complete certificate.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }
}

impl fmt::Debug for Certificate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => formatter.write_str("Certificate::Null"),
            Self::Key(key) => formatter
                .debug_tuple("Certificate::Key")
                .field(key)
                .finish(),
            Self::Unsupported { type_code, payload } => formatter
                .debug_struct("Certificate::Unsupported")
                .field("type_code", type_code)
                .field("length", &payload.len())
                .finish(),
        }
    }
}

/// A key certificate's signing and encryption algorithm identifiers.
#[derive(Clone, Eq, PartialEq)]
pub struct KeyCertificate {
    signing_type: SigningKeyType,
    crypto_type: CryptoKeyType,
    pub(super) excess_signing: Vec<u8>,
    pub(super) excess_crypto: Vec<u8>,
}

impl KeyCertificate {
    /// Creates a key certificate with explicitly supplied excess key bytes.
    pub fn new(
        signing_type: SigningKeyType,
        crypto_type: CryptoKeyType,
        excess_signing: Vec<u8>,
        excess_crypto: Vec<u8>,
    ) -> Result<Self, CodecError> {
        let signing_len = signing_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "signing key type", signing_type.code() as u64))?;
        let crypto_len = crypto_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "encryption key type", crypto_type.code() as u64))?;
        if excess_signing.len() != signing_len.saturating_sub(LEGACY_SIGNING_KEY_SIZE)
            || excess_crypto.len() != crypto_len.saturating_sub(LEGACY_PUBLIC_KEY_SIZE)
        {
            return Err(invalid(0, "key certificate excess data length"));
        }
        Ok(Self {
            signing_type,
            crypto_type,
            excess_signing,
            excess_crypto,
        })
    }

    /// Creates the canonical no-excess key certificate for the supplied types.
    pub fn for_types(
        signing_type: SigningKeyType,
        crypto_type: CryptoKeyType,
    ) -> Result<Self, CodecError> {
        let signing_extra = signing_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "signing key type", signing_type.code() as u64))?
            .saturating_sub(LEGACY_SIGNING_KEY_SIZE);
        let crypto_extra = crypto_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "encryption key type", crypto_type.code() as u64))?
            .saturating_sub(LEGACY_PUBLIC_KEY_SIZE);
        Self::new(
            signing_type,
            crypto_type,
            vec![0; signing_extra],
            vec![0; crypto_extra],
        )
    }

    /// Returns the signing public-key type.
    pub const fn signing_type(&self) -> SigningKeyType {
        self.signing_type
    }

    /// Returns the encryption public-key type.
    pub const fn crypto_type(&self) -> CryptoKeyType {
        self.crypto_type
    }

    pub(super) fn decode_payload(payload: &[u8]) -> Result<Self, CodecError> {
        let mut cursor = DecodeCursor::new(payload, payload.len())?;
        let signing_type = SigningKeyType::from_code(cursor.read_u16()?);
        let crypto_type = CryptoKeyType::from_code(cursor.read_u16()?);
        let signing_len = signing_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "signing key type", signing_type.code() as u64))?;
        let crypto_len = crypto_type
            .public_key_len()
            .ok_or_else(|| unsupported(2, "encryption key type", crypto_type.code() as u64))?;
        let signing_extra_len = signing_len.saturating_sub(LEGACY_SIGNING_KEY_SIZE);
        let crypto_extra_len = crypto_len.saturating_sub(LEGACY_PUBLIC_KEY_SIZE);
        let excess_signing = cursor.take(signing_extra_len)?.to_vec();
        let excess_crypto = cursor.take(crypto_extra_len)?.to_vec();
        cursor.finish()?;
        Self::new(signing_type, crypto_type, excess_signing, excess_crypto)
    }

    pub(super) fn encode_payload(&self) -> Result<Vec<u8>, CodecError> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&self.signing_type.code().to_be_bytes());
        payload.extend_from_slice(&self.crypto_type.code().to_be_bytes());
        payload.extend_from_slice(&self.excess_signing);
        payload.extend_from_slice(&self.excess_crypto);
        Ok(payload)
    }
}

impl fmt::Debug for KeyCertificate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KeyCertificate")
            .field("signing_type", &self.signing_type)
            .field("crypto_type", &self.crypto_type)
            .field("excess_signing_length", &self.excess_signing.len())
            .field("excess_crypto_length", &self.excess_crypto.len())
            .finish()
    }
}
