//! Strict bounded SAM v3.1 line parser.
//!
//! Plan 136 owns the line/command/reply model. The parser converts
//! one complete line into a typed [`CommandOutcome`] without opening
//! or owning a socket. Every failure path returns a typed error so
//! later plans can map it to a deterministic reply.

use core::fmt;

use crate::sam::command::{
    Command, CommandKind, CommandOutcome, DuplicateOption, MalformedCommand, MalformedReason,
    MissingOption, OptionPair, UnknownCommand, Unsupported, UnsupportedReason, UnsupportedStyle,
};
use crate::sam::{MAX_SAM_LINE_BYTES, MAX_SAM_OPTIONS};

/// Failure modes for the SAM line parser.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseError {
    /// The line exceeded `MAX_SAM_LINE_BYTES`.
    LineTooLong {
        /// Actual length.
        actual: usize,
        /// Maximum accepted.
        maximum: usize,
    },
    /// The token count exceeded `MAX_SAM_TOKENS`.
    TooManyTokens {
        /// Actual count.
        actual: usize,
        /// Maximum accepted.
        maximum: usize,
    },
    /// The line was empty.
    Empty,
    /// The line contained a NUL byte.
    ContainsNul,
    /// The line contained a disallowed control character.
    ContainsControlByte {
        /// Byte value.
        byte: u8,
        /// Byte index.
        index: usize,
    },
    /// The line contained invalid quoting.
    InvalidQuoting {
        /// Reason.
        reason: InvalidQuotingReason,
    },
    /// The line ended with a trailing `\` escape.
    TrailingEscape,
    /// A malformed command resulted from the parser.
    Malformed(MalformedCommand),
}

/// Why a SAM line quote was malformed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidQuotingReason {
    /// The line contained a `"` byte that was not the start of a
    /// quoted value.
    StrayQuote,
    /// A quoted value was not closed before end-of-line.
    UnterminatedQuote,
    /// A `\\` escape appeared outside a quoted value.
    EscapeOutsideQuote,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LineTooLong { actual, maximum } => {
                write!(formatter, "line length {actual} exceeds {maximum}")
            }
            Self::TooManyTokens { actual, maximum } => {
                write!(formatter, "token count {actual} exceeds {maximum}")
            }
            Self::Empty => formatter.write_str("line is empty"),
            Self::ContainsNul => formatter.write_str("line contains NUL byte"),
            Self::ContainsControlByte { byte, index } => {
                write!(formatter, "control byte {byte:#x} at index {index}")
            }
            Self::InvalidQuoting { reason } => write!(formatter, "invalid quoting: {reason:?}"),
            Self::TrailingEscape => formatter.write_str("trailing escape character"),
            Self::Malformed(_) => formatter.write_str("malformed command"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parses a single complete SAM line into a typed command outcome.
pub fn parse_line(line: &str) -> Result<CommandOutcome, ParseError> {
    if line.len() > MAX_SAM_LINE_BYTES {
        return Err(ParseError::LineTooLong {
            actual: line.len(),
            maximum: MAX_SAM_LINE_BYTES,
        });
    }
    if line.is_empty() {
        return Err(ParseError::Empty);
    }
    for (index, byte) in line.bytes().enumerate() {
        if byte == 0x00 {
            return Err(ParseError::ContainsNul);
        }
        if byte < 0x20 || byte == 0x7f {
            return Err(ParseError::ContainsControlByte { byte, index });
        }
    }
    let tokens = tokenise(line)?;
    if tokens.is_empty() {
        return Err(ParseError::Empty);
    }
    recognise(&tokens)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Token {
    text: String,
}

fn tokenise(line: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut had_content = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if in_quotes => {
                let next = chars.next().ok_or(ParseError::TrailingEscape)?;
                if next != '"' && next != '\\' {
                    return Err(ParseError::InvalidQuoting {
                        reason: InvalidQuotingReason::EscapeOutsideQuote,
                    });
                }
                current.push(next);
                had_content = true;
            }
            '"' => {
                if !in_quotes {
                    in_quotes = true;
                    had_content = true;
                } else {
                    in_quotes = false;
                }
            }
            ch if ch.is_whitespace() => {
                if in_quotes {
                    current.push(ch);
                    had_content = true;
                } else if had_content {
                    tokens.push(Token {
                        text: std::mem::take(&mut current),
                    });
                    had_content = false;
                }
            }
            ch => {
                current.push(ch);
                had_content = true;
            }
        }
    }
    if in_quotes {
        return Err(ParseError::InvalidQuoting {
            reason: InvalidQuotingReason::UnterminatedQuote,
        });
    }
    if had_content {
        tokens.push(Token { text: current });
    }
    Ok(tokens)
}

fn recognise(tokens: &[Token]) -> Result<CommandOutcome, ParseError> {
    let first = &tokens[0].text;
    let upper: String = first.to_ascii_uppercase();
    match upper.as_str() {
        "HELLO" => recognise_hello(tokens),
        "DEST" => recognise_dest(tokens),
        "SESSION" => recognise_session(tokens),
        "STREAM" => recognise_stream(tokens),
        "NAMING" => recognise_naming(tokens),
        "PING" => Ok(CommandOutcome::Recognised(Command::new(
            CommandKind::Ping,
            Vec::new(),
            ping_payload(tokens),
        ))),
        "PONG" => Ok(CommandOutcome::Recognised(Command::new(
            CommandKind::Pong,
            Vec::new(),
            None,
        ))),
        "QUIT" | "STOP" | "EXIT" => Ok(CommandOutcome::Recognised(Command::new(
            CommandKind::Quit,
            Vec::new(),
            None,
        ))),
        _ => Ok(CommandOutcome::UnknownCommand(UnknownCommand {
            observed: upper,
        })),
    }
}

fn ping_payload(tokens: &[Token]) -> Option<String> {
    // The SAM 3.1 wire allows multi-token PING payloads. Join every
    // token after `PING` with single spaces, matching the Java
    // SAMBridge reference behaviour.
    if tokens.len() < 2 {
        return None;
    }
    let mut joined = String::new();
    for (index, token) in tokens[1..].iter().enumerate() {
        if index > 0 {
            joined.push(' ');
        }
        joined.push_str(token.text.as_str());
    }
    let joined = joined.trim().to_owned();
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

const CRITICAL_HELLO: &[&str] = &["MIN", "MAX"];
const CRITICAL_DEST: &[&str] = &["SIGNATURE_TYPE"];
const CRITICAL_SESSION: &[&str] = &["ID", "DESTINATION", "SIGNATURE_TYPE", "STYLE"];
const CRITICAL_STREAM: &[&str] = &["ID", "DESTINATION", "SILENT"];
const CRITICAL_STREAM_ACCEPT: &[&str] = &["ID", "SILENT"];
const CRITICAL_NAMING: &[&str] = &["NAME", "OPTIONS"];

fn recognise_hello(tokens: &[Token]) -> Result<CommandOutcome, ParseError> {
    if tokens.len() < 2 || !tokens[1].text.eq_ignore_ascii_case("VERSION") {
        return Ok(CommandOutcome::UnknownAction(UnknownCommand {
            observed: format!("HELLO {}", tokens[1].text.to_ascii_uppercase()),
        }));
    }
    let (options, errors) = collect_options(&tokens[2..], CRITICAL_HELLO)?;
    if let Some(reason) = errors.into_iter().next() {
        return Ok(CommandOutcome::Malformed(MalformedCommand { reason }));
    }
    Ok(CommandOutcome::Recognised(Command::new(
        CommandKind::HelloVersion,
        options,
        None,
    )))
}

fn recognise_dest(tokens: &[Token]) -> Result<CommandOutcome, ParseError> {
    if tokens.len() < 2 || !tokens[1].text.eq_ignore_ascii_case("GENERATE") {
        return Ok(CommandOutcome::UnknownAction(UnknownCommand {
            observed: format!("DEST {}", tokens[1].text.to_ascii_uppercase()),
        }));
    }
    let (options, errors) = collect_options(&tokens[2..], CRITICAL_DEST)?;
    if let Some(reason) = errors.into_iter().next() {
        return Ok(CommandOutcome::Malformed(MalformedCommand { reason }));
    }
    Ok(CommandOutcome::Recognised(Command::new(
        CommandKind::DestGenerate,
        options,
        None,
    )))
}

fn recognise_session(tokens: &[Token]) -> Result<CommandOutcome, ParseError> {
    if tokens.len() < 2 || !tokens[1].text.eq_ignore_ascii_case("CREATE") {
        return Ok(CommandOutcome::UnknownAction(UnknownCommand {
            observed: format!("SESSION {}", tokens[1].text.to_ascii_uppercase()),
        }));
    }
    let (options, errors) = collect_options(&tokens[2..], CRITICAL_SESSION)?;
    if let Some(reason) = errors.into_iter().next() {
        return Ok(CommandOutcome::Malformed(MalformedCommand { reason }));
    }
    let command = Command::new(CommandKind::SessionCreate, options, None);
    if let Some(style_text) = command.value("STYLE") {
        let normalised = style_text.to_ascii_uppercase();
        if normalised != "STREAM" {
            return Ok(CommandOutcome::Unsupported(Unsupported {
                kind: CommandKind::SessionCreate,
                reason: UnsupportedReason::UnsupportedSessionStyle(UnsupportedStyle(normalised)),
            }));
        }
    }
    Ok(CommandOutcome::Recognised(command))
}

fn recognise_stream(tokens: &[Token]) -> Result<CommandOutcome, ParseError> {
    if tokens.len() < 2 {
        return Ok(CommandOutcome::Malformed(MalformedCommand {
            reason: MalformedReason::MissingRequiredOption(MissingOption::SessionId),
        }));
    }
    let action = tokens[1].text.to_ascii_uppercase();
    match action.as_str() {
        "CONNECT" => {
            let (options, errors) = collect_options(&tokens[2..], CRITICAL_STREAM)?;
            if let Some(reason) = errors.into_iter().next() {
                return Ok(CommandOutcome::Malformed(MalformedCommand { reason }));
            }
            let command = Command::new(CommandKind::StreamConnect, options, None);
            if command.value("FROM_PORT").is_some() || command.value("TO_PORT").is_some() {
                return Ok(CommandOutcome::Unsupported(Unsupported {
                    kind: CommandKind::StreamConnect,
                    reason: UnsupportedReason::StreamConnectPortOptionUnsupported,
                }));
            }
            Ok(CommandOutcome::Recognised(command))
        }
        "ACCEPT" => {
            let (options, errors) = collect_options(&tokens[2..], CRITICAL_STREAM_ACCEPT)?;
            if let Some(reason) = errors.into_iter().next() {
                return Ok(CommandOutcome::Malformed(MalformedCommand { reason }));
            }
            Ok(CommandOutcome::Recognised(Command::new(
                CommandKind::StreamAccept,
                options,
                None,
            )))
        }
        "FORWARD" => {
            let (options, errors) = collect_options(&tokens[2..], CRITICAL_STREAM)?;
            if let Some(reason) = errors.into_iter().next() {
                return Ok(CommandOutcome::Malformed(MalformedCommand { reason }));
            }
            Ok(CommandOutcome::Recognised(Command::new(
                CommandKind::StreamForward,
                options,
                None,
            )))
        }
        _ => Ok(CommandOutcome::UnknownAction(UnknownCommand {
            observed: format!("STREAM {action}"),
        })),
    }
}

fn recognise_naming(tokens: &[Token]) -> Result<CommandOutcome, ParseError> {
    if tokens.len() < 2 || !tokens[1].text.eq_ignore_ascii_case("LOOKUP") {
        return Ok(CommandOutcome::UnknownAction(UnknownCommand {
            observed: format!("NAMING {}", tokens[1].text.to_ascii_uppercase()),
        }));
    }
    let (options, errors) = collect_options(&tokens[2..], CRITICAL_NAMING)?;
    if let Some(reason) = errors.into_iter().next() {
        return Ok(CommandOutcome::Malformed(MalformedCommand { reason }));
    }
    let command = Command::new(CommandKind::NamingLookup, options, None);
    if let Some(opts) = command.value("OPTIONS")
        && opts.eq_ignore_ascii_case("true")
    {
        return Ok(CommandOutcome::Unsupported(Unsupported {
            kind: CommandKind::NamingLookup,
            reason: UnsupportedReason::NamingLookupOptions,
        }));
    }
    Ok(CommandOutcome::Recognised(command))
}

fn collect_options(
    tokens: &[Token],
    critical: &[&str],
) -> Result<(Vec<OptionPair>, Vec<MalformedReason>), ParseError> {
    let mut options = Vec::new();
    let mut errors = Vec::new();
    for token in tokens {
        let (key, raw_value) = match token.text.split_once('=') {
            Some(parts) => parts,
            None => {
                errors.push(MalformedReason::InvalidQuoting);
                continue;
            }
        };
        if options.len() >= MAX_SAM_OPTIONS {
            errors.push(MalformedReason::TooManyTokens);
            continue;
        }
        let value = match unquote_value(raw_value) {
            Ok(value) => value,
            Err(_) => {
                errors.push(MalformedReason::InvalidQuoting);
                continue;
            }
        };
        if validate_option_value(&value).is_err() {
            errors.push(MalformedReason::OptionTooLong);
            continue;
        }
        let upper = key.to_ascii_uppercase();
        if critical
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&upper))
            && options
                .iter()
                .any(|existing: &OptionPair| existing.key().eq_ignore_ascii_case(&upper))
        {
            let duplicate = match upper.as_str() {
                "ID" => DuplicateOption::Id,
                "DESTINATION" => DuplicateOption::Destination,
                "MIN" => DuplicateOption::Min,
                "MAX" => DuplicateOption::Max,
                "SIGNATURE_TYPE" => DuplicateOption::SignatureType,
                "STYLE" => DuplicateOption::Style,
                "NAME" => DuplicateOption::Name,
                "SILENT" => DuplicateOption::Silent,
                _ => DuplicateOption::Id,
            };
            errors.push(MalformedReason::DuplicateOption(duplicate));
            continue;
        }
        if upper == "ID" {
            if let Err(reason) = validate_session_id(&value) {
                errors.push(reason);
                continue;
            }
        } else if upper == "NAME"
            && let Err(reason) = validate_name(&value)
        {
            errors.push(reason);
            continue;
        }
        options.push(OptionPair::new(key.to_owned(), value));
    }
    Ok((options, errors))
}

fn unquote_value(input: &str) -> Result<String, ()> {
    if !input.starts_with('"') {
        return Ok(input.to_owned());
    }
    if !input.ends_with('"') || input.len() < 2 {
        return Err(());
    }
    let inner = &input[1..input.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let next = chars.next().ok_or(())?;
            if next != '"' && next != '\\' {
                return Err(());
            }
            out.push(next);
        } else {
            out.push(ch);
        }
    }
    Ok(out)
}

use crate::sam::command::{validate_name, validate_option_value, validate_session_id};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_canonical_hello() {
        let outcome = parse_line("HELLO VERSION MIN=3.1 MAX=3.1").unwrap();
        assert!(matches!(outcome, CommandOutcome::Recognised(_)));
    }

    #[test]
    fn parser_normalises_command_case() {
        let outcome = parse_line("hello version min=3.1 max=3.1").unwrap();
        let command = outcome.command().expect("recognised");
        assert_eq!(command.kind(), CommandKind::HelloVersion);
        assert_eq!(command.value("MIN"), Some("3.1"));
        assert_eq!(command.value("min"), Some("3.1"));
    }

    #[test]
    fn parser_rejects_oversized_lines() {
        let huge = "A".repeat(MAX_SAM_LINE_BYTES + 1);
        let error = parse_line(&huge).unwrap_err();
        assert!(matches!(error, ParseError::LineTooLong { .. }));
    }

    #[test]
    fn parser_rejects_embedded_nul() {
        let error = parse_line("HELLO\0VERSION").unwrap_err();
        assert_eq!(error, ParseError::ContainsNul);
    }

    #[test]
    fn parser_rejects_embedded_control_byte() {
        let error = parse_line("HELLO\x07VERSION").unwrap_err();
        assert!(matches!(error, ParseError::ContainsControlByte { .. }));
    }

    #[test]
    fn parser_rejects_duplicate_id() {
        let outcome =
            parse_line("SESSION CREATE STYLE=STREAM ID=alpha DESTINATION=foo ID=alpha").unwrap();
        assert!(matches!(
            outcome,
            CommandOutcome::Malformed(MalformedCommand {
                reason: MalformedReason::DuplicateOption(DuplicateOption::Id),
            })
        ));
    }

    #[test]
    fn parser_recognises_unsupported_style() {
        let outcome = parse_line("SESSION CREATE STYLE=DATAGRAM ID=alpha").unwrap();
        assert!(matches!(
            outcome,
            CommandOutcome::Unsupported(Unsupported {
                reason: UnsupportedReason::UnsupportedSessionStyle(_),
                ..
            })
        ));
    }

    #[test]
    fn parser_rejects_unterminated_quote() {
        let error = parse_line("HELLO VERSION MIN=\"3.1 MAX=3.1").unwrap_err();
        assert!(matches!(
            error,
            ParseError::InvalidQuoting {
                reason: InvalidQuotingReason::UnterminatedQuote,
            }
        ));
    }

    #[test]
    fn parser_handles_escaped_quote_in_quoted_value() {
        // `MIN="3\""` => quoted value contains escaped quote => MIN=3"
        let outcome = parse_line(r#"HELLO VERSION MIN="3\"" MAX=3.1"#).unwrap();
        let command = outcome.command().expect("recognised");
        assert_eq!(command.value("MIN"), Some("3\""));
    }

    #[test]
    fn parser_rejects_trailing_escape() {
        // `MIN="3\` is a quoted value with an unterminated escape.
        let error = parse_line(r#"HELLO VERSION MIN="3\"#).unwrap_err();
        assert_eq!(error, ParseError::TrailingEscape);
    }

    #[test]
    fn parser_recognises_unknown_command() {
        let outcome = parse_line("WHAT AM I").unwrap();
        assert!(matches!(outcome, CommandOutcome::UnknownCommand(_)));
    }

    #[test]
    fn parser_recognises_unknown_action() {
        let outcome = parse_line("SESSION DESTROY ID=foo").unwrap();
        assert!(matches!(outcome, CommandOutcome::UnknownAction(_)));
    }

    #[test]
    fn parser_recognises_naming_lookup_options_unsupported() {
        let outcome = parse_line("NAMING LOOKUP NAME=me OPTIONS=true").unwrap();
        assert!(matches!(
            outcome,
            CommandOutcome::Unsupported(Unsupported {
                reason: UnsupportedReason::NamingLookupOptions,
                ..
            })
        ));
    }

    #[test]
    fn parser_handles_stream_connect_port_unsupported() {
        let outcome = parse_line("STREAM CONNECT ID=alpha DESTINATION=foo FROM_PORT=1234").unwrap();
        assert!(matches!(
            outcome,
            CommandOutcome::Unsupported(Unsupported {
                reason: UnsupportedReason::StreamConnectPortOptionUnsupported,
                ..
            })
        ));
    }

    #[test]
    fn parser_handles_canonical_stream_accept() {
        let outcome = parse_line("STREAM ACCEPT ID=alpha SILENT=true").unwrap();
        let command = outcome.command().expect("recognised");
        assert_eq!(command.kind(), CommandKind::StreamAccept);
        assert_eq!(command.value("SILENT"), Some("true"));
    }
}
