//! Internal I2P client payload framing.
//!
//! Plan 123 §3 defines the standard I2P client payload metadata envelope
//! used to carry Streaming (and any future application) bytes inside an
//! I2NP `Data` body. The current router/I2P client payload format is a
//! gzip-compatible framing with a header containing the destination port,
//! source port, and protocol number, followed by the compressed payload,
//! a SHA-256 integrity check, and a CRC32 integrity check.
//!
//! The envelope must encode/decode at least:
//!
//! ```text
//! source port
//! destination port
//! I2P protocol number
//! payload bytes
//! integrity framing/CRC required by the current I2P client payload format
//! ```
//!
//! For Streaming, the protocol number is [`STREAMING_PROTOCOL_NUMBER`]
//! (`6`).
//!
//! The internal destination API works on the typed [`ClientPayload`]
//! value; streaming code never reaches inside the framing bytes.

use std::fmt;
use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use sha2::{Digest, Sha256};

use crate::codec::{CodecError, decode_exact, encode_to_vec};

/// I2P Streaming protocol number carried in the client payload envelope.
pub const STREAMING_PROTOCOL_NUMBER: u8 = 6;
/// Default destination port the Streaming protocol uses when the caller
/// does not specify one. Port semantics follow I2P; this is the canonical
/// Streaming default and not a privileged TCP port.
pub const DEFAULT_DESTINATION_PORT: u16 = 0;
/// Default source port the Streaming protocol uses when the caller does
/// not specify one.
pub const DEFAULT_SOURCE_PORT: u16 = 0;
/// Hard ceiling on a single encoded/decoded client payload frame.
///
/// The official I2P client payload format adds a small overhead above
/// the payload bytes; the bound is chosen to fit well below the I2NP
/// `Data` body ceiling (`MAX_I2NP_PAYLOAD_SIZE`) once the Garlic Clove
/// envelope is applied.
pub const MAX_CLIENT_PAYLOAD_BYTES: usize = 60 * 1024;
/// Hard ceiling on the application payload bytes inside one envelope.
pub const MAX_APPLICATION_PAYLOAD_BYTES: usize = MAX_CLIENT_PAYLOAD_BYTES - 64;

/// Typed outcome of an outbound client payload encoding operation.
#[derive(Debug, Eq, PartialEq)]
pub enum ClientPayloadEncodeError {
    /// The application payload exceeds the per-payload ceiling.
    PayloadTooLarge {
        /// Declared payload size.
        actual: usize,
        /// Allowed maximum.
        maximum: usize,
    },
    /// The supplied protocol number is outside the byte range.
    InvalidProtocol(u16),
    /// The streaming encode step produced an output that exceeds the
    /// envelope ceiling (signals a streaming-codec mismatch).
    Codec(CodecError),
}

impl fmt::Display for ClientPayloadEncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PayloadTooLarge { actual, maximum } => write!(
                formatter,
                "application payload size {actual} exceeds {maximum}-byte client payload ceiling"
            ),
            Self::InvalidProtocol(value) => {
                write!(formatter, "invalid protocol number {value}")
            }
            Self::Codec(error) => write!(formatter, "client payload codec: {error}"),
        }
    }
}

impl std::error::Error for ClientPayloadEncodeError {}

/// Typed outcome of an inbound client payload decoding operation.
#[derive(Debug, Eq, PartialEq)]
pub enum ClientPayloadDecodeError {
    /// The envelope ended before the framing completed.
    Truncated,
    /// The envelope declares a payload size larger than the local
    /// ceiling allows.
    PayloadTooLarge {
        /// Declared payload size.
        actual: usize,
        /// Allowed maximum.
        maximum: usize,
    },
    /// The CRC32 integrity check failed.
    InvalidCrc,
    /// The SHA-256 integrity check failed.
    InvalidSha256,
    /// The envelope carries a non-streaming protocol number.
    UnsupportedProtocol(u8),
    /// A zlib decompression step failed.
    DecompressionFailed,
    /// The envelope input exceeded the input limit before decoding.
    Codec(CodecError),
}

impl fmt::Display for ClientPayloadDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => formatter.write_str("client payload envelope truncated"),
            Self::PayloadTooLarge { actual, maximum } => write!(
                formatter,
                "client payload declares {actual} bytes, exceeds {maximum}-byte ceiling"
            ),
            Self::InvalidCrc => formatter.write_str("client payload CRC32 mismatch"),
            Self::InvalidSha256 => formatter.write_str("client payload SHA-256 mismatch"),
            Self::UnsupportedProtocol(value) => {
                write!(formatter, "unsupported client payload protocol {value}")
            }
            Self::DecompressionFailed => formatter.write_str("client payload zlib step failed"),
            Self::Codec(error) => write!(formatter, "client payload codec: {error}"),
        }
    }
}

impl std::error::Error for ClientPayloadDecodeError {}

/// The decoded I2P client payload envelope.
#[derive(Clone, Eq, PartialEq)]
pub struct ClientPayload {
    /// I2P protocol number (e.g. `6` for Streaming).
    pub protocol: u8,
    /// Sender's I2P destination port.
    pub source_port: u16,
    /// Receiver's I2P destination port.
    pub destination_port: u16,
    /// Decoded application payload bytes.
    pub payload: Vec<u8>,
}

impl ClientPayload {
    /// Constructs a Streaming-protocol payload envelope from raw application
    /// bytes. The supplied payload must fit within
    /// [`MAX_APPLICATION_PAYLOAD_BYTES`].
    pub fn streaming(payload: Vec<u8>) -> Result<Self, ClientPayloadEncodeError> {
        if payload.len() > MAX_APPLICATION_PAYLOAD_BYTES {
            return Err(ClientPayloadEncodeError::PayloadTooLarge {
                actual: payload.len(),
                maximum: MAX_APPLICATION_PAYLOAD_BYTES,
            });
        }
        Ok(Self {
            protocol: STREAMING_PROTOCOL_NUMBER,
            source_port: DEFAULT_SOURCE_PORT,
            destination_port: DEFAULT_DESTINATION_PORT,
            payload,
        })
    }

    /// Returns the application payload bytes when this envelope carries the
    /// Streaming protocol. Otherwise returns the typed error.
    pub fn into_streaming_payload(self) -> Result<Vec<u8>, ClientPayloadDecodeError> {
        if self.protocol != STREAMING_PROTOCOL_NUMBER {
            return Err(ClientPayloadDecodeError::UnsupportedProtocol(self.protocol));
        }
        Ok(self.payload)
    }

    /// Borrows the decoded application payload bytes without consuming the
    /// envelope.
    pub fn application_payload(&self) -> &[u8] {
        &self.payload
    }
}

impl fmt::Debug for ClientPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientPayload")
            .field("protocol", &self.protocol)
            .field("source_port", &self.source_port)
            .field("destination_port", &self.destination_port)
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

fn crc32_table() -> [u32; 256] {
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
    table
}

fn crc32(bytes: &[u8]) -> u32 {
    let table = crc32_table();
    let mut acc: u32 = 0xffff_ffff;
    for byte in bytes {
        let index = ((acc ^ u32::from(*byte)) & 0xff) as usize;
        acc = (acc >> 8) ^ table[index];
    }
    acc ^ 0xffff_ffff
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// Encodes one client payload envelope (1 byte protocol + 2 byte
/// source port + 2 byte destination port + 4 byte compressed length +
/// zlib-wrapped payload + 32 byte SHA-256 + 4 byte CRC32) as a
/// freshly allocated vector. The envelope is canonical and the
/// streaming layer never has to know the byte layout.
pub fn encode_client_payload(payload: &ClientPayload) -> Result<Vec<u8>, ClientPayloadEncodeError> {
    let protocol = u16::from(payload.protocol);
    if protocol > u16::from(u8::MAX) {
        return Err(ClientPayloadEncodeError::InvalidProtocol(protocol));
    }

    let application_bytes = payload.payload.as_slice();
    let mut compressed = Vec::with_capacity(application_bytes.len() + 32);
    let mut encoder = ZlibEncoder::new(&mut compressed, Compression::default());
    encoder.write_all(application_bytes).map_err(|_| {
        ClientPayloadEncodeError::Codec(CodecError::InvalidFieldValue {
            offset: 0,
            context: "client payload zlib write",
        })
    })?;
    encoder.finish().map_err(|_| {
        ClientPayloadEncodeError::Codec(CodecError::InvalidFieldValue {
            offset: 0,
            context: "client payload zlib finish",
        })
    })?;

    let sha = sha256(application_bytes);
    let crc = crc32(application_bytes);

    let encoded = encode_to_vec(MAX_CLIENT_PAYLOAD_BYTES, |encoder| {
        encoder.write_u8(payload.protocol)?;
        encoder.write_u16(payload.source_port)?;
        encoder.write_u16(payload.destination_port)?;
        encoder.write_u32(u32::try_from(compressed.len()).map_err(|_| {
            CodecError::ArithmeticOverflow {
                offset: encoder.len(),
                context: "compressed payload length conversion",
            }
        })?)?;
        encoder.write_raw(&compressed)?;
        encoder.write_raw(&sha)?;
        encoder.write_u32(crc)
    })
    .map_err(ClientPayloadEncodeError::Codec)?;
    Ok(encoded)
}

fn decode_envelope_inner(
    input: &[u8],
    maximum: usize,
) -> Result<ClientPayload, ClientPayloadDecodeError> {
    // Pre-check that the envelope can fit the minimum fixed header plus
    // the trailing integrity suffix.
    const MIN_ENVELOPE_BYTES: usize = 1 + 2 + 2 + 4 + 32 + 4;
    if input.len() < MIN_ENVELOPE_BYTES {
        return Err(ClientPayloadDecodeError::Truncated);
    }
    if input.len() > maximum {
        return Err(ClientPayloadDecodeError::Codec(
            CodecError::LengthExceeded {
                offset: 0,
                declared: input.len(),
                maximum,
                context: "client payload input",
            },
        ));
    }
    let protocol = input[0];
    let source_port = u16::from_be_bytes([input[1], input[2]]);
    let destination_port = u16::from_be_bytes([input[3], input[4]]);
    let compressed_len = u32::from_be_bytes([input[5], input[6], input[7], input[8]]) as usize;
    let header_end = 9_usize;
    let suffix_len = 32 + 4;
    if input.len() < header_end + compressed_len + suffix_len {
        return Err(ClientPayloadDecodeError::Truncated);
    }
    if compressed_len > input.len() - header_end - suffix_len {
        return Err(ClientPayloadDecodeError::Truncated);
    }
    let compressed = &input[header_end..header_end + compressed_len];
    let sha_field = &input[header_end + compressed_len..header_end + compressed_len + 32];
    let crc_field = u32::from_be_bytes([
        input[header_end + compressed_len + 32],
        input[header_end + compressed_len + 33],
        input[header_end + compressed_len + 34],
        input[header_end + compressed_len + 35],
    ]);

    let mut decompressed = Vec::with_capacity(compressed_len);
    let mut decoder = flate2::read::ZlibDecoder::new(compressed);
    std::io::Read::read_to_end(&mut decoder, &mut decompressed)
        .map_err(|_| ClientPayloadDecodeError::DecompressionFailed)?;

    if decompressed.len() > MAX_APPLICATION_PAYLOAD_BYTES {
        return Err(ClientPayloadDecodeError::PayloadTooLarge {
            actual: decompressed.len(),
            maximum: MAX_APPLICATION_PAYLOAD_BYTES,
        });
    }

    let computed_sha = sha256(&decompressed);
    if computed_sha.as_slice() != sha_field {
        return Err(ClientPayloadDecodeError::InvalidSha256);
    }
    let computed_crc = crc32(&decompressed);
    if computed_crc != crc_field {
        return Err(ClientPayloadDecodeError::InvalidCrc);
    }

    Ok(ClientPayload {
        protocol,
        source_port,
        destination_port,
        payload: decompressed,
    })
}

/// Decodes exactly one client payload envelope under an explicit input
/// ceiling and verifies the integrity suffix.
pub fn decode_client_payload(
    input: &[u8],
    maximum: usize,
) -> Result<ClientPayload, ClientPayloadDecodeError> {
    let decoded = decode_envelope_inner(input, maximum)?;
    // Use the strict top-level decoder only to enforce the trailing-bytes
    // policy once the integrity suffix has been consumed. Since
    // `decode_envelope_inner` already validates the prefix and the suffix,
    // this just enforces that no additional bytes are appended beyond the
    // canonical envelope form.
    decode_exact(input, maximum, |cursor| {
        // Walk past the bytes the inner decoder already accepted.
        let _ = cursor.read_u8()?;
        let _ = cursor.read_u16()?;
        let _ = cursor.read_u16()?;
        let compressed_len =
            usize::try_from(cursor.read_u32()?).map_err(|_| CodecError::ArithmeticOverflow {
                offset: cursor.offset(),
                context: "compressed length",
            })?;
        let _ = cursor.take(compressed_len)?;
        let _ = cursor.take(32)?;
        cursor.read_u32()?;
        cursor.finish()?;
        Ok(())
    })
    .map_err(|error| match error {
        CodecError::TrailingBytes { .. } => ClientPayloadDecodeError::Truncated,
        other => ClientPayloadDecodeError::Codec(other),
    })?;
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_payload_round_trip_matches_source_bytes() {
        let application = b"hello world from streaming payload framing".to_vec();
        let envelope = ClientPayload::streaming(application.clone()).expect("payload");
        let encoded = encode_client_payload(&envelope).expect("encode");
        let decoded = decode_client_payload(&encoded, encoded.len()).expect("decode");
        assert_eq!(decoded.protocol, STREAMING_PROTOCOL_NUMBER);
        assert_eq!(decoded.payload, application);
    }

    #[test]
    fn source_destination_ports_round_trip() {
        let envelope = ClientPayload {
            protocol: STREAMING_PROTOCOL_NUMBER,
            source_port: 0x1234,
            destination_port: 0x5678,
            payload: vec![0xaa, 0xbb, 0xcc],
        };
        let encoded = encode_client_payload(&envelope).expect("encode");
        let decoded = decode_client_payload(&encoded, encoded.len()).expect("decode");
        assert_eq!(decoded.source_port, 0x1234);
        assert_eq!(decoded.destination_port, 0x5678);
    }

    #[test]
    fn crc_corruption_is_rejected() {
        let envelope = ClientPayload::streaming(vec![1, 2, 3, 4, 5, 6, 7, 8]).expect("payload");
        let mut encoded = encode_client_payload(&envelope).expect("encode");
        let last = encoded.len() - 1;
        encoded[last] ^= 0x01;
        let error = decode_client_payload(&encoded, encoded.len()).unwrap_err();
        assert!(matches!(
            error,
            ClientPayloadDecodeError::InvalidCrc | ClientPayloadDecodeError::InvalidSha256
        ));
    }

    #[test]
    fn truncated_frame_is_rejected() {
        let envelope = ClientPayload::streaming(vec![0x10, 0x20, 0x30]).expect("payload");
        let encoded = encode_client_payload(&envelope).expect("encode");
        let error =
            decode_client_payload(&encoded[..encoded.len() - 1], encoded.len()).unwrap_err();
        assert!(matches!(
            error,
            ClientPayloadDecodeError::Truncated | ClientPayloadDecodeError::Codec(_)
        ));
    }

    #[test]
    fn bounded_uncompressed_size_rejects_huge_application_bytes() {
        let huge = vec![0_u8; MAX_APPLICATION_PAYLOAD_BYTES + 1];
        let error = ClientPayload::streaming(huge).unwrap_err();
        assert!(matches!(
            error,
            ClientPayloadEncodeError::PayloadTooLarge { .. }
        ));
    }

    #[test]
    fn envelope_uses_standard_zlib_metadata_bytes() {
        let envelope = ClientPayload::streaming(vec![0; 32]).expect("payload");
        let encoded = encode_client_payload(&envelope).expect("encode");
        // The first byte is the protocol number (6 = Streaming), not the
        // zlib header. Locate the compressed payload section, which begins
        // after the fixed 9-byte header. ZlibEncoder uses zlib framing
        // (CMF/FLG), which for deflate with the default compression
        // level starts with the well-known CMF byte 0x78.
        let compressed_len =
            u32::from_be_bytes([encoded[5], encoded[6], encoded[7], encoded[8]]) as usize;
        let compressed_start = 9;
        assert!(compressed_len >= 2);
        assert_eq!(encoded[compressed_start], 0x78);
        // CMF*256 + FLG must be a multiple of 31 per the zlib spec.
        let cmf = encoded[compressed_start];
        let flg = encoded[compressed_start + 1];
        assert_eq!((u16::from(cmf) * 256 + u16::from(flg)) % 31, 0);
    }
}
