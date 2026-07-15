//! RouterInfo and signed-region preservation.

use super::*;

/// A bounded protocol-version option value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolVersion(String);

impl ProtocolVersion {
    /// Validates a version string without assigning runtime policy to it.
    pub fn new(value: &str) -> Result<Self, CodecError> {
        validate_text(value, false, "router version")?;
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
        {
            return Err(invalid(0, "router version"));
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the encoded version text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A bounded, opaque router capability string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Capabilities(String);

impl Capabilities {
    /// Validates capability text while allowing future capability letters.
    pub fn new(value: &str) -> Result<Self, CodecError> {
        validate_text(value, true, "router capabilities")?;
        if value.bytes().any(|byte| byte.is_ascii_whitespace()) {
            return Err(invalid(0, "router capabilities"));
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the encoded capability text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn validate_router_options(options: &Mapping) -> Result<(), CodecError> {
    if let Some(version) = options.get("router.version") {
        ProtocolVersion::new(version)?;
    }
    if let Some(capabilities) = options.get("caps") {
        Capabilities::new(capabilities)?;
    }
    Ok(())
}

/// A RouterInfo with its exact signed region retained.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouterInfo {
    router_identity: RouterIdentity,
    published: Date,
    addresses: Vec<RouterAddress>,
    peers: Vec<Hash>,
    options: Mapping,
    signed_bytes: Vec<u8>,
    signature: SignatureValue,
}

impl RouterInfo {
    /// Creates a RouterInfo from validated semantic fields and a typed signature.
    pub fn new(
        router_identity: RouterIdentity,
        published: Date,
        addresses: Vec<RouterAddress>,
        peers: Vec<Hash>,
        options: Mapping,
        signature: SignatureValue,
    ) -> Result<Self, CodecError> {
        if addresses.len() > MAX_ROUTER_ADDRESSES || peers.len() > u8::MAX as usize {
            return Err(CodecError::LengthExceeded {
                offset: 0,
                declared: addresses.len().max(peers.len()),
                maximum: MAX_ROUTER_ADDRESSES,
                context: "RouterInfo entry count",
            });
        }
        if signature.key_type() != router_identity.signing_key().key_type() {
            return Err(invalid(0, "RouterInfo signature type"));
        }
        validate_router_options(&options)?;
        let signed_bytes = encode_to_vec(MAX_COMMON_STRUCTURE_SIZE, |encoder| {
            Self::encode_unsigned(
                encoder,
                &router_identity,
                published,
                &addresses,
                &peers,
                &options,
            )
        })?;
        let total = signed_bytes
            .len()
            .checked_add(signature.as_bytes().len())
            .ok_or(CodecError::ArithmeticOverflow {
                offset: signed_bytes.len(),
                context: "RouterInfo length",
            })?;
        if total > MAX_COMMON_STRUCTURE_SIZE {
            return Err(CodecError::LengthExceeded {
                offset: signed_bytes.len(),
                declared: total,
                maximum: MAX_COMMON_STRUCTURE_SIZE,
                context: "RouterInfo",
            });
        }
        Ok(Self {
            router_identity,
            published,
            addresses,
            peers,
            options,
            signed_bytes,
            signature,
        })
    }

    /// Decodes a complete RouterInfo and retains its exact signed bytes.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        if input.len() > maximum {
            return Err(CodecError::LengthExceeded {
                offset: 0,
                declared: input.len(),
                maximum,
                context: "RouterInfo",
            });
        }
        let mut cursor = DecodeCursor::new(input, maximum)?;
        let router_identity = RouterIdentity::decode_from(&mut cursor)?;
        let published = Date::decode_from(&mut cursor)?;
        let address_count = usize::from(cursor.read_u8()?);
        if address_count > MAX_ROUTER_ADDRESSES {
            return Err(CodecError::PolicyRejected {
                offset: cursor.offset().saturating_sub(1),
                context: "RouterInfo address count",
            });
        }
        let mut addresses = Vec::with_capacity(address_count);
        for _ in 0..address_count {
            addresses.push(RouterAddress::decode_from(&mut cursor, maximum)?);
        }
        let peer_count = usize::from(cursor.read_u8()?);
        let mut peers = Vec::with_capacity(peer_count);
        for _ in 0..peer_count {
            peers.push(Hash::decode_from(&mut cursor)?);
        }
        let options = Mapping::decode_from(&mut cursor, maximum)?;
        validate_router_options(&options)?;
        let signed_end = cursor.offset();
        let signature_len = router_identity
            .signing_key()
            .key_type()
            .signature_len()
            .ok_or_else(|| {
                unsupported(
                    signed_end,
                    "RouterInfo signature type",
                    router_identity.signing_key().key_type().code() as u64,
                )
            })?;
        let signature = SignatureValue::new(
            router_identity.signing_key().key_type(),
            cursor.take(signature_len)?.to_vec(),
        )?;
        cursor.finish()?;
        Ok(Self {
            router_identity,
            published,
            addresses,
            peers,
            options,
            signed_bytes: input[..signed_end].to_vec(),
            signature,
        })
    }

    fn encode_unsigned(
        encoder: &mut EncodeBuffer<'_>,
        router_identity: &RouterIdentity,
        published: Date,
        addresses: &[RouterAddress],
        peers: &[Hash],
        options: &Mapping,
    ) -> Result<(), CodecError> {
        router_identity.keys.encode_into(encoder)?;
        published.encode_into(encoder)?;
        encoder.write_u8(
            u8::try_from(addresses.len()).map_err(|_| invalid(0, "RouterInfo address count"))?,
        )?;
        for address in addresses {
            address.encode_into(encoder)?;
        }
        encoder.write_u8(
            u8::try_from(peers.len()).map_err(|_| invalid(0, "RouterInfo peer count"))?,
        )?;
        for peer in peers {
            peer.encode_into(encoder)?;
        }
        options.encode_into(encoder)
    }

    /// Encodes the exact retained signed region followed by its signature.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| {
            encoder.write_raw(&self.signed_bytes)?;
            encoder.write_raw(self.signature.as_bytes())
        })
    }

    /// Returns the exact bytes covered by the RouterInfo signature.
    pub fn signed_bytes(&self) -> &[u8] {
        &self.signed_bytes
    }

    /// Returns the typed RouterInfo signature.
    pub fn signature(&self) -> &SignatureValue {
        &self.signature
    }

    /// Returns the router identity.
    pub fn router_identity(&self) -> &RouterIdentity {
        &self.router_identity
    }

    /// Returns the publication date; freshness policy is deferred.
    pub const fn published(&self) -> Date {
        self.published
    }

    /// Returns the ordered router addresses.
    pub fn addresses(&self) -> &[RouterAddress] {
        &self.addresses
    }

    /// Returns the optional bounded protocol version.
    pub fn protocol_version(&self) -> Result<Option<ProtocolVersion>, CodecError> {
        self.options
            .get("router.version")
            .map(ProtocolVersion::new)
            .transpose()
    }

    /// Returns the optional bounded capability string.
    pub fn capabilities(&self) -> Result<Option<Capabilities>, CodecError> {
        self.options.get("caps").map(Capabilities::new).transpose()
    }

    /// Returns the complete options mapping.
    pub fn options(&self) -> &Mapping {
        &self.options
    }
}
