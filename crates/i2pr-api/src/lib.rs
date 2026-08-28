//! Application-protocol adapter layer for the `i2pr` router.
//!
//! Plan 136 lands the SAM 3.1 protocol and private-destination foundation.
//! Plan 137 extends `i2pr-api` with the runtime-neutral loopback-server
//! surface (server state machine, session registry, line reader, and
//! service limits). This crate owns the strict bounded line/command/
//! reply parser, the typed version-negotiation model, the SAM Base64
//! codec, the `DEST GENERATE` / `SESSION CREATE` private-destination
//! import/export surface, and the bounded session/registry surface
//! that `i2pr-daemon` composes into a supervised TCP service.
//!
//! The crate owns no sockets, timers, channels, or task runtimes. It
//! stays runtime-neutral; `i2pr-daemon` wires it into the
//! supervised loopback listener.
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
//!     i2pr-daemon (Plan 137)
//! ```
//!
//! `i2pr-client` must never depend on `i2pr-api`. The SAM command
//! names, reply strings, parsing rules, key containers, session
//! registry, and loopback-server state surface live here and in the
//! daemon composition root — never in `i2pr-client`.

#![forbid(unsafe_code)]

pub mod sam;

pub use sam::{
    command::{
        Command, CommandKind, CommandOutcome, MissingOption, SessionStyle, Silently,
        StreamAcceptError, StreamAcceptId, StreamAcceptRequest, StreamConnectError,
        StreamConnectRequest, UnknownCommand, UnknownOption, UnsupportedStyle, parse_stream_accept,
        parse_stream_connect,
    },
    dest_generate::{
        DEST_GENERATE_SIGNATURE_TYPE_ED25519, DestGenerate, DestGenerateError, DestGenerateOutcome,
        DestGenerateRequest, DestGenerateSignatureType, dest_generate,
    },
    limits::{
        DEFAULT_SAM_BIND_ADDRESS, DEFAULT_SAM_COMMAND_TIMEOUT_MS, DEFAULT_SAM_ENABLED,
        DEFAULT_SAM_HELLO_TIMEOUT_MS, DEFAULT_SAM_MAX_CLIENTS, DEFAULT_SAM_MAX_SESSIONS,
        DEFAULT_SAM_PENDING_ACCEPTS_PER_SESSION, DEFAULT_SAM_PORT, DEFAULT_SAM_SHUTDOWN_TIMEOUT_MS,
        DEFAULT_SAM_STREAM_SOCKETS_PER_SESSION, MAX_SAM_BUFFERED_BYTES_PER_STREAM_DIRECTION,
        MAX_SAM_CLIENTS, MAX_SAM_COMMAND_TIMEOUT_SECS, MAX_SAM_HELLO_TIMEOUT_SECS,
        MAX_SAM_PENDING_ACCEPTS_PER_SESSION, MAX_SAM_SESSIONS, MAX_SAM_SHUTDOWN_TIMEOUT_SECS,
        MAX_SAM_STREAM_SOCKETS_PER_SESSION, SamLimits, SamLimitsError,
    },
    line_reader::{LineEvent, LineReader, LineReaderError},
    parser::{ParseError, parse_line},
    private_destination::{
        PRIV_LENGTH, PUB_LENGTH, SamPrivateDestination, SamPrivateDestinationError,
    },
    registry::{
        ControlOwnerState, SamSessionEntry, SamSessionRegistry, SamSessionRegistryError,
        SamSessionReservation,
    },
    reply::{
        DestReply, HelloReply, NamingReply, PongReply, Reply, ReplyLine, SessionStatus,
        StreamStatus,
    },
    server_state::{
        CloseReason, DispatchOutcome, ServerConnectionState, SessionCreateApplied,
        SessionCreateFailed, StreamAcceptApplied, StreamAcceptFailed, StreamConnectApplied,
        StreamConnectFailed, apply_session_outcome, apply_stream_accept_outcome,
        apply_stream_connect_outcome, dispatch,
    },
    session::{SamSessionCounters, SamSessionCountersError, SamSessionId},
    session_create::{
        SessionCreateError, SessionCreateRequest, SessionCreateStyle, parse_session_create,
    },
    streams::{
        SamAcceptWaiter, SamOutboundAttachment, SamStreamAttachment, SamStreamDirection,
        SamStreamEntry, SamStreamRegistry, SamStreamRegistryError, SamStreamRegistryHandle,
        SamStreamState,
    },
    version::{
        MAX_SUPPORTED_VERSION, MIN_SUPPORTED_VERSION, NegotiatedVersion, SamVersion,
        SamVersionParseError, negotiate, parse_version,
    },
};
