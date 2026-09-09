//! Plan 178 client-to-network privacy filter and rewrite surface.
//!
//! Implements the typed [`FilterOutcome`] dispositions and the
//! named rewrites documented in Plan 178 §6:
//!
//! - `USER` hostname -> [`DEFAULT_USER_HOSTNAME`],
//!   servername -> [`DEFAULT_USER_SERVERNAME`] (Plan 178 §6.1);
//! - `PING <nonce> <location>` strips/rewrites the location
//!   while retaining one bounded per-connection token so the
//!   corresponding `PONG` shape presented back to the client can
//!   be reconstructed (Plan 178 §6.2);
//! - `QUIT/PART <reason>` rewrites the reason when the
//!   configured [`ReasonRewritePolicy`] selects
//!   [`ReasonRewritePolicy::ReplaceStable`] (Plan 178 §6.3);
//! - `PRIVMSG`/`NOTICE` CTCP delimiter handling: allow `ACTION`,
//!   drop malformed/multi-delimiter messages, drop
//!   address-bearing `DCC`, drop other CTCP requests by default
//!   (Plan 178 §7).
//!
//! The filter is structured around typed `Allow` / `Rewrite` / `Drop`
//! outcomes so the daemon executor can apply accounting without
//! duplicating command policy inline.

#![forbid(unsafe_code)]

use super::config::{
    DEFAULT_PING_LOCATION, DEFAULT_QUIT_REASON, DEFAULT_USER_HOSTNAME, DEFAULT_USER_SERVERNAME,
    IrcCommand, ReasonRewritePolicy,
};
use super::policy::{IrcCommandClass, LineDirection, ParsedLine};

/// Bounded counter names surfaced to the daemon. The filter never
/// retains any user-supplied text; only the counter values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum IrcDropReason {
    /// Command was not in the typed allowlist.
    UnknownCommand,
    /// Command was in the allowlist but disallowed for the
    /// supplied direction.
    DisallowedDirection,
    /// `PRIVMSG`/`NOTICE` contained `DCC` (address-bearing).
    DccBlocked,
    /// `PRIVMSG`/`NOTICE` contained a CTCP request other than
    /// `ACTION`.
    UnsupportedCtcp,
    /// CTCP delimiter appeared more than once or was malformed.
    MalformedCtcp,
    /// `USER` realname exceeded the configured ceiling.
    RealnameTooLong,
    /// `USER` carried an embedded control/whitespace/NUL byte.
    BadUserField,
    /// `PING` carried a malformed or oversized location argument
    /// beyond the rewrite budget.
    PingLocationOverflow,
}

/// Typed disposition for one filter decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FilterOutcome {
    /// The line passes through unchanged.
    Allow,
    /// The line passes through with the supplied byte rewrite.
    /// The rewrite is the complete post-filter line (tag envelope
    /// reattached when present, command and arguments rewritten as
    /// needed, terminating CRLF appended).
    Rewrite { line: Vec<u8> },
    /// The line is dropped without surfacing its content. The
    /// supplied reason is added to per-direction aggregate
    /// counters only.
    Drop(IrcDropReason),
}

impl FilterOutcome {
    /// Returns `true` when the disposition is a pass-through or
    /// a rewrite that the daemon should emit.
    pub fn passes(&self) -> bool {
        !matches!(self, Self::Drop(_))
    }
}

/// Stable text substitutions used by the privacy rewrites.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivacySubstitutions {
    /// Replacement for the USER hostname argument.
    pub user_hostname: String,
    /// Replacement for the USER servername argument.
    pub user_servername: String,
    /// Replacement for QUIT/PART reasons when configured.
    pub quit_reason: String,
    /// Replacement for the rewritten PING location argument.
    pub ping_location: String,
}

impl Default for PrivacySubstitutions {
    fn default() -> Self {
        Self {
            user_hostname: DEFAULT_USER_HOSTNAME.to_owned(),
            user_servername: DEFAULT_USER_SERVERNAME.to_owned(),
            quit_reason: DEFAULT_QUIT_REASON.to_owned(),
            ping_location: DEFAULT_PING_LOCATION.to_owned(),
        }
    }
}

/// Per-connection rewrite token retained for PING/PONG shape
/// restoration. Plan 178 §6.2 caps the table to one outstanding
/// rewrite token; a new rewrite replaces the old one.
#[derive(Clone, Debug, Default)]
pub struct PingRewriteState {
    /// Whether a rewrite token is currently retained.
    active: bool,
    /// Replacement nonce presented back to the client in PONG.
    nonce: Vec<u8>,
    /// Replacement location presented back to the client in PONG.
    location: Vec<u8>,
}

impl PingRewriteState {
    /// Creates a fresh per-connection PING/PONG rewrite state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the current nonce/location pair if a rewrite is
    /// outstanding.
    pub fn current(&self) -> Option<(&[u8], &[u8])> {
        if self.active {
            Some((&self.nonce, &self.location))
        } else {
            None
        }
    }

    /// Records a new rewrite token; replaces any prior token.
    pub fn replace(&mut self, nonce: Vec<u8>, location: Vec<u8>) {
        self.active = true;
        self.nonce = nonce;
        self.location = location;
    }

    /// Consumes the current rewrite token. Subsequent
    /// `consume()` calls return `None`.
    pub fn consume(&mut self) -> Option<(Vec<u8>, Vec<u8>)> {
        if !self.active {
            return None;
        }
        self.active = false;
        let nonce = std::mem::take(&mut self.nonce);
        let location = std::mem::take(&mut self.location);
        Some((nonce, location))
    }
}

/// Decides the disposition for a single client-originated line.
///
/// `line` is the post-tag core bytes including the trailing
/// parameters but excluding the leading `@tag … ` envelope and
/// the terminating CRLF. The filter appends the CRLF when
/// emitting a rewrite.
///
/// `max_realname_bytes` is the configured `user_realname_max_bytes`
/// ceiling from [`super::config::IrcClientOptions`].
pub fn decide_client_to_server(
    parsed: &ParsedLine,
    reason_rewrite: ReasonRewritePolicy,
    substitutions: &PrivacySubstitutions,
    max_realname_bytes: usize,
) -> FilterOutcome {
    let IrcCommandClass::Known(command) = parsed.command else {
        return FilterOutcome::Drop(IrcDropReason::UnknownCommand);
    };
    match command {
        IrcCommand::User => rewrite_user(parsed, max_realname_bytes, substitutions),
        IrcCommand::Ping => FilterOutcome::Allow, // PING is server-to-client only.
        IrcCommand::Pong => FilterOutcome::Allow,
        IrcCommand::Privmsg | IrcCommand::Notice => decide_ctcp_command(parsed),
        IrcCommand::Quit => rewrite_reason(parsed, reason_rewrite, substitutions, "QUIT"),
        IrcCommand::Part => rewrite_reason(parsed, reason_rewrite, substitutions, "PART"),
        _ => FilterOutcome::Allow,
    }
}

/// Decides the disposition for a single server-originated line
/// (e.g. `PING nonce local-address`).
pub fn decide_server_to_client(
    parsed: &ParsedLine,
    ping_state: &mut PingRewriteState,
) -> FilterOutcome {
    let IrcCommandClass::Known(command) = parsed.command else {
        return FilterOutcome::Drop(IrcDropReason::UnknownCommand);
    };
    match command {
        IrcCommand::Ping if parsed.args.len() >= 2 => {
            // Rewrite the location to the stable placeholder while
            // retaining the nonce so the daemon can present the
            // expected PONG shape back to the client.
            let nonce = parsed.args[0].as_bytes().to_vec();
            let location_rewrite = DEFAULT_PING_LOCATION.as_bytes().to_vec();
            ping_state.replace(nonce.clone(), location_rewrite.clone());
            let mut line = Vec::new();
            if let Some(prefix) = &parsed.prefix {
                line.push(b':');
                line.extend_from_slice(prefix);
                line.push(b' ');
            }
            line.extend_from_slice(b"PING ");
            line.extend_from_slice(&nonce);
            line.push(b' ');
            line.extend_from_slice(&location_rewrite);
            line.extend_from_slice(b"\r\n");
            FilterOutcome::Rewrite { line }
        }
        IrcCommand::Ping => {
            // Single-argument PING passes through unchanged.
            FilterOutcome::Allow
        }
        IrcCommand::Privmsg | IrcCommand::Notice => decide_ctcp_command(parsed),
        _ => FilterOutcome::Allow,
    }
}

/// Builds a PONG line from the retained rewrite token (the PING
/// nonce plus the rewritten location). Consumes the token;
/// returns `None` when no rewrite is outstanding.
pub fn emit_pong_response(ping_state: &mut PingRewriteState) -> Option<Vec<u8>> {
    let (nonce, location) = ping_state.consume()?;
    let mut line = Vec::new();
    line.extend_from_slice(b"PONG ");
    line.extend_from_slice(&nonce);
    line.push(b' ');
    line.extend_from_slice(&location);
    line.extend_from_slice(b"\r\n");
    Some(line)
}

fn rewrite_user(
    parsed: &ParsedLine,
    max_realname_bytes: usize,
    substitutions: &PrivacySubstitutions,
) -> FilterOutcome {
    if parsed.args.len() < 4 {
        return FilterOutcome::Drop(IrcDropReason::BadUserField);
    }
    let username = parsed.args[0].clone();
    let hostname = substitutions.user_hostname.clone();
    let servername = substitutions.user_servername.clone();
    let realname = parsed.args[3].clone();
    if realname.len() > max_realname_bytes {
        return FilterOutcome::Drop(IrcDropReason::RealnameTooLong);
    }
    if username.contains('\0')
        || hostname.contains('\0')
        || servername.contains('\0')
        || realname.contains('\0')
    {
        return FilterOutcome::Drop(IrcDropReason::BadUserField);
    }
    let mut line = Vec::new();
    if let Some(prefix) = &parsed.prefix {
        line.push(b':');
        line.extend_from_slice(prefix);
        line.push(b' ');
    }
    // RFC 2812 §3.1.3: only the realname is a trailing
    // parameter. The username/hostname/servername are middle
    // parameters and must not carry a leading colon; otherwise
    // the rewrite would collapse into a single trailing
    // argument and change the command shape.
    line.extend_from_slice(b"USER ");
    line.extend_from_slice(username.as_bytes());
    line.push(b' ');
    line.extend_from_slice(hostname.as_bytes());
    line.push(b' ');
    line.extend_from_slice(servername.as_bytes());
    line.push(b' ');
    line.push(b':');
    line.extend_from_slice(realname.as_bytes());
    line.extend_from_slice(b"\r\n");
    FilterOutcome::Rewrite { line }
}

fn rewrite_reason(
    parsed: &ParsedLine,
    reason_rewrite: ReasonRewritePolicy,
    substitutions: &PrivacySubstitutions,
    command: &str,
) -> FilterOutcome {
    let Some(reason) = parsed.trailing() else {
        return FilterOutcome::Allow;
    };
    if matches!(reason_rewrite, ReasonRewritePolicy::ReplaceStable) {
        if reason.contains('\0') {
            return FilterOutcome::Allow;
        }
        let mut line = Vec::new();
        if let Some(prefix) = &parsed.prefix {
            line.push(b':');
            line.extend_from_slice(prefix);
            line.push(b' ');
        }
        line.extend_from_slice(command.as_bytes());
        line.push(b' ');
        line.push(b':');
        line.extend_from_slice(substitutions.quit_reason.as_bytes());
        line.extend_from_slice(b"\r\n");
        FilterOutcome::Rewrite { line }
    } else {
        FilterOutcome::Allow
    }
}

/// Decides the disposition for a PRIVMSG/NOTICE containing CTCP
/// delimiters (`0x01`). Returns `Allow` when no CTCP delimiter is
/// present, drops malformed/multi-delimiter messages, allows
/// `ACTION`, drops `DCC`, and drops other CTCP requests.
fn decide_ctcp_command(parsed: &ParsedLine) -> FilterOutcome {
    let Some(trailing) = parsed.trailing() else {
        return FilterOutcome::Allow;
    };
    let bytes = trailing.as_bytes();
    let delimiter_count = bytes.iter().filter(|&&b| b == 0x01).count();
    if delimiter_count == 0 {
        return FilterOutcome::Allow;
    }
    if delimiter_count != 2 || !bytes.starts_with(&[0x01]) || !bytes.ends_with(&[0x01]) {
        return FilterOutcome::Drop(IrcDropReason::MalformedCtcp);
    }
    let inner = &bytes[1..bytes.len() - 1];
    let command = match inner.iter().position(|&b| b == b' ') {
        Some(space) => &inner[..space],
        None => inner,
    };
    let command_upper = command
        .iter()
        .map(|b| b.to_ascii_uppercase())
        .collect::<Vec<_>>();
    if command_upper == b"ACTION" {
        return FilterOutcome::Allow;
    }
    if command_upper == b"DCC" {
        return FilterOutcome::Drop(IrcDropReason::DccBlocked);
    }
    if command_upper == b"VERSION"
        || command_upper == b"USERINFO"
        || command_upper == b"CLIENTINFO"
        || command_upper == b"SOURCE"
        || command_upper == b"FINGER"
        || command_upper == b"PING"
        || command_upper == b"TIME"
        || command_upper == b"PAGE"
    {
        return FilterOutcome::Drop(IrcDropReason::UnsupportedCtcp);
    }
    FilterOutcome::Drop(IrcDropReason::UnsupportedCtcp)
}

/// Resolves the disposition for one line in either direction.
/// Convenience wrapper that enforces the per-direction allowlist
/// before calling the typed decision helpers.
pub fn classify_and_decide(
    parsed: &ParsedLine,
    direction: LineDirection,
    reason_rewrite: ReasonRewritePolicy,
    substitutions: &PrivacySubstitutions,
    max_realname_bytes: usize,
    ping_state: &mut PingRewriteState,
) -> FilterOutcome {
    let IrcCommandClass::Known(command) = parsed.command else {
        return FilterOutcome::Drop(IrcDropReason::UnknownCommand);
    };
    if !super::policy::is_allowed(command, direction) {
        return FilterOutcome::Drop(IrcDropReason::DisallowedDirection);
    }
    match direction {
        LineDirection::ClientToServer => {
            decide_client_to_server(parsed, reason_rewrite, substitutions, max_realname_bytes)
        }
        LineDirection::ServerToClient => decide_server_to_client(parsed, ping_state),
    }
}

/// Builds a PONG line from the retained rewrite token. Convenience
/// wrapper used by the daemon executor for `PONG` responses to
/// server PINGs.
pub fn build_pong_for_state(ping_state: &mut PingRewriteState) -> Option<Vec<u8>> {
    emit_pong_response(ping_state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::policy::classify_core;

    fn parse(input: &[u8]) -> ParsedLine {
        classify_core(input).expect("parse")
    }

    fn default_subs() -> PrivacySubstitutions {
        PrivacySubstitutions::default()
    }

    #[test]
    fn user_rewrites_hostname_and_servername() {
        let parsed = parse(b"USER alice 192.168.1.10 irc.example.org :Alice Smith");
        let outcome =
            decide_client_to_server(&parsed, ReasonRewritePolicy::Keep, &default_subs(), 256);
        match outcome {
            FilterOutcome::Rewrite { line } => {
                let text = std::str::from_utf8(&line).expect("utf8");
                assert_eq!(text, "USER alice i2p localhost :Alice Smith\r\n");
                assert!(!text.contains("192.168.1.10"));
                assert!(!text.contains("irc.example.org"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn user_rewrite_round_trips_through_classifier() {
        // The rewritten line must re-parse to the same USER
        // shape (four args, trailing realname); a stray colon
        // would collapse the middle parameters into one
        // trailing argument.
        let parsed = parse(b"USER alice 192.168.1.10 irc.example.org :Alice Smith");
        let outcome =
            decide_client_to_server(&parsed, ReasonRewritePolicy::Keep, &default_subs(), 256);
        let FilterOutcome::Rewrite { line } = outcome else {
            panic!("expected rewrite");
        };
        let reparsed = parse(&line[..line.len() - 2]);
        assert_eq!(
            reparsed.args,
            vec!["alice", "i2p", "localhost", "Alice Smith"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
        assert!(reparsed.has_trailing);
    }

    #[test]
    fn user_preserves_realname_with_spaces() {
        let parsed = parse(b"USER alice host serv :Alice Smith Engineer");
        let outcome =
            decide_client_to_server(&parsed, ReasonRewritePolicy::Keep, &default_subs(), 256);
        match outcome {
            FilterOutcome::Rewrite { line } => {
                let text = std::str::from_utf8(&line).expect("utf8");
                assert!(text.contains("Alice Smith Engineer"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn user_realname_too_long_dropped() {
        let parsed = parse(b"USER alice h s :xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
        let outcome =
            decide_client_to_server(&parsed, ReasonRewritePolicy::Keep, &default_subs(), 10);
        assert_eq!(outcome, FilterOutcome::Drop(IrcDropReason::RealnameTooLong));
    }

    #[test]
    fn quit_reason_passes_through_under_keep() {
        let parsed = parse(b"QUIT :bye");
        let outcome =
            decide_client_to_server(&parsed, ReasonRewritePolicy::Keep, &default_subs(), 256);
        assert_eq!(outcome, FilterOutcome::Allow);
    }

    #[test]
    fn quit_reason_rewritten_under_replace() {
        let parsed = parse(b"QUIT :Client 1.0");
        let outcome = decide_client_to_server(
            &parsed,
            ReasonRewritePolicy::ReplaceStable,
            &default_subs(),
            256,
        );
        match outcome {
            FilterOutcome::Rewrite { line } => {
                let text = std::str::from_utf8(&line).expect("utf8");
                assert!(text.starts_with("QUIT :i2pr\r\n"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn ctcp_action_passes() {
        let parsed = parse(b"PRIVMSG #chan :\x01ACTION waves\x01");
        let outcome =
            decide_client_to_server(&parsed, ReasonRewritePolicy::Keep, &default_subs(), 256);
        assert_eq!(outcome, FilterOutcome::Allow);
    }

    #[test]
    fn ctcp_dcc_dropped() {
        let parsed = parse(b"PRIVMSG #chan :\x01DCC SEND file 127.0.0.1 0 1024\x01");
        let outcome =
            decide_client_to_server(&parsed, ReasonRewritePolicy::Keep, &default_subs(), 256);
        assert_eq!(outcome, FilterOutcome::Drop(IrcDropReason::DccBlocked));
    }

    #[test]
    fn ctcp_version_dropped() {
        let parsed = parse(b"PRIVMSG alice :\x01VERSION\x01");
        let outcome =
            decide_client_to_server(&parsed, ReasonRewritePolicy::Keep, &default_subs(), 256);
        assert_eq!(outcome, FilterOutcome::Drop(IrcDropReason::UnsupportedCtcp));
    }

    #[test]
    fn ctcp_malformed_delimiter_dropped() {
        let parsed = parse(b"PRIVMSG #chan :\x01ACTION waves");
        let outcome =
            decide_client_to_server(&parsed, ReasonRewritePolicy::Keep, &default_subs(), 256);
        assert_eq!(outcome, FilterOutcome::Drop(IrcDropReason::MalformedCtcp));
    }

    #[test]
    fn ping_with_location_is_rewritten() {
        let parsed = parse(b"PING token 192.168.1.10");
        let mut state = PingRewriteState::new();
        let outcome = decide_server_to_client(&parsed, &mut state);
        match outcome {
            FilterOutcome::Rewrite { line } => {
                let text = std::str::from_utf8(&line).expect("utf8");
                assert!(text.starts_with("PING token i2p\r\n"));
                assert!(!text.contains("192.168.1.10"));
            }
            other => panic!("unexpected: {other:?}"),
        }
        let (nonce, location) = state.current().expect("rewrite retained");
        assert_eq!(nonce, b"token".to_vec());
        assert_eq!(location, b"i2p".to_vec());
    }

    #[test]
    fn ping_single_argument_passes_through() {
        let parsed = parse(b"PING token");
        let mut state = PingRewriteState::new();
        let outcome = decide_server_to_client(&parsed, &mut state);
        assert_eq!(outcome, FilterOutcome::Allow);
        assert!(state.current().is_none());
    }

    #[test]
    fn pong_response_uses_retained_token() {
        let parsed = parse(b"PING nonce 192.168.1.10");
        let mut state = PingRewriteState::new();
        decide_server_to_client(&parsed, &mut state);
        let pong = build_pong_for_state(&mut state).expect("pong");
        let text = std::str::from_utf8(&pong).expect("utf8");
        assert_eq!(text, "PONG nonce i2p\r\n");
        // Consume clears the state.
        assert!(state.current().is_none());
    }

    #[test]
    fn pong_response_without_rewrite_returns_none() {
        let mut state = PingRewriteState::new();
        assert!(build_pong_for_state(&mut state).is_none());
    }

    #[test]
    fn ping_rewrite_replaces_old_token() {
        let parsed_a = parse(b"PING aaa 10.0.0.1");
        let parsed_b = parse(b"PING bbb 10.0.0.2");
        let mut state = PingRewriteState::new();
        decide_server_to_client(&parsed_a, &mut state);
        decide_server_to_client(&parsed_b, &mut state);
        let (nonce, _) = state.current().expect("rewrite retained");
        assert_eq!(nonce, b"bbb".to_vec());
    }

    #[test]
    fn privmsg_without_ctcp_passes() {
        let parsed = parse(b"PRIVMSG #chan :hello world");
        let outcome =
            decide_client_to_server(&parsed, ReasonRewritePolicy::Keep, &default_subs(), 256);
        assert_eq!(outcome, FilterOutcome::Allow);
    }

    #[test]
    fn notice_without_ctcp_passes() {
        let parsed = parse(b"NOTICE #chan :hi there");
        let outcome =
            decide_client_to_server(&parsed, ReasonRewritePolicy::Keep, &default_subs(), 256);
        assert_eq!(outcome, FilterOutcome::Allow);
    }
}
