//! Plan 177 runtime-neutral SOCKS5 `.i2p` CONNECT proxy module.
//!
//! Strict, bounded, SOCKS5 RFC 1928 negotiation + request parser
//! plus the deterministic typed reply generator for the M10
//! `socks5-client` service tunnel profile.
//!
//! The module owns:
//! - parser bounds (method count, retained greeting/request bytes,
//!   domain length, generated reply bytes);
//! - greeting negotiation (VER, NMETHODS, METHODS, no-auth only);
//! - CONNECT request parsing with strict `.i2p` target policy
//!   (Base32 / static-alias only; clearnet, IP literals,
//!   `localhost`, mixed-suffix confusion, control/NUL/whitespace,
//!   and BIND/UDP ASSOCIATE rejected);
//! - Plan 290 bounded SOCKS4a CONNECT parsing (domain extension
//!   only; plain SOCKS4 IPv4 literals fail closed) with the
//!   identical `.i2p`-only domain policy, for the pinned
//!   historical SOCKS 4/4a/5 profile parity;
//! - bounded typed reply generation that never echoes untrusted
//!   request bytes and uses a neutral loopback bind (`127.0.0.1:0`).
//!
//! The module is runtime-neutral: no Tokio, no sockets, no timers,
//! no filesystem, no transport internals. Higher protocol/state
//! concerns (Streaming ownership, loopback TCP, per-connection
//! supervision, sibling-isolated byte pumps) belong to the daemon.
//!
//! ## Mandatory behavior
//!
//! - SOCKS5 version `0x05` for greeting + DOMAINNAME CONNECT, plus
//!   Plan 290 bounded SOCKS4a (`0x04`) CONNECT in domain-extension
//!   form only (plain SOCKS4 IPv4 literals rejected);
//! - `NO AUTHENTICATION REQUIRED (0x00)` only;
//! - `CONNECT (0x01)` command only;
//! - `DOMAINNAME (0x03)` address type only;
//! - reject IPv4 (`0x01`) and IPv6 (`0x04`) address types;
//! - reject clearnet / IP literal / `localhost` / `.localhost`
//!   / mixed-suffix confusion / malformed alias authorities;
//! - reject userinfo, empty domain, port zero, unsupported command;
//! - reject NUL / control / whitespace domain bytes;
//! - never echo untrusted request bytes into replies or logs;
//! - reply uses a neutral loopback `127.0.0.1:0` bind address.
//!
//! ## Reference profile (Plan 177 §2)
//!
//! Clean-room behavior from:
//!
//! - RFC 1928;
//! - current official I2P SOCKS/I2PTunnel documentation;
//! - Java I2P 2.13.0 (`9134f808337b401e8e53c73734c81fab04280c9d`)
//!   reference path
//!   `apps/i2ptunnel/java/src/net/i2p/i2ptunnel/socks/SOCKS5Server.java`.
//!
//! Do not copy Java implementation code. Java's broad SOCKS/auth/
//! UDP/IP/outproxy/Tor-resolve profile is explicitly NOT claimed
//! here.

#![forbid(unsafe_code)]

pub mod config;
pub mod errors;
pub mod limits;
pub mod negotiation;
pub mod reply;
pub mod request;
pub mod socks4a;

pub use config::{
    ConnectPortPolicy, DEFAULT_CONNECT_PORT, SOCKS_VERSION, SOCKS5_CMD_BIND, SOCKS5_CMD_CONNECT,
    SOCKS5_CMD_UDP_ASSOCIATE, SOCKS5_DOMAIN_MAX_BYTES, SOCKS5_GREETING_MAX_BYTES,
    SOCKS5_METHOD_COUNT_MAX, SOCKS5_METHOD_NO_AUTH, SOCKS5_METHOD_USER_PASS,
    SOCKS5_NO_ACCEPTABLE_METHOD_REPLY, SOCKS5_NO_AUTH_REPLY, SOCKS5_OPTIONS_MAX_PORTS,
    SOCKS5_REPLY_MAX_BYTES, SOCKS5_REQUEST_HEADER_MAX_BYTES, SOCKS5_RESERVED,
    SOCKS5_RETAINED_BUFFER_MAX_BYTES, Socks5ClientOptions,
};
pub use errors::{Socks5Error, Socks5ErrorKind, Socks5ReplyCode};
pub use limits::Socks5Limits;
pub use negotiation::{GreetingOutcome, GreetingParser};
pub use reply::{REPLY_LEN, build_reply, build_reply_from_code};
pub use request::{
    ConnectDestination, PORT_FIELD_LEN, REQUEST_HEADER_LEN, RequestOutcome, RequestParser,
};
pub use socks4a::{
    SOCKS4A_GRANTED, SOCKS4A_REJECTED, SOCKS4A_REPLY_LEN, Socks4aOutcome, Socks4aRequestParser,
    build_socks4a_reply,
};
