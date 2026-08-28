//! Runtime-neutral SAM 3.1 `STREAM FORWARD` request validation.
//!
//! The daemon owns the TCP connection and cancellation. This module owns
//! only bounded parsing and the M7 loopback-target policy; it deliberately
//! never invokes a resolver or opens a socket.

use std::net::IpAddr;

use super::command::{Command, Silently};
use super::{MAX_SAM_OPTION_VALUE_BYTES, MAX_SAM_SESSION_ID_BYTES};

/// A normalized loopback-only forwarding host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForwardHost {
    /// An IPv4 loopback address, including all of `127.0.0.0/8`.
    V4([u8; 4]),
    /// The IPv6 loopback address.
    V6,
}

impl ForwardHost {
    /// Returns the normalized IP address.
    pub fn ip(self) -> IpAddr {
        match self {
            Self::V4(bytes) => IpAddr::V4(std::net::Ipv4Addr::from(bytes)),
            Self::V6 => IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        }
    }
}

/// A validated `STREAM FORWARD` request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamForwardRequest {
    /// Session whose destination receives inbound Streaming SYNs.
    pub session_id: String,
    /// Local TCP target port.
    pub port: u16,
    /// Explicit host, or `None` to use the forwarding socket peer IP.
    pub host: Option<String>,
    /// Whether SAM metadata must be omitted from the target connection.
    pub silent: bool,
}

/// Typed `STREAM FORWARD` validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamForwardError {
    /// `ID=` was absent.
    MissingId,
    /// `PORT=` was absent.
    MissingPort,
    /// The session ID is outside the SAM bound.
    InvalidId,
    /// The port was not a decimal value in `1..=65535`.
    InvalidPort(String),
    /// `SILENT=` was not a SAM boolean.
    InvalidSilent(String),
    /// `HOST=` is too large or not a loopback literal/localhost.
    InvalidHost(String),
    /// `SSL=true` is a SAM 3.2 feature and is rejected by M7.
    UnsupportedSsl,
    /// A pending ACCEPT or another FORWARD already owns this session's
    /// inbound surface.
    InboundModeConflict,
    /// The daemon could not update its bounded forward registry.
    RegistryUnavailable,
}

impl core::fmt::Display for StreamForwardError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingId => formatter.write_str("STREAM FORWARD missing ID"),
            Self::MissingPort => formatter.write_str("STREAM FORWARD missing PORT"),
            Self::InvalidId => formatter.write_str("STREAM FORWARD invalid ID"),
            Self::InvalidPort(value) => write!(formatter, "STREAM FORWARD invalid PORT={value}"),
            Self::InvalidSilent(value) => {
                write!(formatter, "STREAM FORWARD invalid SILENT={value}")
            }
            Self::InvalidHost(value) => write!(formatter, "STREAM FORWARD invalid HOST={value}"),
            Self::UnsupportedSsl => {
                formatter.write_str("STREAM FORWARD SSL is unsupported in SAM 3.1")
            }
            Self::InboundModeConflict => {
                formatter.write_str("STREAM FORWARD conflicts with the session inbound mode")
            }
            Self::RegistryUnavailable => formatter.write_str("STREAM FORWARD registry unavailable"),
        }
    }
}

/// Extracts a validated forward request from a parsed command.
pub fn parse_stream_forward(command: &Command) -> Result<StreamForwardRequest, StreamForwardError> {
    let session_id = command
        .value("ID")
        .ok_or(StreamForwardError::MissingId)?
        .to_owned();
    if session_id.is_empty() || session_id.len() > MAX_SAM_SESSION_ID_BYTES {
        return Err(StreamForwardError::InvalidId);
    }
    let port = command
        .value("PORT")
        .ok_or(StreamForwardError::MissingPort)?;
    let port = port
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| StreamForwardError::InvalidPort(port.to_owned()))?;
    let host = command.value("HOST").map(str::to_owned);
    if let Some(host) = &host
        && (host.is_empty() || host.len() > MAX_SAM_OPTION_VALUE_BYTES)
    {
        return Err(StreamForwardError::InvalidHost(host.clone()));
    }
    let silent = match command.value("SILENT") {
        None => false,
        Some(value) => match Silently::parse(value) {
            Some(Silently::Yes) => true,
            Some(Silently::No) => false,
            None => return Err(StreamForwardError::InvalidSilent(value.to_owned())),
        },
    };
    if command.value("SSL").is_some() {
        return Err(StreamForwardError::UnsupportedSsl);
    }
    Ok(StreamForwardRequest {
        session_id,
        port,
        host,
        silent,
    })
}

/// Applies the M7 loopback-only target policy without system DNS.
pub fn normalize_forward_host(
    explicit_host: Option<&str>,
    peer_ip: IpAddr,
) -> Result<ForwardHost, StreamForwardError> {
    let Some(host) = explicit_host else {
        return match peer_ip {
            IpAddr::V4(address) if address.is_loopback() => Ok(ForwardHost::V4(address.octets())),
            IpAddr::V6(address) if address.is_loopback() => Ok(ForwardHost::V6),
            _ => Err(StreamForwardError::InvalidHost(
                "peer is not loopback".to_owned(),
            )),
        };
    };
    if host.eq_ignore_ascii_case("localhost") {
        return Ok(ForwardHost::V4([127, 0, 0, 1]));
    }
    let address = host
        .parse::<IpAddr>()
        .map_err(|_| StreamForwardError::InvalidHost(host.to_owned()))?;
    match address {
        IpAddr::V4(address) if address.is_loopback() => Ok(ForwardHost::V4(address.octets())),
        IpAddr::V6(address) if address.is_loopback() => Ok(ForwardHost::V6),
        _ => Err(StreamForwardError::InvalidHost(host.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sam::parser::parse_line;

    #[test]
    fn forward_requires_port_and_rejects_ssl() {
        let command = parse_line("STREAM FORWARD ID=a HOST=127.0.0.1").unwrap();
        assert_eq!(
            parse_stream_forward(command.command().unwrap()),
            Err(StreamForwardError::MissingPort)
        );
        let command = parse_line("STREAM FORWARD ID=a PORT=1 SSL=true").unwrap();
        assert_eq!(
            command,
            crate::sam::command::CommandOutcome::Unsupported(crate::sam::command::Unsupported {
                kind: crate::sam::command::CommandKind::StreamForward,
                reason: crate::sam::command::UnsupportedReason::StreamForwardSsl,
            })
        );
    }

    #[test]
    fn forward_host_policy_never_resolves_non_loopback_names() {
        assert_eq!(
            normalize_forward_host(Some("localhost"), "127.0.0.1".parse().unwrap()),
            Ok(ForwardHost::V4([127, 0, 0, 1]))
        );
        assert!(normalize_forward_host(Some("example.com"), "127.0.0.1".parse().unwrap()).is_err());
        assert!(normalize_forward_host(None, "192.0.2.1".parse().unwrap()).is_err());
    }
}
