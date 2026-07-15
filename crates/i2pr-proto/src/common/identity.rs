//! RouterIdentity, Destination, and key-and-certificate structures.

use super::*;

/// The 384-byte key area and certificate used by a RouterIdentity or Destination.
#[derive(Clone, Eq, PartialEq)]
pub struct KeyAndCert {
    public_key: PublicKey,
    signing_key: SigningPublicKey,
    padding: Vec<u8>,
    certificate: Certificate,
}

impl KeyAndCert {
    /// Creates validated key material. Padding is retained because it is part
    /// of the identity/destination hash input.
    pub fn new(
        public_key: PublicKey,
        signing_key: SigningPublicKey,
        padding: Vec<u8>,
        certificate: Certificate,
    ) -> Result<Self, CodecError> {
        let key = Self {
            public_key,
            signing_key,
            padding,
            certificate,
        };
        key.validate()?;
        Ok(key)
    }

    pub(super) fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        let key_area = cursor.take(KEY_AREA_SIZE)?;
        let certificate = Certificate::decode_from(cursor)?;
        let (public_type, signing_type) = match &certificate {
            Certificate::Null => (CryptoKeyType::ElGamal, SigningKeyType::DsaSha1),
            Certificate::Key(key) => (key.crypto_type(), key.signing_type()),
            Certificate::Unsupported { type_code, .. } => {
                return Err(unsupported(
                    384,
                    "identity certificate",
                    u64::from(*type_code),
                ));
            }
        };
        let public_len = public_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "encryption key type", public_type.code() as u64))?;
        let signing_len = signing_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "signing key type", signing_type.code() as u64))?;
        if public_len > LEGACY_PUBLIC_KEY_SIZE {
            return Err(unsupported(
                0,
                "identity encryption key layout",
                public_type.code() as u64,
            ));
        }
        let signing_prefix_len = signing_len.min(LEGACY_SIGNING_KEY_SIZE);
        let padding_len = KEY_AREA_SIZE
            .checked_sub(public_len + signing_prefix_len)
            .ok_or_else(|| invalid(0, "identity key area"))?;
        let public_bytes = key_area[..public_len].to_vec();
        let signing_start = KEY_AREA_SIZE - signing_prefix_len;
        let mut signing_bytes = key_area[signing_start..].to_vec();
        if let Certificate::Key(key) = &certificate {
            signing_bytes.extend_from_slice(&key.excess_signing);
            let mut full_public = public_bytes.clone();
            full_public.extend_from_slice(&key.excess_crypto);
            let public_key = PublicKey::new(public_type, full_public)?;
            let signing_key = SigningPublicKey::new(signing_type, signing_bytes)?;
            return Self::new(
                public_key,
                signing_key,
                key_area[public_len..public_len + padding_len].to_vec(),
                certificate,
            );
        }
        let public_key = PublicKey::new(public_type, public_bytes)?;
        let signing_key = SigningPublicKey::new(signing_type, signing_bytes)?;
        Self::new(
            public_key,
            signing_key,
            key_area[public_len..public_len + padding_len].to_vec(),
            certificate,
        )
    }

    /// Decodes one complete key-and-certificate value.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    /// Encodes one complete key-and-certificate value.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }

    fn validate(&self) -> Result<(), CodecError> {
        let public_len = self.public_key.key_type().public_key_len().ok_or_else(|| {
            unsupported(
                0,
                "encryption key type",
                self.public_key.key_type().code() as u64,
            )
        })?;
        let signing_len = self
            .signing_key
            .key_type()
            .public_key_len()
            .ok_or_else(|| {
                unsupported(
                    0,
                    "signing key type",
                    self.signing_key.key_type().code() as u64,
                )
            })?;
        if public_len > LEGACY_PUBLIC_KEY_SIZE {
            return Err(unsupported(
                0,
                "identity encryption key layout",
                self.public_key.key_type().code() as u64,
            ));
        }
        let expected_padding = KEY_AREA_SIZE
            .checked_sub(public_len)
            .and_then(|remaining| remaining.checked_sub(signing_len.min(LEGACY_SIGNING_KEY_SIZE)))
            .ok_or_else(|| invalid(0, "identity key area"))?;
        if self.padding.len() != expected_padding {
            return Err(invalid(0, "identity key padding length"));
        }
        match &self.certificate {
            Certificate::Null => {
                if self.public_key.key_type() != CryptoKeyType::ElGamal
                    || self.signing_key.key_type() != SigningKeyType::DsaSha1
                    || self.public_key.as_bytes().len() != LEGACY_PUBLIC_KEY_SIZE
                    || self.signing_key.as_bytes().len() != LEGACY_SIGNING_KEY_SIZE
                    || !self.padding.is_empty()
                {
                    return Err(invalid(0, "null certificate key material"));
                }
            }
            Certificate::Key(key) => {
                if key.crypto_type() != self.public_key.key_type()
                    || key.signing_type() != self.signing_key.key_type()
                {
                    return Err(invalid(0, "key certificate algorithm binding"));
                }
                let public_extra =
                    &self.public_key.as_bytes()[LEGACY_PUBLIC_KEY_SIZE.min(public_len)..];
                let signing_extra =
                    &self.signing_key.as_bytes()[LEGACY_SIGNING_KEY_SIZE.min(signing_len)..];
                if public_extra != key.excess_crypto.as_slice()
                    || signing_extra != key.excess_signing.as_slice()
                {
                    return Err(invalid(0, "key certificate excess key material"));
                }
            }
            Certificate::Unsupported { type_code, .. } => {
                return Err(unsupported(
                    0,
                    "identity certificate",
                    u64::from(*type_code),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        self.validate()?;
        let public_len = self.public_key.as_bytes().len();
        let signing_prefix_len = self
            .signing_key
            .as_bytes()
            .len()
            .min(LEGACY_SIGNING_KEY_SIZE);
        encoder.write_raw(&self.public_key.as_bytes()[..public_len.min(LEGACY_PUBLIC_KEY_SIZE)])?;
        encoder.write_raw(&self.padding)?;
        encoder.write_raw(&self.signing_key.as_bytes()[..signing_prefix_len])?;
        self.certificate.encode_into(encoder)
    }

    /// Returns the validated encryption public key.
    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    /// Returns the validated signing public key.
    pub fn signing_key(&self) -> &SigningPublicKey {
        &self.signing_key
    }

    /// Returns the retained key-area padding bytes.
    pub fn padding(&self) -> &[u8] {
        &self.padding
    }

    /// Returns the certificate.
    pub fn certificate(&self) -> &Certificate {
        &self.certificate
    }
}

impl fmt::Debug for KeyAndCert {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KeyAndCert")
            .field("public_key", &self.public_key)
            .field("signing_key", &self.signing_key)
            .field("padding_length", &self.padding.len())
            .field("certificate", &self.certificate)
            .finish()
    }
}

/// A router identity, including public key material and its certificate.
#[derive(Clone, Eq, PartialEq)]
pub struct RouterIdentity {
    pub(super) keys: KeyAndCert,
}

impl RouterIdentity {
    /// Creates an identity and rejects key types not currently valid for router identities.
    pub fn new(keys: KeyAndCert) -> Result<Self, CodecError> {
        if !keys.public_key().key_type().allowed_in_identity() {
            return Err(CodecError::PolicyRejected {
                offset: 0,
                context: "router identity encryption algorithm",
            });
        }
        Ok(Self { keys })
    }

    /// Decodes a complete RouterIdentity.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    pub(super) fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        Self::new(KeyAndCert::decode_from(cursor)?)
    }

    /// Encodes the complete identity.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.keys.encode_into(encoder))
    }

    /// Returns the SHA-256 hash of the exact canonical identity encoding.
    pub fn hash(&self) -> Result<Hash, CodecError> {
        Ok(Hash::digest(
            &self.encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)?,
        ))
    }

    /// Returns the identity's encryption public key.
    pub fn public_key(&self) -> &PublicKey {
        self.keys.public_key()
    }

    /// Returns the identity's signing public key.
    pub fn signing_key(&self) -> &SigningPublicKey {
        self.keys.signing_key()
    }

    /// Returns the identity certificate.
    pub fn certificate(&self) -> &Certificate {
        self.keys.certificate()
    }
}

impl fmt::Debug for RouterIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("RouterIdentity")
            .field(&self.keys)
            .finish()
    }
}

/// A destination identity. Its public encryption field is structurally retained
/// even though legacy destination encryption does not use it.
#[derive(Clone, Eq, PartialEq)]
pub struct Destination {
    pub(super) keys: KeyAndCert,
}

impl Destination {
    /// Creates a destination and applies the current structural algorithm policy.
    pub fn new(keys: KeyAndCert) -> Result<Self, CodecError> {
        if !keys.public_key().key_type().allowed_in_identity() {
            return Err(CodecError::PolicyRejected {
                offset: 0,
                context: "destination encryption algorithm",
            });
        }
        Ok(Self { keys })
    }

    /// Decodes a complete Destination.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    pub(super) fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        Self::new(KeyAndCert::decode_from(cursor)?)
    }

    /// Encodes the complete destination.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.keys.encode_into(encoder))
    }

    /// Returns the SHA-256 hash of the exact canonical destination encoding.
    pub fn hash(&self) -> Result<Hash, CodecError> {
        Ok(Hash::digest(
            &self.encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)?,
        ))
    }

    /// Returns the destination's encryption public key.
    pub fn public_key(&self) -> &PublicKey {
        self.keys.public_key()
    }

    /// Returns the destination's signing public key.
    pub fn signing_key(&self) -> &SigningPublicKey {
        self.keys.signing_key()
    }

    /// Returns the destination certificate.
    pub fn certificate(&self) -> &Certificate {
        self.keys.certificate()
    }
}

impl fmt::Debug for Destination {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Destination")
            .field(&self.keys)
            .finish()
    }
}
