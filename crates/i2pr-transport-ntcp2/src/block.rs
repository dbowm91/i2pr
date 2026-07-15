//! Authenticated NTCP2 payload blocks.
//!
//! Blocks are parsed only after a frame has passed AEAD authentication.  The
//! parser is still deliberately strict: every block is bounded by the
//! authenticated plaintext, unknown blocks have an aggregate budget, and no
//! block decoder may read into the following block.

use std::fmt;

use i2pr_crypto::{router_identity_hash, verify_router_info};
use i2pr_proto::{Hash, MessageType, RouterInfo};
use i2pr_transport::EncodedI2npMessage;
use thiserror::Error;

use crate::constants;
use crate::crypto::PublicKeyBytes;

/// The encoded size of a block type and its big-endian length.
pub const BLOCK_HEADER_LENGTH: usize = 3;
/// Date/time block type.
pub const BLOCK_DATETIME: u8 = 0;
/// Options block type.
pub const BLOCK_OPTIONS: u8 = 1;
/// RouterInfo block type.
pub const BLOCK_ROUTER_INFO: u8 = 2;
/// I2NP message block type.
pub const BLOCK_I2NP: u8 = 3;
/// Termination block type.
pub const BLOCK_TERMINATION: u8 = 4;
/// Reserved experimental block types are 224 through 253.
pub const BLOCK_EXPERIMENTAL_MIN: u8 = 224;
/// Reserved experimental block types end at 253.
pub const BLOCK_EXPERIMENTAL_MAX: u8 = 253;
/// Padding block type.
pub const BLOCK_PADDING: u8 = 254;
/// Reserved future-extension block type.
pub const BLOCK_FUTURE: u8 = 255;

/// The minimum options payload: four padding ratios and four u16 controls.
pub const OPTIONS_MIN_LENGTH: usize = 12;
/// Maximum options payload retained by the data phase.
pub const MAX_OPTIONS_BYTES: usize = 4 * 1024;
/// Maximum RouterInfo payload retained by one authenticated block.
pub const MAX_ROUTER_INFO_BYTES: usize = constants::MAX_FRAME_PLAINTEXT - BLOCK_HEADER_LENGTH - 1;
/// Maximum padding bytes retained by one block and one data-phase plaintext.
pub const MAX_PADDING_BYTES: usize = constants::MAX_FRAME_PLAINTEXT - BLOCK_HEADER_LENGTH;
/// Maximum additional termination bytes accepted and then discarded.
pub const MAX_TERMINATION_ADDITIONAL_BYTES: usize = 256;
/// Maximum authenticated blocks in one frame.
pub const MAX_BLOCK_COUNT: usize = 256;
/// Maximum aggregate bytes skipped for unknown blocks in one frame.
pub const MAX_UNKNOWN_BLOCK_BYTES: usize = 4 * 1024;

/// Typed errors from canonical block encoding and authenticated parsing.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum BlockError {
    /// A complete block header or body was not present.
    #[error("truncated NTCP2 block")]
    Truncated,
    /// A block length exceeded the authenticated plaintext boundary.
    #[error("NTCP2 block length exceeds the authenticated frame")]
    LengthExceedsFrame,
    /// The total block count exceeded the local bounded parser policy.
    #[error("NTCP2 frame contains too many blocks")]
    ExcessiveBlockCount,
    /// Unknown block bytes exceeded the aggregate skip budget.
    #[error("NTCP2 unknown block budget exceeded")]
    ExcessiveUnknownBytes,
    /// A known block had an invalid fixed or variable length.
    #[error("invalid NTCP2 block length")]
    InvalidLength,
    /// A required/control block appeared more than once.
    #[error("duplicate NTCP2 control block")]
    DuplicateBlock,
    /// A block violated the terminal or padding ordering rule.
    #[error("invalid NTCP2 block ordering")]
    InvalidOrder,
    /// A termination block contained an invalid reason or shape.
    #[error("malformed NTCP2 termination block")]
    InvalidTermination,
    /// A RouterInfo block used reserved flag bits or malformed bytes.
    #[error("malformed NTCP2 RouterInfo block")]
    RouterInfoMalformed,
    /// RouterInfo signature verification failed.
    #[error("NTCP2 RouterInfo signature invalid")]
    RouterInfoSignatureInvalid,
    /// The RouterInfo hash did not match the authenticated peer.
    #[error("NTCP2 RouterInfo peer identity mismatch")]
    PeerIdentityMismatch,
    /// The RouterInfo transport static key did not match the authenticated peer.
    #[error("NTCP2 RouterInfo static-key mismatch")]
    PeerStaticKeyMismatch,
    /// An I2NP block did not contain a bounded complete short message.
    #[error("malformed NTCP2 I2NP message block")]
    I2npMalformed,
    /// A bounded options block was malformed.
    #[error("malformed NTCP2 options block")]
    OptionsMalformed,
    /// A caller supplied a zero or oversized payload.
    #[error("NTCP2 block payload exceeds its bound")]
    PayloadTooLarge,
}

/// A data-phase timestamp in rounded Unix seconds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimestampBlock {
    seconds: u32,
}

impl TimestampBlock {
    /// Creates a timestamp block.
    pub const fn new(seconds: u32) -> Self {
        Self { seconds }
    }

    /// Returns the wire timestamp.
    pub const fn seconds(self) -> u32 {
        self.seconds
    }
}

/// Bounded data-phase options. Unknown trailing option bytes are retained as
/// opaque bounded extensions because the deployed format leaves them open.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionsBlock {
    transmit_min_padding: u8,
    transmit_max_padding: u8,
    receive_min_padding: u8,
    receive_max_padding: u8,
    transmit_dummy_rate: u16,
    receive_dummy_rate: u16,
    transmit_delay_ms: u16,
    receive_delay_ms: u16,
    extensions: Vec<u8>,
}

impl OptionsBlock {
    /// Creates options from the fixed deployed fields and bounded extensions.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        transmit_min_padding: u8,
        transmit_max_padding: u8,
        receive_min_padding: u8,
        receive_max_padding: u8,
        transmit_dummy_rate: u16,
        receive_dummy_rate: u16,
        transmit_delay_ms: u16,
        receive_delay_ms: u16,
        extensions: Vec<u8>,
    ) -> Result<Self, BlockError> {
        let total = OPTIONS_MIN_LENGTH
            .checked_add(extensions.len())
            .ok_or(BlockError::PayloadTooLarge)?;
        if total > MAX_OPTIONS_BYTES {
            return Err(BlockError::PayloadTooLarge);
        }
        if transmit_min_padding > transmit_max_padding || receive_min_padding > receive_max_padding
        {
            return Err(BlockError::OptionsMalformed);
        }
        Ok(Self {
            transmit_min_padding,
            transmit_max_padding,
            receive_min_padding,
            receive_max_padding,
            transmit_dummy_rate,
            receive_dummy_rate,
            transmit_delay_ms,
            receive_delay_ms,
            extensions,
        })
    }

    /// Decodes one complete options payload.
    pub fn decode(payload: &[u8]) -> Result<Self, BlockError> {
        if !(OPTIONS_MIN_LENGTH..=MAX_OPTIONS_BYTES).contains(&payload.len()) {
            return Err(BlockError::InvalidLength);
        }
        Self::new(
            payload[0],
            payload[1],
            payload[2],
            payload[3],
            u16::from_be_bytes([payload[4], payload[5]]),
            u16::from_be_bytes([payload[6], payload[7]]),
            u16::from_be_bytes([payload[8], payload[9]]),
            u16::from_be_bytes([payload[10], payload[11]]),
            payload[OPTIONS_MIN_LENGTH..].to_vec(),
        )
    }

    /// Returns the fixed transmit padding range.
    pub fn transmit_padding(&self) -> (u8, u8) {
        (self.transmit_min_padding, self.transmit_max_padding)
    }

    /// Returns the fixed receive padding range.
    pub fn receive_padding(&self) -> (u8, u8) {
        (self.receive_min_padding, self.receive_max_padding)
    }

    /// Returns the fixed dummy-traffic fields.
    pub fn dummy_rates(&self) -> (u16, u16) {
        (self.transmit_dummy_rate, self.receive_dummy_rate)
    }

    /// Returns the fixed delay fields in milliseconds.
    pub fn delays(&self) -> (u16, u16) {
        (self.transmit_delay_ms, self.receive_delay_ms)
    }

    /// Returns bounded opaque extension bytes.
    pub fn extensions(&self) -> &[u8] {
        &self.extensions
    }

    fn encode_payload(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(OPTIONS_MIN_LENGTH + self.extensions.len());
        output.extend_from_slice(&[
            self.transmit_min_padding,
            self.transmit_max_padding,
            self.receive_min_padding,
            self.receive_max_padding,
        ]);
        output.extend_from_slice(&self.transmit_dummy_rate.to_be_bytes());
        output.extend_from_slice(&self.receive_dummy_rate.to_be_bytes());
        output.extend_from_slice(&self.transmit_delay_ms.to_be_bytes());
        output.extend_from_slice(&self.receive_delay_ms.to_be_bytes());
        output.extend_from_slice(&self.extensions);
        output
    }
}

/// A RouterInfo candidate from an authenticated data-phase block.
pub struct RouterInfoBlock {
    flags: u8,
    encoded: Vec<u8>,
    info: RouterInfo,
    router_hash: Hash,
}

impl RouterInfoBlock {
    /// Creates and verifies a RouterInfo block. NetDB policy is not applied.
    pub fn new(flags: u8, encoded: Vec<u8>) -> Result<Self, BlockError> {
        if flags & !1 != 0 || encoded.is_empty() || encoded.len() > MAX_ROUTER_INFO_BYTES {
            return Err(BlockError::RouterInfoMalformed);
        }
        let info = RouterInfo::decode(&encoded, MAX_ROUTER_INFO_BYTES)
            .map_err(|_| BlockError::RouterInfoMalformed)?;
        verify_router_info(&info).map_err(|_| BlockError::RouterInfoSignatureInvalid)?;
        let router_hash = router_identity_hash(info.router_identity())
            .map_err(|_| BlockError::RouterInfoMalformed)?;
        Ok(Self {
            flags,
            encoded,
            info,
            router_hash,
        })
    }

    /// Returns the bounded flags; bit 0 requests flooding.
    pub const fn flags(&self) -> u8 {
        self.flags
    }

    /// Returns whether the peer requested a flood operation.
    pub const fn flood_requested(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Returns the canonical RouterInfo hash.
    pub const fn router_hash(&self) -> Hash {
        self.router_hash
    }

    /// Borrows the validated RouterInfo.
    pub const fn info(&self) -> &RouterInfo {
        &self.info
    }

    /// Borrows the exact encoded RouterInfo bytes.
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }

    /// Checks that this candidate remains bound to the authenticated peer.
    pub fn validate_peer(
        &self,
        expected_hash: Hash,
        expected_static_key: PublicKeyBytes,
    ) -> Result<(), BlockError> {
        crate::handshake::validate_router_info(
            &self.encoded,
            MAX_ROUTER_INFO_BYTES,
            Some(expected_hash),
            expected_static_key,
        )
        .map(|_| ())
        .map_err(|error| match error {
            crate::handshake::HandshakeError::PeerIdentityMismatch => {
                BlockError::PeerIdentityMismatch
            }
            crate::handshake::HandshakeError::TransportStaticKeyMismatch => {
                BlockError::PeerStaticKeyMismatch
            }
            crate::handshake::HandshakeError::RouterInfoSignatureInvalid => {
                BlockError::RouterInfoSignatureInvalid
            }
            _ => BlockError::RouterInfoMalformed,
        })
    }
}

impl fmt::Debug for RouterInfoBlock {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RouterInfoBlock")
            .field("flags", &self.flags)
            .field("length", &self.encoded.len())
            .field("router_hash", &self.router_hash)
            .finish()
    }
}

/// A bounded encoded I2NP message carried in one complete block.
pub struct I2npMessageBlock {
    message: EncodedI2npMessage,
}

impl I2npMessageBlock {
    /// Takes the transport-owned encoded message without cloning it.
    pub fn new(message: EncodedI2npMessage) -> Result<Self, BlockError> {
        if message.len() < i2pr_proto::SHORT_TRANSPORT_HEADER_SIZE {
            return Err(BlockError::I2npMalformed);
        }
        Ok(Self { message })
    }

    /// Takes an already encoded NTCP2 short-header message.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, BlockError> {
        Self::new(EncodedI2npMessage::new(bytes).map_err(|_| BlockError::I2npMalformed)?)
    }

    /// Borrows the complete encoded message.
    pub fn as_bytes(&self) -> &[u8] {
        self.message.as_bytes()
    }

    /// Consumes the block and returns the transport owner.
    pub fn into_message(self) -> EncodedI2npMessage {
        self.message
    }
}

impl fmt::Debug for I2npMessageBlock {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("I2npMessageBlock")
            .field("length", &self.message.len())
            .finish()
    }
}

/// A bounded padding block. Padding bytes are never included in diagnostics.
pub struct PaddingBlock {
    bytes: Vec<u8>,
}

impl PaddingBlock {
    /// Takes deterministic or runtime-generated padding bytes.
    pub fn new(bytes: Vec<u8>) -> Result<Self, BlockError> {
        if bytes.len() > MAX_PADDING_BYTES {
            return Err(BlockError::PayloadTooLarge);
        }
        Ok(Self { bytes })
    }

    /// Returns the padding length without exposing the bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns whether the padding payload is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl fmt::Debug for PaddingBlock {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PaddingBlock")
            .field("length", &self.bytes.len())
            .finish()
    }
}

/// Bounded termination reason codes from the NTCP2 dossier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminationReason {
    /// Normal or unspecified close.
    Normal,
    /// A termination block was received.
    Received,
    /// Idle timeout.
    IdleTimeout,
    /// Local router shutdown.
    RouterShutdown,
    /// Data-phase AEAD failure.
    AeadFailure,
    /// Incompatible options.
    IncompatibleOptions,
    /// Incompatible signature type.
    IncompatibleSignatureType,
    /// Clock skew.
    ClockSkew,
    /// Padding violation.
    PaddingViolation,
    /// AEAD framing error.
    AeadFramingError,
    /// Payload format error.
    PayloadFormatError,
    /// Handshake message 1 error.
    Message1Error,
    /// Handshake message 2 error.
    Message2Error,
    /// Handshake message 3 error.
    Message3Error,
    /// Intra-frame read timeout.
    IntraFrameReadTimeout,
    /// RouterInfo signature verification failure.
    RouterInfoSignatureFailure,
    /// RouterInfo static-key binding failure.
    RouterInfoStaticKeyFailure,
    /// Peer was banned.
    Banned,
    /// A bounded future reason code.
    Unknown(u8),
}

impl TerminationReason {
    /// Converts the reason to its wire code.
    pub const fn code(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Received => 1,
            Self::IdleTimeout => 2,
            Self::RouterShutdown => 3,
            Self::AeadFailure => 4,
            Self::IncompatibleOptions => 5,
            Self::IncompatibleSignatureType => 6,
            Self::ClockSkew => 7,
            Self::PaddingViolation => 8,
            Self::AeadFramingError => 9,
            Self::PayloadFormatError => 10,
            Self::Message1Error => 11,
            Self::Message2Error => 12,
            Self::Message3Error => 13,
            Self::IntraFrameReadTimeout => 14,
            Self::RouterInfoSignatureFailure => 15,
            Self::RouterInfoStaticKeyFailure => 16,
            Self::Banned => 17,
            Self::Unknown(code) => code,
        }
    }

    /// Converts a wire reason without retaining remote text.
    pub const fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Normal,
            1 => Self::Received,
            2 => Self::IdleTimeout,
            3 => Self::RouterShutdown,
            4 => Self::AeadFailure,
            5 => Self::IncompatibleOptions,
            6 => Self::IncompatibleSignatureType,
            7 => Self::ClockSkew,
            8 => Self::PaddingViolation,
            9 => Self::AeadFramingError,
            10 => Self::PayloadFormatError,
            11 => Self::Message1Error,
            12 => Self::Message2Error,
            13 => Self::Message3Error,
            14 => Self::IntraFrameReadTimeout,
            15 => Self::RouterInfoSignatureFailure,
            16 => Self::RouterInfoStaticKeyFailure,
            17 => Self::Banned,
            other => Self::Unknown(other),
        }
    }
}

/// A termination control block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminationBlock {
    valid_frames_received: u64,
    reason: TerminationReason,
    additional_length: usize,
}

impl TerminationBlock {
    /// Creates a termination block without free-form remote text.
    pub const fn new(valid_frames_received: u64, reason: TerminationReason) -> Self {
        Self {
            valid_frames_received,
            reason,
            additional_length: 0,
        }
    }

    /// Returns the count of authenticated frames received by the sender.
    pub const fn valid_frames_received(self) -> u64 {
        self.valid_frames_received
    }

    /// Returns the bounded typed reason.
    pub const fn reason(self) -> TerminationReason {
        self.reason
    }

    /// Returns the discarded additional-data length.
    pub const fn additional_length(self) -> usize {
        self.additional_length
    }
}

/// One canonical outbound block. It is intentionally not `Clone` because an
/// I2NP payload and padding have one clear owner until frame assembly.
#[allow(clippy::large_enum_variant)]
pub enum Block {
    /// A rounded Unix timestamp.
    Timestamp(TimestampBlock),
    /// Bounded options.
    Options(OptionsBlock),
    /// A validated RouterInfo candidate.
    RouterInfo(RouterInfoBlock),
    /// A consuming I2NP message handoff.
    I2np(I2npMessageBlock),
    /// An explicit termination control block.
    Termination(TerminationBlock),
    /// Authenticated random padding.
    Padding(PaddingBlock),
}

impl fmt::Debug for Block {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timestamp(value) => formatter.debug_tuple("Timestamp").field(value).finish(),
            Self::Options(value) => formatter.debug_tuple("Options").field(value).finish(),
            Self::RouterInfo(value) => formatter.debug_tuple("RouterInfo").field(value).finish(),
            Self::I2np(value) => formatter.debug_tuple("I2np").field(value).finish(),
            Self::Termination(value) => formatter.debug_tuple("Termination").field(value).finish(),
            Self::Padding(value) => formatter.debug_tuple("Padding").field(value).finish(),
        }
    }
}

impl Block {
    /// Returns the block type code.
    pub const fn kind(&self) -> u8 {
        match self {
            Self::Timestamp(_) => BLOCK_DATETIME,
            Self::Options(_) => BLOCK_OPTIONS,
            Self::RouterInfo(_) => BLOCK_ROUTER_INFO,
            Self::I2np(_) => BLOCK_I2NP,
            Self::Termination(_) => BLOCK_TERMINATION,
            Self::Padding(_) => BLOCK_PADDING,
        }
    }

    /// Returns the complete encoded block size.
    pub fn encoded_len(&self) -> usize {
        BLOCK_HEADER_LENGTH
            + match self {
                Self::Timestamp(_) => 4,
                Self::Options(value) => OPTIONS_MIN_LENGTH + value.extensions.len(),
                Self::RouterInfo(value) => 1 + value.encoded.len(),
                Self::I2np(value) => value.message.len(),
                Self::Termination(value) => 9 + value.additional_length,
                Self::Padding(value) => value.bytes.len(),
            }
    }

    fn encode_into(self, output: &mut Vec<u8>) -> Result<(), BlockError> {
        let kind = self.kind();
        let payload_len = self.encoded_len() - BLOCK_HEADER_LENGTH;
        let length = u16::try_from(payload_len).map_err(|_| BlockError::PayloadTooLarge)?;
        output.push(kind);
        output.extend_from_slice(&length.to_be_bytes());
        match self {
            Self::Timestamp(value) => output.extend_from_slice(&value.seconds.to_be_bytes()),
            Self::Options(value) => output.extend_from_slice(&value.encode_payload()),
            Self::RouterInfo(value) => {
                output.push(value.flags);
                output.extend_from_slice(&value.encoded);
            }
            Self::I2np(value) => output.extend_from_slice(value.message.as_bytes()),
            Self::Termination(value) => {
                output.extend_from_slice(&value.valid_frames_received.to_be_bytes());
                output.push(value.reason.code());
            }
            Self::Padding(value) => output.extend_from_slice(&value.bytes),
        }
        Ok(())
    }
}

/// Encodes a bounded sequence of blocks as authenticated plaintext.
pub fn encode_blocks(blocks: Vec<Block>) -> Result<Vec<u8>, BlockError> {
    if blocks.len() > MAX_BLOCK_COUNT {
        return Err(BlockError::ExcessiveBlockCount);
    }
    let mut output = Vec::new();
    let mut padding_seen = false;
    let mut termination_seen = false;
    for block in blocks {
        if block.kind() == BLOCK_PADDING {
            if padding_seen || output.len() + block.encoded_len() > constants::MAX_FRAME_PLAINTEXT {
                return Err(if padding_seen {
                    BlockError::DuplicateBlock
                } else {
                    BlockError::PayloadTooLarge
                });
            }
            padding_seen = true;
        } else if padding_seen {
            return Err(BlockError::InvalidOrder);
        }
        if block.kind() == BLOCK_TERMINATION {
            if termination_seen {
                return Err(BlockError::DuplicateBlock);
            }
            termination_seen = true;
        } else if termination_seen && block.kind() != BLOCK_PADDING {
            return Err(BlockError::InvalidOrder);
        }
        if output.len() + block.encoded_len() > constants::MAX_FRAME_PLAINTEXT {
            return Err(BlockError::PayloadTooLarge);
        }
        block.encode_into(&mut output)?;
    }
    Ok(output)
}

/// A borrowed authenticated I2NP block. The frame owner remains authoritative
/// until the caller explicitly transfers or copies this bounded message.
pub struct ReceivedI2npBlock<'a> {
    bytes: &'a [u8],
}

impl<'a> ReceivedI2npBlock<'a> {
    /// Borrows the complete encoded short-header message.
    pub fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Returns the message type without decoding the body.
    pub fn message_type(&self) -> MessageType {
        MessageType::from_code(self.bytes[0])
    }

    /// Returns the encoded message identifier.
    pub fn message_id(&self) -> u32 {
        u32::from_be_bytes(self.bytes[1..5].try_into().expect("I2NP header checked"))
    }

    /// Returns the short expiration value.
    pub fn expiration_seconds(&self) -> u32 {
        u32::from_be_bytes(self.bytes[5..9].try_into().expect("I2NP header checked"))
    }

    /// Creates the transport owner at the explicit receiver handoff.
    pub fn into_owned(self) -> Result<EncodedI2npMessage, BlockError> {
        EncodedI2npMessage::new(self.bytes.to_vec()).map_err(|_| BlockError::I2npMalformed)
    }
}

impl fmt::Debug for ReceivedI2npBlock<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReceivedI2npBlock")
            .field("length", &self.bytes.len())
            .field("message_type", &self.message_type())
            .finish()
    }
}

/// Borrowed/owned semantic output from one authenticated plaintext parse.
#[allow(clippy::large_enum_variant)]
pub enum DecodedBlock<'a> {
    /// Date/time in Unix seconds.
    Timestamp(TimestampBlock),
    /// Bounded options.
    Options(OptionsBlock),
    /// Verified RouterInfo candidate.
    RouterInfo(RouterInfoBlock),
    /// Complete bounded I2NP short message.
    I2np(ReceivedI2npBlock<'a>),
    /// Authenticated termination metadata; additional data is discarded.
    Termination(TerminationBlock),
    /// Padding length only.
    Padding {
        /// Number of authenticated padding bytes.
        length: usize,
    },
    /// Unknown blocks are authenticated and skipped as bounded padding.
    Unknown {
        /// Unknown wire type code.
        block_type: u8,
        /// Number of authenticated bytes skipped.
        length: usize,
    },
}

impl fmt::Debug for DecodedBlock<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timestamp(value) => formatter.debug_tuple("Timestamp").field(value).finish(),
            Self::Options(value) => formatter.debug_tuple("Options").field(value).finish(),
            Self::RouterInfo(value) => formatter.debug_tuple("RouterInfo").field(value).finish(),
            Self::I2np(value) => formatter.debug_tuple("I2np").field(value).finish(),
            Self::Termination(value) => formatter.debug_tuple("Termination").field(value).finish(),
            Self::Padding { length } => formatter
                .debug_struct("Padding")
                .field("length", length)
                .finish(),
            Self::Unknown { block_type, length } => formatter
                .debug_struct("Unknown")
                .field("block_type", block_type)
                .field("length", length)
                .finish(),
        }
    }
}

/// Parsed authenticated block sequence with bounded aggregate accounting.
pub struct ParsedBlocks<'a> {
    blocks: Vec<DecodedBlock<'a>>,
    unknown_bytes: usize,
}

impl fmt::Debug for ParsedBlocks<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ParsedBlocks")
            .field("block_count", &self.blocks.len())
            .field("unknown_bytes", &self.unknown_bytes)
            .finish()
    }
}

impl<'a> ParsedBlocks<'a> {
    /// Borrows the parsed semantic blocks.
    pub fn blocks(&self) -> &[DecodedBlock<'a>] {
        &self.blocks
    }

    /// Consumes the parse and returns its bounded block vector.
    pub fn into_blocks(self) -> Vec<DecodedBlock<'a>> {
        self.blocks
    }

    /// Returns the total bytes skipped for unknown blocks.
    pub const fn unknown_bytes(&self) -> usize {
        self.unknown_bytes
    }
}

/// Parses one general data-phase authenticated plaintext block sequence.
///
/// This parser intentionally has different sequencing rules from the strict
/// [`crate::handshake::ConfirmedPayload`] parser: specification-permitted
/// non-padding blocks may repeat, and Termination may follow earlier valid
/// blocks. Padding remains at most once and final; Termination remains the
/// final non-padding block.
pub fn parse_blocks(input: &[u8]) -> Result<ParsedBlocks<'_>, BlockError> {
    if input.len() > constants::MAX_FRAME_PLAINTEXT {
        return Err(BlockError::PayloadTooLarge);
    }
    let mut offset = 0;
    let mut blocks = Vec::new();
    let mut unknown_bytes: usize = 0;
    let mut padding_seen = false;
    let mut termination_seen = false;
    while offset < input.len() {
        if blocks.len() == MAX_BLOCK_COUNT {
            return Err(BlockError::ExcessiveBlockCount);
        }
        let header_end = offset
            .checked_add(BLOCK_HEADER_LENGTH)
            .ok_or(BlockError::LengthExceedsFrame)?;
        if header_end > input.len() {
            return Err(BlockError::Truncated);
        }
        let block_type = input[offset];
        let length = usize::from(u16::from_be_bytes([input[offset + 1], input[offset + 2]]));
        let body_start = header_end;
        let body_end = body_start
            .checked_add(length)
            .ok_or(BlockError::LengthExceedsFrame)?;
        if body_end > input.len() {
            return Err(BlockError::Truncated);
        }
        let body = &input[body_start..body_end];
        if block_type == BLOCK_PADDING {
            if padding_seen {
                return Err(BlockError::DuplicateBlock);
            }
            padding_seen = true;
        } else if padding_seen {
            return Err(BlockError::InvalidOrder);
        }
        if block_type == BLOCK_TERMINATION {
            if termination_seen {
                return Err(BlockError::DuplicateBlock);
            }
            termination_seen = true;
        } else if termination_seen && block_type != BLOCK_PADDING {
            return Err(BlockError::InvalidOrder);
        }
        let decoded = match block_type {
            BLOCK_DATETIME => {
                if body.len() != 4 {
                    return Err(BlockError::InvalidLength);
                }
                DecodedBlock::Timestamp(TimestampBlock::new(u32::from_be_bytes(
                    body.try_into().expect("timestamp length checked"),
                )))
            }
            BLOCK_OPTIONS => DecodedBlock::Options(OptionsBlock::decode(body)?),
            BLOCK_ROUTER_INFO => {
                if body.is_empty() {
                    return Err(BlockError::RouterInfoMalformed);
                }
                DecodedBlock::RouterInfo(RouterInfoBlock::new(body[0], body[1..].to_vec())?)
            }
            BLOCK_I2NP => {
                if body.len() < i2pr_proto::SHORT_TRANSPORT_HEADER_SIZE
                    || body.len() > i2pr_transport::MAX_I2NP_MESSAGE_BYTES
                {
                    return Err(BlockError::I2npMalformed);
                }
                DecodedBlock::I2np(ReceivedI2npBlock { bytes: body })
            }
            BLOCK_TERMINATION => {
                if !(9..=9 + MAX_TERMINATION_ADDITIONAL_BYTES).contains(&body.len()) {
                    return Err(BlockError::InvalidTermination);
                }
                DecodedBlock::Termination(TerminationBlock {
                    valid_frames_received: u64::from_be_bytes(
                        body[..8]
                            .try_into()
                            .expect("termination count length checked"),
                    ),
                    reason: TerminationReason::from_code(body[8]),
                    additional_length: body.len() - 9,
                })
            }
            BLOCK_PADDING => DecodedBlock::Padding { length: body.len() },
            _ => {
                unknown_bytes = unknown_bytes
                    .checked_add(body.len())
                    .ok_or(BlockError::ExcessiveUnknownBytes)?;
                if unknown_bytes > MAX_UNKNOWN_BLOCK_BYTES {
                    return Err(BlockError::ExcessiveUnknownBytes);
                }
                DecodedBlock::Unknown {
                    block_type,
                    length: body.len(),
                }
            }
        };
        blocks.push(decoded);
        offset = body_end;
    }
    Ok(ParsedBlocks {
        blocks,
        unknown_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_bytes(value: &str) -> Vec<u8> {
        value
            .trim()
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let high = (pair[0] as char).to_digit(16).expect("fixture hex");
                let low = (pair[1] as char).to_digit(16).expect("fixture hex");
                ((high << 4) | low) as u8
            })
            .collect()
    }

    #[test]
    fn canonical_blocks_round_trip_and_unknown_bytes_are_bounded() {
        let options =
            OptionsBlock::new(0, 16, 1, 32, 7, 8, 9, 10, vec![0xaa, 0xbb]).expect("options");
        let i2np =
            I2npMessageBlock::from_bytes(vec![3, 0, 0, 0, 7, 0, 0, 0, 9, 0xaa]).expect("I2NP");
        let plaintext = encode_blocks(vec![
            Block::Timestamp(TimestampBlock::new(9)),
            Block::Options(options),
            Block::I2np(i2np),
            Block::Padding(PaddingBlock::new(vec![0x55, 0x66]).expect("padding")),
        ])
        .expect("encode");
        let parsed = parse_blocks(&plaintext).expect("parse");
        assert_eq!(parsed.blocks().len(), 4);
        assert_eq!(parsed.unknown_bytes(), 0);
        assert!(matches!(parsed.blocks()[0], DecodedBlock::Timestamp(_)));
        assert!(matches!(parsed.blocks()[2], DecodedBlock::I2np(_)));

        let unknown = [200, 0, 2, 0xaa, 0xbb];
        let parsed = parse_blocks(&unknown).expect("unknown block");
        assert_eq!(parsed.unknown_bytes(), 2);
        assert!(matches!(parsed.blocks()[0], DecodedBlock::Unknown { .. }));
    }

    #[test]
    fn data_phase_accepts_repeated_non_padding_and_late_termination() {
        let bytes = [
            // DateTime, repeated.
            0, 0, 4, 0, 0, 0, 9, 0, 0, 4, 0, 0, 0, 10, // Options, repeated.
            1, 0, 12, 0, 16, 1, 32, 0, 7, 0, 8, 0, 9, 0, 10, 1, 0, 12, 0, 16, 1, 32, 0, 7, 0, 8, 0,
            9, 0, 10, // I2NP, repeated.
            3, 0, 10, 3, 0, 0, 0, 7, 0, 0, 0, 9, 0xaa, 3, 0, 10, 3, 0, 0, 0, 8, 0, 0, 0, 10, 0xbb,
            // Unknown blocks, mixed with known blocks and bounded in aggregate.
            200, 0, 1, 0xcc, 201, 0, 2, 0xdd, 0xee,
            // Termination may follow earlier valid blocks; Padding is allowed after it.
            4, 0, 9, 0, 0, 0, 0, 0, 0, 0, 7, 0, 254, 0, 2, 0x55, 0x66,
        ];
        let parsed = parse_blocks(&bytes).expect("general data-phase sequence");

        assert_eq!(parsed.blocks().len(), 10);
        assert_eq!(parsed.unknown_bytes(), 3);
        assert!(matches!(parsed.blocks()[0], DecodedBlock::Timestamp(_)));
        assert!(matches!(parsed.blocks()[1], DecodedBlock::Timestamp(_)));
        assert!(matches!(parsed.blocks()[2], DecodedBlock::Options(_)));
        assert!(matches!(parsed.blocks()[3], DecodedBlock::Options(_)));
        assert!(matches!(parsed.blocks()[4], DecodedBlock::I2np(_)));
        assert!(matches!(parsed.blocks()[5], DecodedBlock::I2np(_)));
        assert!(matches!(parsed.blocks()[6], DecodedBlock::Unknown { .. }));
        assert!(matches!(parsed.blocks()[7], DecodedBlock::Unknown { .. }));
        assert!(matches!(parsed.blocks()[8], DecodedBlock::Termination(_)));
        assert!(matches!(
            parsed.blocks()[9],
            DecodedBlock::Padding { length: 2 }
        ));
    }

    #[test]
    fn data_phase_encoder_accepts_repeated_non_padding_blocks() {
        let encoded = encode_blocks(vec![
            Block::Timestamp(TimestampBlock::new(9)),
            Block::Timestamp(TimestampBlock::new(10)),
            Block::Termination(TerminationBlock::new(7, TerminationReason::Normal)),
            Block::Padding(PaddingBlock::new(vec![0x55, 0x66]).expect("padding")),
        ])
        .expect("general data-phase sequence");
        assert_eq!(
            encoded,
            [
                0, 0, 4, 0, 0, 0, 9, 0, 0, 4, 0, 0, 0, 10, 4, 0, 9, 0, 0, 0, 0, 0, 0, 0, 7, 0, 254,
                0, 2, 0x55, 0x66,
            ]
        );
    }

    #[test]
    fn malformed_order_duplicates_and_trailing_headers_are_rejected() {
        let duplicate_padding = [BLOCK_PADDING, 0, 0, BLOCK_PADDING, 0, 0];
        assert!(matches!(
            parse_blocks(&duplicate_padding),
            Err(BlockError::DuplicateBlock)
        ));
        let after_padding = [BLOCK_PADDING, 0, 0, BLOCK_DATETIME, 0, 4, 0, 0, 0, 1];
        assert!(matches!(
            parse_blocks(&after_padding),
            Err(BlockError::InvalidOrder)
        ));
        let invalid_after_termination = [
            BLOCK_TERMINATION,
            0,
            9,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            7,
            0,
            BLOCK_DATETIME,
            0,
            4,
            0,
            0,
            0,
            1,
        ];
        assert!(matches!(
            parse_blocks(&invalid_after_termination),
            Err(BlockError::InvalidOrder)
        ));
        let duplicate_termination = [
            BLOCK_TERMINATION,
            0,
            9,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            7,
            0,
            BLOCK_TERMINATION,
            0,
            9,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            8,
            0,
        ];
        assert!(matches!(
            parse_blocks(&duplicate_termination),
            Err(BlockError::DuplicateBlock)
        ));
        assert!(matches!(
            parse_blocks(&[BLOCK_DATETIME, 0]),
            Err(BlockError::Truncated)
        ));
        let oversized_unknown = [200, 0xff, 0xff];
        assert!(matches!(
            parse_blocks(&oversized_unknown),
            Err(BlockError::Truncated)
        ));
    }

    #[test]
    fn termination_is_typed_and_does_not_retain_additional_text() {
        let mut bytes = vec![BLOCK_TERMINATION, 0, 10];
        bytes.extend_from_slice(&7_u64.to_be_bytes());
        bytes.extend_from_slice(&[TerminationReason::AeadFailure.code(), 0xaa]);
        let parsed = parse_blocks(&bytes).expect("termination");
        let DecodedBlock::Termination(value) = &parsed.blocks()[0] else {
            panic!("termination block");
        };
        assert_eq!(value.valid_frames_received(), 7);
        assert_eq!(value.reason(), TerminationReason::AeadFailure);
        assert_eq!(value.additional_length(), 1);

        let terminal_with_padding = [
            BLOCK_TERMINATION,
            0,
            9,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            7,
            0,
            BLOCK_PADDING,
            0,
            1,
            0,
        ];
        assert_eq!(
            parse_blocks(&terminal_with_padding)
                .expect("terminal padding parse")
                .blocks()
                .len(),
            2
        );
    }

    #[test]
    fn committed_block_fixtures_are_consumed() {
        let positive = fixture_bytes(include_str!(
            "../../../tests/fixtures/ntcp2/crypto/data-phase-blocks.hex"
        ));
        let parsed = parse_blocks(&positive).expect("positive fixture");
        assert_eq!(parsed.blocks().len(), 2);
        let malformed = fixture_bytes(include_str!(
            "../../../tests/fixtures/ntcp2/crypto/data-phase-malformed.hex"
        ));
        assert!(matches!(
            parse_blocks(&malformed),
            Err(BlockError::DuplicateBlock)
        ));
    }
}
