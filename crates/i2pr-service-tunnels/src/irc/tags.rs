//! Plan 178 IRCv3 message-tag framing.
//!
//! Parses the IRCv3 message-tag envelope (`@tag1=val1;tag2=val2;…`)
//! from the start of an IRC line. Tag names/values are treated as
//! opaque per IRCv3 framing; the parser enforces:
//!
//! - total envelope ceiling [`IRC_TAG_ENVELOPE_MAX_BYTES`](super::config::IRC_TAG_ENVELOPE_MAX_BYTES);
//! - per-tag key ceiling [`IRC_TAG_KEY_MAX_BYTES`](super::config::IRC_TAG_KEY_MAX_BYTES);
//! - cumulative tag-value ceiling [`IRC_CLIENT_TAG_DATA_MAX_BYTES`](super::config::IRC_CLIENT_TAG_DATA_MAX_BYTES);
//! - total tag count ceiling [`IRC_TAG_COUNT_MAX`](super::config::IRC_TAG_COUNT_MAX);
//! - structural validity: `=` separates key and value (value may
//!   be empty), tags are `;`-separated, key/value characters are
//!   restricted to the IRCv3 grammar subset (`[A-Za-z0-9_\-\./]` for
//!   keys and tag-value characters plus `\:` escapes for `;` and
//!   space inside values).
//!
//! The parser never replaces invalid UTF-8 bytes in tag values
//! with replacement characters; it surfaces a typed
//! [`IrcErrorKind::InvalidTag`] instead. Tag presence does not
//! bypass command filtering (the line classifier in
//! [`crate::irc::policy`] runs over the post-tag core).
//!
//! See [IRCv3 Message Tags](https://ircv3.net/specs/extensions/message-tags.html)
//! for the reference grammar.

#![forbid(unsafe_code)]

use super::errors::{IrcError, IrcErrorKind};
use super::limits::IrcLimits;

/// One parsed IRCv3 message tag. Tag values are treated as opaque
/// byte spans inside the envelope; tag keys are ASCII-only per
/// IRCv3 grammar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrcTag {
    /// Tag key (lowercased canonical form).
    pub key: String,
    /// Tag value bytes (un-escaped). Empty when the tag was
    /// `tag=` with no value.
    pub value: Vec<u8>,
    /// Byte offset in the original envelope where this tag's
    /// value bytes started (after the `=`). For empty-value tags
    /// the offset points at the trailing `;` or end-of-envelope.
    pub value_offset: usize,
}

/// Outcome of tag-envelope parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TagsOutcome {
    /// Envelope is incomplete; more bytes are required.
    Incomplete,
    /// Envelope was structurally invalid; the line must be
    /// dropped.
    Invalid(IrcError),
    /// Envelope parsed. The remainder is the bytes after the
    /// separating space (the IRC core message).
    Ready {
        /// Parsed tags in envelope order.
        tags: Vec<IrcTag>,
        /// Number of bytes consumed from the input (including
        /// the separating space).
        consumed: usize,
        /// Cumulative tag-value bytes (un-escaped).
        tag_data_bytes: usize,
    },
}

/// Incremental IRCv3 tag envelope parser.
#[derive(Clone, Debug, Default)]
pub struct TagsParser {
    buffer: Vec<u8>,
}

impl TagsParser {
    /// Creates a fresh tags parser.
    pub fn new() -> Self {
        Self::default()
    }

    /// Resets the parser, releasing its retained bytes.
    pub fn reset(&mut self) {
        self.buffer.clear();
    }

    /// Returns the bytes retained so far.
    pub fn retained(&self) -> &[u8] {
        &self.buffer
    }

    /// Feeds bytes into the envelope parser. The supplied bytes
    /// may include trailing core-message bytes; the parser
    /// identifies the envelope terminator (the first ASCII space
    /// after `@`) and returns the consumed count.
    pub fn advance(&mut self, bytes: &[u8], limits: IrcLimits) -> TagsOutcome {
        if self.buffer.len() + bytes.len() > limits.line_buffer_max_bytes {
            return TagsOutcome::Invalid(IrcError::new(
                IrcErrorKind::BufferCeiling,
                "tag envelope exceeds the line buffer ceiling",
            ));
        }
        self.buffer.extend_from_slice(bytes);
        // The envelope must begin with `@`; lines without a leading
        // `@` carry no envelope and the parser returns Incomplete
        // until it sees the `@` prefix.
        if self.buffer.is_empty() {
            return TagsOutcome::Incomplete;
        }
        if self.buffer[0] != b'@' {
            // Not a tagged envelope: caller treats the buffer as
            // core-only and consumes it directly.
            return TagsOutcome::Ready {
                tags: Vec::new(),
                consumed: 0,
                tag_data_bytes: 0,
            };
        }
        // Search for the envelope terminator: the first ASCII space
        // after the leading `@`. Lines shorter than the terminator
        // are incomplete; lines whose envelope (including `@` and
        // everything before the space) exceeds the ceiling are
        // invalid.
        match self.buffer.iter().position(|&byte| byte == b' ') {
            None => {
                if self.buffer.len() > limits.tag_envelope_max_bytes {
                    return TagsOutcome::Invalid(IrcError::new(
                        IrcErrorKind::TagEnvelopeTooLong,
                        "tag envelope exceeds the ceiling",
                    ));
                }
                TagsOutcome::Incomplete
            }
            Some(space_index) => {
                if space_index > limits.tag_envelope_max_bytes {
                    return TagsOutcome::Invalid(IrcError::new(
                        IrcErrorKind::TagEnvelopeTooLong,
                        "tag envelope exceeds the ceiling",
                    ));
                }
                let envelope_bytes = &self.buffer[1..space_index];
                let parsed = match parse_tag_block(envelope_bytes, limits) {
                    Ok(value) => value,
                    Err(error) => return TagsOutcome::Invalid(error),
                };
                TagsOutcome::Ready {
                    tags: parsed.0,
                    consumed: space_index + 1,
                    tag_data_bytes: parsed.1,
                }
            }
        }
    }
}

/// Parses the body of the tag envelope (bytes after `@`, before
/// the separating space). Returns the tag list and cumulative
/// tag-data byte count.
fn parse_tag_block(body: &[u8], limits: IrcLimits) -> Result<(Vec<IrcTag>, usize), IrcError> {
    let invalid = |kind, reason| IrcError::new(kind, reason);
    let mut tags: Vec<IrcTag> = Vec::new();
    let mut tag_data_bytes: usize = 0;
    if body.is_empty() {
        return Ok((tags, tag_data_bytes));
    }
    for raw_tag in body.split(|&byte| byte == b';') {
        if raw_tag.is_empty() {
            // Tolerated IRCv3 framing: leading or trailing `;`
            // produces an empty tag that is silently ignored.
            continue;
        }
        if tags.len() >= limits.tag_count_max {
            return Err(invalid(
                IrcErrorKind::TooManyTags,
                "too many tags on message",
            ));
        }
        let (key, value) = match raw_tag.iter().position(|&byte| byte == b'=') {
            Some(eq_index) => {
                let key_bytes = &raw_tag[..eq_index];
                let value_bytes = &raw_tag[eq_index + 1..];
                if key_bytes.is_empty() {
                    return Err(invalid(
                        IrcErrorKind::InvalidTag,
                        "tag key must not be empty",
                    ));
                }
                if key_bytes.len() > limits.tag_key_max_bytes {
                    return Err(invalid(
                        IrcErrorKind::TagKeyTooLong,
                        "tag key exceeds the ceiling",
                    ));
                }
                let key = std::str::from_utf8(key_bytes)
                    .map_err(|_| invalid(IrcErrorKind::InvalidTag, "tag key is not valid UTF-8"))?;
                let key = key.to_ascii_lowercase();
                if !key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'/' | b'_'))
                {
                    return Err(invalid(
                        IrcErrorKind::InvalidTag,
                        "tag key contains invalid characters",
                    ));
                }
                let mut value = Vec::with_capacity(value_bytes.len());
                let mut iter = value_bytes.iter();
                while let Some(&byte) = iter.next() {
                    match byte {
                        b'\\' => {
                            let Some(&escaped) = iter.next() else {
                                return Err(invalid(
                                    IrcErrorKind::InvalidTag,
                                    "tag value escape is dangling",
                                ));
                            };
                            match escaped {
                                b':' => value.push(b';'),
                                b's' => value.push(b' '),
                                b'\\' => value.push(b'\\'),
                                b'r' => value.push(b'\r'),
                                b'n' => value.push(b'\n'),
                                _ => {
                                    return Err(invalid(
                                        IrcErrorKind::InvalidTag,
                                        "tag value escape is invalid",
                                    ));
                                }
                            }
                        }
                        0..=0x1f | 0x7f => {
                            return Err(invalid(
                                IrcErrorKind::InvalidTag,
                                "tag value contains control byte",
                            ));
                        }
                        _ => value.push(byte),
                    }
                }
                (key, value)
            }
            None => {
                // Valueless tags are legal IRCv3; value defaults to empty.
                let key_bytes = raw_tag;
                if key_bytes.len() > limits.tag_key_max_bytes {
                    return Err(invalid(
                        IrcErrorKind::TagKeyTooLong,
                        "tag key exceeds the ceiling",
                    ));
                }
                let key = std::str::from_utf8(key_bytes)
                    .map_err(|_| invalid(IrcErrorKind::InvalidTag, "tag key is not valid UTF-8"))?;
                let key = key.to_ascii_lowercase();
                if !key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'/' | b'_'))
                {
                    return Err(invalid(
                        IrcErrorKind::InvalidTag,
                        "tag key contains invalid characters",
                    ));
                }
                (key, Vec::new())
            }
        };
        tag_data_bytes = tag_data_bytes.saturating_add(value.len());
        if tag_data_bytes > limits.client_tag_data_max_bytes {
            return Err(invalid(
                IrcErrorKind::TagDataTooLong,
                "tag values exceed the client data ceiling",
            ));
        }
        let value_offset = 1
            + body
                .iter()
                .take_while(|byte| **byte != b';')
                .take_while(|byte| **byte != b'=')
                .count()
            + 1;
        tags.push(IrcTag {
            key,
            value,
            value_offset,
        });
    }
    Ok((tags, tag_data_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> IrcLimits {
        IrcLimits::defaults()
    }

    #[test]
    fn parses_minimal_tag_envelope() {
        let mut parser = TagsParser::new();
        let outcome = parser.advance(
            b"@time=2020-01-01T00:00:00.000Z :nick!u@h PRIVMSG #c :hi",
            limits(),
        );
        let outcome = match outcome {
            TagsOutcome::Ready {
                tags,
                consumed,
                tag_data_bytes,
            } => (tags, consumed, tag_data_bytes),
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(outcome.0.len(), 1);
        assert_eq!(outcome.0[0].key, "time");
        assert_eq!(outcome.0[0].value, b"2020-01-01T00:00:00.000Z".to_vec());
        assert!(outcome.1 >= 31);
        assert!(outcome.2 > 0);
    }

    #[test]
    fn parses_multiple_tags() {
        let mut parser = TagsParser::new();
        let outcome = parser.advance(b"@a=1;b=2;c :nick PRIVMSG #c :hi", limits());
        let outcome = match outcome {
            TagsOutcome::Ready {
                tags,
                consumed,
                tag_data_bytes,
            } => (tags, consumed, tag_data_bytes),
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(outcome.0.len(), 3);
        assert_eq!(outcome.0[0].key, "a");
        assert_eq!(outcome.0[0].value, b"1".to_vec());
        assert_eq!(outcome.0[2].key, "c");
        assert!(outcome.0[2].value.is_empty());
    }

    #[test]
    fn parses_escaped_value() {
        let mut parser = TagsParser::new();
        let outcome = parser.advance(b"@a=hi\\:there;s=\\sbye :x Y z", limits());
        let tags = match outcome {
            TagsOutcome::Ready { tags, .. } => tags,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(tags[0].value, b"hi;there".to_vec());
        assert_eq!(tags[1].value, b" bye".to_vec());
    }

    #[test]
    fn no_envelope_returns_empty_ready() {
        let mut parser = TagsParser::new();
        let outcome = parser.advance(b"PRIVMSG #c :hi", limits());
        assert!(matches!(
            outcome,
            TagsOutcome::Ready {
                tags,
                consumed,
                tag_data_bytes,
            } if tags.is_empty() && consumed == 0 && tag_data_bytes == 0
        ));
    }

    #[test]
    fn incomplete_envelope() {
        let mut parser = TagsParser::new();
        let outcome = parser.advance(b"@a=1;b", limits());
        assert!(matches!(outcome, TagsOutcome::Incomplete));
    }

    #[test]
    fn empty_envelope_at_sign_only() {
        let mut parser = TagsParser::new();
        let outcome = parser.advance(b"@ :prefix PRIVMSG #c :hi", limits());
        let outcome = match outcome {
            TagsOutcome::Ready { tags, consumed, .. } => (tags, consumed),
            other => panic!("unexpected: {other:?}"),
        };
        assert!(outcome.0.is_empty());
        assert_eq!(outcome.1, 2);
    }

    #[test]
    fn oversized_envelope_rejected() {
        let mut parser = TagsParser::new();
        // 8192 bytes envelope with no terminating space is
        // incomplete at first, then rejected when larger.
        let huge = vec![b'a'; IrcLimits::defaults().tag_envelope_max_bytes + 8];
        let mut bytes = vec![b'@'];
        bytes.extend_from_slice(&huge);
        let outcome = parser.advance(&bytes, limits());
        assert!(matches!(outcome, TagsOutcome::Invalid(_)));
    }

    #[test]
    fn too_many_tags_rejected() {
        let mut parser = TagsParser::new();
        let count = IrcLimits::defaults().tag_count_max + 1;
        let mut envelope = vec![b'@'];
        for index in 0..count {
            if index > 0 {
                envelope.push(b';');
            }
            envelope.push(b'a');
            envelope.push(b'=');
            envelope.push(b'1');
        }
        envelope.push(b' ');
        envelope.extend_from_slice(b"CMD args");
        let outcome = parser.advance(&envelope, limits());
        assert!(matches!(outcome, TagsOutcome::Invalid(_)));
    }

    #[test]
    fn tag_value_data_ceiling_rejected() {
        let mut parser = TagsParser::new();
        let mut envelope = vec![b'@'];
        envelope.push(b'a');
        envelope.push(b'=');
        envelope.extend_from_slice(&vec![
            b'x';
            IrcLimits::defaults().client_tag_data_max_bytes + 1
        ]);
        envelope.push(b' ');
        envelope.extend_from_slice(b"CMD args");
        let outcome = parser.advance(&envelope, limits());
        assert!(matches!(outcome, TagsOutcome::Invalid(_)));
    }

    #[test]
    fn invalid_escape_rejected() {
        let mut parser = TagsParser::new();
        let outcome = parser.advance(b"@a=hi\\q :prefix CMD", limits());
        assert!(matches!(outcome, TagsOutcome::Invalid(_)));
    }

    #[test]
    fn empty_key_rejected() {
        let mut parser = TagsParser::new();
        let outcome = parser.advance(b"@=1 :prefix CMD", limits());
        assert!(matches!(outcome, TagsOutcome::Invalid(_)));
    }
}
