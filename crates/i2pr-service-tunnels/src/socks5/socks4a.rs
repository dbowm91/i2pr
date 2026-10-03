//! Plan 290 bounded SOCKS4a CONNECT request parser.
//!
//! Java I2PTunnel's historical SOCKS profile accepts SOCKS 4/4a/5
//! (official I2PTunnel documentation: "SOCKS 4/4a/5 — Enables using
//! the I2P router as a SOCKS proxy"). The pinned Proposal 170
//! `socks`/`socksirc` types map onto that profile, so the ordinary
//! `socks` backend and the new `socks-irc` composition accept
//! SOCKS4a CONNECT alongside SOCKS5. Plain SOCKS4 (literal IPv4,
//! no domain extension) stays fail-closed: an IPv4 destination is
//! clearnet by definition under the M10 `.i2p`-only policy.
//!
//! Wire shape (client request):
//! `VN(0x04) + CD(0x01 CONNECT) + DSTPORT(u16 BE) +
//! DSTIP(0.0.0.x, x != 0 marks the 4a extension) +
//! USERID(NUL-terminated, bounded) + DOMAIN(NUL-terminated, bounded)`.
//!
//! Server reply: `VN(0x00) + CD(90 granted / 91 rejected) +
//! DSTPORT(0) + DSTIP(0)`. The reply never echoes request bytes.
//!
//! The parser is incremental and bounded like the SOCKS5 request
//! parser: one byte at a time or the whole request plus early
//! tunnel bytes in one read; same-read leftovers are preserved
//! verbatim. Domain policy (`.i2p`-only) is shared with the SOCKS5
//! request parser.

#![forbid(unsafe_code)]

use super::errors::{Socks5Error, Socks5ErrorKind};
use super::limits::Socks5Limits;
use super::request::{ConnectDestination, validate_domain_policy};

/// SOCKS4a version byte.
pub const SOCKS4A_VERSION: u8 = 0x04;
/// SOCKS4a CONNECT command byte.
pub const SOCKS4A_CMD_CONNECT: u8 = 0x01;
/// Length of the fixed request header
/// (`VN + CD + DSTPORT + DSTIP`).
pub const SOCKS4A_FIXED_HEADER_LEN: usize = 8;
/// Maximum USERID bytes before the NUL terminator.
pub const SOCKS4A_USERID_MAX_BYTES: usize = 255;
/// Maximum retained bytes before a complete 4a request
/// (fixed header + max userid + NUL + max domain + NUL).
pub const SOCKS4A_RETAINED_MAX_BYTES: usize = 520;
/// Length of the generated 4a reply.
pub const SOCKS4A_REPLY_LEN: usize = 8;
/// SOCKS4a granted reply code.
pub const SOCKS4A_GRANTED: u8 = 90;
/// SOCKS4a rejected reply code.
pub const SOCKS4A_REJECTED: u8 = 91;

/// Typed terminal outcome of a SOCKS4a CONNECT request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Socks4aOutcome {
    /// Request is fully parsed and validated against the
    /// runtime-neutral `.i2p` target policy. The daemon must open
    /// I2P Streaming and only emit the granted reply once the
    /// connection reaches `Established`. Leftover bytes are the
    /// same-read application bytes after the domain terminator.
    ReadyToConnect {
        /// Parsed CONNECT destination.
        destination: ConnectDestination,
        /// Same-read bytes after the end of the CONNECT request.
        leftover: Vec<u8>,
    },
    /// Request is structurally complete but rejected (non-CONNECT
    /// command, plain SOCKS4 IPv4, zero port, overlong/empty
    /// domain, or `.i2p` policy failure). The daemon emits the
    /// 91 reply and closes without entering tunnel mode.
    Rejected,
}

/// Builds one bounded SOCKS4a reply. Never echoes request bytes.
pub fn build_socks4a_reply(granted: bool) -> [u8; SOCKS4A_REPLY_LEN] {
    let code = if granted {
        SOCKS4A_GRANTED
    } else {
        SOCKS4A_REJECTED
    };
    [0x00, code, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InnerState {
    FixedHeader,
    Userid,
    Domain,
}

/// Incremental SOCKS4a CONNECT request parser.
#[derive(Clone, Debug)]
pub struct Socks4aRequestParser {
    state: InnerState,
    buffer: Vec<u8>,
    port: u16,
    userid: Vec<u8>,
    domain: Vec<u8>,
}

impl Default for Socks4aRequestParser {
    fn default() -> Self {
        Self::new()
    }
}

impl Socks4aRequestParser {
    /// Creates a fresh 4a request parser.
    pub fn new() -> Self {
        Self {
            state: InnerState::FixedHeader,
            buffer: Vec::with_capacity(SOCKS4A_FIXED_HEADER_LEN),
            port: 0,
            userid: Vec::new(),
            domain: Vec::new(),
        }
    }

    /// Returns the bytes retained so far.
    pub fn retained(&self) -> &[u8] {
        &self.buffer
    }

    /// Feeds bytes into the parser. Returns:
    ///
    /// - `Ok(None)` when the request is incomplete;
    /// - `Ok(Some(Socks4aOutcome))` on a terminal decision;
    /// - `Err(Socks5Error)` on a structural violation that has no
    ///   4a reply mapping (wrong version, retained ceiling
    ///   overflow, USERID framing failure).
    pub fn advance(
        &mut self,
        bytes: &[u8],
        limits: Socks5Limits,
    ) -> Result<Option<Socks4aOutcome>, Socks5Error> {
        let rejected = |kind, reason| Socks5Error::new(kind, reason);
        if self.buffer.len() + bytes.len() > SOCKS4A_RETAINED_MAX_BYTES {
            return Err(rejected(
                Socks5ErrorKind::BufferCeilingExceeded,
                "4a request bytes exceed the retained buffer ceiling",
            ));
        }
        self.buffer.extend_from_slice(bytes);
        loop {
            match self.state {
                InnerState::FixedHeader => {
                    if self.buffer.len() < SOCKS4A_FIXED_HEADER_LEN {
                        return Ok(None);
                    }
                    let version = self.buffer[0];
                    let command = self.buffer[1];
                    if version != SOCKS4A_VERSION {
                        return Err(rejected(
                            Socks5ErrorKind::WrongRequestVersion,
                            "4a request version must be 0x04",
                        ));
                    }
                    if command != SOCKS4A_CMD_CONNECT {
                        self.drain(SOCKS4A_FIXED_HEADER_LEN);
                        return Ok(Some(Socks4aOutcome::Rejected));
                    }
                    let port = u16::from_be_bytes([self.buffer[2], self.buffer[3]]);
                    if port == 0 {
                        self.drain(SOCKS4A_FIXED_HEADER_LEN);
                        return Ok(Some(Socks4aOutcome::Rejected));
                    }
                    // Only the 4a extension marker (`0.0.0.x`, x !=
                    // 0) is accepted. Any other DSTIP is a plain
                    // SOCKS4 IPv4 literal: clearnet by definition,
                    // fail-closed without a domain to validate.
                    let ip = &self.buffer[4..8];
                    if ip[0] != 0 || ip[1] != 0 || ip[2] != 0 || ip[3] == 0 {
                        self.drain(SOCKS4A_FIXED_HEADER_LEN);
                        return Ok(Some(Socks4aOutcome::Rejected));
                    }
                    self.port = port;
                    self.drain(SOCKS4A_FIXED_HEADER_LEN);
                    self.state = InnerState::Userid;
                }
                InnerState::Userid => {
                    let Some(end) = self.buffer.iter().position(|byte| *byte == 0) else {
                        if self.buffer.len() > SOCKS4A_USERID_MAX_BYTES {
                            return Err(rejected(
                                Socks5ErrorKind::MalformedUserid,
                                "4a userid exceeds the ceiling",
                            ));
                        }
                        return Ok(None);
                    };
                    if end > SOCKS4A_USERID_MAX_BYTES {
                        return Err(rejected(
                            Socks5ErrorKind::MalformedUserid,
                            "4a userid exceeds the ceiling",
                        ));
                    };
                    // USERID is opaque (never logged); reject
                    // control bytes other than the terminator so a
                    // smuggled framing byte cannot hide inside it.
                    for &byte in &self.buffer[..end] {
                        if byte < 0x20 || byte == 0x7f {
                            return Err(rejected(
                                Socks5ErrorKind::MalformedUserid,
                                "4a userid contains control bytes",
                            ));
                        }
                    }
                    self.userid.extend_from_slice(&self.buffer[..end]);
                    self.drain(end + 1);
                    self.state = InnerState::Domain;
                }
                InnerState::Domain => {
                    let Some(end) = self.buffer.iter().position(|byte| *byte == 0) else {
                        if self.buffer.len() > limits.domain_max_bytes + 1 {
                            return Ok(Some(Socks4aOutcome::Rejected));
                        }
                        return Ok(None);
                    };
                    if end == 0 || end > limits.domain_max_bytes {
                        return Ok(Some(Socks4aOutcome::Rejected));
                    }
                    self.domain.extend_from_slice(&self.buffer[..end]);
                    // Reject NUL/control/whitespace during
                    // accumulation, matching the SOCKS5 parser.
                    for &byte in &self.domain {
                        if byte <= 0x20 || byte == 0x7f {
                            return Err(rejected(
                                Socks5ErrorKind::MalformedDomain,
                                "4a domain contains control or whitespace bytes",
                            ));
                        }
                    }
                    if validate_domain_policy(&self.domain).is_err() {
                        return Ok(Some(Socks4aOutcome::Rejected));
                    }
                    let host = std::str::from_utf8(&self.domain).map_err(|_| {
                        Socks5Error::new(
                            Socks5ErrorKind::MalformedDomain,
                            "4a domain is not valid UTF-8",
                        )
                    })?;
                    let destination = ConnectDestination {
                        host: host.to_ascii_lowercase(),
                        port: self.port,
                    };
                    let leftover = std::mem::take(&mut self.buffer);
                    // Consume the domain terminator from the
                    // leftover: `buffer` still holds
                    // `domain + NUL + post-request bytes`.
                    let leftover = leftover[end + 1..].to_vec();
                    return Ok(Some(Socks4aOutcome::ReadyToConnect {
                        destination,
                        leftover,
                    }));
                }
            }
        }
    }

    /// Drains `count` leading bytes from the retained buffer.
    fn drain(&mut self, count: usize) {
        self.buffer.copy_within(count.., 0);
        self.buffer.truncate(self.buffer.len() - count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Socks5Limits {
        Socks5Limits::defaults()
    }

    fn feed(
        parser: &mut Socks4aRequestParser,
        bytes: &[u8],
    ) -> Result<Option<Socks4aOutcome>, Socks5Error> {
        parser.advance(bytes, limits())
    }

    fn make_request(host: &str, port: u16) -> Vec<u8> {
        let mut bytes = vec![SOCKS4A_VERSION, SOCKS4A_CMD_CONNECT];
        bytes.extend_from_slice(&port.to_be_bytes());
        // 4a extension marker 0.0.0.1.
        bytes.extend_from_slice(&[0, 0, 0, 1]);
        bytes.extend_from_slice(b"userid");
        bytes.push(0);
        bytes.extend_from_slice(host.as_bytes());
        bytes.push(0);
        bytes
    }

    #[test]
    fn parses_minimal_4a_connect() {
        let mut parser = Socks4aRequestParser::new();
        let outcome = feed(&mut parser, &make_request("example.i2p", 443))
            .expect("ok")
            .expect("terminal");
        match outcome {
            Socks4aOutcome::ReadyToConnect {
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
        let mut parser = Socks4aRequestParser::new();
        let request = make_request("example.i2p", 443);
        for byte in &request[..request.len() - 1] {
            let result = feed(&mut parser, &[*byte]).expect("ok");
            assert!(result.is_none(), "non-terminal before final byte");
        }
        let result = feed(&mut parser, &[request[request.len() - 1]]).expect("ok");
        assert!(matches!(
            result,
            Some(Socks4aOutcome::ReadyToConnect { .. })
        ));
    }

    #[test]
    fn early_tunnel_bytes_preserved() {
        let mut parser = Socks4aRequestParser::new();
        let mut bytes = make_request("example.i2p", 443);
        bytes.extend_from_slice(b"EARLY-IRC-LINE\r\n");
        let outcome = feed(&mut parser, &bytes).expect("ok").expect("terminal");
        match outcome {
            Socks4aOutcome::ReadyToConnect { leftover, .. } => {
                assert_eq!(leftover, b"EARLY-IRC-LINE\r\n");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn plain_socks4_ipv4_is_rejected() {
        let mut parser = Socks4aRequestParser::new();
        let mut bytes = vec![SOCKS4A_VERSION, SOCKS4A_CMD_CONNECT];
        bytes.extend_from_slice(&443_u16.to_be_bytes());
        // Plain SOCKS4 IPv4 literal (no 4a marker): clearnet, fail-closed.
        bytes.extend_from_slice(&[93, 184, 216, 34]);
        bytes.extend_from_slice(b"user");
        bytes.push(0);
        let outcome = feed(&mut parser, &bytes).expect("ok").expect("terminal");
        assert_eq!(outcome, Socks4aOutcome::Rejected);
    }

    #[test]
    fn bind_command_is_rejected() {
        let mut parser = Socks4aRequestParser::new();
        let mut bytes = vec![SOCKS4A_VERSION, 0x02];
        bytes.extend_from_slice(&443_u16.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 1]);
        bytes.extend_from_slice(b"user");
        bytes.push(0);
        bytes.extend_from_slice(b"example.i2p");
        bytes.push(0);
        let outcome = feed(&mut parser, &bytes).expect("ok").expect("terminal");
        assert_eq!(outcome, Socks4aOutcome::Rejected);
    }

    #[test]
    fn zero_port_is_rejected() {
        let mut parser = Socks4aRequestParser::new();
        let mut bytes = vec![SOCKS4A_VERSION, SOCKS4A_CMD_CONNECT];
        bytes.extend_from_slice(&0_u16.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 1]);
        bytes.extend_from_slice(b"user");
        bytes.push(0);
        bytes.extend_from_slice(b"example.i2p");
        bytes.push(0);
        let outcome = feed(&mut parser, &bytes).expect("ok").expect("terminal");
        assert_eq!(outcome, Socks4aOutcome::Rejected);
    }

    #[test]
    fn clearnet_domain_is_rejected() {
        let mut parser = Socks4aRequestParser::new();
        let outcome = feed(&mut parser, &make_request("example.com", 443))
            .expect("ok")
            .expect("terminal");
        assert_eq!(outcome, Socks4aOutcome::Rejected);
    }

    #[test]
    fn empty_domain_is_rejected() {
        let mut parser = Socks4aRequestParser::new();
        let mut bytes = vec![SOCKS4A_VERSION, SOCKS4A_CMD_CONNECT];
        bytes.extend_from_slice(&443_u16.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 1]);
        bytes.extend_from_slice(b"user");
        bytes.push(0);
        bytes.push(0);
        let outcome = feed(&mut parser, &bytes).expect("ok").expect("terminal");
        assert_eq!(outcome, Socks4aOutcome::Rejected);
    }

    #[test]
    fn overlong_domain_is_rejected() {
        let mut parser = Socks4aRequestParser::new();
        let long = format!("{}.i2p", "a".repeat(300));
        let outcome = feed(&mut parser, &make_request(&long, 443))
            .expect("ok")
            .expect("terminal");
        assert_eq!(outcome, Socks4aOutcome::Rejected);
    }

    #[test]
    fn userid_control_byte_is_structural_error() {
        let mut parser = Socks4aRequestParser::new();
        let mut bytes = vec![SOCKS4A_VERSION, SOCKS4A_CMD_CONNECT];
        bytes.extend_from_slice(&443_u16.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 1]);
        bytes.extend_from_slice(b"us\x01er");
        bytes.push(0);
        bytes.extend_from_slice(b"example.i2p");
        bytes.push(0);
        let error = feed(&mut parser, &bytes).expect_err("control byte");
        assert!(matches!(error.kind, Socks5ErrorKind::MalformedUserid));
    }

    #[test]
    fn wrong_version_is_structural_error() {
        let mut parser = Socks4aRequestParser::new();
        // A full 8-byte header with a non-0x04 version byte.
        let error = feed(&mut parser, &[0x05, 0x01, 0x01, 0xbb, 0, 0, 0, 1])
            .expect_err("version")
            .kind;
        assert!(matches!(error, Socks5ErrorKind::WrongRequestVersion));
    }

    #[test]
    fn reply_shape_is_bounded_neutral() {
        assert_eq!(
            build_socks4a_reply(true),
            [0x00, SOCKS4A_GRANTED, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            build_socks4a_reply(false),
            [0x00, SOCKS4A_REJECTED, 0, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn base32_domain_accepted_structurally() {
        let mut parser = Socks4aRequestParser::new();
        let b32 = format!("{}.b32.i2p", "a".repeat(52));
        let outcome = feed(&mut parser, &make_request(&b32, 443))
            .expect("ok")
            .expect("terminal");
        assert!(matches!(outcome, Socks4aOutcome::ReadyToConnect { .. }));
    }
}
