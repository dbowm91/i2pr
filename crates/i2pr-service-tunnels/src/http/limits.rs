//! Plan 176 HTTP/1.1 hard ceilings.
//!
//! All parser/storage ceilings live in one place so every other
//! module can import them by name. Conservative defaults that
//! accommodate ordinary browsers and `curl` are selected.
//!
//! ## Ceilings
//!
//! | Region | Default | Plan |
//! | --- | ---: | --- |
//! | Request line bytes (including CRLF) | 8192 | §3.1 |
//! | Total header bytes (lines + final CRLF) | 65536 | §3.1 |
//! | Header count | 100 | §3.1 |
//! | Field-name bytes | 256 | §3.1 |
//! | Field-value bytes (single line) | 8192 | §3.1 |
//! | CONNECT authority bytes | 512 | §3.1 |
//! | Retained buffered bytes before complete header | 65536 | §3.1 |
//! | Generated error response bytes | 1024 | §3.1 |
//!
//! Every ceiling is also a hard maximum: callers must reject inputs
//! at or beyond the ceiling. The `default_*` helpers return the
//! Plan 176 values; tests use `custom` to verify exact-boundary
//! rejection.

#![forbid(unsafe_code)]

/// Central hard ceilings for the Plan 176 HTTP parser.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HttpLimits {
    /// Maximum bytes for one request line, including the trailing CRLF.
    pub request_line_max_bytes: usize,
    /// Maximum total header bytes (sum of header lines + final CRLF).
    pub total_header_bytes_max: usize,
    /// Maximum number of header lines.
    pub header_count_max: usize,
    /// Maximum bytes for one field name.
    pub field_name_max_bytes: usize,
    /// Maximum bytes for one field value (single line; obs-fold is rejected).
    pub field_value_max_bytes: usize,
    /// Maximum bytes for one CONNECT authority.
    pub connect_authority_max_bytes: usize,
    /// Maximum retained bytes before a complete header section.
    pub retained_buffer_max_bytes: usize,
    /// Maximum bytes for one generated error response.
    pub error_response_max_bytes: usize,
}

impl HttpLimits {
    /// Plan 176 conservative defaults compatible with ordinary
    /// browsers and `curl`.
    pub fn defaults() -> Self {
        Self {
            request_line_max_bytes: 8192,
            total_header_bytes_max: 65_536,
            header_count_max: 100,
            field_name_max_bytes: 256,
            field_value_max_bytes: 8192,
            connect_authority_max_bytes: 512,
            retained_buffer_max_bytes: 65_536,
            error_response_max_bytes: 1024,
        }
    }

    /// Constructs a limits value with all fields set to the supplied
    /// uniform value. Used by boundary tests.
    pub fn uniform(value: usize) -> Self {
        Self {
            request_line_max_bytes: value,
            total_header_bytes_max: value,
            header_count_max: value,
            field_name_max_bytes: value,
            field_value_max_bytes: value,
            connect_authority_max_bytes: value,
            retained_buffer_max_bytes: value,
            error_response_max_bytes: value,
        }
    }

    /// Validates that every ceiling fits within its hard typed
    /// maximum. The hard maxima exist so the daemon cannot accept
    /// configuration that lets the parser keep gigabytes in flight.
    pub fn validate(self) -> Result<Self, super::error::HttpError> {
        if self.request_line_max_bytes == 0
            || self.request_line_max_bytes > super::config::HTTP_REQUEST_LINE_MAX_BYTES
        {
            return Err(super::error::HttpError::new(
                super::error::HttpErrorKind::InvalidLimits,
                "request_line_max_bytes out of range",
            ));
        }
        if self.total_header_bytes_max == 0
            || self.total_header_bytes_max > super::config::HTTP_TOTAL_HEADER_MAX_BYTES
        {
            return Err(super::error::HttpError::new(
                super::error::HttpErrorKind::InvalidLimits,
                "total_header_bytes_max out of range",
            ));
        }
        if self.header_count_max == 0
            || self.header_count_max > super::config::HTTP_HEADER_COUNT_MAX
        {
            return Err(super::error::HttpError::new(
                super::error::HttpErrorKind::InvalidLimits,
                "header_count_max out of range",
            ));
        }
        if self.field_name_max_bytes == 0
            || self.field_name_max_bytes > super::config::HTTP_FIELD_NAME_MAX_BYTES
        {
            return Err(super::error::HttpError::new(
                super::error::HttpErrorKind::InvalidLimits,
                "field_name_max_bytes out of range",
            ));
        }
        if self.field_value_max_bytes == 0
            || self.field_value_max_bytes > super::config::HTTP_FIELD_VALUE_MAX_BYTES
        {
            return Err(super::error::HttpError::new(
                super::error::HttpErrorKind::InvalidLimits,
                "field_value_max_bytes out of range",
            ));
        }
        if self.connect_authority_max_bytes == 0
            || self.connect_authority_max_bytes > super::config::HTTP_CONNECT_AUTHORITY_MAX_BYTES
        {
            return Err(super::error::HttpError::new(
                super::error::HttpErrorKind::InvalidLimits,
                "connect_authority_max_bytes out of range",
            ));
        }
        if self.retained_buffer_max_bytes == 0
            || self.retained_buffer_max_bytes > super::config::HTTP_RETAINED_BUFFER_MAX_BYTES
        {
            return Err(super::error::HttpError::new(
                super::error::HttpErrorKind::InvalidLimits,
                "retained_buffer_max_bytes out of range",
            ));
        }
        if self.error_response_max_bytes == 0
            || self.error_response_max_bytes > super::config::HTTP_ERROR_RESPONSE_MAX_BYTES
        {
            return Err(super::error::HttpError::new(
                super::error::HttpErrorKind::InvalidLimits,
                "error_response_max_bytes out of range",
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
        HttpLimits::defaults()
            .validate()
            .expect("defaults validate");
    }

    #[test]
    fn zero_ceiling_rejected() {
        for limits in [
            HttpLimits {
                request_line_max_bytes: 0,
                ..HttpLimits::defaults()
            },
            HttpLimits {
                total_header_bytes_max: 0,
                ..HttpLimits::defaults()
            },
            HttpLimits {
                header_count_max: 0,
                ..HttpLimits::defaults()
            },
            HttpLimits {
                field_name_max_bytes: 0,
                ..HttpLimits::defaults()
            },
            HttpLimits {
                field_value_max_bytes: 0,
                ..HttpLimits::defaults()
            },
            HttpLimits {
                connect_authority_max_bytes: 0,
                ..HttpLimits::defaults()
            },
            HttpLimits {
                retained_buffer_max_bytes: 0,
                ..HttpLimits::defaults()
            },
            HttpLimits {
                error_response_max_bytes: 0,
                ..HttpLimits::defaults()
            },
        ] {
            assert!(limits.validate().is_err(), "zero must reject: {limits:?}");
        }
    }
}
