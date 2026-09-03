//! Strict SSU2 v2 packet-header codecs (structural only).
//!
//! These types encode/decode the long (32-byte) and short (16-byte)
//! header layouts before header protection is applied. Cryptographic
//! header protection belongs to Plan 156; this module performs no
//! ChaCha20 masking and no AEAD. Parsing requires exact sizes,
//! validates version/network/type fields with typed errors, and
//! exposes connection IDs and packet numbers only after structural
//! validation.
//!
//! Normative traceability: SSU2 specification §Packet Header (long
//! vs short forms), §Header Validation (version 2, network ID 2),
//! §Connection ID Numbering (source and destination IDs must differ).

use std::fmt;

use thiserror::Error;

use crate::constants;

/// Typed failures from SSU2 header decoding and construction.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum HeaderError {
    /// Fewer bytes than the header form requires.
    #[error("truncated SSU2 packet header")]
    Truncated,
    /// Trailing bytes followed an exact-sized header.
    #[error("trailing bytes after SSU2 packet header")]
    TrailingBytes,
    /// The message type byte is not an assigned SSU2 type.
    #[error("unknown SSU2 message type")]
    UnknownMessageType,
    /// The header form does not match the message type's defined form
    /// (long vs short).
    #[error("SSU2 message type does not use this header form")]
    WrongHeaderForm,
    /// The version field is not the supported v2 value.
    #[error("unsupported SSU2 protocol version")]
    UnsupportedVersion,
    /// The network-ID field does not match the production network.
    #[error("unexpected SSU2 network ID")]
    InvalidNetworkId,
    /// A reserved flags field was nonzero.
    #[error("nonzero SSU2 reserved header flags")]
    InvalidFlags,
    /// Source and destination connection IDs were identical.
    #[error("SSU2 source and destination connection IDs must differ")]
    IdenticalConnectionIds,
    /// A SessionConfirmed header carried a nonzero packet number.
    #[error("SSU2 SessionConfirmed packet number must be zero")]
    InvalidPacketNumber,
    /// A SessionConfirmed frag byte was structurally invalid.
    #[error("malformed SSU2 SessionConfirmed fragment info")]
    InvalidFragmentInfo,
}

/// SSU2 message types assigned by the specification.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MessageType {
    /// Type 0: SessionRequest (long header).
    SessionRequest,
    /// Type 1: SessionCreated (long header).
    SessionCreated,
    /// Type 2: SessionConfirmed (short header).
    SessionConfirmed,
    /// Type 6: Data (short header).
    Data,
    /// Type 7: PeerTest (long header).
    PeerTest,
    /// Type 9: Retry (long header).
    Retry,
    /// Type 10: TokenRequest (long header).
    TokenRequest,
    /// Type 11: HolePunch (long header).
    HolePunch,
}

impl MessageType {
    /// Converts a wire type byte to its message type.
    pub const fn from_u8(value: u8) -> Result<Self, HeaderError> {
        match value {
            constants::MESSAGE_SESSION_REQUEST => Ok(Self::SessionRequest),
            constants::MESSAGE_SESSION_CREATED => Ok(Self::SessionCreated),
            constants::MESSAGE_SESSION_CONFIRMED => Ok(Self::SessionConfirmed),
            constants::MESSAGE_DATA => Ok(Self::Data),
            constants::MESSAGE_PEER_TEST => Ok(Self::PeerTest),
            constants::MESSAGE_RETRY => Ok(Self::Retry),
            constants::MESSAGE_TOKEN_REQUEST => Ok(Self::TokenRequest),
            constants::MESSAGE_HOLE_PUNCH => Ok(Self::HolePunch),
            _ => Err(HeaderError::UnknownMessageType),
        }
    }

    /// Converts the message type to its wire byte.
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::SessionRequest => constants::MESSAGE_SESSION_REQUEST,
            Self::SessionCreated => constants::MESSAGE_SESSION_CREATED,
            Self::SessionConfirmed => constants::MESSAGE_SESSION_CONFIRMED,
            Self::Data => constants::MESSAGE_DATA,
            Self::PeerTest => constants::MESSAGE_PEER_TEST,
            Self::Retry => constants::MESSAGE_RETRY,
            Self::TokenRequest => constants::MESSAGE_TOKEN_REQUEST,
            Self::HolePunch => constants::MESSAGE_HOLE_PUNCH,
        }
    }

    /// Returns whether the type uses the 32-byte long header.
    pub const fn is_long_header(self) -> bool {
        match self {
            Self::SessionRequest
            | Self::SessionCreated
            | Self::PeerTest
            | Self::Retry
            | Self::TokenRequest
            | Self::HolePunch => true,
            Self::SessionConfirmed | Self::Data => false,
        }
    }
}

impl fmt::Display for MessageType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::SessionRequest => "SessionRequest",
            Self::SessionCreated => "SessionCreated",
            Self::SessionConfirmed => "SessionConfirmed",
            Self::Data => "Data",
            Self::PeerTest => "PeerTest",
            Self::Retry => "Retry",
            Self::TokenRequest => "TokenRequest",
            Self::HolePunch => "HolePunch",
        };
        formatter.write_str(name)
    }
}

/// Which structural header form a datagram carries, determined from
/// the spec-defined message-type byte (offset 12), not heuristics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeaderForm {
    /// 32-byte long header (types 0/1/7/9/10/11).
    Long,
    /// 16-byte SessionConfirmed short header (type 2).
    SessionConfirmed,
    /// 16-byte Data short header (type 6).
    Data,
}

impl HeaderForm {
    /// Classifies the header form from the exact 13-byte common
    /// prefix (destination connection ID + packet number + type).
    pub fn classify_prefix(prefix: &[u8]) -> Result<Self, HeaderError> {
        if prefix.len() < constants::HEADER_COMMON_PREFIX_LENGTH {
            return Err(HeaderError::Truncated);
        }
        match MessageType::from_u8(prefix[12])? {
            MessageType::SessionConfirmed => Ok(Self::SessionConfirmed),
            MessageType::Data => Ok(Self::Data),
            _ => Ok(Self::Long),
        }
    }

    /// Returns the exact header length for this form.
    pub const fn header_length(self) -> usize {
        match self {
            Self::Long => constants::LONG_HEADER_LENGTH,
            Self::SessionConfirmed | Self::Data => constants::SHORT_HEADER_LENGTH,
        }
    }
}

/// A decoded 32-byte long header (pre-header-protection layout).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LongHeader {
    dst_conn_id: u64,
    packet_number: u32,
    message_type: MessageType,
    src_conn_id: u64,
    token: u64,
}

impl LongHeader {
    /// Constructs a long header, enforcing distinct connection IDs.
    /// The packet number is random and ignored by the peer for these
    /// types; it is carried opaquely.
    pub fn new(
        dst_conn_id: u64,
        packet_number: u32,
        message_type: MessageType,
        src_conn_id: u64,
        token: u64,
    ) -> Result<Self, HeaderError> {
        if !message_type.is_long_header() {
            return Err(HeaderError::WrongHeaderForm);
        }
        if src_conn_id == dst_conn_id {
            return Err(HeaderError::IdenticalConnectionIds);
        }
        Ok(Self {
            dst_conn_id,
            packet_number,
            message_type,
            src_conn_id,
            token,
        })
    }

    /// Decodes an exact 32-byte long header, rejecting trailing bytes.
    pub fn decode(input: &[u8]) -> Result<Self, HeaderError> {
        if input.len() < constants::LONG_HEADER_LENGTH {
            return Err(HeaderError::Truncated);
        }
        if input.len() > constants::LONG_HEADER_LENGTH {
            return Err(HeaderError::TrailingBytes);
        }
        let message_type = MessageType::from_u8(input[12])?;
        if !message_type.is_long_header() {
            return Err(HeaderError::WrongHeaderForm);
        }
        if input[13] != constants::SSU2_VERSION {
            return Err(HeaderError::UnsupportedVersion);
        }
        if input[14] != constants::SSU2_NETWORK_ID {
            return Err(HeaderError::InvalidNetworkId);
        }
        if input[15] != 0 {
            return Err(HeaderError::InvalidFlags);
        }
        let header = Self {
            dst_conn_id: u64::from_be_bytes(input[0..8].try_into().expect("length checked")),
            packet_number: u32::from_be_bytes(input[8..12].try_into().expect("length checked")),
            message_type,
            src_conn_id: u64::from_be_bytes(input[16..24].try_into().expect("length checked")),
            token: u64::from_be_bytes(input[24..32].try_into().expect("length checked")),
        };
        if header.src_conn_id == header.dst_conn_id {
            return Err(HeaderError::IdenticalConnectionIds);
        }
        Ok(header)
    }

    /// Encodes the exact 32-byte long header.
    pub fn encode(&self) -> [u8; constants::LONG_HEADER_LENGTH] {
        let mut output = [0_u8; constants::LONG_HEADER_LENGTH];
        output[0..8].copy_from_slice(&self.dst_conn_id.to_be_bytes());
        output[8..12].copy_from_slice(&self.packet_number.to_be_bytes());
        output[12] = self.message_type.as_u8();
        output[13] = constants::SSU2_VERSION;
        output[14] = constants::SSU2_NETWORK_ID;
        output[15] = 0;
        output[16..24].copy_from_slice(&self.src_conn_id.to_be_bytes());
        output[24..32].copy_from_slice(&self.token.to_be_bytes());
        output
    }

    /// Returns the destination connection ID.
    pub const fn dst_conn_id(self) -> u64 {
        self.dst_conn_id
    }

    /// Returns the opaque (random, ignored) packet number.
    pub const fn packet_number(self) -> u32 {
        self.packet_number
    }

    /// Returns the message type.
    pub const fn message_type(self) -> MessageType {
        self.message_type
    }

    /// Returns the source connection ID.
    pub const fn src_conn_id(self) -> u64 {
        self.src_conn_id
    }

    /// Returns the address-validation token.
    pub const fn token(self) -> u64 {
        self.token
    }
}

/// A decoded 16-byte SessionConfirmed short header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionConfirmedHeader {
    dst_conn_id: u64,
    frag_number: u8,
    frag_total: u8,
}

impl SessionConfirmedHeader {
    /// Constructs a confirmed header. Fragment numbers run 0..=14
    /// with a total of 1..=15, and the number must be below the total.
    pub const fn new(
        dst_conn_id: u64,
        frag_number: u8,
        frag_total: u8,
    ) -> Result<Self, HeaderError> {
        if frag_number > 14 || frag_total == 0 || frag_total > 15 || frag_number >= frag_total {
            return Err(HeaderError::InvalidFragmentInfo);
        }
        Ok(Self {
            dst_conn_id,
            frag_number,
            frag_total,
        })
    }

    /// Decodes an exact 16-byte SessionConfirmed header.
    pub fn decode(input: &[u8]) -> Result<Self, HeaderError> {
        if input.len() < constants::SHORT_HEADER_LENGTH {
            return Err(HeaderError::Truncated);
        }
        if input.len() > constants::SHORT_HEADER_LENGTH {
            return Err(HeaderError::TrailingBytes);
        }
        if MessageType::from_u8(input[12])? != MessageType::SessionConfirmed {
            return Err(HeaderError::WrongHeaderForm);
        }
        if u32::from_be_bytes(input[8..12].try_into().expect("length checked")) != 0 {
            return Err(HeaderError::InvalidPacketNumber);
        }
        let frag = input[13];
        let frag_number = frag >> 4;
        let frag_total = frag & 0x0f;
        if u16::from_be_bytes(input[14..16].try_into().expect("length checked")) != 0 {
            return Err(HeaderError::InvalidFlags);
        }
        Self::new(
            u64::from_be_bytes(input[0..8].try_into().expect("length checked")),
            frag_number,
            frag_total,
        )
    }

    /// Encodes the exact 16-byte SessionConfirmed header.
    pub fn encode(&self) -> [u8; constants::SHORT_HEADER_LENGTH] {
        let mut output = [0_u8; constants::SHORT_HEADER_LENGTH];
        output[0..8].copy_from_slice(&self.dst_conn_id.to_be_bytes());
        output[8..12].copy_from_slice(&0_u32.to_be_bytes());
        output[12] = constants::MESSAGE_SESSION_CONFIRMED;
        output[13] = (self.frag_number << 4) | self.frag_total;
        output[14..16].copy_from_slice(&0_u16.to_be_bytes());
        output
    }

    /// Returns the destination connection ID.
    pub const fn dst_conn_id(self) -> u64 {
        self.dst_conn_id
    }

    /// Returns the zero-based fragment number.
    pub const fn frag_number(self) -> u8 {
        self.frag_number
    }

    /// Returns the total fragment count.
    pub const fn frag_total(self) -> u8 {
        self.frag_total
    }
}

/// A decoded 16-byte Data short header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DataHeader {
    dst_conn_id: u64,
    packet_number: u32,
    immediate_ack: bool,
}

impl DataHeader {
    /// Constructs a data header with the immediate-ACK request flag.
    pub const fn new(dst_conn_id: u64, packet_number: u32, immediate_ack: bool) -> Self {
        Self {
            dst_conn_id,
            packet_number,
            immediate_ack,
        }
    }

    /// Decodes an exact 16-byte Data header.
    pub fn decode(input: &[u8]) -> Result<Self, HeaderError> {
        if input.len() < constants::SHORT_HEADER_LENGTH {
            return Err(HeaderError::Truncated);
        }
        if input.len() > constants::SHORT_HEADER_LENGTH {
            return Err(HeaderError::TrailingBytes);
        }
        if MessageType::from_u8(input[12])? != MessageType::Data {
            return Err(HeaderError::WrongHeaderForm);
        }
        if input[13] & 0xfe != 0 {
            return Err(HeaderError::InvalidFlags);
        }
        if u16::from_be_bytes(input[14..16].try_into().expect("length checked")) != 0 {
            return Err(HeaderError::InvalidFlags);
        }
        Ok(Self {
            dst_conn_id: u64::from_be_bytes(input[0..8].try_into().expect("length checked")),
            packet_number: u32::from_be_bytes(input[8..12].try_into().expect("length checked")),
            immediate_ack: input[13] & 0x01 != 0,
        })
    }

    /// Encodes the exact 16-byte Data header.
    pub fn encode(&self) -> [u8; constants::SHORT_HEADER_LENGTH] {
        let mut output = [0_u8; constants::SHORT_HEADER_LENGTH];
        output[0..8].copy_from_slice(&self.dst_conn_id.to_be_bytes());
        output[8..12].copy_from_slice(&self.packet_number.to_be_bytes());
        output[12] = constants::MESSAGE_DATA;
        output[13] = u8::from(self.immediate_ack);
        output[14..16].copy_from_slice(&0_u16.to_be_bytes());
        output
    }

    /// Returns the destination connection ID.
    pub const fn dst_conn_id(self) -> u64 {
        self.dst_conn_id
    }

    /// Returns the data-phase packet number.
    pub const fn packet_number(self) -> u32 {
        self.packet_number
    }

    /// Returns whether the sender requested an immediate ACK.
    pub const fn immediate_ack(self) -> bool {
        self.immediate_ack
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_types_round_trip_and_classify_forms() {
        for (byte, form) in [
            (0, HeaderForm::Long),
            (1, HeaderForm::Long),
            (2, HeaderForm::SessionConfirmed),
            (6, HeaderForm::Data),
            (7, HeaderForm::Long),
            (9, HeaderForm::Long),
            (10, HeaderForm::Long),
            (11, HeaderForm::Long),
        ] {
            let message_type = MessageType::from_u8(byte).expect("assigned type");
            assert_eq!(message_type.as_u8(), byte);
            let mut prefix = [0_u8; 13];
            prefix[12] = byte;
            assert_eq!(HeaderForm::classify_prefix(&prefix), Ok(form));
        }
        for unassigned in [3, 4, 5, 8, 12, 255] {
            assert_eq!(
                MessageType::from_u8(unassigned),
                Err(HeaderError::UnknownMessageType),
                "type {unassigned}"
            );
        }
        assert_eq!(
            HeaderForm::classify_prefix(&[0_u8; 5]),
            Err(HeaderError::Truncated)
        );
    }

    #[test]
    fn long_header_round_trip_is_exact() {
        let header = LongHeader::new(
            0x0102_0304_0506_0708,
            0xdead_beef,
            MessageType::Retry,
            0x1112_1314_1516_1718,
            42,
        )
        .expect("header");
        let encoded = header.encode();
        assert_eq!(encoded.len(), 32);
        assert_eq!(LongHeader::decode(&encoded), Ok(header));
        assert_eq!(
            LongHeader::decode(&encoded[..31]),
            Err(HeaderError::Truncated)
        );
        let mut trailing = encoded.to_vec();
        trailing.push(0);
        assert_eq!(
            LongHeader::decode(&trailing),
            Err(HeaderError::TrailingBytes)
        );
    }

    #[test]
    fn long_header_rejects_bad_version_network_flags_and_ids() {
        let header = LongHeader::new(1, 2, MessageType::SessionRequest, 3, 0).expect("header");
        let mut encoded = header.encode();

        encoded[13] = 3;
        assert_eq!(
            LongHeader::decode(&encoded),
            Err(HeaderError::UnsupportedVersion)
        );
        encoded[13] = 2;
        encoded[14] = 7;
        assert_eq!(
            LongHeader::decode(&encoded),
            Err(HeaderError::InvalidNetworkId)
        );
        encoded[14] = 2;
        encoded[15] = 1;
        assert_eq!(LongHeader::decode(&encoded), Err(HeaderError::InvalidFlags));
        encoded[15] = 0;
        encoded[16..24].copy_from_slice(&1_u64.to_be_bytes());
        assert_eq!(
            LongHeader::decode(&encoded),
            Err(HeaderError::IdenticalConnectionIds)
        );
        assert_eq!(
            LongHeader::new(9, 0, MessageType::Data, 10, 0),
            Err(HeaderError::WrongHeaderForm)
        );
        assert_eq!(
            LongHeader::new(5, 0, MessageType::TokenRequest, 5, 0),
            Err(HeaderError::IdenticalConnectionIds)
        );
    }

    #[test]
    fn session_confirmed_header_enforces_zero_packet_and_frag_shape() {
        let header = SessionConfirmedHeader::new(0x0102_0304_0506_0708, 0, 1).expect("header");
        let encoded = header.encode();
        assert_eq!(encoded.len(), 16);
        assert_eq!(SessionConfirmedHeader::decode(&encoded), Ok(header));

        let mut nonzero_packet = encoded;
        nonzero_packet[11] = 1;
        assert_eq!(
            SessionConfirmedHeader::decode(&nonzero_packet),
            Err(HeaderError::InvalidPacketNumber)
        );
        let mut bad_flags = encoded;
        bad_flags[15] = 1;
        assert_eq!(
            SessionConfirmedHeader::decode(&bad_flags),
            Err(HeaderError::InvalidFlags)
        );
        let mut wrong_type = encoded;
        wrong_type[12] = 6;
        assert_eq!(
            SessionConfirmedHeader::decode(&wrong_type),
            Err(HeaderError::WrongHeaderForm)
        );
        for (number, total) in [(1, 1), (15, 15), (0, 0), (2, 2), (14, 14)] {
            assert_eq!(
                SessionConfirmedHeader::new(1, number, total),
                Err(HeaderError::InvalidFragmentInfo),
                "frag {number}/{total}"
            );
        }
        assert!(SessionConfirmedHeader::new(1, 14, 15).is_ok());
    }

    #[test]
    fn committed_header_fixtures_decode() {
        let long = fixture_bytes(include_str!("../../../tests/fixtures/ssu2/long-header.hex"));
        let header = LongHeader::decode(&long).expect("long fixture");
        assert_eq!(header.message_type(), MessageType::SessionRequest);
        assert_eq!(header.dst_conn_id(), 0x0102_0304_0506_0708);
        assert_eq!(header.packet_number(), 0xdead_beef);
        assert_eq!(header.token(), 42);

        let data = fixture_bytes(include_str!(
            "../../../tests/fixtures/ssu2/short-header-data.hex"
        ));
        let data = DataHeader::decode(&data).expect("data fixture");
        assert_eq!(data.packet_number(), 7);
        assert!(data.immediate_ack());

        let confirmed = fixture_bytes(include_str!(
            "../../../tests/fixtures/ssu2/short-header-confirmed.hex"
        ));
        let confirmed = SessionConfirmedHeader::decode(&confirmed).expect("confirmed fixture");
        assert_eq!(confirmed.frag_number(), 0);
        assert_eq!(confirmed.frag_total(), 1);
    }

    #[test]
    fn data_header_round_trip_and_flag_rules() {
        let header = DataHeader::new(0x0102_0304_0506_0708, 0x1234_5678, true);
        let encoded = header.encode();
        assert_eq!(encoded.len(), 16);
        let decoded = DataHeader::decode(&encoded).expect("data header");
        assert_eq!(decoded, header);
        assert!(decoded.immediate_ack());

        let plain = DataHeader::new(1, 0, false);
        assert!(
            !DataHeader::decode(&plain.encode())
                .expect("plain")
                .immediate_ack()
        );

        let mut reserved = encoded;
        reserved[13] = 0x02;
        assert_eq!(
            DataHeader::decode(&reserved),
            Err(HeaderError::InvalidFlags)
        );
        let mut moreflags = encoded;
        moreflags[14] = 0x01;
        assert_eq!(
            DataHeader::decode(&moreflags),
            Err(HeaderError::InvalidFlags)
        );
        let mut wrong_type = encoded;
        wrong_type[12] = 2;
        assert_eq!(
            DataHeader::decode(&wrong_type),
            Err(HeaderError::WrongHeaderForm)
        );
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
