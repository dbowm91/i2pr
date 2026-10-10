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
    /// A STREAM I2P port was not a decimal value in 0..=65535.
    InvalidPort,
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
    /// Duplicate `FROM_PORT=`.
    FromPort,
    /// Duplicate `TO_PORT=`.
    ToPort,
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
    /// Recognized but unsupported `SESSION ADD`.
    SessionAdd,
    /// Recognized but unsupported `SESSION REMOVE`.
    SessionRemove,
    /// Recognized but unsupported `AUTH`.
    Auth,
    /// Recognized but unsupported `DATAGRAM`.
    Datagram,
    /// Recognized but unsupported `RAW`.
    Raw,
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
            Self::SessionAdd => "SESSION ADD",
            Self::SessionRemove => "SESSION REMOVE",
            Self::Auth => "AUTH",
            Self::Datagram => "DATAGRAM",
            Self::Raw => "RAW",
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
    /// `STREAM FORWARD SSL=true` belongs to SAM 3.2+.
    StreamForwardSsl,
    /// A `STREAM CONNECT` carried a 3.2-only port option.
    StreamConnectPortOptionUnsupported,
    /// A recognized command family is outside the M7 baseline.
    UnsupportedCommandFamily(String),
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

/// Typed failure of parsing a `STREAM CONNECT` request from a
/// `Command`. The failure reason is mapped to a SAM reply by the
/// daemon.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamConnectError {
    /// `ID=` was missing from the command.
    MissingId,
    /// `DESTINATION=` was missing from the command.
    MissingDestination,
    /// `SILENT=` was present but did not parse to `true` or `false`.
    InvalidSilent(String),
    /// `FROM_PORT=` was present but was not a decimal I2P port in 0..=65535.
    InvalidFromPort(String),
    /// `TO_PORT=` was present but was not a decimal I2P port in 0..=65535.
    InvalidToPort(String),
    /// `ID=` was empty or longer than the SAM session-id byte ceiling.
    InvalidId,
}

impl core::fmt::Display for StreamConnectError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingId => formatter.write_str("STREAM CONNECT missing ID"),
            Self::MissingDestination => formatter.write_str("STREAM CONNECT missing DESTINATION"),
            Self::InvalidSilent(value) => {
                write!(formatter, "STREAM CONNECT invalid SILENT={value}")
            }
            Self::InvalidFromPort(value) => {
                write!(formatter, "STREAM CONNECT invalid FROM_PORT={value}")
            }
            Self::InvalidToPort(value) => {
                write!(formatter, "STREAM CONNECT invalid TO_PORT={value}")
            }
            Self::InvalidId => formatter.write_str("STREAM CONNECT invalid ID"),
        }
    }
}

/// Parsed `STREAM CONNECT` request. The daemon drives the actual
/// `StreamingManager::connect(...)` call; this struct carries only the
/// validated wire fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamConnectRequest {
    /// `ID=` value identifying the target session.
    pub session_id: String,
    /// `DESTINATION=` value, byte-equal to the wire text. The daemon
    /// decodes it through the existing destination codec.
    pub destination: String,
    /// `SILENT=` value (`true` / `false`). `None` when the option was
    /// absent (the SAM default is non-silent).
    pub silent: Option<bool>,
    /// Local I2P source port override. `None` inherits SESSION CREATE.
    pub from_port: Option<u16>,
    /// Remote I2P destination port override. `None` inherits SESSION CREATE.
    pub to_port: Option<u16>,
}

/// Typed failure of parsing a `STREAM ACCEPT` request from a `Command`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamAcceptError {
    /// `ID=` was missing from the command.
    MissingId,
    /// `SILENT=` was present but did not parse to `true` or `false`.
    InvalidSilent(String),
    /// `ID=` was empty or longer than the SAM session-id byte ceiling.
    InvalidId,
}

impl core::fmt::Display for StreamAcceptError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingId => formatter.write_str("STREAM ACCEPT missing ID"),
            Self::InvalidSilent(value) => {
                write!(formatter, "STREAM ACCEPT invalid SILENT={value}")
            }
            Self::InvalidId => formatter.write_str("STREAM ACCEPT invalid ID"),
        }
    }
}

/// Parsed `STREAM ACCEPT` request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamAcceptRequest {
    /// `ID=` value identifying the target session.
    pub session_id: String,
    /// `SILENT=` value (`true` / `false`). `None` when the option was
    /// absent.
    pub silent: Option<bool>,
}

/// Typed SAM 3.3 `SESSION ADD` subsession request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionAddRequest {
    /// Globally unique child session ID.
    pub id: String,
    /// Child protocol style.
    pub style: crate::sam::session_create::SessionCreateStyle,
    /// Outbound I2P source port.
    pub from_port: u16,
    /// Outbound I2P destination port.
    pub to_port: u16,
    /// Optional host UDP port for ordinary SAM bridge clients. Private managed-app
    /// data operations never acquire or forward to this host endpoint.
    pub port: Option<u16>,
    /// Optional loopback host for ordinary UDP forwarding. DNS names other
    /// than `localhost` are rejected so the daemon never performs resolution.
    pub host: Option<std::net::IpAddr>,
    /// Whether RAW datagrams forwarded to the UDP endpoint include I2CP
    /// protocol and port metadata.
    pub raw_header: bool,
    /// Outbound I2CP protocol for RAW children; fixed by STYLE otherwise.
    pub protocol: u8,
    /// Inbound I2CP listen protocol for RAW children; fixed by STYLE otherwise.
    pub listen_protocol: u8,
    /// Local I2P receive port; defaults to FROM_PORT for child sessions.
    pub listen_port: u16,
}

/// Typed `SESSION ADD` validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionAddError {
    /// Required ID or STYLE is missing.
    MissingField,
    /// ID is invalid.
    InvalidId,
    /// Style is unsupported for subsessions.
    InvalidStyle,
    /// A port field is not decimal or exceeds 65535.
    InvalidPort,
    /// Datagram styles require PORT while STREAM forbids it.
    PortStyleMismatch,
    /// HOST is not a loopback IP literal or `localhost`.
    InvalidHost,
    /// HEADER is malformed or used with a non-RAW style.
    InvalidHeader,
    /// RAW protocol is not a supported value, or the selected field forbids it.
    InvalidProtocol,
    /// UDP binding is configured once for the daemon, not per subsession.
    UnsupportedUdpBinding,
    /// SESSION ADD must reuse the PRIMARY Destination.
    DestinationNotAllowed,
}

/// Parses a validated `SESSION ADD` command.
pub fn parse_session_add(command: &Command) -> Result<SessionAddRequest, SessionAddError> {
    if command.value("DESTINATION").is_some() {
        return Err(SessionAddError::DestinationNotAllowed);
    }
    if command.value("sam.udp.host").is_some() || command.value("sam.udp.port").is_some() {
        return Err(SessionAddError::UnsupportedUdpBinding);
    }
    let id = command.value("ID").ok_or(SessionAddError::MissingField)?;
    if id.is_empty() || id.len() > MAX_SAM_SESSION_ID_BYTES {
        return Err(SessionAddError::InvalidId);
    }
    let style = command
        .value("STYLE")
        .and_then(crate::sam::session_create::SessionCreateStyle::parse)
        .filter(|style| {
            matches!(
                style,
                crate::sam::session_create::SessionCreateStyle::Stream
                    | crate::sam::session_create::SessionCreateStyle::Datagram
                    | crate::sam::session_create::SessionCreateStyle::Datagram2
                    | crate::sam::session_create::SessionCreateStyle::Datagram3
                    | crate::sam::session_create::SessionCreateStyle::Raw
            )
        })
        .ok_or(SessionAddError::InvalidStyle)?;
    let port = command
        .value("PORT")
        .map(|value| parse_sam_port(value).ok_or(SessionAddError::InvalidPort))
        .transpose()?;
    let host = command.value("HOST").map(parse_loopback_host).transpose()?;
    if host.is_some()
        && !matches!(
            style,
            crate::sam::session_create::SessionCreateStyle::Datagram
                | crate::sam::session_create::SessionCreateStyle::Datagram2
                | crate::sam::session_create::SessionCreateStyle::Datagram3
                | crate::sam::session_create::SessionCreateStyle::Raw
        )
    {
        return Err(SessionAddError::InvalidHost);
    }
    let raw_header = match command.value("HEADER") {
        None | Some("false") => false,
        Some("true") if style == crate::sam::session_create::SessionCreateStyle::Raw => true,
        Some("true") => return Err(SessionAddError::InvalidHeader),
        Some(_) => return Err(SessionAddError::InvalidHeader),
    };
    if command.value("HEADER").is_some()
        && style != crate::sam::session_create::SessionCreateStyle::Raw
    {
        return Err(SessionAddError::InvalidHeader);
    }
    let raw_style = style == crate::sam::session_create::SessionCreateStyle::Raw;
    if !raw_style
        && (command.value("PROTOCOL").is_some() || command.value("LISTEN_PROTOCOL").is_some())
    {
        return Err(SessionAddError::InvalidProtocol);
    }
    let (protocol, listen_protocol) = if raw_style {
        let protocol = command
            .value("PROTOCOL")
            .map(parse_raw_protocol)
            .transpose()?
            .unwrap_or(18);
        let listen_protocol = command
            .value("LISTEN_PROTOCOL")
            .map(parse_raw_listen_protocol)
            .transpose()?
            .unwrap_or(protocol);
        (protocol, listen_protocol)
    } else {
        let protocol = match style {
            crate::sam::session_create::SessionCreateStyle::Stream => {
                i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER
            }
            crate::sam::session_create::SessionCreateStyle::Datagram => {
                i2pr_proto::PROTOCOL_TYPE_DATAGRAM
            }
            crate::sam::session_create::SessionCreateStyle::Datagram2 => {
                i2pr_proto::PROTOCOL_TYPE_DATAGRAM2
            }
            crate::sam::session_create::SessionCreateStyle::Datagram3 => {
                i2pr_proto::PROTOCOL_TYPE_DATAGRAM3
            }
            crate::sam::session_create::SessionCreateStyle::Raw
            | crate::sam::session_create::SessionCreateStyle::Primary => unreachable!(),
        };
        (protocol, protocol)
    };
    if matches!(
        style,
        crate::sam::session_create::SessionCreateStyle::Datagram
            | crate::sam::session_create::SessionCreateStyle::Datagram2
            | crate::sam::session_create::SessionCreateStyle::Datagram3
            | crate::sam::session_create::SessionCreateStyle::Raw
    ) != port.is_some()
    {
        return Err(SessionAddError::PortStyleMismatch);
    }
    let from_port = command
        .value("FROM_PORT")
        .map(|value| parse_sam_port(value).ok_or(SessionAddError::InvalidPort))
        .transpose()?
        .unwrap_or(0);
    let to_port = command
        .value("TO_PORT")
        .map(|value| parse_sam_port(value).ok_or(SessionAddError::InvalidPort))
        .transpose()?
        .unwrap_or(0);
    let listen_port = command
        .value("LISTEN_PORT")
        .map(|value| parse_sam_port(value).ok_or(SessionAddError::InvalidPort))
        .transpose()?
        .unwrap_or(from_port);
    if style == crate::sam::session_create::SessionCreateStyle::Stream
        && listen_port != 0
        && listen_port != from_port
    {
        return Err(SessionAddError::PortStyleMismatch);
    }
    Ok(SessionAddRequest {
        id: id.to_owned(),
        style,
        from_port,
        to_port,
        port,
        host,
        raw_header,
        protocol,
        listen_protocol,
        listen_port,
    })
}

fn parse_raw_protocol(value: &str) -> Result<u8, SessionAddError> {
    let protocol = value
        .parse::<u8>()
        .map_err(|_| SessionAddError::InvalidProtocol)?;
    (!matches!(protocol, 6 | 17 | 19 | 20))
        .then_some(protocol)
        .ok_or(SessionAddError::InvalidProtocol)
}

fn parse_raw_listen_protocol(value: &str) -> Result<u8, SessionAddError> {
    let protocol = value
        .parse::<u8>()
        .map_err(|_| SessionAddError::InvalidProtocol)?;
    (protocol != 6)
        .then_some(protocol)
        .ok_or(SessionAddError::InvalidProtocol)
}

fn parse_loopback_host(value: &str) -> Result<std::net::IpAddr, SessionAddError> {
    if value.eq_ignore_ascii_case("localhost") {
        return Ok(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    }
    value
        .parse::<std::net::IpAddr>()
        .ok()
        .filter(std::net::IpAddr::is_loopback)
        .ok_or(SessionAddError::InvalidHost)
}

/// Extracts a `StreamConnectRequest` from a `STREAM CONNECT` command.
/// Returns a typed error on any malformed field; the daemon maps the
/// error to the SAM `ReplyResult` vocabulary.
pub fn parse_stream_connect(command: &Command) -> Result<StreamConnectRequest, StreamConnectError> {
    let session_id = command
        .value("ID")
        .ok_or(StreamConnectError::MissingId)?
        .to_owned();
    if session_id.is_empty() || session_id.len() > MAX_SAM_SESSION_ID_BYTES {
        return Err(StreamConnectError::InvalidId);
    }
    let destination = command
        .value("DESTINATION")
        .ok_or(StreamConnectError::MissingDestination)?
        .to_owned();
    let silent = match command.value("SILENT") {
        None => None,
        Some(value) => match Silently::parse(value) {
            Some(Silently::Yes) => Some(true),
            Some(Silently::No) => Some(false),
            None => return Err(StreamConnectError::InvalidSilent(value.to_owned())),
        },
    };
    let from_port = command
        .value("FROM_PORT")
        .map(|value| {
            parse_sam_port(value)
                .ok_or_else(|| StreamConnectError::InvalidFromPort(value.to_owned()))
        })
        .transpose()?;
    let to_port = command
        .value("TO_PORT")
        .map(|value| {
            parse_sam_port(value).ok_or_else(|| StreamConnectError::InvalidToPort(value.to_owned()))
        })
        .transpose()?;
    Ok(StreamConnectRequest {
        session_id,
        destination,
        silent,
        from_port,
        to_port,
    })
}

pub(super) fn parse_sam_port(value: &str) -> Option<u16> {
    if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        value.parse::<u16>().ok()
    } else {
        None
    }
}

/// Extracts a `StreamAcceptRequest` from a `STREAM ACCEPT` command.
pub fn parse_stream_accept(command: &Command) -> Result<StreamAcceptRequest, StreamAcceptError> {
    let session_id = command
        .value("ID")
        .ok_or(StreamAcceptError::MissingId)?
        .to_owned();
    if session_id.is_empty() || session_id.len() > MAX_SAM_SESSION_ID_BYTES {
        return Err(StreamAcceptError::InvalidId);
    }
    let silent = match command.value("SILENT") {
        None => None,
        Some(value) => match Silently::parse(value) {
            Some(Silently::Yes) => Some(true),
            Some(Silently::No) => Some(false),
            None => return Err(StreamAcceptError::InvalidSilent(value.to_owned())),
        },
    };
    Ok(StreamAcceptRequest { session_id, silent })
}

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

    #[test]
    fn stream_connect_ports_default_and_accept_full_u16_range() {
        let command = Command::new(
            CommandKind::StreamConnect,
            vec![
                OptionPair::new("ID".to_owned(), "s".to_owned()),
                OptionPair::new("DESTINATION".to_owned(), "dest".to_owned()),
            ],
            None,
        );
        let request = parse_stream_connect(&command).expect("default ports");
        assert_eq!((request.from_port, request.to_port), (None, None));

        let command = Command::new(
            CommandKind::StreamConnect,
            vec![
                OptionPair::new("ID".to_owned(), "s".to_owned()),
                OptionPair::new("DESTINATION".to_owned(), "dest".to_owned()),
                OptionPair::new("FROM_PORT".to_owned(), "65535".to_owned()),
                OptionPair::new("TO_PORT".to_owned(), "0".to_owned()),
            ],
            None,
        );
        let request = parse_stream_connect(&command).expect("boundary ports");
        assert_eq!(
            (request.from_port, request.to_port),
            (Some(u16::MAX), Some(0))
        );
    }

    #[test]
    fn stream_connect_rejects_non_decimal_and_overflow_ports() {
        for (name, value, expected) in [
            ("FROM_PORT", "-1", true),
            ("FROM_PORT", "65536", true),
            ("TO_PORT", "1x", false),
            ("TO_PORT", "65536", false),
        ] {
            let command = Command::new(
                CommandKind::StreamConnect,
                vec![
                    OptionPair::new("ID".to_owned(), "s".to_owned()),
                    OptionPair::new("DESTINATION".to_owned(), "dest".to_owned()),
                    OptionPair::new(name.to_owned(), value.to_owned()),
                ],
                None,
            );
            let error = parse_stream_connect(&command).expect_err("invalid port");
            assert_eq!(
                matches!(error, StreamConnectError::InvalidFromPort(_)),
                expected
            );
            assert_eq!(
                matches!(error, StreamConnectError::InvalidToPort(_)),
                !expected
            );
        }
    }

    #[test]
    fn session_add_parses_stream_child_and_requires_port_for_datagram_styles() {
        let stream = crate::sam::parser::parse_line(
            "SESSION ADD STYLE=STREAM ID=child FROM_PORT=25 TO_PORT=110",
        )
        .expect("parse stream child");
        let request =
            parse_session_add(stream.command().expect("recognized")).expect("valid stream child");
        assert_eq!(
            request.style,
            crate::sam::session_create::SessionCreateStyle::Stream
        );
        assert_eq!((request.from_port, request.to_port), (25, 110));
        assert_eq!(request.port, None);

        let datagram =
            crate::sam::parser::parse_line("SESSION ADD STYLE=DATAGRAM ID=datagram-child")
                .expect("parse datagram child");
        assert_eq!(
            parse_session_add(datagram.command().expect("recognized")),
            Err(SessionAddError::PortStyleMismatch)
        );
        for style in ["DATAGRAM", "DATAGRAM2", "DATAGRAM3", "RAW"] {
            let line = format!("SESSION ADD STYLE={style} ID=child PORT=1234");
            let request = crate::sam::parser::parse_line(&line).expect("parse child");
            assert_eq!(
                parse_session_add(request.command().expect("recognized"))
                    .expect("valid datagram child")
                    .style
                    .as_str(),
                style
            );
        }
    }

    #[test]
    fn session_add_host_and_raw_header_are_strict_and_loopback_only() {
        let request = crate::sam::parser::parse_line(
            "SESSION ADD STYLE=RAW ID=raw PORT=7655 HOST=localhost HEADER=true",
        )
        .expect("parse RAW child");
        let parsed = parse_session_add(request.command().expect("recognized"))
            .expect("valid RAW forwarding options");
        assert_eq!(
            parsed.host,
            Some(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
        );
        assert!(parsed.raw_header);
        let custom_raw = crate::sam::parser::parse_line(
            "SESSION ADD STYLE=RAW ID=raw-custom PORT=7655 PROTOCOL=42 LISTEN_PROTOCOL=43",
        )
        .expect("parse custom RAW child");
        let custom_raw = parse_session_add(custom_raw.command().expect("recognized"))
            .expect("valid custom RAW protocol pair");
        assert_eq!((custom_raw.protocol, custom_raw.listen_protocol), (42, 43));

        for line in [
            "SESSION ADD STYLE=RAW ID=raw PORT=7655 HOST=example.com",
            "SESSION ADD STYLE=RAW ID=raw PORT=7655 HOST=192.0.2.1",
            "SESSION ADD STYLE=STREAM ID=stream HOST=127.0.0.1",
            "SESSION ADD STYLE=DATAGRAM ID=dgram PORT=7655 HEADER=true",
            "SESSION ADD STYLE=RAW ID=raw PORT=7655 HEADER=maybe",
            "SESSION ADD STYLE=RAW ID=raw PORT=7655 PROTOCOL=17",
            "SESSION ADD STYLE=RAW ID=raw PORT=7655 LISTEN_PROTOCOL=6",
            "SESSION ADD STYLE=RAW ID=raw PORT=7655 DESTINATION=TRANSIENT",
        ] {
            let command = crate::sam::parser::parse_line(line).expect("parse command");
            assert!(
                parse_session_add(command.command().expect("recognized")).is_err(),
                "{line}"
            );
        }
    }
}
