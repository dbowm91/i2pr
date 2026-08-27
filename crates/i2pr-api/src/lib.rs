//! Application-protocol adapter layer for the `i2pr` router.
//!
//! Plan 136 lands the SAM 3.1 protocol and private-destination foundation.
//! This crate owns the strict bounded line/command/reply parser, the
//! typed version-negotiation model, the SAM Base64 codec, and the
//! `DEST GENERATE` / `SESSION CREATE` private-destination import/export
//! surface required by Milestone 7.
//!
//! The crate owns no sockets, timers, channels, or task runtimes. It
//! stays runtime-neutral; Plan 137 will wire it into the supervised
//! loopback listener through `i2pr-daemon`.
//!
//! # Layering
//!
//! ```text
//! i2pr-proto / i2pr-crypto
//!          ↑
//!      i2pr-client
//!          ↑
//!       i2pr-api
//!          ↑
//!     i2pr-daemon  (Plan 137)
//! ```
//!
//! `i2pr-client` must never depend on `i2pr-api`. The SAM command
//! names, reply strings, parsing rules, and key containers live here
//! and in the daemon composition root — never in `i2pr-client`.

#![forbid(unsafe_code)]

pub mod sam;

pub use sam::{
    command::{
        Command, CommandKind, CommandOutcome, MissingOption, SessionStyle, Silently,
        StreamAcceptId, UnknownCommand, UnknownOption, UnsupportedStyle,
    },
    dest_generate::{
        DEST_GENERATE_SIGNATURE_TYPE_ED25519, DestGenerate, DestGenerateError, DestGenerateOutcome,
        DestGenerateRequest, DestGenerateSignatureType, dest_generate,
    },
    parser::{ParseError, parse_line},
    private_destination::{
        PRIV_LENGTH, PUB_LENGTH, SamPrivateDestination, SamPrivateDestinationError,
    },
    reply::{
        DestReply, HelloReply, NamingReply, PongReply, Reply, ReplyLine, SessionStatus,
        StreamStatus,
    },
    session_create::{
        SessionCreateError, SessionCreateRequest, SessionCreateStyle, parse_session_create,
    },
    version::{
        MAX_SUPPORTED_VERSION, MIN_SUPPORTED_VERSION, NegotiatedVersion, SamVersion,
        SamVersionParseError, negotiate, parse_version,
    },
};
