//! Standard I2P/I2CP client payload framing (RFC 1952 gzip member).
//!
//! Plan 125 §1.1 / §3 defines the canonical I2P client payload format
//! used to carry Streaming (and any future application) bytes inside an
//! I2NP `Data` body. The official I2P I2CP payload format is one
//! standard RFC 1952 gzip member whose fixed 10-byte header is
//! repurposed as follows:
//!
//! ```text
//! bytes 0..=2  = 1f 8b 08
//! byte 3       = gzip flags (must be 0)
//! bytes 4..=5  = I2P source port (big-endian)
//! bytes 6..=7  = I2P destination port (big-endian)
//! byte 8       = gzip xflags; 2 (maximum compression) for Java-compatible output
//! byte 9       = I2P protocol; 6 for Streaming
//! body          = raw DEFLATE stream
//! trailer       = standard gzip CRC-32 + ISIZE
//! ```
//!
//! There is no extra SHA-256 integrity field and no custom
//! compressed-length prefix.
//!
//! For Streaming, the protocol byte is
//! [`STREAMING_PROTOCOL_NUMBER`] (`6`).
//!
//! The internal destination API works on the typed [`ClientPayload`]
//! value; streaming code never reaches inside the framing bytes.
//!
//! Normative references:
//!
//! ```text
//! https://datatracker.ietf.org/doc/html/rfc1952
//! https://i2p.net/en/docs/specs/i2cp-overview/
//! ```

use std::fmt;
use std::io::{Read, Write};

use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;

use crate::codec::{CodecError, encode_to_vec};

/// `Read` adapter that counts the total bytes consumed from the
/// underlying source. `flate2::read::DeflateDecoder` does not expose a
/// reliable body-length accessor through its public API, so we track
/// the slice position ourselves. The `consumed` accessor is unused
/// after we adopted `total_in()` as the authoritative body length;
/// it remains as a debug-only fallback.
#[derive(Debug)]
struct CountingSliceReader<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> CountingSliceReader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, position: 0 }
    }

    #[allow(dead_code)]
    fn consumed(&self) -> usize {
        self.position
    }
}

impl<'a> Read for CountingSliceReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.position >= self.input.len() {
            return Ok(0);
        }
        let remaining = &self.input[self.position..];
        let n = remaining.len().min(buf.len());
        buf[..n].copy_from_slice(&remaining[..n]);
        self.position += n;
        Ok(n)
    }
}

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

/// Fixed gzip header magic bytes `1f 8b`.
pub const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];
/// Compression method for deflate (`0x08`).
pub const GZIP_CM_DEFLATE: u8 = 0x08;

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
    /// The envelope magic bytes did not match `1f 8b`.
    BadMagic,
    /// The envelope declared a compression method other than `deflate`.
    UnsupportedCompressionMethod(u8),
    /// The envelope carried an unsupported/unsafe optional-header layout.
    UnsupportedOptionalHeaderLayout(&'static str),
    /// The gzip flag byte carried reserved bits 5-7.
    ReservedFlagBits(u8),
    /// The CRC32 integrity check failed.
    InvalidCrc {
        /// Trailer CRC value.
        declared: u32,
        /// Computed CRC value.
        actual: u32,
    },
    /// The ISIZE trailer field disagreed with the uncompressed length.
    InvalidIsize {
        /// Trailer ISIZE value.
        declared: u32,
        /// Computed length value.
        actual: u32,
    },
    /// The decompressed body exceeded the configured maximum.
    PayloadTooLarge {
        /// Projected size.
        actual: usize,
        /// Allowed maximum.
        maximum: usize,
    },
    /// The envelope carries a non-streaming protocol number.
    UnsupportedProtocol(u8),
    /// Decompression failed (malformed/truncated deflate stream).
    DecompressionFailed(String),
    /// Strict decoder found bytes outside the gzip member boundary.
    TrailingBytes {
        /// Offset of the trailing-byte region.
        offset: usize,
        /// Number of trailing bytes.
        length: usize,
    },
    /// The envelope input exceeded the input limit before decoding.
    Codec(CodecError),
}

impl fmt::Display for ClientPayloadDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => formatter.write_str("client payload envelope truncated"),
            Self::BadMagic => formatter.write_str("client payload gzip magic mismatch"),
            Self::UnsupportedCompressionMethod(value) => write!(
                formatter,
                "client payload unsupported gzip compression method {value}"
            ),
            Self::UnsupportedOptionalHeaderLayout(label) => write!(
                formatter,
                "client payload unsupported gzip optional-header layout {label}"
            ),
            Self::ReservedFlagBits(bits) => write!(
                formatter,
                "client payload gzip flag reserved bits set: {bits:#04x}"
            ),
            Self::InvalidCrc { declared, actual } => write!(
                formatter,
                "client payload gzip CRC32 mismatch (declared {declared:#010x}, computed {actual:#010x})"
            ),
            Self::InvalidIsize { declared, actual } => write!(
                formatter,
                "client payload gzip ISIZE mismatch (declared {declared}, computed {actual})"
            ),
            Self::PayloadTooLarge { actual, maximum } => write!(
                formatter,
                "client payload declares {actual} bytes, exceeds {maximum}-byte ceiling"
            ),
            Self::UnsupportedProtocol(value) => {
                write!(formatter, "unsupported client payload protocol {value}")
            }
            Self::DecompressionFailed(message) => {
                write!(
                    formatter,
                    "client payload gzip decompression failed: {message}"
                )
            }
            Self::TrailingBytes { offset, length } => write!(
                formatter,
                "client payload carries {length} trailing byte(s) at offset {offset}"
            ),
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

/// Encodes one client payload envelope as a single RFC 1952 gzip
/// member with the I2P header convention. Returns the freshly
/// allocated wire bytes.
///
/// The header layout is:
///
/// ```text
/// 1f 8b 08 <flags:0> <src_port BE u16> <dst_port BE u16> <xfl:2> <protocol>
/// raw deflate stream
/// crc32 (LE) + isize (LE)
/// ```
pub fn encode_client_payload(payload: &ClientPayload) -> Result<Vec<u8>, ClientPayloadEncodeError> {
    let protocol = u16::from(payload.protocol);
    if protocol > u16::from(u8::MAX) {
        return Err(ClientPayloadEncodeError::InvalidProtocol(protocol));
    }
    if payload.payload.len() > MAX_APPLICATION_PAYLOAD_BYTES {
        return Err(ClientPayloadEncodeError::PayloadTooLarge {
            actual: payload.payload.len(),
            maximum: MAX_APPLICATION_PAYLOAD_BYTES,
        });
    }

    let mut compressed = Vec::with_capacity(payload.payload.len() + 32);
    {
        let mut encoder = DeflateEncoder::new(&mut compressed, Compression::default());
        encoder.write_all(&payload.payload).map_err(|_| {
            ClientPayloadEncodeError::Codec(CodecError::InvalidFieldValue {
                offset: 0,
                context: "client payload deflate write",
            })
        })?;
        encoder.finish().map_err(|_| {
            ClientPayloadEncodeError::Codec(CodecError::InvalidFieldValue {
                offset: 0,
                context: "client payload deflate finish",
            })
        })?;
    }

    let crc = crc32(&payload.payload);
    let isize = payload.payload.len() as u32;

    let header = [
        GZIP_MAGIC[0],
        GZIP_MAGIC[1],
        GZIP_CM_DEFLATE,
        0x00, // FLG: no optional fields
        (payload.source_port >> 8) as u8,
        payload.source_port as u8,
        (payload.destination_port >> 8) as u8,
        payload.destination_port as u8,
        0x02, // XFL: maximum compression (matches Java/i2pd I2P client payload output)
        payload.protocol,
    ];

    encode_to_vec(MAX_CLIENT_PAYLOAD_BYTES, |encoder| {
        encoder.write_raw(&header)?;
        encoder.write_raw(&compressed)?;
        // RFC 1952 §2.2: CRC32 and ISIZE are both stored in
        // little-endian byte order. The codec's `write_u32` is
        // big-endian, so we append raw bytes here.
        encoder.write_raw(&crc.to_le_bytes())?;
        encoder.write_raw(&isize.to_le_bytes())?;
        Ok(())
    })
    .map_err(ClientPayloadEncodeError::Codec)
}

/// Decodes exactly one client payload envelope under an explicit input
/// ceiling and returns the [`ClientPayload`] value.
///
/// The decoder verifies the gzip header (magic, compression method,
/// flags), parses the I2P-specific header fields, decompresses the
/// deflate body, and verifies the CRC32 + ISIZE trailer. Optional gzip
/// header layouts (FTEXT/FHCRC/FEXTRA/FNAME/FCOMMENT) are rejected.
pub fn decode_client_payload(
    input: &[u8],
    maximum: usize,
) -> Result<ClientPayload, ClientPayloadDecodeError> {
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
    if input.len() < 10 {
        return Err(ClientPayloadDecodeError::Truncated);
    }
    if input[0] != GZIP_MAGIC[0] || input[1] != GZIP_MAGIC[1] {
        return Err(ClientPayloadDecodeError::BadMagic);
    }
    if input[2] != GZIP_CM_DEFLATE {
        return Err(ClientPayloadDecodeError::UnsupportedCompressionMethod(
            input[2],
        ));
    }
    let flags = input[3];
    let source_port = u16::from_be_bytes([input[4], input[5]]);
    let destination_port = u16::from_be_bytes([input[6], input[7]]);
    let protocol = input[9];

    if flags & 0b0000_0001 != 0 {
        return Err(ClientPayloadDecodeError::UnsupportedOptionalHeaderLayout(
            "FTEXT",
        ));
    }
    if flags & 0b0000_0010 != 0 {
        return Err(ClientPayloadDecodeError::UnsupportedOptionalHeaderLayout(
            "FHCRC",
        ));
    }
    if flags & 0b0000_0100 != 0 {
        return Err(ClientPayloadDecodeError::UnsupportedOptionalHeaderLayout(
            "FEXTRA",
        ));
    }
    if flags & 0b0000_1000 != 0 {
        return Err(ClientPayloadDecodeError::UnsupportedOptionalHeaderLayout(
            "FNAME",
        ));
    }
    if flags & 0b0001_0000 != 0 {
        return Err(ClientPayloadDecodeError::UnsupportedOptionalHeaderLayout(
            "FCOMMENT",
        ));
    }
    if flags & 0b1110_0000 != 0 {
        return Err(ClientPayloadDecodeError::ReservedFlagBits(flags));
    }

    // The I2P client payload is a canonical gzip member with
    // FLG=0/XFL=2/OS=I2P-protocol. We feed the post-header portion
    // (deflate body + trailer + possible trailing bytes) to a
    // `DeflateDecoder` that wraps a counting slice reader; the deflate
    // stream is self-terminating, so `read()` returns 0 once the body
    // ends. `total_in()` then gives the exact deflate body length.
    // We then read the 8-byte trailer (CRC32 + ISIZE) manually and
    // reject any remaining bytes as trailing data.
    let mut reader = CountingSliceReader::new(&input[10..]);
    let mut decoder = DeflateDecoder::new(&mut reader);
    let mut payload = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        match decoder.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if payload.len() + n > MAX_APPLICATION_PAYLOAD_BYTES {
                    return Err(ClientPayloadDecodeError::PayloadTooLarge {
                        actual: payload.len() + n,
                        maximum: MAX_APPLICATION_PAYLOAD_BYTES,
                    });
                }
                payload.extend_from_slice(&chunk[..n]);
            }
            Err(error) => {
                return Err(ClientPayloadDecodeError::DecompressionFailed(format!(
                    "{error:?}"
                )));
            }
        }
    }
    // `total_in()` is the authoritative body length: the deflate
    // decoder self-terminates and the trailing 8 bytes (CRC32 +
    // ISIZE) are never interpreted as deflate data.
    let body_consumed = decoder.total_in() as usize;
    if body_consumed + 8 > input.len() - 10 {
        return Err(ClientPayloadDecodeError::Truncated);
    }
    let trailer_crc = u32::from_le_bytes([
        input[10 + body_consumed],
        input[10 + body_consumed + 1],
        input[10 + body_consumed + 2],
        input[10 + body_consumed + 3],
    ]);
    let trailer_isize = u32::from_le_bytes([
        input[10 + body_consumed + 4],
        input[10 + body_consumed + 5],
        input[10 + body_consumed + 6],
        input[10 + body_consumed + 7],
    ]);
    if trailer_isize != payload.len() as u32 {
        return Err(ClientPayloadDecodeError::InvalidIsize {
            declared: trailer_isize,
            actual: payload.len() as u32,
        });
    }
    let computed_crc = crc32(&payload);
    if computed_crc != trailer_crc {
        return Err(ClientPayloadDecodeError::InvalidCrc {
            declared: trailer_crc,
            actual: computed_crc,
        });
    }
    if 10 + body_consumed + 8 != input.len() {
        return Err(ClientPayloadDecodeError::TrailingBytes {
            offset: 10 + body_consumed + 8,
            length: input.len() - 10 - body_consumed - 8,
        });
    }
    Ok(ClientPayload {
        protocol,
        source_port,
        destination_port,
        payload,
    })
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
            destination_port: 0xabcd,
            payload: vec![0xaa, 0xbb, 0xcc],
        };
        let encoded = encode_client_payload(&envelope).expect("encode");
        let decoded = decode_client_payload(&encoded, encoded.len()).expect("decode");
        assert_eq!(decoded.source_port, 0x1234);
        assert_eq!(decoded.destination_port, 0xabcd);
    }

    #[test]
    fn encoded_magic_is_1f_8b_08() {
        let envelope = ClientPayload::streaming(vec![1, 2, 3]).expect("payload");
        let encoded = encode_client_payload(&envelope).expect("encode");
        assert_eq!(encoded[0], 0x1f);
        assert_eq!(encoded[1], 0x8b);
        assert_eq!(encoded[2], 0x08);
        assert_eq!(encoded[3], 0x00); // FLG
        assert_eq!(encoded[8], 0x02); // XFL
        assert_eq!(encoded[9], STREAMING_PROTOCOL_NUMBER);
    }

    #[test]
    fn gzip_crc_corruption_rejected() {
        let envelope = ClientPayload::streaming(vec![1, 2, 3, 4, 5, 6, 7, 8]).expect("payload");
        let mut encoded = encode_client_payload(&envelope).expect("encode");
        // Flip a byte inside the CRC32 field (bytes -8..-4).
        let crc_byte = encoded.len() - 5;
        encoded[crc_byte] ^= 0x01;
        let error = decode_client_payload(&encoded, encoded.len()).unwrap_err();
        assert!(matches!(
            error,
            ClientPayloadDecodeError::InvalidCrc { .. }
                | ClientPayloadDecodeError::InvalidIsize { .. }
                | ClientPayloadDecodeError::DecompressionFailed(_)
        ));
    }

    #[test]
    fn gzip_isize_corruption_rejected() {
        let envelope = ClientPayload::streaming(vec![1, 2, 3, 4, 5, 6, 7, 8]).expect("payload");
        let mut encoded = encode_client_payload(&envelope).expect("encode");
        // Flip a byte in the ISIZE field (4 bytes before end).
        let isize_start = encoded.len() - 4;
        encoded[isize_start] ^= 0x01;
        let error = decode_client_payload(&encoded, encoded.len()).unwrap_err();
        assert!(matches!(
            error,
            ClientPayloadDecodeError::InvalidIsize { .. }
                | ClientPayloadDecodeError::DecompressionFailed(_)
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
            ClientPayloadDecodeError::DecompressionFailed(_) | ClientPayloadDecodeError::Truncated
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
    fn bad_gzip_magic_is_rejected() {
        let mut bytes =
            encode_client_payload(&ClientPayload::streaming(vec![1, 2, 3, 4]).expect("payload"))
                .expect("encode");
        bytes[0] = 0x42;
        let error = decode_client_payload(&bytes, bytes.len()).unwrap_err();
        assert!(matches!(error, ClientPayloadDecodeError::BadMagic));
    }

    #[test]
    fn unsupported_compression_method_is_rejected() {
        let mut bytes =
            encode_client_payload(&ClientPayload::streaming(vec![1, 2, 3, 4]).expect("payload"))
                .expect("encode");
        bytes[2] = 0x09; // not deflate
        let error = decode_client_payload(&bytes, bytes.len()).unwrap_err();
        assert!(matches!(
            error,
            ClientPayloadDecodeError::UnsupportedCompressionMethod(_)
        ));
    }

    #[test]
    fn reserved_flag_bits_are_rejected() {
        let mut bytes =
            encode_client_payload(&ClientPayload::streaming(vec![1, 2, 3, 4]).expect("payload"))
                .expect("encode");
        bytes[3] = 0x80; // reserved bit 7
        let error = decode_client_payload(&bytes, bytes.len()).unwrap_err();
        assert!(matches!(
            error,
            ClientPayloadDecodeError::ReservedFlagBits(_)
        ));
    }

    #[test]
    fn optional_header_layouts_are_rejected() {
        for (label, bit) in [
            ("FTEXT", 0x01),
            ("FHCRC", 0x02),
            ("FEXTRA", 0x04),
            ("FNAME", 0x08),
            ("FCOMMENT", 0x10),
        ] {
            let mut bytes = encode_client_payload(
                &ClientPayload::streaming(vec![1, 2, 3, 4]).expect("payload"),
            )
            .expect("encode");
            bytes[3] = bit;
            let error = decode_client_payload(&bytes, bytes.len()).unwrap_err();
            assert!(
                matches!(error, ClientPayloadDecodeError::UnsupportedOptionalHeaderLayout(label_observed) if label_observed == label),
                "expected unsupported {label}, got {error:?}"
            );
        }
    }

    #[test]
    fn zlib_wrapped_old_i2pr_format_is_rejected() {
        // The legacy i2pr format used zlib's 78 xx prefix and a custom
        // 9-byte header. A zlib-wrapped payload must not pass as an
        // I2P client payload gzip member.
        let mut compressor =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut compressor, b"hello").unwrap();
        let body = compressor.finish().unwrap();
        assert_eq!(body[0], 0x78);
        let error = decode_client_payload(&body, body.len()).unwrap_err();
        assert!(matches!(error, ClientPayloadDecodeError::BadMagic));
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes =
            encode_client_payload(&ClientPayload::streaming(vec![1, 2, 3, 4]).expect("payload"))
                .expect("encode");
        bytes.push(0xff);
        let error = decode_client_payload(&bytes, bytes.len()).unwrap_err();
        assert!(matches!(
            error,
            ClientPayloadDecodeError::TrailingBytes { .. }
        ));
    }

    #[test]
    fn known_good_payload_matches_i2p_destination_ports() {
        // Synthesize a frozen independently-derived I2P client payload
        // gzip member using the standard RFC 1952 layout. The source
        // port is 0x1234 and the destination port is 0xabcd so a
        // byte-order reversal cannot pass accidentally.
        //
        // The fixture body uses an uncompressed (BTYPE=00) deflate
        // block per RFC 1951 §3.2.4: 5-byte block header, LEN, NLEN,
        // then the literal payload. This is fully specified without
        // depending on flate2 (which the encoder under test also
        // uses), so a regression that breaks the CRC check cannot
        // pass this fixture.
        let payload = b"hello I2P streaming fixture";
        let body_len = payload.len() as u16;
        // Deflate stored block: 01 (BFINAL=1 BTYPE=00) + LEN (16 LE) +
        // NLEN (one's complement of LEN) + payload.
        let body: Vec<u8> = std::iter::once(0x01_u8)
            .chain(body_len.to_le_bytes())
            .chain((!body_len).to_le_bytes())
            .chain(payload.iter().copied())
            .collect();
        let header = [0x1f, 0x8b, 0x08, 0x00, 0x12, 0x34, 0xab, 0xcd, 0x02, 0x06];
        let crc = crc32(payload);
        let isize = payload.len() as u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&header);
        bytes.extend_from_slice(&body);
        bytes.extend_from_slice(&crc.to_le_bytes());
        bytes.extend_from_slice(&isize.to_le_bytes());
        let decoded = decode_client_payload(&bytes, bytes.len()).expect("decode");
        assert_eq!(decoded.source_port, 0x1234);
        assert_eq!(decoded.destination_port, 0xabcd);
        assert_eq!(decoded.protocol, STREAMING_PROTOCOL_NUMBER);
        assert_eq!(decoded.payload, payload);
    }
}
