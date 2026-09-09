//! Plan 178 runtime-neutral IRC client filter module.
//!
//! ```text
//! local IRC client
//!   -> loopback i2pr IRC client listener
//!   -> bounded IRC/IRCv3 line parser + privacy filter
//!   -> fixed configured I2P IRC destination
//!   -> existing Streaming path
//! ```
//!
//! The module owns:
//!
//! - parser bounds (core line 512 bytes, tag envelope 8191 bytes,
//!   client tag data 4094 bytes, per-direction retained buffer
//!   8192 bytes, generated line 8192 bytes, tag count 128, tag key
//!   64 bytes);
//! - IRCv3 message-tag framing with structural validation and
//!   per-tag and cumulative tag-value ceilings;
//! - typed command classification and a per-direction allowlist
//!   covering the Plan 178 §4 set;
//! - client-to-network privacy rewrites for `USER`, `PING`,
//!   `QUIT`, `PART`;
//! - `PRIVMSG`/`NOTICE` CTCP/DCC policy: allow `ACTION`, drop
//!   malformed/multi-delimiter messages, drop address-bearing
//!   `DCC`, drop other CTCP requests by default;
//! - bounded typed errors and per-connection PING/PONG rewrite
//!   token state.
//!
//! The module is runtime-neutral: no Tokio, no sockets, no
//! timers, no filesystem, no transport internals. Higher
//! socket/Streaming/byte-pump concerns belong to the daemon.

#![forbid(unsafe_code)]

pub mod client_filter;
pub mod config;
pub mod errors;
pub mod limits;
pub mod line;
pub mod policy;
pub mod tags;

pub use client_filter::{
    FilterOutcome, IrcDropReason, PingRewriteState, PrivacySubstitutions, build_pong_for_state,
    classify_and_decide, decide_client_to_server, decide_server_to_client,
};
pub use config::{
    DEFAULT_PING_LOCATION, DEFAULT_QUIT_REASON, DEFAULT_USER_HOSTNAME, DEFAULT_USER_SERVERNAME,
    IRC_CLIENT_TAG_DATA_MAX_BYTES, IRC_CORE_MAX_BYTES, IRC_GENERATED_LINE_MAX_BYTES,
    IRC_LINE_BUFFER_MAX_BYTES, IRC_OPTIONS_MAX_ALLOWED_HOSTS, IRC_TAG_COUNT_MAX,
    IRC_TAG_ENVELOPE_MAX_BYTES, IRC_TAG_KEY_MAX_BYTES, IrcClientOptions, IrcCommand,
    ReasonRewritePolicy,
};
pub use errors::{IrcError, IrcErrorKind, IrcLimits};
pub use line::{
    IrcLineParser, LineParserOutcome, classify_post_tag_core, is_command_allowed, pong_from_state,
};
pub use policy::{IrcCommandClass, LineDirection, ParsedLine, classify_core, is_allowed};
pub use tags::{IrcTag, TagsOutcome, TagsParser};
