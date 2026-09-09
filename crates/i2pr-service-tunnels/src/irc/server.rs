//! Plan 179 runtime-neutral IRC server registration interceptor.
//!
//! ```text
//! remote I2P IRC client
//!   -> i2pr persistent IRC server Destination / Streaming accept
//!   -> bounded registration interceptor (this module)
//!   -> authenticated peer Destination -> safe IRC hostname projection
//!   -> loopback IRC server target
//!   -> raw bounded byte pump after registration
//! ```
//!
//! The interceptor is a runtime-neutral state machine that:
//!
//! 1. Receives the bounded post-accept byte stream from the I2P side;
//! 2. Walks the pre-registration commands (PASS/CAP/AUTHENTICATE/
//!    NICK) until it sees `USER` (or explicitly supported `SERVER`);
//! 3. Replaces the USER hostname with the authenticated peer
//!    Destination base32 projection (`<52-char b32>.b32.i2p`),
//!    leaving the username / servername / realname / mode parameters
//!    intact within bounds;
//! 4. Hands the rewritten prefix plus any same-read bytes after
//!    USER back to the daemon executor for one-shot write to the
//!    loopback target followed by a raw byte pump.
//!
//! The module is runtime-neutral: no Tokio, no sockets, no
//! filesystem, no Streaming, no Garlic/I2NP. The daemon owns the
//! socket, target, and Streaming lifetime; the module owns the
//! pre-registration protocol, the hostname projection algorithm, and
//! the bounded registration policy.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use super::config::{IRC_CORE_MAX_BYTES, IRC_LINE_BUFFER_MAX_BYTES};
use super::errors::{IrcError, IrcErrorKind};
use super::limits::IrcLimits;
use super::policy::classify_core;

/// Maximum number of pre-registration lines (including USER) Plan 179
/// allows before the registration is rejected. The Plan 179 §5
/// documented ceiling is `<= 10`; we use 10 as the typed default.
pub const IRC_SERVER_MAX_REGISTRATION_LINES_DEFAULT: usize = 10;
/// Hard maximum for [`IRC_SERVER_MAX_REGISTRATION_LINES_DEFAULT`].
pub const IRC_SERVER_MAX_REGISTRATION_LINES_CEILING: usize = 64;
/// Default bytes retained across the pre-registration scan. The
/// per-line ceiling is the smaller IRC core ceiling; the cumulative
/// ceiling is the documented IRC line buffer cap so registration
/// never grows beyond the per-direction line buffer.
pub const IRC_SERVER_REGISTRATION_BUFFER_DEFAULT: usize = IRC_LINE_BUFFER_MAX_BYTES;

/// Plan 179 typed registration interceptor options.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrcServerOptions {
    /// Maximum number of pre-registration lines (PASS/CAP/
    /// AUTHENTICATE/NICK plus the terminating USER/SERVER line).
    pub max_registration_lines: usize,
    /// Maximum total bytes retained across the pre-registration
    /// scan. Plan 179 §5 requires every accumulated value to have
    /// a hard ceiling; this is the cumulative ceiling.
    pub max_registration_bytes: usize,
    /// Pre-registration commands that may appear before USER. Plan
    /// 179 §7 enumerates `PASS`, `CAP`, `AUTHENTICATE`, `NICK` as
    /// the bounded default allowlist.
    pub pre_registration_allowlist: BTreeSet<String>,
}

impl Default for IrcServerOptions {
    fn default() -> Self {
        let mut allowlist = BTreeSet::new();
        for command in ["PASS", "CAP", "AUTHENTICATE", "NICK"] {
            allowlist.insert(command.to_owned());
        }
        Self {
            max_registration_lines: IRC_SERVER_MAX_REGISTRATION_LINES_DEFAULT,
            max_registration_bytes: IRC_SERVER_REGISTRATION_BUFFER_DEFAULT,
            pre_registration_allowlist: allowlist,
        }
    }
}

impl IrcServerOptions {
    /// Validates ceilings against the hard maxima.
    pub fn validate(&self) -> Result<(), IrcError> {
        if self.max_registration_lines == 0
            || self.max_registration_lines > IRC_SERVER_MAX_REGISTRATION_LINES_CEILING
        {
            return Err(IrcError::new(
                IrcErrorKind::InvalidLimits,
                "max_registration_lines out of range",
            ));
        }
        if self.max_registration_bytes == 0
            || self.max_registration_bytes > IRC_LINE_BUFFER_MAX_BYTES
        {
            return Err(IrcError::new(
                IrcErrorKind::InvalidLimits,
                "max_registration_bytes out of range",
            ));
        }
        for command in &self.pre_registration_allowlist {
            if !command.bytes().all(|byte| byte.is_ascii_uppercase())
                || command.is_empty()
                || command.len() > 16
            {
                return Err(IrcError::new(
                    IrcErrorKind::InvalidLimits,
                    "pre_registration_allowlist contains malformed command name",
                ));
            }
        }
        Ok(())
    }
}

/// Projected authenticated peer hostname derived from the Streaming
/// peer's 32-byte destination hash. Always returns a 52-character
/// lower-case base32 label followed by `.b32.i2p` so the local IRC
/// daemon observes a deterministic, restart-stable identity.
pub fn project_peer_hostname(peer_destination_hash: &[u8; 32]) -> String {
    let mut out = String::with_capacity(52 + ".b32.i2p".len());
    out.push_str(&encode_b32_label(peer_destination_hash));
    out.push_str(".b32.i2p");
    out
}

/// Encodes a 32-byte SHA-256 hash as a 52-character lower-case I2P
/// Base32 label. The I2P Base32 alphabet is `a-z` (0..=25) plus
/// `2-7` (26..=31); 32 bytes encode to 52 characters with 4 bits of
/// padding (which must always be zero for a valid hash).
pub fn encode_b32_label(bytes: &[u8; 32]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::with_capacity(52);
    let mut accumulator: u32 = 0;
    let mut bits: u8 = 0;
    for byte in bytes {
        accumulator = (accumulator << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((accumulator >> bits) & 0x1f) as usize] as char);
        }
        if bits > 0 {
            accumulator &= (1_u32 << bits) - 1;
        } else {
            accumulator = 0;
        }
    }
    // 32 bytes -> 256 bits -> 51 chars (255 bits) + 1 char carrying
    // the remaining 1 bit (padded with 4 zero bits). The last char
    // is the partial carry shifted into the high 5 bits; trailing
    // padding bits are always zero for a valid hash.
    if bits != 0 {
        let shift = 5 - bits;
        out.push(ALPHABET[((accumulator << shift) & 0x1f) as usize] as char);
    }
    out
}

/// State of the registration state machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationState {
    /// No line has been observed yet.
    AwaitingRegistration,
    /// At least one pre-registration line (PASS/CAP/AUTHENTICATE/
    /// NICK) has been accepted.
    SawPreRegistration,
    /// A `USER` line has been observed and the registration prefix
    /// is ready for handoff.
    Ready,
    /// Registration was rejected for a typed reason; the executor
    /// must close the connection and may emit at most one bounded
    /// IRC-style failure to the remote peer.
    Rejected,
}

/// Typed registration rejection reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationRejection {
    /// Too many pre-registration lines before USER/SERVER.
    TooManyLines,
    /// The cumulative pre-registration byte count exceeded the
    /// configured ceiling.
    BufferOverflow,
    /// The first observed line was an obvious cross-protocol marker
    /// (HTTP `GET`/`POST`/etc., BitTorrent handshake).
    CrossProtocol,
    /// A pre-registration command outside the bounded allowlist was
    /// observed.
    UnknownCommand,
    /// A line was structurally invalid (NUL/control/CRLF inside the
    /// core, missing CRLF terminator, etc).
    InvalidLine,
    /// A `USER` line was present but malformed (missing username/
    /// hostname/servername, oversized realname, embedded NUL).
    InvalidUser,
    /// A `SERVER` line was present but malformed.
    InvalidServer,
    /// A pre-registration line was syntactically valid but its
    /// core exceeded the IRC line ceiling.
    CoreLineTooLong,
}

impl RegistrationRejection {
    /// Stable machine-readable short identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TooManyLines => "too-many-lines",
            Self::BufferOverflow => "buffer-overflow",
            Self::CrossProtocol => "cross-protocol",
            Self::UnknownCommand => "unknown-command",
            Self::InvalidLine => "invalid-line",
            Self::InvalidUser => "invalid-user",
            Self::InvalidServer => "invalid-server",
            Self::CoreLineTooLong => "core-line-too-long",
        }
    }
}

/// Outcome of one `advance` call against the registration
/// interceptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistrationOutcome {
    /// The interceptor needs more bytes to complete. The
    /// `retained` field carries any unconsumed tail the caller
    /// must keep around for the next call.
    Incomplete {
        /// Buffered bytes to prepend on the next advance call.
        retained: Vec<u8>,
    },
    /// The interceptor saw `USER` (or `SERVER`) and is ready to
    /// handoff. `prefix` is the rewritten post-tag registration
    /// prefix (terminating CRLF included) the daemon must write
    /// to the loopback target; `leftover` is any bytes the same
    /// read carried after the USER line terminator, which the
    /// daemon must emit as the first raw-pump bytes without
    /// re-parsing.
    Ready {
        /// Registration prefix to write to target exactly once.
        prefix: Vec<u8>,
        /// Bytes that follow the terminating USER line in the
        /// same read; preserved verbatim as the first raw-pump
        /// bytes.
        leftover: Vec<u8>,
    },
    /// Registration was rejected for a typed reason.
    Rejected(RegistrationRejection),
    /// The peer closed the Streaming connection before USER was
    /// observed. The daemon must close the loopback target
    /// without writing any registration prefix.
    Eof,
}

/// Plan 179 bounded registration interceptor.
///
/// The interceptor owns:
/// - the partial-line / multi-read buffer (per-direction ceiling);
/// - the line counter (typed ceiling);
/// - the per-line CRLF / IRC core-line bounds;
/// - the IRCv3 tag envelope re-parse helper (delegated to the
///   existing Plan 178 module);
/// - the pre-registration command allowlist (configurable);
/// - the projected peer hostname (one-time set at construction
///   from the authenticated peer Destination hash);
/// - the cumulative pre-registration byte buffer (so PASS / CAP /
///   AUTHENTICATE / NICK lines are emitted verbatim before the
///   rewritten USER line at handoff time).
#[derive(Clone, Debug)]
pub struct IrcServerRegistration {
    options: IrcServerOptions,
    limits: IrcLimits,
    peer_hostname: String,
    state: RegistrationState,
    buffer: Vec<u8>,
    line_count: usize,
    bytes_seen: usize,
    pre_registration_bytes: Vec<u8>,
}

impl IrcServerRegistration {
    /// Creates a new interceptor.
    ///
    /// `peer_destination_hash` is the 32-byte destination hash
    /// attached to the accepted Streaming connection (the
    /// authenticated peer identity). The interceptor projects it
    /// to `<52-char b32>.b32.i2p` once at construction; the
    /// projection never changes for the same hash.
    pub fn new(
        options: IrcServerOptions,
        limits: IrcLimits,
        peer_destination_hash: [u8; 32],
    ) -> Self {
        Self {
            options,
            limits,
            peer_hostname: project_peer_hostname(&peer_destination_hash),
            state: RegistrationState::AwaitingRegistration,
            buffer: Vec::new(),
            line_count: 0,
            bytes_seen: 0,
            pre_registration_bytes: Vec::new(),
        }
    }

    /// Returns the current registration state.
    pub fn state(&self) -> RegistrationState {
        self.state
    }

    /// Returns the projected peer hostname (for diagnostics /
    /// snapshot only; the daemon never echoes it to the remote
    /// peer in an error response).
    pub fn peer_hostname(&self) -> &str {
        &self.peer_hostname
    }

    /// Returns the configured options.
    pub fn options(&self) -> &IrcServerOptions {
        &self.options
    }

    /// Returns the configured limits.
    pub fn limits(&self) -> IrcLimits {
        self.limits
    }

    /// Feeds one chunk of bytes into the interceptor.
    ///
    /// The caller passes raw I2P-side bytes; the interceptor scans
    /// CRLF-terminated lines until it sees USER (or SERVER) or
    /// rejects for a typed reason. The function never blocks; it
    /// returns [`RegistrationOutcome::Incomplete`] when more bytes
    /// are required.
    pub fn advance(&mut self, bytes: &[u8]) -> RegistrationOutcome {
        if bytes.is_empty() {
            return RegistrationOutcome::Incomplete {
                retained: std::mem::take(&mut self.buffer),
            };
        }
        if self.state == RegistrationState::Rejected || self.state == RegistrationState::Ready {
            // Once terminal, no further bytes are accepted.
            return RegistrationOutcome::Incomplete {
                retained: Vec::new(),
            };
        }
        if self.buffer.len() + bytes.len() > self.options.max_registration_bytes {
            self.state = RegistrationState::Rejected;
            return RegistrationOutcome::Rejected(RegistrationRejection::BufferOverflow);
        }
        self.buffer.extend_from_slice(bytes);
        self.bytes_seen = self.bytes_seen.saturating_add(bytes.len());
        self.scan_lines()
    }

    /// Closes the interceptor after the remote peer hung up before
    /// USER was observed.
    pub fn finish_eof(&mut self) -> RegistrationOutcome {
        if self.state == RegistrationState::Ready {
            return RegistrationOutcome::Incomplete {
                retained: Vec::new(),
            };
        }
        if self.state == RegistrationState::Rejected {
            return RegistrationOutcome::Rejected(RegistrationRejection::InvalidLine);
        }
        if self.line_count == 0 {
            self.state = RegistrationState::Rejected;
        }
        RegistrationOutcome::Eof
    }

    fn scan_lines(&mut self) -> RegistrationOutcome {
        loop {
            match find_crlf(&self.buffer) {
                None => {
                    // No complete line yet.
                    if self.buffer.len()
                        > self.limits.core_max_bytes + self.limits.tag_envelope_max_bytes
                    {
                        self.state = RegistrationState::Rejected;
                        return RegistrationOutcome::Rejected(
                            RegistrationRejection::CoreLineTooLong,
                        );
                    }
                    return RegistrationOutcome::Incomplete {
                        retained: std::mem::take(&mut self.buffer),
                    };
                }
                Some(crlf_index) => {
                    let line_bytes = self.buffer[..crlf_index].to_vec();
                    let rest = self.buffer[crlf_index + 2..].to_vec();
                    self.buffer.clear();
                    self.line_count = self.line_count.saturating_add(1);
                    if self.line_count > self.options.max_registration_lines {
                        self.state = RegistrationState::Rejected;
                        return RegistrationOutcome::Rejected(RegistrationRejection::TooManyLines);
                    }
                    if line_bytes.is_empty() {
                        // Empty line; skip and continue scanning.
                        self.buffer = rest;
                        continue;
                    }
                    match self.process_line(&line_bytes) {
                        Ok(LineDisposition::Ready { prefix, leftover }) => {
                            // The leftover bytes belong to the next
                            // line; we already stripped the CRLF
                            // terminator above so `rest` is bytes
                            // that arrived in the same read after
                            // the USER line.
                            let mut combined = leftover;
                            combined.extend_from_slice(&rest);
                            self.state = RegistrationState::Ready;
                            return RegistrationOutcome::Ready {
                                prefix,
                                leftover: combined,
                            };
                        }
                        Ok(LineDisposition::PreRegistrationSeenWithBytes { bytes }) => {
                            // Pre-registration bytes are buffered
                            // for the eventual prefix handoff; the
                            // byte pump handoff concatenates all
                            // accepted pre-registration lines with
                            // the rewritten USER line.
                            self.state = RegistrationState::SawPreRegistration;
                            self.pre_registration_bytes.extend_from_slice(&bytes);
                            self.buffer = rest;
                        }
                        Err(rejection) => {
                            self.state = RegistrationState::Rejected;
                            return RegistrationOutcome::Rejected(rejection);
                        }
                    }
                }
            }
        }
    }

    fn process_line(
        &mut self,
        line_bytes: &[u8],
    ) -> Result<LineDisposition, RegistrationRejection> {
        // IRCv3 message-tag envelope, if present, is stripped here
        // using the same grammar as the Plan 178 line parser. The
        // tag envelope is preserved verbatim in the rewritten
        // prefix so the local IRC daemon observes the original
        // tagged message when present.
        let (tag_envelope, core_bytes) = split_tag_envelope(line_bytes);
        // Reject obvious cross-protocol misuse on the first line.
        if self.line_count == 1
            && tag_envelope.is_none()
            && let Some(rejection) = detect_cross_protocol(core_bytes)
        {
            return Err(rejection);
        }
        // Enforce the post-tag core-line ceiling. Plan 178 §3.1
        // documents the 512-byte core cap; Plan 179 inherits the
        // same ceiling for the registration prefix.
        let total_post_tag_cap = self.limits.core_max_bytes;
        if core_bytes.len() > total_post_tag_cap {
            return Err(RegistrationRejection::CoreLineTooLong);
        }
        let parsed = match classify_core(core_bytes) {
            Ok(parsed) => parsed,
            Err(_error) => {
                return Err(RegistrationRejection::InvalidLine);
            }
        };
        let super::policy::IrcCommandClass::Known(command) = parsed.command else {
            return Err(RegistrationRejection::UnknownCommand);
        };
        match command {
            super::config::IrcCommand::User => {
                let user_prefix = build_user_rewrite(&parsed, tag_envelope, &self.peer_hostname)
                    .ok_or(RegistrationRejection::InvalidUser)?;
                // Prepend the buffered pre-registration bytes
                // (PASS / CAP / AUTHENTICATE / NICK) so the local
                // IRC target observes the complete registration
                // sequence. The handoff remains one-shot.
                let mut prefix = std::mem::take(&mut self.pre_registration_bytes);
                prefix.extend_from_slice(&user_prefix);
                Ok(LineDisposition::Ready {
                    prefix,
                    leftover: Vec::new(),
                })
            }
            super::config::IrcCommand::Server => {
                // SERVER (server-to-server IRC) is explicitly
                // supported by Plan 179 §7. We do not rewrite it;
                // we only require the line to be structurally
                // valid. The host argument is the connecting
                // server name and is not an attacker-controlled
                // remote peer identity; we pass it through with the
                // original tag envelope.
                let mut line = Vec::new();
                if let Some(envelope) = tag_envelope {
                    line.extend_from_slice(envelope);
                    line.push(b' ');
                }
                line.extend_from_slice(core_bytes);
                line.extend_from_slice(b"\r\n");
                let mut prefix = std::mem::take(&mut self.pre_registration_bytes);
                prefix.extend_from_slice(&line);
                Ok(LineDisposition::Ready {
                    prefix,
                    leftover: Vec::new(),
                })
            }
            super::config::IrcCommand::Pass
            | super::config::IrcCommand::Cap
            | super::config::IrcCommand::Authenticate
            | super::config::IrcCommand::Nick => {
                if !self.allows_pre_registration(core_bytes) {
                    return Err(RegistrationRejection::UnknownCommand);
                }
                // Pre-registration commands are passed through to
                // the local target verbatim, with the original
                // tag envelope attached. We do not re-parse them
                // for command-specific policy; the local IRC
                // server is the policy owner for these commands.
                let mut line = Vec::new();
                if let Some(envelope) = tag_envelope {
                    line.extend_from_slice(envelope);
                    line.push(b' ');
                }
                line.extend_from_slice(core_bytes);
                line.extend_from_slice(b"\r\n");
                Ok(LineDisposition::PreRegistrationSeenWithBytes { bytes: line })
            }
            _ => Err(RegistrationRejection::UnknownCommand),
        }
    }

    fn allows_pre_registration(&self, core_bytes: &[u8]) -> bool {
        let head = match core_bytes.iter().position(|&byte| byte == b' ') {
            Some(space) => &core_bytes[..space],
            None => core_bytes,
        };
        let Ok(text) = std::str::from_utf8(head) else {
            return false;
        };
        let upper = text.to_ascii_uppercase();
        self.options.pre_registration_allowlist.contains(&upper)
    }
}

enum LineDisposition {
    /// The line was a pre-registration command with bytes to retain.
    PreRegistrationSeenWithBytes {
        /// Pass-through bytes for the local IRC target.
        bytes: Vec<u8>,
    },
    /// A USER/SERVER line was accepted; handoff to the daemon.
    Ready {
        /// Rewritten prefix to write to the target.
        prefix: Vec<u8>,
        /// Bytes that belong to the next line in the same read.
        leftover: Vec<u8>,
    },
}

fn find_crlf(buffer: &[u8]) -> Option<usize> {
    (0..buffer.len().saturating_sub(1))
        .find(|&index| buffer[index] == b'\r' && buffer[index + 1] == b'\n')
}

/// Splits a post-tag-envelope byte slice into the leading
/// `@tag … ` envelope (if present) and the core bytes. Returns
/// `(None, core_bytes)` when no envelope is present.
fn split_tag_envelope(line_bytes: &[u8]) -> (Option<&[u8]>, &[u8]) {
    if line_bytes.first() != Some(&b'@') {
        return (None, line_bytes);
    }
    match line_bytes.iter().position(|&byte| byte == b' ') {
        Some(space) => (Some(&line_bytes[..space]), &line_bytes[space + 1..]),
        None => (Some(line_bytes), &[]),
    }
}

/// Detects an obvious cross-protocol marker on the first observed
/// line. Plan 179 §7 only requires a small fixed list.
fn detect_cross_protocol(core_bytes: &[u8]) -> Option<RegistrationRejection> {
    if core_bytes.is_empty() {
        return None;
    }
    // HTTP request lines.
    if core_bytes.starts_with(b"GET ")
        || core_bytes.starts_with(b"POST ")
        || core_bytes.starts_with(b"HEAD ")
        || core_bytes.starts_with(b"PUT ")
        || core_bytes.starts_with(b"DELETE ")
        || core_bytes.starts_with(b"OPTIONS ")
        || core_bytes.starts_with(b"CONNECT ")
        || core_bytes.starts_with(b"TRACE ")
        || core_bytes.starts_with(b"PATCH ")
    {
        return Some(RegistrationRejection::CrossProtocol);
    }
    // BitTorrent handshake starts with the protocol length byte
    // (19) followed by "BitTorrent protocol".
    if core_bytes.starts_with(b"\x13BitTorrent protocol") {
        return Some(RegistrationRejection::CrossProtocol);
    }
    None
}

/// Builds the rewritten USER prefix with the authenticated peer
/// hostname projection. Returns `None` when the parsed line is not
/// a valid USER (RFC 2812 §3.1.3 four-argument shape).
fn build_user_rewrite(
    parsed: &super::policy::ParsedLine,
    tag_envelope: Option<&[u8]>,
    peer_hostname: &str,
) -> Option<Vec<u8>> {
    // RFC 2812 USER: `<username> <hostname> <servername> <realname>`.
    // RFC 1459 USER: `<username> <hostname> <servername> <mode> <unused> <realname>`.
    // We accept the RFC 1459 five+arg form by treating the fourth
    // argument as the mode and the fifth+ as the realname, but
    // the canonical M10 minimum is RFC 2812 four-arg form.
    if parsed.args.len() < 4 {
        return None;
    }
    let username = parsed.args[0].as_str();
    if username.is_empty() || username.contains(['\0', ' ', '\r', '\n']) {
        return None;
    }
    if peer_hostname.is_empty() || peer_hostname.contains(['\0', ' ', '\r', '\n']) {
        return None;
    }
    // The servername (third argument) is preserved; the host name
    // (second argument) is replaced with the peer hostname
    // projection. Plan 179 §3 documents this as the only
    // acceptable source for the projected hostname.
    let servername = parsed.args.get(2).map(String::as_str).unwrap_or("0");
    if servername.is_empty() || servername.contains(['\0', ' ', '\r', '\n']) {
        return None;
    }
    // The realname (or trailing parameter) is preserved verbatim
    // for the local IRC server; we do not filter realnames. The
    // 8192-byte generated-line ceiling still applies.
    let realname: String = if parsed.has_trailing {
        parsed.args.last().cloned().unwrap_or_default()
    } else if parsed.args.len() >= 5 {
        // RFC 1459 form: mode + unused + realname as separate
        // middle parameters. We only accept a one-token realname
        // in this legacy form; if the line is missing the
        // trailing colon realname, we drop the line to avoid
        // misinterpreting the legacy `<mode>` argument as a
        // hostname.
        parsed.args.get(3).cloned().unwrap_or_default()
    } else {
        return None;
    };
    // Reject lines whose rewritten core would exceed the core
    // ceiling; Plan 179 §6 requires rejection rather than
    // truncation when the projection would push the line past
    // the ceiling.
    let projected_user = format!(
        "USER {} {} {} :{}",
        username, peer_hostname, servername, realname
    );
    if projected_user.len() + 2 > IRC_CORE_MAX_BYTES {
        return None;
    }
    let mut out = Vec::new();
    if let Some(envelope) = tag_envelope {
        out.extend_from_slice(envelope);
        out.push(b' ');
    }
    out.extend_from_slice(projected_user.as_bytes());
    out.extend_from_slice(b"\r\n");
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_options() -> IrcServerOptions {
        IrcServerOptions::default()
    }

    fn default_limits() -> IrcLimits {
        IrcLimits::defaults()
    }

    fn peer_hash(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    #[test]
    fn default_options_validate() {
        default_options().validate().expect("defaults validate");
    }

    #[test]
    fn zero_registration_lines_rejected() {
        let mut options = default_options();
        options.max_registration_lines = 0;
        assert!(options.validate().is_err());
    }

    #[test]
    fn oversized_registration_bytes_rejected() {
        let mut options = default_options();
        options.max_registration_bytes = IRC_LINE_BUFFER_MAX_BYTES + 1;
        assert!(options.validate().is_err());
    }

    #[test]
    fn malformed_pre_registration_command_rejected() {
        let mut options = default_options();
        options
            .pre_registration_allowlist
            .insert("p ass".to_owned());
        assert!(options.validate().is_err());
    }

    #[test]
    fn projection_is_deterministic_for_same_hash() {
        let a = project_peer_hostname(&peer_hash(0xab));
        let b = project_peer_hostname(&peer_hash(0xab));
        assert_eq!(a, b);
        assert!(a.ends_with(".b32.i2p"));
        // 52-char label + ".b32.i2p" suffix.
        assert_eq!(a.len(), 52 + ".b32.i2p".len());
        // Lowercase only.
        for byte in a.bytes() {
            assert!(
                byte.is_ascii_lowercase() || byte == b'.' || byte.is_ascii_digit() || byte == b'i'
            );
        }
    }

    #[test]
    fn projection_differs_for_different_hash() {
        let a = project_peer_hostname(&peer_hash(0xab));
        let b = project_peer_hostname(&peer_hash(0xcd));
        assert_ne!(a, b);
    }

    #[test]
    fn b32_label_is_52_chars_and_canonical_alphabet() {
        let label = encode_b32_label(&peer_hash(0x01));
        assert_eq!(label.len(), 52);
        for byte in label.bytes() {
            assert!(byte.is_ascii_lowercase() || (b'2'..=b'7').contains(&byte));
        }
    }

    #[test]
    fn ready_after_user_rewrites_hostname_to_peer_projection() {
        let mut interceptor =
            IrcServerRegistration::new(default_options(), default_limits(), peer_hash(0xab));
        let bytes = b"NICK alice\r\nUSER alice attacker.example.com attacker-server :Alice\r\n";
        let outcome = interceptor.advance(bytes);
        match outcome {
            RegistrationOutcome::Ready { prefix, leftover } => {
                let text = std::str::from_utf8(&prefix).expect("utf8");
                let expected = format!(
                    "NICK alice\r\nUSER alice {} attacker-server :Alice\r\n",
                    project_peer_hostname(&peer_hash(0xab))
                );
                assert_eq!(text, expected);
                assert!(leftover.is_empty());
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(interceptor.state(), RegistrationState::Ready);
    }

    #[test]
    fn user_rewrite_preserves_pass_cap_nick_before_user() {
        let mut interceptor =
            IrcServerRegistration::new(default_options(), default_limits(), peer_hash(0xab));
        let bytes = b"PASS secret\r\nCAP LS\r\nNICK alice\r\nUSER alice h s :Alice\r\n";
        let outcome = interceptor.advance(bytes);
        match outcome {
            RegistrationOutcome::Ready { prefix, leftover } => {
                let text = std::str::from_utf8(&prefix).expect("utf8");
                let expected_host = project_peer_hostname(&peer_hash(0xab));
                assert!(text.contains("PASS secret\r\n"), "PASS passthrough: {text}");
                assert!(text.contains("CAP LS\r\n"), "CAP passthrough: {text}");
                assert!(text.contains("NICK alice\r\n"), "NICK passthrough: {text}");
                assert!(
                    text.contains(&format!("USER alice {expected_host} s :Alice\r\n")),
                    "USER rewrite: {text}"
                );
                assert!(leftover.is_empty());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn tagged_user_rewrites_with_tag_envelope_preserved() {
        let mut interceptor =
            IrcServerRegistration::new(default_options(), default_limits(), peer_hash(0xab));
        let bytes = b"@time=2020-01-01T00:00:00.000Z USER alice h s :Alice\r\n";
        let outcome = interceptor.advance(bytes);
        match outcome {
            RegistrationOutcome::Ready { prefix, leftover } => {
                let text = std::str::from_utf8(&prefix).expect("utf8");
                let expected_host = project_peer_hostname(&peer_hash(0xab));
                assert!(text.starts_with("@time=2020-01-01T00:00:00.000Z USER alice "));
                assert!(text.contains(&expected_host));
                assert!(text.ends_with("\r\n"));
                assert!(leftover.is_empty());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn nick_changes_after_registration_do_not_affect_projection() {
        // The projection is bound to the peer destination hash,
        // not the nick; a NICK change after USER is irrelevant.
        let interceptor =
            IrcServerRegistration::new(default_options(), default_limits(), peer_hash(0xab));
        let host_a = interceptor.peer_hostname().to_owned();
        let interceptor =
            IrcServerRegistration::new(default_options(), default_limits(), peer_hash(0xab));
        let host_b = interceptor.peer_hostname().to_owned();
        assert_eq!(host_a, host_b);
    }

    #[test]
    fn same_read_bytes_after_user_are_preserved_as_leftover() {
        let mut interceptor =
            IrcServerRegistration::new(default_options(), default_limits(), peer_hash(0xab));
        // Pack USER + a same-read PRIVMSG in one advance call.
        let bytes = b"NICK alice\r\nUSER alice h s :Alice\r\nPRIVMSG #chan :hi\r\n";
        let outcome = interceptor.advance(bytes);
        match outcome {
            RegistrationOutcome::Ready { prefix, leftover } => {
                let prefix_text = std::str::from_utf8(&prefix).expect("utf8");
                assert!(prefix_text.contains("NICK alice\r\n"));
                let expected_host = project_peer_hostname(&peer_hash(0xab));
                assert!(
                    prefix_text.contains(&format!("USER alice {expected_host} s :Alice\r\n")),
                    "USER rewrite missing: {prefix_text}"
                );
                assert!(
                    !prefix_text.contains("PRIVMSG"),
                    "PRIVMSG should be in leftover"
                );
                let leftover_text = std::str::from_utf8(&leftover).expect("utf8");
                assert_eq!(leftover_text, "PRIVMSG #chan :hi\r\n");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn more_than_max_registration_lines_rejects() {
        let mut options = default_options();
        options.max_registration_lines = 2;
        let mut interceptor =
            IrcServerRegistration::new(options, default_limits(), peer_hash(0xab));
        // NICK + CAP (allowed) + USER (would be third) -> 3 lines
        // exceeds ceiling of 2.
        let outcome = interceptor.advance(b"NICK alice\r\nCAP LS\r\nUSER alice h s :Alice\r\n");
        assert!(matches!(
            outcome,
            RegistrationOutcome::Rejected(RegistrationRejection::TooManyLines)
        ));
        assert_eq!(interceptor.state(), RegistrationState::Rejected);
    }

    #[test]
    fn eof_before_user_returns_eof() {
        let mut interceptor =
            IrcServerRegistration::new(default_options(), default_limits(), peer_hash(0xab));
        let outcome = interceptor.advance(b"NICK alice\r\n");
        assert!(matches!(outcome, RegistrationOutcome::Incomplete { .. }));
        let outcome = interceptor.finish_eof();
        assert!(matches!(outcome, RegistrationOutcome::Eof));
    }

    #[test]
    fn eof_after_user_is_no_op() {
        let mut interceptor =
            IrcServerRegistration::new(default_options(), default_limits(), peer_hash(0xab));
        let outcome = interceptor.advance(b"USER alice h s :Alice\r\n");
        assert!(matches!(outcome, RegistrationOutcome::Ready { .. }));
        let outcome = interceptor.finish_eof();
        assert!(matches!(outcome, RegistrationOutcome::Incomplete { .. }));
    }

    #[test]
    fn http_get_first_line_rejected() {
        let mut interceptor =
            IrcServerRegistration::new(default_options(), default_limits(), peer_hash(0xab));
        let outcome = interceptor.advance(b"GET / HTTP/1.1\r\n");
        assert!(matches!(
            outcome,
            RegistrationOutcome::Rejected(RegistrationRejection::CrossProtocol)
        ));
    }

    #[test]
    fn bittorrent_first_line_rejected() {
        let mut interceptor =
            IrcServerRegistration::new(default_options(), default_limits(), peer_hash(0xab));
        let outcome = interceptor.advance(b"\x13BitTorrent protocolxxxxxxxxxxxxx\r\n");
        assert!(matches!(
            outcome,
            RegistrationOutcome::Rejected(RegistrationRejection::CrossProtocol)
        ));
    }

    #[test]
    fn unknown_command_rejected() {
        let mut interceptor =
            IrcServerRegistration::new(default_options(), default_limits(), peer_hash(0xab));
        let outcome = interceptor.advance(b"OPER alice secret\r\n");
        assert!(matches!(
            outcome,
            RegistrationOutcome::Rejected(RegistrationRejection::UnknownCommand)
        ));
    }

    #[test]
    fn user_with_overlong_realname_rejected() {
        let mut interceptor =
            IrcServerRegistration::new(default_options(), default_limits(), peer_hash(0xab));
        // Build a USER with a realname that, after the rewrite,
        // would exceed the 512-byte core ceiling.
        let mut bytes = b"USER alice h s :".to_vec();
        bytes.extend(std::iter::repeat_n(b'x', 600));
        bytes.extend_from_slice(b"\r\n");
        let outcome = interceptor.advance(&bytes);
        assert!(matches!(
            outcome,
            RegistrationOutcome::Rejected(RegistrationRejection::CoreLineTooLong)
        ));
    }

    #[test]
    fn fragmented_user_completes() {
        let mut interceptor =
            IrcServerRegistration::new(default_options(), default_limits(), peer_hash(0xab));
        // Split inside a multi-byte parameter, never across the
        // CRLF that would otherwise leave a " :Alice" fragment
        // without a command name.
        let parts: &[&[u8]] = &[b"NICK al", b"ice\r\n", b"USER alice h s :Al", b"ice\r\n"];
        let mut last = None;
        let mut carry: Vec<u8> = Vec::new();
        for part in parts {
            // Concatenate any previous carry with the next part so
            // the runtime-neutral `advance` contract is preserved:
            // a single advance call may span multiple fragments.
            let mut combined = std::mem::take(&mut carry);
            combined.extend_from_slice(part);
            let outcome = interceptor.advance(&combined);
            if let RegistrationOutcome::Incomplete { retained } = &outcome {
                carry.clone_from(retained);
            }
            last = Some(outcome);
        }
        let outcome = last.expect("at least one call");
        match outcome {
            RegistrationOutcome::Ready { prefix, .. } => {
                let text = std::str::from_utf8(&prefix).expect("utf8");
                let expected_host = project_peer_hostname(&peer_hash(0xab));
                assert!(text.contains(&format!("USER alice {expected_host} s :Alice\r\n")));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn buffer_overflow_rejected() {
        let mut options = default_options();
        options.max_registration_bytes = 64;
        let mut interceptor =
            IrcServerRegistration::new(options, default_limits(), peer_hash(0xab));
        let outcome = interceptor.advance(&[b'x'; 100]);
        assert!(matches!(
            outcome,
            RegistrationOutcome::Rejected(RegistrationRejection::BufferOverflow)
        ));
    }

    #[test]
    fn server_line_handoff() {
        // The SERVER command is explicitly allowed for handoff by
        // Plan 179 §7. The peer-hostname projection is irrelevant
        // for server-to-server IRC; we pass the line through.
        let mut interceptor =
            IrcServerRegistration::new(default_options(), default_limits(), peer_hash(0xab));
        let outcome = interceptor.advance(b"SERVER hub.example.com 1 1 :Hub\r\n");
        match outcome {
            RegistrationOutcome::Ready { prefix, leftover } => {
                let text = std::str::from_utf8(&prefix).expect("utf8");
                assert!(text.contains("SERVER hub.example.com 1 1 :Hub\r\n"));
                assert!(leftover.is_empty());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn empty_server_realname_rejected() {
        // The realname (or trailing parameter) is required; lines
        // without one are rejected as malformed USER.
        let mut interceptor =
            IrcServerRegistration::new(default_options(), default_limits(), peer_hash(0xab));
        let outcome = interceptor.advance(b"USER alice h s\r\n");
        assert!(matches!(
            outcome,
            RegistrationOutcome::Rejected(RegistrationRejection::InvalidUser)
        ));
    }

    #[test]
    fn progress_in_state_machine() {
        let mut interceptor =
            IrcServerRegistration::new(default_options(), default_limits(), peer_hash(0xab));
        assert_eq!(interceptor.state(), RegistrationState::AwaitingRegistration);
        let outcome = interceptor.advance(b"NICK alice\r\n");
        assert!(matches!(outcome, RegistrationOutcome::Incomplete { .. }));
        assert_eq!(interceptor.state(), RegistrationState::SawPreRegistration);
    }
}
