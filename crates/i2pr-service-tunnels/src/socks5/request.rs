//! Plan 177 SOCKS5 CONNECT request parser and central state
//! machine.
//!
//! Parses the RFC 1928 CONNECT request (`VER + CMD + RSV + ATYP +
//! DST.ADDR + DST.PORT`) from a bounded byte buffer. Only
//! `DOMAINNAME` (0x03) is accepted; `IPv4` (0x01) and `IPv6` (0x04)
//! are rejected with explicit typed failures. CONNECT is the only
//! command accepted; BIND and UDP ASSOCIATE are rejected.
//!
//! The parser is incremental and bounded. It can receive one byte at
//! a time, or the entire request plus early tunnel bytes in one
//! read; the same-read bytes are preserved verbatim and returned
//! alongside the parsed request.
//!
//! The state machine returns the typed terminal states
//! `ReadyToConnect` (with the same-read leftover bytes) or
//! `Rejected` (with the SOCKS5 reply code that should be emitted
//! before close). Higher-level transport states (`TunnelMode`) live
//! in the daemon.

#![forbid(unsafe_code)]

use super::config::{
    DEFAULT_CONNECT_PORT, SOCKS_VERSION, SOCKS5_ATYP_DOMAIN, SOCKS5_ATYP_IPV4, SOCKS5_ATYP_IPV6,
    SOCKS5_CMD_BIND, SOCKS5_CMD_CONNECT, SOCKS5_CMD_UDP_ASSOCIATE, SOCKS5_RESERVED,
};
use super::errors::{Socks5Error, Socks5ErrorKind};
use super::limits::Socks5Limits;
use crate::destination::{B32_SUFFIX, validate_static_alias};

/// Length of the fixed request header (`VER + CMD + RSV + ATYP`).
pub const REQUEST_HEADER_LEN: usize = 4;
/// Length of the port field (big-endian u16).
pub const PORT_FIELD_LEN: usize = 2;

/// Parsed CONNECT request destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectDestination {
    /// Lower-case canonical host (e.g. `example.i2p`,
    /// `aaaa…aaaa.b32.i2p`).
    pub host: String,
    /// Destination port (RFC 1928 field is u16; M10 rejects zero).
    pub port: u16,
}

impl ConnectDestination {
    /// Returns the canonical authority string (`host:port`).
    pub fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Typed terminal outcome of a SOCKS5 CONNECT request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestOutcome {
    /// Request is fully parsed and validated against the runtime-
    /// neutral `.i2p` target policy. The daemon must open I2P
    /// Streaming and only emit the success reply once the
    /// connection reaches `Established`. The leftover bytes are
    /// the same-read application bytes that follow the end of the
    /// CONNECT request and must be the first bytes the daemon
    /// forwards into the shared tunnel pump.
    ReadyToConnect {
        /// Parsed CONNECT destination.
        destination: ConnectDestination,
        /// Same-read bytes after the end of the CONNECT request.
        /// Always empty when the parser is fed one byte at a time.
        leftover: Vec<u8>,
    },
    /// Request is structurally complete but failed target policy
    /// (clearnet, IP literal, localhost, mixed-suffix, etc).
    /// The daemon emits the corresponding SOCKS5 reply code and
    /// closes the socket without entering tunnel mode.
    Rejected {
        /// SOCKS5 reply code for the daemon to emit.
        reply_code: u8,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InnerState {
    RequestHeader,
    Domain { length: usize },
    Port,
}

/// Incremental CONNECT request parser. Does not own the greeting
/// parser; the daemon combines both.
#[derive(Clone, Debug)]
pub struct RequestParser {
    state: InnerState,
    buffer: Vec<u8>,
    domain_length: usize,
    domain: Vec<u8>,
}

impl Default for RequestParser {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestParser {
    /// Creates a fresh request parser.
    pub fn new() -> Self {
        Self {
            state: InnerState::RequestHeader,
            buffer: Vec::with_capacity(REQUEST_HEADER_LEN + PORT_FIELD_LEN),
            domain_length: 0,
            domain: Vec::new(),
        }
    }

    /// Returns the bytes retained so far.
    pub fn retained(&self) -> &[u8] {
        &self.buffer
    }

    /// Returns the partial domain accumulated so far.
    pub fn domain(&self) -> &[u8] {
        &self.domain
    }

    /// Returns the configured domain length (only meaningful once
    /// the parser has consumed the request header byte).
    pub fn domain_length(&self) -> usize {
        self.domain_length
    }

    /// Feeds bytes into the request parser. Returns:
    ///
    /// - `Ok(None)` when the request is incomplete and more bytes
    ///   are required;
    /// - `Ok(Some(RequestOutcome::ReadyToConnect | Rejected))`
    ///   when a terminal decision has been reached;
    /// - `Err(Socks5Error)` on a structural violation (NUL/control
    ///   bytes inside the domain, ceiling overflow, framing
    ///   failures, or grammar violations that cannot be mapped to
    ///   a SOCKS5 reply code).
    pub fn advance(
        &mut self,
        bytes: &[u8],
        limits: Socks5Limits,
    ) -> Result<Option<RequestOutcome>, Socks5Error> {
        let rejected = |kind, reason| Socks5Error::new(kind, reason);
        // Plan 177 §8: never allocate proportional to attacker-
        // provided counts; every read appends to the bounded buffer.
        // The request header is exactly 4 bytes (VER/CMD/RSV/ATYP),
        // so the per-state ceiling is implicit. The retained
        // buffer ceiling bounds the total of greeting + request +
        // post-request bytes retained before parsing completes.
        if self.buffer.len() + bytes.len() > limits.retained_buffer_max_bytes {
            return Err(rejected(
                Socks5ErrorKind::BufferCeilingExceeded,
                "request bytes exceed the retained buffer ceiling",
            ));
        }
        self.buffer.extend_from_slice(bytes);
        loop {
            match self.state {
                InnerState::RequestHeader => {
                    if self.buffer.len() < REQUEST_HEADER_LEN {
                        return Ok(None);
                    }
                    let ver = self.buffer[0];
                    let cmd = self.buffer[1];
                    let rsv = self.buffer[2];
                    let atyp = self.buffer[3];
                    if ver != SOCKS_VERSION {
                        return Err(rejected(
                            Socks5ErrorKind::WrongRequestVersion,
                            "request version must be 0x05",
                        ));
                    }
                    if rsv != SOCKS5_RESERVED {
                        return Err(rejected(Socks5ErrorKind::BadReserved, "RSV must be 0x00"));
                    }
                    if cmd != SOCKS5_CMD_CONNECT {
                        if cmd == SOCKS5_CMD_BIND || cmd == SOCKS5_CMD_UDP_ASSOCIATE {
                            return Ok(Some(RequestOutcome::Rejected {
                                reply_code: Socks5ErrorKind::UnsupportedCommand.reply_code().code(),
                            }));
                        }
                        return Ok(Some(RequestOutcome::Rejected {
                            reply_code: Socks5ErrorKind::UnsupportedCommand.reply_code().code(),
                        }));
                    }
                    match atyp {
                        SOCKS5_ATYP_IPV4 | SOCKS5_ATYP_IPV6 => {
                            return Ok(Some(RequestOutcome::Rejected {
                                reply_code: Socks5ErrorKind::UnsupportedAddressType
                                    .reply_code()
                                    .code(),
                            }));
                        }
                        SOCKS5_ATYP_DOMAIN => {}
                        _ => {
                            return Ok(Some(RequestOutcome::Rejected {
                                reply_code: Socks5ErrorKind::UnsupportedAddressType
                                    .reply_code()
                                    .code(),
                            }));
                        }
                    }
                    // Drain the header.
                    let mut header = [0_u8; REQUEST_HEADER_LEN];
                    header.copy_from_slice(&self.buffer[..REQUEST_HEADER_LEN]);
                    self.buffer.copy_within(REQUEST_HEADER_LEN.., 0);
                    self.buffer.truncate(self.buffer.len() - REQUEST_HEADER_LEN);
                    let _ = header; // buffer drain is sufficient
                    self.state = InnerState::Domain { length: 0 };
                }
                InnerState::Domain { length } => {
                    if length == 0 {
                        if self.buffer.is_empty() {
                            return Ok(None);
                        }
                        let len = self.buffer[0] as usize;
                        if len == 0 {
                            return Ok(Some(RequestOutcome::Rejected {
                                reply_code: Socks5ErrorKind::ZeroDomain.reply_code().code(),
                            }));
                        }
                        if len > limits.domain_max_bytes {
                            return Ok(Some(RequestOutcome::Rejected {
                                reply_code: Socks5ErrorKind::DomainCeiling.reply_code().code(),
                            }));
                        }
                        self.domain_length = len;
                        // Consume the length byte.
                        self.buffer.copy_within(1.., 0);
                        self.buffer.truncate(self.buffer.len() - 1);
                        self.state = InnerState::Domain { length: len };
                        continue;
                    }
                    if self.domain.len() < length {
                        if self.buffer.is_empty() {
                            return Ok(None);
                        }
                        let needed = length - self.domain.len();
                        let take = needed.min(self.buffer.len());
                        // Reject NUL/control/whitespace bytes during
                        // accumulation so the failure surfaces as a
                        // structural Socks5Error rather than a
                        // policy Rejected reply.
                        for &byte in &self.buffer[..take] {
                            if byte <= 0x20 || byte == 0x7f {
                                return Err(rejected(
                                    Socks5ErrorKind::MalformedDomain,
                                    "domain contains control or whitespace bytes",
                                ));
                            }
                        }
                        self.domain.extend_from_slice(&self.buffer[..take]);
                        self.buffer.copy_within(take.., 0);
                        self.buffer.truncate(self.buffer.len() - take);
                        if self.domain.len() < length {
                            return Ok(None);
                        }
                    }
                    // Domain fully assembled; validate the `.i2p`
                    // policy and reject IP literals, clearnet hosts,
                    // localhost, mixed-suffix confusion, and
                    // malformed alias spellings via the typed
                    // Rejected reply.
                    if let Err(error) = validate_domain_policy(&self.domain) {
                        let code = error.kind.reply_code().code();
                        return Ok(Some(RequestOutcome::Rejected { reply_code: code }));
                    }
                    self.state = InnerState::Port;
                }
                InnerState::Port => {
                    if self.buffer.len() < PORT_FIELD_LEN {
                        return Ok(None);
                    }
                    let port = u16::from_be_bytes([self.buffer[0], self.buffer[1]]);
                    self.buffer.copy_within(PORT_FIELD_LEN.., 0);
                    self.buffer.truncate(self.buffer.len() - PORT_FIELD_LEN);
                    if port == 0 {
                        return Ok(Some(RequestOutcome::Rejected {
                            reply_code: Socks5ErrorKind::ZeroPort.reply_code().code(),
                        }));
                    }
                    // Domain was validated as ASCII during accumulation.
                    let host = std::str::from_utf8(&self.domain).map_err(|_| {
                        Socks5Error::new(
                            Socks5ErrorKind::MalformedDomain,
                            "domain is not valid UTF-8",
                        )
                    })?;
                    let destination = ConnectDestination {
                        host: host.to_ascii_lowercase(),
                        port,
                    };
                    let leftover = std::mem::take(&mut self.buffer);
                    return Ok(Some(RequestOutcome::ReadyToConnect {
                        destination,
                        leftover,
                    }));
                }
            }
        }
    }
}

/// Validates the `.i2p` target policy for a fully accumulated
/// domain. NUL/control/whitespace byte rejection is handled during
/// accumulation; this function only enforces the RFC 1928 + I2P
/// policy outcomes (clearnet, IP literal, localhost, mixed-suffix,
/// malformed alias).
fn validate_domain_policy(bytes: &[u8]) -> Result<(), Socks5Error> {
    let rejected = |kind, reason| Socks5Error::new(kind, reason);
    if bytes.is_empty() {
        return Err(rejected(Socks5ErrorKind::ZeroDomain, "domain is empty"));
    }
    let host = std::str::from_utf8(bytes).map_err(|_| {
        rejected(
            Socks5ErrorKind::MalformedDomain,
            "domain is not valid UTF-8",
        )
    })?;
    let host = host.to_ascii_lowercase();
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Err(rejected(
            Socks5ErrorKind::NonI2pTarget,
            "IP literals are rejected",
        ));
    }
    if host == "localhost" || host.ends_with(".localhost") {
        return Err(rejected(
            Socks5ErrorKind::NonI2pTarget,
            "localhost authority is rejected",
        ));
    }
    if !host.ends_with(".i2p") {
        return Err(rejected(
            Socks5ErrorKind::NonI2pTarget,
            "domain must end with .i2p",
        ));
    }
    if host.ends_with(B32_SUFFIX) {
        // Base32 references are accepted structurally here; deeper
        // validation (52-char canonical `a-z2-7`) lives in
        // `crate::destination::DestinationRef::parse` and is
        // performed by the daemon before the streaming connect.
        return Ok(());
    }
    validate_static_alias(&host)
        .map_err(|_| rejected(Socks5ErrorKind::MalformedAlias, "static alias is malformed"))?;
    let _ = DEFAULT_CONNECT_PORT; // keep the constant referenced
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Socks5Limits {
        Socks5Limits::defaults()
    }

    fn feed(
        parser: &mut RequestParser,
        bytes: &[u8],
    ) -> Result<Option<RequestOutcome>, Socks5Error> {
        parser.advance(bytes, limits())
    }

    fn make_request(host: &str, port: u16) -> Vec<u8> {
        let mut bytes = vec![
            SOCKS_VERSION,
            SOCKS5_CMD_CONNECT,
            SOCKS5_RESERVED,
            SOCKS5_ATYP_DOMAIN,
        ];
        bytes.push(host.len() as u8);
        bytes.extend_from_slice(host.as_bytes());
        bytes.extend_from_slice(&port.to_be_bytes());
        bytes
    }

    #[test]
    fn parses_minimal_connect_request() {
        let mut parser = RequestParser::new();
        let outcome = feed(&mut parser, &make_request("example.i2p", 443))
            .expect("ok")
            .expect("terminal");
        match outcome {
            RequestOutcome::ReadyToConnect {
                destination,
                leftover,
            } => {
                assert_eq!(destination.host, "example.i2p");
                assert_eq!(destination.port, 443);
                assert!(leftover.is_empty());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_incremental_one_byte() {
        let mut parser = RequestParser::new();
        let request = make_request("example.i2p", 443);
        for byte in &request[..request.len() - 1] {
            let result = feed(&mut parser, &[*byte]).expect("ok");
            assert!(result.is_none(), "non-terminal before final byte");
        }
        let result = feed(&mut parser, &[request[request.len() - 1]]).expect("ok");
        assert!(matches!(
            result,
            Some(RequestOutcome::ReadyToConnect { .. })
        ));
    }

    #[test]
    fn request_plus_early_tunnel_bytes_preserved() {
        let mut parser = RequestParser::new();
        let mut bytes = make_request("example.i2p", 443);
        bytes.extend_from_slice(b"PRESHARED-TUNNEL-PAYLOAD");
        let outcome = feed(&mut parser, &bytes).expect("ok").expect("terminal");
        match outcome {
            RequestOutcome::ReadyToConnect { leftover, .. } => {
                assert_eq!(leftover, b"PRESHARED-TUNNEL-PAYLOAD");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn rejects_bind_command() {
        let mut parser = RequestParser::new();
        let mut bytes = vec![
            SOCKS_VERSION,
            SOCKS5_CMD_BIND,
            SOCKS5_RESERVED,
            SOCKS5_ATYP_DOMAIN,
        ];
        bytes.push(11);
        bytes.extend_from_slice(b"example.i2p");
        bytes.extend_from_slice(&443_u16.to_be_bytes());
        let outcome = feed(&mut parser, &bytes).expect("ok").expect("terminal");
        assert_eq!(
            outcome,
            RequestOutcome::Rejected {
                reply_code: Socks5ErrorKind::UnsupportedCommand.reply_code().code()
            }
        );
    }

    #[test]
    fn rejects_udp_associate_command() {
        let mut parser = RequestParser::new();
        let mut bytes = vec![
            SOCKS_VERSION,
            SOCKS5_CMD_UDP_ASSOCIATE,
            SOCKS5_RESERVED,
            SOCKS5_ATYP_DOMAIN,
        ];
        bytes.push(11);
        bytes.extend_from_slice(b"example.i2p");
        bytes.extend_from_slice(&443_u16.to_be_bytes());
        let outcome = feed(&mut parser, &bytes).expect("ok").expect("terminal");
        assert_eq!(
            outcome,
            RequestOutcome::Rejected {
                reply_code: Socks5ErrorKind::UnsupportedCommand.reply_code().code()
            }
        );
    }

    #[test]
    fn rejects_unknown_command() {
        let mut parser = RequestParser::new();
        let mut bytes = vec![SOCKS_VERSION, 0x09, SOCKS5_RESERVED, SOCKS5_ATYP_DOMAIN];
        bytes.push(11);
        bytes.extend_from_slice(b"example.i2p");
        bytes.extend_from_slice(&443_u16.to_be_bytes());
        let outcome = feed(&mut parser, &bytes).expect("ok").expect("terminal");
        assert_eq!(
            outcome,
            RequestOutcome::Rejected {
                reply_code: Socks5ErrorKind::UnsupportedCommand.reply_code().code()
            }
        );
    }

    #[test]
    fn rejects_ipv4_address_type() {
        let mut parser = RequestParser::new();
        let mut bytes = vec![
            SOCKS_VERSION,
            SOCKS5_CMD_CONNECT,
            SOCKS5_RESERVED,
            SOCKS5_ATYP_IPV4,
        ];
        bytes.extend_from_slice(&[127, 0, 0, 1]);
        bytes.extend_from_slice(&443_u16.to_be_bytes());
        let outcome = feed(&mut parser, &bytes).expect("ok").expect("terminal");
        assert_eq!(
            outcome,
            RequestOutcome::Rejected {
                reply_code: Socks5ErrorKind::UnsupportedAddressType.reply_code().code()
            }
        );
    }

    #[test]
    fn rejects_ipv6_address_type() {
        let mut parser = RequestParser::new();
        let mut bytes = vec![
            SOCKS_VERSION,
            SOCKS5_CMD_CONNECT,
            SOCKS5_RESERVED,
            SOCKS5_ATYP_IPV6,
        ];
        bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        bytes.extend_from_slice(&443_u16.to_be_bytes());
        let outcome = feed(&mut parser, &bytes).expect("ok").expect("terminal");
        assert_eq!(
            outcome,
            RequestOutcome::Rejected {
                reply_code: Socks5ErrorKind::UnsupportedAddressType.reply_code().code()
            }
        );
    }

    #[test]
    fn rejects_zero_length_domain() {
        let mut parser = RequestParser::new();
        let mut bytes = vec![
            SOCKS_VERSION,
            SOCKS5_CMD_CONNECT,
            SOCKS5_RESERVED,
            SOCKS5_ATYP_DOMAIN,
        ];
        bytes.push(0);
        bytes.extend_from_slice(&443_u16.to_be_bytes());
        let outcome = feed(&mut parser, &bytes).expect("ok").expect("terminal");
        assert_eq!(
            outcome,
            RequestOutcome::Rejected {
                reply_code: Socks5ErrorKind::ZeroDomain.reply_code().code()
            }
        );
    }

    #[test]
    fn rejects_zero_port() {
        let mut parser = RequestParser::new();
        let mut bytes = vec![
            SOCKS_VERSION,
            SOCKS5_CMD_CONNECT,
            SOCKS5_RESERVED,
            SOCKS5_ATYP_DOMAIN,
        ];
        bytes.push(11);
        bytes.extend_from_slice(b"example.i2p");
        bytes.extend_from_slice(&0_u16.to_be_bytes());
        let outcome = feed(&mut parser, &bytes).expect("ok").expect("terminal");
        assert_eq!(
            outcome,
            RequestOutcome::Rejected {
                reply_code: Socks5ErrorKind::ZeroPort.reply_code().code()
            }
        );
    }

    #[test]
    fn rejects_clearnet_target() {
        let mut parser = RequestParser::new();
        let outcome = feed(&mut parser, &make_request("example.com", 443))
            .expect("ok")
            .expect("terminal");
        assert_eq!(
            outcome,
            RequestOutcome::Rejected {
                reply_code: Socks5ErrorKind::NonI2pTarget.reply_code().code()
            }
        );
    }

    #[test]
    fn rejects_localhost_target() {
        let mut parser = RequestParser::new();
        let outcome = feed(&mut parser, &make_request("localhost", 443))
            .expect("ok")
            .expect("terminal");
        assert_eq!(
            outcome,
            RequestOutcome::Rejected {
                reply_code: Socks5ErrorKind::NonI2pTarget.reply_code().code()
            }
        );
    }

    #[test]
    fn rejects_mixed_suffix_trick() {
        let mut parser = RequestParser::new();
        let outcome = feed(&mut parser, &make_request("example.i2p.example.com", 443))
            .expect("ok")
            .expect("terminal");
        assert_eq!(
            outcome,
            RequestOutcome::Rejected {
                reply_code: Socks5ErrorKind::NonI2pTarget.reply_code().code()
            }
        );
    }

    #[test]
    fn rejects_uppercase_static_alias() {
        let mut parser = RequestParser::new();
        let outcome = feed(&mut parser, &make_request("BAD_UPPER.i2p", 443))
            .expect("ok")
            .expect("terminal");
        // Lowercased then validated: still rejected because the
        // strict alias grammar does not accept underscore; the
        // policy reports MalformedAlias (which is a non-I2P target).
        assert!(matches!(
            outcome,
            RequestOutcome::Rejected { reply_code: 0x02 }
        ));
    }

    #[test]
    fn rejects_domain_with_control_byte() {
        let mut parser = RequestParser::new();
        let mut bytes = vec![
            SOCKS_VERSION,
            SOCKS5_CMD_CONNECT,
            SOCKS5_RESERVED,
            SOCKS5_ATYP_DOMAIN,
        ];
        let host = b"exam\x01ple.i2p";
        bytes.push(host.len() as u8);
        bytes.extend_from_slice(host);
        bytes.extend_from_slice(&443_u16.to_be_bytes());
        let err = feed(&mut parser, &bytes).expect_err("control byte");
        assert!(matches!(err.kind, Socks5ErrorKind::MalformedDomain));
    }

    #[test]
    fn wrong_request_version_is_rejected() {
        let mut parser = RequestParser::new();
        let err = feed(
            &mut parser,
            &[0x04, SOCKS5_CMD_CONNECT, 0, SOCKS5_ATYP_DOMAIN],
        )
        .expect_err("wrong version");
        assert!(matches!(err.kind, Socks5ErrorKind::WrongRequestVersion));
    }

    #[test]
    fn bad_reserved_is_rejected() {
        let mut parser = RequestParser::new();
        let err = feed(
            &mut parser,
            &[SOCKS_VERSION, SOCKS5_CMD_CONNECT, 0x42, SOCKS5_ATYP_DOMAIN],
        )
        .expect_err("bad rsv");
        assert!(matches!(err.kind, Socks5ErrorKind::BadReserved));
    }

    #[test]
    fn domain_length_byte_too_large_is_rejected() {
        let mut parser = RequestParser::new();
        // Length byte claims more than the configured ceiling.
        let len = (Socks5Limits::defaults().domain_max_bytes + 1) as u8;
        let mut bytes = vec![
            SOCKS_VERSION,
            SOCKS5_CMD_CONNECT,
            SOCKS5_RESERVED,
            SOCKS5_ATYP_DOMAIN,
        ];
        bytes.push(len);
        bytes.extend_from_slice(&vec![b'a'; len as usize]);
        bytes.extend_from_slice(&443_u16.to_be_bytes());
        let outcome = feed(&mut parser, &bytes).expect("ok").expect("terminal");
        assert_eq!(
            outcome,
            RequestOutcome::Rejected {
                reply_code: Socks5ErrorKind::DomainCeiling.reply_code().code()
            }
        );
    }

    #[test]
    fn domain_with_nul_byte_is_rejected() {
        let mut parser = RequestParser::new();
        let mut bytes = vec![
            SOCKS_VERSION,
            SOCKS5_CMD_CONNECT,
            SOCKS5_RESERVED,
            SOCKS5_ATYP_DOMAIN,
        ];
        let host = b"exam\0ple.i2p";
        bytes.push(host.len() as u8);
        bytes.extend_from_slice(host);
        bytes.extend_from_slice(&443_u16.to_be_bytes());
        let err = feed(&mut parser, &bytes).expect_err("nul byte");
        assert!(matches!(err.kind, Socks5ErrorKind::MalformedDomain));
    }

    #[test]
    fn base32_domain_is_accepted_structurally() {
        let mut parser = RequestParser::new();
        let b32 = format!("{}{B32_SUFFIX}", "a".repeat(52));
        let outcome = feed(&mut parser, &make_request(&b32, 443))
            .expect("ok")
            .expect("terminal");
        assert!(matches!(outcome, RequestOutcome::ReadyToConnect { .. }));
    }
}
