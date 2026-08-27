//! SAM v3.1 protocol surface.
//!
//! Strict bounded command/reply model, version negotiation, Base64
//! codec, and private-destination format. Plan 136 only — no socket
//! or session lifecycle ownership.

pub mod base64;
pub mod command;
pub mod dest_generate;
pub mod parser;
pub mod private_destination;
pub mod reply;
pub mod session_create;
pub mod version;

/// Maximum byte length of a single SAM command line, before the
/// terminating newline.
pub const MAX_SAM_LINE_BYTES: usize = 8192;

/// Maximum number of whitespace-separated tokens in a single SAM line.
pub const MAX_SAM_TOKENS: usize = 64;

/// Maximum number of `KEY=VALUE` option pairs in a single SAM line.
pub const MAX_SAM_OPTIONS: usize = 32;

/// Maximum UTF-8 byte length of a quoted or unquoted option value.
pub const MAX_SAM_OPTION_VALUE_BYTES: usize = 4096;

/// Maximum UTF-8 byte length of a SAM session identifier (`ID=`).
pub const MAX_SAM_SESSION_ID_BYTES: usize = 256;

/// Maximum UTF-8 byte length of a SAM `NAME=` value.
pub const MAX_SAM_NAME_BYTES: usize = 256;

/// Maximum byte length of a SAM Base64-encoded `PRIV` value (608 +
/// margin for the standard `=` padding).
pub const MAX_SAM_PRIV_TEXT_BYTES: usize = 1024;

/// Maximum byte length of a SAM Base64-encoded `PUB` value (524 +
/// margin).
pub const MAX_SAM_PUB_TEXT_BYTES: usize = 1024;
