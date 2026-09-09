//! Plan 178 typed IRC command classification and allowlist.
//!
//! Classifies an IRC command (post-tag core) into the typed
//! [`IrcCommand`] identity. The classifier enforces the typed
//! allowlist documented in Plan 178 §4:
//!
//! ```text
//! PASS CAP AUTHENTICATE NICK USER PING PONG
//! JOIN PART QUIT PRIVMSG NOTICE MODE TOPIC
//! AWAY NAMES LIST WHO WHOIS WHOWAS ISON
//! INVITE KICK USERHOST
//! ```
//!
//! Plus `001`-`999` numeric server replies and the documented
//! server-originated commands. Unknown / unclassified commands
//! are dropped via [`IrcCommandClass::Unknown`] rather than
//! silently allowed. The classifier does not perform rewrites;
//! the [`crate::irc::client_filter`] module layers rewrites on
//! top.

#![forbid(unsafe_code)]

use super::config::IrcCommand;
use super::errors::{IrcError, IrcErrorKind};

/// Typed classification of an IRC command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrcCommandClass {
    /// Command is recognized client- or server-issued; the
    /// classifier returns the typed [`IrcCommand`].
    Known(IrcCommand),
    /// Command is not present in the typed allowlist; the line
    /// must be dropped as a policy drop.
    Unknown,
}

/// Direction in which the line is moving. Plan 178 separates
/// classification from direction so the daemon can apply
/// per-direction policy (e.g. block user-issued OPER on the
/// client tunnel even though OPER is a documented command).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineDirection {
    /// Line is moving from the local client to the remote server.
    ClientToServer,
    /// Line is moving from the remote server to the local client.
    ServerToClient,
}

impl LineDirection {
    /// Returns the opposite direction.
    pub const fn opposite(self) -> Self {
        match self {
            Self::ClientToServer => Self::ServerToClient,
            Self::ServerToClient => Self::ClientToServer,
        }
    }
}

/// Parsed command and arguments extracted from the IRC core line.
///
/// `args` is the trailing-arguments vector per RFC 2812 §2.3:
///
/// - For `NICK newnick`, `args = ["newnick"]`.
/// - For `JOIN #chan`, `args = ["#chan"]`.
/// - For `PRIVMSG #chan :hello world`, `args = ["#chan", "hello
///   world"]`.
/// - For `USER u h s :real`, `args = ["u", "h", "s", "real"]`.
///
/// The last entry is the trailing parameter (no leading colon
/// in the stored form).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedLine {
    /// Command classification.
    pub command: IrcCommandClass,
    /// Optional message prefix (the part before the first space
    /// starting with `:`).
    pub prefix: Option<Vec<u8>>,
    /// Trailing-argument vector per RFC 2812 §2.3.
    pub args: Vec<String>,
    /// Whether the trailing parameter was preceded by a colon
    /// (`:`) and therefore may contain whitespace.
    pub has_trailing: bool,
}

impl ParsedLine {
    /// Returns the trailing parameter (last entry in `args`) as
    /// `Some(&str)` when the message carried a trailing
    /// parameter, `None` otherwise.
    pub fn trailing(&self) -> Option<&str> {
        if !self.has_trailing {
            return None;
        }
        self.args.last().map(String::as_str)
    }

    /// Returns the first middle parameter as `&str` when one
    /// exists.
    pub fn first_arg(&self) -> Option<&str> {
        self.args.first().map(String::as_str)
    }
}

/// Classifies the supplied post-tag core bytes into a
/// [`ParsedLine`]. Returns a typed [`IrcError`] for structural
/// failures (NUL, CR/LF/control inside the command) but the
/// `Unknown` classification is a typed policy outcome (not an
/// error) returned through [`ParsedLine::command`].
///
/// The caller is responsible for enforcing the core-line ceiling;
/// this function performs only the post-ceiling classification
/// step.
pub fn classify_core(core: &[u8]) -> Result<ParsedLine, IrcError> {
    let invalid = |kind, reason| IrcError::new(kind, reason);
    if core.is_empty() {
        return Ok(ParsedLine {
            command: IrcCommandClass::Unknown,
            prefix: None,
            args: Vec::new(),
            has_trailing: false,
        });
    }
    for &byte in core {
        if byte == 0 || byte == b'\r' || byte == b'\n' {
            return Err(invalid(
                IrcErrorKind::InvalidTag,
                "core line contains CR/LF/NUL",
            ));
        }
    }
    // Split into prefix and remainder. Prefix is `:` up to the
    // first space; if there is no leading `:`, prefix is None and
    // the remainder is the whole core.
    let (prefix, remainder) = if core.first() == Some(&b':') {
        match core.iter().position(|&byte| byte == b' ') {
            Some(space) => {
                let prefix = core[1..space].to_vec();
                let remainder = &core[space + 1..];
                (Some(prefix), remainder)
            }
            None => {
                // Prefix-only line; classify as Unknown.
                return Ok(ParsedLine {
                    command: IrcCommandClass::Unknown,
                    prefix: Some(core[1..].to_vec()),
                    args: Vec::new(),
                    has_trailing: false,
                });
            }
        }
    } else {
        (None, core)
    };
    if remainder.is_empty() {
        return Ok(ParsedLine {
            command: IrcCommandClass::Unknown,
            prefix,
            args: Vec::new(),
            has_trailing: false,
        });
    }
    let (head, trailing) = split_command_and_args(remainder);
    let command_name = match std::str::from_utf8(head) {
        Ok(text) => text.to_ascii_uppercase(),
        Err(_) => {
            return Ok(ParsedLine {
                command: IrcCommandClass::Unknown,
                prefix,
                args: Vec::new(),
                has_trailing: false,
            });
        }
    };
    let command = classify_command_name(&command_name);
    let (args, has_trailing) = parse_args(trailing);
    Ok(ParsedLine {
        command,
        prefix,
        args,
        has_trailing,
    })
}

fn classify_command_name(name: &str) -> IrcCommandClass {
    match name {
        "PASS" => IrcCommandClass::Known(IrcCommand::Pass),
        "CAP" => IrcCommandClass::Known(IrcCommand::Cap),
        "AUTHENTICATE" => IrcCommandClass::Known(IrcCommand::Authenticate),
        "NICK" => IrcCommandClass::Known(IrcCommand::Nick),
        "USER" => IrcCommandClass::Known(IrcCommand::User),
        "PING" => IrcCommandClass::Known(IrcCommand::Ping),
        "PONG" => IrcCommandClass::Known(IrcCommand::Pong),
        "JOIN" => IrcCommandClass::Known(IrcCommand::Join),
        "PART" => IrcCommandClass::Known(IrcCommand::Part),
        "QUIT" => IrcCommandClass::Known(IrcCommand::Quit),
        "PRIVMSG" => IrcCommandClass::Known(IrcCommand::Privmsg),
        "NOTICE" => IrcCommandClass::Known(IrcCommand::Notice),
        "MODE" => IrcCommandClass::Known(IrcCommand::Mode),
        "TOPIC" => IrcCommandClass::Known(IrcCommand::Topic),
        "AWAY" => IrcCommandClass::Known(IrcCommand::Away),
        "NAMES" => IrcCommandClass::Known(IrcCommand::Names),
        "LIST" => IrcCommandClass::Known(IrcCommand::List),
        "WHO" => IrcCommandClass::Known(IrcCommand::Who),
        "WHOIS" => IrcCommandClass::Known(IrcCommand::Whois),
        "WHOWAS" => IrcCommandClass::Known(IrcCommand::Whowas),
        "ISON" => IrcCommandClass::Known(IrcCommand::Ison),
        "INVITE" => IrcCommandClass::Known(IrcCommand::Invite),
        "KICK" => IrcCommandClass::Known(IrcCommand::Kick),
        "USERHOST" => IrcCommandClass::Known(IrcCommand::Userhost),
        // Server-originated commands documented as accepted
        // inbound (Plan 178 §4) but not classified as one of the
        // above client-issued variants. The classification still
        // returns Known(ServerCommand) so the caller can apply
        // inbound allowlist policy.
        "ACCOUNT" | "CHGHOST" | "ERROR" => IrcCommandClass::Known(IrcCommand::ServerCommand),
        // Numeric server reply: three digits.
        _ => {
            if name.len() == 3 && name.bytes().all(|b| b.is_ascii_digit()) {
                IrcCommandClass::Known(IrcCommand::NumericReply)
            } else {
                IrcCommandClass::Unknown
            }
        }
    }
}

/// Splits `input` into the command-name bytes and the parameter
/// tail. Returns an empty slice for the tail when no space
/// follows the command name.
fn split_command_and_args(input: &[u8]) -> (&[u8], &[u8]) {
    match input.iter().position(|&byte| byte == b' ') {
        Some(space) => (&input[..space], &input[space + 1..]),
        None => (input, &[]),
    }
}

/// Parses the parameter tail into a `Vec<String>` per RFC 2812
/// §2.3. Returns the parsed args and whether the trailing
/// parameter was preceded by `:`. The trailing parameter (if
/// present) is the final element of the returned vector with the
/// leading colon already removed.
fn parse_args(tail: &[u8]) -> (Vec<String>, bool) {
    if tail.is_empty() {
        return (Vec::new(), false);
    }
    let mut args: Vec<String> = Vec::new();
    let mut start = 0_usize;
    let mut has_trailing = false;
    let mut idx = 0_usize;
    while idx < tail.len() {
        if tail[idx] == b':' && (idx == 0 || tail[idx - 1] == b' ') {
            // Trailing parameter; everything from after the `:` to
            // the end is one argument. Always record the trailing
            // parameter (even if empty) so callers can distinguish
            // `CMD :` from a missing trailing.
            let value_start = idx + 1;
            let value = std::str::from_utf8(&tail[value_start..])
                .unwrap_or("")
                .to_owned();
            if has_trailing {
                // Replace the previous trailing entry; there
                // should be at most one `:`.
                if let Some(last) = args.last_mut() {
                    *last = value;
                }
            } else {
                args.push(value);
                has_trailing = true;
            }
            return (args, has_trailing);
        }
        if tail[idx] == b' ' {
            if start < idx {
                let value = std::str::from_utf8(&tail[start..idx])
                    .unwrap_or("")
                    .to_owned();
                args.push(value);
            }
            start = idx + 1;
        }
        idx += 1;
    }
    if start < tail.len() {
        let value = std::str::from_utf8(&tail[start..]).unwrap_or("").to_owned();
        args.push(value);
    }
    (args, has_trailing)
}

/// Returns `true` when the supplied typed command is allowed for
/// the supplied direction. Used by the filter to make
/// direction-specific disposition decisions.
///
/// Client-to-server admits the full Plan 178 §4 client set.
/// Server-to-client admits numeric replies, the documented
/// server-originated commands (Plan 178 §4 inbound list), plus
/// the relayed channel/messaging commands ordinary operation
/// requires (PRIVMSG/NOTICE still pass through the CTCP filter;
/// PONG answers client lag-check PINGs).
pub fn is_allowed(command: IrcCommand, direction: LineDirection) -> bool {
    match direction {
        LineDirection::ClientToServer => !matches!(
            command,
            IrcCommand::NumericReply | IrcCommand::ServerCommand
        ),
        LineDirection::ServerToClient => matches!(
            command,
            IrcCommand::NumericReply
                | IrcCommand::ServerCommand
                | IrcCommand::Ping
                | IrcCommand::Pong
                | IrcCommand::Privmsg
                | IrcCommand::Notice
                | IrcCommand::Mode
                | IrcCommand::Join
                | IrcCommand::Nick
                | IrcCommand::Quit
                | IrcCommand::Part
                | IrcCommand::Kick
                | IrcCommand::Topic
                | IrcCommand::Cap
                | IrcCommand::Authenticate
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_simple_nick() {
        let parsed = classify_core(b"NICK alice").expect("ok");
        assert_eq!(parsed.command, IrcCommandClass::Known(IrcCommand::Nick));
        assert_eq!(parsed.args, vec!["alice".to_owned()]);
    }

    #[test]
    fn classifies_user_with_trailing() {
        let parsed = classify_core(b"USER u h s :Alice Smith").expect("ok");
        assert_eq!(parsed.command, IrcCommandClass::Known(IrcCommand::User));
        assert!(parsed.has_trailing);
        assert_eq!(
            parsed.args,
            vec!["u", "h", "s", "Alice Smith"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn classifies_server_prefixed_privmsg() {
        let parsed = classify_core(b":nick!u@h PRIVMSG #c :hi there").expect("ok");
        assert_eq!(parsed.command, IrcCommandClass::Known(IrcCommand::Privmsg));
        assert_eq!(parsed.prefix.as_deref(), Some(&b"nick!u@h"[..]));
        assert!(parsed.has_trailing);
    }

    #[test]
    fn classifies_numeric_reply() {
        let parsed = classify_core(b":irc.example.org 001 alice :Welcome").expect("ok");
        assert_eq!(
            parsed.command,
            IrcCommandClass::Known(IrcCommand::NumericReply)
        );
    }

    #[test]
    fn classifies_unknown_command() {
        let parsed = classify_core(b"OPER alice secret").expect("ok");
        assert_eq!(parsed.command, IrcCommandClass::Unknown);
    }

    #[test]
    fn classifies_case_insensitive() {
        let parsed = classify_core(b"nick alice").expect("ok");
        assert_eq!(parsed.command, IrcCommandClass::Known(IrcCommand::Nick));
    }

    #[test]
    fn classifies_account_as_server_command() {
        let parsed = classify_core(b":server ACCOUNT alice").expect("ok");
        assert_eq!(
            parsed.command,
            IrcCommandClass::Known(IrcCommand::ServerCommand)
        );
    }

    #[test]
    fn classifies_kick() {
        let parsed = classify_core(b"KICK #chan user :reason").expect("ok");
        assert_eq!(parsed.command, IrcCommandClass::Known(IrcCommand::Kick));
        assert!(parsed.has_trailing);
        assert_eq!(parsed.trailing(), Some("reason"));
    }

    #[test]
    fn classifies_pass_cap_authenticate() {
        let parsed = classify_core(b"PASS secret").expect("ok");
        assert_eq!(parsed.command, IrcCommandClass::Known(IrcCommand::Pass));
        let parsed = classify_core(b"CAP LS").expect("ok");
        assert_eq!(parsed.command, IrcCommandClass::Known(IrcCommand::Cap));
        let parsed = classify_core(b"AUTHENTICATE PLAIN").expect("ok");
        assert_eq!(
            parsed.command,
            IrcCommandClass::Known(IrcCommand::Authenticate)
        );
    }

    #[test]
    fn empty_core_yields_unknown() {
        let parsed = classify_core(b"").expect("ok");
        assert_eq!(parsed.command, IrcCommandClass::Unknown);
    }

    #[test]
    fn prefix_only_yields_unknown_with_prefix() {
        let parsed = classify_core(b":server.example.org").expect("ok");
        assert_eq!(parsed.command, IrcCommandClass::Unknown);
        assert_eq!(parsed.prefix.as_deref(), Some(&b"server.example.org"[..]));
    }

    #[test]
    fn core_with_nul_rejected() {
        let err = classify_core(b"NI\0CK alice").expect_err("nul");
        assert!(matches!(err.kind, IrcErrorKind::InvalidTag));
    }

    #[test]
    fn core_with_crlf_rejected() {
        let err = classify_core(b"NICK alice\r\nQUIT").expect_err("crlf");
        assert!(matches!(err.kind, IrcErrorKind::InvalidTag));
    }

    #[test]
    fn direction_policy_allows_kick_both_directions() {
        assert!(is_allowed(IrcCommand::Kick, LineDirection::ClientToServer));
        assert!(is_allowed(IrcCommand::Kick, LineDirection::ServerToClient));
    }

    #[test]
    fn direction_policy_allows_numeric_reply_server_to_client() {
        assert!(is_allowed(
            IrcCommand::NumericReply,
            LineDirection::ServerToClient
        ));
        assert!(!is_allowed(
            IrcCommand::NumericReply,
            LineDirection::ClientToServer
        ));
    }

    #[test]
    fn direction_policy_allows_ping_both_directions() {
        assert!(is_allowed(IrcCommand::Ping, LineDirection::ClientToServer));
        assert!(is_allowed(IrcCommand::Ping, LineDirection::ServerToClient));
    }

    #[test]
    fn direction_policy_relays_messaging_server_to_client() {
        for command in [
            IrcCommand::Privmsg,
            IrcCommand::Notice,
            IrcCommand::Join,
            IrcCommand::Part,
            IrcCommand::Quit,
            IrcCommand::Nick,
            IrcCommand::Mode,
            IrcCommand::Topic,
        ] {
            assert!(
                is_allowed(command, LineDirection::ServerToClient),
                "{command:?} must relay server-to-client"
            );
            assert!(
                is_allowed(command, LineDirection::ClientToServer),
                "{command:?} must pass client-to-server"
            );
        }
    }

    #[test]
    fn direction_policy_blocks_server_only_commands_client_to_server() {
        assert!(!is_allowed(
            IrcCommand::ServerCommand,
            LineDirection::ClientToServer
        ));
        assert!(is_allowed(
            IrcCommand::ServerCommand,
            LineDirection::ServerToClient
        ));
    }

    #[test]
    fn direction_policy_blocks_registration_commands_server_to_client() {
        for command in [
            IrcCommand::Pass,
            IrcCommand::User,
            IrcCommand::Away,
            IrcCommand::Names,
            IrcCommand::List,
            IrcCommand::Who,
            IrcCommand::Whois,
            IrcCommand::Whowas,
            IrcCommand::Ison,
            IrcCommand::Invite,
            IrcCommand::Userhost,
        ] {
            assert!(
                !is_allowed(command, LineDirection::ServerToClient),
                "{command:?} must not arrive server-to-client"
            );
        }
    }

    #[test]
    fn empty_trailing_yields_empty_string() {
        let parsed = classify_core(b"PRIVMSG #c :").expect("ok");
        assert!(parsed.has_trailing);
        assert_eq!(parsed.trailing(), Some(""));
    }

    #[test]
    fn non_numeric_three_char_name_is_unknown() {
        let parsed = classify_core(b"12a x y").expect("ok");
        assert_eq!(parsed.command, IrcCommandClass::Unknown);
    }

    #[test]
    fn direction_opposite_is_symmetric() {
        assert_eq!(
            LineDirection::ClientToServer.opposite(),
            LineDirection::ServerToClient
        );
        assert_eq!(
            LineDirection::ServerToClient.opposite(),
            LineDirection::ClientToServer
        );
    }
}
