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
    /// `STYLE=PRIMARY` shared-Destination owner. `MASTER` is accepted as a
    /// compatibility spelling for deployed i2pd and older Java I2P.
    Primary,
    /// Child style accepted only by `SESSION ADD`.
    Datagram,
    /// Proposal 163 authenticated Datagram2 child style.
    Datagram2,
    /// Proposal 163 unauthenticated Datagram3 child style.
    Datagram3,
    /// Child style accepted only by `SESSION ADD`.
    Raw,
}

impl SessionCreateStyle {
    /// Parses the SAM spelling.
    pub fn parse(input: &str) -> Option<Self> {
        if input.eq_ignore_ascii_case("STREAM") {
            Some(Self::Stream)
        } else if input.eq_ignore_ascii_case("PRIMARY") || input.eq_ignore_ascii_case("MASTER") {
            Some(Self::Primary)
        } else if input.eq_ignore_ascii_case("DATAGRAM") {
            Some(Self::Datagram)
        } else if input.eq_ignore_ascii_case("DATAGRAM2") {
            Some(Self::Datagram2)
        } else if input.eq_ignore_ascii_case("DATAGRAM3") {
            Some(Self::Datagram3)
        } else if input.eq_ignore_ascii_case("RAW") {
            Some(Self::Raw)
        } else {
            None
        }
    }

    /// Returns the canonical wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stream => "STREAM",
            Self::Primary => "PRIMARY",
            Self::Datagram => "DATAGRAM",
            Self::Datagram2 => "DATAGRAM2",
            Self::Datagram3 => "DATAGRAM3",
            Self::Raw => "RAW",
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
    /// The SAM 3.2 FROM_PORT option was invalid.
    #[error("invalid SESSION CREATE FROM_PORT: {0}")]
    InvalidFromPort(String),
    /// The SAM 3.2 TO_PORT option was invalid.
    #[error("invalid SESSION CREATE TO_PORT: {0}")]
    InvalidToPort(String),
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
    /// Default local I2P source port.
    pub from_port: u16,
    /// Default remote I2P destination/listen port.
    pub to_port: u16,
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
    parse_session_create_with_ports(id, style, destination, None, None)
}

/// Parses SESSION CREATE with optional SAM 3.2 I2P port defaults.
pub fn parse_session_create_with_ports(
    id: &str,
    style: &str,
    destination: &str,
    from_port: Option<&str>,
    to_port: Option<&str>,
) -> Result<SessionCreateRequest, SessionCreateError> {
    if id.is_empty() {
        return Err(SessionCreateError::MissingId);
    }
    let parsed_style = SessionCreateStyle::parse(style)
        .ok_or_else(|| SessionCreateError::UnsupportedStyle(style.to_owned()))?;
    if !matches!(
        parsed_style,
        SessionCreateStyle::Stream | SessionCreateStyle::Primary
    ) {
        return Err(SessionCreateError::UnsupportedStyle(style.to_owned()));
    }
    if destination.is_empty() {
        return Err(SessionCreateError::MissingDestination);
    }
    let destination_source = if destination.eq_ignore_ascii_case("TRANSIENT") {
        DestinationSource::Transient
    } else {
        let wrapper = SamPrivateDestination::from_base64(destination)?;
        DestinationSource::Imported(wrapper)
    };
    let from_port = from_port
        .map(|value| {
            super::command::parse_sam_port(value)
                .ok_or_else(|| SessionCreateError::InvalidFromPort(value.to_owned()))
        })
        .transpose()?
        .unwrap_or(0);
    let to_port = to_port
        .map(|value| {
            super::command::parse_sam_port(value)
                .ok_or_else(|| SessionCreateError::InvalidToPort(value.to_owned()))
        })
        .transpose()?
        .unwrap_or(0);
    if parsed_style == SessionCreateStyle::Primary && (from_port != 0 || to_port != 0) {
        return Err(SessionCreateError::UnsupportedStyle(
            "PRIMARY does not accept FROM_PORT or TO_PORT".to_owned(),
        ));
    }
    Ok(SessionCreateRequest {
        id: id.to_owned(),
        style: parsed_style,
        destination: destination_source,
        from_port,
        to_port,
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
        assert_eq!((request.from_port, request.to_port), (0, 0));
        assert!(matches!(request.destination, DestinationSource::Transient));
    }

    #[test]
    fn session_create_parses_port_defaults_and_rejects_overflow() {
        let request = parse_session_create_with_ports(
            "alpha",
            "STREAM",
            "TRANSIENT",
            Some("65535"),
            Some("110"),
        )
        .expect("valid ports");
        assert_eq!((request.from_port, request.to_port), (u16::MAX, 110));
        assert!(matches!(
            parse_session_create_with_ports("alpha", "STREAM", "TRANSIENT", None, Some("65536")),
            Err(SessionCreateError::InvalidToPort(_))
        ));
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
