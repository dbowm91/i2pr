//! Plan 177 SOCKS5 greeting negotiation.
//!
//! Parses the RFC 1928 greeting (`VER + NMETHODS + METHODS`) from a
//! bounded byte buffer and selects the no-authentication method
//! when offered.
//!
//! The greeting parser is incremental and bounded:
//!
//! - VER must be `0x05`; any other byte is
//!   [`Socks5ErrorKind::WrongVersion`];
//! - `NMETHODS` must be `1..=method_count_max`;
//!   zero methods is [`Socks5ErrorKind::ZeroMethods`];
//!   oversized lists are [`Socks5ErrorKind::TooManyMethods`];
//! - the method list must contain `0x00 NO AUTHENTICATION REQUIRED`;
//!   otherwise the parser emits the `0xff` no-acceptable-method
//!   state and the daemon closes the socket after flushing the
//!   reply.
//!
//! The greeting section never includes the request body. The
//! request parser handles the post-greeting bytes.

#![forbid(unsafe_code)]

use super::config::{SOCKS_VERSION, SOCKS5_METHOD_NO_AUTH, SOCKS5_METHOD_USER_PASS};
use super::errors::{Socks5Error, Socks5ErrorKind};
use super::limits::Socks5Limits;

/// Outcome of greeting negotiation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GreetingOutcome {
    /// Greeting accepted; reply `05 00` (no-authentication) should
    /// be emitted and the parser advanced to the request stage.
    NoAuthentication,
    /// Greeting offered no acceptable method; reply `05 ff`
    /// should be emitted and the daemon must close the socket
    /// after flushing the reply.
    NoAcceptableMethod,
}

/// Incremental greeting parser state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GreetingParser {
    /// Bounded greeting bytes retained so far.
    buffer: Vec<u8>,
}

impl GreetingParser {
    /// Creates a fresh greeting parser.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the bytes retained so far.
    pub fn retained(&self) -> &[u8] {
        &self.buffer
    }

    /// Feeds bytes into the greeting parser. Returns:
    ///
    /// - `Ok(None)` when the greeting is incomplete and more bytes
    ///   are required;
    /// - `Ok(Some((outcome, consumed)))` when a terminal greeting
    ///   decision has been reached; `consumed` is the number of
    ///   bytes that belong to the greeting (the daemon forwards the
    ///   remainder to the request parser);
    /// - `Err(Socks5Error)` on a structural violation.
    pub fn advance(
        &mut self,
        bytes: &[u8],
        limits: Socks5Limits,
    ) -> Result<Option<(GreetingOutcome, usize)>, Socks5Error> {
        let rejected = |kind, reason| Socks5Error::new(kind, reason);
        if self.buffer.len() + bytes.len() > limits.greeting_max_bytes {
            return Err(rejected(
                Socks5ErrorKind::GreetingCeiling,
                "greeting bytes exceed the ceiling",
            ));
        }
        self.buffer.extend_from_slice(bytes);
        // Need at least VER + NMETHODS to dispatch.
        if self.buffer.len() < 2 {
            return Ok(None);
        }
        if self.buffer[0] != SOCKS_VERSION {
            return Err(rejected(
                Socks5ErrorKind::WrongVersion,
                "greeting version must be 0x05",
            ));
        }
        let nmethods = self.buffer[1] as usize;
        if nmethods == 0 {
            return Err(rejected(
                Socks5ErrorKind::ZeroMethods,
                "NMETHODS must be at least one",
            ));
        }
        if nmethods > limits.method_count_max {
            return Err(rejected(
                Socks5ErrorKind::TooManyMethods,
                "NMETHODS exceeds the configured ceiling",
            ));
        }
        let total = 2 + nmethods;
        if self.buffer.len() < total {
            return Ok(None);
        }
        // Trim the buffer so it holds only the greeting bytes; the
        // daemon forwards the remainder to the request parser.
        self.buffer.truncate(total);
        let methods = &self.buffer[2..total];
        // Plan 177 §4: never silently accept `0x02` username/password
        // just because the client offered it; require `0x00`.
        if !methods.contains(&SOCKS5_METHOD_NO_AUTH) {
            // `0x02` is never accepted even when offered.
            let _ = SOCKS5_METHOD_USER_PASS;
            return Ok(Some((GreetingOutcome::NoAcceptableMethod, total)));
        }
        Ok(Some((GreetingOutcome::NoAuthentication, total)))
    }

    /// Returns the no-authentication reply bytes (`05 00`).
    pub fn no_auth_reply() -> [u8; 2] {
        super::config::SOCKS5_NO_AUTH_REPLY
    }

    /// Returns the no-acceptable-method reply bytes (`05 ff`).
    pub fn no_acceptable_method_reply() -> [u8; 2] {
        super::config::SOCKS5_NO_ACCEPTABLE_METHOD_REPLY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Socks5Limits {
        Socks5Limits::defaults()
    }

    #[test]
    fn no_auth_happy_path() {
        let mut parser = GreetingParser::new();
        let bytes = [SOCKS_VERSION, 1, SOCKS5_METHOD_NO_AUTH];
        let outcome = parser
            .advance(&bytes, limits())
            .expect("ok")
            .expect("terminal");
        assert_eq!(outcome, (GreetingOutcome::NoAuthentication, 3));
    }

    #[test]
    fn multiple_methods_with_zero_present() {
        let mut parser = GreetingParser::new();
        let bytes = [SOCKS_VERSION, 3, SOCKS5_METHOD_NO_AUTH, 0x80, 0x81];
        let outcome = parser
            .advance(&bytes, limits())
            .expect("ok")
            .expect("terminal");
        assert_eq!(outcome, (GreetingOutcome::NoAuthentication, 5));
    }

    #[test]
    fn no_acceptable_method_when_zero_absent() {
        let mut parser = GreetingParser::new();
        let bytes = [SOCKS_VERSION, 2, 0x80, 0x81];
        let outcome = parser
            .advance(&bytes, limits())
            .expect("ok")
            .expect("terminal");
        assert_eq!(outcome, (GreetingOutcome::NoAcceptableMethod, 4));
    }

    #[test]
    fn username_password_only_is_rejected() {
        let mut parser = GreetingParser::new();
        let bytes = [SOCKS_VERSION, 1, SOCKS5_METHOD_USER_PASS];
        let outcome = parser
            .advance(&bytes, limits())
            .expect("ok")
            .expect("terminal");
        assert_eq!(outcome, (GreetingOutcome::NoAcceptableMethod, 3));
    }

    #[test]
    fn wrong_version_is_rejected() {
        let mut parser = GreetingParser::new();
        let err = parser
            .advance(&[0x04, 1, SOCKS5_METHOD_NO_AUTH], limits())
            .expect_err("wrong version");
        assert!(matches!(err.kind, Socks5ErrorKind::WrongVersion));
    }

    #[test]
    fn zero_methods_is_rejected() {
        let mut parser = GreetingParser::new();
        let err = parser
            .advance(&[SOCKS_VERSION, 0], limits())
            .expect_err("zero");
        assert!(matches!(err.kind, Socks5ErrorKind::ZeroMethods));
    }

    #[test]
    fn too_many_methods_is_rejected() {
        let mut parser = GreetingParser::new();
        let nmethods = (Socks5Limits::defaults().method_count_max + 1) as u8;
        let mut bytes = vec![SOCKS_VERSION, nmethods];
        bytes.extend(std::iter::repeat_n(
            SOCKS5_METHOD_NO_AUTH,
            nmethods as usize,
        ));
        let err = parser.advance(&bytes, limits()).expect_err("oversized");
        assert!(matches!(err.kind, Socks5ErrorKind::TooManyMethods));
    }

    #[test]
    fn incremental_one_byte_parsing() {
        let mut parser = GreetingParser::new();
        let bytes = [SOCKS_VERSION, 2, SOCKS5_METHOD_NO_AUTH, 0x80];
        for byte in &bytes[..bytes.len() - 1] {
            assert!(parser.advance(&[*byte], limits()).expect("ok").is_none());
        }
        let outcome = parser
            .advance(&[bytes[bytes.len() - 1]], limits())
            .expect("ok")
            .expect("terminal");
        assert_eq!(outcome, (GreetingOutcome::NoAuthentication, 4));
    }

    #[test]
    fn greeting_only_consumes_greeting_bytes() {
        let mut parser = GreetingParser::new();
        let mut bytes = vec![SOCKS_VERSION, 1, SOCKS5_METHOD_NO_AUTH];
        // Append an early request byte (VER=0x05) which must remain
        // for the request parser.
        bytes.push(SOCKS_VERSION);
        let (outcome, consumed) = parser
            .advance(&bytes, limits())
            .expect("ok")
            .expect("terminal");
        assert_eq!(outcome, GreetingOutcome::NoAuthentication);
        assert_eq!(consumed, 3);
        assert_eq!(parser.retained().len(), 3);
    }
}
