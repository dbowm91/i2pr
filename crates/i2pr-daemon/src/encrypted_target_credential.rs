//! Plan 380: the ELS2 **consumer** client credential and its sealed stored form.
//!
//! # What was missing
//!
//! Plan 349 built `EncryptedServiceResolver::begin_authorized` and Plan 351 gave it a
//! production caller that only ever takes the **no-credential** branch. The manager holds a
//! `LookupSecret` — the `b33` lookup secret, a different value — and nothing anywhere held the
//! per-client PSK or DH key that a `.b33` with `B32_FLAG_REQUIRES_CLIENT_KEY` actually demands.
//! This module is that missing value, plus the sealed form it is persisted as.
//!
//! # The value is the operator's, not the protocol's
//!
//! For PSK the client holds the same pre-shared key the publisher listed in its authorization
//! block; for DH the client holds the private half of the key pair whose public half the publisher
//! listed. Neither is derivable from the `.b33`, and neither is derivable from the lookup secret.
//! There is no protocol source for it, so the only correct answer is a configuration value —
//! which is why this is a control option and not a derived quantity.
//!
//! For Diffie-Hellman the option carries the **private key only**. The public key is derived from
//! it rather than configured separately: the record's derivation names the client's own public
//! key, and a configuration that could state a public key inconsistent with its private key would
//! be a credential that can never authorize anything, for a reason the operator could not see.
//!
//! # Secrets, not strings
//!
//! [`EncryptedTargetCredential`] holds key material and therefore has **no `Debug`, no `Display`,
//! no `Clone`, and no serializer** — the same discipline
//! [`i2pr_crypto::red25519::LookupSecret`] and `PskClientKey` already follow. It is reachable only
//! through [`EncryptedTargetCredential::as_borrowed`], which reborrows it as the
//! `Els2ClientAuth` the protocol owner takes, and the borrow ends with the resolution.
//!
//! What the manager holds long-lived is [`SealedEncryptedTargetCredential`]: the ciphertext and
//! the owner that can open it, never plaintext. That mirrors Plan 341's
//! `RouterOutproxyProvider`, and it is what makes "the credential is absent from the control store
//! in plaintext" a structural property rather than a discipline anyone has to maintain.
//!
//! Every error in this module carries a `&'static str` and nothing else. A malformed credential is
//! reported by shape, never by value.

#![forbid(unsafe_code)]

use std::sync::Arc;

use i2pr_crypto::X25519PrivateKey;
use i2pr_netdb::{AuthClientPublicKey, Els2AuthScheme, Els2ClientAuth, PskClientKey};
use i2pr_service_tunnels::outbound_secret::{
    OutboundSecret, RouterSecretOwner, validate_els2_credential_form,
};

/// The control option that carries the consumer credential.
///
/// Deliberately **not** part of the frozen Proposal 170 `TUNNEL_OPTIONS` inventory. Proposal 170
/// has no consumer-credential field: on the publisher side `LeaseSetClientAuths` carries the
/// authorized clients, and a client is expected to already hold the matching secret out of band.
/// Adding a key here would advertise a Proposal field that does not exist, so the option is
/// admitted through the daemon's own `SUPPORTED_380_OPTIONS` allow-list instead and appears in no
/// Proposal projection.
///
/// It reaches the daemon through `CustomOptions` as
/// `{"i2pr": {"LeasesetClientCredential": "..."}}`. Proposal 170 defines no field for it, so
/// `i2pr_i2pcontrol::extension_options` is the typed seam that carries it, and the untyped
/// `CustomOptions` form is still refused exactly as it always was.
///
/// Named from that contract rather than restated, so the wire-to-internal mapping and the
/// daemon cannot drift into two names for one option.
pub const ELS2_CREDENTIAL_OPTION: &str =
    i2pr_i2pcontrol::extension_options::LEASE_SET_CLIENT_CREDENTIAL_OPTION;

/// Longest accepted option value, in bytes.
///
/// The longest legal value is `psk:` plus 64 hex characters, so 68. The ceiling is looser than the
/// exact grammar because its job is to stop an unbounded allocation before any parse work, and a
/// backstop that exactly equals the grammar would be one edit away from being wrong.
pub const MAX_ELS2_CREDENTIAL_OPTION_LEN: usize = 128;

/// Hex length of one ELS2 client key. Both schemes use 32 bytes.
const KEY_HEX_LEN: usize = 64;

/// A malformed or unusable consumer credential.
///
/// Every reason is a literal. There is no `Display` on the credential itself to leak through, and
/// no variant carries the offending value, so an operator learns which rule fired and nothing else.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncryptedTargetCredentialError {
    /// The option value exceeded [`MAX_ELS2_CREDENTIAL_OPTION_LEN`].
    OverBound,
    /// The scheme prefix was absent or unrecognised.
    UnknownScheme,
    /// The key was not exactly 64 lowercase hex characters.
    MalformedKey,
    /// No secret owner is installed, so the value cannot be sealed.
    NoSecretOwner,
    /// The stored form did not open: wrong router, tampered, or malformed framing.
    CannotOpen,
}

impl core::fmt::Display for EncryptedTargetCredentialError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let reason = match self {
            Self::OverBound => "exceeds the bounded length",
            Self::UnknownScheme => "must begin with psk: or dh:",
            Self::MalformedKey => "key must be exactly 64 lowercase hex characters",
            Self::NoSecretOwner => "no outbound secret owner is installed",
            Self::CannotOpen => "stored form did not open under this router's secret owner",
        };
        formatter.write_str(reason)
    }
}

impl std::error::Error for EncryptedTargetCredentialError {}

/// The owned client half of an ELS2 service's authorization.
///
/// Exactly one credential, never a list: a consumer of one `.b33` is one client, and a resolved
/// lookup either authorizes or does not. Carrying a list here would turn "which credential
/// authorizes me" into a loop whose answer could differ per attempt, which is precisely the
/// contract question Plan 380 lists as a stop condition.
///
/// `PskClientKey` and `X25519PrivateKey` are both zeroizing, so dropping the credential wipes the
/// bytes. It is not `Clone`: a clone would put a second copy in memory that nothing tracks and
/// nothing wipes.
pub enum EncryptedTargetCredential {
    /// A 32-byte pre-shared key shared with the publisher.
    Psk(PskClientKey),
    /// A client key pair. `public` is always derived from `private` at parse time.
    Dh {
        /// The client's private key.
        private: X25519PrivateKey,
        /// The client's own public key, derived from `private`.
        public: AuthClientPublicKey,
    },
}

impl EncryptedTargetCredential {
    /// Parses one option value.
    ///
    /// `psk:<64 lowercase hex>` or `dh:<64 lowercase hex>`. The hex is decoded with no leniency:
    /// an uppercase digit, a short key, and a long key are all one refusal, because
    /// there is exactly one legal spelling and a lenient decoder would make two option values mean
    /// the same credential while comparing unequal.
    pub fn parse(value: &str) -> Result<Self, EncryptedTargetCredentialError> {
        if value.is_empty() || value.len() > MAX_ELS2_CREDENTIAL_OPTION_LEN {
            return Err(EncryptedTargetCredentialError::OverBound);
        }
        let (scheme, key) = value
            .split_once(':')
            .ok_or(EncryptedTargetCredentialError::UnknownScheme)?;
        let bytes = decode_hex_32(key).ok_or(EncryptedTargetCredentialError::MalformedKey)?;
        match scheme {
            "psk" => Ok(Self::Psk(PskClientKey::from_bytes(bytes))),
            "dh" => {
                let private = X25519PrivateKey::from_bytes(bytes);
                let public = AuthClientPublicKey::from_bytes(private.public_bytes());
                Ok(Self::Dh { private, public })
            }
            _ => Err(EncryptedTargetCredentialError::UnknownScheme),
        }
    }

    /// Renders the credential back to its option value, for sealing.
    ///
    /// The round trip `parse(value) -> to_option_value()` is the identity, which is what lets the
    /// control plane seal a value it received and open it again after a restart without a second
    /// encoding. The plaintext exists only for the duration of the caller's sealing step.
    pub fn to_option_value(&self) -> String {
        match self {
            Self::Psk(key) => format!("psk:{}", to_hex(key.as_bytes())),
            Self::Dh { private, .. } => format!("dh:{}", to_hex(private.secret_bytes())),
        }
    }

    /// Reborrows the key material as the protocol owner's credential type.
    ///
    /// Split into a named method so the borrow is created at the call site rather than spanning a
    /// `&mut` borrow of the resolver that is driven with it — the same reason
    /// `OwnedClientCredential::as_borrowed` exists on the request side.
    pub fn as_borrowed(&self) -> Els2ClientAuth<'_> {
        match self {
            Self::Psk(key) => Els2ClientAuth::Psk(key),
            Self::Dh { private, public } => Els2ClientAuth::Dh {
                private,
                public: *public,
            },
        }
    }

    /// The scheme this credential presents.
    pub const fn scheme(&self) -> Els2AuthScheme {
        match self {
            Self::Psk(_) => Els2AuthScheme::Psk,
            Self::Dh { .. } => Els2AuthScheme::Dh,
        }
    }
}

/// The sealed stored form of a consumer credential, plus the owner that opens it.
///
/// What the manager holds long-lived. The plaintext never appears in this struct, so a heap dump
/// of a running router shows only ciphertext, and the `Debug` below can safely report that a
/// credential is configured without risking that it prints one.
pub struct SealedEncryptedTargetCredential {
    sealed: String,
    store: Arc<dyn RouterSecretOwner>,
}

impl SealedEncryptedTargetCredential {
    /// Seals one plaintext option value into the stored form.
    ///
    /// The plaintext is consumed here and dropped at the end of the call; the returned value holds
    /// only the ciphertext. A credential that cannot be sealed is an error, never a plaintext
    /// fallback — the same rule Plan 342 states for the outproxy credential.
    pub fn seal(
        plaintext: &str,
        store: Arc<dyn RouterSecretOwner>,
    ) -> Result<Self, EncryptedTargetCredentialError> {
        if !store.is_available() {
            return Err(EncryptedTargetCredentialError::NoSecretOwner);
        }
        let sealed = store
            .seal_credential(
                &OutboundSecret::new(plaintext)
                    .map_err(|_| EncryptedTargetCredentialError::OverBound)?,
            )
            .map_err(|_| EncryptedTargetCredentialError::CannotOpen)?;
        // Prove the framing before it is stored. A value this owner produced cannot fail, so a
        // failure here means the owner is not the one that sealed it.
        validate_els2_credential_form(&sealed)
            .map_err(|_| EncryptedTargetCredentialError::CannotOpen)?;
        Ok(Self { sealed, store })
    }

    /// Adopts an already-sealed stored form, as read back from a generation file.
    ///
    /// Framing **and** authenticity are both proved here, before installation,
    /// by opening the value and dropping the result. That is deliberate: a
    /// form sealed under a different router identity — the copied-data-directory
    /// case — otherwise installs cleanly and fails only later, at resolve time,
    /// as a per-service status an operator has to correlate with a router
    /// restart. Refusing it here makes it a failed control transaction, which is
    /// the same posture the lookup secret's reconciliation already takes.
    ///
    /// The cost is one AEAD open of a bounded frame per reconciliation, which
    /// happens on configuration mutations rather than on a timer.
    pub fn from_sealed(
        sealed: &str,
        store: Arc<dyn RouterSecretOwner>,
    ) -> Result<Self, EncryptedTargetCredentialError> {
        validate_els2_credential_form(sealed)
            .map_err(|_| EncryptedTargetCredentialError::CannotOpen)?;
        let candidate = Self {
            sealed: sealed.to_owned(),
            store,
        };
        // Opening also proves the plaintext parses as a credential, so a stored
        // form that decrypts to something this router cannot use is refused at
        // the same point rather than at first use.
        candidate.open()?;
        Ok(candidate)
    }

    /// Opens the stored form back into usable key material.
    ///
    /// The returned credential is owned and zeroizing; the caller passes it into one resolution
    /// and lets it drop. Opening happens here, inside the resolve path, and nowhere else.
    pub fn open(&self) -> Result<EncryptedTargetCredential, EncryptedTargetCredentialError> {
        let plaintext = self
            .store
            .open_credential(&self.sealed)
            .map_err(|_| EncryptedTargetCredentialError::CannotOpen)?;
        let text = plaintext
            .expose_str()
            .map_err(|_| EncryptedTargetCredentialError::CannotOpen)?;
        EncryptedTargetCredential::parse(text)
    }

    /// Whether a credential is configured. Safe for a status surface.
    pub fn is_configured(&self) -> bool {
        true
    }

    /// The sealed stored form, as ciphertext.
    ///
    /// Returns `&str`, not a re-render: this is the exact frame the control
    /// plane stored, so a generation file round-trips byte-for-byte and a
    /// restart does not depend on a second rendering agreeing with the first.
    ///
    /// Safe to expose because the value is ciphertext under a key derived from
    /// this router's identity. It is not a secret to *this* router and it is
    /// useless to any other, which is why the presence probe and the "is this
    /// still the right shape?" check can both read it. The plaintext behind it
    /// remains reachable only through [`Self::open`].
    pub fn sealed_form(&self) -> &str {
        &self.sealed
    }
}

/// Presence and size only. Never the sealed bytes, and certainly never a key.
impl core::fmt::Debug for SealedEncryptedTargetCredential {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("SealedEncryptedTargetCredential")
            .field("configured", &true)
            .field("sealed_bytes", &self.sealed.len())
            .finish()
    }
}

/// Strict lowercase-hex decoding of exactly one 32-byte key.
fn decode_hex_32(text: &str) -> Option<[u8; 32]> {
    let bytes = text.as_bytes();
    if bytes.len() != KEY_HEX_LEN {
        return None;
    }
    let mut out = [0_u8; 32];
    for (index, pair) in bytes.chunks_exact(2).enumerate() {
        out[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    Some(out)
}

/// One lowercase hex digit. Uppercase is rejected rather than folded: a canonical option value
/// round-trips through this exact spelling, so accepting both would admit two stored forms for
/// one credential.
fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Lowercase hex encoding.
fn to_hex(bytes: &[u8; 32]) -> String {
    const DIGITS: [u8; 16] = *b"0123456789abcdef";
    let mut out = String::with_capacity(KEY_HEX_LEN);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn psk_value(seed: u8) -> String {
        format!("psk:{}", to_hex(&[seed; 32]))
    }

    fn dh_value(seed: u8) -> String {
        format!("dh:{}", to_hex(&[seed; 32]))
    }

    #[test]
    fn both_schemes_parse_and_round_trip_exactly() {
        for value in [psk_value(0xa1), dh_value(0xb2)] {
            let credential = EncryptedTargetCredential::parse(&value).expect("parses");
            assert_eq!(
                credential.to_option_value(),
                value,
                "parse then render must be the identity"
            );
        }
    }

    #[test]
    fn the_scheme_is_reported_and_never_the_key() {
        assert_eq!(
            EncryptedTargetCredential::parse(&psk_value(1))
                .expect("psk")
                .scheme(),
            Els2AuthScheme::Psk
        );
        assert_eq!(
            EncryptedTargetCredential::parse(&dh_value(1))
                .expect("dh")
                .scheme(),
            Els2AuthScheme::Dh
        );
    }

    #[test]
    fn a_dh_credential_derives_its_public_key_from_the_private_one() {
        let value = dh_value(0x5a);
        let credential = EncryptedTargetCredential::parse(&value).expect("dh");
        let EncryptedTargetCredential::Dh { private, public } = &credential else {
            panic!("a dh: value must parse as the DH variant");
        };
        assert_eq!(
            *public,
            AuthClientPublicKey::from_bytes(private.public_bytes()),
            "the configured public key must be the one the private key derives"
        );
    }

    #[test]
    fn malformed_values_are_refused_by_shape_and_never_echoed() {
        // `Result::unwrap_err` is unusable here on purpose: it requires the
        // success type to be `Debug`, and the whole point of this type is that
        // it is not. Every assertion below therefore matches the error rather
        // than unwrapping, which is itself a standing check that the credential
        // type never grows a `Debug`.
        let error_of = |value: &str| match EncryptedTargetCredential::parse(value) {
            Ok(_) => panic!("must refuse {value}"),
            Err(error) => error,
        };

        // Empty and over-bound.
        assert_eq!(error_of(""), EncryptedTargetCredentialError::OverBound);
        assert_eq!(
            error_of(&format!("psk:{}", "00".repeat(200))),
            EncryptedTargetCredentialError::OverBound
        );
        // Missing scheme, unknown scheme.
        assert_eq!(
            error_of(&"00".repeat(32)),
            EncryptedTargetCredentialError::UnknownScheme
        );
        assert_eq!(
            error_of(&format!("rsa:{}", "00".repeat(32))),
            EncryptedTargetCredentialError::UnknownScheme
        );
        // Short, long, non-hex, and uppercase hex are all the same refusal.
        for bad in [
            format!("psk:{}", "00".repeat(31)),
            format!("psk:{}", "00".repeat(33)),
            format!("psk:{}", "zz".repeat(32)),
            format!("psk:{}", "AA".repeat(32)),
        ] {
            assert_eq!(
                error_of(&bad),
                EncryptedTargetCredentialError::MalformedKey,
                "must refuse {bad}"
            );
        }
    }

    /// The credential has no `Debug`, so the only way to learn anything about
    /// it for an operator is the scheme — which is public protocol material —
    /// and the option spelling, which the control plane uses to seal it.
    #[test]
    fn the_credential_is_describable_without_being_printable() {
        let credential = EncryptedTargetCredential::parse(&psk_value(0x7e)).expect("psk");
        assert_eq!(credential.scheme(), Els2AuthScheme::Psk);
        assert!(credential.to_option_value().starts_with("psk:"));
        // A borrowed credential is reachable, which is how the resolver gets
        // it, and it is a borrow of this value rather than a copy.
        let borrowed = credential.as_borrowed();
        assert_eq!(borrowed.scheme(), Els2AuthScheme::Psk);
    }
}
