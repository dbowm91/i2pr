//! Top-level I2NP body registry, message framing, and dispatch.

use super::*;

/// The typed body registry for the initial I2NP subset.
#[derive(Debug, Eq, PartialEq)]
pub enum I2npBody {
    /// DatabaseStore body.
    DatabaseStore(Box<DatabaseStoreMessage>),
    /// DatabaseLookup body.
    DatabaseLookup(Box<DatabaseLookupMessage>),
    /// DatabaseSearchReply body.
    DatabaseSearchReply(DatabaseSearchReplyMessage),
    /// DeliveryStatus body.
    DeliveryStatus(DeliveryStatusMessage),
    /// Cryptographic garlic interpretation is deferred after length framing.
    Garlic(OpaqueMessageBody),
    /// Fixed tunnel data body.
    TunnelData(Box<TunnelDataMessage>),
    /// Nested standard I2NP message delivered to a tunnel gateway.
    TunnelGateway(Box<TunnelGatewayMessage>),
    /// Data body framing is retained for the later garlic/client layer.
    Data(OpaqueMessageBody),
    /// Deprecated fixed tunnel-build records; record cryptography is deferred.
    TunnelBuild(DeferredBuildRecords),
    /// Deprecated fixed tunnel-build reply records; record cryptography is deferred.
    TunnelBuildReply(DeferredBuildRecords),
    /// Variable tunnel-build records; record cryptography is deferred.
    VariableTunnelBuild(DeferredBuildRecords),
    /// Variable tunnel-build reply records; record cryptography is deferred.
    VariableTunnelBuildReply(DeferredBuildRecords),
    /// Short tunnel-build records; record cryptography is deferred.
    ShortTunnelBuild(DeferredBuildRecords),
    /// Short tunnel-build reply records; record cryptography is deferred.
    OutboundTunnelBuildReply(DeferredBuildRecords),
}

impl I2npBody {
    /// Returns the registry type associated with this body.
    pub const fn message_type(&self) -> MessageType {
        match self {
            Self::DatabaseStore(_) => MessageType::DatabaseStore,
            Self::DatabaseLookup(_) => MessageType::DatabaseLookup,
            Self::DatabaseSearchReply(_) => MessageType::DatabaseSearchReply,
            Self::DeliveryStatus(_) => MessageType::DeliveryStatus,
            Self::Garlic(_) => MessageType::Garlic,
            Self::TunnelData(_) => MessageType::TunnelData,
            Self::TunnelGateway(_) => MessageType::TunnelGateway,
            Self::Data(_) => MessageType::Data,
            Self::TunnelBuild(_) => MessageType::TunnelBuild,
            Self::TunnelBuildReply(_) => MessageType::TunnelBuildReply,
            Self::VariableTunnelBuild(_) => MessageType::VariableTunnelBuild,
            Self::VariableTunnelBuildReply(_) => MessageType::VariableTunnelBuildReply,
            Self::ShortTunnelBuild(_) => MessageType::ShortTunnelBuild,
            Self::OutboundTunnelBuildReply(_) => MessageType::OutboundTunnelBuildReply,
        }
    }

    /// Encodes one complete body under an explicit total limit.
    pub fn encode_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        encode_to_vec(maximum, |encoder| self.encode_into(encoder))
    }

    pub(super) fn encode_into(&self, encoder: &mut EncodeBuffer<'_>) -> Result<(), CodecError> {
        match self {
            Self::DatabaseStore(value) => encode_database_store(encoder, value),
            Self::DatabaseLookup(value) => encode_database_lookup(encoder, value),
            Self::DatabaseSearchReply(value) => encode_database_search_reply(encoder, value),
            Self::DeliveryStatus(value) => {
                encoder.write_u32(value.message_id)?;
                encoder.write_u64(value.timestamp.as_millis())
            }
            Self::Garlic(value) | Self::Data(value) => {
                encoder.write_u32(u32::try_from(value.payload.as_bytes().len()).map_err(
                    |_| CodecError::InvalidFieldValue {
                        offset: encoder.len(),
                        context: "opaque I2NP payload length",
                    },
                )?)?;
                encoder.write_raw(value.payload.as_bytes())
            }
            Self::TunnelData(value) => {
                if value.tunnel_id == 0 {
                    return Err(CodecError::InvalidFieldValue {
                        offset: 0,
                        context: "TunnelData tunnel ID",
                    });
                }
                encoder.write_u32(value.tunnel_id)?;
                encoder.write_raw(&value.data)
            }
            Self::TunnelGateway(value) => {
                if value.tunnel_id == 0 {
                    return Err(CodecError::InvalidFieldValue {
                        offset: 0,
                        context: "TunnelGateway tunnel ID",
                    });
                }
                let nested = value
                    .message
                    .encode_standard_to_vec(MAX_I2NP_PAYLOAD_SIZE)?;
                let length =
                    u16::try_from(nested.len()).map_err(|_| CodecError::LengthExceeded {
                        offset: 4,
                        declared: nested.len(),
                        maximum: usize::from(u16::MAX),
                        context: "TunnelGateway nested message",
                    })?;
                encoder.write_u32(value.tunnel_id)?;
                encoder.write_u16(length)?;
                encoder.write_raw(&nested)
            }
            Self::TunnelBuild(value) | Self::TunnelBuildReply(value) => {
                encode_fixed_records(encoder, value, 8, VARIABLE_BUILD_RECORD_SIZE)
            }
            Self::VariableTunnelBuild(value) | Self::VariableTunnelBuildReply(value) => {
                encode_variable_records(encoder, value, VARIABLE_BUILD_RECORD_SIZE)
            }
            Self::ShortTunnelBuild(value) | Self::OutboundTunnelBuildReply(value) => {
                encode_variable_records(encoder, value, SHORT_BUILD_RECORD_SIZE)
            }
        }
    }
}

/// A complete I2NP message with one of the specification's header variants.
#[derive(Debug, Eq, PartialEq)]
pub struct I2npMessage {
    header: I2npHeader,
    body: I2npBody,
}

impl I2npMessage {
    /// Creates a standard-header message after validating its body size.
    pub fn new_standard(
        message_id: u32,
        expiration: Date,
        body: I2npBody,
    ) -> Result<Self, CodecError> {
        validate_body_size(&body)?;
        Ok(Self {
            header: I2npHeader::Standard {
                message_type: body.message_type(),
                message_id,
                expiration,
            },
            body,
        })
    }

    /// Creates an obsolete SSU short-header message.
    pub fn new_short_ssu(expiration_seconds: u32, body: I2npBody) -> Result<Self, CodecError> {
        validate_body_size(&body)?;
        Ok(Self {
            header: I2npHeader::ShortSsu {
                message_type: body.message_type(),
                expiration_seconds,
            },
            body,
        })
    }

    /// Creates an NTCP2/SSU2 short-header message.
    pub fn new_short_transport(
        message_id: u32,
        expiration_seconds: u32,
        body: I2npBody,
    ) -> Result<Self, CodecError> {
        validate_body_size(&body)?;
        Ok(Self {
            header: I2npHeader::ShortTransport {
                message_type: body.message_type(),
                message_id,
                expiration_seconds,
            },
            body,
        })
    }

    /// Decodes a complete standard-header message.
    pub fn decode(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        Self::decode_standard(input, maximum)
    }

    /// Decodes a complete standard-header message.
    pub fn decode_standard(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        let mut cursor = DecodeCursor::new(input, maximum)?;
        let message_type = read_message_type(&mut cursor)?;
        let message_id = cursor.read_u32()?;
        let expiration = Date::from_millis(cursor.read_u64()?);
        let payload_length = usize::from(cursor.read_u16()?);
        if payload_length > MAX_I2NP_PAYLOAD_SIZE {
            return Err(CodecError::LengthExceeded {
                offset: cursor.offset().saturating_sub(2),
                declared: payload_length,
                maximum: MAX_I2NP_PAYLOAD_SIZE,
                context: "I2NP payload",
            });
        }
        let checksum = cursor.read_u8()?;
        let payload = cursor.take(payload_length)?;
        cursor.finish()?;
        verify_checksum(payload, checksum, STANDARD_HEADER_SIZE - 1)?;
        let body = decode_body(message_type, payload, maximum)?;
        Ok(Self {
            header: I2npHeader::Standard {
                message_type,
                message_id,
                expiration,
            },
            body,
        })
    }

    /// Decodes a complete obsolete five-byte SSU short-header message.
    pub fn decode_short_ssu(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        let mut cursor = DecodeCursor::new(input, maximum)?;
        let message_type = read_message_type(&mut cursor)?;
        let expiration_seconds = cursor.read_u32()?;
        let payload = cursor.take(cursor.remaining())?;
        cursor.finish()?;
        let body = decode_body(message_type, payload, maximum)?;
        Ok(Self {
            header: I2npHeader::ShortSsu {
                message_type,
                expiration_seconds,
            },
            body,
        })
    }

    /// Decodes a complete NTCP2/SSU2 nine-byte short-header message.
    pub fn decode_short_transport(input: &[u8], maximum: usize) -> Result<Self, CodecError> {
        let mut cursor = DecodeCursor::new(input, maximum)?;
        let message_type = read_message_type(&mut cursor)?;
        let message_id = cursor.read_u32()?;
        let expiration_seconds = cursor.read_u32()?;
        let payload = cursor.take(cursor.remaining())?;
        cursor.finish()?;
        let body = decode_body(message_type, payload, maximum)?;
        Ok(Self {
            header: I2npHeader::ShortTransport {
                message_type,
                message_id,
                expiration_seconds,
            },
            body,
        })
    }

    /// Encodes this message using the standard header.
    pub fn encode_standard_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        let I2npHeader::Standard {
            message_type,
            message_id,
            expiration,
        } = self.header
        else {
            return Err(CodecError::InvalidFieldValue {
                offset: 0,
                context: "I2NP header variant",
            });
        };
        if message_type != self.body.message_type() {
            return Err(CodecError::InvalidFieldValue {
                offset: 0,
                context: "I2NP message type/body",
            });
        }
        encode_message(
            maximum,
            STANDARD_HEADER_SIZE,
            |encoder, body| {
                encoder.write_u8(message_type.code())?;
                encoder.write_u32(message_id)?;
                encoder.write_u64(expiration.as_millis())?;
                write_body_length(encoder, body.len())?;
                encoder.write_u8(checksum(body))?;
                encoder.write_raw(body)
            },
            &self.body,
        )
    }

    /// Encodes this message using the obsolete five-byte SSU header.
    pub fn encode_short_ssu_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        let I2npHeader::ShortSsu {
            message_type,
            expiration_seconds,
        } = self.header
        else {
            return Err(CodecError::InvalidFieldValue {
                offset: 0,
                context: "I2NP header variant",
            });
        };
        if message_type != self.body.message_type() {
            return Err(CodecError::InvalidFieldValue {
                offset: 0,
                context: "I2NP message type/body",
            });
        }
        encode_message(
            maximum,
            SHORT_SSU_HEADER_SIZE,
            |encoder, body| {
                encoder.write_u8(message_type.code())?;
                encoder.write_u32(expiration_seconds)?;
                encoder.write_raw(body)
            },
            &self.body,
        )
    }

    /// Encodes this message using the NTCP2/SSU2 short header.
    pub fn encode_short_transport_to_vec(&self, maximum: usize) -> Result<Vec<u8>, CodecError> {
        let I2npHeader::ShortTransport {
            message_type,
            message_id,
            expiration_seconds,
        } = self.header
        else {
            return Err(CodecError::InvalidFieldValue {
                offset: 0,
                context: "I2NP header variant",
            });
        };
        if message_type != self.body.message_type() {
            return Err(CodecError::InvalidFieldValue {
                offset: 0,
                context: "I2NP message type/body",
            });
        }
        encode_message(
            maximum,
            SHORT_TRANSPORT_HEADER_SIZE,
            |encoder, body| {
                encoder.write_u8(message_type.code())?;
                encoder.write_u32(message_id)?;
                encoder.write_u32(expiration_seconds)?;
                encoder.write_raw(body)
            },
            &self.body,
        )
    }

    /// Returns the parsed header.
    pub const fn header(&self) -> I2npHeader {
        self.header
    }

    /// Returns the typed body.
    pub fn body(&self) -> &I2npBody {
        &self.body
    }
}

fn encode_message<F>(
    maximum: usize,
    header_size: usize,
    write_header: F,
    body: &I2npBody,
) -> Result<Vec<u8>, CodecError>
where
    F: FnOnce(&mut EncodeBuffer<'_>, &[u8]) -> Result<(), CodecError>,
{
    let body_bytes = body.encode_to_vec(MAX_I2NP_PAYLOAD_SIZE)?;
    if body_bytes.len() > MAX_I2NP_PAYLOAD_SIZE {
        return Err(CodecError::LengthExceeded {
            offset: header_size,
            declared: body_bytes.len(),
            maximum: MAX_I2NP_PAYLOAD_SIZE,
            context: "I2NP payload",
        });
    }
    let total =
        header_size
            .checked_add(body_bytes.len())
            .ok_or(CodecError::ArithmeticOverflow {
                offset: header_size,
                context: "I2NP message length",
            })?;
    if total > maximum {
        return Err(CodecError::LengthExceeded {
            offset: 0,
            declared: total,
            maximum,
            context: "I2NP message",
        });
    }
    encode_to_vec(maximum, |encoder| write_header(encoder, &body_bytes))
}

fn validate_body_size(body: &I2npBody) -> Result<(), CodecError> {
    let encoded = body.encode_to_vec(MAX_I2NP_PAYLOAD_SIZE)?;
    if encoded.len() > MAX_I2NP_PAYLOAD_SIZE {
        return Err(CodecError::LengthExceeded {
            offset: 0,
            declared: encoded.len(),
            maximum: MAX_I2NP_PAYLOAD_SIZE,
            context: "I2NP payload",
        });
    }
    Ok(())
}

fn read_message_type(cursor: &mut DecodeCursor<'_>) -> Result<MessageType, CodecError> {
    let code = cursor.read_u8()?;
    let message_type = MessageType::from_code(code);
    if !message_type.supported() {
        return Err(CodecError::Unsupported {
            offset: cursor.offset().saturating_sub(1),
            context: "I2NP message type",
            value: u64::from(code),
        });
    }
    Ok(message_type)
}

fn decode_body(
    message_type: MessageType,
    input: &[u8],
    maximum: usize,
) -> Result<I2npBody, CodecError> {
    if input.len() > MAX_I2NP_PAYLOAD_SIZE {
        return Err(CodecError::LengthExceeded {
            offset: 0,
            declared: input.len(),
            maximum: MAX_I2NP_PAYLOAD_SIZE,
            context: "I2NP payload",
        });
    }
    let body_maximum = maximum.min(MAX_I2NP_PAYLOAD_SIZE);
    decode_exact(input, body_maximum, |cursor| match message_type {
        MessageType::DatabaseStore => Ok(I2npBody::DatabaseStore(Box::new(decode_database_store(
            cursor,
            body_maximum,
        )?))),
        MessageType::DatabaseLookup => Ok(I2npBody::DatabaseLookup(Box::new(
            decode_database_lookup(cursor)?,
        ))),
        MessageType::DatabaseSearchReply => Ok(I2npBody::DatabaseSearchReply(
            decode_database_search_reply(cursor)?,
        )),
        MessageType::DeliveryStatus => Ok(I2npBody::DeliveryStatus(DeliveryStatusMessage::new(
            cursor.read_u32()?,
            Date::from_millis(cursor.read_u64()?),
        ))),
        MessageType::Garlic => Ok(I2npBody::Garlic(OpaqueMessageBody {
            payload: decode_length_prefixed_u32(cursor, body_maximum, "Garlic payload")?,
        })),
        MessageType::TunnelData => {
            let tunnel_id = read_nonzero_u32(cursor, "TunnelData tunnel ID")?;
            let bytes = cursor.take(TUNNEL_DATA_PAYLOAD_SIZE)?;
            let mut data = [0_u8; TUNNEL_DATA_PAYLOAD_SIZE];
            data.copy_from_slice(bytes);
            Ok(I2npBody::TunnelData(Box::new(TunnelDataMessage {
                tunnel_id,
                data,
            })))
        }
        MessageType::TunnelGateway => {
            let tunnel_id = read_nonzero_u32(cursor, "TunnelGateway tunnel ID")?;
            let nested_length = usize::from(cursor.read_u16()?);
            let nested = cursor.take(nested_length)?;
            let message = I2npMessage::decode_standard(nested, nested.len())?;
            Ok(I2npBody::TunnelGateway(Box::new(TunnelGatewayMessage {
                tunnel_id,
                message: Box::new(message),
            })))
        }
        MessageType::Data => Ok(I2npBody::Data(OpaqueMessageBody {
            payload: decode_length_prefixed_u32(cursor, body_maximum, "Data payload")?,
        })),
        MessageType::TunnelBuild => Ok(I2npBody::TunnelBuild(decode_fixed_records(
            cursor,
            8,
            VARIABLE_BUILD_RECORD_SIZE,
        )?)),
        MessageType::TunnelBuildReply => Ok(I2npBody::TunnelBuildReply(decode_fixed_records(
            cursor,
            8,
            VARIABLE_BUILD_RECORD_SIZE,
        )?)),
        MessageType::VariableTunnelBuild => Ok(I2npBody::VariableTunnelBuild(
            decode_variable_records(cursor, VARIABLE_BUILD_RECORD_SIZE)?,
        )),
        MessageType::VariableTunnelBuildReply => Ok(I2npBody::VariableTunnelBuildReply(
            decode_variable_records(cursor, VARIABLE_BUILD_RECORD_SIZE)?,
        )),
        MessageType::ShortTunnelBuild => Ok(I2npBody::ShortTunnelBuild(decode_variable_records(
            cursor,
            SHORT_BUILD_RECORD_SIZE,
        )?)),
        MessageType::OutboundTunnelBuildReply => Ok(I2npBody::OutboundTunnelBuildReply(
            decode_variable_records(cursor, SHORT_BUILD_RECORD_SIZE)?,
        )),
        MessageType::Unknown(code) => Err(CodecError::Unsupported {
            offset: 0,
            context: "I2NP message type",
            value: u64::from(code),
        }),
    })
}

fn decode_database_store(
    cursor: &mut DecodeCursor<'_>,
    maximum: usize,
) -> Result<DatabaseStoreMessage, CodecError> {
    let key = read_hash(cursor)?;
    let type_code = cursor.read_u8()?;
    if type_code & 0xf0 != 0 {
        return Err(CodecError::InvalidFieldValue {
            offset: cursor.offset().saturating_sub(1),
            context: "DatabaseStore reserved type bits",
        });
    }
    let store_type = DatabaseStoreType::from_code(type_code)?;
    let reply_token = cursor.read_u32()?;
    let (reply_tunnel_id, reply_gateway) = if reply_token == 0 {
        (None, None)
    } else {
        (Some(cursor.read_u32()?), Some(read_hash(cursor)?))
    };
    let data = match store_type {
        DatabaseStoreType::RouterInfo => {
            let length = usize::from(cursor.read_u16()?);
            let bytes = cursor.take(length)?.to_vec();
            DatabaseStoreData::RouterInfoCompressed(DeferredPayload::new(
                bytes,
                maximum.min(MAX_I2NP_PAYLOAD_SIZE),
            )?)
        }
        DatabaseStoreType::LeaseSet => {
            let bytes = cursor.take(cursor.remaining())?;
            DatabaseStoreData::LeaseSet(Box::new(LeaseSet::decode(bytes, bytes.len())?))
        }
        DatabaseStoreType::LeaseSet2
        | DatabaseStoreType::EncryptedLeaseSet
        | DatabaseStoreType::MetaLeaseSet => DatabaseStoreData::Deferred {
            store_type,
            payload: DeferredPayload::new(
                cursor.take(cursor.remaining())?.to_vec(),
                maximum.min(MAX_I2NP_PAYLOAD_SIZE),
            )?,
        },
    };
    Ok(DatabaseStoreMessage {
        key,
        reply_token,
        reply_tunnel_id,
        reply_gateway,
        data,
    })
}

fn decode_database_lookup(
    cursor: &mut DecodeCursor<'_>,
) -> Result<DatabaseLookupMessage, CodecError> {
    let key = read_hash(cursor)?;
    let from = read_hash(cursor)?;
    let flags = cursor.read_u8()?;
    if flags & 0xe0 != 0 {
        return Err(CodecError::InvalidFieldValue {
            offset: cursor.offset().saturating_sub(1),
            context: "DatabaseLookup reserved flags",
        });
    }
    let delivery_flag = flags & 1 != 0;
    let encryption_flag = flags & 2 != 0;
    let lookup_type = (flags >> 2) & 3;
    let ecies_flag = flags & 0x10 != 0;
    let reply_tunnel_id = if delivery_flag {
        Some(read_nonzero_u32(cursor, "DatabaseLookup reply tunnel ID")?)
    } else {
        None
    };
    let excluded_count = usize::from(cursor.read_u16()?);
    if excluded_count > MAX_DATABASE_LOOKUP_EXCLUDED_PEERS {
        return Err(CodecError::PolicyRejected {
            offset: cursor.offset().saturating_sub(2),
            context: "DatabaseLookup excluded peer count",
        });
    }
    let mut excluded_peers = Vec::with_capacity(excluded_count);
    for _ in 0..excluded_count {
        excluded_peers.push(read_hash(cursor)?);
    }
    let reply_encryption = match (encryption_flag, ecies_flag) {
        (false, false) => ReplyEncryption::None,
        (true, false) => ReplyEncryption::ElGamal {
            reply_key: read_reply_secret(cursor)?,
            reply_tags: read_tags::<32>(cursor)?,
        },
        (false, true) => ReplyEncryption::Ecies {
            reply_key: read_reply_secret(cursor)?,
            reply_tags: read_tags::<8>(cursor)?,
        },
        (true, true) => {
            return Err(CodecError::Unsupported {
                offset: cursor.offset().saturating_sub(1),
                context: "DatabaseLookup ECIES key-derivation mode",
                value: 1,
            });
        }
    };
    Ok(DatabaseLookupMessage {
        key,
        from,
        delivery_flag,
        reply_tunnel_id,
        lookup_type,
        excluded_peers,
        reply_encryption,
    })
}

fn decode_database_search_reply(
    cursor: &mut DecodeCursor<'_>,
) -> Result<DatabaseSearchReplyMessage, CodecError> {
    let key = read_hash(cursor)?;
    let count = usize::from(cursor.read_u8()?);
    if count > MAX_DATABASE_SEARCH_REPLY_PEERS {
        return Err(CodecError::PolicyRejected {
            offset: cursor.offset().saturating_sub(1),
            context: "DatabaseSearchReply peer count",
        });
    }
    let mut peer_hashes = Vec::with_capacity(count);
    for _ in 0..count {
        peer_hashes.push(read_hash(cursor)?);
    }
    Ok(DatabaseSearchReplyMessage {
        key,
        peer_hashes,
        from: read_hash(cursor)?,
    })
}

fn decode_length_prefixed_u32(
    cursor: &mut DecodeCursor<'_>,
    maximum: usize,
    context: &'static str,
) -> Result<DeferredPayload, CodecError> {
    let length =
        usize::try_from(cursor.read_u32()?).map_err(|_| CodecError::ArithmeticOverflow {
            offset: cursor.offset().saturating_sub(4),
            context: "I2NP payload length conversion",
        })?;
    if length > maximum {
        return Err(CodecError::LengthExceeded {
            offset: cursor.offset().saturating_sub(4),
            declared: length,
            maximum,
            context,
        });
    }
    DeferredPayload::new(cursor.take(length)?.to_vec(), maximum)
}

fn decode_fixed_records(
    cursor: &mut DecodeCursor<'_>,
    count: u8,
    record_size: usize,
) -> Result<DeferredBuildRecords, CodecError> {
    if count == 0 || usize::from(count) > MAX_BUILD_RECORDS {
        return Err(CodecError::InvalidFieldValue {
            offset: cursor.offset(),
            context: "tunnel-build record count",
        });
    }
    let length =
        usize::from(count)
            .checked_mul(record_size)
            .ok_or(CodecError::ArithmeticOverflow {
                offset: cursor.offset(),
                context: "tunnel-build records",
            })?;
    DeferredBuildRecords::new(count, record_size, cursor.take(length)?.to_vec())
}

fn decode_variable_records(
    cursor: &mut DecodeCursor<'_>,
    record_size: usize,
) -> Result<DeferredBuildRecords, CodecError> {
    let count = cursor.read_u8()?;
    decode_fixed_records(cursor, count, record_size)
}

fn encode_database_store(
    encoder: &mut EncodeBuffer<'_>,
    value: &DatabaseStoreMessage,
) -> Result<(), CodecError> {
    if (value.reply_token == 0)
        != (value.reply_tunnel_id.is_none() && value.reply_gateway.is_none())
    {
        return Err(CodecError::InvalidFieldValue {
            offset: 32,
            context: "DatabaseStore reply fields",
        });
    }
    write_hash(encoder, &value.key)?;
    let store_type = match &value.data {
        DatabaseStoreData::RouterInfoCompressed(_) => DatabaseStoreType::RouterInfo,
        DatabaseStoreData::LeaseSet(_) => DatabaseStoreType::LeaseSet,
        DatabaseStoreData::Deferred { store_type, .. } => *store_type,
    };
    encoder.write_u8(store_type.code())?;
    encoder.write_u32(value.reply_token)?;
    if value.reply_token != 0 {
        encoder.write_u32(value.reply_tunnel_id.ok_or(CodecError::InvalidFieldValue {
            offset: encoder.len(),
            context: "DatabaseStore reply tunnel ID",
        })?)?;
        let reply_gateway = value
            .reply_gateway
            .as_ref()
            .ok_or(CodecError::InvalidFieldValue {
                offset: encoder.len(),
                context: "DatabaseStore reply gateway",
            })?;
        write_hash(encoder, reply_gateway)?;
    }
    match &value.data {
        DatabaseStoreData::RouterInfoCompressed(payload) => {
            let length = u16::try_from(payload.as_bytes().len()).map_err(|_| {
                CodecError::LengthExceeded {
                    offset: encoder.len(),
                    declared: payload.as_bytes().len(),
                    maximum: usize::from(u16::MAX),
                    context: "DatabaseStore RouterInfo",
                }
            })?;
            encoder.write_u16(length)?;
            encoder.write_raw(payload.as_bytes())
        }
        DatabaseStoreData::LeaseSet(value) => {
            let bytes = value.encode_to_vec(MAX_I2NP_PAYLOAD_SIZE)?;
            encoder.write_raw(&bytes)
        }
        DatabaseStoreData::Deferred { payload, .. } => encoder.write_raw(payload.as_bytes()),
    }
}

fn encode_database_lookup(
    encoder: &mut EncodeBuffer<'_>,
    value: &DatabaseLookupMessage,
) -> Result<(), CodecError> {
    if value.excluded_peers.len() > MAX_DATABASE_LOOKUP_EXCLUDED_PEERS {
        return Err(CodecError::LengthExceeded {
            offset: encoder.len(),
            declared: value.excluded_peers.len(),
            maximum: MAX_DATABASE_LOOKUP_EXCLUDED_PEERS,
            context: "DatabaseLookup excluded peer count",
        });
    }
    if value.lookup_type > 3 {
        return Err(CodecError::InvalidFieldValue {
            offset: encoder.len(),
            context: "DatabaseLookup lookup type",
        });
    }
    let (encryption_flag, ecies_flag) = match &value.reply_encryption {
        ReplyEncryption::None => (false, false),
        ReplyEncryption::ElGamal { .. } => (true, false),
        ReplyEncryption::Ecies { .. } => (false, true),
    };
    if value.delivery_flag {
        if !matches!(value.reply_tunnel_id, Some(id) if id != 0) {
            return Err(CodecError::InvalidFieldValue {
                offset: encoder.len(),
                context: "DatabaseLookup reply tunnel ID",
            });
        }
    } else if value.reply_tunnel_id.is_some() {
        return Err(CodecError::InvalidFieldValue {
            offset: encoder.len(),
            context: "DatabaseLookup delivery fields",
        });
    }
    write_hash(encoder, &value.key)?;
    write_hash(encoder, &value.from)?;
    let flags = (u8::from(value.delivery_flag))
        | (u8::from(encryption_flag) << 1)
        | (value.lookup_type << 2)
        | (u8::from(ecies_flag) << 4);
    encoder.write_u8(flags)?;
    if value.delivery_flag {
        encoder.write_u32(value.reply_tunnel_id.ok_or(CodecError::InvalidFieldValue {
            offset: encoder.len(),
            context: "DatabaseLookup reply tunnel ID",
        })?)?;
    }
    encoder.write_u16(u16::try_from(value.excluded_peers.len()).map_err(|_| {
        CodecError::InvalidFieldValue {
            offset: encoder.len(),
            context: "DatabaseLookup excluded peer count",
        }
    })?)?;
    for peer in &value.excluded_peers {
        write_hash(encoder, peer)?;
    }
    match &value.reply_encryption {
        ReplyEncryption::None => Ok(()),
        ReplyEncryption::ElGamal {
            reply_key,
            reply_tags,
        } => {
            validate_tag_count(reply_tags.len(), encoder.len())?;
            encoder.write_raw(reply_key.as_bytes())?;
            encoder.write_u8(reply_tags.len() as u8)?;
            for tag in reply_tags {
                encoder.write_raw(tag.as_bytes())?;
            }
            Ok(())
        }
        ReplyEncryption::Ecies {
            reply_key,
            reply_tags,
        } => {
            validate_tag_count(reply_tags.len(), encoder.len())?;
            encoder.write_raw(reply_key.as_bytes())?;
            encoder.write_u8(reply_tags.len() as u8)?;
            for tag in reply_tags {
                encoder.write_raw(tag.as_bytes())?;
            }
            Ok(())
        }
    }
}

fn encode_database_search_reply(
    encoder: &mut EncodeBuffer<'_>,
    value: &DatabaseSearchReplyMessage,
) -> Result<(), CodecError> {
    if value.peer_hashes.len() > MAX_DATABASE_SEARCH_REPLY_PEERS {
        return Err(CodecError::LengthExceeded {
            offset: encoder.len(),
            declared: value.peer_hashes.len(),
            maximum: MAX_DATABASE_SEARCH_REPLY_PEERS,
            context: "DatabaseSearchReply peer count",
        });
    }
    write_hash(encoder, &value.key)?;
    encoder.write_u8(value.peer_hashes.len() as u8)?;
    for peer in &value.peer_hashes {
        write_hash(encoder, peer)?;
    }
    write_hash(encoder, &value.from)
}

fn encode_fixed_records(
    encoder: &mut EncodeBuffer<'_>,
    value: &DeferredBuildRecords,
    expected_count: u8,
    expected_size: usize,
) -> Result<(), CodecError> {
    if value.count != expected_count || usize::from(value.record_size) != expected_size {
        return Err(CodecError::InvalidFieldValue {
            offset: encoder.len(),
            context: "fixed tunnel-build record shape",
        });
    }
    encoder.write_raw(&value.records)
}

fn encode_variable_records(
    encoder: &mut EncodeBuffer<'_>,
    value: &DeferredBuildRecords,
    expected_size: usize,
) -> Result<(), CodecError> {
    if usize::from(value.record_size) != expected_size
        || value.count == 0
        || usize::from(value.count) > MAX_BUILD_RECORDS
    {
        return Err(CodecError::InvalidFieldValue {
            offset: encoder.len(),
            context: "variable tunnel-build record shape",
        });
    }
    encoder.write_u8(value.count)?;
    encoder.write_raw(&value.records)
}

fn write_body_length(encoder: &mut EncodeBuffer<'_>, length: usize) -> Result<(), CodecError> {
    encoder.write_u16(
        u16::try_from(length).map_err(|_| CodecError::LengthExceeded {
            offset: encoder.len(),
            declared: length,
            maximum: usize::from(u16::MAX),
            context: "I2NP payload length",
        })?,
    )
}

fn verify_checksum(payload: &[u8], expected: u8, offset: usize) -> Result<(), CodecError> {
    let actual = checksum(payload);
    if actual != expected {
        return Err(CodecError::InvalidFieldValue {
            offset,
            context: "I2NP payload checksum",
        });
    }
    Ok(())
}

fn checksum(payload: &[u8]) -> u8 {
    Hash::digest(payload).as_bytes()[0]
}

fn write_hash(encoder: &mut EncodeBuffer<'_>, value: &Hash) -> Result<(), CodecError> {
    encoder.write_raw(value.as_bytes())
}

fn read_hash(cursor: &mut DecodeCursor<'_>) -> Result<Hash, CodecError> {
    Ok(Hash::from_bytes(read_array(cursor)?))
}

fn read_array<const N: usize>(cursor: &mut DecodeCursor<'_>) -> Result<[u8; N], CodecError> {
    let bytes = cursor.take(N)?;
    let mut result = [0_u8; N];
    result.copy_from_slice(bytes);
    Ok(result)
}

fn read_nonzero_u32(
    cursor: &mut DecodeCursor<'_>,
    context: &'static str,
) -> Result<u32, CodecError> {
    let value = cursor.read_u32()?;
    if value == 0 {
        return Err(CodecError::InvalidFieldValue {
            offset: cursor.offset().saturating_sub(4),
            context,
        });
    }
    Ok(value)
}

fn read_tags<const N: usize>(
    cursor: &mut DecodeCursor<'_>,
) -> Result<Vec<ReplySecret<N>>, CodecError> {
    let count = usize::from(cursor.read_u8()?);
    validate_tag_count(count, cursor.offset().saturating_sub(1))?;
    (0..count).map(|_| read_reply_secret(cursor)).collect()
}

fn read_reply_secret<const N: usize>(
    cursor: &mut DecodeCursor<'_>,
) -> Result<ReplySecret<N>, CodecError> {
    let mut bytes = Zeroizing::new([0_u8; N]);
    bytes.copy_from_slice(cursor.take(N)?);
    Ok(ReplySecret::from_zeroizing(bytes))
}

fn validate_tag_count(count: usize, offset: usize) -> Result<(), CodecError> {
    if !(1..=32).contains(&count) {
        return Err(CodecError::InvalidFieldValue {
            offset,
            context: "DatabaseLookup reply tag count",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: usize = MAX_I2NP_PAYLOAD_SIZE + STANDARD_HEADER_SIZE;

    fn hash(value: u8) -> Hash {
        Hash::from_bytes([value; 32])
    }

    fn delivery() -> I2npBody {
        I2npBody::DeliveryStatus(DeliveryStatusMessage::new(
            0x0102_0304,
            Date::from_millis(0x0506_0708_090a_0b0c),
        ))
    }

    #[test]
    fn message_registry_has_no_unknown_fallback() {
        assert_eq!(MessageType::from_code(1), MessageType::DatabaseStore);
        assert_eq!(MessageType::from_code(26).code(), 26);
        assert_eq!(MessageType::from_code(99), MessageType::Unknown(99));
        assert!(matches!(
            I2npMessage::decode_standard(&[99], MAX),
            Err(CodecError::Unsupported { .. })
        ));
    }

    #[test]
    fn standard_delivery_status_golden_round_trip() {
        let message = I2npMessage::new_standard(
            0xa1b2_c3d4,
            Date::from_millis(0x0102_0304_0506_0708),
            delivery(),
        )
        .unwrap();
        let encoded = message.encode_standard_to_vec(MAX).unwrap();
        let expected = [
            0x0a, 0xa1, 0xb2, 0xc3, 0xd4, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x00,
            0x0c, 0x20, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
        ];
        assert_eq!(encoded, expected);
        assert_eq!(
            I2npMessage::decode_standard(&encoded, MAX).unwrap(),
            message
        );
    }

    #[test]
    fn short_headers_round_trip_without_checksum_or_length() {
        let ssu = I2npMessage::new_short_ssu(0x0102_0304, delivery()).unwrap();
        let ssu_bytes = ssu.encode_short_ssu_to_vec(MAX).unwrap();
        assert_eq!(&ssu_bytes[..5], &[10, 1, 2, 3, 4]);
        assert_eq!(I2npMessage::decode_short_ssu(&ssu_bytes, MAX).unwrap(), ssu);

        let transport = I2npMessage::new_short_transport(7, 0x0506_0708, delivery()).unwrap();
        let transport_bytes = transport.encode_short_transport_to_vec(MAX).unwrap();
        assert_eq!(&transport_bytes[..9], &[10, 0, 0, 0, 7, 5, 6, 7, 8]);
        assert_eq!(
            I2npMessage::decode_short_transport(&transport_bytes, MAX).unwrap(),
            transport
        );
    }

    #[test]
    fn standard_header_rejects_checksum_truncation_and_trailing_bytes() {
        let message = I2npMessage::new_standard(7, Date::from_millis(9), delivery()).unwrap();
        let encoded = message.encode_standard_to_vec(MAX).unwrap();
        for end in 0..encoded.len() {
            assert!(I2npMessage::decode_standard(&encoded[..end], MAX).is_err());
        }
        let mut bad_checksum = encoded.clone();
        bad_checksum[15] ^= 1;
        assert!(matches!(
            I2npMessage::decode_standard(&bad_checksum, MAX),
            Err(CodecError::InvalidFieldValue {
                context: "I2NP payload checksum",
                ..
            })
        ));
        let mut trailing = encoded;
        trailing.push(0);
        assert!(matches!(
            I2npMessage::decode_standard(&trailing, MAX),
            Err(CodecError::TrailingBytes { .. })
        ));
    }

    #[test]
    fn search_reply_is_bounded_and_round_trips() {
        let body = I2npBody::DatabaseSearchReply(DatabaseSearchReplyMessage {
            key: hash(1),
            peer_hashes: vec![hash(2), hash(3)],
            from: hash(4),
        });
        let message = I2npMessage::new_standard(8, Date::from_millis(9), body).unwrap();
        let encoded = message.encode_standard_to_vec(MAX).unwrap();
        assert_eq!(
            I2npMessage::decode_standard(&encoded, MAX).unwrap(),
            message
        );

        let too_many = I2npBody::DatabaseSearchReply(DatabaseSearchReplyMessage {
            key: hash(1),
            peer_hashes: vec![hash(2); MAX_DATABASE_SEARCH_REPLY_PEERS + 1],
            from: hash(4),
        });
        assert!(matches!(
            I2npMessage::new_standard(8, Date::from_millis(9), too_many),
            Err(CodecError::LengthExceeded { .. })
        ));
    }

    #[test]
    fn database_lookup_rejects_unsupported_dh_mode() {
        let mut body = vec![0_u8; 64];
        body.extend_from_slice(&[0x12, 0, 0]);
        let result = decode_body(MessageType::DatabaseLookup, &body, MAX);
        assert!(matches!(result, Err(CodecError::Unsupported { .. })));
    }

    #[test]
    fn tunnel_data_and_build_records_validate_fixed_shapes() {
        let mut payload = vec![0, 0, 0, 1];
        payload.extend_from_slice(&[0xaa; TUNNEL_DATA_PAYLOAD_SIZE]);
        let body = decode_body(MessageType::TunnelData, &payload, MAX).unwrap();
        assert!(matches!(body, I2npBody::TunnelData(_)));

        let mut invalid = payload;
        invalid.pop();
        assert!(matches!(
            decode_body(MessageType::TunnelData, &invalid, MAX),
            Err(CodecError::Truncated { .. })
        ));

        let variable = [&[1_u8][..], &[0xbb; VARIABLE_BUILD_RECORD_SIZE][..]].concat();
        let body = decode_body(MessageType::VariableTunnelBuild, &variable, MAX).unwrap();
        assert!(matches!(body, I2npBody::VariableTunnelBuild(_)));
    }

    #[test]
    fn deferred_payload_debug_redacts_bytes() {
        let payload = DeferredPayload::new(vec![0x42; 4], 4).unwrap();
        let rendered = format!("{payload:?}");
        assert!(rendered.contains("length"));
        assert!(!rendered.contains("66"));
    }

    fn fixture_bytes(input: &str) -> Vec<u8> {
        input
            .split_whitespace()
            .flat_map(|pair| {
                let value = u8::from_str_radix(pair, 16).unwrap();
                std::iter::once(value)
            })
            .collect()
    }

    #[test]
    fn committed_fixture_vectors_are_decoded_and_mutation_is_rejected() {
        let valid = fixture_bytes(include_str!(
            "../../../../tests/fixtures/i2np/standard-delivery-status.hex"
        ));
        let malformed = fixture_bytes(include_str!(
            "../../../../tests/fixtures/i2np/malformed-checksum.hex"
        ));
        assert_eq!(
            I2npMessage::decode_standard(&valid, MAX)
                .unwrap()
                .header()
                .message_type(),
            MessageType::DeliveryStatus
        );
        assert!(matches!(
            I2npMessage::decode_standard(&malformed, MAX),
            Err(CodecError::InvalidFieldValue {
                context: "I2NP payload checksum",
                ..
            })
        ));
    }
}
