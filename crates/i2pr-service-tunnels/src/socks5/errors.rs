//! Plan 177 typed SOCKS5 errors.
//!
//! All errors are structural and carry no socket handles, no private
//! application bytes, and no hostnames. They are emitted as typed
//! failure modes for the daemon to map to bounded SOCKS5 replies
//! (see [`crate::socks5::reply::build_reply`]).
//!
//! Every typed error is also a [`Socks5ReplyCode`]; the reply
//! generator never invents a code outside this set.

#![forbid(unsafe_code)]

use thiserror::Error;

/// Plan 177 §6 reply-code mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Socks5ReplyCode {
    /// 0x00 success (only emitted after a real Streaming
    /// establishment).
    Success,
    /// 0x01 general SOCKS failure (bounded generic failure).
    GeneralFailure,
    /// 0x02 connection not allowed by ruleset (non-I2P / clearnet /
    /// local target rejected).
    ConnectionNotAllowed,
    /// 0x04 host unreachable (no LeaseSet / no alias / unresolvable
    /// `.i2p` name).
    HostUnreachable,
    /// 0x05 connection refused (Streaming establishment refused).
    ConnectionRefused,
    /// 0x06 TTL expired / connect deadline exceeded.
    TtlExpired,
    /// 0x07 command not supported (BIND, UDP ASSOCIATE, unknown).
    CommandNotSupported,
    /// 0x08 address type not supported (IPv4, IPv6).
    AddressTypeNotSupported,
}

impl Socks5ReplyCode {
    /// Returns the RFC 1928 reply code byte.
    pub const fn code(self) -> u8 {
        match self {
            Self::Success => 0x00,
            Self::GeneralFailure => 0x01,
            Self::ConnectionNotAllowed => 0x02,
            Self::HostUnreachable => 0x04,
            Self::ConnectionRefused => 0x05,
            Self::TtlExpired => 0x06,
            Self::CommandNotSupported => 0x07,
            Self::AddressTypeNotSupported => 0x08,
        }
    }

    /// Returns the conventional short reason string used by
    /// diagnostics. Never echoed into generated reply bytes.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::GeneralFailure => "general failure",
            Self::ConnectionNotAllowed => "connection not allowed",
            Self::HostUnreachable => "host unreachable",
            Self::ConnectionRefused => "connection refused",
            Self::TtlExpired => "ttl expired",
            Self::CommandNotSupported => "command not supported",
            Self::AddressTypeNotSupported => "address type not supported",
        }
    }
}

/// One typed SOCKS5 failure category. Used by the reply generator
/// to choose the reply code.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Socks5ErrorKind {
    /// Wrong greeting SOCKS version byte.
    WrongVersion,
    /// `NMETHODS == 0` or method list is empty.
    ZeroMethods,
    /// Method list exceeds the configured ceiling.
    TooManyMethods,
    /// Retained greeting bytes exceed the configured ceiling.
    GreetingCeiling,
    /// Method `0x00 NO AUTHENTICATION REQUIRED` is not offered.
    NoAcceptableMethod,
    /// Wrong request SOCKS version byte.
    WrongRequestVersion,
    /// Reserved request byte is not `0x00`.
    BadReserved,
    /// Command is not `0x01 CONNECT`.
    UnsupportedCommand,
    /// Address type is not `0x03 DOMAINNAME`.
    UnsupportedAddressType,
    /// Domain length is zero.
    ZeroDomain,
    /// Domain length exceeds the configured ceiling.
    DomainCeiling,
    /// Domain contains NUL/control/whitespace bytes.
    MalformedDomain,
    /// Domain fails `.i2p` policy (clearnet, IP literal, localhost,
    /// mixed-suffix, malformed alias, etc).
    NonI2pTarget,
    /// Domain failed strict static-alias grammar.
    MalformedAlias,
    /// Port is zero.
    ZeroPort,
    /// Request header bytes exceed the configured ceiling.
    RequestCeiling,
    /// Total retained buffer exceeds the configured ceiling.
    BufferCeilingExceeded,
    /// Limits were misconfigured.
    InvalidLimits,
    /// Streaming connect, lookup, or remote failure.
    ConnectFailure,
}

impl Socks5ErrorKind {
    /// Maps a [`Socks5ErrorKind`] to the SOCKS5 reply code it
    /// surfaces when used as the daemon terminal reply.
    pub fn reply_code(self) -> Socks5ReplyCode {
        match self {
            Socks5ErrorKind::WrongVersion
            | Socks5ErrorKind::ZeroMethods
            | Socks5ErrorKind::TooManyMethods
            | Socks5ErrorKind::GreetingCeiling
            | Socks5ErrorKind::NoAcceptableMethod
            | Socks5ErrorKind::WrongRequestVersion
            | Socks5ErrorKind::BadReserved
            | Socks5ErrorKind::MalformedDomain
            | Socks5ErrorKind::RequestCeiling
            | Socks5ErrorKind::BufferCeilingExceeded
            | Socks5ErrorKind::InvalidLimits => Socks5ReplyCode::GeneralFailure,
            Socks5ErrorKind::UnsupportedCommand => Socks5ReplyCode::CommandNotSupported,
            Socks5ErrorKind::UnsupportedAddressType => Socks5ReplyCode::AddressTypeNotSupported,
            Socks5ErrorKind::ZeroDomain => Socks5ReplyCode::GeneralFailure,
            Socks5ErrorKind::DomainCeiling => Socks5ReplyCode::GeneralFailure,
            // Malformed alias and non-I2P target are both
            // `.i2p`-only policy rejections under RFC 1928 §6
            // `connection not allowed by ruleset`.
            Socks5ErrorKind::MalformedAlias | Socks5ErrorKind::NonI2pTarget => {
                Socks5ReplyCode::ConnectionNotAllowed
            }
            Socks5ErrorKind::ZeroPort => Socks5ReplyCode::GeneralFailure,
            Socks5ErrorKind::ConnectFailure => Socks5ReplyCode::ConnectionRefused,
        }
    }
}

/// Typed SOCKS5 error carrying a kind plus a machine-readable reason.
#[derive(Clone, Debug, Eq, PartialEq, Error)]
#[error("socks5: {kind:?}: {reason}")]
pub struct Socks5Error {
    /// Failure category.
    pub kind: Socks5ErrorKind,
    /// Short machine-readable reason.
    pub reason: &'static str,
}

impl Socks5Error {
    /// Builds one error with a static reason string.
    pub fn new(kind: Socks5ErrorKind, reason: &'static str) -> Self {
        Self { kind, reason }
    }

    /// Returns the SOCKS5 reply code that should be emitted for this
    /// error.
    pub fn reply_code(&self) -> Socks5ReplyCode {
        self.kind.reply_code()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_codes_match_plan_177_section_6() {
        assert_eq!(Socks5ReplyCode::Success.code(), 0x00);
        assert_eq!(Socks5ReplyCode::GeneralFailure.code(), 0x01);
        assert_eq!(Socks5ReplyCode::ConnectionNotAllowed.code(), 0x02);
        assert_eq!(Socks5ReplyCode::HostUnreachable.code(), 0x04);
        assert_eq!(Socks5ReplyCode::ConnectionRefused.code(), 0x05);
        assert_eq!(Socks5ReplyCode::TtlExpired.code(), 0x06);
        assert_eq!(Socks5ReplyCode::CommandNotSupported.code(), 0x07);
        assert_eq!(Socks5ReplyCode::AddressTypeNotSupported.code(), 0x08);
    }

    #[test]
    fn error_kinds_map_to_reply_codes() {
        assert_eq!(
            Socks5ErrorKind::NonI2pTarget.reply_code(),
            Socks5ReplyCode::ConnectionNotAllowed
        );
        assert_eq!(
            Socks5ErrorKind::UnsupportedCommand.reply_code(),
            Socks5ReplyCode::CommandNotSupported
        );
        assert_eq!(
            Socks5ErrorKind::UnsupportedAddressType.reply_code(),
            Socks5ReplyCode::AddressTypeNotSupported
        );
        assert_eq!(
            Socks5ErrorKind::ConnectFailure.reply_code(),
            Socks5ReplyCode::ConnectionRefused
        );
        assert_eq!(
            Socks5ErrorKind::WrongVersion.reply_code(),
            Socks5ReplyCode::GeneralFailure
        );
    }
}
