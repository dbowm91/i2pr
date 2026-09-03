//! Bounded SSU2 v2 authenticated-plaintext block codec.
//!
//! Blocks are parsed only after a payload has passed AEAD
//! authentication (Plan 156/157 concern). The parser is still
//! deliberately strict: every block is bounded by the authenticated
//! payload, unknown/reserved blocks have an aggregate budget, and no
//! block decoder may read into the following block.
//!
//! Normative traceability: SSU2 specification §Payload Blocks (type
//! table 0–21, 224–253 experimental, 254 padding, 255 reserved),
//! §Block Specifications (per-block layouts), §Block Ordering Rules
//! (Padding last and singular; Termination last non-padding block;
//! RouterInfo first in SessionConfirmed). ACK range
//! interpretation/loss semantics belong to Plan 157; relay/peer-test
//! signature verification belongs to Plan 160; RouterInfo
//! validation/fragmentation belongs to Plan 156. This module stores
//! relay/peer-test signatures as bounded opaque evidence and splits
//! the RelayResponse token from its fixed 8-byte tail.

use std::{fmt, net::IpAddr};

use i2pr_proto::{MessageType, SHORT_TRANSPORT_HEADER_SIZE};
use i2pr_transport::{EncodedI2npMessage, MAX_I2NP_MESSAGE_BYTES};
use thiserror::Error;

use crate::address::Ssu2Endpoint;
use crate::constants;

/// The encoded size of a block type and its big-endian length.
pub const BLOCK_HEADER_LENGTH: usize = constants::BLOCK_HEADER_LENGTH;
/// Minimum DateTime body length (4-byte Unix timestamp).
pub const DATETIME_BODY_LENGTH: usize = 4;
/// Minimum Options body length (12 fixed bytes).
pub const OPTIONS_MIN_LENGTH: usize = 12;
/// Maximum Options body length retained by the parser.
pub const MAX_OPTIONS_BYTES: usize = 4 * 1024;
/// RouterInfo fixed prefix (flags + frag bytes).
pub const ROUTER_INFO_PREFIX_LENGTH: usize = 2;
/// Complete I2NP / first-fragment fixed I2NP header length.
pub const I2NP_BLOCK_HEADER_LENGTH: usize = 9;
/// Follow-on fragment fixed prefix (frag byte + message ID).
pub const FOLLOW_ON_PREFIX_LENGTH: usize = 5;
/// Termination fixed body length (8-byte counter + reason).
pub const TERMINATION_BODY_LENGTH: usize = 9;
/// ACK fixed body length (ack-through + acnt).
pub const ACK_PREFIX_LENGTH: usize = 5;
/// NewToken exact body length (expires + token).
pub const NEW_TOKEN_BODY_LENGTH: usize = 12;
/// RelayTagRequest exact body length (empty).
pub const RELAY_TAG_REQUEST_BODY_LENGTH: usize = 0;
/// RelayTag exact body length (4-byte nonzero tag).
pub const RELAY_TAG_BODY_LENGTH: usize = 4;
/// FirstPacketNumber exact body length.
pub const FIRST_PACKET_NUMBER_BODY_LENGTH: usize = 4;
/// Address body lengths (port + IPv4 or IPv6).
pub const ADDRESS_V4_BODY_LENGTH: usize = 6;
/// Address body length for IPv6 endpoints.
pub const ADDRESS_V6_BODY_LENGTH: usize = 18;

/// Typed errors from canonical block encoding and authenticated parsing.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum BlockError {
    /// A complete block header or body was not present.
    #[error("truncated SSU2 block")]
    Truncated,
    /// A block length exceeded the authenticated payload boundary.
    #[error("SSU2 block length exceeds the authenticated payload")]
    LengthExceedsPayload,
    /// The total block count exceeded the local bounded parser policy.
    #[error("SSU2 payload contains too many blocks")]
    ExcessiveBlockCount,
    /// Unknown block bytes exceeded the aggregate skip budget.
    #[error("SSU2 unknown block budget exceeded")]
    ExcessiveUnknownBytes,
    /// A known block had an invalid fixed or variable length.
    #[error("invalid SSU2 block length")]
    InvalidLength,
    /// A required/control block appeared more than once.
    #[error("duplicate SSU2 control block")]
    DuplicateBlock,
    /// A block violated the terminal or padding ordering rule.
    #[error("invalid SSU2 block ordering")]
    InvalidOrder,
    /// A termination block contained an invalid shape.
    #[error("malformed SSU2 termination block")]
    InvalidTermination,
    /// A RouterInfo block used reserved flag bits, a fragmented frag
    /// byte, or malformed bytes.
    #[error("malformed SSU2 RouterInfo block")]
    RouterInfoMalformed,
    /// An I2NP block did not contain a bounded complete message.
    #[error("malformed SSU2 I2NP message block")]
    I2npMalformed,
    /// An I2NP fragment block had invalid metadata or an empty fragment.
    #[error("malformed SSU2 I2NP fragment block")]
    FragmentMalformed,
    /// An ACK block had an invalid range encoding.
    #[error("malformed SSU2 ACK block")]
    AckMalformed,
    /// An Address block was not a 6- or 18-byte endpoint.
    #[error("malformed SSU2 address block")]
    AddressMalformed,
    /// A relay block had invalid fixed fields or unbounded evidence.
    #[error("malformed SSU2 relay block")]
    RelayMalformed,
    /// A PeerTest block had an invalid message number, hash presence,
    /// or signature shape.
    #[error("malformed SSU2 peer-test block")]
    PeerTestMalformed,
    /// A bounded options block was malformed.
    #[error("malformed SSU2 options block")]
    OptionsMalformed,
    /// A NextNonce block was observed; key rotation is not implemented
    /// in this foundation (spec marks the block TODO).
    #[error("unsupported SSU2 block (key rotation not implemented)")]
    UnsupportedBlock,
    /// A caller supplied a zero or oversized payload.
    #[error("SSU2 block payload exceeds its bound")]
    PayloadTooLarge,
}

/// A data-phase timestamp in Unix seconds.
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

/// Bounded SSU2 options.
///
/// The first four bytes are 4.4 fixed-point padding ratios (unlike
/// NTCP2's padding-count bytes, no min/max ordering is enforced);
/// the remaining eight bytes are dummy-rate/delay controls. Trailing
/// `more_options` bytes are retained as opaque bounded extensions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionsBlock {
    transmit_min_ratio: u8,
    transmit_max_ratio: u8,
    receive_min_ratio: u8,
    receive_max_ratio: u8,
    transmit_dummy_rate: u16,
    receive_dummy_rate: u16,
    transmit_delay_ms: u16,
    receive_delay_ms: u16,
    extensions: Vec<u8>,
}

impl OptionsBlock {
    /// Creates options from the fixed fields and bounded extensions.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        transmit_min_ratio: u8,
        transmit_max_ratio: u8,
        receive_min_ratio: u8,
        receive_max_ratio: u8,
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
        Ok(Self {
            transmit_min_ratio,
            transmit_max_ratio,
            receive_min_ratio,
            receive_max_ratio,
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

    /// Returns the fixed transmit padding-ratio range.
    pub fn transmit_ratios(&self) -> (u8, u8) {
        (self.transmit_min_ratio, self.transmit_max_ratio)
    }

    /// Returns the fixed receive padding-ratio range.
    pub fn receive_ratios(&self) -> (u8, u8) {
        (self.receive_min_ratio, self.receive_max_ratio)
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
            self.transmit_min_ratio,
            self.transmit_max_ratio,
            self.receive_min_ratio,
            self.receive_max_ratio,
        ]);
        output.extend_from_slice(&self.transmit_dummy_rate.to_be_bytes());
        output.extend_from_slice(&self.receive_dummy_rate.to_be_bytes());
        output.extend_from_slice(&self.transmit_delay_ms.to_be_bytes());
        output.extend_from_slice(&self.receive_delay_ms.to_be_bytes());
        output.extend_from_slice(&self.extensions);
        output
    }
}

/// A RouterInfo candidate from an authenticated payload block.
///
/// The SSU2 RouterInfo block is never fragmented: the frag byte must
/// be exactly `0x01` (fragment 0 of 1). Signature verification and
/// establishment fragmentation policy belong to Plan 156; this type
/// enforces flag/frag/size structure only.
pub struct RouterInfoBlock {
    flags: u8,
    encoded: Vec<u8>,
}

impl RouterInfoBlock {
    /// Creates a RouterInfo block after structural validation.
    pub fn new(flags: u8, encoded: Vec<u8>) -> Result<Self, BlockError> {
        if flags & !0x03 != 0
            || encoded.is_empty()
            || encoded.len() > constants::MAX_ROUTER_INFO_BLOCK_BYTES
        {
            return Err(BlockError::RouterInfoMalformed);
        }
        Ok(Self { flags, encoded })
    }

    /// Decodes one complete RouterInfo body (flags + frag + bytes).
    pub fn decode(body: &[u8]) -> Result<Self, BlockError> {
        if body.len() < ROUTER_INFO_PREFIX_LENGTH {
            return Err(BlockError::RouterInfoMalformed);
        }
        if body[1] != 0x01 {
            return Err(BlockError::RouterInfoMalformed);
        }
        Self::new(body[0], body[ROUTER_INFO_PREFIX_LENGTH..].to_vec())
    }

    /// Returns the bounded flags (bit 0 flood request, bit 1 gzip).
    pub const fn flags(&self) -> u8 {
        self.flags
    }

    /// Returns whether the peer requested a flood operation.
    pub const fn flood_requested(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Returns whether the RouterInfo bytes are gzip compressed.
    pub const fn compressed(&self) -> bool {
        self.flags & 2 != 0
    }

    /// Borrows the exact encoded RouterInfo bytes.
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
}

impl fmt::Debug for RouterInfoBlock {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RouterInfoBlock")
            .field("flags", &self.flags)
            .field("length", &self.encoded.len())
            .finish()
    }
}

/// A bounded complete I2NP message carried in one block.
pub struct I2npMessageBlock {
    message: EncodedI2npMessage,
}

impl I2npMessageBlock {
    /// Takes the transport-owned encoded message without cloning it.
    pub fn new(message: EncodedI2npMessage) -> Result<Self, BlockError> {
        if message.len() < SHORT_TRANSPORT_HEADER_SIZE {
            return Err(BlockError::I2npMalformed);
        }
        Ok(Self { message })
    }

    /// Takes an already encoded short-header message.
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

/// The first fragment of an I2NP message (fragment 0, 9-byte header).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirstFragmentBlock {
    message_type: MessageType,
    message_id: u32,
    expiration_seconds: u32,
    fragment: Vec<u8>,
}

impl FirstFragmentBlock {
    /// Creates a first fragment with a nonempty partial message.
    pub fn new(
        message_type: MessageType,
        message_id: u32,
        expiration_seconds: u32,
        fragment: Vec<u8>,
    ) -> Result<Self, BlockError> {
        if fragment.is_empty() || fragment.len() > MAX_I2NP_MESSAGE_BYTES {
            return Err(BlockError::FragmentMalformed);
        }
        Ok(Self {
            message_type,
            message_id,
            expiration_seconds,
            fragment,
        })
    }

    /// Decodes one complete first-fragment body.
    pub fn decode(body: &[u8]) -> Result<Self, BlockError> {
        if body.len() < I2NP_BLOCK_HEADER_LENGTH {
            return Err(BlockError::FragmentMalformed);
        }
        Self::new(
            MessageType::from_code(body[0]),
            u32::from_be_bytes(body[1..5].try_into().expect("length checked")),
            u32::from_be_bytes(body[5..9].try_into().expect("length checked")),
            body[I2NP_BLOCK_HEADER_LENGTH..].to_vec(),
        )
    }

    /// Returns the I2NP message type.
    pub const fn message_type(&self) -> MessageType {
        self.message_type
    }

    /// Returns the I2NP message ID.
    pub const fn message_id(&self) -> u32 {
        self.message_id
    }

    /// Returns the short expiration value.
    pub const fn expiration_seconds(&self) -> u32 {
        self.expiration_seconds
    }

    /// Borrows the partial message bytes.
    pub fn fragment(&self) -> &[u8] {
        &self.fragment
    }
}

/// A follow-on fragment of an I2NP message (fragment 1..=127).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FollowOnFragmentBlock {
    frag_number: u8,
    is_last: bool,
    message_id: u32,
    fragment: Vec<u8>,
}

impl FollowOnFragmentBlock {
    /// Creates a follow-on fragment. Numbers run 1..=127; 0 is rejected.
    pub fn new(
        frag_number: u8,
        is_last: bool,
        message_id: u32,
        fragment: Vec<u8>,
    ) -> Result<Self, BlockError> {
        if !(1..=constants::MAX_FRAGMENT_NUMBER).contains(&frag_number)
            || fragment.is_empty()
            || fragment.len() > MAX_I2NP_MESSAGE_BYTES
        {
            return Err(BlockError::FragmentMalformed);
        }
        Ok(Self {
            frag_number,
            is_last,
            message_id,
            fragment,
        })
    }

    /// Decodes one complete follow-on fragment body.
    pub fn decode(body: &[u8]) -> Result<Self, BlockError> {
        if body.len() < FOLLOW_ON_PREFIX_LENGTH {
            return Err(BlockError::FragmentMalformed);
        }
        Self::new(
            body[0] >> 1,
            body[0] & 1 != 0,
            u32::from_be_bytes(body[1..5].try_into().expect("length checked")),
            body[FOLLOW_ON_PREFIX_LENGTH..].to_vec(),
        )
    }

    /// Returns the fragment number (1..=127).
    pub const fn frag_number(&self) -> u8 {
        self.frag_number
    }

    /// Returns whether this is the final fragment.
    pub const fn is_last(&self) -> bool {
        self.is_last
    }

    /// Returns the I2NP message ID.
    pub const fn message_id(&self) -> u32 {
        self.message_id
    }

    /// Borrows the partial message bytes.
    pub fn fragment(&self) -> &[u8] {
        &self.fragment
    }
}

/// Bounded termination reason codes from the SSU2 specification.
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
    /// SessionRequest error.
    SessionRequestError,
    /// SessionCreated error.
    SessionCreatedError,
    /// SessionConfirmed error.
    SessionConfirmedError,
    /// Timeout.
    Timeout,
    /// RouterInfo signature verification failure.
    RouterInfoSignatureFailure,
    /// Missing/invalid/mismatched `s` parameter in RouterInfo.
    StaticKeyFailure,
    /// Peer was banned.
    Banned,
    /// Bad token.
    BadToken,
    /// Connection limits.
    ConnectionLimits,
    /// Incompatible version.
    IncompatibleVersion,
    /// Wrong network ID.
    WrongNetworkId,
    /// Replaced by a new session.
    Replaced,
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
            Self::SessionRequestError => 11,
            Self::SessionCreatedError => 12,
            Self::SessionConfirmedError => 13,
            Self::Timeout => 14,
            Self::RouterInfoSignatureFailure => 15,
            Self::StaticKeyFailure => 16,
            Self::Banned => 17,
            Self::BadToken => 18,
            Self::ConnectionLimits => 19,
            Self::IncompatibleVersion => 20,
            Self::WrongNetworkId => 21,
            Self::Replaced => 22,
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
            11 => Self::SessionRequestError,
            12 => Self::SessionCreatedError,
            13 => Self::SessionConfirmedError,
            14 => Self::Timeout,
            15 => Self::RouterInfoSignatureFailure,
            16 => Self::StaticKeyFailure,
            17 => Self::Banned,
            18 => Self::BadToken,
            19 => Self::ConnectionLimits,
            20 => Self::IncompatibleVersion,
            21 => Self::WrongNetworkId,
            22 => Self::Replaced,
            other => Self::Unknown(other),
        }
    }
}

/// A termination control block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminationBlock {
    valid_packets_received: u64,
    reason: TerminationReason,
    additional_length: usize,
}

impl TerminationBlock {
    /// Creates a termination block without free-form remote text.
    pub const fn new(valid_packets_received: u64, reason: TerminationReason) -> Self {
        Self {
            valid_packets_received,
            reason,
            additional_length: 0,
        }
    }

    /// Returns the count of valid packets received by the sender.
    pub const fn valid_packets_received(self) -> u64 {
        self.valid_packets_received
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

/// A bounded ACK block (structural only; loss interpretation is Plan 157).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AckBlock {
    ack_through: u32,
    ack_count: u8,
    ranges: Vec<(u8, u8)>,
}

impl AckBlock {
    /// Creates an ACK block. Each range is a (nack, ack) pair; a pair
    /// with both counts zero is rejected, as is an over-limit range list.
    pub fn new(ack_through: u32, ack_count: u8, ranges: Vec<(u8, u8)>) -> Result<Self, BlockError> {
        if ranges.len() > constants::MAX_ACK_RANGES
            || ranges.iter().any(|(nack, ack)| *nack == 0 && *ack == 0)
        {
            return Err(BlockError::AckMalformed);
        }
        Ok(Self {
            ack_through,
            ack_count,
            ranges,
        })
    }

    /// Decodes one complete ACK body.
    pub fn decode(body: &[u8]) -> Result<Self, BlockError> {
        if body.len() < ACK_PREFIX_LENGTH || !(body.len() - ACK_PREFIX_LENGTH).is_multiple_of(2) {
            return Err(BlockError::AckMalformed);
        }
        let mut ranges = Vec::new();
        let mut offset = ACK_PREFIX_LENGTH;
        while offset < body.len() {
            ranges.push((body[offset], body[offset + 1]));
            offset += 2;
        }
        Self::new(
            u32::from_be_bytes(body[0..4].try_into().expect("length checked")),
            body[4],
            ranges,
        )
    }

    /// Returns the highest packet number acked.
    pub const fn ack_through(&self) -> u32 {
        self.ack_through
    }

    /// Returns the count of packets below ack-through also acked.
    pub const fn ack_count(&self) -> u8 {
        self.ack_count
    }

    /// Returns the (nack, ack) ranges below the initial run.
    pub fn ranges(&self) -> &[(u8, u8)] {
        &self.ranges
    }
}

/// An Address block endpoint (port + IPv4/IPv6, network order).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddressBlock {
    endpoint: Ssu2Endpoint,
}

impl AddressBlock {
    /// Creates an address block from a validated endpoint.
    pub const fn new(endpoint: Ssu2Endpoint) -> Self {
        Self { endpoint }
    }

    /// Decodes a 6- or 18-byte address body (port first, then IP).
    pub fn decode(body: &[u8]) -> Result<Self, BlockError> {
        let ip = match body.len() {
            ADDRESS_V4_BODY_LENGTH => {
                IpAddr::from(<[u8; 4]>::try_from(&body[2..6]).expect("length checked"))
            }
            ADDRESS_V6_BODY_LENGTH => {
                IpAddr::from(<[u8; 16]>::try_from(&body[2..18]).expect("length checked"))
            }
            _ => return Err(BlockError::AddressMalformed),
        };
        let endpoint = Ssu2Endpoint::new(
            ip,
            u16::from_be_bytes(body[0..2].try_into().expect("length checked")),
        )
        .map_err(|_| BlockError::AddressMalformed)?;
        Ok(Self { endpoint })
    }

    /// Returns the carried endpoint.
    pub const fn endpoint(self) -> Ssu2Endpoint {
        self.endpoint
    }
}

/// A RelayRequest block (Alice to Bob, in-session Data).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayRequestBlock {
    nonce: u32,
    relay_tag: u32,
    timestamp: u32,
    version: u8,
    endpoint: Ssu2Endpoint,
    signature: Vec<u8>,
}

impl RelayRequestBlock {
    /// Creates a relay request with bounded opaque signature evidence.
    /// Signature verification belongs to Plan 160.
    pub fn new(
        nonce: u32,
        relay_tag: u32,
        timestamp: u32,
        version: u8,
        endpoint: Ssu2Endpoint,
        signature: Vec<u8>,
    ) -> Result<Self, BlockError> {
        if relay_tag == 0 || !matches!(version, 1 | 2) {
            return Err(BlockError::RelayMalformed);
        }
        check_signature(&signature)?;
        Ok(Self {
            nonce,
            relay_tag,
            timestamp,
            version,
            endpoint,
            signature,
        })
    }

    /// Returns the request nonce.
    pub const fn nonce(&self) -> u32 {
        self.nonce
    }

    /// Returns the introducer relay tag.
    pub const fn relay_tag(&self) -> u32 {
        self.relay_tag
    }

    /// Returns the Unix timestamp.
    pub const fn timestamp(&self) -> u32 {
        self.timestamp
    }

    /// Returns the SSU version requested for the introduction.
    pub const fn version(&self) -> u8 {
        self.version
    }

    /// Returns Alice's endpoint.
    pub const fn endpoint(&self) -> Ssu2Endpoint {
        self.endpoint
    }

    /// Returns the bounded opaque signature evidence.
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }
}

/// Typed RelayResponse status codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelayResponseCode {
    /// Accept (0); carries endpoint, signature, and a fresh token.
    Accept,
    /// Rejected by Bob (1..=63); no signature is present.
    RejectedByBob(u8),
    /// Rejected by Charlie (64..=127); a signature may be present.
    RejectedByCharlie(u8),
    /// Unspecified/other reject (128..=255).
    RejectedOther(u8),
}

impl RelayResponseCode {
    /// Converts a wire code to its typed category.
    pub const fn from_u8(code: u8) -> Self {
        match code {
            0 => Self::Accept,
            1..=63 => Self::RejectedByBob(code),
            64..=127 => Self::RejectedByCharlie(code),
            _ => Self::RejectedOther(code),
        }
    }

    /// Converts the category back to its wire code.
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Accept => 0,
            Self::RejectedByBob(code)
            | Self::RejectedByCharlie(code)
            | Self::RejectedOther(code) => code,
        }
    }

    /// Returns whether the response accepts the relay.
    pub const fn is_accept(self) -> bool {
        matches!(self, Self::Accept)
    }
}

/// A RelayResponse block.
///
/// Accept responses carry endpoint, signature, and a fresh 8-byte
/// token split from the fixed body tail. Bob rejections carry
/// timestamp/version only; Charlie rejections may carry endpoint and
/// signature but never a token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayResponseBlock {
    code: RelayResponseCode,
    nonce: u32,
    timestamp: u32,
    version: u8,
    endpoint: Option<Ssu2Endpoint>,
    signature: Vec<u8>,
    token: Option<u64>,
}

impl RelayResponseBlock {
    /// Creates an accept response with a fresh token.
    pub fn accept(
        nonce: u32,
        timestamp: u32,
        version: u8,
        endpoint: Ssu2Endpoint,
        signature: Vec<u8>,
        token: u64,
    ) -> Result<Self, BlockError> {
        check_version(version)?;
        check_signature(&signature)?;
        Ok(Self {
            code: RelayResponseCode::Accept,
            nonce,
            timestamp,
            version,
            endpoint: Some(endpoint),
            signature,
            token: Some(token),
        })
    }

    /// Creates a rejection. Bob rejections must not carry endpoint or
    /// signature evidence; Charlie/other rejections may carry both.
    pub fn reject(
        code: RelayResponseCode,
        nonce: u32,
        timestamp: u32,
        version: u8,
        endpoint: Option<Ssu2Endpoint>,
        signature: Vec<u8>,
    ) -> Result<Self, BlockError> {
        if code.is_accept() {
            return Err(BlockError::RelayMalformed);
        }
        check_version(version)?;
        match code {
            RelayResponseCode::Accept => return Err(BlockError::RelayMalformed),
            RelayResponseCode::RejectedByBob(_) => {
                if endpoint.is_some() || !signature.is_empty() {
                    return Err(BlockError::RelayMalformed);
                }
            }
            RelayResponseCode::RejectedByCharlie(_) | RelayResponseCode::RejectedOther(_) => {
                check_signature_or_empty(&signature)?;
            }
        }
        Ok(Self {
            code,
            nonce,
            timestamp,
            version,
            endpoint,
            signature,
            token: None,
        })
    }

    /// Returns the typed status code.
    pub const fn code(&self) -> RelayResponseCode {
        self.code
    }

    /// Returns the request nonce.
    pub const fn nonce(&self) -> u32 {
        self.nonce
    }

    /// Returns the Unix timestamp.
    pub const fn timestamp(&self) -> u32 {
        self.timestamp
    }

    /// Returns the SSU version for the introduction.
    pub const fn version(&self) -> u8 {
        self.version
    }

    /// Returns Charlie's endpoint, if present.
    pub const fn endpoint(&self) -> Option<Ssu2Endpoint> {
        self.endpoint
    }

    /// Returns the bounded opaque signature evidence.
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }

    /// Returns the fresh token (accept responses only).
    pub const fn token(&self) -> Option<u64> {
        self.token
    }
}

/// A RelayIntro block (Bob to Charlie, in-session Data).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayIntroBlock {
    alice_hash: [u8; 32],
    nonce: u32,
    relay_tag: u32,
    timestamp: u32,
    version: u8,
    endpoint: Ssu2Endpoint,
    signature: Vec<u8>,
}

impl RelayIntroBlock {
    /// Creates a relay introduction with bounded opaque signature evidence.
    pub fn new(
        alice_hash: [u8; 32],
        nonce: u32,
        relay_tag: u32,
        timestamp: u32,
        version: u8,
        endpoint: Ssu2Endpoint,
        signature: Vec<u8>,
    ) -> Result<Self, BlockError> {
        if relay_tag == 0 {
            return Err(BlockError::RelayMalformed);
        }
        check_version(version)?;
        check_signature(&signature)?;
        Ok(Self {
            alice_hash,
            nonce,
            relay_tag,
            timestamp,
            version,
            endpoint,
            signature,
        })
    }

    /// Returns Alice's 32-byte router hash.
    pub const fn alice_hash(&self) -> &[u8; 32] {
        &self.alice_hash
    }

    /// Returns the request nonce.
    pub const fn nonce(&self) -> u32 {
        self.nonce
    }

    /// Returns the introducer relay tag.
    pub const fn relay_tag(&self) -> u32 {
        self.relay_tag
    }

    /// Returns the Unix timestamp.
    pub const fn timestamp(&self) -> u32 {
        self.timestamp
    }

    /// Returns the SSU version for the introduction.
    pub const fn version(&self) -> u8 {
        self.version
    }

    /// Returns Alice's endpoint.
    pub const fn endpoint(&self) -> Ssu2Endpoint {
        self.endpoint
    }

    /// Returns the bounded opaque signature evidence.
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }
}

/// A PeerTest block (in-session Data or out-of-session PeerTest).
///
/// Messages 2 and 4 carry a 32-byte router hash; all other messages
/// carry none. Messages 1–4 require a nonempty signature; messages
/// 5–7 leave it optional. Signature verification belongs to Plan 160.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeerTestBlock {
    message: u8,
    code: u8,
    router_hash: Option<[u8; 32]>,
    version: u8,
    nonce: u32,
    timestamp: u32,
    endpoint: Ssu2Endpoint,
    signature: Vec<u8>,
}

impl PeerTestBlock {
    /// Creates a peer-test block with the message-appropriate hash and
    /// signature shape.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        message: u8,
        code: u8,
        router_hash: Option<[u8; 32]>,
        version: u8,
        nonce: u32,
        timestamp: u32,
        endpoint: Ssu2Endpoint,
        signature: Vec<u8>,
    ) -> Result<Self, BlockError> {
        if !(1..=7).contains(&message) {
            return Err(BlockError::PeerTestMalformed);
        }
        let needs_hash = matches!(message, 2 | 4);
        if router_hash.is_some() != needs_hash {
            return Err(BlockError::PeerTestMalformed);
        }
        if version != constants::SSU2_VERSION {
            return Err(BlockError::PeerTestMalformed);
        }
        if message <= 4 {
            check_signature(&signature)?;
        } else {
            check_signature_or_empty(&signature)?;
        }
        Ok(Self {
            message,
            code,
            router_hash,
            version,
            nonce,
            timestamp,
            endpoint,
            signature,
        })
    }

    /// Returns the peer-test message number (1..=7).
    pub const fn message(&self) -> u8 {
        self.message
    }

    /// Returns the raw status code.
    pub const fn code(&self) -> u8 {
        self.code
    }

    /// Returns the message 2/4 router hash, if present.
    pub const fn router_hash(&self) -> Option<&[u8; 32]> {
        self.router_hash.as_ref()
    }

    /// Returns the SSU version under test.
    pub const fn version(&self) -> u8 {
        self.version
    }

    /// Returns the test nonce.
    pub const fn nonce(&self) -> u32 {
        self.nonce
    }

    /// Returns the Unix timestamp.
    pub const fn timestamp(&self) -> u32 {
        self.timestamp
    }

    /// Returns Alice's endpoint.
    pub const fn endpoint(&self) -> Ssu2Endpoint {
        self.endpoint
    }

    /// Returns the bounded opaque signature evidence.
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }
}

/// A NewToken block carrying a one-use token and its expiry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NewTokenBlock {
    expires: u32,
    token: u64,
}

impl NewTokenBlock {
    /// Creates a new-token block.
    pub const fn new(expires: u32, token: u64) -> Self {
        Self { expires, token }
    }

    /// Returns the Unix expiry timestamp.
    pub const fn expires(self) -> u32 {
        self.expires
    }

    /// Returns the one-use token.
    pub const fn token(self) -> u64 {
        self.token
    }
}

/// A PathChallenge block with bounded opaque challenge data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathChallengeBlock {
    data: Vec<u8>,
}

impl PathChallengeBlock {
    /// Creates a path challenge with bounded data.
    pub fn new(data: Vec<u8>) -> Result<Self, BlockError> {
        if data.len() > constants::MAX_PATH_DATA_BYTES {
            return Err(BlockError::PayloadTooLarge);
        }
        Ok(Self { data })
    }

    /// Returns the opaque challenge data.
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

/// A PathResponse block echoing bounded challenge data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathResponseBlock {
    data: Vec<u8>,
}

impl PathResponseBlock {
    /// Creates a path response with bounded data.
    pub fn new(data: Vec<u8>) -> Result<Self, BlockError> {
        if data.len() > constants::MAX_PATH_DATA_BYTES {
            return Err(BlockError::PayloadTooLarge);
        }
        Ok(Self { data })
    }

    /// Returns the echoed challenge data.
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

/// A RelayTag block carrying a nonzero tag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelayTagBlock {
    tag: u32,
}

impl RelayTagBlock {
    /// Creates a relay-tag block; zero tags are rejected.
    pub const fn new(tag: u32) -> Result<Self, BlockError> {
        if tag == 0 {
            return Err(BlockError::RelayMalformed);
        }
        Ok(Self { tag })
    }

    /// Returns the nonzero relay tag.
    pub const fn tag(self) -> u32 {
        self.tag
    }
}

/// A FirstPacketNumber block selecting the data-phase start number.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FirstPacketNumberBlock {
    first_packet_number: u32,
}

impl FirstPacketNumberBlock {
    /// Creates a first-packet-number block.
    pub const fn new(first_packet_number: u32) -> Self {
        Self {
            first_packet_number,
        }
    }

    /// Returns the first data-phase packet number.
    pub const fn first_packet_number(self) -> u32 {
        self.first_packet_number
    }
}

/// A Congestion block with flags and bounded extensions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CongestionBlock {
    flags: u8,
    extensions: Vec<u8>,
}

impl CongestionBlock {
    /// Creates a congestion block with bounded extension bytes.
    pub fn new(flags: u8, extensions: Vec<u8>) -> Result<Self, BlockError> {
        if extensions.len() > constants::MAX_CONGESTION_EXTENSION_BYTES {
            return Err(BlockError::PayloadTooLarge);
        }
        Ok(Self { flags, extensions })
    }

    /// Decodes one complete congestion body (flags + extensions).
    pub fn decode(body: &[u8]) -> Result<Self, BlockError> {
        if body.is_empty() {
            return Err(BlockError::InvalidLength);
        }
        Self::new(body[0], body[1..].to_vec())
    }

    /// Returns the congestion flags.
    pub const fn flags(&self) -> u8 {
        self.flags
    }

    /// Returns whether an immediate ACK was requested.
    pub fn requests_immediate_ack(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Returns bounded extension bytes.
    pub fn extensions(&self) -> &[u8] {
        &self.extensions
    }
}

/// A bounded padding block. Padding bytes are never in diagnostics.
pub struct PaddingBlock {
    bytes: Vec<u8>,
}

impl PaddingBlock {
    /// Takes deterministic or runtime-generated padding bytes.
    pub fn new(bytes: Vec<u8>) -> Result<Self, BlockError> {
        if bytes.len() > max_padding_body() {
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

fn max_padding_body() -> usize {
    constants::MAX_DATAGRAM_IPV4_LENGTH - BLOCK_HEADER_LENGTH
}

fn check_version(version: u8) -> Result<(), BlockError> {
    if !matches!(version, 1 | 2) {
        return Err(BlockError::RelayMalformed);
    }
    Ok(())
}

fn check_signature(signature: &[u8]) -> Result<(), BlockError> {
    if signature.is_empty() || signature.len() > constants::MAX_SIGNATURE_BYTES {
        return Err(BlockError::RelayMalformed);
    }
    Ok(())
}

fn check_signature_or_empty(signature: &[u8]) -> Result<(), BlockError> {
    if signature.len() > constants::MAX_SIGNATURE_BYTES {
        return Err(BlockError::RelayMalformed);
    }
    Ok(())
}

fn decode_endpoint(asz: u8, bytes: &[u8]) -> Result<(Ssu2Endpoint, usize), BlockError> {
    let (expected, ip_len) = match asz {
        6 => (6_usize, 4_usize),
        18 => (18_usize, 16_usize),
        _ => return Err(BlockError::RelayMalformed),
    };
    if bytes.len() < expected {
        return Err(BlockError::RelayMalformed);
    }
    let port = u16::from_be_bytes([bytes[0], bytes[1]]);
    let ip = match ip_len {
        4 => IpAddr::from(<[u8; 4]>::try_from(&bytes[2..6]).expect("length checked")),
        _ => IpAddr::from(<[u8; 16]>::try_from(&bytes[2..18]).expect("length checked")),
    };
    let endpoint = Ssu2Endpoint::new(ip, port).map_err(|_| BlockError::RelayMalformed)?;
    Ok((endpoint, expected))
}

fn encode_endpoint(endpoint: Ssu2Endpoint, output: &mut Vec<u8>) {
    output.extend_from_slice(&endpoint.port().to_be_bytes());
    match endpoint.ip() {
        IpAddr::V4(address) => output.extend_from_slice(&address.octets()),
        IpAddr::V6(address) => output.extend_from_slice(&address.octets()),
    }
}

fn endpoint_size(endpoint: Ssu2Endpoint) -> u8 {
    match endpoint.ip() {
        IpAddr::V4(_) => 6,
        IpAddr::V6(_) => 18,
    }
}

/// One canonical outbound block.
#[allow(clippy::large_enum_variant)]
pub enum Block {
    /// A Unix timestamp.
    Timestamp(TimestampBlock),
    /// Bounded options.
    Options(OptionsBlock),
    /// A RouterInfo candidate (structural; Plan 156 verifies).
    RouterInfo(RouterInfoBlock),
    /// A complete I2NP message.
    I2np(I2npMessageBlock),
    /// A first I2NP fragment.
    FirstFragment(FirstFragmentBlock),
    /// A follow-on I2NP fragment.
    FollowOnFragment(FollowOnFragmentBlock),
    /// An explicit termination control block.
    Termination(TerminationBlock),
    /// A relay request.
    RelayRequest(RelayRequestBlock),
    /// A relay response.
    RelayResponse(RelayResponseBlock),
    /// A relay introduction.
    RelayIntro(RelayIntroBlock),
    /// A peer-test message.
    PeerTest(PeerTestBlock),
    /// An ACK block (structural; Plan 157 interprets).
    Ack(AckBlock),
    /// An address endpoint.
    Address(AddressBlock),
    /// A relay-tag request (empty).
    RelayTagRequest,
    /// A relay tag.
    RelayTag(RelayTagBlock),
    /// A one-use token with expiry.
    NewToken(NewTokenBlock),
    /// A path challenge.
    PathChallenge(PathChallengeBlock),
    /// A path response.
    PathResponse(PathResponseBlock),
    /// A first-packet-number selector.
    FirstPacketNumber(FirstPacketNumberBlock),
    /// Congestion information.
    Congestion(CongestionBlock),
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
            Self::FirstFragment(value) => {
                formatter.debug_tuple("FirstFragment").field(value).finish()
            }
            Self::FollowOnFragment(value) => formatter
                .debug_tuple("FollowOnFragment")
                .field(value)
                .finish(),
            Self::Termination(value) => formatter.debug_tuple("Termination").field(value).finish(),
            Self::RelayRequest(value) => {
                formatter.debug_tuple("RelayRequest").field(value).finish()
            }
            Self::RelayResponse(value) => {
                formatter.debug_tuple("RelayResponse").field(value).finish()
            }
            Self::RelayIntro(value) => formatter.debug_tuple("RelayIntro").field(value).finish(),
            Self::PeerTest(value) => formatter.debug_tuple("PeerTest").field(value).finish(),
            Self::Ack(value) => formatter.debug_tuple("Ack").field(value).finish(),
            Self::Address(value) => formatter.debug_tuple("Address").field(value).finish(),
            Self::RelayTagRequest => formatter.debug_tuple("RelayTagRequest").finish(),
            Self::RelayTag(value) => formatter.debug_tuple("RelayTag").field(value).finish(),
            Self::NewToken(value) => formatter.debug_tuple("NewToken").field(value).finish(),
            Self::PathChallenge(value) => {
                formatter.debug_tuple("PathChallenge").field(value).finish()
            }
            Self::PathResponse(value) => {
                formatter.debug_tuple("PathResponse").field(value).finish()
            }
            Self::FirstPacketNumber(value) => formatter
                .debug_tuple("FirstPacketNumber")
                .field(value)
                .finish(),
            Self::Congestion(value) => formatter.debug_tuple("Congestion").field(value).finish(),
            Self::Padding(value) => formatter.debug_tuple("Padding").field(value).finish(),
        }
    }
}

impl Block {
    /// Returns the block type code.
    pub const fn kind(&self) -> u8 {
        match self {
            Self::Timestamp(_) => constants::BLOCK_DATETIME,
            Self::Options(_) => constants::BLOCK_OPTIONS,
            Self::RouterInfo(_) => constants::BLOCK_ROUTER_INFO,
            Self::I2np(_) => constants::BLOCK_I2NP_MESSAGE,
            Self::FirstFragment(_) => constants::BLOCK_FIRST_FRAGMENT,
            Self::FollowOnFragment(_) => constants::BLOCK_FOLLOW_ON_FRAGMENT,
            Self::Termination(_) => constants::BLOCK_TERMINATION,
            Self::RelayRequest(_) => constants::BLOCK_RELAY_REQUEST,
            Self::RelayResponse(_) => constants::BLOCK_RELAY_RESPONSE,
            Self::RelayIntro(_) => constants::BLOCK_RELAY_INTRO,
            Self::PeerTest(_) => constants::BLOCK_PEER_TEST,
            Self::Ack(_) => constants::BLOCK_ACK,
            Self::Address(_) => constants::BLOCK_ADDRESS,
            Self::RelayTagRequest => constants::BLOCK_RELAY_TAG_REQUEST,
            Self::RelayTag(_) => constants::BLOCK_RELAY_TAG,
            Self::NewToken(_) => constants::BLOCK_NEW_TOKEN,
            Self::PathChallenge(_) => constants::BLOCK_PATH_CHALLENGE,
            Self::PathResponse(_) => constants::BLOCK_PATH_RESPONSE,
            Self::FirstPacketNumber(_) => constants::BLOCK_FIRST_PACKET_NUMBER,
            Self::Congestion(_) => constants::BLOCK_CONGESTION,
            Self::Padding(_) => constants::BLOCK_PADDING,
        }
    }

    /// Returns the complete encoded block size.
    pub fn encoded_len(&self) -> usize {
        BLOCK_HEADER_LENGTH
            + match self {
                Self::Timestamp(_) => DATETIME_BODY_LENGTH,
                Self::Options(value) => OPTIONS_MIN_LENGTH + value.extensions.len(),
                Self::RouterInfo(value) => ROUTER_INFO_PREFIX_LENGTH + value.encoded.len(),
                Self::I2np(value) => value.message.len(),
                Self::FirstFragment(value) => I2NP_BLOCK_HEADER_LENGTH + value.fragment.len(),
                Self::FollowOnFragment(value) => FOLLOW_ON_PREFIX_LENGTH + value.fragment.len(),
                Self::Termination(value) => TERMINATION_BODY_LENGTH + value.additional_length,
                Self::RelayRequest(value) => {
                    15 + usize::from(endpoint_size(value.endpoint)) + value.signature.len()
                }
                Self::RelayResponse(value) => relay_response_len(value),
                Self::RelayIntro(value) => {
                    47 + usize::from(endpoint_size(value.endpoint)) + value.signature.len()
                }
                Self::PeerTest(value) => peer_test_len(value),
                Self::Ack(value) => ACK_PREFIX_LENGTH + 2 * value.ranges.len(),
                Self::Address(value) => match value.endpoint.ip() {
                    IpAddr::V4(_) => ADDRESS_V4_BODY_LENGTH,
                    IpAddr::V6(_) => ADDRESS_V6_BODY_LENGTH,
                },
                Self::RelayTagRequest => RELAY_TAG_REQUEST_BODY_LENGTH,
                Self::RelayTag(_) => RELAY_TAG_BODY_LENGTH,
                Self::NewToken(_) => NEW_TOKEN_BODY_LENGTH,
                Self::PathChallenge(value) => value.data.len(),
                Self::PathResponse(value) => value.data.len(),
                Self::FirstPacketNumber(_) => FIRST_PACKET_NUMBER_BODY_LENGTH,
                Self::Congestion(value) => 1 + value.extensions.len(),
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
                output.push(0x01);
                output.extend_from_slice(&value.encoded);
            }
            Self::I2np(value) => output.extend_from_slice(value.message.as_bytes()),
            Self::FirstFragment(value) => {
                output.push(value.message_type.code());
                output.extend_from_slice(&value.message_id.to_be_bytes());
                output.extend_from_slice(&value.expiration_seconds.to_be_bytes());
                output.extend_from_slice(&value.fragment);
            }
            Self::FollowOnFragment(value) => {
                output.push((value.frag_number << 1) | u8::from(value.is_last));
                output.extend_from_slice(&value.message_id.to_be_bytes());
                output.extend_from_slice(&value.fragment);
            }
            Self::Termination(value) => {
                output.extend_from_slice(&value.valid_packets_received.to_be_bytes());
                output.push(value.reason.code());
            }
            Self::RelayRequest(value) => {
                output.push(0);
                output.extend_from_slice(&value.nonce.to_be_bytes());
                output.extend_from_slice(&value.relay_tag.to_be_bytes());
                output.extend_from_slice(&value.timestamp.to_be_bytes());
                output.push(value.version);
                output.push(endpoint_size(value.endpoint));
                encode_endpoint(value.endpoint, output);
                output.extend_from_slice(&value.signature);
            }
            Self::RelayResponse(value) => encode_relay_response(value, output),
            Self::RelayIntro(value) => {
                output.push(0);
                output.extend_from_slice(&value.alice_hash);
                output.extend_from_slice(&value.nonce.to_be_bytes());
                output.extend_from_slice(&value.relay_tag.to_be_bytes());
                output.extend_from_slice(&value.timestamp.to_be_bytes());
                output.push(value.version);
                output.push(endpoint_size(value.endpoint));
                encode_endpoint(value.endpoint, output);
                output.extend_from_slice(&value.signature);
            }
            Self::PeerTest(value) => encode_peer_test(value, output),
            Self::Ack(value) => {
                output.extend_from_slice(&value.ack_through.to_be_bytes());
                output.push(value.ack_count);
                for (nack, ack) in value.ranges {
                    output.push(nack);
                    output.push(ack);
                }
            }
            Self::Address(value) => {
                output.extend_from_slice(&value.endpoint.port().to_be_bytes());
                match value.endpoint.ip() {
                    IpAddr::V4(address) => output.extend_from_slice(&address.octets()),
                    IpAddr::V6(address) => output.extend_from_slice(&address.octets()),
                }
            }
            Self::RelayTagRequest => {}
            Self::RelayTag(value) => output.extend_from_slice(&value.tag.to_be_bytes()),
            Self::NewToken(value) => {
                output.extend_from_slice(&value.expires.to_be_bytes());
                output.extend_from_slice(&value.token.to_be_bytes());
            }
            Self::PathChallenge(value) => output.extend_from_slice(&value.data),
            Self::PathResponse(value) => output.extend_from_slice(&value.data),
            Self::FirstPacketNumber(value) => {
                output.extend_from_slice(&value.first_packet_number.to_be_bytes());
            }
            Self::Congestion(value) => {
                output.push(value.flags);
                output.extend_from_slice(&value.extensions);
            }
            Self::Padding(value) => output.extend_from_slice(&value.bytes),
        }
        Ok(())
    }
}

fn relay_response_has_trailer(value: &RelayResponseBlock) -> bool {
    !matches!(value.code, RelayResponseCode::RejectedByBob(_)) || value.endpoint.is_some()
}

fn relay_response_len(value: &RelayResponseBlock) -> usize {
    // flag(1) + code(1) + nonce(4)
    let mut len = 6;
    if relay_response_has_trailer(value) {
        // timestamp(4) + ver(1) + csz(1) + endpoint + signature
        len += 6;
        if let Some(endpoint) = value.endpoint {
            len += usize::from(endpoint_size(endpoint));
        }
        len += value.signature.len();
    } else {
        // Bob rejection: timestamp(4) + ver(1) + csz(1), no evidence.
        len += 6;
    }
    if value.token.is_some() {
        len += 8;
    }
    len
}

fn peer_test_len(value: &PeerTestBlock) -> usize {
    let hash_len = if value.router_hash.is_some() { 32 } else { 0 };
    // msg(1) + code(1) + flag(1) + hash + ver(1) + nonce(4) +
    // timestamp(4) + asz(1) + endpoint + signature
    3 + hash_len
        + 1
        + 4
        + 4
        + 1
        + usize::from(endpoint_size(value.endpoint))
        + value.signature.len()
}

fn encode_relay_response(value: RelayResponseBlock, output: &mut Vec<u8>) {
    output.push(0);
    output.push(value.code.as_u8());
    output.extend_from_slice(&value.nonce.to_be_bytes());
    if relay_response_has_trailer(&value) {
        output.extend_from_slice(&value.timestamp.to_be_bytes());
        output.push(value.version);
        match value.endpoint {
            Some(endpoint) => {
                output.push(endpoint_size(endpoint));
                encode_endpoint(endpoint, output);
            }
            None => output.push(0),
        }
        output.extend_from_slice(&value.signature);
    } else {
        output.extend_from_slice(&value.timestamp.to_be_bytes());
        output.push(value.version);
        output.push(0);
    }
    if let Some(token) = value.token {
        output.extend_from_slice(&token.to_be_bytes());
    }
}

fn encode_peer_test(value: PeerTestBlock, output: &mut Vec<u8>) {
    output.push(value.message);
    output.push(value.code);
    output.push(0);
    if let Some(hash) = value.router_hash {
        output.extend_from_slice(&hash);
    }
    output.push(value.version);
    output.extend_from_slice(&value.nonce.to_be_bytes());
    output.extend_from_slice(&value.timestamp.to_be_bytes());
    output.push(endpoint_size(value.endpoint));
    encode_endpoint(value.endpoint, output);
    output.extend_from_slice(&value.signature);
}

/// Encodes a bounded sequence of blocks as authenticated plaintext.
pub fn encode_blocks(blocks: Vec<Block>) -> Result<Vec<u8>, BlockError> {
    if blocks.len() > constants::MAX_BLOCK_COUNT {
        return Err(BlockError::ExcessiveBlockCount);
    }
    let mut output = Vec::new();
    let mut padding_seen = false;
    let mut termination_seen = false;
    for block in blocks {
        if block.kind() == constants::BLOCK_PADDING {
            if padding_seen {
                return Err(BlockError::DuplicateBlock);
            }
            padding_seen = true;
        } else if padding_seen {
            return Err(BlockError::InvalidOrder);
        }
        if block.kind() == constants::BLOCK_TERMINATION {
            if termination_seen {
                return Err(BlockError::DuplicateBlock);
            }
            termination_seen = true;
        } else if termination_seen && block.kind() != constants::BLOCK_PADDING {
            return Err(BlockError::InvalidOrder);
        }
        if output.len() + block.encoded_len() > max_payload_bytes() {
            return Err(BlockError::PayloadTooLarge);
        }
        block.encode_into(&mut output)?;
    }
    Ok(output)
}

fn max_payload_bytes() -> usize {
    constants::MAX_DATAGRAM_IPV4_LENGTH - 60
}

/// A borrowed authenticated I2NP block.
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

/// Borrowed semantic output from one authenticated payload parse.
#[allow(clippy::large_enum_variant)]
pub enum DecodedBlock<'a> {
    /// Date/time in Unix seconds.
    Timestamp(TimestampBlock),
    /// Bounded options.
    Options(OptionsBlock),
    /// RouterInfo candidate (structural; Plan 156 verifies).
    RouterInfo(RouterInfoBlock),
    /// Complete bounded I2NP message.
    I2np(ReceivedI2npBlock<'a>),
    /// First I2NP fragment.
    FirstFragment(FirstFragmentBlock),
    /// Follow-on I2NP fragment.
    FollowOnFragment(FollowOnFragmentBlock),
    /// Authenticated termination metadata; additional data discarded.
    Termination(TerminationBlock),
    /// Relay request.
    RelayRequest(RelayRequestBlock),
    /// Relay response.
    RelayResponse(RelayResponseBlock),
    /// Relay introduction.
    RelayIntro(RelayIntroBlock),
    /// Peer-test message.
    PeerTest(PeerTestBlock),
    /// ACK ranges (structural; Plan 157 interprets).
    Ack(AckBlock),
    /// Address endpoint.
    Address(AddressBlock),
    /// Empty relay-tag request.
    RelayTagRequest,
    /// Nonzero relay tag.
    RelayTag(RelayTagBlock),
    /// One-use token with expiry.
    NewToken(NewTokenBlock),
    /// Path challenge data.
    PathChallenge(PathChallengeBlock),
    /// Path response data.
    PathResponse(PathResponseBlock),
    /// First data-phase packet number.
    FirstPacketNumber(FirstPacketNumberBlock),
    /// Congestion information.
    Congestion(CongestionBlock),
    /// Padding length only.
    Padding {
        /// Number of authenticated padding bytes.
        length: usize,
    },
    /// Unknown/reserved blocks are authenticated and skipped as
    /// bounded padding.
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
            Self::FirstFragment(value) => {
                formatter.debug_tuple("FirstFragment").field(value).finish()
            }
            Self::FollowOnFragment(value) => formatter
                .debug_tuple("FollowOnFragment")
                .field(value)
                .finish(),
            Self::Termination(value) => formatter.debug_tuple("Termination").field(value).finish(),
            Self::RelayRequest(value) => {
                formatter.debug_tuple("RelayRequest").field(value).finish()
            }
            Self::RelayResponse(value) => {
                formatter.debug_tuple("RelayResponse").field(value).finish()
            }
            Self::RelayIntro(value) => formatter.debug_tuple("RelayIntro").field(value).finish(),
            Self::PeerTest(value) => formatter.debug_tuple("PeerTest").field(value).finish(),
            Self::Ack(value) => formatter.debug_tuple("Ack").field(value).finish(),
            Self::Address(value) => formatter.debug_tuple("Address").field(value).finish(),
            Self::RelayTagRequest => formatter.debug_tuple("RelayTagRequest").finish(),
            Self::RelayTag(value) => formatter.debug_tuple("RelayTag").field(value).finish(),
            Self::NewToken(value) => formatter.debug_tuple("NewToken").field(value).finish(),
            Self::PathChallenge(value) => {
                formatter.debug_tuple("PathChallenge").field(value).finish()
            }
            Self::PathResponse(value) => {
                formatter.debug_tuple("PathResponse").field(value).finish()
            }
            Self::FirstPacketNumber(value) => formatter
                .debug_tuple("FirstPacketNumber")
                .field(value)
                .finish(),
            Self::Congestion(value) => formatter.debug_tuple("Congestion").field(value).finish(),
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

/// Parses one authenticated plaintext block sequence.
///
/// Ordering follows the specification: Padding, if present, is the
/// last block and appears at most once; Termination, if present, is
/// the last non-padding block and appears at most once. All other
/// known blocks may repeat. The SessionConfirmed RouterInfo-first
/// rule is a handshake-payload concern for Plan 156, not enforced
/// here.
pub fn parse_blocks(input: &[u8]) -> Result<ParsedBlocks<'_>, BlockError> {
    if input.len() > max_payload_bytes() {
        return Err(BlockError::PayloadTooLarge);
    }
    let mut offset = 0;
    let mut blocks = Vec::new();
    let mut unknown_bytes: usize = 0;
    let mut padding_seen = false;
    let mut termination_seen = false;
    while offset < input.len() {
        if blocks.len() == constants::MAX_BLOCK_COUNT {
            return Err(BlockError::ExcessiveBlockCount);
        }
        let header_end = offset
            .checked_add(BLOCK_HEADER_LENGTH)
            .ok_or(BlockError::LengthExceedsPayload)?;
        if header_end > input.len() {
            return Err(BlockError::Truncated);
        }
        let block_type = input[offset];
        let length = usize::from(u16::from_be_bytes([input[offset + 1], input[offset + 2]]));
        let body_start = header_end;
        let body_end = body_start
            .checked_add(length)
            .ok_or(BlockError::LengthExceedsPayload)?;
        if body_end > input.len() {
            return Err(BlockError::Truncated);
        }
        let body = &input[body_start..body_end];
        if block_type == constants::BLOCK_PADDING {
            if padding_seen {
                return Err(BlockError::DuplicateBlock);
            }
            padding_seen = true;
        } else if padding_seen {
            return Err(BlockError::InvalidOrder);
        }
        if block_type == constants::BLOCK_TERMINATION {
            if termination_seen {
                return Err(BlockError::DuplicateBlock);
            }
            termination_seen = true;
        } else if termination_seen && block_type != constants::BLOCK_PADDING {
            return Err(BlockError::InvalidOrder);
        }
        let decoded = decode_body(block_type, body, &mut unknown_bytes)?;
        blocks.push(decoded);
        offset = body_end;
    }
    Ok(ParsedBlocks {
        blocks,
        unknown_bytes,
    })
}

fn decode_body<'a>(
    block_type: u8,
    body: &'a [u8],
    unknown_bytes: &mut usize,
) -> Result<DecodedBlock<'a>, BlockError> {
    match block_type {
        constants::BLOCK_DATETIME => {
            if body.len() != DATETIME_BODY_LENGTH {
                return Err(BlockError::InvalidLength);
            }
            Ok(DecodedBlock::Timestamp(TimestampBlock::new(
                u32::from_be_bytes(body.try_into().expect("length checked")),
            )))
        }
        constants::BLOCK_OPTIONS => Ok(DecodedBlock::Options(OptionsBlock::decode(body)?)),
        constants::BLOCK_ROUTER_INFO => {
            Ok(DecodedBlock::RouterInfo(RouterInfoBlock::decode(body)?))
        }
        constants::BLOCK_I2NP_MESSAGE => {
            if body.len() < SHORT_TRANSPORT_HEADER_SIZE || body.len() > MAX_I2NP_MESSAGE_BYTES {
                return Err(BlockError::I2npMalformed);
            }
            Ok(DecodedBlock::I2np(ReceivedI2npBlock { bytes: body }))
        }
        constants::BLOCK_FIRST_FRAGMENT => Ok(DecodedBlock::FirstFragment(
            FirstFragmentBlock::decode(body)?,
        )),
        constants::BLOCK_FOLLOW_ON_FRAGMENT => Ok(DecodedBlock::FollowOnFragment(
            FollowOnFragmentBlock::decode(body)?,
        )),
        constants::BLOCK_TERMINATION => {
            if !(TERMINATION_BODY_LENGTH
                ..=TERMINATION_BODY_LENGTH + constants::MAX_TERMINATION_ADDITIONAL_BYTES)
                .contains(&body.len())
            {
                return Err(BlockError::InvalidTermination);
            }
            Ok(DecodedBlock::Termination(TerminationBlock {
                valid_packets_received: u64::from_be_bytes(
                    body[..8].try_into().expect("length checked"),
                ),
                reason: TerminationReason::from_code(body[8]),
                additional_length: body.len() - TERMINATION_BODY_LENGTH,
            }))
        }
        constants::BLOCK_RELAY_REQUEST => {
            Ok(DecodedBlock::RelayRequest(decode_relay_request(body)?))
        }
        constants::BLOCK_RELAY_RESPONSE => {
            Ok(DecodedBlock::RelayResponse(decode_relay_response(body)?))
        }
        constants::BLOCK_RELAY_INTRO => Ok(DecodedBlock::RelayIntro(decode_relay_intro(body)?)),
        constants::BLOCK_PEER_TEST => Ok(DecodedBlock::PeerTest(decode_peer_test(body)?)),
        constants::BLOCK_NEXT_NONCE => Err(BlockError::UnsupportedBlock),
        constants::BLOCK_ACK => Ok(DecodedBlock::Ack(AckBlock::decode(body)?)),
        constants::BLOCK_ADDRESS => Ok(DecodedBlock::Address(AddressBlock::decode(body)?)),
        constants::BLOCK_RELAY_TAG_REQUEST => {
            if body.len() != RELAY_TAG_REQUEST_BODY_LENGTH {
                return Err(BlockError::InvalidLength);
            }
            Ok(DecodedBlock::RelayTagRequest)
        }
        constants::BLOCK_RELAY_TAG => {
            if body.len() != RELAY_TAG_BODY_LENGTH {
                return Err(BlockError::InvalidLength);
            }
            let tag = u32::from_be_bytes(body.try_into().expect("length checked"));
            RelayTagBlock::new(tag)
                .map(DecodedBlock::RelayTag)
                .map_err(|_| BlockError::RelayMalformed)
        }
        constants::BLOCK_NEW_TOKEN => {
            if body.len() != NEW_TOKEN_BODY_LENGTH {
                return Err(BlockError::InvalidLength);
            }
            Ok(DecodedBlock::NewToken(NewTokenBlock::new(
                u32::from_be_bytes(body[0..4].try_into().expect("length checked")),
                u64::from_be_bytes(body[4..12].try_into().expect("length checked")),
            )))
        }
        constants::BLOCK_PATH_CHALLENGE => PathChallengeBlock::new(body.to_vec())
            .map(DecodedBlock::PathChallenge)
            .map_err(|_| BlockError::PayloadTooLarge),
        constants::BLOCK_PATH_RESPONSE => PathResponseBlock::new(body.to_vec())
            .map(DecodedBlock::PathResponse)
            .map_err(|_| BlockError::PayloadTooLarge),
        constants::BLOCK_FIRST_PACKET_NUMBER => {
            if body.len() != FIRST_PACKET_NUMBER_BODY_LENGTH {
                return Err(BlockError::InvalidLength);
            }
            Ok(DecodedBlock::FirstPacketNumber(
                FirstPacketNumberBlock::new(u32::from_be_bytes(
                    body.try_into().expect("length checked"),
                )),
            ))
        }
        constants::BLOCK_CONGESTION => Ok(DecodedBlock::Congestion(CongestionBlock::decode(body)?)),
        constants::BLOCK_PADDING => Ok(DecodedBlock::Padding { length: body.len() }),
        _ => {
            *unknown_bytes = unknown_bytes
                .checked_add(body.len())
                .ok_or(BlockError::ExcessiveUnknownBytes)?;
            if *unknown_bytes > constants::MAX_UNKNOWN_BLOCK_BYTES {
                return Err(BlockError::ExcessiveUnknownBytes);
            }
            Ok(DecodedBlock::Unknown {
                block_type,
                length: body.len(),
            })
        }
    }
}

fn decode_relay_request(body: &[u8]) -> Result<RelayRequestBlock, BlockError> {
    // flag(1) + nonce(4) + tag(4) + timestamp(4) + ver(1) + asz(1) = 15
    if body.len() < 15 {
        return Err(BlockError::RelayMalformed);
    }
    if body[0] != 0 {
        return Err(BlockError::RelayMalformed);
    }
    let (endpoint, endpoint_len) =
        decode_endpoint(body[14], &body[15..]).map_err(|_| BlockError::RelayMalformed)?;
    let signature = body[15 + endpoint_len..].to_vec();
    RelayRequestBlock::new(
        u32::from_be_bytes(body[1..5].try_into().expect("length checked")),
        u32::from_be_bytes(body[5..9].try_into().expect("length checked")),
        u32::from_be_bytes(body[9..13].try_into().expect("length checked")),
        body[13],
        endpoint,
        signature,
    )
}

fn decode_relay_response(body: &[u8]) -> Result<RelayResponseBlock, BlockError> {
    // flag(1) + code(1) + nonce(4) = 6 minimum (Bob rejection adds
    // timestamp(4) + ver(1) + csz(1) = 12 total).
    if body.len() < 12 {
        return Err(BlockError::RelayMalformed);
    }
    if body[0] != 0 {
        return Err(BlockError::RelayMalformed);
    }
    let code = RelayResponseCode::from_u8(body[1]);
    let nonce = u32::from_be_bytes(body[2..6].try_into().expect("length checked"));
    let timestamp = u32::from_be_bytes(body[6..10].try_into().expect("length checked"));
    let version = body[10];
    check_version(version).map_err(|_| BlockError::RelayMalformed)?;
    let csz = body[11];
    match code {
        RelayResponseCode::Accept => {
            let (endpoint, endpoint_len) =
                decode_endpoint(csz, &body[12..]).map_err(|_| BlockError::RelayMalformed)?;
            let rest = &body[12 + endpoint_len..];
            if rest.len() < 8 + 1 {
                return Err(BlockError::RelayMalformed);
            }
            let (signature, token_bytes) = rest.split_at(rest.len() - 8);
            RelayResponseBlock::accept(
                nonce,
                timestamp,
                version,
                endpoint,
                signature.to_vec(),
                u64::from_be_bytes(token_bytes.try_into().expect("tail split")),
            )
        }
        RelayResponseCode::RejectedByBob(_) => {
            if csz != 0 || body.len() != 12 {
                return Err(BlockError::RelayMalformed);
            }
            RelayResponseBlock::reject(code, nonce, timestamp, version, None, Vec::new())
        }
        RelayResponseCode::RejectedByCharlie(_) | RelayResponseCode::RejectedOther(_) => {
            let (endpoint, signature) = if csz == 0 {
                if body.len() != 12 {
                    return Err(BlockError::RelayMalformed);
                }
                (None, Vec::new())
            } else {
                let (endpoint, endpoint_len) =
                    decode_endpoint(csz, &body[12..]).map_err(|_| BlockError::RelayMalformed)?;
                let signature = body[12 + endpoint_len..].to_vec();
                (Some(endpoint), signature)
            };
            RelayResponseBlock::reject(code, nonce, timestamp, version, endpoint, signature)
        }
    }
}

fn decode_relay_intro(body: &[u8]) -> Result<RelayIntroBlock, BlockError> {
    // flag(1) + hash(32) + nonce(4) + tag(4) + timestamp(4) + ver(1) +
    // asz(1) = 47 minimum.
    if body.len() < 47 {
        return Err(BlockError::RelayMalformed);
    }
    if body[0] != 0 {
        return Err(BlockError::RelayMalformed);
    }
    let (endpoint, endpoint_len) =
        decode_endpoint(body[46], &body[47..]).map_err(|_| BlockError::RelayMalformed)?;
    let signature = body[47 + endpoint_len..].to_vec();
    RelayIntroBlock::new(
        body[1..33].try_into().expect("length checked"),
        u32::from_be_bytes(body[33..37].try_into().expect("length checked")),
        u32::from_be_bytes(body[37..41].try_into().expect("length checked")),
        u32::from_be_bytes(body[41..45].try_into().expect("length checked")),
        body[45],
        endpoint,
        signature,
    )
}

fn decode_peer_test(body: &[u8]) -> Result<PeerTestBlock, BlockError> {
    // msg(1) + code(1) + flag(1) [+ hash(32)] + ver(1) + nonce(4) +
    // timestamp(4) + asz(1) + endpoint + signature.
    if body.len() < 3 {
        return Err(BlockError::PeerTestMalformed);
    }
    let message = body[0];
    if !(1..=7).contains(&message) {
        return Err(BlockError::PeerTestMalformed);
    }
    if body[2] != 0 {
        return Err(BlockError::PeerTestMalformed);
    }
    let (hash, mut offset) = if matches!(message, 2 | 4) {
        if body.len() < 3 + 32 {
            return Err(BlockError::PeerTestMalformed);
        }
        (Some(body[3..35].try_into().expect("length checked")), 35)
    } else {
        (None, 3)
    };
    // ver(1) + nonce(4) + timestamp(4) + asz(1) = 10 bytes minimum.
    if body.len() < offset + 10 {
        return Err(BlockError::PeerTestMalformed);
    }
    let version = body[offset];
    let nonce = u32::from_be_bytes(
        body[offset + 1..offset + 5]
            .try_into()
            .expect("length checked"),
    );
    let timestamp = u32::from_be_bytes(
        body[offset + 5..offset + 9]
            .try_into()
            .expect("length checked"),
    );
    let asz = body[offset + 9];
    offset += 10;
    let (endpoint, endpoint_len) =
        decode_endpoint(asz, &body[offset..]).map_err(|_| BlockError::PeerTestMalformed)?;
    let signature = body[offset + endpoint_len..].to_vec();
    PeerTestBlock::new(
        message, body[1], hash, version, nonce, timestamp, endpoint, signature,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_endpoint() -> Ssu2Endpoint {
        Ssu2Endpoint::new("192.0.2.1".parse().unwrap(), 12345).expect("endpoint")
    }

    fn signature(len: usize) -> Vec<u8> {
        vec![0x5a; len]
    }

    #[test]
    fn every_implemented_block_round_trips() {
        let plaintext = encode_blocks(vec![
            Block::Timestamp(TimestampBlock::new(1_700_000_000)),
            Block::Options(
                OptionsBlock::new(0x00, 0x10, 0x01, 0x80, 7, 8, 9, 10, vec![0xaa])
                    .expect("options"),
            ),
            Block::RouterInfo(RouterInfoBlock::new(0x01, vec![0xde, 0xad]).expect("routerinfo")),
            Block::I2np(
                I2npMessageBlock::from_bytes(vec![3, 0, 0, 0, 7, 0, 0, 0, 9, 0xaa]).expect("i2np"),
            ),
            Block::FirstFragment(
                FirstFragmentBlock::new(MessageType::Data, 11, 22, vec![0x01, 0x02])
                    .expect("first"),
            ),
            Block::FollowOnFragment(
                FollowOnFragmentBlock::new(3, true, 11, vec![0x03]).expect("follow-on"),
            ),
            Block::RelayRequest(
                RelayRequestBlock::new(1, 2, 3, 2, test_endpoint(), signature(64))
                    .expect("relay request"),
            ),
            Block::RelayResponse(
                RelayResponseBlock::accept(1, 2, 2, test_endpoint(), signature(64), 99)
                    .expect("relay response"),
            ),
            Block::RelayIntro(
                RelayIntroBlock::new([0x77; 32], 1, 2, 3, 2, test_endpoint(), signature(64))
                    .expect("relay intro"),
            ),
            Block::PeerTest(
                PeerTestBlock::new(
                    2,
                    0,
                    Some([0x88; 32]),
                    2,
                    1,
                    2,
                    test_endpoint(),
                    signature(64),
                )
                .expect("peer test"),
            ),
            Block::Ack(AckBlock::new(10, 2, vec![(1, 2), (2, 3)]).expect("ack")),
            Block::Address(AddressBlock::new(test_endpoint())),
            Block::RelayTagRequest,
            Block::RelayTag(RelayTagBlock::new(7).expect("relay tag")),
            Block::NewToken(NewTokenBlock::new(100, 200)),
            Block::PathChallenge(PathChallengeBlock::new(vec![0x01; 8]).expect("challenge")),
            Block::PathResponse(PathResponseBlock::new(vec![0x01; 8]).expect("response")),
            Block::FirstPacketNumber(FirstPacketNumberBlock::new(5)),
            Block::Congestion(CongestionBlock::new(0x01, vec![0x02]).expect("congestion")),
            Block::Termination(TerminationBlock::new(7, TerminationReason::Replaced)),
            Block::Padding(PaddingBlock::new(vec![0x55, 0x66]).expect("padding")),
        ])
        .expect("encode");
        let parsed = parse_blocks(&plaintext).expect("parse");
        assert_eq!(parsed.blocks().len(), 21);
        assert_eq!(parsed.unknown_bytes(), 0);
        assert!(matches!(parsed.blocks()[0], DecodedBlock::Timestamp(_)));
        assert!(matches!(
            parsed.blocks()[5],
            DecodedBlock::FollowOnFragment(_)
        ));
        assert!(matches!(parsed.blocks()[10], DecodedBlock::Ack(_)));
        assert!(matches!(parsed.blocks()[19], DecodedBlock::Termination(_)));
        assert!(matches!(
            parsed.blocks()[20],
            DecodedBlock::Padding { length: 2 }
        ));

        if let DecodedBlock::RelayResponse(response) = &parsed.blocks()[7] {
            assert_eq!(response.code(), RelayResponseCode::Accept);
            assert_eq!(response.token(), Some(99));
            assert_eq!(response.signature(), signature(64).as_slice());
        } else {
            panic!("relay response");
        }
        if let DecodedBlock::PeerTest(peer_test) = &parsed.blocks()[9] {
            assert_eq!(peer_test.message(), 2);
            assert_eq!(peer_test.router_hash(), Some(&[0x88; 32]));
        } else {
            panic!("peer test");
        }
    }

    #[test]
    fn relay_response_rejections_round_trip() {
        let bob = encode_blocks(vec![Block::RelayResponse(
            RelayResponseBlock::reject(RelayResponseCode::from_u8(4), 1, 2, 2, None, Vec::new())
                .expect("bob reject"),
        )])
        .expect("encode");
        let parsed = parse_blocks(&bob).expect("parse");
        if let DecodedBlock::RelayResponse(response) = &parsed.blocks()[0] {
            assert_eq!(response.code(), RelayResponseCode::RejectedByBob(4));
            assert_eq!(response.token(), None);
            assert!(response.endpoint().is_none());
        } else {
            panic!("bob rejection");
        }

        let charlie = encode_blocks(vec![Block::RelayResponse(
            RelayResponseBlock::reject(
                RelayResponseCode::from_u8(67),
                1,
                2,
                2,
                Some(test_endpoint()),
                signature(64),
            )
            .expect("charlie reject"),
        )])
        .expect("encode");
        let parsed = parse_blocks(&charlie).expect("parse");
        if let DecodedBlock::RelayResponse(response) = &parsed.blocks()[0] {
            assert_eq!(response.code(), RelayResponseCode::RejectedByCharlie(67));
            assert!(response.endpoint().is_some());
            assert_eq!(response.token(), None);
        } else {
            panic!("charlie rejection");
        }
    }

    #[test]
    fn truncation_at_every_boundary_is_rejected_without_panics() {
        let plaintext = encode_blocks(vec![
            Block::Timestamp(TimestampBlock::new(9)),
            Block::Ack(AckBlock::new(10, 0, Vec::new()).expect("ack")),
            Block::Address(AddressBlock::new(test_endpoint())),
            Block::NewToken(NewTokenBlock::new(1, 2)),
            Block::Padding(PaddingBlock::new(vec![0x55]).expect("padding")),
        ])
        .expect("encode");
        // Block boundaries: 7, 15, 24, 39, 43. Prefixes ending on a
        // boundary (including the empty prefix) parse; all other
        // strict prefixes must fail without panicking.
        for end in 0..plaintext.len() {
            let result = parse_blocks(&plaintext[..end]);
            if [0, 7, 15, 24, 39].contains(&end) {
                assert!(result.is_ok(), "prefix length {end}");
            } else {
                assert!(
                    matches!(
                        result,
                        Err(BlockError::Truncated | BlockError::LengthExceedsPayload)
                    ),
                    "prefix length {end}"
                );
            }
        }
        assert_eq!(parse_blocks(&plaintext).expect("full").blocks().len(), 5);
    }

    #[test]
    fn malformed_order_duplicates_counts_and_oversize_are_rejected() {
        let duplicate_padding = [
            constants::BLOCK_PADDING,
            0,
            0,
            constants::BLOCK_PADDING,
            0,
            0,
        ];
        assert!(matches!(
            parse_blocks(&duplicate_padding),
            Err(BlockError::DuplicateBlock)
        ));
        let after_padding = [
            constants::BLOCK_PADDING,
            0,
            0,
            constants::BLOCK_DATETIME,
            0,
            4,
            0,
            0,
            0,
            1,
        ];
        assert!(matches!(
            parse_blocks(&after_padding),
            Err(BlockError::InvalidOrder)
        ));

        let mut too_many = Vec::new();
        for _ in 0..=constants::MAX_BLOCK_COUNT {
            too_many.push(Block::Timestamp(TimestampBlock::new(1)));
        }
        assert!(matches!(
            encode_blocks(too_many),
            Err(BlockError::ExcessiveBlockCount)
        ));

        // Unknown-block byte ceiling: three 350-byte unknown blocks
        // total 1050 unknown bytes over the 1024 budget while the
        // 1059-byte payload stays under the datagram-scale cap.
        let mut over_budget = Vec::new();
        for block_type in [200_u8, 201, 202] {
            over_budget.extend_from_slice(&[block_type, 0x01, 0x5e]);
            over_budget.extend_from_slice(&[0xab; 350]);
        }
        assert!(matches!(
            parse_blocks(&over_budget),
            Err(BlockError::ExcessiveUnknownBytes)
        ));

        // Over-limit RouterInfo is rejected.
        let huge = vec![0x77; constants::MAX_ROUTER_INFO_BLOCK_BYTES + 1];
        assert!(matches!(
            RouterInfoBlock::new(0, huge),
            Err(BlockError::RouterInfoMalformed)
        ));
        // Fragmented frag byte is rejected (SSU2 RI blocks never fragment).
        assert!(matches!(
            RouterInfoBlock::decode(&[0, 0x02, 0xaa]),
            Err(BlockError::RouterInfoMalformed)
        ));
        // Reserved flag bits are rejected.
        assert!(matches!(
            RouterInfoBlock::decode(&[0x04, 0x01, 0xaa]),
            Err(BlockError::RouterInfoMalformed)
        ));
        // Follow-on fragment number 0 is rejected.
        assert!(matches!(
            FollowOnFragmentBlock::decode(&[0x00, 0, 0, 0, 1, 0xaa]),
            Err(BlockError::FragmentMalformed)
        ));
        // Empty first-fragment partial is rejected.
        assert!(matches!(
            FirstFragmentBlock::decode(&[3, 0, 0, 0, 1, 0, 0, 0, 2]),
            Err(BlockError::FragmentMalformed)
        ));
        // Degenerate ACK range is rejected.
        assert!(matches!(
            AckBlock::decode(&[0, 0, 0, 10, 0, 0, 0]),
            Err(BlockError::AckMalformed)
        ));
        // Zero relay tag is rejected.
        assert!(matches!(
            RelayTagBlock::new(0),
            Err(BlockError::RelayMalformed)
        ));
        // NextNonce is distinctly unsupported, not unknown-skipped.
        assert!(matches!(
            parse_blocks(&[constants::BLOCK_NEXT_NONCE, 0, 0]),
            Err(BlockError::UnsupportedBlock)
        ));
    }

    #[test]
    fn unknown_and_reserved_blocks_are_bounded_and_skipped() {
        let payload = [
            200, 0, 2, 0xaa, 0xbb, // experimental-range unknown
            14, 0, 1, 0xcc, // reserved 14
            255, 0, 1, 0xdd, // reserved future
        ];
        let parsed = parse_blocks(&payload).expect("unknown blocks");
        assert_eq!(parsed.blocks().len(), 3);
        assert_eq!(parsed.unknown_bytes(), 4);
    }

    #[test]
    fn committed_fixtures_are_consumed() {
        let positive = fixture_bytes(include_str!(
            "../../../tests/fixtures/ssu2/blocks-positive.hex"
        ));
        let parsed = parse_blocks(&positive).expect("positive fixture");
        assert!(!parsed.blocks().is_empty());
        let malformed = fixture_bytes(include_str!(
            "../../../tests/fixtures/ssu2/blocks-malformed.hex"
        ));
        assert!(matches!(
            parse_blocks(&malformed),
            Err(BlockError::DuplicateBlock)
        ));
    }

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
}
