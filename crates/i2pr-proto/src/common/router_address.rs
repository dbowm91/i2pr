//! RouterAddress structural data; transport behavior is out of scope.

use super::*;

/// A transport address containing only protocol data, never a transport object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouterAddress {
    cost: u8,
    expiration: Date,
    transport_style: String,
    options: Mapping,
}

impl RouterAddress {
    /// Creates a validated router address.
    pub fn new(
        cost: u8,
        expiration: Date,
        transport_style: String,
        options: Mapping,
    ) -> Result<Self, CodecError> {
        validate_text(&transport_style, false, "transport style")?;
        Ok(Self {
            cost,
            expiration,
            transport_style,
            options,
        })
    }

    pub(super) fn decode_from(
        cursor: &mut DecodeCursor<'_>,
        maximum: usize,
    ) -> Result<Self, CodecError> {
        let cost = cursor.read_u8()?;
        let expiration = Date::decode_from(cursor)?;
        let transport_style = cursor.read_utf8_u8(u8::MAX as usize)?.to_owned();
        let options = Mapping::decode_from(cursor, maximum)?;
        Self::new(cost, expiration, transport_style, options)
    }

    /// Decodes one complete RouterAddress.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        decode_exact(input, maximum, |cursor| Self::decode_from(cursor, maximum))
    }

    /// Encodes one complete RouterAddress.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }

    pub(super) fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        encoder.write_u8(self.cost)?;
        self.expiration.encode_into(encoder)?;
        encoder.write_utf8_u8(&self.transport_style, u8::MAX as usize)?;
        self.options.encode_into(encoder)
    }

    /// Returns the relative transport cost.
    pub const fn cost(&self) -> u8 {
        self.cost
    }

    /// Returns the structural expiration date. Freshness policy is deferred.
    pub const fn expiration(&self) -> Date {
        self.expiration
    }

    /// Returns the opaque transport style identifier.
    pub fn transport_style(&self) -> &str {
        &self.transport_style
    }

    /// Returns canonical transport options.
    pub fn options(&self) -> &Mapping {
        &self.options
    }
}
