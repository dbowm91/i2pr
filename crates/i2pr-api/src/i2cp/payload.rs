//! I2CP payload wrapper and gzip metadata contract.
//!
//! Every I2CP application message travels inside a [`Payload`]: a
//! four-byte big-endian length followed by that many bytes. The
//! payload bytes themselves are a gzip member whose ten-byte header
//! carries I2P metadata (source/destination ports, protocol number,
//! and flags) in repurposed gzip fields, exactly as recorded on the
//! pinned I2CP overview page.
//!
//! This pass implements structural parse/encode helpers only and does
//! not connect payloads to destination routing. There is deliberately
//! no decompression helper yet: any future inflation must enforce
//! [`MAX_I2CP_DECOMPRESSED_BYTES`] and reject expansion beyond it
//! (see the Plan 168 data-plane pass). The ceiling is recorded here
//! so the bound is fixed before any decoder exists.

use i2pr_proto::CodecError;

use super::error::I2cpError;

/// Maximum I2CP payload body in bytes (64 KiB, matching the frame ceiling).
pub const MAX_I2CP_PAYLOAD_BYTES: usize = 64 * 1024;

/// Maximum decompressed I2CP application bytes a future helper may produce.
///
/// Recorded now so Plan 168 cannot introduce unbounded gzip
/// expansion; no decompression exists in this pass.
pub const MAX_I2CP_DECOMPRESSED_BYTES: usize = 64 * 1024;

/// Gzip magic bytes opening every I2CP payload.
pub const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

/// Gzip compression method: deflate.
pub const GZIP_METHOD_DEFLATE: u8 = 8;

/// XFL value emitted by the Java implementation.
pub const GZIP_XFLAGS_JAVA: u8 = 2;

/// I2P protocol number for Streaming payloads.
pub const PROTOCOL_STREAMING: u8 = 6;

/// I2P protocol number for repliable datagrams.
pub const PROTOCOL_DATAGRAM: u8 = 17;

/// I2P protocol number for raw datagrams.
pub const PROTOCOL_DATAGRAM_RAW: u8 = 18;

/// First experimental I2P protocol number.
pub const PROTOCOL_EXPERIMENTAL_FIRST: u8 = 224;

/// Last experimental I2P protocol number.
pub const PROTOCOL_EXPERIMENTAL_LAST: u8 = 254;

/// Reserved I2P protocol number (never assigned).
pub const PROTOCOL_RESERVED: u8 = 255;

/// Length of the fixed I2CP gzip header.
pub const GZIP_HEADER_LEN: usize = 10;

/// Length of the gzip trailer (CRC-32 plus uncompressed size).
pub const GZIP_TRAILER_LEN: usize = 8;

/// An opaque I2CP application payload (length-prefixed bytes).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payload {
    bytes: Vec<u8>,
}

impl Payload {
    /// Wraps payload bytes after enforcing the ceiling.
    pub fn new(bytes: Vec<u8>) -> Result<Self, I2cpError> {
        if bytes.len() > MAX_I2CP_PAYLOAD_BYTES {
            return Err(I2cpError::malformed(
                "i2cp payload",
                CodecError::LengthExceeded {
                    offset: 0,
                    declared: bytes.len(),
                    maximum: MAX_I2CP_PAYLOAD_BYTES,
                    context: "i2cp payload",
                },
            ));
        }
        Ok(Self { bytes })
    }

    /// Returns the payload bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Consumes the wrapper into its bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// Encodes the length-prefixed payload.
    pub fn encode(&self) -> Result<Vec<u8>, I2cpError> {
        let length = u32::try_from(self.bytes.len()).map_err(|_| {
            I2cpError::malformed(
                "i2cp payload",
                CodecError::LengthExceeded {
                    offset: 0,
                    declared: self.bytes.len(),
                    maximum: MAX_I2CP_PAYLOAD_BYTES,
                    context: "i2cp payload",
                },
            )
        })?;
        let mut out = Vec::with_capacity(4 + self.bytes.len());
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(&self.bytes);
        Ok(out)
    }

    /// Encoded length including the four-byte prefix.
    pub fn encoded_len(&self) -> Result<usize, I2cpError> {
        self.bytes.len().checked_add(4).ok_or(I2cpError::malformed(
            "i2cp payload",
            CodecError::ArithmeticOverflow {
                offset: 0,
                context: "i2cp payload length",
            },
        ))
    }
}

/// Splits one length-prefixed payload off the front of `input`.
///
/// Returns the payload and the unconsumed remainder. The declared
/// length is checked against [`MAX_I2CP_PAYLOAD_BYTES`] before any
/// allocation or copy. Short inputs fail as malformed bodies: this
/// helper operates on complete frame bodies, while partial TCP reads
/// are the frame decoder's `Incomplete` domain.
pub fn split_payload(input: &[u8]) -> Result<(Payload, &[u8]), I2cpError> {
    if input.len() < 4 {
        return Err(I2cpError::malformed(
            "i2cp payload",
            CodecError::Truncated {
                offset: 0,
                needed: 4,
                remaining: input.len(),
            },
        ));
    }
    let declared = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
    let declared_usize = usize::try_from(declared).map_err(|_| {
        I2cpError::malformed(
            "i2cp payload",
            CodecError::ArithmeticOverflow {
                offset: 0,
                context: "i2cp payload length conversion",
            },
        )
    })?;
    if declared_usize > MAX_I2CP_PAYLOAD_BYTES {
        return Err(I2cpError::malformed(
            "i2cp payload",
            CodecError::LengthExceeded {
                offset: 0,
                declared: declared_usize,
                maximum: MAX_I2CP_PAYLOAD_BYTES,
                context: "i2cp payload",
            },
        ));
    }
    let total = 4usize
        .checked_add(declared_usize)
        .ok_or(I2cpError::malformed(
            "i2cp payload",
            CodecError::ArithmeticOverflow {
                offset: 0,
                context: "i2cp payload total length",
            },
        ))?;
    if input.len() < total {
        return Err(I2cpError::malformed(
            "i2cp payload",
            CodecError::Truncated {
                offset: 4,
                needed: declared_usize,
                remaining: input.len() - 4,
            },
        ));
    }
    let payload = Payload::new(input[4..total].to_vec())?;
    Ok((payload, &input[total..]))
}

/// I2P metadata repurposed inside the ten-byte gzip header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PayloadGzipHeader {
    /// I2P source port (gzip MTIME low bytes).
    pub source_port: u16,
    /// I2P destination port (gzip MTIME high bytes).
    pub destination_port: u16,
    /// Gzip XFL byte; the Java implementation emits 2.
    pub xflags: u8,
    /// I2P protocol number (gzip OS byte).
    pub protocol: u8,
}

impl PayloadGzipHeader {
    /// Parses the ten-byte gzip header of a payload.
    ///
    /// Requires the gzip magic, the deflate method, and zero flag
    /// bits (the M9 profile emits the fixed ten-byte header with no
    /// extra field, name, comment, or header CRC). The CRC-32 trailer
    /// is retained opaquely; integrity validation belongs to the
    /// Plan 168 data plane together with bounded inflation.
    pub fn parse(input: &[u8]) -> Result<Self, I2cpError> {
        if input.len() < GZIP_HEADER_LEN {
            return Err(I2cpError::malformed(
                "i2cp payload gzip header",
                CodecError::Truncated {
                    offset: 0,
                    needed: GZIP_HEADER_LEN,
                    remaining: input.len(),
                },
            ));
        }
        if input[0] != GZIP_MAGIC[0] || input[1] != GZIP_MAGIC[1] {
            return Err(I2cpError::malformed(
                "i2cp payload gzip header",
                CodecError::InvalidFieldValue {
                    offset: 0,
                    context: "gzip magic",
                },
            ));
        }
        if input[2] != GZIP_METHOD_DEFLATE {
            return Err(I2cpError::malformed(
                "i2cp payload gzip header",
                CodecError::InvalidFieldValue {
                    offset: 2,
                    context: "gzip compression method",
                },
            ));
        }
        if input[3] != 0 {
            return Err(I2cpError::malformed(
                "i2cp payload gzip header",
                CodecError::Unsupported {
                    offset: 3,
                    context: "gzip flag bits",
                    value: u64::from(input[3]),
                },
            ));
        }
        Ok(Self {
            source_port: u16::from_be_bytes([input[4], input[5]]),
            destination_port: u16::from_be_bytes([input[6], input[7]]),
            xflags: input[8],
            protocol: input[9],
        })
    }

    /// Encodes the ten-byte gzip header.
    pub fn encode(self) -> [u8; GZIP_HEADER_LEN] {
        let source = self.source_port.to_be_bytes();
        let destination = self.destination_port.to_be_bytes();
        [
            GZIP_MAGIC[0],
            GZIP_MAGIC[1],
            GZIP_METHOD_DEFLATE,
            0,
            source[0],
            source[1],
            destination[0],
            destination[1],
            self.xflags,
            self.protocol,
        ]
    }

    /// Reports whether the protocol number is reserved.
    pub const fn is_reserved_protocol(self) -> bool {
        self.protocol == PROTOCOL_RESERVED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_round_trip() {
        let payload = Payload::new(vec![0x01, 0x02, 0x03]).expect("new");
        let encoded = payload.encode().expect("encode");
        assert_eq!(encoded, vec![0, 0, 0, 3, 1, 2, 3]);
        let (decoded, rest) = split_payload(&encoded).expect("split");
        assert_eq!(decoded, payload);
        assert!(rest.is_empty());
    }

    #[test]
    fn payload_split_returns_remainder() {
        let payload = Payload::new(vec![0xaa]).expect("new");
        let mut encoded = payload.encode().expect("encode");
        encoded.extend_from_slice(&[0x99, 0x88]);
        let (decoded, rest) = split_payload(&encoded).expect("split");
        assert_eq!(decoded.as_bytes(), &[0xaa]);
        assert_eq!(rest, &[0x99, 0x88]);
    }

    #[test]
    fn payload_ceiling_enforced_before_copy() {
        let oversized = vec![0u8; MAX_I2CP_PAYLOAD_BYTES + 1];
        assert!(matches!(
            Payload::new(oversized),
            Err(I2cpError::Malformed { .. })
        ));
        let mut header = ((MAX_I2CP_PAYLOAD_BYTES + 1) as u32).to_be_bytes().to_vec();
        header.extend_from_slice(&[0u8; 4]);
        assert!(matches!(
            split_payload(&header),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn truncated_payload_is_malformed() {
        assert!(matches!(
            split_payload(&[0, 0]),
            Err(I2cpError::Malformed { .. })
        ));
        assert!(matches!(
            split_payload(&[0, 0, 0, 4, 1, 2]),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn gzip_header_round_trip() {
        let header = PayloadGzipHeader {
            source_port: 80,
            destination_port: 8080,
            xflags: GZIP_XFLAGS_JAVA,
            protocol: PROTOCOL_STREAMING,
        };
        let encoded = header.encode();
        assert_eq!(&encoded[..3], &[0x1f, 0x8b, 0x08]);
        assert_eq!(PayloadGzipHeader::parse(&encoded).expect("parse"), header);
    }

    #[test]
    fn gzip_header_rejects_bad_magic_method_flags() {
        let good = PayloadGzipHeader {
            source_port: 0,
            destination_port: 0,
            xflags: GZIP_XFLAGS_JAVA,
            protocol: PROTOCOL_DATAGRAM,
        }
        .encode();
        let mut bad_magic = good;
        bad_magic[0] = 0x00;
        assert!(matches!(
            PayloadGzipHeader::parse(&bad_magic),
            Err(I2cpError::Malformed { .. })
        ));
        let mut bad_method = good;
        bad_method[2] = 0x07;
        assert!(matches!(
            PayloadGzipHeader::parse(&bad_method),
            Err(I2cpError::Malformed { .. })
        ));
        let mut bad_flags = good;
        bad_flags[3] = 0x04;
        assert!(matches!(
            PayloadGzipHeader::parse(&bad_flags),
            Err(I2cpError::Malformed { .. })
        ));
        assert!(matches!(
            PayloadGzipHeader::parse(&good[..5]),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn protocol_number_classes() {
        assert!(
            !PayloadGzipHeader {
                source_port: 0,
                destination_port: 0,
                xflags: 0,
                protocol: PROTOCOL_STREAMING,
            }
            .is_reserved_protocol()
        );
        assert!(
            PayloadGzipHeader {
                source_port: 0,
                destination_port: 0,
                xflags: 0,
                protocol: PROTOCOL_RESERVED,
            }
            .is_reserved_protocol()
        );
    }
}
