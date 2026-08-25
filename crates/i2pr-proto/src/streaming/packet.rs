//! Streaming packet wire codec (Plan 128 normative form).
//!
//! The packet layout follows the current official Streaming
//! specification:
//!
//! ```text
//! sendStreamId      u32
//! receiveStreamId   u32
//! sequenceNum       u32
//! ackThrough        u32
//! nackCount         u8
//! NACKs             nackCount * u32
//! resendDelay       u8
//! flags             u16
//! optionSize        u16
//! optionData        optionSize bytes
//! payload           remaining bytes
//! ```
//!
//! # Flag bits (normative I2P assignment)
//!
//! ```text
//! bit 0   SYNCHRONIZE              0x0001
//! bit 1   CLOSE                    0x0002
//! bit 2   RESET                    0x0004
//! bit 3   SIGNATURE_INCLUDED       0x0008
//! bit 4   SIGNATURE_REQUESTED      0x0010
//! bit 5   FROM_INCLUDED            0x0020
//! bit 6   DELAY_REQUESTED          0x0040
//! bit 7   MAX_PACKET_SIZE_INCLUDED 0x0080
//! bit 8   PROFILE_INTERACTIVE      0x0100
//! bit 9   ECHO                     0x0200
//! bit 10  NO_ACK                   0x0400
//! bit 11  OFFLINE_SIGNATURE        0x0800
//! bits 12-15 reserved              0xF000
//! ```
//!
//! # Option region (not a TLV list)
//!
//! The 2-byte `optionSize` field gives the total option-data length.
//! The flags determine which structures are present and the
//! specification fixes their order:
//!
//! ```text
//! 1. DELAY_REQUESTED          2-byte integer, if present
//! 2. FROM_INCLUDED            self-encoded Destination, if present
//! 3. MAX_PACKET_SIZE_INCLUDED 2-byte big-endian integer, if present
//! 4. OFFLINE_SIGNATURE        OfflineSig, if present (unsupported here)
//! 5. SIGNATURE_INCLUDED       raw variable-length Signature, if present
//! ```
//!
//! There are no type/length/value records inside the option region.
//! The SIGNATURE field is the final option field and contains only the
//! raw signature bytes; its length is inferred from the signing key in
//! the `FROM_INCLUDED` Destination or, on an established connection,
//! from the peer signing key retained in connection state.
//!
//! The signature preimage is the complete packet with the raw
//! signature bytes set to zero.
//!
//! Unknown flag bits 12-15 must be zero for current compatibility.

use core::fmt;

use crate::codec::{CodecError, DecodeCursor, encode_to_vec};
use crate::common::{Destination, SigningPublicKey};

/// Minimum fixed packet header size: `4 + 4 + 4 + 4 + 1 + 1 + 2 + 2`
/// = 22 bytes (with `nackCount == 0`).
pub const MIN_STREAMING_HEADER_BYTES: usize = 22;
/// Default advertised maximum payload bytes carried by the
/// `MAX_PACKET_SIZE_INCLUDED` option. This bounds the Streaming
/// payload only, never the total packet size.
pub const DEFAULT_ADVERTISED_MAX_PAYLOAD: u16 = 1730;
/// Hard ceiling on the application payload region inside one packet.
/// This is the negotiated-payload ceiling only; the full encoded
/// packet may be larger because of header, NACKs, and options.
pub const MAX_STREAMING_PAYLOAD_BYTES: usize = DEFAULT_ADVERTISED_MAX_PAYLOAD as usize;
/// Hard ceiling on the option region size.
///
/// The streaming packet format reserves up to 65535 bytes for the
/// option region (a `u16` width); this implementation keeps the upper
/// bound below the maximum packet size ceiling while accommodating a
/// FROM destination (~397 bytes for Ed25519+X25519 destinations) plus
/// the DELAY / MAX_PACKET_SIZE / SIGNATURE fields.
pub const MAX_STREAMING_OPTION_BYTES: usize = 1024;
/// Hard ceiling on the number of NACK identifiers carried in one packet.
pub const MAX_STREAMING_NACK_COUNT: usize = 64;
/// Number of NACK entries the initial-SYN replay binding uses. Each
/// entry is `u32`, so 8 entries carry exactly 32 bytes of receiver
/// Destination hash (Proposal 164 replay prevention).
pub const SYN_REPLAY_NACK_COUNT: usize = 8;
/// Full encoded packet ceiling: checked sum of minimum header, NACKs,
/// options, and payload. Payload and option regions are bounded
/// independently of this value.
pub const MAX_STREAMING_PACKET_BYTES: usize = MIN_STREAMING_HEADER_BYTES
    + MAX_STREAMING_NACK_COUNT * 4
    + MAX_STREAMING_OPTION_BYTES
    + MAX_STREAMING_PAYLOAD_BYTES;

/// Bit 0 (`0x0001`) — `SYNCHRONIZE`.
pub const FLAG_SYNCHRONIZE: u16 = 0x0001;
/// Bit 1 (`0x0002`) — `CLOSE`.
pub const FLAG_CLOSE: u16 = 0x0002;
/// Bit 2 (`0x0004`) — `RESET`.
pub const FLAG_RESET: u16 = 0x0004;
/// Bit 3 (`0x0008`) — `SIGNATURE_INCLUDED`.
pub const FLAG_SIGNATURE_INCLUDED: u16 = 0x0008;
/// Bit 4 (`0x0010`) — `SIGNATURE_REQUESTED`.
pub const FLAG_SIGNATURE_REQUESTED: u16 = 0x0010;
/// Bit 5 (`0x0020`) — `FROM_INCLUDED`.
pub const FLAG_FROM_INCLUDED: u16 = 0x0020;
/// Bit 6 (`0x0040`) — `DELAY_REQUESTED`.
pub const FLAG_DELAY_REQUESTED: u16 = 0x0040;
/// Bit 7 (`0x0080`) — `MAX_PACKET_SIZE_INCLUDED`.
pub const FLAG_MAX_PACKET_SIZE_INCLUDED: u16 = 0x0080;
/// Bit 8 (`0x0100`) — `PROFILE_INTERACTIVE`.
pub const FLAG_PROFILE_INTERACTIVE: u16 = 0x0100;
/// Bit 9 (`0x0200`) — `ECHO`.
pub const FLAG_ECHO: u16 = 0x0200;
/// Bit 10 (`0x0400`) — `NO_ACK`.
pub const FLAG_NO_ACK: u16 = 0x0400;
/// Bit 11 (`0x0800`) — `OFFLINE_SIGNATURE`.
pub const FLAG_OFFLINE_SIGNATURE: u16 = 0x0800;
/// Reserved flag bits 12..=15. Receivers must reject packets with any
/// of these bits set.
pub const FLAG_RESERVED_MASK: u16 = 0xF000;

/// Initial originator SYN flag set:
/// `SYNCHRONIZE | SIGNATURE_INCLUDED | FROM_INCLUDED |
/// MAX_PACKET_SIZE_INCLUDED | NO_ACK` = `0x04A9`.
pub const INITIAL_SYN_FLAGS: u16 = FLAG_SYNCHRONIZE
    | FLAG_SIGNATURE_INCLUDED
    | FLAG_FROM_INCLUDED
    | FLAG_MAX_PACKET_SIZE_INCLUDED
    | FLAG_NO_ACK;
/// SYN response flag set:
/// `SYNCHRONIZE | SIGNATURE_INCLUDED | FROM_INCLUDED |
/// MAX_PACKET_SIZE_INCLUDED` = `0x00A9`. Proposal 164 explicitly
/// excludes both `NO_ACK` and the replay-prevention NACKs here.
pub const SYN_RESPONSE_FLAGS: u16 =
    FLAG_SYNCHRONIZE | FLAG_SIGNATURE_INCLUDED | FLAG_FROM_INCLUDED | FLAG_MAX_PACKET_SIZE_INCLUDED;
/// CLOSE flag set: `CLOSE | SIGNATURE_INCLUDED` = `0x000A`. FROM is
/// not required since 0.9.20; verification uses the retained peer
/// signing key.
pub const CLOSE_FLAGS: u16 = FLAG_CLOSE | FLAG_SIGNATURE_INCLUDED;
/// RESET flag set: `RESET | SIGNATURE_INCLUDED` = `0x000C`.
pub const RESET_FLAGS: u16 = FLAG_RESET | FLAG_SIGNATURE_INCLUDED;

/// Flag set captured as a typed value. Builders use this to avoid
/// silent flag-bit mixups at the connection layer.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StreamingFlags(u16);

impl StreamingFlags {
    /// Wraps a raw flag set after rejecting reserved bits.
    pub const fn new(bits: u16) -> Result<Self, StreamingPacketError> {
        if bits & FLAG_RESERVED_MASK != 0 {
            return Err(StreamingPacketError::ReservedFlagBits(bits));
        }
        Ok(Self(bits))
    }

    /// Returns the empty flag set.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns the raw flag bits.
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Combines another flag set with this one.
    pub const fn union(self, other: Self) -> Result<Self, StreamingPacketError> {
        Self::new(self.0 | other.0)
    }

    /// Returns whether the SYNCHRONIZE flag is set.
    pub const fn synchronize(self) -> bool {
        self.0 & FLAG_SYNCHRONIZE != 0
    }

    /// Returns whether the CLOSE flag is set.
    pub const fn close(self) -> bool {
        self.0 & FLAG_CLOSE != 0
    }

    /// Returns whether the RESET flag is set.
    pub const fn reset(self) -> bool {
        self.0 & FLAG_RESET != 0
    }

    /// Returns whether `SIGNATURE_INCLUDED` is set.
    pub const fn signature_included(self) -> bool {
        self.0 & FLAG_SIGNATURE_INCLUDED != 0
    }

    /// Returns whether `SIGNATURE_REQUESTED` is set.
    pub const fn signature_requested(self) -> bool {
        self.0 & FLAG_SIGNATURE_REQUESTED != 0
    }

    /// Returns whether `FROM_INCLUDED` is set.
    pub const fn from_included(self) -> bool {
        self.0 & FLAG_FROM_INCLUDED != 0
    }

    /// Returns whether `DELAY_REQUESTED` is set.
    pub const fn delay_requested(self) -> bool {
        self.0 & FLAG_DELAY_REQUESTED != 0
    }

    /// Returns whether `MAX_PACKET_SIZE_INCLUDED` is set.
    pub const fn max_packet_size_included(self) -> bool {
        self.0 & FLAG_MAX_PACKET_SIZE_INCLUDED != 0
    }

    /// Returns whether `PROFILE_INTERACTIVE` is set.
    pub const fn profile_interactive(self) -> bool {
        self.0 & FLAG_PROFILE_INTERACTIVE != 0
    }

    /// Returns whether `ECHO` is set.
    pub const fn echo(self) -> bool {
        self.0 & FLAG_ECHO != 0
    }

    /// Returns whether `NO_ACK` is set.
    pub const fn no_ack(self) -> bool {
        self.0 & FLAG_NO_ACK != 0
    }

    /// Returns whether `OFFLINE_SIGNATURE` is set.
    pub const fn offline_signature(self) -> bool {
        self.0 & FLAG_OFFLINE_SIGNATURE != 0
    }
}

/// Caller-supplied decoder limits. The strict decoder never allocates
/// based solely on attacker-controlled byte counts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamingReceiveLimit {
    /// Maximum allowed packet bytes including header, NACKs, options,
    /// payload.
    pub max_packet_bytes: usize,
    /// Maximum allowed option region size in bytes.
    pub max_option_bytes: usize,
    /// Maximum allowed NACK count.
    pub max_nack_count: usize,
    /// Maximum allowed payload bytes.
    pub max_payload_bytes: usize,
}

impl Default for StreamingReceiveLimit {
    fn default() -> Self {
        Self {
            max_packet_bytes: MAX_STREAMING_PACKET_BYTES,
            max_option_bytes: MAX_STREAMING_OPTION_BYTES,
            max_nack_count: MAX_STREAMING_NACK_COUNT,
            max_payload_bytes: MAX_STREAMING_PAYLOAD_BYTES,
        }
    }
}

/// Caller-supplied encoder limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamingSendLimit {
    /// Maximum packet bytes the encoder is allowed to emit.
    pub max_packet_bytes: usize,
    /// Maximum option bytes the encoder is allowed to emit.
    pub max_option_bytes: usize,
    /// Maximum NACK count the encoder is allowed to emit.
    pub max_nack_count: usize,
    /// Maximum payload bytes the encoder is allowed to emit.
    pub max_payload_bytes: usize,
}

impl Default for StreamingSendLimit {
    fn default() -> Self {
        Self {
            max_packet_bytes: MAX_STREAMING_PACKET_BYTES,
            max_option_bytes: MAX_STREAMING_OPTION_BYTES,
            max_nack_count: MAX_STREAMING_NACK_COUNT,
            max_payload_bytes: MAX_STREAMING_PAYLOAD_BYTES,
        }
    }
}

/// Typed failure surface for streaming packet encode/decode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamingPacketError {
    /// The strict top-level decoder found trailing bytes.
    TrailingBytes,
    /// The input ended before the header was complete.
    Truncated,
    /// The option region declares more bytes than the configured limit.
    OptionOverflow {
        /// Declared option bytes.
        declared: usize,
        /// Configured maximum.
        maximum: usize,
    },
    /// The NACK count declares more entries than the configured limit.
    NackOverflow {
        /// Declared NACK count.
        declared: usize,
        /// Configured maximum.
        maximum: usize,
    },
    /// The payload region declares more bytes than the configured limit.
    PayloadOverflow {
        /// Declared payload bytes.
        declared: usize,
        /// Configured maximum.
        maximum: usize,
    },
    /// A length-prefixed region declares more bytes than the configured
    /// limit.
    LengthExceeded {
        /// Static context label.
        context: &'static str,
        /// Declared length.
        declared: usize,
        /// Configured maximum.
        maximum: usize,
    },
    /// A SYN packet is missing `FROM_INCLUDED`.
    SynMissingFrom,
    /// A SYN packet is missing `SIGNATURE_INCLUDED`.
    SynMissingSignature,
    /// A SYN packet is missing `MAX_PACKET_SIZE_INCLUDED`.
    SynMissingMaxPacketSize,
    /// A SYN packet is missing the Bob-hash replay binding.
    SynMissingReplayBinding,
    /// The packet carries reserved flag bits 12-15 set.
    ReservedFlagBits(u16),
    /// A signed packet's raw signature length disagrees with the length
    /// inferred from the signing-key context.
    SignatureLengthMismatch {
        /// Expected signature length.
        expected: usize,
        /// Observed signature length.
        actual: usize,
    },
    /// The minimum SYN replay binding requires exactly `nackCount == 8`.
    SynReplayNackCountMismatch {
        /// Observed `nackCount`.
        observed: usize,
    },
    /// An arithmetic operation overflowed during decode.
    ArithmeticOverflow,
    /// The signature field is missing from a packet that requires it.
    SignatureMissing,
    /// A signed packet's preimage signature verification failed.
    SignatureInvalid,
    /// A signed packet carries neither `FROM_INCLUDED` nor a peer
    /// signing key in the decode context, so the signature length
    /// cannot be inferred. The decoder fails closed instead of
    /// guessing.
    SignatureContextUnavailable,
    /// The packet sets `OFFLINE_SIGNATURE`, which this implementation
    /// does not support. The decoder rejects before misparsing later
    /// option fields.
    UnsupportedOfflineSignature,
    /// A CLOSE packet was sent without `SIGNATURE_INCLUDED`.
    CloseMissingSignature,
    /// A RESET packet was sent without `SIGNATURE_INCLUDED`.
    ResetMissingSignature,
}

impl fmt::Display for StreamingPacketError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TrailingBytes => formatter.write_str("trailing bytes after streaming packet"),
            Self::Truncated => formatter.write_str("truncated streaming packet"),
            Self::OptionOverflow { declared, maximum } => write!(
                formatter,
                "streaming option region {declared} exceeds {maximum}-byte limit"
            ),
            Self::NackOverflow { declared, maximum } => write!(
                formatter,
                "streaming NACK count {declared} exceeds {maximum}-entry limit"
            ),
            Self::PayloadOverflow { declared, maximum } => write!(
                formatter,
                "streaming payload {declared} exceeds {maximum}-byte limit"
            ),
            Self::LengthExceeded {
                context,
                declared,
                maximum,
            } => write!(
                formatter,
                "streaming length exceeded for {context}: {declared} > {maximum}"
            ),
            Self::SynMissingFrom => formatter.write_str("SYN missing FROM_INCLUDED"),
            Self::SynMissingSignature => formatter.write_str("SYN missing SIGNATURE_INCLUDED"),
            Self::SynMissingMaxPacketSize => {
                formatter.write_str("SYN missing MAX_PACKET_SIZE_INCLUDED")
            }
            Self::SynMissingReplayBinding => {
                formatter.write_str("SYN missing receiver Destination hash replay binding")
            }
            Self::ReservedFlagBits(bits) => write!(
                formatter,
                "streaming packet carries reserved flag bits {bits:#06x}"
            ),
            Self::SignatureLengthMismatch { expected, actual } => write!(
                formatter,
                "streaming signature length {actual} disagrees with expected {expected}"
            ),
            Self::SynReplayNackCountMismatch { observed } => write!(
                formatter,
                "SYN replay binding requires nackCount == 8, observed {observed}"
            ),
            Self::ArithmeticOverflow => {
                formatter.write_str("streaming decoder arithmetic overflow")
            }
            Self::SignatureMissing => formatter.write_str("streaming signature missing"),
            Self::SignatureInvalid => {
                formatter.write_str("streaming signature verification failed")
            }
            Self::SignatureContextUnavailable => formatter.write_str(
                "streaming signature context unavailable (no FROM and no peer signing key)",
            ),
            Self::UnsupportedOfflineSignature => {
                formatter.write_str("streaming OFFLINE_SIGNATURE not supported")
            }
            Self::CloseMissingSignature => {
                formatter.write_str("streaming CLOSE missing SIGNATURE_INCLUDED")
            }
            Self::ResetMissingSignature => {
                formatter.write_str("streaming RESET missing SIGNATURE_INCLUDED")
            }
        }
    }
}

impl std::error::Error for StreamingPacketError {}

impl From<CodecError> for StreamingPacketError {
    fn from(error: CodecError) -> Self {
        match error {
            CodecError::Truncated { .. } => Self::Truncated,
            CodecError::TrailingBytes { .. } => Self::TrailingBytes,
            CodecError::ArithmeticOverflow { .. } => Self::ArithmeticOverflow,
            CodecError::LengthExceeded {
                declared, maximum, ..
            } => Self::LengthExceeded {
                context: "streaming codec",
                declared,
                maximum,
            },
            _other => Self::LengthExceeded {
                context: "streaming codec",
                declared: 0,
                maximum: 0,
            },
        }
    }
}

/// Parsed semantic Streaming option fields, decoded per the packet
/// flags in normative order. The encoder writes these fields in the
/// same order; there are no type/length/value records.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StreamingOptions {
    /// `DELAY_REQUESTED` 2-byte integer, when the flag is set.
    pub delay_requested: Option<u16>,
    /// `FROM_INCLUDED` self-encoded Destination, when the flag is set.
    pub from_destination: Option<Destination>,
    /// `MAX_PACKET_SIZE_INCLUDED` 2-byte big-endian integer bounding
    /// the Streaming payload only, when the flag is set.
    pub max_payload_size: Option<u16>,
    /// `SIGNATURE_INCLUDED` raw signature bytes, when the flag is set.
    /// The length comes from signing-key context, never from a wire
    /// prefix.
    pub signature: Option<Vec<u8>>,
}

impl StreamingOptions {
    /// Encodes the option region in normative flag order with exactly
    /// `signature_length` zero bytes occupying the final
    /// `SIGNATURE_INCLUDED` position. The caller signs the resulting
    /// full packet (whose signature bytes are already zero, i.e. the
    /// canonical preimage) and then patches the real signature in via
    /// [`install_packet_signature`].
    ///
    /// Requires `signature: None`; the signing path must never encode
    /// over live signature bytes. Passing a populated `signature`
    /// field fails closed with [`StreamingPacketError::SignatureMissing`].
    #[allow(clippy::result_large_err)]
    pub fn encode_with_placeholder(
        &self,
        flags: StreamingFlags,
        signature_length: usize,
    ) -> Result<Vec<u8>, StreamingPacketError> {
        if self.signature.is_some() {
            return Err(StreamingPacketError::SignatureMissing);
        }
        let mut out = Vec::new();
        if flags.delay_requested() {
            let delay = self
                .delay_requested
                .ok_or(StreamingPacketError::LengthExceeded {
                    context: "delay requested option",
                    declared: 0,
                    maximum: 0,
                })?;
            out.extend_from_slice(&delay.to_be_bytes());
        }
        if flags.from_included() {
            let destination = self
                .from_destination
                .as_ref()
                .ok_or(StreamingPacketError::SynMissingFrom)?;
            out.extend_from_slice(
                &destination.encode_to_vec(crate::common::MAX_COMMON_STRUCTURE_SIZE)?,
            );
        }
        if flags.max_packet_size_included() {
            let max = self
                .max_payload_size
                .ok_or(StreamingPacketError::SynMissingMaxPacketSize)?;
            out.extend_from_slice(&max.to_be_bytes());
        }
        if flags.offline_signature() {
            return Err(StreamingPacketError::UnsupportedOfflineSignature);
        }
        if flags.signature_included() {
            out.resize(out.len() + signature_length, 0_u8);
        }
        if out.len() > MAX_STREAMING_OPTION_BYTES {
            return Err(StreamingPacketError::OptionOverflow {
                declared: out.len(),
                maximum: MAX_STREAMING_OPTION_BYTES,
            });
        }
        Ok(out)
    }
}

fn destination_len(destination: &Destination) -> Result<usize, StreamingPacketError> {
    let encoded = destination.encode_to_vec(crate::common::MAX_COMMON_STRUCTURE_SIZE)?;
    Ok(encoded.len())
}

/// Identifies where in the wire buffer the raw signature bytes live so
/// the verification layer can zero them while computing the canonical
/// signature preimage. The location covers only the signature bytes —
/// there is no type/length prefix on the wire.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignatureLocation {
    /// Absolute offset of the first raw signature byte.
    pub offset: usize,
    /// Number of raw signature bytes.
    pub length: usize,
}

/// Decoded Streaming packet with parsed options and payload bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamingPacket {
    /// The sender's stream identifier.
    pub send_stream_id: u32,
    /// The receiver's stream identifier chosen by the sender.
    pub receive_stream_id: u32,
    /// The packet's sequence number.
    pub sequence_num: u32,
    /// `ackThrough` cumulative acknowledgement.
    pub ack_through: u32,
    /// NACK identifiers carried in the NACK field.
    pub nacks: Vec<u32>,
    /// Resend delay hint in seconds.
    pub resend_delay: u8,
    /// Decoded flag set.
    pub flags: StreamingFlags,
    /// Raw option region bytes (signature bytes preserved).
    pub option_bytes: Vec<u8>,
    /// Parsed semantic options per the packet flags.
    pub options: StreamingOptions,
    /// Application payload bytes after the option region.
    pub payload: Vec<u8>,
}

/// Explicit decode context for option fields whose lengths depend on
/// connection state rather than the packet itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamingOptionDecodeContext<'a> {
    /// Peer signing key retained in connection state. Used to infer
    /// the `SIGNATURE_INCLUDED` field length when `FROM_INCLUDED` is
    /// absent (established-connection control packets such as
    /// CLOSE/RESET after 0.9.20).
    pub peer_signing_key: Option<&'a SigningPublicKey>,
}

impl<'a> StreamingOptionDecodeContext<'a> {
    /// Context without retained peer state. Signed packets must then
    /// carry `FROM_INCLUDED` for their signatures to parse.
    pub const fn anonymous() -> Self {
        Self {
            peer_signing_key: None,
        }
    }

    /// Context carrying the established connection's peer signing key.
    pub const fn with_peer_key(key: &'a SigningPublicKey) -> Self {
        Self {
            peer_signing_key: Some(key),
        }
    }
}

/// Lightweight header peek used to route a packet before full option
/// parsing. Routing decisions (initial SYN vs SYN response vs
/// established-connection traffic) need only the stream IDs and flag
/// bits; the full decode runs afterwards with the correct option
/// context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamingHeaderPeek {
    /// The sender's stream identifier.
    pub send_stream_id: u32,
    /// The receiver's stream identifier chosen by the sender.
    pub receive_stream_id: u32,
    /// The packet's sequence number.
    pub sequence_num: u32,
    /// Raw flag bits (reserved-bit validation happens in the strict
    /// decoder).
    pub flags_bits: u16,
}

/// Peeks the fixed streaming header fields without parsing the option
/// region. Fails closed when the input cannot hold the declared
/// header plus NACK words.
#[allow(clippy::result_large_err)]
pub fn peek_streaming_header(input: &[u8]) -> Result<StreamingHeaderPeek, StreamingPacketError> {
    if input.len() < MIN_STREAMING_HEADER_BYTES {
        return Err(StreamingPacketError::Truncated);
    }
    let send_stream_id = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
    let receive_stream_id = u32::from_be_bytes([input[4], input[5], input[6], input[7]]);
    let sequence_num = u32::from_be_bytes([input[8], input[9], input[10], input[11]]);
    let nack_count = input[16] as usize;
    let header_end = MIN_STREAMING_HEADER_BYTES
        .checked_add(nack_count * 4)
        .ok_or(StreamingPacketError::ArithmeticOverflow)?;
    if input.len() < header_end {
        return Err(StreamingPacketError::Truncated);
    }
    let flags_offset = 18 + nack_count * 4;
    let flags_bits = u16::from_be_bytes([input[flags_offset], input[flags_offset + 1]]);
    Ok(StreamingHeaderPeek {
        send_stream_id,
        receive_stream_id,
        sequence_num,
        flags_bits,
    })
}

/// Decodes a streaming packet from the wire form, also returning the
/// absolute offset and length of the raw signature bytes (when present)
/// so the caller can verify the signature over the canonical bytes with
/// the signature zeroed.
#[allow(clippy::result_large_err)]
pub fn decode_streaming_packet(
    input: &[u8],
    limit: StreamingReceiveLimit,
    context: StreamingOptionDecodeContext<'_>,
) -> Result<(StreamingPacket, Option<SignatureLocation>), StreamingPacketError> {
    if input.len() > limit.max_packet_bytes {
        return Err(StreamingPacketError::PayloadOverflow {
            declared: input.len(),
            maximum: limit.max_packet_bytes,
        });
    }
    if input.len() < MIN_STREAMING_HEADER_BYTES {
        return Err(StreamingPacketError::Truncated);
    }

    let mut cursor =
        DecodeCursor::new(input, limit.max_packet_bytes).map_err(StreamingPacketError::from)?;
    let send_stream_id = cursor.read_u32().map_err(StreamingPacketError::from)?;
    let receive_stream_id = cursor.read_u32().map_err(StreamingPacketError::from)?;
    let sequence_num = cursor.read_u32().map_err(StreamingPacketError::from)?;
    let ack_through = cursor.read_u32().map_err(StreamingPacketError::from)?;
    let nack_count = cursor.read_u8().map_err(StreamingPacketError::from)? as usize;
    if nack_count > limit.max_nack_count {
        return Err(StreamingPacketError::NackOverflow {
            declared: nack_count,
            maximum: limit.max_nack_count,
        });
    }
    let mut nacks = Vec::with_capacity(nack_count);
    for _ in 0..nack_count {
        nacks.push(cursor.read_u32().map_err(StreamingPacketError::from)?);
    }
    let resend_delay = cursor.read_u8().map_err(StreamingPacketError::from)?;
    let flags_bits = cursor.read_u16().map_err(StreamingPacketError::from)?;
    let flags = StreamingFlags::new(flags_bits)?;
    let option_size = cursor.read_u16().map_err(StreamingPacketError::from)? as usize;
    if option_size > limit.max_option_bytes {
        return Err(StreamingPacketError::OptionOverflow {
            declared: option_size,
            maximum: limit.max_option_bytes,
        });
    }
    let option_start = cursor.offset();
    let option_end = option_start
        .checked_add(option_size)
        .ok_or(StreamingPacketError::ArithmeticOverflow)?;
    if option_end > input.len() {
        return Err(StreamingPacketError::Truncated);
    }
    let option_bytes = input[option_start..option_end].to_vec();
    let payload = if option_end < input.len() {
        input[option_end..].to_vec()
    } else {
        Vec::new()
    };
    if payload.len() > limit.max_payload_bytes {
        return Err(StreamingPacketError::PayloadOverflow {
            declared: payload.len(),
            maximum: limit.max_payload_bytes,
        });
    }

    let (options, location) = parse_options(&flags, &option_bytes, option_start, &context)?;

    Ok((
        StreamingPacket {
            send_stream_id,
            receive_stream_id,
            sequence_num,
            ack_through,
            nacks,
            resend_delay,
            flags,
            option_bytes,
            options,
            payload,
        },
        location,
    ))
}

/// Parses the option region according to the packet flags in normative
/// order. Fails closed on unsupported layouts, truncated fields, and
/// unparsed trailing option data.
#[allow(clippy::result_large_err)]
fn parse_options(
    flags: &StreamingFlags,
    option_bytes: &[u8],
    option_start: usize,
    context: &StreamingOptionDecodeContext<'_>,
) -> Result<(StreamingOptions, Option<SignatureLocation>), StreamingPacketError> {
    let mut options = StreamingOptions::default();
    let mut position = 0usize;

    // 1. DELAY_REQUESTED — 2-byte integer.
    if flags.delay_requested() {
        let end = position
            .checked_add(2)
            .ok_or(StreamingPacketError::ArithmeticOverflow)?;
        let field = option_bytes
            .get(position..end)
            .ok_or(StreamingPacketError::Truncated)?;
        options.delay_requested = Some(u16::from_be_bytes([field[0], field[1]]));
        position = end;
    }

    // 2. FROM_INCLUDED — self-encoded Destination (no prefix).
    if flags.from_included() {
        let remaining = option_bytes
            .get(position..)
            .ok_or(StreamingPacketError::Truncated)?;
        let destination =
            Destination::decode_from_cursor(remaining, crate::common::MAX_COMMON_STRUCTURE_SIZE)
                .map_err(StreamingPacketError::from)?;
        // The canonical re-encoding length equals the consumed wire
        // length because the common-structure codecs are canonical.
        let consumed = destination_len(&destination)?;
        options.from_destination = Some(destination);
        position += consumed;
    }

    // 3. MAX_PACKET_SIZE_INCLUDED — 2-byte big-endian integer bounding
    //    the Streaming payload only.
    if flags.max_packet_size_included() {
        let end = position
            .checked_add(2)
            .ok_or(StreamingPacketError::ArithmeticOverflow)?;
        let field = option_bytes
            .get(position..end)
            .ok_or(StreamingPacketError::Truncated)?;
        options.max_payload_size = Some(u16::from_be_bytes([field[0], field[1]]));
        position = end;
    }

    // 4. OFFLINE_SIGNATURE — rejected before any later field could be
    //    misparsed against the wrong layout.
    if flags.offline_signature() {
        return Err(StreamingPacketError::UnsupportedOfflineSignature);
    }

    // 5. SIGNATURE_INCLUDED — final field, raw variable-length
    //    signature bytes. Length inference: FROM destination signing
    //    key first, retained peer signing key second; no context means
    //    fail closed.
    let mut location = None;
    if flags.signature_included() {
        let expected = if let Some(destination) = options.from_destination.as_ref() {
            destination
                .signing_key()
                .key_type()
                .signature_len()
                .ok_or(StreamingPacketError::SignatureContextUnavailable)?
        } else if let Some(key) = context.peer_signing_key {
            key.key_type()
                .signature_len()
                .ok_or(StreamingPacketError::SignatureContextUnavailable)?
        } else {
            return Err(StreamingPacketError::SignatureContextUnavailable);
        };
        let end = position
            .checked_add(expected)
            .ok_or(StreamingPacketError::ArithmeticOverflow)?;
        let signature = option_bytes
            .get(position..end)
            .ok_or(StreamingPacketError::SignatureLengthMismatch {
                expected,
                actual: option_bytes.len().saturating_sub(position),
            })?
            .to_vec();
        location = Some(SignatureLocation {
            offset: option_start + position,
            length: expected,
        });
        options.signature = Some(signature);
        position = end;
    }

    if position != option_bytes.len() {
        return Err(StreamingPacketError::TrailingBytes);
    }
    Ok((options, location))
}

/// Builds a Streaming signature preimage: the supplied wire bytes with
/// the supplied raw signature bytes zeroed.
pub fn build_signature_preimage(
    wire_bytes: &[u8],
    signature_location: Option<SignatureLocation>,
) -> Vec<u8> {
    let mut preimage = wire_bytes.to_vec();
    if let Some(location) = signature_location {
        let end = location
            .offset
            .checked_add(location.length)
            .unwrap_or(preimage.len());
        if location.offset <= preimage.len() && end <= preimage.len() {
            for byte in preimage
                .iter_mut()
                .skip(location.offset)
                .take(location.length)
            {
                *byte = 0;
            }
        }
    }
    preimage
}

/// Overwrites the zeroed signature placeholder at the tail of the
/// encoded packet with the real signature bytes and returns the
/// absolute wire offset written. The placeholder must be exactly
/// `signature.len()` zero bytes at the end of the packet; anything
/// else fails closed.
#[allow(clippy::result_large_err)]
pub fn install_packet_signature(
    wire: &mut [u8],
    signature: &[u8],
) -> Result<usize, StreamingPacketError> {
    if signature.is_empty() {
        return Err(StreamingPacketError::SignatureMissing);
    }
    if wire.len() < signature.len() {
        return Err(StreamingPacketError::Truncated);
    }
    let offset = wire.len() - signature.len();
    if wire[offset..].iter().any(|byte| *byte != 0) {
        return Err(StreamingPacketError::SignatureInvalid);
    }
    wire[offset..].copy_from_slice(signature);
    Ok(offset)
}

/// Builder input for [`encode_streaming_packet`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamingPacketBuilder {
    pub send_stream_id: u32,
    pub receive_stream_id: u32,
    pub sequence_num: u32,
    pub ack_through: u32,
    pub nacks: Vec<u32>,
    pub resend_delay: u8,
    pub flags: StreamingFlags,
    pub option_bytes: Vec<u8>,
    pub payload: Vec<u8>,
}

impl StreamingPacketBuilder {
    /// Constructs an initial originator SYN packet builder with the
    /// canonical `INITIAL_SYN_FLAGS` (`0x04A9`). Plan 128 §7:
    /// `send_stream_id` must be 0, `receive_stream_id` carries the
    /// locally selected nonzero id, `sequence_num` is 0, and `nacks`
    /// carries the eight replay-binding words (the remote Destination
    /// hash).
    #[allow(clippy::result_large_err)]
    pub fn new_initial_syn(
        send_stream_id: u32,
        receive_stream_id: u32,
        sequence_num: u32,
        option_bytes: Vec<u8>,
        nacks: Vec<u32>,
    ) -> Result<Self, StreamingPacketError> {
        let flags = StreamingFlags::new(INITIAL_SYN_FLAGS)?;
        Ok(Self {
            send_stream_id,
            receive_stream_id,
            sequence_num,
            ack_through: 0,
            nacks,
            resend_delay: 0,
            flags,
            option_bytes,
            payload: Vec::new(),
        })
    }

    /// Constructs a SYN response packet builder with the canonical
    /// `SYN_RESPONSE_FLAGS` (`0x00A9`). Proposal 164: the response
    /// carries no replay-prevention NACKs and does not set `NO_ACK`;
    /// `ack_through` remains valid and acknowledges the initial SYN.
    #[allow(clippy::result_large_err)]
    pub fn new_syn_response(
        send_stream_id: u32,
        receive_stream_id: u32,
        sequence_num: u32,
        ack_through: u32,
        option_bytes: Vec<u8>,
    ) -> Result<Self, StreamingPacketError> {
        let flags = StreamingFlags::new(SYN_RESPONSE_FLAGS)?;
        Ok(Self {
            send_stream_id,
            receive_stream_id,
            sequence_num,
            ack_through,
            nacks: Vec::new(),
            resend_delay: 0,
            flags,
            option_bytes,
            payload: Vec::new(),
        })
    }
}

/// Encodes a streaming packet from the typed builder input and returns
/// the wire bytes. Callers producing signed packets must encode the
/// option region with a zeroed signature placeholder
/// ([`StreamingOptions::encode_with_placeholder`]), sign the returned
/// zeroed bytes directly (they already equal the canonical preimage),
/// and patch the signature in with [`install_packet_signature`].
#[allow(clippy::result_large_err)]
pub fn encode_streaming_packet(
    packet: &StreamingPacketBuilder,
    limit: StreamingSendLimit,
) -> Result<Vec<u8>, StreamingPacketError> {
    if packet.nacks.len() > limit.max_nack_count {
        return Err(StreamingPacketError::NackOverflow {
            declared: packet.nacks.len(),
            maximum: limit.max_nack_count,
        });
    }
    if packet.option_bytes.len() > limit.max_option_bytes {
        return Err(StreamingPacketError::OptionOverflow {
            declared: packet.option_bytes.len(),
            maximum: limit.max_option_bytes,
        });
    }
    if packet.payload.len() > limit.max_payload_bytes {
        return Err(StreamingPacketError::PayloadOverflow {
            declared: packet.payload.len(),
            maximum: limit.max_payload_bytes,
        });
    }

    let header_size = MIN_STREAMING_HEADER_BYTES
        .checked_add(packet.nacks.len() * 4)
        .ok_or(StreamingPacketError::ArithmeticOverflow)?;
    let total_size = header_size
        .checked_add(packet.option_bytes.len())
        .and_then(|v| v.checked_add(packet.payload.len()))
        .ok_or(StreamingPacketError::ArithmeticOverflow)?;
    if total_size > limit.max_packet_bytes {
        return Err(StreamingPacketError::PayloadOverflow {
            declared: total_size,
            maximum: limit.max_packet_bytes,
        });
    }

    let header = encode_to_vec(limit.max_packet_bytes, |encoder| {
        encoder.write_u32(packet.send_stream_id)?;
        encoder.write_u32(packet.receive_stream_id)?;
        encoder.write_u32(packet.sequence_num)?;
        encoder.write_u32(packet.ack_through)?;
        encoder.write_u8(u8::try_from(packet.nacks.len()).map_err(|_| {
            CodecError::InvalidFieldValue {
                offset: encoder.len(),
                context: "nack count conversion",
            }
        })?)?;
        for nack in &packet.nacks {
            encoder.write_u32(*nack)?;
        }
        encoder.write_u8(packet.resend_delay)?;
        encoder.write_u16(packet.flags.bits())?;
        encoder.write_u16(u16::try_from(packet.option_bytes.len()).map_err(|_| {
            CodecError::InvalidFieldValue {
                offset: encoder.len(),
                context: "option bytes conversion",
            }
        })?)?;
        encoder.write_raw(&packet.option_bytes)
    })
    .map_err(StreamingPacketError::from)?;
    let mut out = header;
    out.extend_from_slice(&packet.payload);
    Ok(out)
}

/// Verifies a SYN replay binding against the supplied receiver
/// Destination hash. Returns `Ok(true)` when the binding matches,
/// `Ok(false)` when the binding does not match the supplied hash, and
/// an error otherwise.
#[allow(clippy::result_large_err)]
pub fn verify_syn_replay_binding(
    packet: &StreamingPacket,
    receiver_destination_hash: &[u8; 32],
) -> Result<bool, StreamingPacketError> {
    if packet.nacks.len() != SYN_REPLAY_NACK_COUNT {
        return Err(StreamingPacketError::SynReplayNackCountMismatch {
            observed: packet.nacks.len(),
        });
    }
    let mut buffer = [0_u8; 32];
    for (index, nack) in packet.nacks.iter().enumerate() {
        let offset = index * 4;
        buffer[offset..offset + 4].copy_from_slice(&nack.to_be_bytes());
    }
    Ok(buffer == *receiver_destination_hash)
}

/// Encodes the canonical 32-byte SYN replay binding value (the
/// receiver Destination hash) as the eight `u32` NACK entries the
/// initial SYN packet carries.
pub fn encode_syn_replay_binding(
    receiver_destination_hash: &[u8; 32],
) -> [u32; SYN_REPLAY_NACK_COUNT] {
    let mut out = [0_u32; SYN_REPLAY_NACK_COUNT];
    for (index, word) in out.iter_mut().enumerate() {
        let offset = index * 4;
        *word = u32::from_be_bytes([
            receiver_destination_hash[offset],
            receiver_destination_hash[offset + 1],
            receiver_destination_hash[offset + 2],
            receiver_destination_hash[offset + 3],
        ]);
    }
    out
}

/// Validates an initial originator SYN against the current protocol
/// policy (Plan 128 §7): SYNCHRONIZE with FROM, SIGNATURE, and
/// MAX_PACKET_SIZE included, plus the Proposal 164 replay binding
/// (exactly eight NACK words carrying the receiver Destination hash).
#[allow(clippy::result_large_err)]
pub fn validate_initial_syn(
    packet: &StreamingPacket,
    receiver_destination_hash: &[u8; 32],
) -> Result<(), StreamingPacketError> {
    if !packet.flags.synchronize() {
        return Err(StreamingPacketError::SynMissingReplayBinding);
    }
    if !packet.flags.from_included() {
        return Err(StreamingPacketError::SynMissingFrom);
    }
    if !packet.flags.signature_included() {
        return Err(StreamingPacketError::SynMissingSignature);
    }
    if !packet.flags.max_packet_size_included() {
        return Err(StreamingPacketError::SynMissingMaxPacketSize);
    }
    if !verify_syn_replay_binding(packet, receiver_destination_hash)? {
        return Err(StreamingPacketError::SynMissingReplayBinding);
    }
    Ok(())
}

/// Validates a SYN response against the current protocol policy
/// (Plan 128 §8): SYNCHRONIZE with FROM, SIGNATURE, and
/// MAX_PACKET_SIZE included. The initial-SYN Bob-hash replay binding
/// is deliberately NOT required on the response (Proposal 164).
#[allow(clippy::result_large_err)]
pub fn validate_syn_response(packet: &StreamingPacket) -> Result<(), StreamingPacketError> {
    if !packet.flags.synchronize() {
        return Err(StreamingPacketError::SynMissingFrom);
    }
    if !packet.flags.from_included() {
        return Err(StreamingPacketError::SynMissingFrom);
    }
    if !packet.flags.signature_included() {
        return Err(StreamingPacketError::SynMissingSignature);
    }
    if !packet.flags.max_packet_size_included() {
        return Err(StreamingPacketError::SynMissingMaxPacketSize);
    }
    Ok(())
}

/// Validates a signed packet has raw signature bytes whose length
/// matches the supplied signing key type.
#[allow(clippy::result_large_err)]
pub fn validate_signature_policy(
    packet: &StreamingPacket,
    signing_key: &SigningPublicKey,
) -> Result<(), StreamingPacketError> {
    let expected = signing_key
        .key_type()
        .signature_len()
        .ok_or(StreamingPacketError::SignatureContextUnavailable)?;
    let actual = packet
        .options
        .signature
        .as_ref()
        .map(Vec::len)
        .ok_or(StreamingPacketError::SignatureMissing)?;
    if expected != actual {
        return Err(StreamingPacketError::SignatureLengthMismatch { expected, actual });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{Certificate, CryptoKeyType, KeyAndCert, KeyCertificate, SigningKeyType};

    fn ed_destination() -> Destination {
        let public_len = CryptoKeyType::X25519.public_key_len().unwrap();
        let signing_len = SigningKeyType::EdDsaSha512Ed25519.public_key_len().unwrap();
        let padding_len = 384 - public_len - signing_len;
        let keys = KeyAndCert::new(
            crate::PublicKey::new(CryptoKeyType::X25519, vec![0x11; public_len]).unwrap(),
            SigningPublicKey::new(SigningKeyType::EdDsaSha512Ed25519, vec![0x22; signing_len])
                .unwrap(),
            vec![0x33; padding_len],
            Certificate::Key(
                KeyCertificate::for_types(
                    SigningKeyType::EdDsaSha512Ed25519,
                    CryptoKeyType::X25519,
                )
                .unwrap(),
            ),
        )
        .unwrap();
        Destination::new(keys).unwrap()
    }

    #[test]
    fn empty_packet_round_trip_minimum_header_only() {
        let builder = StreamingPacketBuilder {
            send_stream_id: 0,
            receive_stream_id: 0,
            sequence_num: 0,
            ack_through: 0,
            nacks: Vec::new(),
            resend_delay: 0,
            flags: StreamingFlags::empty(),
            option_bytes: Vec::new(),
            payload: Vec::new(),
        };
        let encoded = encode_streaming_packet(&builder, StreamingSendLimit::default()).unwrap();
        assert_eq!(encoded.len(), MIN_STREAMING_HEADER_BYTES);
        let (packet, location) = decode_streaming_packet(
            &encoded,
            StreamingReceiveLimit::default(),
            StreamingOptionDecodeContext::anonymous(),
        )
        .unwrap();
        assert_eq!(packet.send_stream_id, 0);
        assert_eq!(packet.receive_stream_id, 0);
        assert_eq!(packet.sequence_num, 0);
        assert_eq!(packet.ack_through, 0);
        assert!(packet.nacks.is_empty());
        assert!(packet.payload.is_empty());
        assert!(packet.options.signature.is_none());
        assert!(location.is_none());
    }

    #[test]
    fn reserved_flag_bits_are_rejected() {
        let error = StreamingFlags::new(FLAG_RESERVED_MASK).unwrap_err();
        assert!(matches!(error, StreamingPacketError::ReservedFlagBits(_)));
    }

    #[test]
    fn nack_overflow_is_rejected_at_decode() {
        // nackCount byte declares 255 NACKs, which is above the limit. The
        // option region stays empty so the rest of the packet stays well
        // under the byte ceiling.
        let mut bytes = vec![0_u8; MIN_STREAMING_HEADER_BYTES + 4 * 16];
        bytes[16] = 255; // nackCount = 255
        let error = decode_streaming_packet(
            &bytes,
            StreamingReceiveLimit {
                max_packet_bytes: 4096,
                max_option_bytes: 256,
                max_nack_count: 16,
                max_payload_bytes: 4096,
            },
            StreamingOptionDecodeContext::anonymous(),
        )
        .unwrap_err();
        assert!(matches!(error, StreamingPacketError::NackOverflow { .. }));
    }

    #[test]
    fn option_overflow_is_rejected_at_decode() {
        // With nackCount = 0, the fixed header is exactly
        // MIN_STREAMING_HEADER_BYTES (22) bytes; optionSize sits at byte
        // 20-21. Set optionSize to a value above the configured
        // `max_option_bytes` ceiling so the decoder fails closed.
        let mut bytes = vec![0_u8; MIN_STREAMING_HEADER_BYTES + 2];
        bytes[20] = 0x00;
        bytes[21] = 0x41; // 65 > 64
        let error = decode_streaming_packet(
            &bytes,
            StreamingReceiveLimit {
                max_packet_bytes: 4096,
                max_option_bytes: 64,
                max_nack_count: 16,
                max_payload_bytes: 4096,
            },
            StreamingOptionDecodeContext::anonymous(),
        )
        .unwrap_err();
        assert!(matches!(error, StreamingPacketError::OptionOverflow { .. }));
    }

    #[test]
    fn payload_overflow_is_rejected_at_decode() {
        let bytes = vec![0_u8; MAX_STREAMING_PACKET_BYTES + 1];
        let error = decode_streaming_packet(
            &bytes,
            StreamingReceiveLimit::default(),
            StreamingOptionDecodeContext::anonymous(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            StreamingPacketError::PayloadOverflow { .. }
        ));
    }

    #[test]
    fn signature_preimage_zeroes_raw_signature_bytes() {
        let bytes = [7u8; MIN_STREAMING_HEADER_BYTES];
        let preimage = build_signature_preimage(
            &bytes,
            Some(SignatureLocation {
                offset: MIN_STREAMING_HEADER_BYTES - 5,
                length: 5,
            }),
        );
        assert_eq!(preimage.len(), bytes.len());
        assert_eq!(&preimage[..MIN_STREAMING_HEADER_BYTES - 5], &[7u8; 17]);
        for byte in &preimage[MIN_STREAMING_HEADER_BYTES - 5..MIN_STREAMING_HEADER_BYTES] {
            assert_eq!(*byte, 0);
        }
    }

    #[test]
    fn syn_replay_binding_round_trip() {
        let hash: [u8; 32] = (0..32u8).collect::<Vec<_>>().try_into().unwrap();
        let binding = encode_syn_replay_binding(&hash);
        assert_eq!(binding.len(), SYN_REPLAY_NACK_COUNT);

        let destination = ed_destination();
        let options = StreamingOptions {
            delay_requested: None,
            from_destination: Some(destination.clone()),
            max_payload_size: Some(DEFAULT_ADVERTISED_MAX_PAYLOAD),
            signature: None,
        };
        let option_bytes = options
            .encode_with_placeholder(StreamingFlags::new(INITIAL_SYN_FLAGS).unwrap(), 64)
            .unwrap();

        let builder =
            StreamingPacketBuilder::new_initial_syn(0, 1, 0, option_bytes, binding.to_vec())
                .unwrap();
        let encoded = encode_streaming_packet(&builder, StreamingSendLimit::default()).unwrap();
        let (packet, location) = decode_streaming_packet(
            &encoded,
            StreamingReceiveLimit::default(),
            StreamingOptionDecodeContext::anonymous(),
        )
        .unwrap();
        assert!(packet.flags.synchronize());
        assert!(packet.flags.no_ack());
        assert_eq!(packet.flags.bits(), INITIAL_SYN_FLAGS);
        assert!(packet.options.from_destination.is_some());
        assert_eq!(
            packet.options.max_payload_size,
            Some(DEFAULT_ADVERTISED_MAX_PAYLOAD)
        );
        assert_eq!(packet.options.signature.as_ref().map(Vec::len), Some(64));
        assert!(location.is_some());
        assert!(verify_syn_replay_binding(&packet, &hash).unwrap());
        assert!(validate_initial_syn(&packet, &hash).is_ok());
    }

    #[test]
    fn syn_response_carries_no_replay_nacks() {
        let destination = ed_destination();
        let options = StreamingOptions {
            delay_requested: None,
            from_destination: Some(destination.clone()),
            max_payload_size: Some(DEFAULT_ADVERTISED_MAX_PAYLOAD),
            signature: None,
        };
        let option_bytes = options
            .encode_with_placeholder(StreamingFlags::new(SYN_RESPONSE_FLAGS).unwrap(), 64)
            .unwrap();
        let builder = StreamingPacketBuilder::new_syn_response(5, 6, 0, 0, option_bytes).unwrap();
        let encoded = encode_streaming_packet(&builder, StreamingSendLimit::default()).unwrap();
        let (packet, _) = decode_streaming_packet(
            &encoded,
            StreamingReceiveLimit::default(),
            StreamingOptionDecodeContext::anonymous(),
        )
        .unwrap();
        assert_eq!(packet.flags.bits(), SYN_RESPONSE_FLAGS);
        assert!(!packet.flags.no_ack());
        assert!(packet.nacks.is_empty());
        assert!(validate_syn_response(&packet).is_ok());
    }

    #[test]
    fn signed_control_without_context_fails_closed() {
        // A signed packet without FROM and without peer-signing-key
        // context must be rejected before any signature misparse.
        let destination = ed_destination();
        let options = StreamingOptions {
            delay_requested: None,
            from_destination: Some(destination),
            max_payload_size: None,
            signature: None,
        };
        let option_bytes = options
            .encode_with_placeholder(StreamingFlags::new(CLOSE_FLAGS).unwrap(), 64)
            .unwrap();
        let mut builder = StreamingPacketBuilder {
            send_stream_id: 1,
            receive_stream_id: 2,
            sequence_num: 0,
            ack_through: 0,
            nacks: Vec::new(),
            resend_delay: 0,
            flags: StreamingFlags::new(CLOSE_FLAGS).unwrap(),
            option_bytes,
            payload: Vec::new(),
        };
        builder
            .option_bytes
            .truncate(builder.option_bytes.len() - 64);
        let encoded = encode_streaming_packet(&builder, StreamingSendLimit::default()).unwrap();
        // Re-add opaque signature-length bytes at the tail of the
        // option region: the decoder has no way to infer their length
        // without identity context and must fail closed.
        let mut tampered = encoded.clone();
        tampered[20..22].copy_from_slice(&64_u16.to_be_bytes());
        tampered.resize(tampered.len() + 64, 0xAA);
        let error = decode_streaming_packet(
            &tampered,
            StreamingReceiveLimit::default(),
            StreamingOptionDecodeContext::anonymous(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            StreamingPacketError::SignatureContextUnavailable
        ));

        // With peer-key context the same bytes parse cleanly.
        let key = ed_destination().signing_key().clone();
        let (packet, location) = decode_streaming_packet(
            &tampered,
            StreamingReceiveLimit::default(),
            StreamingOptionDecodeContext::with_peer_key(&key),
        )
        .unwrap();
        assert_eq!(packet.options.signature.as_ref().map(Vec::len), Some(64));
        assert_eq!(location.map(|l| l.length), Some(64));
    }

    #[test]
    fn offline_signature_is_rejected_before_misparsing() {
        let destination = ed_destination();
        let flags = StreamingFlags::new(CLOSE_FLAGS | FLAG_OFFLINE_SIGNATURE).unwrap();
        let options = StreamingOptions {
            delay_requested: None,
            from_destination: Some(destination.clone()),
            max_payload_size: None,
            signature: None,
        };
        // The encoder refuses to produce an OFFLINE_SIGNATURE packet.
        let error = options.encode_with_placeholder(flags, 64).unwrap_err();
        assert!(matches!(
            error,
            StreamingPacketError::UnsupportedOfflineSignature
        ));

        // A hand-built offline-signature packet is also rejected at
        // decode time even though the trailing fields would otherwise
        // look plausible.
        let mut option_bytes = destination
            .encode_to_vec(crate::common::MAX_COMMON_STRUCTURE_SIZE)
            .unwrap();
        option_bytes.extend_from_slice(&[0u8; 64]);
        let builder = StreamingPacketBuilder {
            send_stream_id: 1,
            receive_stream_id: 2,
            sequence_num: 0,
            ack_through: 0,
            nacks: Vec::new(),
            resend_delay: 0,
            flags,
            option_bytes,
            payload: Vec::new(),
        };
        let encoded = encode_streaming_packet(&builder, StreamingSendLimit::default()).unwrap();
        let error = decode_streaming_packet(
            &encoded,
            StreamingReceiveLimit::default(),
            StreamingOptionDecodeContext::anonymous(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            StreamingPacketError::UnsupportedOfflineSignature
        ));
    }

    #[test]
    fn install_packet_signature_fails_closed_on_nonzero_tail() {
        let mut wire = vec![0_u8; 30];
        wire[29] = 1;
        let error = install_packet_signature(&mut wire, &[2u8; 4]).unwrap_err();
        assert!(matches!(error, StreamingPacketError::SignatureInvalid));
        let mut wire = vec![0_u8; 30];
        let offset = install_packet_signature(&mut wire, &[2u8; 4]).unwrap();
        assert_eq!(offset, 26);
        assert_eq!(&wire[26..], &[2, 2, 2, 2]);
    }
}
