//! Plan 178 IRC hard ceilings.
//!
//! All parser/storage ceilings live in one place so every other
//! module can import them by name. The Plan 178 ceilings are tight
//! but conservative against ordinary IRCv3-capable clients.
//!
//! The ceiling constants are defined in [`crate::irc::config`] and
//! the [`IrcLimits`] struct lives in [`crate::irc::errors`]; this
//! module simply re-exports the type so callers do not have to
//! depend on `errors` directly to query the limits.

#![forbid(unsafe_code)]

pub use super::errors::IrcLimits;

pub use super::config::{
    DEFAULT_PING_LOCATION, DEFAULT_QUIT_REASON, DEFAULT_USER_HOSTNAME, DEFAULT_USER_SERVERNAME,
    IRC_CLIENT_TAG_DATA_MAX_BYTES, IRC_CORE_MAX_BYTES, IRC_GENERATED_LINE_MAX_BYTES,
    IRC_LINE_BUFFER_MAX_BYTES, IRC_OPTIONS_MAX_ALLOWED_HOSTS, IRC_TAG_COUNT_MAX,
    IRC_TAG_ENVELOPE_MAX_BYTES, IRC_TAG_KEY_MAX_BYTES,
};
