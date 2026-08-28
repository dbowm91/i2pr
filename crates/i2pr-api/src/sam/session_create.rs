//! SAM `SESSION CREATE` private-destination import foundation.
//!
//! Plan 136 implements only the pure conversion/validation layer. The
//! module accepts a typed [`SessionCreateRequest`] and validates the
//! `DESTINATION=` option (either `TRANSIENT` or a SAM-compatible
//! `PRIV` value), but does **not** create a session or register the
//! destination. Plan 137 owns the lifecycle.

use core::fmt;

use crate::sam::{
    base64,
    private_destination::{SamPrivateDestination, SamPrivateDestinationError},
};

/// Supported session styles for `SESSION CREATE`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionCreateStyle {
    /// `STYLE=STREAM` (the M7 baseline).
    Stream,
}

impl SessionCreateStyle {
    /// Parses the SAM spelling.
    pub fn parse(input: &str) -> Option<Self> {
        if input.eq_ignore_ascii_case("STREAM") {
            Some(Self::Stream)
        } else {
            None
        }
    }

    /// Returns the canonical wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stream => "STREAM",
        }
    }
}

/// Errors emitted by the `SESSION CREATE` typed request parser.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SessionCreateError {
    /// The required `ID=` option was missing.
    #[error("SESSION CREATE requires ID=")]
    MissingId,
    /// The required `STYLE=` option was missing.
    #[error("SESSION CREATE requires STYLE=")]
    MissingStyle,
    /// The required `DESTINATION=` option was missing.
    #[error("SESSION CREATE requires DESTINATION=")]
    MissingDestination,
    /// The `STYLE=` value was not `STREAM`.
    #[error("unsupported SESSION CREATE STYLE: {0}")]
    UnsupportedStyle(String),
    /// The `DESTINATION=` value could not be parsed.
    #[error("invalid SESSION CREATE DESTINATION: {0}")]
    InvalidDestination(String),
    /// The private-destination codec rejected the supplied `PRIV`.
    #[error("private destination invalid: {0}")]
    PrivateDestination(#[from] SamPrivateDestinationError),
    /// The `PRIV` value could not be Base64 decoded.
    #[error("private destination base64 invalid: {0}")]
    Base64(#[from] base64::SamBase64Error),
}

/// Typed `SESSION CREATE` request.
#[derive(Debug, Eq, PartialEq)]
pub struct SessionCreateRequest {
    /// The session identifier (`ID=`).
    pub id: String,
    /// The session style.
    pub style: SessionCreateStyle,
    /// The `DESTINATION=` value: `TRANSIENT` or a `PRIV` text.
    pub destination: DestinationSource,
}

/// The source of a SAM session's destination.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum DestinationSource {
    /// `DESTINATION=TRANSIENT` — the server generates the destination.
    Transient,
    /// `DESTINATION=<PRIV>` — the supplied private destination.
    Imported(SamPrivateDestination),
}

impl PartialEq for DestinationSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Transient, Self::Transient) => true,
            (Self::Imported(left), Self::Imported(right)) => left == right,
            _ => false,
        }
    }
}

impl Eq for DestinationSource {}

impl fmt::Display for DestinationSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transient => formatter.write_str("TRANSIENT"),
            Self::Imported(_) => formatter.write_str("<PRIV>"),
        }
    }
}

/// Parses a typed `SESSION CREATE` request from already-validated
/// option values. The helper expects the caller to have already
/// applied duplicate/missing-option checks.
pub fn parse_session_create(
    id: &str,
    style: &str,
    destination: &str,
) -> Result<SessionCreateRequest, SessionCreateError> {
    if id.is_empty() {
        return Err(SessionCreateError::MissingId);
    }
    let parsed_style = SessionCreateStyle::parse(style)
        .ok_or_else(|| SessionCreateError::UnsupportedStyle(style.to_owned()))?;
    if destination.is_empty() {
        return Err(SessionCreateError::MissingDestination);
    }
    let destination_source = if destination.eq_ignore_ascii_case("TRANSIENT") {
        DestinationSource::Transient
    } else {
        let wrapper = SamPrivateDestination::from_base64(destination)?;
        DestinationSource::Imported(wrapper)
    };
    Ok(SessionCreateRequest {
        id: id.to_owned(),
        style: parsed_style,
        destination: destination_source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_client::DestinationIdentity;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn identity_priv_text() -> String {
        let mut rng = ChaCha8Rng::seed_from_u64(100);
        let identity = DestinationIdentity::generate(&mut rng).expect("identity");
        SamPrivateDestination::from_identity(&identity)
            .expect("wrapper")
            .encode_base64()
    }

    #[test]
    fn transient_session_request_is_constructed() {
        let request = parse_session_create("alpha", "STREAM", "TRANSIENT").expect("parse");
        assert_eq!(request.id, "alpha");
        assert_eq!(request.style, SessionCreateStyle::Stream);
        assert!(matches!(request.destination, DestinationSource::Transient));
    }

    #[test]
    fn imported_session_request_reconstructs_identity() {
        let priv_text = identity_priv_text();
        let mut request = parse_session_create("alpha", "STREAM", &priv_text).expect("parse");
        let wrapper = match &request.destination {
            DestinationSource::Imported(_) => {
                let DestinationSource::Imported(wrapper) =
                    std::mem::replace(&mut request.destination, DestinationSource::Transient)
                else {
                    unreachable!()
                };
                wrapper
            }
            DestinationSource::Transient => panic!("expected imported destination"),
        };
        let restored = wrapper.into_identity().expect("identity");
        let original_id = restored.id();
        drop(request);
        assert_eq!(restored.id(), original_id);
    }

    #[test]
    fn unsupported_style_is_rejected() {
        let error = parse_session_create("alpha", "DATAGRAM", "TRANSIENT").unwrap_err();
        assert!(matches!(error, SessionCreateError::UnsupportedStyle(_)));
    }

    #[test]
    fn invalid_priv_is_rejected() {
        let error = parse_session_create("alpha", "STREAM", "this-is-not-base64!").unwrap_err();
        assert!(matches!(
            error,
            SessionCreateError::PrivateDestination(_) | SessionCreateError::Base64(_)
        ));
    }

    #[test]
    fn missing_destination_is_rejected() {
        let error = parse_session_create("alpha", "STREAM", "").unwrap_err();
        assert!(matches!(error, SessionCreateError::MissingDestination));
    }

    #[test]
    fn style_normalisation_accepts_lowercase() {
        let request = parse_session_create("alpha", "stream", "TRANSIENT").expect("parse");
        assert_eq!(request.style, SessionCreateStyle::Stream);
    }
}
