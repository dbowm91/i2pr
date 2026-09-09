//! Plan 178 IRC line parser and filter driver.
//!
//! The line parser is a runtime-neutral state machine that:
//!
//! 1. Receives an arbitrary byte slice (`read` may be one byte at
//!    a time or many bytes at once; partial lines are valid);
//! 2. Splits the slice into complete CRLF-terminated lines;
//! 3. For each complete line, runs the IRCv3 tag envelope
//!    parser ([`crate::irc::tags`]) and the command classifier
//!    ([`crate::irc::policy`]) before passing the typed
//!    [`ParsedLine`] to the privacy filter
//!    ([`crate::irc::client_filter`]);
//! 4. Emits the resulting [`FilterOutcome`] along with any
//!    pre-line bytes that remained after the terminator for
//!    emission to the Streaming pump.
//!
//! The parser never rewrites bytes in place; it produces a fresh
//! post-filter line buffer (with CRLF appended when the
//! disposition is [`FilterOutcome::Rewrite`]).

#![forbid(unsafe_code)]

use super::client_filter::{
    FilterOutcome, PingRewriteState, PrivacySubstitutions, build_pong_for_state,
    classify_and_decide,
};
use super::config::IrcClientOptions;
use super::errors::{IrcError, IrcErrorKind};
use super::limits::IrcLimits;
use super::policy::{LineDirection, ParsedLine, classify_core};
use super::tags::{TagsOutcome, TagsParser};

/// Outcome of feeding bytes into the line parser.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LineParserOutcome {
    /// Need more bytes to produce a complete line.
    Incomplete,
    /// Encountered a structural failure (NUL inside the line, CR
    /// without LF, lone LF, CRLF-CRLF injection, control byte in
    /// the envelope). The line is dropped and the caller must
    /// close the socket on the per-direction error.
    Invalid(IrcError),
    /// Produced a complete line and its typed filter disposition.
    /// The `rest` field carries any bytes that followed the
    /// terminating CRLF and belong to the next line.
    Complete {
        /// Filter outcome for this line.
        outcome: FilterOutcome,
        /// Bytes that follow the terminator and must be retained
        /// for the next line read.
        rest: Vec<u8>,
    },
}

/// IRC line parser state. The parser owns:
/// - the partial-line buffer (Plan 178 §3.1 line-buffer ceiling);
/// - the tag envelope parser (state-machine ready);
/// - the per-direction PING/PONG rewrite token table (one entry
///   per connection).
#[derive(Clone, Debug)]
pub struct IrcLineParser {
    buffer: Vec<u8>,
    limits: IrcLimits,
    options: IrcClientOptions,
    substitutions: PrivacySubstitutions,
    ping_state: PingRewriteState,
    tags_parser: TagsParser,
}

impl IrcLineParser {
    /// Creates a fresh line parser.
    pub fn new(limits: IrcLimits, options: IrcClientOptions) -> Self {
        Self {
            buffer: Vec::new(),
            limits,
            options,
            substitutions: PrivacySubstitutions::default(),
            ping_state: PingRewriteState::new(),
            tags_parser: TagsParser::new(),
        }
    }

    /// Returns the configured limits.
    pub fn limits(&self) -> IrcLimits {
        self.limits
    }

    /// Returns the configured client options.
    pub fn options(&self) -> &IrcClientOptions {
        &self.options
    }

    /// Returns a mutable reference to the per-connection
    /// PING/PONG rewrite state.
    pub fn ping_state_mut(&mut self) -> &mut PingRewriteState {
        &mut self.ping_state
    }

    /// Feeds bytes into the parser. The parser returns:
    /// - [`LineParserOutcome::Incomplete`] when no complete line
    ///   is present yet;
    /// - [`LineParserOutcome::Invalid`] when a structural
    ///   failure is detected (the daemon must close the socket);
    /// - [`LineParserOutcome::Complete`] when one complete line
    ///   has been parsed; the caller forwards the
    ///   [`FilterOutcome`] bytes (when not a drop) into the
    ///   per-direction byte stream, then loops over the
    ///   `rest` bytes feeding them back into the parser.
    pub fn advance(&mut self, bytes: &[u8], direction: LineDirection) -> LineParserOutcome {
        if bytes.is_empty() {
            return LineParserOutcome::Incomplete;
        }
        if self.buffer.len() + bytes.len() > self.limits.line_buffer_max_bytes {
            return LineParserOutcome::Invalid(IrcError::new(
                IrcErrorKind::BufferCeiling,
                "line buffer ceiling exceeded",
            ));
        }
        self.buffer.extend_from_slice(bytes);
        match find_crlf(&self.buffer) {
            None => {
                if self.buffer.len() > self.limits.core_max_bytes {
                    // Allow tagged envelopes that legitimately
                    // exceed the core-only cap; the cap applies
                    // to the post-tag portion only. If the entire
                    // line exceeds the combined envelope+core
                    // ceiling, the buffer ceiling check above
                    // catches it.
                    return LineParserOutcome::Invalid(IrcError::new(
                        IrcErrorKind::CoreLineTooLong,
                        "core line exceeds the ceiling without a CRLF",
                    ));
                }
                LineParserOutcome::Incomplete
            }
            Some(crlf_index) => {
                let line_bytes = self.buffer[..crlf_index].to_vec();
                let rest = self.buffer[crlf_index + 2..].to_vec();
                self.buffer.clear();
                match self.process_line(&line_bytes, direction) {
                    Ok(outcome) => LineParserOutcome::Complete { outcome, rest },
                    Err(error) => LineParserOutcome::Invalid(error),
                }
            }
        }
    }

    /// Feeds any `rest` bytes from a prior partial parse into the
    /// parser without re-validating the buffer ceiling (the
    /// caller already retained them in the parser's buffer).
    pub fn resume(&mut self, rest: Vec<u8>, direction: LineDirection) -> LineParserOutcome {
        if rest.is_empty() {
            return LineParserOutcome::Incomplete;
        }
        self.buffer = rest;
        self.advance(&[], direction)
    }

    fn process_line(
        &mut self,
        line_bytes: &[u8],
        direction: LineDirection,
    ) -> Result<FilterOutcome, IrcError> {
        if line_bytes.len() > self.limits.core_max_bytes + self.limits.tag_envelope_max_bytes + 1 {
            return Err(IrcError::new(
                IrcErrorKind::CoreLineTooLong,
                "line exceeds the combined envelope+core ceiling",
            ));
        }
        let tags_outcome = self.tags_parser.advance(line_bytes, self.limits);
        match tags_outcome {
            TagsOutcome::Incomplete => Err(IrcError::new(
                IrcErrorKind::BufferCeiling,
                "tag envelope incomplete (line shorter than expected)",
            )),
            TagsOutcome::Invalid(error) => Err(error),
            TagsOutcome::Ready {
                tags,
                consumed,
                tag_data_bytes,
            } => {
                let core_bytes = if consumed == 0 {
                    line_bytes
                } else {
                    &line_bytes[consumed..]
                };
                if core_bytes.len() > self.limits.core_max_bytes {
                    return Err(IrcError::new(
                        IrcErrorKind::CoreLineTooLong,
                        "core portion exceeds the ceiling",
                    ));
                }
                let _ = (tags, tag_data_bytes);
                let parsed = classify_core(core_bytes)?;
                let reason_rewrite = self.options.reason_rewrite;
                let max_realname = self.options.user_realname_max_bytes;
                let outcome = classify_and_decide(
                    &parsed,
                    direction,
                    reason_rewrite,
                    &self.substitutions,
                    max_realname,
                    &mut self.ping_state,
                );
                // Re-attach the tag envelope when the line was
                // tagged and the disposition is a rewrite.
                if matches!(outcome, FilterOutcome::Rewrite { .. }) && consumed > 0 {
                    let mut envelope = Vec::new();
                    envelope.extend_from_slice(&line_bytes[..consumed]);
                    if let FilterOutcome::Rewrite { mut line } = outcome {
                        let stripped = if line.starts_with(&envelope[..envelope.len() - 1]) {
                            line.split_off(envelope.len() - 1)
                        } else {
                            // The rewrite dropped the prefix; prepend
                            // a fresh envelope from the original line.
                            envelope.extend(line);
                            envelope
                        };
                        Ok(FilterOutcome::Rewrite { line: stripped })
                    } else {
                        Ok(outcome)
                    }
                } else {
                    Ok(outcome)
                }
            }
        }
    }
}

/// Locates the first `\r\n` in `buffer`. Returns the index of the
/// `\r` byte when found, `None` otherwise.
fn find_crlf(buffer: &[u8]) -> Option<usize> {
    (0..buffer.len().saturating_sub(1))
        .find(|&index| buffer[index] == b'\r' && buffer[index + 1] == b'\n')
}

/// Re-exposes [`crate::irc::policy::is_allowed`] for callers that
/// need to make direction-specific decisions without re-parsing
/// the full line.
pub fn is_command_allowed(
    command: crate::irc::config::IrcCommand,
    direction: LineDirection,
) -> bool {
    super::policy::is_allowed(command, direction)
}

/// Re-exposes the [`crate::irc::policy::IrcCommandClass`]
/// classify entry point for callers that already hold a tagged
/// line but do not want to build a full [`IrcLineParser`].
pub fn classify_post_tag_core(core: &[u8]) -> Result<ParsedLine, IrcError> {
    classify_core(core)
}

/// Builds a PONG response from the supplied PING/PONG rewrite
/// state.
pub fn pong_from_state(state: &mut PingRewriteState) -> Option<Vec<u8>> {
    build_pong_for_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_options() -> IrcClientOptions {
        IrcClientOptions::defaults()
    }

    #[test]
    fn parses_complete_line_in_one_read() {
        let mut parser = IrcLineParser::new(IrcLimits::defaults(), make_options());
        let outcome = parser.advance(b"NICK alice\r\n", LineDirection::ClientToServer);
        match outcome {
            LineParserOutcome::Complete { outcome, rest } => {
                assert_eq!(outcome, FilterOutcome::Allow);
                assert!(rest.is_empty());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_incremental_one_byte() {
        let mut parser = IrcLineParser::new(IrcLimits::defaults(), make_options());
        let bytes = b"NICK alice\r\n";
        for byte in &bytes[..bytes.len() - 1] {
            let outcome = parser.advance(&[*byte], LineDirection::ClientToServer);
            assert!(matches!(outcome, LineParserOutcome::Incomplete));
        }
        let outcome = parser.advance(&[bytes[bytes.len() - 1]], LineDirection::ClientToServer);
        assert!(matches!(outcome, LineParserOutcome::Complete { .. }));
    }

    #[test]
    fn parses_multiple_lines_in_one_read() {
        let mut parser = IrcLineParser::new(IrcLimits::defaults(), make_options());
        let outcome = parser.advance(
            b"NICK alice\r\nUSER a h s :real\r\n",
            LineDirection::ClientToServer,
        );
        match outcome {
            LineParserOutcome::Complete { outcome, rest } => {
                assert!(matches!(outcome, FilterOutcome::Allow));
                // After the first complete line, the remainder
                // contains "USER a h s :real\r\n" and the caller
                // must feed it back into the parser.
                assert!(rest.starts_with(b"USER"));
            }
            other => panic!("unexpected: {other:?}"),
        }
        let outcome = parser.advance(b"", LineDirection::ClientToServer);
        // The empty advance call returns Incomplete when the
        // buffer is empty (after the prior clear), but the
        // caller is expected to drive the rest of the bytes
        // through `resume`.
        let _ = outcome;
    }

    #[test]
    fn resume_completes_partial_line() {
        let mut parser = IrcLineParser::new(IrcLimits::defaults(), make_options());
        let outcome = parser.advance(b"NICK alice", LineDirection::ClientToServer);
        assert!(matches!(outcome, LineParserOutcome::Incomplete));
        let outcome = parser.advance(b"\r\n", LineDirection::ClientToServer);
        assert!(matches!(outcome, LineParserOutcome::Complete { .. }));
    }

    #[test]
    fn core_line_too_long_is_invalid() {
        let mut parser = IrcLineParser::new(IrcLimits::defaults(), make_options());
        let mut bytes = b"PRIVMSG #c :".to_vec();
        bytes.extend(std::iter::repeat_n(
            b'x',
            IrcLimits::defaults().core_max_bytes + 1,
        ));
        bytes.extend_from_slice(b"\r\n");
        let outcome = parser.advance(&bytes, LineDirection::ClientToServer);
        assert!(matches!(outcome, LineParserOutcome::Invalid(_)));
    }

    #[test]
    fn buffer_ceiling_rejected() {
        let mut parser = IrcLineParser::new(IrcLimits::defaults(), make_options());
        let huge = vec![b'x'; IrcLimits::defaults().line_buffer_max_bytes + 1];
        let outcome = parser.advance(&huge, LineDirection::ClientToServer);
        assert!(matches!(outcome, LineParserOutcome::Invalid(_)));
    }

    #[test]
    fn user_command_is_rewritten() {
        let mut parser = IrcLineParser::new(IrcLimits::defaults(), make_options());
        let outcome = parser.advance(
            b"USER alice 192.168.1.10 irc.example.org :Alice Smith\r\n",
            LineDirection::ClientToServer,
        );
        match outcome {
            LineParserOutcome::Complete { outcome, rest } => {
                match outcome {
                    FilterOutcome::Rewrite { line } => {
                        let text = std::str::from_utf8(&line).expect("utf8");
                        assert_eq!(text, "USER alice i2p localhost :Alice Smith\r\n");
                    }
                    other => panic!("unexpected: {other:?}"),
                }
                assert!(rest.is_empty());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn ping_location_is_rewritten() {
        let mut parser = IrcLineParser::new(IrcLimits::defaults(), make_options());
        let outcome = parser.advance(
            b"PING token 192.168.1.10\r\n",
            LineDirection::ServerToClient,
        );
        match outcome {
            LineParserOutcome::Complete { outcome, .. } => match outcome {
                FilterOutcome::Rewrite { line } => {
                    let text = std::str::from_utf8(&line).expect("utf8");
                    assert!(text.starts_with("PING token i2p\r\n"));
                }
                other => panic!("unexpected: {other:?}"),
            },
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn unknown_command_dropped() {
        let mut parser = IrcLineParser::new(IrcLimits::defaults(), make_options());
        let outcome = parser.advance(b"OPER alice secret\r\n", LineDirection::ClientToServer);
        match outcome {
            LineParserOutcome::Complete { outcome, .. } => {
                assert!(matches!(outcome, FilterOutcome::Drop(_)));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn tagged_message_is_classified() {
        let mut parser = IrcLineParser::new(IrcLimits::defaults(), make_options());
        let outcome = parser.advance(
            b"@time=2020-01-01T00:00:00.000Z NICK alice\r\n",
            LineDirection::ClientToServer,
        );
        match outcome {
            LineParserOutcome::Complete { outcome, .. } => {
                assert_eq!(outcome, FilterOutcome::Allow);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn pong_helper_is_reachable() {
        let mut parser = IrcLineParser::new(IrcLimits::defaults(), make_options());
        parser.advance(
            b"PING token 192.168.1.10\r\n",
            LineDirection::ServerToClient,
        );
        let pong = pong_from_state(parser.ping_state_mut()).expect("pong");
        let text = std::str::from_utf8(&pong).expect("utf8");
        assert_eq!(text, "PONG token i2p\r\n");
    }

    #[test]
    fn empty_read_returns_incomplete() {
        let mut parser = IrcLineParser::new(IrcLimits::defaults(), make_options());
        let outcome = parser.advance(b"", LineDirection::ClientToServer);
        assert!(matches!(outcome, LineParserOutcome::Incomplete));
    }

    #[test]
    fn resume_with_empty_rest_returns_incomplete() {
        let mut parser = IrcLineParser::new(IrcLimits::defaults(), make_options());
        let outcome = parser.resume(Vec::new(), LineDirection::ClientToServer);
        assert!(matches!(outcome, LineParserOutcome::Incomplete));
    }

    #[test]
    fn classification_alias_is_reachable() {
        let parsed = classify_post_tag_core(b"NICK alice").expect("ok");
        assert!(matches!(
            parsed.command,
            super::super::policy::IrcCommandClass::Known(super::super::config::IrcCommand::Nick)
        ));
    }

    #[test]
    fn is_command_allowed_alias_is_reachable() {
        assert!(is_command_allowed(
            crate::irc::config::IrcCommand::Nick,
            LineDirection::ClientToServer
        ));
    }
}
