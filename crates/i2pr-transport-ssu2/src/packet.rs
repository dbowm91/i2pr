//! SSU2 datagram bounds and header/payload splitting (structural only).
//!
//! Every inbound datagram is length-checked before any header field
//! is accessed, and every header is structurally validated before
//! its payload region is exposed. No cryptographic header
//! protection, AEAD, or socket I/O occurs here.
//!
//! Normative traceability: SSU2 specification §Messages (one message
//! per datagram; 40-byte minimum; 1472/1452-byte IPv4/IPv6 maxima).

use thiserror::Error;

use crate::constants;
use crate::header::{DataHeader, HeaderError, HeaderForm, LongHeader, SessionConfirmedHeader};

/// Typed failures from SSU2 datagram validation and splitting.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PacketError {
    /// The datagram is shorter than the SSU2 minimum.
    #[error("SSU2 datagram is shorter than the 40-byte minimum")]
    TooShort,
    /// The datagram exceeds the IPv6 maximum (it may still fit IPv4).
    #[error("SSU2 datagram exceeds the IPv6 maximum")]
    ExceedsIpv6Maximum,
    /// The datagram exceeds the IPv4 maximum.
    #[error("SSU2 datagram exceeds the IPv4 maximum")]
    ExceedsIpv4Maximum,
    /// A header field was structurally invalid.
    #[error("SSU2 packet header is invalid")]
    Header(#[from] HeaderError),
    /// Too few bytes remain after the header for the minimum
    /// authenticated tail (payload floor plus MAC).
    #[error("SSU2 datagram has no room for an authenticated payload")]
    PayloadTooShort,
}

/// Length classification of a raw datagram.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DatagramLengthClass {
    /// Fits both IPv4 and IPv6 maxima.
    FitsBoth,
    /// Fits the IPv4 maximum only.
    Ipv4Only,
}

impl DatagramLengthClass {
    /// Validates a raw datagram length without touching its bytes.
    pub const fn classify(length: usize) -> Result<Self, PacketError> {
        if length < constants::MIN_DATAGRAM_LENGTH {
            return Err(PacketError::TooShort);
        }
        if length > constants::MAX_DATAGRAM_IPV4_LENGTH {
            return Err(PacketError::ExceedsIpv4Maximum);
        }
        if length > constants::MAX_DATAGRAM_IPV6_LENGTH {
            return Ok(Self::Ipv4Only);
        }
        Ok(Self::FitsBoth)
    }
}

/// A structurally validated SSU2 packet header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PacketHeader {
    /// A 32-byte long header.
    Long(LongHeader),
    /// A 16-byte SessionConfirmed short header.
    SessionConfirmed(SessionConfirmedHeader),
    /// A 16-byte Data short header.
    Data(DataHeader),
}

impl PacketHeader {
    /// Returns the header form.
    pub const fn form(self) -> HeaderForm {
        match self {
            Self::Long(_) => HeaderForm::Long,
            Self::SessionConfirmed(_) => HeaderForm::SessionConfirmed,
            Self::Data(_) => HeaderForm::Data,
        }
    }

    /// Returns the exact header length.
    pub const fn header_length(self) -> usize {
        self.form().header_length()
    }

    /// Returns the destination connection ID.
    pub const fn dst_conn_id(self) -> u64 {
        match self {
            Self::Long(header) => header.dst_conn_id(),
            Self::SessionConfirmed(header) => header.dst_conn_id(),
            Self::Data(header) => header.dst_conn_id(),
        }
    }
}

/// A split datagram: validated header plus the opaque post-header
/// bytes (ephemeral key and authenticated payload for handshake
/// messages; authenticated payload for the rest).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SplitPacket<'a> {
    header: PacketHeader,
    rest: &'a [u8],
}

impl<'a> SplitPacket<'a> {
    /// Returns the validated header.
    pub const fn header(self) -> PacketHeader {
        self.header
    }

    /// Returns the opaque post-header bytes.
    pub const fn rest(self) -> &'a [u8] {
        self.rest
    }
}

/// Validates a datagram, decodes its header, and splits off the
/// post-header bytes.
///
/// SessionRequest/SessionCreated long headers are followed by the
/// 32-byte ephemeral key before the authenticated payload, so those
/// two types require 32 additional bytes beyond the minimum
/// authenticated tail.
pub fn split_packet(datagram: &[u8]) -> Result<SplitPacket<'_>, PacketError> {
    DatagramLengthClass::classify(datagram.len())?;
    let form = HeaderForm::classify_prefix(datagram)?;
    let header_length = form.header_length();
    if datagram.len() < header_length {
        return Err(PacketError::TooShort);
    }
    let (header, rest) = match form {
        HeaderForm::Long => {
            let header = LongHeader::decode(&datagram[..header_length])?;
            (PacketHeader::Long(header), &datagram[header_length..])
        }
        HeaderForm::SessionConfirmed => {
            let header = SessionConfirmedHeader::decode(&datagram[..header_length])?;
            (
                PacketHeader::SessionConfirmed(header),
                &datagram[header_length..],
            )
        }
        HeaderForm::Data => {
            let header = DataHeader::decode(&datagram[..header_length])?;
            (PacketHeader::Data(header), &datagram[header_length..])
        }
    };
    let minimum_rest = match header {
        PacketHeader::Long(header) => match header.message_type() {
            crate::header::MessageType::SessionRequest
            | crate::header::MessageType::SessionCreated => {
                constants::HANDSHAKE_EPHEMERAL_LENGTH + constants::MIN_POST_HEADER_BYTES
            }
            _ => constants::MIN_POST_HEADER_BYTES,
        },
        _ => constants::MIN_POST_HEADER_BYTES,
    };
    if rest.len() < minimum_rest {
        return Err(PacketError::PayloadTooShort);
    }
    Ok(SplitPacket { header, rest })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::MessageType;

    fn long_datagram(message_type: MessageType, total: usize) -> Vec<u8> {
        let header = LongHeader::new(1, 0xaa_bb_cc_dd, message_type, 2, 0).expect("long header");
        let mut datagram = header.encode().to_vec();
        datagram.extend(std::iter::repeat_n(0x77, total - datagram.len()));
        datagram
    }

    fn data_datagram(total: usize) -> Vec<u8> {
        let header = DataHeader::new(1, 7, false);
        let mut datagram = header.encode().to_vec();
        datagram.extend(std::iter::repeat_n(0x77, total - datagram.len()));
        datagram
    }

    #[test]
    fn datagram_lengths_classify_without_touching_bytes() {
        assert_eq!(
            DatagramLengthClass::classify(39),
            Err(PacketError::TooShort)
        );
        assert_eq!(
            DatagramLengthClass::classify(40),
            Ok(DatagramLengthClass::FitsBoth)
        );
        assert_eq!(
            DatagramLengthClass::classify(1452),
            Ok(DatagramLengthClass::FitsBoth)
        );
        assert_eq!(
            DatagramLengthClass::classify(1453),
            Ok(DatagramLengthClass::Ipv4Only)
        );
        assert_eq!(
            DatagramLengthClass::classify(1472),
            Ok(DatagramLengthClass::Ipv4Only)
        );
        assert_eq!(
            DatagramLengthClass::classify(1473),
            Err(PacketError::ExceedsIpv4Maximum)
        );
    }

    #[test]
    fn split_packet_validates_before_exposing_payload() {
        let datagram = data_datagram(40);
        let split = split_packet(&datagram).expect("data packet");
        assert_eq!(split.header().dst_conn_id(), 1);
        assert_eq!(split.rest().len(), 24);

        let handshake = long_datagram(MessageType::SessionRequest, 88);
        let split = split_packet(&handshake).expect("session request");
        assert_eq!(split.rest().len(), 56);

        // SessionRequest-shaped datagram too short for X + auth tail.
        let short = long_datagram(MessageType::SessionRequest, 87);
        assert_eq!(split_packet(&short), Err(PacketError::PayloadTooShort));
        // Retry needs only the minimum authenticated tail.
        let retry = long_datagram(MessageType::Retry, 56);
        assert!(split_packet(&retry).is_ok());
        assert_eq!(
            split_packet(&retry[..55]),
            Err(PacketError::PayloadTooShort)
        );
    }

    #[test]
    fn split_packet_rejects_short_oversize_and_bad_headers() {
        assert_eq!(split_packet(&[0_u8; 39]), Err(PacketError::TooShort));
        assert_eq!(
            split_packet(&[0_u8; 1473]).map(|_| ()),
            Err(PacketError::ExceedsIpv4Maximum)
        );
        let mut bad_version = long_datagram(MessageType::TokenRequest, 64);
        bad_version[13] = 3;
        assert_eq!(
            split_packet(&bad_version).map(|_| ()),
            Err(PacketError::Header(HeaderError::UnsupportedVersion))
        );
        let mut unknown_type = data_datagram(40);
        unknown_type[12] = 5;
        assert_eq!(
            split_packet(&unknown_type).map(|_| ()),
            Err(PacketError::Header(HeaderError::UnknownMessageType))
        );
    }
}
