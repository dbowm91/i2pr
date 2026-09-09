//! Plan 178 typed IRC errors.
//!
//! All errors are structural and carry no socket handles, no
//! private application bytes, no hostnames or nicknames. They are
//! emitted as typed failure modes for the daemon to map onto
//! either structural socket closure or a bounded log record.
//!
//! IRC has no native reply-code taxonomy the way SOCKS5 does; the
//! IRC profile signals failure to the client by simply closing
//! the socket after accounting for the drop. The daemon never
//! echoes untrusted bytes back to the client.

#![forbid(unsafe_code)]

use thiserror::Error;

use super::config::{
    IRC_CLIENT_TAG_DATA_MAX_BYTES, IRC_CORE_MAX_BYTES, IRC_GENERATED_LINE_MAX_BYTES,
    IRC_LINE_BUFFER_MAX_BYTES, IRC_TAG_COUNT_MAX, IRC_TAG_ENVELOPE_MAX_BYTES,
    IRC_TAG_KEY_MAX_BYTES,
};

/// One typed IRC failure category. Used by the daemon to decide
/// whether the failure is structural (close immediately) or a
/// recoverable policy drop (continue, increment counter).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum IrcErrorKind {
    /// Core line (post-tag) exceeded the [`IRC_CORE_MAX_BYTES`]
    /// cap. The line is dropped without truncation.
    CoreLineTooLong,
    /// Tag envelope exceeded the [`IRC_TAG_ENVELOPE_MAX_BYTES`]
    /// cap. The line is dropped without truncation.
    TagEnvelopeTooLong,
    /// Tag value exceeded the [`IRC_CLIENT_TAG_DATA_MAX_BYTES`]
    /// cap.
    TagDataTooLong,
    /// Per-direction retained buffer exceeded the
    /// [`IRC_LINE_BUFFER_MAX_BYTES`] cap (slowloris protection).
    BufferCeiling,
    /// Command is not present in the typed command classifier
    /// (`IrcCommandClass::Unknown`).
    UnknownCommand,
    /// Command is in the allowlist but explicitly disallowed for
    /// this direction (e.g. client-issued server-only command).
    DisallowedDirection,
    /// Tag framing is structurally invalid (missing `=`, dangling
    /// escape, etc).
    InvalidTag,
    /// Tag key exceeded the [`IRC_TAG_KEY_MAX_BYTES`] cap.
    TagKeyTooLong,
    /// Too many tags on one message.
    TooManyTags,
    /// USER/HOST field exceeds the configured realname ceiling.
    RealnameTooLong,
    /// Numeric reply has malformed length or non-digit bytes.
    InvalidNumeric,
    /// Limits were misconfigured.
    InvalidLimits,
    /// Limits were validated but a connection-level PING/PONG
    /// rewrite token table overflowed (bounded per connection).
    PingTokenOverflow,
}

impl IrcErrorKind {
    /// Returns a stable machine-readable short identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CoreLineTooLong => "core-line-too-long",
            Self::TagEnvelopeTooLong => "tag-envelope-too-long",
            Self::TagDataTooLong => "tag-data-too-long",
            Self::BufferCeiling => "buffer-ceiling",
            Self::UnknownCommand => "unknown-command",
            Self::DisallowedDirection => "disallowed-direction",
            Self::InvalidTag => "invalid-tag",
            Self::TagKeyTooLong => "tag-key-too-long",
            Self::TooManyTags => "too-many-tags",
            Self::RealnameTooLong => "realname-too-long",
            Self::InvalidNumeric => "invalid-numeric",
            Self::InvalidLimits => "invalid-limits",
            Self::PingTokenOverflow => "ping-token-overflow",
        }
    }
}

/// Typed IRC error carrying a kind plus a machine-readable reason.
#[derive(Clone, Debug, Eq, PartialEq, Error)]
#[error("irc: {kind:?}: {reason}")]
pub struct IrcError {
    /// Failure category.
    pub kind: IrcErrorKind,
    /// Short machine-readable reason.
    pub reason: &'static str,
}

impl IrcError {
    /// Builds one error with a static reason string.
    pub fn new(kind: IrcErrorKind, reason: &'static str) -> Self {
        Self { kind, reason }
    }
}

/// Central hard ceilings for the Plan 178 IRC parser.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IrcLimits {
    /// Maximum bytes for one core (non-tag) message portion
    /// including CRLF. Defaults to [`IRC_CORE_MAX_BYTES`].
    pub core_max_bytes: usize,
    /// Maximum bytes for the IRCv3 message-tag envelope
    /// (leading `@` plus tag keys/values plus the separating
    /// space). Defaults to [`IRC_TAG_ENVELOPE_MAX_BYTES`].
    pub tag_envelope_max_bytes: usize,
    /// Maximum bytes of client-originated tag data. Defaults to
    /// [`IRC_CLIENT_TAG_DATA_MAX_BYTES`].
    pub client_tag_data_max_bytes: usize,
    /// Maximum bytes retained per connection partial-line buffer.
    /// Defaults to [`IRC_LINE_BUFFER_MAX_BYTES`].
    pub line_buffer_max_bytes: usize,
    /// Maximum bytes for one generated line after rewrite. Used
    /// by the daemon executor for its post-filter emit buffer.
    pub generated_line_max_bytes: usize,
    /// Maximum number of tags on one message.
    pub tag_count_max: usize,
    /// Maximum bytes for a single tag key.
    pub tag_key_max_bytes: usize,
}

impl IrcLimits {
    /// Plan 178 conservative defaults compatible with modern
    /// IRC clients and IRCv3-capable servers.
    pub fn defaults() -> Self {
        Self {
            core_max_bytes: IRC_CORE_MAX_BYTES,
            tag_envelope_max_bytes: IRC_TAG_ENVELOPE_MAX_BYTES,
            client_tag_data_max_bytes: IRC_CLIENT_TAG_DATA_MAX_BYTES,
            line_buffer_max_bytes: IRC_LINE_BUFFER_MAX_BYTES,
            generated_line_max_bytes: IRC_GENERATED_LINE_MAX_BYTES,
            tag_count_max: IRC_TAG_COUNT_MAX,
            tag_key_max_bytes: IRC_TAG_KEY_MAX_BYTES,
        }
    }

    /// Validates every ceiling against the hard maxima.
    pub fn validate(self) -> Result<Self, IrcError> {
        if self.core_max_bytes == 0 || self.core_max_bytes > IRC_CORE_MAX_BYTES {
            return Err(IrcError::new(
                IrcErrorKind::InvalidLimits,
                "core_max_bytes out of range",
            ));
        }
        if self.tag_envelope_max_bytes == 0
            || self.tag_envelope_max_bytes > IRC_TAG_ENVELOPE_MAX_BYTES
        {
            return Err(IrcError::new(
                IrcErrorKind::InvalidLimits,
                "tag_envelope_max_bytes out of range",
            ));
        }
        if self.client_tag_data_max_bytes == 0
            || self.client_tag_data_max_bytes > IRC_CLIENT_TAG_DATA_MAX_BYTES
        {
            return Err(IrcError::new(
                IrcErrorKind::InvalidLimits,
                "client_tag_data_max_bytes out of range",
            ));
        }
        if self.line_buffer_max_bytes < self.core_max_bytes
            || self.line_buffer_max_bytes > IRC_LINE_BUFFER_MAX_BYTES
        {
            return Err(IrcError::new(
                IrcErrorKind::InvalidLimits,
                "line_buffer_max_bytes out of range",
            ));
        }
        if self.generated_line_max_bytes == 0
            || self.generated_line_max_bytes > IRC_GENERATED_LINE_MAX_BYTES
        {
            return Err(IrcError::new(
                IrcErrorKind::InvalidLimits,
                "generated_line_max_bytes out of range",
            ));
        }
        if self.tag_count_max == 0 || self.tag_count_max > IRC_TAG_COUNT_MAX {
            return Err(IrcError::new(
                IrcErrorKind::InvalidLimits,
                "tag_count_max out of range",
            ));
        }
        if self.tag_key_max_bytes == 0 || self.tag_key_max_bytes > IRC_TAG_KEY_MAX_BYTES {
            return Err(IrcError::new(
                IrcErrorKind::InvalidLimits,
                "tag_key_max_bytes out of range",
            ));
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_validate() {
        IrcLimits::defaults().validate().expect("defaults validate");
    }

    #[test]
    fn zero_ceiling_rejected() {
        let mut limits = IrcLimits::defaults();
        limits.core_max_bytes = 0;
        assert!(limits.validate().is_err());
        limits = IrcLimits::defaults();
        limits.tag_envelope_max_bytes = 0;
        assert!(limits.validate().is_err());
        limits = IrcLimits::defaults();
        limits.client_tag_data_max_bytes = 0;
        assert!(limits.validate().is_err());
        limits = IrcLimits::defaults();
        limits.line_buffer_max_bytes = 0;
        assert!(limits.validate().is_err());
        limits = IrcLimits::defaults();
        limits.generated_line_max_bytes = 0;
        assert!(limits.validate().is_err());
        limits = IrcLimits::defaults();
        limits.tag_count_max = 0;
        assert!(limits.validate().is_err());
        limits = IrcLimits::defaults();
        limits.tag_key_max_bytes = 0;
        assert!(limits.validate().is_err());
    }

    #[test]
    fn oversized_ceiling_rejected() {
        let mut limits = IrcLimits::defaults();
        limits.core_max_bytes = IRC_CORE_MAX_BYTES + 1;
        assert!(limits.validate().is_err());
        limits = IrcLimits::defaults();
        limits.tag_envelope_max_bytes = IRC_TAG_ENVELOPE_MAX_BYTES + 1;
        assert!(limits.validate().is_err());
    }

    #[test]
    fn line_buffer_below_core_rejected() {
        let mut limits = IrcLimits::defaults();
        limits.line_buffer_max_bytes = limits.core_max_bytes - 1;
        assert!(limits.validate().is_err());
    }

    #[test]
    fn error_kind_str_is_stable() {
        assert_eq!(IrcErrorKind::CoreLineTooLong.as_str(), "core-line-too-long");
        assert_eq!(IrcErrorKind::UnknownCommand.as_str(), "unknown-command");
    }
}
