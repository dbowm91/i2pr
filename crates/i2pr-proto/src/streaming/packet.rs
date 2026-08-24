//! Streaming packet wire codec.
//!
//! Plan 123 owns the minimal interoperable I2P Streaming wire structure.
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
//! SYN uses `SYNCHRONIZE`, requires `FROM_INCLUDED` and `SIGNATURE_INCLUDED`,
//! and uses `nackCount = 8` with the receiver's 32-byte Destination hash in
//! the NACK field as the current replay binding. The signature preimage is
//! the full packet bytes with the signature option bytes zeroed while
//! computing/verifying the signature.
//!
//! Unknown flag bits 12-15 must be zero for current compatibility.
//! Unsupported known flags/options must receive deliberate policy rather
//! than being silently interpreted.

use core::fmt;

use crate::codec::{CodecError, DecodeCursor, encode_to_vec};
use crate::common::{Destination, SigningPublicKey};

/// Minimum packet header size: `4 + 4 + 4 + 4 + 1 + 1 + 2 + 2` = 22 bytes.
pub const MIN_STREAMING_HEADER_BYTES: usize = 22;
/// Hard ceiling on the full Streaming packet bytes including payload.
pub const MAX_STREAMING_PACKET_BYTES: usize = 1730;
/// Hard ceiling on the option region size.
///
/// The streaming packet format reserves up to 65535 bytes for the option
/// region (a `u16` width); Plan 123 keeps the upper bound below the
/// maximum packet size ceiling while accommodating the FROM destination
/// option (typically ~397 bytes for Ed25519+X25519 destinations) plus
/// the MAX_PACKET_SIZE and SIGNATURE options.
pub const MAX_STREAMING_OPTION_BYTES: usize = 1024;
/// Hard ceiling on the application payload region inside one packet.
pub const MAX_STREAMING_PAYLOAD_BYTES: usize =
    MAX_STREAMING_PACKET_BYTES - MIN_STREAMING_HEADER_BYTES;
/// Hard ceiling on the number of NACK identifiers carried in one packet.
pub const MAX_STREAMING_NACK_COUNT: usize = 64;
/// Number of NACK entries the SYN replay binding uses. Each entry is
/// `u32`, so 8 entries carry exactly 32 bytes of receiver Destination hash.
pub const SYN_REPLAY_NACK_COUNT: usize = 8;

/// Bit 0 (`0x0001`).
pub const FLAG_SYNCHRONIZE: u16 = 0x0001;
/// Bit 1 (`0x0002`).
pub const FLAG_CLOSE: u16 = 0x0002;
/// Bit 2 (`0x0004`).
pub const FLAG_RESET: u16 = 0x0004;
/// Bit 3 (`0x0008`) — `FROM_INCLUDED`.
pub const FLAG_FROM_INCLUDED: u16 = 0x0008;
/// Bit 4 (`0x0010`) — `SIGNATURE_INCLUDED`.
pub const FLAG_SIGNATURE_INCLUDED: u16 = 0x0010;
/// Bit 5 (`0x0020`) — `MAX_PACKET_SIZE_INCLUDED` (negotiated max).
pub const FLAG_MAX_PACKET_SIZE_INCLUDED: u16 = 0x0020;
/// Bit 6 (`0x0040`) — `NO_ACK`.
pub const FLAG_NO_ACK: u16 = 0x0040;
/// Bit 7 (`0x0080`) — `DELAY_REQUESTED` (informational, optional).
pub const FLAG_DELAY_REQUESTED: u16 = 0x0080;
/// Bit 8 (`0x0100`) — `INTERACTIVE_PROFILE` (optional, deferred).
pub const FLAG_PROFILE_INTERACTIVE: u16 = 0x0100;
/// Bit 9 (`0x0200`) — `ECHO` (deferred).
pub const FLAG_ECHO: u16 = 0x0200;
/// Bit 10 (`0x0400`) — `SIGNATURE_REQUESTED` (deferred).
pub const FLAG_SIGNATURE_REQUESTED: u16 = 0x0400;
/// Bit 11 (`0x0800`) — `OFFLINE_SIGNATURE` (deferred).
pub const FLAG_OFFLINE_SIGNATURE: u16 = 0x0800;
/// Reserved flag bits 12..=15. Receivers must reject packets with any of
/// these bits set.
pub const FLAG_RESERVED_MASK: u16 = 0xF000;

/// Type alias retained for backwards compatibility with the initial
/// Phase-B module surface.
pub const MAX_STREAMING_HEADER_BYTES: usize = MIN_STREAMING_HEADER_BYTES;
/// Type alias retained for backwards compatibility with the initial
/// Phase-B module surface.
pub const MAX_STEAMING_NACK_COUNT: usize = MAX_STREAMING_NACK_COUNT;

/// Flag set captured as a typed value. Builders use this to avoid silent
/// flag-bit mixups at the connection layer.
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

    /// Returns whether `FROM_INCLUDED` is set.
    pub const fn from_included(self) -> bool {
        self.0 & FLAG_FROM_INCLUDED != 0
    }

    /// Returns whether `SIGNATURE_INCLUDED` is set.
    pub const fn signature_included(self) -> bool {
        self.0 & FLAG_SIGNATURE_INCLUDED != 0
    }

    /// Returns whether `MAX_PACKET_SIZE_INCLUDED` is set.
    pub const fn max_packet_size_included(self) -> bool {
        self.0 & FLAG_MAX_PACKET_SIZE_INCLUDED != 0
    }

    /// Returns whether `NO_ACK` is set.
    pub const fn no_ack(self) -> bool {
        self.0 & FLAG_NO_ACK != 0
    }

    /// Returns whether `DELAY_REQUESTED` is set.
    pub const fn delay_requested(self) -> bool {
        self.0 & FLAG_DELAY_REQUESTED != 0
    }
}

/// Caller-supplied decoder limits. The strict decoder never allocates based
/// solely on attacker-controlled byte counts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamingReceiveLimit {
    /// Maximum allowed packet bytes including header, NACKs, options, payload.
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
    /// A signed packet has a signature option whose length disagrees with
    /// the destination's signing key type.
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
    /// The signature option is missing from a packet that requires it.
    SignatureMissing,
    /// A signed packet's preimage signature verification failed.
    SignatureInvalid,
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
                "streaming signature option length {actual} disagrees with expected {expected}"
            ),
            Self::SynReplayNackCountMismatch { observed } => write!(
                formatter,
                "SYN replay binding requires nackCount == 8, observed {observed}"
            ),
            Self::ArithmeticOverflow => {
                formatter.write_str("streaming decoder arithmetic overflow")
            }
            Self::SignatureMissing => formatter.write_str("streaming signature option missing"),
            Self::SignatureInvalid => {
                formatter.write_str("streaming signature verification failed")
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

/// Decoded Streaming packet with raw option region and payload bytes.
///
/// The signature and replay-binding fields are exposed through typed
/// helpers so connection code never has to know the wire layout.
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
    /// Raw option region bytes (signature option bytes preserved for
    /// verification; the wire bytes remain available for canonical signing).
    pub option_bytes: Vec<u8>,
    /// Optional signature extracted from the option region. `None` when
    /// `SIGNATURE_INCLUDED` is not set.
    pub signature: Option<Vec<u8>>,
    /// Application payload bytes after the option region.
    pub payload: Vec<u8>,
}

impl StreamingPacket {
    /// Returns the option-region byte slice that carries the
    /// `FROM_INCLUDED` Destination when `FROM_INCLUDED` is set.
    pub fn from_destination_bytes(&self) -> Option<&[u8]> {
        if !self.flags.from_included() {
            return None;
        }
        Some(self.option_bytes.as_slice())
    }

    /// Reconstructs the Destination carried in the option region. Returns
    /// `None` when `FROM_INCLUDED` is not set.
    pub fn decode_destination(&self) -> Result<Option<Destination>, CodecError> {
        if !self.flags.from_included() {
            return Ok(None);
        }
        let destination = Destination::decode_from_cursor(
            &self.option_bytes,
            crate::common::MAX_COMMON_STRUCTURE_SIZE,
        )?;
        Ok(Some(destination))
    }

    /// Returns the signature option byte length declared in the option
    /// region when `SIGNATURE_INCLUDED` is set.
    pub fn signature_option_length(&self) -> Option<usize> {
        self.signature.as_ref().map(Vec::len)
    }
}

/// Decodes a streaming packet from the wire form, also returning the
/// absolute offset and length of the signature option region (when present)
/// so the caller can verify the signature over the canonical bytes with
/// the signature option zeroed.
pub fn decode_streaming_packet(
    input: &[u8],
    limit: StreamingReceiveLimit,
) -> Result<(StreamingPacket, Option<SignatureOptionLocation>), StreamingPacketError> {
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
    let payload_end = input.len();
    let payload = if option_end < payload_end {
        input[option_end..payload_end].to_vec()
    } else {
        Vec::new()
    };
    if payload.len() > limit.max_payload_bytes {
        return Err(StreamingPacketError::PayloadOverflow {
            declared: payload.len(),
            maximum: limit.max_payload_bytes,
        });
    }

    let mut signature = None;
    let mut signature_location = None;
    if flags.signature_included() {
        let (offset, length, sig) = extract_signature_option(&option_bytes)?;
        signature = Some(sig);
        signature_location = Some(SignatureOptionLocation {
            offset: option_start + offset,
            length,
        });
    }

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
            signature,
            payload,
        },
        signature_location,
    ))
}

/// Identifies where in the wire buffer the signature option bytes live so
/// the verification layer can zero them while computing the canonical
/// signature preimage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignatureOptionLocation {
    /// Offset of the signature option bytes (covering type + length
    /// prefix plus signature bytes; the full byte region is zeroed).
    pub offset: usize,
    /// Length of the signature option bytes covered by the zero-fill.
    pub length: usize,
}

fn extract_signature_option(
    option_bytes: &[u8],
) -> Result<(usize, usize, Vec<u8>), StreamingPacketError> {
    // The SIGNATURE option is always the LAST option in the option
    // region. The option region is laid out as:
    //
    //   [FROM destination (self-encoded, no type/length prefix)]
    //   [MAX_PACKET_SIZE option (type=1, length=4, u32)]
    //   [SIGNATURE option (type=3, length=u8, sig_bytes)]
    //
    // For a 64-byte Ed25519 signature the SIGNATURE option occupies
    // exactly 66 bytes (`type + length + 64 sig bytes`). We locate the
    // SIGNATURE option by reading its length byte at offset
    // `total_len - 65` and verifying the type byte at `total_len - 66`.
    if option_bytes.len() < 66 {
        return Err(StreamingPacketError::SignatureMissing);
    }
    let total_len = option_bytes.len();
    let length = option_bytes[total_len - 65] as usize;
    if length != 64 {
        return Err(StreamingPacketError::SignatureMissing);
    }
    let opt_type = option_bytes[total_len - 66];
    if opt_type != STREAMING_OPTION_SIGNATURE {
        return Err(StreamingPacketError::SignatureMissing);
    }
    let sig_offset = total_len - 66;
    let signature = option_bytes[sig_offset + 2..total_len].to_vec();
    let total = 66_usize;
    Ok((sig_offset, total, signature))
}

/// Builds a Streaming signature preimage: the supplied wire bytes with
/// the supplied signature option region zeroed.
pub fn build_signature_preimage(
    wire_bytes: &[u8],
    signature_option: Option<SignatureOptionLocation>,
) -> Vec<u8> {
    let mut preimage = wire_bytes.to_vec();
    if let Some(location) = signature_option {
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
    /// Constructs a SYN packet builder with the canonical
    /// `SYNCHRONIZE | FROM_INCLUDED | SIGNATURE_INCLUDED |
    /// MAX_PACKET_SIZE_INCLUDED` flag set. The caller is responsible for
    /// supplying the option bytes (including the FROM Destination, the
    /// MAX_PACKET_SIZE option, and the SIGNATURE option) and the NACK
    /// replay binding.
    pub fn new_syn(
        send_stream_id: u32,
        sequence_num: u32,
        option_bytes: Vec<u8>,
        nacks: Vec<u32>,
    ) -> Result<Self, StreamingPacketError> {
        let flags = StreamingFlags::new(
            FLAG_SYNCHRONIZE
                | FLAG_FROM_INCLUDED
                | FLAG_SIGNATURE_INCLUDED
                | FLAG_MAX_PACKET_SIZE_INCLUDED,
        )?;
        Ok(Self {
            send_stream_id,
            receive_stream_id: 0,
            sequence_num,
            ack_through: 0,
            nacks,
            resend_delay: 0,
            flags,
            option_bytes,
            payload: Vec::new(),
        })
    }

    /// Constructs a SYN response packet builder.
    pub fn new_syn_response(
        send_stream_id: u32,
        receive_stream_id: u32,
        sequence_num: u32,
        option_bytes: Vec<u8>,
        nacks: Vec<u32>,
    ) -> Result<Self, StreamingPacketError> {
        let flags = StreamingFlags::new(
            FLAG_SYNCHRONIZE
                | FLAG_FROM_INCLUDED
                | FLAG_SIGNATURE_INCLUDED
                | FLAG_MAX_PACKET_SIZE_INCLUDED
                | FLAG_NO_ACK,
        )?;
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
}

/// Encodes a streaming packet from the typed builder input and returns
/// the wire bytes. Callers that need to sign the packet must compute the
/// canonical preimage via [`build_signature_preimage`] over the returned
/// bytes with the signature option zeroed.
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

/// Verifies a SYN replay binding against the supplied receiver Destination
/// hash. Returns `Ok(true)` when the binding matches, `Ok(false)` when the
/// binding does not match the supplied hash, and an error otherwise.
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

/// Encodes the canonical 32-byte SYN replay binding value (the receiver
/// Destination hash) as the eight `u32` NACK entries the SYN packet
/// carries.
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
    out.to_vec().try_into().expect("constant size")
}

/// Option type code for the SIGNATURE option.
pub const STREAMING_OPTION_SIGNATURE: u8 = 3;
/// Option type code for the FROM option (carries the full Destination).
pub const STREAMING_OPTION_FROM: u8 = 4;
/// Option type code for the MAX_PACKET_SIZE option (4-byte value).
pub const STREAMING_OPTION_MAX_PACKET_SIZE: u8 = 1;

/// Validates a `SYNCHRONIZE` packet against the minimal-core policy
/// described in Plan 123 §5.
pub fn validate_syn_policy(
    packet: &StreamingPacket,
    receiver_destination_hash: &[u8; 32],
    expected_signature_length: usize,
) -> Result<(), StreamingPacketError> {
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
    let length = packet
        .signature
        .as_ref()
        .map(Vec::len)
        .ok_or(StreamingPacketError::SignatureMissing)?;
    if length != expected_signature_length {
        return Err(StreamingPacketError::SignatureLengthMismatch {
            expected: expected_signature_length,
            actual: length,
        });
    }
    if !verify_syn_replay_binding(packet, receiver_destination_hash)? {
        return Err(StreamingPacketError::SynMissingReplayBinding);
    }
    Ok(())
}

/// Validates a signed packet has a signature option whose length matches
/// the supplied destination's signing key type.
pub fn validate_signature_policy(
    packet: &StreamingPacket,
    destination: &SigningPublicKey,
) -> Result<(), StreamingPacketError> {
    let expected = destination.key_type().signature_len().ok_or(
        StreamingPacketError::SignatureLengthMismatch {
            expected: 0,
            actual: 0,
        },
    )?;
    let actual = packet
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
        let (packet, location) =
            decode_streaming_packet(&encoded, StreamingReceiveLimit::default()).unwrap();
        assert_eq!(packet.send_stream_id, 0);
        assert_eq!(packet.receive_stream_id, 0);
        assert_eq!(packet.sequence_num, 0);
        assert_eq!(packet.ack_through, 0);
        assert!(packet.nacks.is_empty());
        assert!(packet.payload.is_empty());
        assert!(packet.signature.is_none());
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
        )
        .unwrap_err();
        assert!(matches!(error, StreamingPacketError::OptionOverflow { .. }));
    }

    #[test]
    fn payload_overflow_is_rejected_at_decode() {
        let bytes = vec![0_u8; MAX_STREAMING_PACKET_BYTES + 1];
        let error = decode_streaming_packet(&bytes, StreamingReceiveLimit::default()).unwrap_err();
        assert!(matches!(
            error,
            StreamingPacketError::PayloadOverflow { .. }
        ));
    }

    #[test]
    fn signature_preimage_zeroes_signature_option_bytes() {
        let bytes = [0u8; MIN_STREAMING_HEADER_BYTES];
        let preimage = build_signature_preimage(
            &bytes,
            Some(SignatureOptionLocation {
                offset: MIN_STREAMING_HEADER_BYTES - 5,
                length: 5,
            }),
        );
        assert_eq!(preimage.len(), bytes.len());
        for byte in &preimage[MIN_STREAMING_HEADER_BYTES - 5..MIN_STREAMING_HEADER_BYTES] {
            assert_eq!(*byte, 0);
        }
    }

    #[test]
    fn syn_replay_binding_round_trip() {
        let hash: [u8; 32] = (0..32u8).collect::<Vec<_>>().try_into().unwrap();
        let binding = encode_syn_replay_binding(&hash);
        assert_eq!(binding.len(), SYN_REPLAY_NACK_COUNT);

        // Build a SYN packet with a stub signature option so the decoder
        // can locate the signature bytes (the policy validators run later
        // and reject mismatched signature lengths for an actual Ed25519
        // destination).
        let signature = vec![0u8; 64];
        let mut option_bytes = Vec::new();
        option_bytes.push(STREAMING_OPTION_SIGNATURE);
        option_bytes.push(signature.len() as u8);
        option_bytes.extend_from_slice(&signature);

        let builder =
            StreamingPacketBuilder::new_syn(0, 1, option_bytes, binding.to_vec()).unwrap();
        let encoded = encode_streaming_packet(&builder, StreamingSendLimit::default()).unwrap();
        let (packet, location) =
            decode_streaming_packet(&encoded, StreamingReceiveLimit::default()).unwrap();
        assert!(packet.flags.synchronize());
        assert!(packet.flags.from_included());
        assert!(packet.flags.signature_included());
        assert!(packet.flags.max_packet_size_included());
        assert!(location.is_some());
        assert!(verify_syn_replay_binding(&packet, &hash).unwrap());
    }
}
