//! Typed SAM v3.1 command surface.
//!
//! Plan 136 recognises the Milestone 7 command vocabulary even when
//! later plans own execution. Recognition does **not** imply feature
//! support; the typed model carries enough information for later
//! handlers to distinguish:
//!
//! - known/supported baseline command;
//! - known command with unsupported style/version/option;
//! - unknown command/action;
//! - malformed command.

use core::fmt;

use crate::sam::version::SamVersion;

use super::{MAX_SAM_NAME_BYTES, MAX_SAM_OPTION_VALUE_BYTES, MAX_SAM_SESSION_ID_BYTES};

/// Outcome of attempting to recognise a SAM command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandOutcome {
    /// The input matched a supported baseline command.
    Recognised(Command),
    /// The input matched a known command family but used an
    /// unsupported style, version, or option.
    Unsupported(Unsupported),
    /// The input had a recognised command name but an unknown action
    /// (e.g. `SESSION DELETE`).
    UnknownAction(UnknownCommand),
    /// The input named a command that the SAM 3.1 baseline does not
    /// implement at all.
    UnknownCommand(UnknownCommand),
    /// The parser could not classify the input as any of the above.
    Malformed(MalformedCommand),
}

impl CommandOutcome {
    /// Returns the recognised command, if any.
    pub const fn command(&self) -> Option<&Command> {
        match self {
            Self::Recognised(command) => Some(command),
            _ => None,
        }
    }
}

/// A malformed command, retaining the original reason.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MalformedCommand {
    /// Why the command was rejected.
    pub reason: MalformedReason,
}

/// Reasons a SAM command can be malformed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MalformedReason {
    /// The line exceeded `MAX_SAM_LINE_BYTES`.
    LineTooLong,
    /// The token count exceeded `MAX_SAM_TOKENS`.
    TooManyTokens,
    /// A required option was absent.
    MissingRequiredOption(MissingOption),
    /// A critical option appeared twice.
    DuplicateOption(DuplicateOption),
    /// The quoted value was malformed (missing closing quote, trailing
    /// escape, etc.).
    InvalidQuoting,
    /// An option key/value violated the byte-length ceiling.
    OptionTooLong,
    /// The version literal was malformed.
    BadVersion,
}

/// A required option that was missing from the command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissingOption {
    /// `STYLE=` is required for `SESSION CREATE`.
    SessionStyle,
    /// `ID=` is required for `SESSION CREATE`.
    SessionId,
    /// `DESTINATION=` is required for `SESSION CREATE` (when not
    /// `TRANSIENT`).
    SessionDestination,
}

/// A critical option that appeared twice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DuplicateOption {
    /// Duplicate `ID=`.
    Id,
    /// Duplicate `DESTINATION=`.
    Destination,
    /// Duplicate `MIN=`.
    Min,
    /// Duplicate `MAX=`.
    Max,
    /// Duplicate `SIGNATURE_TYPE=`.
    SignatureType,
    /// Duplicate `STYLE=`.
    Style,
    /// Duplicate `NAME=`.
    Name,
    /// Duplicate `SILENT=`.
    Silent,
}

/// A SAM 3.1 command family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandKind {
    /// `HELLO VERSION`.
    HelloVersion,
    /// `DEST GENERATE`.
    DestGenerate,
    /// `SESSION CREATE`.
    SessionCreate,
    /// `STREAM CONNECT`.
    StreamConnect,
    /// `STREAM ACCEPT`.
    StreamAccept,
    /// `STREAM FORWARD`.
    StreamForward,
    /// `NAMING LOOKUP`.
    NamingLookup,
    /// `PING`.
    Ping,
    /// `PONG` (client-to-server only; unsolicited PONG is a
    /// protocol error).
    Pong,
    /// `QUIT` / `STOP` / `EXIT`.
    Quit,
}

impl CommandKind {
    /// Returns the canonical command name (uppercase).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HelloVersion => "HELLO VERSION",
            Self::DestGenerate => "DEST GENERATE",
            Self::SessionCreate => "SESSION CREATE",
            Self::StreamConnect => "STREAM CONNECT",
            Self::StreamAccept => "STREAM ACCEPT",
            Self::StreamForward => "STREAM FORWARD",
            Self::NamingLookup => "NAMING LOOKUP",
            Self::Ping => "PING",
            Self::Pong => "PONG",
            Self::Quit => "QUIT",
        }
    }
}

/// A typed SAM 3.1 command, including its option pairs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Command {
    kind: CommandKind,
    options: Vec<OptionPair>,
    action_target: Option<String>,
}

impl Command {
    /// Constructs a typed command.
    pub fn new(kind: CommandKind, options: Vec<OptionPair>, action_target: Option<String>) -> Self {
        Self {
            kind,
            options,
            action_target,
        }
    }

    /// Returns the command family.
    pub const fn kind(&self) -> CommandKind {
        self.kind
    }

    /// Returns the command's option pairs in canonical insertion order.
    pub fn options(&self) -> &[OptionPair] {
        &self.options
    }

    /// Returns the action target (the argument following the command
    /// name, if any).
    pub fn action_target(&self) -> Option<&str> {
        self.action_target.as_deref()
    }

    /// Returns the value of an option, byte-equal to what the parser
    /// observed (case-preserved).
    pub fn value(&self, key: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|pair| pair.key.eq_ignore_ascii_case(key))
            .map(|pair| pair.value.as_str())
    }
}

/// A single `KEY=VALUE` option pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionPair {
    key: String,
    value: String,
}

impl OptionPair {
    /// Constructs a new option pair from already-validated strings.
    pub fn new(key: String, value: String) -> Self {
        Self { key, value }
    }

    /// Returns the option key (preserving case).
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the option value (preserving case).
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Display for CommandKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A recognised but unsupported command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unsupported {
    /// Which command was recognised.
    pub kind: CommandKind,
    /// Why it was rejected.
    pub reason: UnsupportedReason,
}

/// Why a known command was rejected as unsupported.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UnsupportedReason {
    /// `SESSION CREATE STYLE=` was not `STREAM` in the M7 baseline.
    UnsupportedSessionStyle(UnsupportedStyle),
    /// `SIGNATURE_TYPE=` was not the supported type.
    UnsupportedSignatureType(String),
    /// `NAMING LOOKUP OPTIONS=true` is outside the M7 3.1 baseline.
    NamingLookupOptions,
    /// A `STREAM CONNECT` carried a 3.2-only port option.
    StreamConnectPortOptionUnsupported,
}

/// Style values for `SESSION CREATE` (Plan 136 baseline supports
/// `STREAM` only).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionStyle {
    /// `STREAM` (the M7 supported baseline).
    Stream,
    /// A non-stream style that is recognised but not implemented.
    Other,
}

/// Typed value for `SILENT=`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Silently {
    /// `SILENT=true`.
    Yes,
    /// `SILENT=false`.
    No,
}

impl Silently {
    /// Parses the SAM boolean spelling. Accepts the literal strings
    /// `true` and `false` case-insensitively.
    pub fn parse(input: &str) -> Option<Self> {
        if input.eq_ignore_ascii_case("true") {
            Some(Self::Yes)
        } else if input.eq_ignore_ascii_case("false") {
            Some(Self::No)
        } else {
            None
        }
    }
}

/// A non-stream `STYLE=` value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnsupportedStyle(pub String);

/// An unknown command or unknown action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownCommand {
    /// The full command word the parser observed, uppercased.
    pub observed: String,
}

/// Unknown option retained for later explicit rejection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownOption(pub OptionPair);

/// Typed helper that asserts the supplied `ID=` value fits the SAM
/// session-id byte ceiling.
pub fn validate_session_id(value: &str) -> Result<(), MalformedReason> {
    if value.is_empty() || value.len() > MAX_SAM_SESSION_ID_BYTES {
        return Err(MalformedReason::OptionTooLong);
    }
    Ok(())
}

/// Typed helper that asserts the supplied `NAME=` value fits the SAM
/// name byte ceiling.
pub fn validate_name(value: &str) -> Result<(), MalformedReason> {
    if value.is_empty() || value.len() > MAX_SAM_NAME_BYTES {
        return Err(MalformedReason::OptionTooLong);
    }
    Ok(())
}

/// Typed helper that asserts the supplied generic option value fits
/// the option-value byte ceiling.
pub fn validate_option_value(value: &str) -> Result<(), MalformedReason> {
    if value.len() > MAX_SAM_OPTION_VALUE_BYTES {
        return Err(MalformedReason::OptionTooLong);
    }
    Ok(())
}

/// Typed helper that extracts the `MIN=` and `MAX=` versions from a
/// `HELLO VERSION` command, returning `BadVersion` if either is
/// malformed.
pub fn extract_versions(
    command: &Command,
) -> Result<(Option<SamVersion>, Option<SamVersion>), MalformedReason> {
    let min = match command.value("MIN") {
        Some(text) => Some(
            crate::sam::version::parse_version(text).map_err(|_| MalformedReason::BadVersion)?,
        ),
        None => None,
    };
    let max = match command.value("MAX") {
        Some(text) => Some(
            crate::sam::version::parse_version(text).map_err(|_| MalformedReason::BadVersion)?,
        ),
        None => None,
    };
    Ok((min, max))
}

/// Typed stream-id validator. The SAM 3.1 stream-id is an unsigned
/// 32-bit integer but typically fits in a `u16`.
pub type StreamAcceptId = u32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silently_parses_case_insensitive() {
        assert_eq!(Silently::parse("true"), Some(Silently::Yes));
        assert_eq!(Silently::parse("TRUE"), Some(Silently::Yes));
        assert_eq!(Silently::parse("false"), Some(Silently::No));
        assert_eq!(Silently::parse("False"), Some(Silently::No));
        assert_eq!(Silently::parse("yes"), None);
    }

    #[test]
    fn session_id_validation_rejects_overlong_inputs() {
        let overlong = "x".repeat(MAX_SAM_SESSION_ID_BYTES + 1);
        assert!(matches!(
            validate_session_id(&overlong),
            Err(MalformedReason::OptionTooLong)
        ));
        assert!(matches!(
            validate_session_id(""),
            Err(MalformedReason::OptionTooLong)
        ));
        assert!(validate_session_id("my-session").is_ok());
    }

    #[test]
    fn value_lookup_is_case_insensitive_on_keys() {
        let command = Command::new(
            CommandKind::SessionCreate,
            vec![OptionPair::new("ID".to_owned(), "alpha".to_owned())],
            None,
        );
        assert_eq!(command.value("ID"), Some("alpha"));
        assert_eq!(command.value("id"), Some("alpha"));
        assert_eq!(command.value("Id"), Some("alpha"));
    }
}
