//! I2NP identifiers and standard/short headers.

use super::*;

/// Message identifiers assigned by the I2NP specification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MessageType {
    /// DatabaseStoreMessage.
    DatabaseStore,
    /// DatabaseLookupMessage.
    DatabaseLookup,
    /// DatabaseSearchReplyMessage.
    DatabaseSearchReply,
    /// DeliveryStatusMessage.
    DeliveryStatus,
    /// GarlicMessage; cryptographic processing is deferred.
    Garlic,
    /// TunnelDataMessage.
    TunnelData,
    /// TunnelGatewayMessage.
    TunnelGateway,
    /// DataMessage; end-to-end interpretation is deferred.
    Data,
    /// Deprecated fixed tunnel-build message.
    TunnelBuild,
    /// Deprecated fixed tunnel-build reply message.
    TunnelBuildReply,
    /// Variable tunnel-build message.
    VariableTunnelBuild,
    /// Variable tunnel-build reply message.
    VariableTunnelBuildReply,
    /// Short ECIES tunnel-build message.
    ShortTunnelBuild,
    /// Short ECIES tunnel-build reply message.
    OutboundTunnelBuildReply,
    /// An identifier not assigned by the pinned specification.
    Unknown(u8),
}

impl MessageType {
    /// Maps a wire identifier without silently selecting a default type.
    pub const fn from_code(code: u8) -> Self {
        match code {
            1 => Self::DatabaseStore,
            2 => Self::DatabaseLookup,
            3 => Self::DatabaseSearchReply,
            10 => Self::DeliveryStatus,
            11 => Self::Garlic,
            18 => Self::TunnelData,
            19 => Self::TunnelGateway,
            20 => Self::Data,
            21 => Self::TunnelBuild,
            22 => Self::TunnelBuildReply,
            23 => Self::VariableTunnelBuild,
            24 => Self::VariableTunnelBuildReply,
            25 => Self::ShortTunnelBuild,
            26 => Self::OutboundTunnelBuildReply,
            other => Self::Unknown(other),
        }
    }

    /// Returns the numeric wire identifier.
    pub const fn code(self) -> u8 {
        match self {
            Self::DatabaseStore => 1,
            Self::DatabaseLookup => 2,
            Self::DatabaseSearchReply => 3,
            Self::DeliveryStatus => 10,
            Self::Garlic => 11,
            Self::TunnelData => 18,
            Self::TunnelGateway => 19,
            Self::Data => 20,
            Self::TunnelBuild => 21,
            Self::TunnelBuildReply => 22,
            Self::VariableTunnelBuild => 23,
            Self::VariableTunnelBuildReply => 24,
            Self::ShortTunnelBuild => 25,
            Self::OutboundTunnelBuildReply => 26,
            Self::Unknown(code) => code,
        }
    }

    pub(super) const fn supported(self) -> bool {
        !matches!(self, Self::Unknown(_))
    }
}

/// The header variant used to carry an I2NP message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum I2npHeader {
    /// The 16-byte header with millisecond expiration, length, and checksum.
    Standard {
        /// Message type.
        message_type: MessageType,
        /// Message identifier.
        message_id: u32,
        /// Millisecond expiration value; freshness is deferred.
        expiration: Date,
    },
    /// The obsolete five-byte SSU short header.
    ShortSsu {
        /// Message type.
        message_type: MessageType,
        /// Seconds-since-epoch expiration; freshness is deferred.
        expiration_seconds: u32,
    },
    /// The nine-byte NTCP2/SSU2 short header.
    ShortTransport {
        /// Message type.
        message_type: MessageType,
        /// Message identifier.
        message_id: u32,
        /// Seconds-since-epoch expiration; freshness is deferred.
        expiration_seconds: u32,
    },
}

impl I2npHeader {
    /// Returns the message type carried by this header.
    pub const fn message_type(self) -> MessageType {
        match self {
            Self::Standard { message_type, .. }
            | Self::ShortSsu { message_type, .. }
            | Self::ShortTransport { message_type, .. } => message_type,
        }
    }

    /// Returns the message identifier where this header variant carries one.
    pub const fn message_id(self) -> Option<u32> {
        match self {
            Self::Standard { message_id, .. } | Self::ShortTransport { message_id, .. } => {
                Some(message_id)
            }
            Self::ShortSsu { .. } => None,
        }
    }
}
