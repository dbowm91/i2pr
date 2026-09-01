//! Bounded, runtime-neutral incremental SAM line reader.
//!
//! Plan 137 §6 requires:
//!
//! 1. tolerate TCP segmentation: one command may arrive across many reads;
//! 2. tolerate multiple complete command lines in one read;
//! 3. enforce `MAX_SAM_LINE_BYTES` while data is accumulating,
//!    before newline arrives;
//! 4. terminate the client on line overflow rather than buffering
//!    until newline;
//! 5. split on `\n` and accept trailing `\r`;
//! 6. avoid `read_line()` with attacker-controlled unbounded `String`;
//! 7. partial UTF-8 is handled by buffering bytes; the strict
//!    byte-level Plan 136 control-byte policy is enforced by the
//!    downstream parser;
//! 8. the caller decides the per-line overflow / partial-UTF-8
//!    disposition; this module only surfaces typed events.

use core::fmt;

use super::MAX_SAM_LINE_BYTES;

/// Outcome of feeding bytes to a [`LineReader`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LineEvent {
    /// One complete command line was decoded. The trailing `\n`
    /// (and any preceding `\r`) was consumed.
    CompleteLine {
        /// The decoded line bytes (UTF-8 must be validated by the
        /// caller; the reader itself never panics on non-UTF-8).
        line: Vec<u8>,
    },
    /// One complete command line was decoded, but the line exceeded
    /// the configured byte ceiling. The caller must terminate the
    /// client rather than forwarding the bytes to the parser.
    OverflowLine {
        /// Observed byte length (after consuming the trailing `\n`).
        observed: usize,
        /// Accepted ceiling.
        ceiling: usize,
    },
    /// A control byte or NUL was observed before the line completed;
    /// the caller must terminate the client. Plan 136 forbids these
    /// bytes in command lines.
    ControlByteInLine {
        /// Rejected byte value.
        byte: u8,
        /// 0-based byte index inside the buffered line.
        index: usize,
    },
    /// The reader is still accumulating bytes; no complete line was
    /// observed in this `push` call.
    NeedMore,
}

/// Bounded, runtime-neutral incremental SAM line reader.
#[derive(Debug)]
pub struct LineReader {
    /// Buffered bytes (may exceed `MAX_SAM_LINE_BYTES` only if the
    /// caller opts into observing [`LineEvent::ControlByteInLine`]
    /// first; once the byte ceiling is exceeded the reader reports
    /// [`LineEvent::OverflowLine`] and discards the buffer).
    buf: Vec<u8>,
    /// Per-line byte ceiling. Defaults to `MAX_SAM_LINE_BYTES`.
    line_ceiling: usize,
    /// Per-line control-byte policy. When true (the default), the
    /// reader scans the buffer on every push and emits
    /// [`LineEvent::ControlByteInLine`] before overflow detection.
    reject_control_bytes: bool,
}

impl LineReader {
    /// Constructs a new bounded line reader using the documented
    /// Plan 137 ceiling.
    pub fn new() -> Self {
        Self::with_ceiling(MAX_SAM_LINE_BYTES)
    }

    /// Constructs a new bounded line reader using a custom per-line
    /// ceiling (clamped to `[1, MAX_SAM_LINE_BYTES]`).
    pub fn with_ceiling(line_ceiling: usize) -> Self {
        let clamped = line_ceiling.clamp(1, MAX_SAM_LINE_BYTES);
        Self {
            buf: Vec::new(),
            line_ceiling: clamped,
            reject_control_bytes: true,
        }
    }

    /// Returns the per-line byte ceiling.
    pub const fn line_ceiling(&self) -> usize {
        self.line_ceiling
    }

    /// Returns the number of buffered bytes that have not yet been
    /// returned as a complete line.
    pub fn buffered_len(&self) -> usize {
        self.buf.len()
    }

    /// Clears the internal buffer (used on overflow or when the
    /// caller wants to drop a partially accumulated line).
    pub fn reset(&mut self) {
        self.buf.clear();
    }

    /// Consumes any bytes that arrived after the last complete
    /// newline and returns them verbatim. Used by the SAM daemon at
    /// the `STREAM CONNECT` / `STREAM ACCEPT` -> raw-mode
    /// transition so post-command application bytes that the
    /// reader had already buffered are preserved as the first raw
    /// bytes flowing into the new raw driver.
    ///
    /// Plan 147 §4: the leftover bytes are bounded by the
    /// per-line ceiling at command time, the buffer is empty
    /// after the call, and the call never re-validates or
    /// re-parses the bytes — the raw driver owns the post-command
    /// byte stream from this point onward.
    pub fn take_buffered(&mut self) -> Vec<u8> {
        let drained: Vec<u8> = self.buf.drain(..).collect();
        drained
    }

    /// Feeds the supplied bytes to the reader.
    ///
    /// The reader scans the supplied bytes plus any previously
    /// buffered bytes to find the next `\n`. When a complete line is
    /// observed the bytes are returned as a [`LineEvent::CompleteLine`]
    /// (or [`LineEvent::OverflowLine`] if the line exceeded the
    /// ceiling) and the buffer is rotated past the trailing `\n`.
    /// Multiple complete lines in a single push are surfaced one at
    /// a time; the caller must call `push` again to retrieve the next
    /// line from the same input slice. If the trailing bytes do not
    /// contain a newline they remain buffered for the next call.
    pub fn push(&mut self, bytes: &[u8]) -> LineEvent {
        // Append every byte to the buffer (after the byte-level
        // policy checks below) so a subsequent call sees any
        // post-newline bytes that arrived with this call.
        let mut consumed = 0;
        for byte in bytes {
            if self.buf.len() >= self.line_ceiling && !self.buf.contains(&b'\n') {
                let observed = self.buf.len();
                self.buf.clear();
                return LineEvent::OverflowLine {
                    observed,
                    ceiling: self.line_ceiling,
                };
            }
            if *byte == b'\n' {
                consumed += 1;
                let line_end = if self.buf.last() == Some(&b'\r') {
                    self.buf.len() - 1
                } else {
                    self.buf.len()
                };
                let line_bytes = self.buf[..line_end].to_vec();
                let overflow = line_bytes.len() > self.line_ceiling;
                if overflow {
                    self.buf.clear();
                    return LineEvent::OverflowLine {
                        observed: line_bytes.len(),
                        ceiling: self.line_ceiling,
                    };
                }
                self.buf.clear();
                // Buffer the remainder of the supplied slice so the
                // next `push` can process it without losing bytes.
                if consumed < bytes.len() {
                    self.append_unchecked(&bytes[consumed..]);
                }
                return LineEvent::CompleteLine { line: line_bytes };
            }
            // Allow `\r` to be buffered so a CRLF terminator is
            // stripped to LF when the trailing `\n` arrives. Other
            // control bytes remain rejected per Plan 136.
            if *byte == b'\r' {
                self.buf.push(*byte);
                consumed += 1;
                continue;
            }
            if self.reject_control_bytes && (*byte < 0x20 || *byte == 0x7f) {
                let index = self.buf.len();
                self.buf.clear();
                return LineEvent::ControlByteInLine { byte: *byte, index };
            }
            self.buf.push(*byte);
            consumed += 1;
        }
        // If the buffer already holds a complete line (from a
        // previous push that left post-newline bytes behind), drain
        // and return it now even though this `push` call added
        // nothing new.
        if let Some(newline_index) = self.buf.iter().position(|byte| *byte == b'\n') {
            let line_end = if newline_index > 0 && self.buf[newline_index - 1] == b'\r' {
                newline_index - 1
            } else {
                newline_index
            };
            let line_bytes = self.buf[..line_end].to_vec();
            let observed = line_bytes.len();
            self.buf.drain(..=newline_index);
            if observed > self.line_ceiling {
                return LineEvent::OverflowLine {
                    observed,
                    ceiling: self.line_ceiling,
                };
            }
            return LineEvent::CompleteLine { line: line_bytes };
        }
        LineEvent::NeedMore
    }

    /// Appends bytes to the buffer without re-validating them. Used
    /// to keep post-newline bytes that arrived in the same `push`
    /// call so the next call can return the next line.
    fn append_unchecked(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.buf.push(*byte);
        }
    }
}

impl Default for LineReader {
    fn default() -> Self {
        Self::new()
    }
}

/// Typed line-reader error returned by [`LineReader::push`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LineReaderError {
    /// The reader overflowed the per-line ceiling.
    Overflow {
        /// Observed byte length.
        observed: usize,
        /// Accepted ceiling.
        ceiling: usize,
    },
    /// The reader observed a control byte or NUL inside an
    /// accumulating line.
    ControlByte {
        /// Rejected byte value.
        byte: u8,
        /// 0-based byte index.
        index: usize,
    },
}

impl fmt::Display for LineReaderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow { observed, ceiling } => {
                write!(formatter, "line length {observed} exceeds {ceiling}")
            }
            Self::ControlByte { byte, index } => {
                write!(formatter, "control byte {byte:#x} at index {index}")
            }
        }
    }
}

impl std::error::Error for LineReaderError {}

impl LineReader {
    /// Convenience: feeds bytes and returns the first complete line
    /// or a typed error. The caller uses this when the upper layer
    /// can map every `LineEvent` variant to a terminal close.
    pub fn push_one(&mut self, bytes: &[u8]) -> Result<Option<Vec<u8>>, LineReaderError> {
        match self.push(bytes) {
            LineEvent::CompleteLine { line } => Ok(Some(line)),
            LineEvent::OverflowLine { observed, ceiling } => {
                Err(LineReaderError::Overflow { observed, ceiling })
            }
            LineEvent::ControlByteInLine { byte, index } => {
                Err(LineReaderError::ControlByte { byte, index })
            }
            LineEvent::NeedMore => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_push_reports_need_more() {
        let mut reader = LineReader::new();
        assert_eq!(reader.push(b""), LineEvent::NeedMore);
    }

    #[test]
    fn complete_line_without_carriage_return() {
        let mut reader = LineReader::new();
        assert_eq!(
            reader.push(b"HELLO VERSION MIN=3.1 MAX=3.1\n"),
            LineEvent::CompleteLine {
                line: b"HELLO VERSION MIN=3.1 MAX=3.1".to_vec(),
            }
        );
        assert_eq!(reader.buffered_len(), 0);
    }

    #[test]
    fn complete_line_with_carriage_return() {
        let mut reader = LineReader::new();
        assert_eq!(
            reader.push(b"HELLO VERSION MIN=3.1 MAX=3.1\r\n"),
            LineEvent::CompleteLine {
                line: b"HELLO VERSION MIN=3.1 MAX=3.1".to_vec(),
            }
        );
    }

    #[test]
    fn split_across_multiple_pushes() {
        let mut reader = LineReader::new();
        assert_eq!(reader.push(b"HELLO VER"), LineEvent::NeedMore);
        assert_eq!(reader.push(b"SION MIN="), LineEvent::NeedMore);
        assert_eq!(
            reader.push(b"3.1 MAX=3.1\n"),
            LineEvent::CompleteLine {
                line: b"HELLO VERSION MIN=3.1 MAX=3.1".to_vec(),
            }
        );
    }

    #[test]
    fn multiple_lines_in_one_push_surface_one_at_a_time() {
        let mut reader = LineReader::new();
        assert_eq!(
            reader.push(b"HELLO VERSION MIN=3.1 MAX=3.1\nDEST GENERATE SIGNATURE_TYPE=7\n"),
            LineEvent::CompleteLine {
                line: b"HELLO VERSION MIN=3.1 MAX=3.1".to_vec(),
            }
        );
        assert_eq!(
            reader.push(b""),
            LineEvent::CompleteLine {
                line: b"DEST GENERATE SIGNATURE_TYPE=7".to_vec(),
            }
        );
        assert_eq!(reader.push(b""), LineEvent::NeedMore);
    }

    #[test]
    fn byte_by_byte_matches_single_push() {
        let mut single = LineReader::new();
        let mut bytewise = LineReader::new();
        let payload = b"PING hello world\n";
        let single_event = single.push(payload);
        let mut bytewise_event = LineEvent::NeedMore;
        for byte in payload {
            bytewise_event = bytewise.push(&[*byte]);
        }
        assert_eq!(single_event, bytewise_event);
    }

    #[test]
    fn overflow_is_reported_before_buffer_grows_unbounded() {
        let mut reader = LineReader::with_ceiling(8);
        let event = reader.push(b"ABCDEFGHIJK\n");
        assert!(matches!(event, LineEvent::OverflowLine { ceiling: 8, .. }));
        assert_eq!(reader.buffered_len(), 0);
    }

    #[test]
    fn embedded_nul_is_rejected() {
        let mut reader = LineReader::new();
        let event = reader.push(b"HELLO\0VERSION\n");
        assert!(matches!(
            event,
            LineEvent::ControlByteInLine { byte: 0, .. }
        ));
    }

    #[test]
    fn control_byte_before_newline_is_rejected() {
        let mut reader = LineReader::new();
        let event = reader.push(b"HELLO\x07VERSION\n");
        assert!(matches!(
            event,
            LineEvent::ControlByteInLine { byte: 0x07, .. }
        ));
    }

    #[test]
    fn push_one_returns_complete_lines() {
        let mut reader = LineReader::new();
        let line = reader
            .push_one(b"HELLO VERSION MIN=3.1 MAX=3.1\n")
            .expect("no error")
            .expect("complete line");
        assert_eq!(line, b"HELLO VERSION MIN=3.1 MAX=3.1".to_vec());
    }

    #[test]
    fn push_one_propagates_overflow() {
        let mut reader = LineReader::with_ceiling(4);
        let error = reader.push_one(b"ABCDE\n").unwrap_err();
        assert!(matches!(error, LineReaderError::Overflow { .. }));
    }

    #[test]
    fn take_buffered_returns_post_newline_bytes_verbatim() {
        let mut reader = LineReader::new();
        let _ = reader.push(b"STREAM CONNECT ID=A DESTINATION=B\nraw-bytes");
        assert!(reader.buffered_len() > 0);
        let leftover = reader.take_buffered();
        assert_eq!(leftover, b"raw-bytes");
        assert_eq!(reader.buffered_len(), 0);
    }

    #[test]
    fn take_buffered_is_empty_when_no_post_newline_bytes() {
        let mut reader = LineReader::new();
        let _ = reader.push(b"STREAM CONNECT ID=A DESTINATION=B\n");
        let leftover = reader.take_buffered();
        assert!(leftover.is_empty());
        assert_eq!(reader.buffered_len(), 0);
    }

    #[test]
    fn take_buffered_preserves_binary_bytes() {
        let mut reader = LineReader::new();
        let binary = [0_u8, 0x7F, 0xFF, 0x80, b'\n', b'\r', 0x00];
        let mut combined = b"STREAM CONNECT ID=A DESTINATION=B\n".to_vec();
        combined.extend_from_slice(&binary);
        let _ = reader.push(&combined);
        let leftover = reader.take_buffered();
        assert_eq!(leftover, binary.to_vec());
    }
}
