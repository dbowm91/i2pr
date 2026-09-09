//! Plan 177 SOCKS5 hard ceilings.
//!
//! All parser/storage ceilings live in one place so every other
//! module can import them by name. SOCKS5 is a small protocol with
//! narrow wire fields; ceilings are tight but conservative against
//! ordinary `curl --socks5-hostname` and browser clients.
//!
//! ## Ceilings
//!
//! | Region | Default | Plan |
//! | --- | ---: | --- |
//! | Method count | 16 | §8 |
//! | Greeting bytes (VER/NMETHODS/METHODS) | 32 | §8 |
//! | Request header bytes (VER/CMD/RSV/ATYP) | 32 | §8 |
//! | Domain length (RFC field u8; local policy 255) | 255 | §8 |
//! | Port bytes | 2 | §8 |
//! | Retained greeting/request bytes | 320 | §8 |
//! | Generated reply bytes | 64 | §8 |
//!
//! Every ceiling is also a hard maximum: callers must reject inputs
//! at or beyond the ceiling. The `default_*` helpers return the
//! Plan 177 values; tests use `custom` to verify exact-boundary
//! rejection.

#![forbid(unsafe_code)]

/// Central hard ceilings for the Plan 177 SOCKS5 parser.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Socks5Limits {
    /// Maximum number of methods offered by a client in the
    /// greeting. Plan 177 §8: hard capped at
    /// [`super::config::SOCKS5_METHOD_COUNT_MAX`].
    pub method_count_max: usize,
    /// Maximum bytes for the greeting section
    /// (`VER + NMETHODS + METHODS`).
    pub greeting_max_bytes: usize,
    /// Maximum bytes for the request header section
    /// (`VER + CMD + RSV + ATYP`).
    pub request_header_max_bytes: usize,
    /// Maximum length of the DOMAINNAME address field (RFC field
    /// is u8; Plan 177 hard caps at 255 bytes to leave a single
    /// well-known reserved value untouched).
    pub domain_max_bytes: usize,
    /// Maximum retained bytes before a complete greeting+request.
    /// Equal to greeting + request header + domain + port.
    pub retained_buffer_max_bytes: usize,
    /// Maximum bytes for one generated reply.
    pub reply_max_bytes: usize,
}

impl Socks5Limits {
    /// Plan 177 conservative defaults compatible with ordinary
    /// `curl --socks5-hostname` and browser SOCKS clients.
    pub fn defaults() -> Self {
        Self {
            method_count_max: 16,
            greeting_max_bytes: 32,
            request_header_max_bytes: 32,
            domain_max_bytes: 255,
            retained_buffer_max_bytes: 320,
            reply_max_bytes: 64,
        }
    }

    /// Constructs a limits value with all fields set to the supplied
    /// uniform value. Used by boundary tests.
    pub fn uniform(value: usize) -> Self {
        Self {
            method_count_max: value,
            greeting_max_bytes: value,
            request_header_max_bytes: value,
            domain_max_bytes: value,
            retained_buffer_max_bytes: value,
            reply_max_bytes: value,
        }
    }

    /// Validates that every ceiling fits within its hard typed
    /// maximum. The hard maxima exist so the daemon cannot accept
    /// configuration that lets the parser keep gigabytes in flight.
    pub fn validate(self) -> Result<Self, super::errors::Socks5Error> {
        use super::config::{
            SOCKS5_DOMAIN_MAX_BYTES, SOCKS5_GREETING_MAX_BYTES, SOCKS5_METHOD_COUNT_MAX,
            SOCKS5_REPLY_MAX_BYTES, SOCKS5_REQUEST_HEADER_MAX_BYTES,
            SOCKS5_RETAINED_BUFFER_MAX_BYTES,
        };
        use super::errors::{Socks5Error, Socks5ErrorKind};
        if self.method_count_max == 0 || self.method_count_max > SOCKS5_METHOD_COUNT_MAX {
            return Err(Socks5Error::new(
                Socks5ErrorKind::InvalidLimits,
                "method_count_max out of range",
            ));
        }
        if self.greeting_max_bytes == 0 || self.greeting_max_bytes > SOCKS5_GREETING_MAX_BYTES {
            return Err(Socks5Error::new(
                Socks5ErrorKind::InvalidLimits,
                "greeting_max_bytes out of range",
            ));
        }
        if self.request_header_max_bytes == 0
            || self.request_header_max_bytes > SOCKS5_REQUEST_HEADER_MAX_BYTES
        {
            return Err(Socks5Error::new(
                Socks5ErrorKind::InvalidLimits,
                "request_header_max_bytes out of range",
            ));
        }
        if self.domain_max_bytes == 0 || self.domain_max_bytes > SOCKS5_DOMAIN_MAX_BYTES {
            return Err(Socks5Error::new(
                Socks5ErrorKind::InvalidLimits,
                "domain_max_bytes out of range",
            ));
        }
        if self.retained_buffer_max_bytes == 0
            || self.retained_buffer_max_bytes > SOCKS5_RETAINED_BUFFER_MAX_BYTES
        {
            return Err(Socks5Error::new(
                Socks5ErrorKind::InvalidLimits,
                "retained_buffer_max_bytes out of range",
            ));
        }
        if self.reply_max_bytes == 0 || self.reply_max_bytes > SOCKS5_REPLY_MAX_BYTES {
            return Err(Socks5Error::new(
                Socks5ErrorKind::InvalidLimits,
                "reply_max_bytes out of range",
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
        Socks5Limits::defaults()
            .validate()
            .expect("defaults validate");
    }

    #[test]
    fn zero_ceiling_rejected() {
        for limits in [
            Socks5Limits {
                method_count_max: 0,
                ..Socks5Limits::defaults()
            },
            Socks5Limits {
                greeting_max_bytes: 0,
                ..Socks5Limits::defaults()
            },
            Socks5Limits {
                request_header_max_bytes: 0,
                ..Socks5Limits::defaults()
            },
            Socks5Limits {
                domain_max_bytes: 0,
                ..Socks5Limits::defaults()
            },
            Socks5Limits {
                retained_buffer_max_bytes: 0,
                ..Socks5Limits::defaults()
            },
            Socks5Limits {
                reply_max_bytes: 0,
                ..Socks5Limits::defaults()
            },
        ] {
            assert!(limits.validate().is_err(), "zero must reject: {limits:?}");
        }
    }
}
