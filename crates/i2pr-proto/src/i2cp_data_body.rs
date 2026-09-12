//! Plan 192 — i2pd-compatible I2CP-style Data body codec.
//!
//! The codec emits and parses the wire layout that
//! `i2pd::client::ClientDestination::HandleDataMessage`
//! (`/tmp/i2pd-src/libi2pd/Destination.cpp:1192-1236`) and
//! `i2pd::datagram::DatagramDestination::CreateDataMessage`
//! (`/tmp/i2pd-src/libi2pd/Datagram.cpp:455-481`)
//! exchange for SAM `STYLE=RAW` / `STYLE=DATAGRAM VERSION=3`
//! sessions and for any I2P/I2CP client that targets an ECIES-only
//! sender.
//!
//! ## Wire layout
//!
//! The codec produces / consumes the bytes that appear immediately
//! after the I2NP `Data` (type 20) payload-length prefix; i.e. the
//! "Data body" i2pd's `HandleDataMessage` parses as an I2CP payload:
//!
//! ```text
//! offset 0..4    length (BE u32)  — bytes from offset 4 onwards
//!                                 (i2pd's writer writes the gzip
//!                                 member size here).
//! offset 4..8    reserved/garbage — i2pd writes the gzip magic
//!                                 (0x1f 0x8b) + deflate method
//!                                 (0x08) + FLG (0x00) here when it
//!                                 patches the trailing fields via
//!                                 the "patch the gzip header" trick
//!                                 (Destination.cpp reader at +8,
//!                                 writer at `+4` on a pre-bumped
//!                                 buffer).
//! offset 8..10   fromPort (BE u16) — i2pd's `CreateDataMessage`
//!                                 writes these into the gzip MTIME
//!                                 field; the inflater ignores them
//!                                 because MTIME is informational.
//! offset 10..12  toPort (BE u16)   — second half of gzip MTIME
//!                                 in the wire format.
//! offset 12      padding (1 byte)  — gzip XFL (0x02 = "maximum
//!                                 compression"); preserved as the
//!                                 I2CP "padding" byte.
//! offset 13      protocol (1 byte) — gzip OS; i2pd overwrites it
//!                                 with one of PROTOCOL_TYPE_*.
//! offset 14      0x01 (BFINAL=1 + BTYPE=00 + 5 padding bits =
//!                                 the start of the RFC 1951 stored
//!                                 deflate block — i2pd bundles
//!                                 this byte into its 11-byte gzip
//!                                 header constant).
//! offset 15..17  LEN (LE u16)      — inner payload length
//!                                 (i.e. application payload size).
//! offset 17..19  NLEN (LE u16)     — bitwise complement of LEN.
//! offset 19..    gzip-no-compression-wrapped application payload
//!                                 = inner payload (LEN bytes)
//!                                 + CRC32 (LE u32)
//!                                 + ISIZE (LE u32)
//! ```
//!
//! The total encoded length is `I2CP_DATA_BODY_TOTAL_OVERHEAD + N`
//! (= 27 + N bytes); the I2CP length prefix carries `23 + N` because
//! it counts only the bytes from offset 4 onwards.
//!
//! The codec is fully deterministic, owns no runtime state, and never
//! logs payload bytes.

#![forbid(unsafe_code)]

use crate::codec::{CodecError, DecodeCursor};

/// `PROTOCOL_TYPE_STREAMING` — i2pd's I2CP data protocol constant for
/// Streaming messages (used by `StreamingDestination::HandleDataMessagePayload`).
pub const PROTOCOL_TYPE_STREAMING: u8 = 6;
/// `PROTOCOL_TYPE_DATAGRAM` — i2pd's I2CP data protocol constant for
/// legacy DATAGRAM (version 1) sessions. Requires a 384-byte
/// ElGamal/DSA `from` identity; ECIES-only senders cannot use this.
pub const PROTOCOL_TYPE_DATAGRAM: u8 = 17;
/// `PROTOCOL_TYPE_RAW` — i2pd's I2CP data protocol constant for
/// `STYLE=RAW` SAM sessions. Does not require a `from` identity; an
/// ECIES-only sender can use this. The matching SAM wire prefix is
/// `RAW RECEIVED SIZE=%lu\n`.
pub const PROTOCOL_TYPE_RAW: u8 = 18;
/// `PROTOCOL_TYPE_DATAGRAM2` — DATAGRAM version 2 (signed-only);
/// requires an ElGamal/DSA `from` identity. ECIES-only senders cannot
/// use this.
pub const PROTOCOL_TYPE_DATAGRAM2: u8 = 19;
/// `PROTOCOL_TYPE_DATAGRAM3` — DATAGRAM version 3; uses the inbound
/// destination hash instead of an ElGamal/DSA `from` identity.
pub const PROTOCOL_TYPE_DATAGRAM3: u8 = 20;

/// Reserved byte at offset 12 — i2pd reuses the gzip header's `XFL`
/// field (0x02 = "maximum compression") as the I2CP padding byte.
pub const I2CP_DATA_BODY_PADDING: u8 = 0x02;

/// Hard ceiling on the inner application payload that the codec will
/// accept. The encoder rejects anything larger; the decoder rejects
/// inputs whose declared payload length exceeds this bound. Matches
/// `MAX_I2NP_PAYLOAD_SIZE` minus the fixed header overhead so a single
/// payload fits inside one I2NP Data message.
pub const MAX_I2CP_DATA_BODY_PAYLOAD: usize = 61_440;

/// Fixed header overhead of an I2CP Data body that does NOT include
/// the variable application payload length:
///
/// - 4 bytes length (BE)
/// - 4 bytes reserved (gzip magic + CM + FLG)
/// - 2 bytes fromPort (BE)
/// - 2 bytes toPort (BE)
/// - 1 byte padding
/// - 1 byte protocol
/// - 1 byte 0x01 (BFINAL=1 + BTYPE=00 + 5 padding bits)
/// - 2 bytes LEN (LE)
/// - 2 bytes NLEN (LE)
///
/// = 19 bytes.
pub const I2CP_DATA_BODY_HEADER_OVERHEAD: usize = 19;

/// Trailing bytes after the inner payload:
///
/// - 4 bytes CRC32 (LE)
/// - 4 bytes ISIZE (LE)
///
/// = 8 bytes.
pub const I2CP_DATA_BODY_TRAILER_OVERHEAD: usize = 8;

/// Total fixed overhead = header + trailer.
pub const I2CP_DATA_BODY_TOTAL_OVERHEAD: usize =
    I2CP_DATA_BODY_HEADER_OVERHEAD + I2CP_DATA_BODY_TRAILER_OVERHEAD;

/// Typed i2pd-compatible I2CP Data body fields. The codec treats
/// `from_port` and `to_port` as informational (the wire format
/// preserves them inside the gzip MTIME field; the inflater ignores
/// them) and `protocol` as the dispatch selector.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct I2cpDataBody {
    /// Source port (informational).
    pub from_port: u16,
    /// Destination port (informational).
    pub to_port: u16,
    /// Protocol byte. Must match one of the i2pd `PROTOCOL_TYPE_*`
    /// constants. The decoder rejects any other value because i2pd's
    /// `HandleDataMessage` switch statement only handles the five
    /// documented constants.
    pub protocol: u8,
    /// Application payload bytes (the inflater output of the
    /// gzip-no-compression member).
    pub payload: Vec<u8>,
}

/// Errors surfaced by [`decode_i2cp_data_body`].
#[derive(Debug)]
pub enum I2cpDataBodyDecodeError {
    /// The input is shorter than the minimum I2CP Data body header.
    Truncated,
    /// The declared length exceeds the supplied input.
    DeclaredLengthOverflow {
        /// Declared length (from the I2CP length prefix).
        declared: usize,
        /// Available bytes after the length prefix.
        available: usize,
    },
    /// The declared length exceeds [`MAX_I2CP_DATA_BODY_PAYLOAD`]
    /// plus the fixed overhead.
    PayloadTooLarge {
        /// Inner payload length.
        declared: usize,
        /// Local ceiling.
        maximum: usize,
    },
    /// The protocol byte does not match any i2pd-documented
    /// `PROTOCOL_TYPE_*` constant.
    UnknownProtocol {
        /// Observed protocol byte.
        observed: u8,
    },
    /// The gzip member magic is wrong.
    BadGzipMagic,
    /// The gzip member compression method is not deflate.
    UnsupportedCompressionMethod {
        /// Observed compression method.
        observed: u8,
    },
    /// The gzip member flags carry a reserved bit that the codec
    /// refuses to interpret.
    ReservedGzipFlagBits {
        /// Observed FLG byte.
        observed: u8,
    },
    /// The gzip member advertises an optional-header layout
    /// (`FTEXT` / `FHCRC` / `FEXTRA` / `FNAME` / `FCOMMENT`) that
    /// this stripped-down inflater does not parse. i2pd's writer
    /// always sets FLG = 0, so any nonzero bit is a wire defect.
    UnsupportedOptionalHeaderLayout {
        /// Observed layout label.
        label: &'static str,
    },
    /// The CRC32 trailer does not match the recovered payload.
    Crc32Mismatch {
        /// Declared CRC32.
        declared: u32,
        /// Computed CRC32.
        actual: u32,
    },
    /// The ISIZE trailer does not match the recovered payload
    /// length.
    IsizeMismatch {
        /// Declared ISIZE.
        declared: u32,
        /// Actual payload length.
        actual: usize,
    },
    /// The stored-block LEN header does not match the inner payload
    /// length derived from the gzip trailer.
    StoredBlockLengthMismatch {
        /// LEN from the stored-block header.
        declared: u16,
        /// Inner payload length derived from the ISIZE trailer.
        actual: usize,
    },
    /// The stored-block BFINAL/BTYPE byte is not the canonical
    /// "last stored block" marker.
    BadStoredBlockHeader {
        /// Observed header byte.
        observed: u8,
    },
    /// A codec error from the underlying length/structure codec.
    Codec(CodecError),
}

impl core::fmt::Display for I2cpDataBodyDecodeError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated => formatter.write_str("I2CP Data body is truncated"),
            Self::DeclaredLengthOverflow {
                declared,
                available,
            } => write!(
                formatter,
                "I2CP Data body declared length {declared} exceeds available {available} bytes"
            ),
            Self::PayloadTooLarge { declared, maximum } => write!(
                formatter,
                "I2CP Data body inner payload {declared} exceeds local ceiling {maximum}"
            ),
            Self::UnknownProtocol { observed } => {
                write!(formatter, "I2CP Data body unknown protocol {observed}")
            }
            Self::BadGzipMagic => formatter.write_str("I2CP Data body gzip magic mismatch"),
            Self::UnsupportedCompressionMethod { observed } => write!(
                formatter,
                "I2CP Data body unsupported gzip compression method {observed}"
            ),
            Self::ReservedGzipFlagBits { observed } => write!(
                formatter,
                "I2CP Data body gzip FLG reserved bits set: {observed:#04x}"
            ),
            Self::UnsupportedOptionalHeaderLayout { label } => write!(
                formatter,
                "I2CP Data body unsupported gzip optional-header layout {label}"
            ),
            Self::Crc32Mismatch { declared, actual } => write!(
                formatter,
                "I2CP Data body CRC32 mismatch (declared {declared:#010x}, computed {actual:#010x})"
            ),
            Self::IsizeMismatch { declared, actual } => write!(
                formatter,
                "I2CP Data body ISIZE mismatch (declared {declared}, actual {actual})"
            ),
            Self::StoredBlockLengthMismatch { declared, actual } => write!(
                formatter,
                "I2CP Data body stored-block LEN mismatch (declared {declared}, actual {actual})"
            ),
            Self::BadStoredBlockHeader { observed } => write!(
                formatter,
                "I2CP Data body stored-block header byte {observed:#04x} is not 0x01"
            ),
            Self::Codec(error) => write!(formatter, "I2CP Data body codec: {error}"),
        }
    }
}

impl std::error::Error for I2cpDataBodyDecodeError {}

/// Errors surfaced by [`encode_i2cp_data_body`].
#[derive(Debug)]
pub enum I2cpDataBodyEncodeError {
    /// The supplied application payload exceeds
    /// [`MAX_I2CP_DATA_BODY_PAYLOAD`].
    PayloadTooLarge {
        /// Observed payload length.
        actual: usize,
        /// Local ceiling.
        maximum: usize,
    },
    /// The supplied protocol byte does not match any i2pd-documented
    /// `PROTOCOL_TYPE_*` constant.
    UnknownProtocol {
        /// Observed protocol byte.
        observed: u8,
    },
}

impl core::fmt::Display for I2cpDataBodyEncodeError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::PayloadTooLarge { actual, maximum } => write!(
                formatter,
                "I2CP Data body payload {actual} exceeds local ceiling {maximum}"
            ),
            Self::UnknownProtocol { observed } => {
                write!(formatter, "I2CP Data body unknown protocol byte {observed}")
            }
        }
    }
}

impl std::error::Error for I2cpDataBodyEncodeError {}

fn validate_protocol(protocol: u8) -> Result<(), I2cpDataBodyEncodeError> {
    match protocol {
        PROTOCOL_TYPE_STREAMING
        | PROTOCOL_TYPE_DATAGRAM
        | PROTOCOL_TYPE_RAW
        | PROTOCOL_TYPE_DATAGRAM2
        | PROTOCOL_TYPE_DATAGRAM3 => Ok(()),
        other => Err(I2cpDataBodyEncodeError::UnknownProtocol { observed: other }),
    }
}

fn reserved_gzip_flag_value() -> u8 {
    // i2pd's writer writes [0x1f, 0x8b, 0x08, 0x00] at offsets 4..8
    // (gzip magic + CM + FLG). FLG = 0 means no optional headers.
    0x00
}

/// Encodes one application payload into the i2pd-compatible I2CP
/// Data body wire format. The returned bytes are ready to be wrapped
/// in an `I2npBody::Data(OpaqueMessageBody { payload })` envelope
/// (and, per Plan 192, that envelope itself is wrapped in the
/// 9-byte NTCP2/SSU2 short-transport I2NP header that i2pd parses
/// inside an ECIES-X25519 Garlic clove).
///
/// The returned vector is exactly `I2CP_DATA_BODY_TOTAL_OVERHEAD +
/// payload.len()` (= 27 + N) bytes long.
pub fn encode_i2cp_data_body(
    from_port: u16,
    to_port: u16,
    protocol: u8,
    payload: &[u8],
) -> Result<Vec<u8>, I2cpDataBodyEncodeError> {
    if payload.len() > MAX_I2CP_DATA_BODY_PAYLOAD {
        return Err(I2cpDataBodyEncodeError::PayloadTooLarge {
            actual: payload.len(),
            maximum: MAX_I2CP_DATA_BODY_PAYLOAD,
        });
    }
    validate_protocol(protocol)?;
    let n = payload.len();
    // The I2CP length prefix counts the bytes from offset 4 onwards
    // (= i2pd's gzip member size = 23 + N for `GzipNoCompression`).
    let payload_length_field: u32 = u32::try_from(I2CP_DATA_BODY_TOTAL_OVERHEAD - 4 + n)
        .expect("I2CP Data body length always fits in u32");
    let crc = crc32(payload);
    let len_le = u16::try_from(n).expect("N fits in u16");
    let nlen_le = u16::wrapping_sub(0xffff, len_le);
    let mut out = Vec::with_capacity(I2CP_DATA_BODY_TOTAL_OVERHEAD + n);
    out.extend_from_slice(&payload_length_field.to_be_bytes());
    out.extend_from_slice(&[
        0x1f,
        0x8b,
        0x08,
        reserved_gzip_flag_value(),
        (from_port >> 8) as u8,
        from_port as u8,
        (to_port >> 8) as u8,
        to_port as u8,
        I2CP_DATA_BODY_PADDING,
        protocol,
        0x01,
    ]);
    out.extend_from_slice(&len_le.to_le_bytes());
    out.extend_from_slice(&nlen_le.to_le_bytes());
    out.extend_from_slice(payload);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&u32::try_from(n).expect("N fits in u32").to_le_bytes());
    debug_assert_eq!(out.len(), I2CP_DATA_BODY_TOTAL_OVERHEAD + n);
    debug_assert_eq!(
        u32::from_be_bytes([out[0], out[1], out[2], out[3]]) as usize,
        I2CP_DATA_BODY_TOTAL_OVERHEAD - 4 + n
    );
    Ok(out)
}

/// Decodes one i2pd-compatible I2CP Data body into its typed
/// fields. The function validates the gzip magic, rejects any
/// gzip FLG bits set (i2pd's writer always sets FLG = 0x00), parses
/// the stored-block header, and verifies the CRC32 + ISIZE trailer.
/// Unknown protocol bytes are rejected because i2pd's reader's
/// switch statement only handles the five documented constants.
pub fn decode_i2cp_data_body(input: &[u8]) -> Result<I2cpDataBody, I2cpDataBodyDecodeError> {
    if input.len() < I2CP_DATA_BODY_HEADER_OVERHEAD {
        return Err(I2cpDataBodyDecodeError::Truncated);
    }
    let mut cursor =
        DecodeCursor::new(input, input.len()).map_err(I2cpDataBodyDecodeError::Codec)?;
    let length = cursor.read_u32().map_err(I2cpDataBodyDecodeError::Codec)? as usize;
    let available = cursor.remaining();
    if length > available {
        return Err(I2cpDataBodyDecodeError::DeclaredLengthOverflow {
            declared: length,
            available,
        });
    }
    // length encodes the gzip member starting at offset 4; the gzip
    // member is `length` bytes long. The trailing `input.len() - 4 -
    // length` bytes (if any) are i2pd's "reserved" padding tail and
    // must be zero on the wire; we accept trailing zeros but reject
    // any nonzero trailing byte because the i2pd writer always
    // produces exactly `4 + length` bytes.
    let gzip_member_end = 4 + length;
    if input.len() > gzip_member_end {
        for byte in &input[gzip_member_end..] {
            if *byte != 0 {
                return Err(I2cpDataBodyDecodeError::Codec(CodecError::TrailingBytes {
                    offset: gzip_member_end,
                    remaining: input.len() - gzip_member_end,
                }));
            }
        }
    }
    if length < I2CP_DATA_BODY_HEADER_OVERHEAD - 4 + I2CP_DATA_BODY_TRAILER_OVERHEAD {
        return Err(I2cpDataBodyDecodeError::Truncated);
    }
    let payload_length = length
        .checked_sub((I2CP_DATA_BODY_HEADER_OVERHEAD - 4) + I2CP_DATA_BODY_TRAILER_OVERHEAD)
        .ok_or(I2cpDataBodyDecodeError::Truncated)?;
    if payload_length > MAX_I2CP_DATA_BODY_PAYLOAD {
        return Err(I2cpDataBodyDecodeError::PayloadTooLarge {
            declared: payload_length,
            maximum: MAX_I2CP_DATA_BODY_PAYLOAD,
        });
    }
    let gzip_start = 4_usize;
    if input[gzip_start] != 0x1f || input[gzip_start + 1] != 0x8b {
        return Err(I2cpDataBodyDecodeError::BadGzipMagic);
    }
    if input[gzip_start + 2] != 0x08 {
        return Err(I2cpDataBodyDecodeError::UnsupportedCompressionMethod {
            observed: input[gzip_start + 2],
        });
    }
    let flags = input[gzip_start + 3];
    if flags & 0b0000_0001 != 0 {
        return Err(I2cpDataBodyDecodeError::UnsupportedOptionalHeaderLayout { label: "FTEXT" });
    }
    if flags & 0b0000_0010 != 0 {
        return Err(I2cpDataBodyDecodeError::UnsupportedOptionalHeaderLayout { label: "FHCRC" });
    }
    if flags & 0b0000_0100 != 0 {
        return Err(I2cpDataBodyDecodeError::UnsupportedOptionalHeaderLayout { label: "FEXTRA" });
    }
    if flags & 0b0000_1000 != 0 {
        return Err(I2cpDataBodyDecodeError::UnsupportedOptionalHeaderLayout { label: "FNAME" });
    }
    if flags & 0b1_0000 != 0 {
        return Err(I2cpDataBodyDecodeError::UnsupportedOptionalHeaderLayout { label: "FCOMMENT" });
    }
    if flags & 0b1110_0000 != 0 {
        return Err(I2cpDataBodyDecodeError::ReservedGzipFlagBits { observed: flags });
    }
    let from_port = u16::from_be_bytes([input[gzip_start + 4], input[gzip_start + 5]]);
    let to_port = u16::from_be_bytes([input[gzip_start + 6], input[gzip_start + 7]]);
    let _padding = input[gzip_start + 8];
    let protocol = input[gzip_start + 9];
    match protocol {
        PROTOCOL_TYPE_STREAMING
        | PROTOCOL_TYPE_DATAGRAM
        | PROTOCOL_TYPE_RAW
        | PROTOCOL_TYPE_DATAGRAM2
        | PROTOCOL_TYPE_DATAGRAM3 => {}
        other => {
            return Err(I2cpDataBodyDecodeError::UnknownProtocol { observed: other });
        }
    }
    let stored_header = input[gzip_start + 10];
    if stored_header != 0x01 {
        return Err(I2cpDataBodyDecodeError::BadStoredBlockHeader {
            observed: stored_header,
        });
    }
    let stored_len = u16::from_le_bytes([input[gzip_start + 11], input[gzip_start + 12]]);
    let stored_nlen = u16::from_le_bytes([input[gzip_start + 13], input[gzip_start + 14]]);
    if u16::wrapping_sub(0xffff, stored_len) != stored_nlen {
        return Err(I2cpDataBodyDecodeError::StoredBlockLengthMismatch {
            declared: stored_len,
            actual: payload_length,
        });
    }
    if stored_len as usize != payload_length {
        return Err(I2cpDataBodyDecodeError::StoredBlockLengthMismatch {
            declared: stored_len,
            actual: payload_length,
        });
    }
    let payload_start = gzip_start + 15;
    let payload_end = payload_start + payload_length;
    let payload = input[payload_start..payload_end].to_vec();
    let trailer_crc = u32::from_le_bytes([
        input[payload_end],
        input[payload_end + 1],
        input[payload_end + 2],
        input[payload_end + 3],
    ]);
    let trailer_isize = u32::from_le_bytes([
        input[payload_end + 4],
        input[payload_end + 5],
        input[payload_end + 6],
        input[payload_end + 7],
    ]);
    if trailer_isize != payload_length as u32 {
        return Err(I2cpDataBodyDecodeError::IsizeMismatch {
            declared: trailer_isize,
            actual: payload_length,
        });
    }
    let computed_crc = crc32(&payload);
    if computed_crc != trailer_crc {
        return Err(I2cpDataBodyDecodeError::Crc32Mismatch {
            declared: trailer_crc,
            actual: computed_crc,
        });
    }
    Ok(I2cpDataBody {
        from_port,
        to_port,
        protocol,
        payload,
    })
}

/// Encodes one application payload into the i2pd-compatible I2NP
/// `Data` body in the same shape i2pd writes inside ECIES-X25519
/// Garlic cloves: a 9-byte NTCP2/SSU2 short-transport header whose
/// payload is the bytes returned by [`encode_i2cp_data_body`].
///
/// Per Plan 192, this is the canonical outbound destination message
/// carrier. i2pd's `ECIESX25519AEADRatchetSession::HandleECIESX25519GarlicClove`
/// (`/tmp/i2pd-src/libi2pd/Garlic.cpp:1023-1028`) parses a 9-byte
/// short-transport header inside ECIES cloves; the prior 16-byte
/// standard header is misread as part of the inner Data payload and
/// causes i2pd to log `Data message length ... exceeds buffer
/// length ...` and return without dispatching the message to the
/// `ClientDestination::HandleDataMessage` path that reaches the SAM
/// bridge.
pub fn encode_destination_data_envelope(
    from_port: u16,
    to_port: u16,
    protocol: u8,
    message_id: u32,
    expiration_seconds: u32,
    payload: &[u8],
) -> Result<crate::I2npMessage, I2cpDataBodyEncodeError> {
    let body_bytes = encode_i2cp_data_body(from_port, to_port, protocol, payload)?;
    let body = crate::I2npBody::Data(crate::OpaqueMessageBody {
        payload: crate::DeferredPayload::new(body_bytes, crate::MAX_I2NP_PAYLOAD_SIZE).map_err(
            |_| I2cpDataBodyEncodeError::PayloadTooLarge {
                actual: payload.len(),
                maximum: MAX_I2CP_DATA_BODY_PAYLOAD,
            },
        )?,
    });
    crate::I2npMessage::new_short_transport(message_id, expiration_seconds, body).map_err(|_| {
        I2cpDataBodyEncodeError::PayloadTooLarge {
            actual: payload.len(),
            maximum: MAX_I2CP_DATA_BODY_PAYLOAD,
        }
    })
}

/// CRC-32 (IEEE 802.3, "gzip" polynomial 0xedb8_8320) over the
/// supplied bytes. Matches the CRC-32 that i2pd's
/// `GzipNoCompression` writer computes for the gzip trailer and the
/// CRC-32 the Java I2P client payload inflater verifies.
///
/// Implementation mirrors the table-driven CRC-32 in
/// `crates/i2pr-proto/src/streaming/payload.rs::crc32`; kept here
/// to avoid making the i2cp_data_body module depend on the streaming
/// module's private helpers.
fn crc32(bytes: &[u8]) -> u32 {
    let mut table = [0_u32; 256];
    for index in 0..256_u32 {
        let mut acc = index;
        for _ in 0..8 {
            acc = if acc & 1 != 0 {
                0xedb8_8320 ^ (acc >> 1)
            } else {
                acc >> 1
            };
        }
        table[index as usize] = acc;
    }
    let mut acc: u32 = 0xffff_ffff;
    for byte in bytes {
        let index = ((acc ^ u32::from(*byte)) & 0xff) as usize;
        acc = (acc >> 8) ^ table[index];
    }
    acc ^ 0xffff_ffff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_for_each_protocol_recovers_original_payload() {
        for protocol in [
            PROTOCOL_TYPE_STREAMING,
            PROTOCOL_TYPE_DATAGRAM,
            PROTOCOL_TYPE_RAW,
            PROTOCOL_TYPE_DATAGRAM2,
            PROTOCOL_TYPE_DATAGRAM3,
        ] {
            let payload = b"plan192-i2cp-data-body-payload";
            let encoded = encode_i2cp_data_body(0x1234, 0xabcd, protocol, payload).expect("encode");
            let decoded = decode_i2cp_data_body(&encoded).expect("decode");
            assert_eq!(decoded.protocol, protocol);
            assert_eq!(decoded.from_port, 0x1234);
            assert_eq!(decoded.to_port, 0xabcd);
            assert_eq!(decoded.payload, payload);
        }
    }

    #[test]
    fn encoded_body_matches_i2pd_offset_layout() {
        // i2pd's writer writes [0x1f, 0x8b, 0x08, 0x00] at offsets 4..8
        // (gzip magic + CM + FLG), then fromPort (BE), toPort (BE),
        // padding (= gzip XFL = 0x02), and protocol (= gzip OS). The
        // 11th byte of the 11-byte gzip header constant is the start
        // of the stored deflate block (BFINAL=1, BTYPE=00, 5
        // padding bits = 0x01).
        let payload = b"x";
        let encoded =
            encode_i2cp_data_body(0x1122, 0x3344, PROTOCOL_TYPE_RAW, payload).expect("encode");
        assert_eq!(encoded.len(), I2CP_DATA_BODY_TOTAL_OVERHEAD + payload.len());
        let payload_length_field = (encoded.len() - 4) as u32;
        assert_eq!(encoded[0..4], payload_length_field.to_be_bytes());
        assert_eq!(encoded[4], 0x1f);
        assert_eq!(encoded[5], 0x8b);
        assert_eq!(encoded[6], 0x08);
        assert_eq!(encoded[7], 0x00);
        assert_eq!(encoded[8..10], 0x1122_u16.to_be_bytes());
        assert_eq!(encoded[10..12], 0x3344_u16.to_be_bytes());
        assert_eq!(encoded[12], I2CP_DATA_BODY_PADDING);
        assert_eq!(encoded[13], PROTOCOL_TYPE_RAW);
        assert_eq!(encoded[14], 0x01);
        let stored_len = u16::from_le_bytes([encoded[15], encoded[16]]);
        assert_eq!(stored_len as usize, payload.len());
        let stored_nlen = u16::from_le_bytes([encoded[17], encoded[18]]);
        assert_eq!(u16::wrapping_sub(0xffff, stored_len), stored_nlen);
        let crc = u32::from_le_bytes([
            encoded[encoded.len() - 8],
            encoded[encoded.len() - 7],
            encoded[encoded.len() - 6],
            encoded[encoded.len() - 5],
        ]);
        let isize = u32::from_le_bytes([
            encoded[encoded.len() - 4],
            encoded[encoded.len() - 3],
            encoded[encoded.len() - 2],
            encoded[encoded.len() - 1],
        ]);
        assert_eq!(crc, crc32(payload));
        assert_eq!(isize, payload.len() as u32);
    }

    #[test]
    fn decode_rejects_truncated_input() {
        let encoded = encode_i2cp_data_body(0, 0, PROTOCOL_TYPE_RAW, b"hello").expect("encode");
        for truncate_at in 0..encoded.len() {
            let error = decode_i2cp_data_body(&encoded[..truncate_at])
                .expect_err(&format!("truncated at {truncate_at} must reject"));
            assert!(
                matches!(
                    error,
                    I2cpDataBodyDecodeError::Truncated
                        | I2cpDataBodyDecodeError::DeclaredLengthOverflow { .. }
                ),
                "truncate_at={truncate_at} got {error:?}"
            );
        }
    }

    #[test]
    fn decode_rejects_crc32_corruption() {
        let mut encoded =
            encode_i2cp_data_body(0, 0, PROTOCOL_TYPE_RAW, b"hello-world").expect("encode");
        let crc_offset = encoded.len() - 8;
        encoded[crc_offset] ^= 0x01;
        let error = decode_i2cp_data_body(&encoded).expect_err("corrupted CRC must reject");
        assert!(matches!(
            error,
            I2cpDataBodyDecodeError::Crc32Mismatch { .. }
        ));
    }

    #[test]
    fn decode_rejects_isize_corruption() {
        let mut encoded =
            encode_i2cp_data_body(0, 0, PROTOCOL_TYPE_RAW, b"hello-world").expect("encode");
        let isize_offset = encoded.len() - 4;
        encoded[isize_offset] ^= 0x01;
        let error = decode_i2cp_data_body(&encoded).expect_err("corrupted ISIZE must reject");
        assert!(matches!(
            error,
            I2cpDataBodyDecodeError::IsizeMismatch { .. }
        ));
    }

    #[test]
    fn decode_rejects_unknown_protocol() {
        let mut encoded = encode_i2cp_data_body(0, 0, PROTOCOL_TYPE_RAW, b"x").expect("encode");
        encoded[13] = 0xFE;
        let error = decode_i2cp_data_body(&encoded).expect_err("unknown protocol must reject");
        assert!(matches!(
            error,
            I2cpDataBodyDecodeError::UnknownProtocol { observed: 0xFE }
        ));
    }

    #[test]
    fn encode_rejects_unknown_protocol() {
        let error = encode_i2cp_data_body(0, 0, 0xFE, b"x").expect_err("unknown protocol");
        assert!(matches!(
            error,
            I2cpDataBodyEncodeError::UnknownProtocol { observed: 0xFE }
        ));
    }

    #[test]
    fn encode_rejects_oversize_payload() {
        let payload = vec![0u8; MAX_I2CP_DATA_BODY_PAYLOAD + 1];
        let error =
            encode_i2cp_data_body(0, 0, PROTOCOL_TYPE_RAW, &payload).expect_err("oversize payload");
        assert!(matches!(
            error,
            I2cpDataBodyEncodeError::PayloadTooLarge { .. }
        ));
    }

    #[test]
    fn decode_rejects_oversize_declared_payload() {
        // Build a body large enough that the decoder reaches the
        // payload-length bound check instead of failing on the
        // declared-length-vs-input bound first.
        let declared_payload = (MAX_I2CP_DATA_BODY_PAYLOAD + 1) as u32;
        let declared_length = declared_payload
            + (I2CP_DATA_BODY_HEADER_OVERHEAD - 4) as u32
            + I2CP_DATA_BODY_TRAILER_OVERHEAD as u32;
        let total = (declared_length + 4) as usize;
        let mut bytes = vec![0u8; total];
        bytes[0..4].copy_from_slice(&declared_length.to_be_bytes());
        bytes[4] = 0x1f;
        bytes[5] = 0x8b;
        bytes[6] = 0x08;
        bytes[7] = 0x00;
        bytes[13] = PROTOCOL_TYPE_RAW;
        let error = decode_i2cp_data_body(&bytes).expect_err("oversize declared");
        assert!(matches!(
            error,
            I2cpDataBodyDecodeError::PayloadTooLarge { .. }
        ));
    }

    #[test]
    fn empty_payload_round_trips() {
        let encoded = encode_i2cp_data_body(0, 0, PROTOCOL_TYPE_RAW, b"").expect("encode");
        assert_eq!(encoded.len(), I2CP_DATA_BODY_TOTAL_OVERHEAD);
        let decoded = decode_i2cp_data_body(&encoded).expect("decode");
        assert_eq!(decoded.protocol, PROTOCOL_TYPE_RAW);
        assert!(decoded.payload.is_empty());
    }

    #[test]
    fn trailing_zero_bytes_are_tolerated() {
        let mut encoded = encode_i2cp_data_body(0, 0, PROTOCOL_TYPE_RAW, b"hi").expect("encode");
        encoded.extend_from_slice(&[0u8; 4]);
        let decoded = decode_i2cp_data_body(&encoded).expect("decode");
        assert_eq!(decoded.payload, b"hi");
    }

    #[test]
    fn envelope_encodes_short_transport_inner() {
        let envelope =
            encode_destination_data_envelope(0, 0, PROTOCOL_TYPE_RAW, 0xC0FFEE, 60, b"plan192")
                .expect("envelope");
        let encoded = envelope
            .encode_short_transport_to_vec(crate::MAX_I2NP_PAYLOAD_SIZE)
            .expect("encode short transport");
        // 9-byte short-transport header (1 type + 4 message id + 4 expiration)
        // + the i2cp data body.
        assert_eq!(encoded[0], 0x14); // MessageType::Data code
        assert_eq!(
            u32::from_be_bytes([encoded[1], encoded[2], encoded[3], encoded[4]]),
            0xC0FFEE
        );
        let decoded =
            crate::I2npMessage::decode_short_transport(&encoded, crate::MAX_I2NP_PAYLOAD_SIZE)
                .expect("decode short transport");
        assert!(matches!(decoded.body(), crate::I2npBody::Data(_)));
    }
}
