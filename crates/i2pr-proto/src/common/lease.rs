//! Lease and classic LeaseSet structural codecs.

use super::*;

/// A classic 44-byte Lease.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lease {
    tunnel_gateway: Hash,
    tunnel_id: u32,
    end_date: Date,
}

impl Lease {
    /// Creates a structural Lease. Tunnel ID zero is retained for special cases.
    pub const fn new(tunnel_gateway: Hash, tunnel_id: u32, end_date: Date) -> Self {
        Self {
            tunnel_gateway,
            tunnel_id,
            end_date,
        }
    }

    pub(super) fn decode_from(cursor: &mut DecodeCursor<'_>) -> Result<Self, CodecError> {
        Ok(Self::new(
            Hash::decode_from(cursor)?,
            cursor.read_u32()?,
            Date::decode_from(cursor)?,
        ))
    }

    /// Decodes one complete 44-byte Lease.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, Self::decode_from)
    }

    /// Encodes one complete Lease.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }

    pub(super) fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        self.tunnel_gateway.encode_into(encoder)?;
        encoder.write_u32(self.tunnel_id)?;
        self.end_date.encode_into(encoder)
    }

    /// Returns the gateway hash.
    pub const fn tunnel_gateway(&self) -> Hash {
        self.tunnel_gateway
    }

    /// Returns the tunnel ID.
    pub const fn tunnel_id(&self) -> u32 {
        self.tunnel_id
    }

    /// Returns the expiration date; freshness policy is deferred.
    pub const fn end_date(&self) -> Date {
        self.end_date
    }
}

/// A classic LeaseSet. LeaseSet2-family structures are intentionally deferred.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaseSet {
    destination: Destination,
    encryption_key: PublicKey,
    signing_key: SigningPublicKey,
    leases: Vec<Lease>,
    signed_bytes: Vec<u8>,
    signature: SignatureValue,
}

impl LeaseSet {
    /// Creates a validated classic LeaseSet.
    pub fn new(
        destination: Destination,
        encryption_key: PublicKey,
        signing_key: SigningPublicKey,
        leases: Vec<Lease>,
        signature: SignatureValue,
    ) -> Result<Self, CodecError> {
        if encryption_key.key_type() != CryptoKeyType::ElGamal
            || encryption_key.as_bytes().len() != LEGACY_PUBLIC_KEY_SIZE
        {
            return Err(CodecError::PolicyRejected {
                offset: 0,
                context: "classic LeaseSet encryption key",
            });
        }
        if signing_key.key_type() != destination.signing_key().key_type()
            || signature.key_type() != destination.signing_key().key_type()
        {
            return Err(invalid(0, "LeaseSet signing type"));
        }
        if leases.len() > MAX_LEASES {
            return Err(CodecError::LengthExceeded {
                offset: 0,
                declared: leases.len(),
                maximum: MAX_LEASES,
                context: "LeaseSet lease count",
            });
        }
        let signed_bytes = encode_to_vec(MAX_COMMON_STRUCTURE_SIZE, |encoder| {
            Self::encode_unsigned(
                encoder,
                &destination,
                &encryption_key,
                &signing_key,
                &leases,
            )
        })?;
        let total = signed_bytes
            .len()
            .checked_add(signature.as_bytes().len())
            .ok_or(CodecError::ArithmeticOverflow {
                offset: signed_bytes.len(),
                context: "LeaseSet length",
            })?;
        if total > MAX_COMMON_STRUCTURE_SIZE {
            return Err(CodecError::LengthExceeded {
                offset: signed_bytes.len(),
                declared: total,
                maximum: MAX_COMMON_STRUCTURE_SIZE,
                context: "LeaseSet",
            });
        }
        Ok(Self {
            destination,
            encryption_key,
            signing_key,
            leases,
            signed_bytes,
            signature,
        })
    }

    /// Decodes a complete classic LeaseSet.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        let mut cursor = DecodeCursor::new(input, maximum)?;
        let destination = Destination::decode_from(&mut cursor)?;
        let encryption_key = PublicKey::new(
            CryptoKeyType::ElGamal,
            cursor.take(LEGACY_PUBLIC_KEY_SIZE)?.to_vec(),
        )?;
        let signing_type = destination.signing_key().key_type();
        let signing_key = SigningPublicKey::new(
            signing_type,
            cursor
                .take(signing_type.public_key_len().ok_or_else(|| {
                    unsupported(0, "LeaseSet signing type", signing_type.code() as u64)
                })?)?
                .to_vec(),
        )?;
        let lease_count = usize::from(cursor.read_u8()?);
        if lease_count > MAX_LEASES {
            return Err(CodecError::PolicyRejected {
                offset: cursor.offset().saturating_sub(1),
                context: "LeaseSet lease count",
            });
        }
        let mut leases = Vec::with_capacity(lease_count);
        for _ in 0..lease_count {
            leases.push(Lease::decode_from(&mut cursor)?);
        }
        let signed_end = cursor.offset();
        let signature = SignatureValue::new(
            signing_type,
            cursor
                .take(signing_type.signature_len().ok_or_else(|| {
                    unsupported(0, "LeaseSet signature type", signing_type.code() as u64)
                })?)?
                .to_vec(),
        )?;
        cursor.finish()?;
        Ok(Self {
            destination,
            encryption_key,
            signing_key,
            leases,
            signed_bytes: input[..signed_end].to_vec(),
            signature,
        })
    }

    fn encode_unsigned(
        encoder: &mut EncodeBuffer<'_>,
        destination: &Destination,
        encryption_key: &PublicKey,
        signing_key: &SigningPublicKey,
        leases: &[Lease],
    ) -> Result<(), CodecError> {
        destination.keys.encode_into(encoder)?;
        encoder.write_raw(encryption_key.as_bytes())?;
        encoder.write_raw(signing_key.as_bytes())?;
        encoder.write_u8(
            u8::try_from(leases.len()).map_err(|_| invalid(0, "LeaseSet lease count"))?,
        )?;
        for lease in leases {
            lease.encode_into(encoder)?;
        }
        Ok(())
    }

    /// Encodes the exact retained signed region followed by its signature.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| {
            encoder.write_raw(&self.signed_bytes)?;
            encoder.write_raw(self.signature.as_bytes())
        })
    }

    /// Returns the exact bytes covered by the LeaseSet signature.
    pub fn signed_bytes(&self) -> &[u8] {
        &self.signed_bytes
    }

    /// Returns the destination.
    pub fn destination(&self) -> &Destination {
        &self.destination
    }

    /// Returns the encryption key.
    pub fn encryption_key(&self) -> &PublicKey {
        &self.encryption_key
    }

    /// Returns the revocation key material.
    pub fn signing_key(&self) -> &SigningPublicKey {
        &self.signing_key
    }

    /// Returns the leases.
    pub fn leases(&self) -> &[Lease] {
        &self.leases
    }

    /// Returns the typed signature.
    pub fn signature(&self) -> &SignatureValue {
        &self.signature
    }
}

/// LeaseSet-family variants deliberately deferred until their later crypto and
/// NetDB plans define complete semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeferredLeaseSetVariant {
    /// LeaseSet2 and its offline-signature/header semantics.
    LeaseSet2,
    /// MetaLeaseSet records.
    MetaLeaseSet,
    /// EncryptedLeaseSet records.
    EncryptedLeaseSet,
}

/// Decodes only classic LeaseSets from a DatabaseStore type and rejects the
/// later variants explicitly rather than guessing their wire layout.
pub fn decode_lease_set_variant(
    store_type: u8,
    input: &[u8],
    maximum: usize,
) -> Result<LeaseSet, CodecError> {
    match store_type {
        1 => LeaseSet::decode(input, maximum),
        3 => Err(unsupported(0, "LeaseSet2 variant", 3)),
        5 => Err(unsupported(0, "EncryptedLeaseSet variant", 5)),
        7 => Err(unsupported(0, "MetaLeaseSet variant", 7)),
        other => Err(unsupported(
            0,
            "LeaseSet DatabaseStore type",
            u64::from(other),
        )),
    }
}
