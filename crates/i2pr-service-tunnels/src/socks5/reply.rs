//! Plan 177 SOCKS5 reply generator.
//!
//! Produces a strict, byte-exact RFC 1928 reply whose bound address
//! is the loopback `127.0.0.1:0` neutral value and never leaks
//! destination private material or local router identity. The
//! reply generator is the only place in the M10 SOCKS5 proxy that
//! constructs raw wire output for clients; it must remain
//! conservative.
//!
//! Wire format:
//!
//! ```text
//! +----+-----+-------+------+----------+----------+
//! |VER | REP |  RSV  | ATYP | BND.ADDR | BND.PORT |
//! +----+-----+-------+------+----------+----------+
//! | 1  |  1  | X'00' |  1   | Variable |    2     |
//! +----+-----+-------+------+----------+----------+
//! ```
//!
//! The reply is always 10 bytes (ATYP=0x01 IPv4 + 4-byte
//! `127.0.0.1` + 2-byte big-endian port). It must not echo any
//! client-supplied hostname or address into the BND.ADDR / BND.PORT
//! fields.

#![forbid(unsafe_code)]

use super::errors::Socks5ReplyCode;

/// Length of a single SOCKS5 reply (ATYP=0x01 IPv4, 4-byte
/// 127.0.0.1, 2-byte port).
pub const REPLY_LEN: usize = 10;

/// Default bound address (`127.0.0.1`) used in success replies.
/// Plan 177 §6 forbids exposing destination private material,
/// local router identities, or unrelated listener addresses; the
/// reply must report a neutral loopback bind.
pub const SUCCESS_BND_ADDR: [u8; 4] = [127, 0, 0, 1];
/// Default bound port used in success replies. Zero is RFC-legal
/// ("no bound port known") and remains the safest neutral value
/// until the daemon has a typed reason to advertise anything else.
pub const SUCCESS_BND_PORT: [u8; 2] = [0, 0];

/// Builds one bounded SOCKS5 reply with the supplied code and a
/// neutral loopback bind. The output is exactly 10 bytes and is
/// safe to flush before close.
pub fn build_reply(code: Socks5ReplyCode) -> [u8; REPLY_LEN] {
    let mut reply = [0_u8; REPLY_LEN];
    reply[0] = 0x05; // VER
    reply[1] = code.code(); // REP
    reply[2] = 0x00; // RSV
    reply[3] = 0x01; // ATYP = IPv4
    reply[4..8].copy_from_slice(&SUCCESS_BND_ADDR);
    reply[8..10].copy_from_slice(&SUCCESS_BND_PORT);
    reply
}

/// Builds one bounded SOCKS5 reply from a raw reply-code byte.
/// Used by the request parser terminal state where the byte is
/// produced inline. Validates that the byte is one of the eight
/// RFC 1928 codes; any other byte returns `GeneralFailure` instead.
pub fn build_reply_from_code(code: u8) -> [u8; REPLY_LEN] {
    let mapped = match code {
        0x00 => Socks5ReplyCode::Success,
        0x01 => Socks5ReplyCode::GeneralFailure,
        0x02 => Socks5ReplyCode::ConnectionNotAllowed,
        0x04 => Socks5ReplyCode::HostUnreachable,
        0x05 => Socks5ReplyCode::ConnectionRefused,
        0x06 => Socks5ReplyCode::TtlExpired,
        0x07 => Socks5ReplyCode::CommandNotSupported,
        0x08 => Socks5ReplyCode::AddressTypeNotSupported,
        _ => Socks5ReplyCode::GeneralFailure,
    };
    build_reply(mapped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_reply_is_bounded_and_neutral() {
        let reply = build_reply(Socks5ReplyCode::Success);
        assert_eq!(reply.len(), REPLY_LEN);
        assert_eq!(reply[0], 0x05);
        assert_eq!(reply[1], 0x00);
        assert_eq!(reply[2], 0x00);
        assert_eq!(reply[3], 0x01);
        assert_eq!(&reply[4..8], &[127, 0, 0, 1]);
        assert_eq!(&reply[8..10], &[0, 0]);
    }

    #[test]
    fn rejection_reply_carries_correct_code() {
        for (code, byte) in [
            (Socks5ReplyCode::Success, 0x00),
            (Socks5ReplyCode::GeneralFailure, 0x01),
            (Socks5ReplyCode::ConnectionNotAllowed, 0x02),
            (Socks5ReplyCode::HostUnreachable, 0x04),
            (Socks5ReplyCode::ConnectionRefused, 0x05),
            (Socks5ReplyCode::TtlExpired, 0x06),
            (Socks5ReplyCode::CommandNotSupported, 0x07),
            (Socks5ReplyCode::AddressTypeNotSupported, 0x08),
        ] {
            let reply = build_reply(code);
            assert_eq!(reply[1], byte, "wrong code for {code:?}");
        }
    }

    #[test]
    fn reply_from_raw_byte_validates() {
        let reply = build_reply_from_code(0x07);
        assert_eq!(reply[1], 0x07);
        let reply = build_reply_from_code(0x99);
        assert_eq!(reply[1], 0x01);
    }
}
