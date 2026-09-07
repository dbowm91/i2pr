//! I2CP connection preamble and common frame codec.
//!
//! Every I2CP TCP connection opens with the single protocol byte
//! `0x2a`. Every subsequent message is a common frame:
//!
//! ```text
//! uint32 body_length, big-endian
//! uint8  message_type
//! body[body_length]
//! ```
//!
//! The official limit is "about 64 KiB"; this module enforces a named
//! ceiling of exactly 64 KiB before allocating body storage. The
//! incremental [`FrameDecoder`] supports partial header/body reads and
//! never grows without bound: its buffer is capped at one header plus
//! one maximum body.

use super::error::I2cpError;

/// The I2CP TCP protocol byte sent first by every client.
pub const PROTOCOL_BYTE: u8 = 0x2a;

/// Maximum accepted I2CP message body in bytes (64 KiB).
///
/// Named from the official "about 64 KB" limit. Both decoding (reject
/// before allocating) and encoding enforce the same ceiling.
pub const MAX_I2CP_BODY_BYTES: usize = 64 * 1024;

/// On-wire frame header length: four length bytes plus one type byte.
pub const FRAME_HEADER_LEN: usize = 5;

/// A decoded I2CP frame header without its body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameHeader {
    /// Declared body length in bytes.
    pub body_length: usize,
    /// Raw message type byte.
    pub message_type: u8,
}

/// A complete framed I2CP message with its raw type byte and body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawFrame {
    /// Raw message type byte (classification lives in [`super::message`]).
    pub message_type: u8,
    /// Exact body bytes.
    pub body: Vec<u8>,
}

/// Checks the opening protocol byte of a new connection.
pub fn check_protocol_byte(actual: u8) -> Result<(), I2cpError> {
    if actual == PROTOCOL_BYTE {
        Ok(())
    } else {
        Err(I2cpError::InvalidProtocolByte { actual })
    }
}

/// Encodes one framed message, rejecting bodies above the ceiling.
pub fn encode_frame(message_type: u8, body: &[u8]) -> Result<Vec<u8>, I2cpError> {
    if body.len() > MAX_I2CP_BODY_BYTES {
        return Err(I2cpError::BodyTooLarge {
            declared: body.len(),
            maximum: MAX_I2CP_BODY_BYTES,
        });
    }
    let length = body.len() as u32;
    let mut out = Vec::with_capacity(FRAME_HEADER_LEN + body.len());
    out.extend_from_slice(&length.to_be_bytes());
    out.push(message_type);
    out.extend_from_slice(body);
    Ok(out)
}

/// Decodes one frame header from the front of `input`.
///
/// Returns the header; the caller must supply the following
/// `body_length` body bytes. Fewer than five bytes is
/// [`I2cpError::Incomplete`], and a declared length above the ceiling
/// is [`I2cpError::BodyTooLarge`] reported before any allocation.
pub fn decode_header(input: &[u8]) -> Result<FrameHeader, I2cpError> {
    if input.len() < FRAME_HEADER_LEN {
        return Err(I2cpError::Incomplete {
            needed: FRAME_HEADER_LEN - input.len(),
        });
    }
    let declared = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
    let declared_usize = usize::try_from(declared).map_err(|_| I2cpError::BodyTooLarge {
        declared: u32::MAX as usize,
        maximum: MAX_I2CP_BODY_BYTES,
    })?;
    if declared_usize > MAX_I2CP_BODY_BYTES {
        return Err(I2cpError::BodyTooLarge {
            declared: declared_usize,
            maximum: MAX_I2CP_BODY_BYTES,
        });
    }
    Ok(FrameHeader {
        body_length: declared_usize,
        message_type: input[4],
    })
}

/// Decodes exactly one complete frame; trailing bytes are rejected.
///
/// Prefer [`FrameDecoder`] for streaming reads; this helper is the
/// strict single-frame entry point used by tests and fixtures.
pub fn decode_frame(input: &[u8]) -> Result<RawFrame, I2cpError> {
    let header = decode_header(input)?;
    let total =
        FRAME_HEADER_LEN
            .checked_add(header.body_length)
            .ok_or(I2cpError::BodyTooLarge {
                declared: usize::MAX,
                maximum: MAX_I2CP_BODY_BYTES,
            })?;
    if input.len() < total {
        return Err(I2cpError::Incomplete {
            needed: total - input.len(),
        });
    }
    if input.len() > total {
        return Err(I2cpError::Malformed {
            message: "i2cp frame",
            source: i2pr_proto::CodecError::TrailingBytes {
                offset: total,
                remaining: input.len() - total,
            },
        });
    }
    Ok(RawFrame {
        message_type: header.message_type,
        body: input[FRAME_HEADER_LEN..total].to_vec(),
    })
}

/// Incremental I2CP frame decoder for partial TCP reads.
///
/// Bytes are pushed with [`FrameDecoder::push`]; complete frames are
/// returned in order while an incomplete header/body tail is retained.
/// The buffer never exceeds one header plus one maximum body; feeding
/// beyond that fails closed instead of growing.
#[derive(Clone, Debug, Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    /// Creates an empty decoder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the retained (incomplete) byte count.
    pub fn buffered(&self) -> usize {
        self.buffer.len()
    }

    /// Feeds newly read bytes and drains every complete frame.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<RawFrame>, I2cpError> {
        let end = self
            .buffer
            .len()
            .checked_add(chunk.len())
            .ok_or(I2cpError::BodyTooLarge {
                declared: usize::MAX,
                maximum: MAX_I2CP_BODY_BYTES,
            })?;
        if end > FRAME_HEADER_LEN + MAX_I2CP_BODY_BYTES {
            return Err(I2cpError::BodyTooLarge {
                declared: end,
                maximum: FRAME_HEADER_LEN + MAX_I2CP_BODY_BYTES,
            });
        }
        self.buffer.extend_from_slice(chunk);
        let mut frames = Vec::new();
        loop {
            if self.buffer.len() < FRAME_HEADER_LEN {
                break;
            }
            let header = decode_header(&self.buffer)?;
            let total = FRAME_HEADER_LEN + header.body_length;
            if self.buffer.len() < total {
                break;
            }
            let body = self.buffer[FRAME_HEADER_LEN..total].to_vec();
            self.buffer.drain(..total);
            frames.push(RawFrame {
                message_type: header.message_type,
                body,
            });
        }
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_byte_accepts_magic() {
        assert!(check_protocol_byte(0x2a).is_ok());
        assert_eq!(
            check_protocol_byte(0x00),
            Err(I2cpError::InvalidProtocolByte { actual: 0x00 })
        );
        assert_eq!(
            check_protocol_byte(0x2b),
            Err(I2cpError::InvalidProtocolByte { actual: 0x2b })
        );
    }

    #[test]
    fn frame_round_trip() {
        let body = vec![0x01, 0x02, 0x03];
        let encoded = encode_frame(5, &body).expect("encode");
        assert_eq!(encoded.len(), FRAME_HEADER_LEN + body.len());
        let frame = decode_frame(&encoded).expect("decode");
        assert_eq!(frame.message_type, 5);
        assert_eq!(frame.body, body);
    }

    #[test]
    fn empty_body_frame_round_trip() {
        let encoded = encode_frame(8, &[]).expect("encode");
        assert_eq!(encoded.len(), FRAME_HEADER_LEN);
        let frame = decode_frame(&encoded).expect("decode");
        assert_eq!(frame.message_type, 8);
        assert!(frame.body.is_empty());
    }

    #[test]
    fn partial_header_reports_incomplete() {
        for len in 0..FRAME_HEADER_LEN {
            let input = vec![0u8; len];
            let error = decode_header(&input).expect_err("must be incomplete");
            assert_eq!(
                error,
                I2cpError::Incomplete {
                    needed: FRAME_HEADER_LEN - len
                },
                "partial header of {len} bytes"
            );
        }
    }

    #[test]
    fn oversize_length_rejected_before_allocation() {
        let over = (MAX_I2CP_BODY_BYTES + 1) as u32;
        let mut input = Vec::from(over.to_be_bytes());
        input.push(5);
        assert_eq!(
            decode_header(&input),
            Err(I2cpError::BodyTooLarge {
                declared: MAX_I2CP_BODY_BYTES + 1,
                maximum: MAX_I2CP_BODY_BYTES,
            })
        );
    }

    #[test]
    fn max_body_accepted_max_plus_one_rejected() {
        let body = vec![0xabu8; MAX_I2CP_BODY_BYTES];
        let encoded = encode_frame(5, &body).expect("max body must encode");
        let frame = decode_frame(&encoded).expect("max body must decode");
        assert_eq!(frame.body.len(), MAX_I2CP_BODY_BYTES);

        let over = vec![0xabu8; MAX_I2CP_BODY_BYTES + 1];
        assert_eq!(
            encode_frame(5, &over),
            Err(I2cpError::BodyTooLarge {
                declared: MAX_I2CP_BODY_BYTES + 1,
                maximum: MAX_I2CP_BODY_BYTES,
            })
        );
    }

    #[test]
    fn truncated_body_reports_incomplete() {
        let encoded = encode_frame(5, &[1, 2, 3, 4]).expect("encode");
        let cut = &encoded[..encoded.len() - 2];
        assert_eq!(decode_frame(cut), Err(I2cpError::Incomplete { needed: 2 }));
    }

    #[test]
    fn trailing_bytes_rejected() {
        let mut encoded = encode_frame(5, &[1, 2]).expect("encode");
        encoded.push(0xff);
        assert!(matches!(
            decode_frame(&encoded),
            Err(I2cpError::Malformed { .. })
        ));
    }

    #[test]
    fn incremental_decoder_supports_partial_reads() {
        let first = encode_frame(5, &[1, 2, 3]).expect("encode");
        let second = encode_frame(8, &[]).expect("encode");
        let mut decoder = FrameDecoder::new();
        // Feed one byte at a time: no frame until the tail completes.
        let mut bytes = first.iter().chain(second.iter());
        let mut frames = Vec::new();
        // Feed header byte-by-byte for the first frame.
        for byte in (&mut bytes).take(1) {
            frames.extend(decoder.push(std::slice::from_ref(byte)).expect("push"));
        }
        assert!(frames.is_empty());
        assert_eq!(decoder.buffered(), 1);
        for byte in bytes {
            frames.extend(decoder.push(std::slice::from_ref(byte)).expect("push"));
        }
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].message_type, 5);
        assert_eq!(frames[0].body, vec![1, 2, 3]);
        assert_eq!(frames[1].message_type, 8);
        assert!(frames[1].body.is_empty());
        assert_eq!(decoder.buffered(), 0);
    }

    #[test]
    fn decoder_buffer_cannot_grow_without_bound() {
        let mut decoder = FrameDecoder::new();
        let chunk = vec![0xffu8; FRAME_HEADER_LEN + MAX_I2CP_BODY_BYTES + 1];
        assert_eq!(
            decoder.push(&chunk),
            Err(I2cpError::BodyTooLarge {
                declared: chunk.len(),
                maximum: FRAME_HEADER_LEN + MAX_I2CP_BODY_BYTES,
            })
        );
    }
}
