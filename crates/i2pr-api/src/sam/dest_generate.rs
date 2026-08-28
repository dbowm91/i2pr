//! SAM `DEST GENERATE` runtime-neutral core operation.
//!
//! Plan 136 implements the typed request/response model. The
//! operation takes a CSPRNG, generates one Ed25519+X25519 destination
//! identity, and returns the SAM-compatible public/private
//! representations. No generated key is inserted into the router
//! destination registry; session ownership begins only in Plan 137.

use i2pr_client::DestinationIdentity;
use rand_core::TryCryptoRng;

use crate::sam::{
    base64,
    private_destination::{SamPrivateDestination, SamPrivateDestinationError},
};

/// Canonical numeric identifier for Ed25519 signing in SAM.
pub const DEST_GENERATE_SIGNATURE_TYPE_ED25519: u16 = 7;

/// A typed `SIGNATURE_TYPE` value accepted by `DEST GENERATE`.
///
/// Plan 136 implements the Ed25519 profile only. Any other value is
/// returned as `UnsupportedSignatureType`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestGenerateSignatureType {
    /// `SIGNATURE_TYPE=7` (EdDSA-Ed25519).
    Ed25519,
}

impl DestGenerateSignatureType {
    /// Returns the numeric identifier.
    pub const fn code(self) -> u16 {
        match self {
            Self::Ed25519 => DEST_GENERATE_SIGNATURE_TYPE_ED25519,
        }
    }

    /// Returns the canonical SAM string spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ed25519 => "7",
        }
    }

    /// Parses a SAM `SIGNATURE_TYPE=` value, accepting the numeric
    /// form and the case-insensitive canonical name.
    pub fn parse(input: &str) -> Option<Self> {
        if input == Self::Ed25519.as_str()
            || input.eq_ignore_ascii_case("EdDSA_SHA512_Ed25519")
            || input.eq_ignore_ascii_case("Ed25519")
            || input.eq_ignore_ascii_case("EDDSA")
        {
            Some(Self::Ed25519)
        } else {
            None
        }
    }
}

/// A typed `DEST GENERATE` request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestGenerateRequest {
    /// The requested `SIGNATURE_TYPE=`. `None` means the request
    /// omitted the option. Plan 136 returns
    /// `UnsupportedSignatureType` for the omitted-signature path
    /// because legacy DSA is not implemented.
    signature_type: Option<DestGenerateSignatureType>,
}

impl DestGenerateRequest {
    /// Constructs a request from the parsed `SIGNATURE_TYPE=` option.
    pub fn new(signature_type: Option<DestGenerateSignatureType>) -> Self {
        Self { signature_type }
    }
}

/// The outcome of a `DEST GENERATE` operation.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Eq, PartialEq)]
pub enum DestGenerateOutcome {
    /// `DEST REPLY RESULT=OK PUB=... PRIV=...`.
    Ok(DestReply),
    /// `DEST REPLY RESULT=I2P_ERROR` — the CSPRNG could not produce
    /// fresh material.
    RandomnessUnavailable,
    /// `DEST REPLY RESULT=NOT_IMPLEMENTED` — the requested signature
    /// type is outside the M7 3.1 baseline.
    UnsupportedSignatureType(String),
}

/// The destination-generation reply payload.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub struct DestReply {
    /// The SAM public destination text.
    pub public_destination: String,
    /// The SAM private destination text.
    pub private_destination: String,
    /// The [`SamPrivateDestination`] wrapper retained for follow-on
    /// session creation.
    pub wrapper: SamPrivateDestination,
}

impl PartialEq for DestReply {
    fn eq(&self, other: &Self) -> bool {
        // The wrapper owns secrets; only the public portion of the
        // reply participates in equality assertions.
        self.public_destination == other.public_destination
            && self.private_destination == other.private_destination
            && self.wrapper == other.wrapper
    }
}

impl Eq for DestReply {}

/// Errors emitted by the `DEST GENERATE` core operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DestGenerateError {
    /// The CSPRNG could not produce fresh material.
    #[error("randomness unavailable for destination generation")]
    RandomnessUnavailable,
    /// The wrapper construction failed.
    #[error("private destination construction failed: {0}")]
    PrivateDestination(#[from] SamPrivateDestinationError),
}

/// Runtime-neutral `DEST GENERATE` core operation.
pub fn dest_generate<R: TryCryptoRng + ?Sized>(
    rng: &mut R,
    request: DestGenerateRequest,
) -> Result<DestGenerateOutcome, DestGenerateError> {
    match request.signature_type {
        None => Ok(DestGenerateOutcome::UnsupportedSignatureType(
            "SIGNATURE_TYPE required".to_owned(),
        )),
        Some(DestGenerateSignatureType::Ed25519) => {
            let identity = DestinationIdentity::generate(rng)
                .map_err(|_| DestGenerateError::RandomnessUnavailable)?;
            let wrapper = SamPrivateDestination::from_identity(&identity)?;
            let public_destination = base64::encode(wrapper.public_bytes());
            let private_destination = base64::encode(wrapper.private_bytes());
            Ok(DestGenerateOutcome::Ok(DestReply {
                public_destination,
                private_destination,
                wrapper,
            }))
        }
    }
}

/// The `DEST GENERATE` core facade type, mirroring the runtime-neutral
/// command surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DestGenerate;

impl DestGenerate {
    /// Returns the canonical command name.
    pub const fn command_name() -> &'static str {
        "DEST GENERATE"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    #[test]
    fn dest_generate_ed25519_produces_priv_round_trip() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let request = DestGenerateRequest::new(Some(DestGenerateSignatureType::Ed25519));
        let outcome = dest_generate(&mut rng, request).expect("generate");
        let reply = match outcome {
            DestGenerateOutcome::Ok(reply) => reply,
            other => panic!("unexpected outcome {other:?}"),
        };
        assert_eq!(reply.public_destination.len(), 524);
        assert_eq!(reply.private_destination.len(), 608);
        let restored = SamPrivateDestination::from_base64(&reply.private_destination)
            .expect("restore")
            .into_identity()
            .expect("identity");
        let original_id = reply
            .wrapper
            .destination_id()
            .expect("original destination id");
        assert_eq!(restored.id(), original_id);
    }

    #[test]
    fn dest_generate_absent_signature_type_is_rejected() {
        let mut rng = ChaCha8Rng::seed_from_u64(43);
        let request = DestGenerateRequest::new(None);
        let outcome = dest_generate(&mut rng, request).expect("generate");
        assert!(matches!(
            outcome,
            DestGenerateOutcome::UnsupportedSignatureType(_)
        ));
    }

    #[test]
    fn dest_generate_signature_type_parser_accepts_known_forms() {
        assert_eq!(
            DestGenerateSignatureType::parse("7"),
            Some(DestGenerateSignatureType::Ed25519)
        );
        assert_eq!(
            DestGenerateSignatureType::parse("EDDSA_SHA512_ED25519"),
            Some(DestGenerateSignatureType::Ed25519)
        );
        assert_eq!(
            DestGenerateSignatureType::parse("Ed25519"),
            Some(DestGenerateSignatureType::Ed25519)
        );
        assert_eq!(DestGenerateSignatureType::parse("0"), None);
    }
}
