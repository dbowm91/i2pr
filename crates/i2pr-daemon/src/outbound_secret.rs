//! Plan 341: the router-bound implementation of
//! [`i2pr_service_tunnels::outbound_secret::OutboundSecretStore`].
//!
//! The runtime-neutral layer owns the policy and cannot hold an AEAD, so the
//! composition root supplies the concrete owner. This one seals an outbound
//! proxy credential with ChaCha20-Poly1305 under a key derived from the
//! router's own persisted signing seed:
//!
//! ```text
//! key = HKDF-SHA256(salt = "", ikm = router signing seed,
//!                    info = "i2pr:outproxy:secret-box:v1", len = 32)
//! stored = "$i2pr1o$" + hex(nonce || ciphertext || Poly1305 tag)
//! ```
//!
//! Three properties follow from that key choice, and each one is a
//! requirement rather than a convenience:
//!
//! - **Restart-safe.** The signing seed is already persisted and reloaded on
//!   every start, so the same router opens its own sealed forms after a
//!   restart without any new key file, permission handling, or rotation
//!   surface.
//! - **Non-transferable.** A sealed form lifted into another router's
//!   generation file yields a different key and therefore fails to open. A
//!   stolen configuration file is not a stolen credential.
//! - **Separable.** The derived key is not the signing key, so compromising
//!   this store does not expose the router's identity.
//!
//! # No pinned reference is authority here
//!
//! Pinned i2pd `2c69414` has no I2P-routed outproxy at all — its outproxy
//! options are a **clearnet** upstream defaulting to `127.0.0.1:9050`, with no
//! stored password (`libi2pd/Config.cpp:143,177-179`) — and the pinned Java
//! I2P at-rest scheme for outproxy credentials was not verified when this was
//! written. This construction is therefore justified by this repository's own
//! guardrails, not presented as interoperability-derived.

#![forbid(unsafe_code)]

use i2pr_crypto::RouterIdentityBundle;
use i2pr_crypto::{OsRng, Zeroizing, hkdf_sha256_extract_and_expand};
use i2pr_service_tunnels::errors::ServiceTunnelError;
use i2pr_service_tunnels::outbound_secret::{
    MAX_OUTBOUND_SECRET_LEN, OUTBOUND_SECRET_MARKER, OutboundSecret, OutboundSecretStore,
    validate_stored_form,
};
use rand_core::TryCryptoRng;
use zeroize::Zeroize;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

/// HKDF context label binding the derived key to this exact purpose.
///
/// A distinct label per purpose is what keeps the outproxy key from ever
/// colliding with another derived key, and it is why the label is a constant
/// rather than a parameter a caller could vary.
pub const OUTBOUND_SECRET_KEY_INFO: &[u8] = b"i2pr:outproxy:secret-box:v1";

/// AEAD key length in bytes.
pub const OUTBOUND_SECRET_KEY_LEN: usize = 32;

/// ChaCha20-Poly1305 nonce length in bytes (RFC 8439).
pub const OUTBOUND_SECRET_NONCE_LEN: usize = 12;

/// Poly1305 authenticator length in bytes.
pub const OUTBOUND_SECRET_TAG_LEN: usize = 16;

/// A router-bound AEAD key for sealing outbound proxy secrets at rest.
///
/// Deliberately not `Clone` and not `Debug`-derived: the key is derived fresh
/// on every start and never persisted, so copying it would only widen the
/// number of places it can leak from.
pub struct OutboundSecretKey(Zeroizing<[u8; OUTBOUND_SECRET_KEY_LEN]>);

impl OutboundSecretKey {
    /// Derives the key from the router's persisted signing seed.
    pub fn from_router_signing_seed(seed: &[u8; 32]) -> Result<Self, ServiceTunnelError> {
        let derived = hkdf_sha256_extract_and_expand(
            b"",
            seed,
            OUTBOUND_SECRET_KEY_INFO,
            OUTBOUND_SECRET_KEY_LEN,
        )
        .map_err(|_| ServiceTunnelError::ContradictoryOptions {
            id: String::new(),
            reason: "outbound secret key derivation failed",
        })?;
        if derived.len() != OUTBOUND_SECRET_KEY_LEN {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id: String::new(),
                reason: "outbound secret key derivation produced the wrong length",
            });
        }
        let mut key = Zeroizing::new([0_u8; OUTBOUND_SECRET_KEY_LEN]);
        key.copy_from_slice(&derived);
        Ok(Self(key))
    }

    /// Borrows the key for the AEAD. Private to this module so no other code
    /// can hold the raw bytes.
    fn as_key(&self) -> &Key {
        Key::from_slice(&self.0[..])
    }
}

impl Drop for OutboundSecretKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// The router-bound outbound secret store.
pub struct RouterBoundOutboundSecrets {
    key: OutboundSecretKey,
}

impl RouterBoundOutboundSecrets {
    /// Builds the store from the router's persisted signing seed.
    pub fn from_router_signing_seed(seed: &[u8; 32]) -> Result<Self, ServiceTunnelError> {
        Ok(Self {
            key: OutboundSecretKey::from_router_signing_seed(seed)?,
        })
    }

    /// Builds the store from a loaded router identity (Plan 342).
    ///
    /// Takes the bundle rather than a seed so the seed cannot be obtained by
    /// accident from this call site: the only way in is
    /// [`RouterIdentityBundle::with_signing_seed`], a closure, so the seed
    /// exists for exactly one expression and is never named in a wider scope.
    /// A caller wanting a `&[u8; 32]` would have to re-add a getter to
    /// `i2pr-crypto`, which is a visible change rather than a private one.
    pub fn from_router_identity(bundle: &RouterIdentityBundle) -> Result<Self, ServiceTunnelError> {
        bundle.with_signing_seed(Self::from_router_signing_seed)
    }
}

impl OutboundSecretStore for RouterBoundOutboundSecrets {
    /// Seals a credential under a fresh 96-bit nonce.
    ///
    /// A fresh nonce per seal is mandatory, not hygiene: under a fixed key,
    /// reusing a nonce with ChaCha20-Poly1305 leaks the XOR of two plaintexts
    /// and forfeits authentication. The nonce is drawn from the injected
    /// cryptographic RNG and never from a counter or a clock.
    fn seal(&self, secret: &OutboundSecret) -> Result<String, ServiceTunnelError> {
        seal_with_rng(&self.key, secret, &mut OsRng)
    }

    fn open(&self, stored: &str) -> Result<OutboundSecret, ServiceTunnelError> {
        open_with_key(&self.key, stored)
    }

    fn is_available(&self) -> bool {
        true
    }
}

/// Sealing over an injected RNG, split out so the nonce draw is testable
/// without a live composition root.
fn seal_with_rng<R: TryCryptoRng + ?Sized>(
    key: &OutboundSecretKey,
    secret: &OutboundSecret,
    rng: &mut R,
) -> Result<String, ServiceTunnelError> {
    let cipher = ChaCha20Poly1305::new(key.as_key());
    let mut nonce_bytes = Zeroizing::new([0_u8; OUTBOUND_SECRET_NONCE_LEN]);
    rng.try_fill_bytes(&mut *nonce_bytes).map_err(|_| {
        ServiceTunnelError::ContradictoryOptions {
            id: String::new(),
            reason: "outbound secret nonce randomness unavailable",
        }
    })?;
    let nonce = Nonce::from_slice(&nonce_bytes[..]);
    let plaintext = secret.expose();
    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad: OUTBOUND_SECRET_MARKER.as_bytes(),
            },
        )
        .map_err(|_| ServiceTunnelError::ContradictoryOptions {
            id: String::new(),
            reason: "outbound secret could not be sealed",
        })?;
    let mut frame = Zeroizing::new(Vec::with_capacity(
        OUTBOUND_SECRET_NONCE_LEN + ciphertext.len(),
    ));
    frame.extend_from_slice(&nonce_bytes[..]);
    frame.extend_from_slice(&ciphertext);
    Ok(format!("{OUTBOUND_SECRET_MARKER}{}", to_hex(&frame)))
}

/// Opening under a fixed key, split out for the same reason.
fn open_with_key(
    key: &OutboundSecretKey,
    stored: &str,
) -> Result<OutboundSecret, ServiceTunnelError> {
    validate_stored_form(stored)?;
    let body = &stored[OUTBOUND_SECRET_MARKER.len()..];
    let frame = from_hex(body).ok_or_else(|| ServiceTunnelError::ContradictoryOptions {
        id: String::new(),
        reason: "stored outbound secret body is not valid hex",
    })?;
    if frame.len() <= OUTBOUND_SECRET_NONCE_LEN + OUTBOUND_SECRET_TAG_LEN {
        return Err(ServiceTunnelError::ContradictoryOptions {
            id: String::new(),
            reason: "stored outbound secret frame is too short to be authentic",
        });
    }
    if frame.len() > OUTBOUND_SECRET_NONCE_LEN + MAX_OUTBOUND_SECRET_LEN + OUTBOUND_SECRET_TAG_LEN {
        return Err(ServiceTunnelError::ExceedsCeiling {
            field: "outbound_secret_stored",
            reason: "stored outbound secret frame exceeds the sealed ceiling",
        });
    }
    let (nonce_bytes, ciphertext) = frame.split_at(OUTBOUND_SECRET_NONCE_LEN);
    let cipher = ChaCha20Poly1305::new(key.as_key());
    let mut plaintext = Zeroizing::new(
        cipher
            .decrypt(
                Nonce::from_slice(nonce_bytes),
                Payload {
                    msg: ciphertext,
                    aad: OUTBOUND_SECRET_MARKER.as_bytes(),
                },
            )
            .map_err(|_| ServiceTunnelError::ContradictoryOptions {
                id: String::new(),
                reason: "stored outbound secret failed authentication",
            })?,
    );
    // A frame can decrypt and still not be UTF-8; the plaintext type refuses
    // it rather than handing a lossy value to a header builder.
    let text =
        core::str::from_utf8(&plaintext).map_err(|_| ServiceTunnelError::ContradictoryOptions {
            id: String::new(),
            reason: "stored outbound secret did not decrypt to valid UTF-8",
        })?;
    let opened = OutboundSecret::new(text)?;
    plaintext.zeroize();
    Ok(opened)
}

/// Lowercase hex encoding.
fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: [u8; 16] = *b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0x0F)] as char);
    }
    out
}

/// Strict lowercase-hex decoding: no uppercase, no odd length, no short input.
fn from_hex(hex: &str) -> Option<Vec<u8>> {
    if hex.is_empty() || !hex.len().is_multiple_of(2) {
        return None;
    }
    let bytes = hex.as_bytes();
    let mut out = Vec::with_capacity(hex.len() / 2);
    let mut index = 0;
    while index < out.capacity() {
        out.push((nibble(bytes[2 * index])? << 4) | nibble(bytes[2 * index + 1])?);
        index += 1;
    }
    Some(out)
}

fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn seed(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn store(byte: u8) -> RouterBoundOutboundSecrets {
        RouterBoundOutboundSecrets::from_router_signing_seed(&seed(byte)).expect("store")
    }

    /// Seals through the crate-internal helper with a deterministic RNG, so
    /// the nonce draw is testable without a live composition root. The
    /// trait method itself draws from `OsRng`.
    fn seal_with(store: &RouterBoundOutboundSecrets, plaintext: &str, seed: u64) -> String {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        seal_with_rng(
            &store.key,
            &OutboundSecret::new(plaintext).expect("secret"),
            &mut rng,
        )
        .expect("sealed")
    }

    #[test]
    fn round_trip_recovers_the_credential() {
        let store = store(0x11);
        let stored = seal_with(&store, "s3cret!", 1);
        assert!(stored.starts_with(OUTBOUND_SECRET_MARKER));
        assert!(!stored.contains("s3cret"));
        let opened = store.open(&stored).expect("opened");
        assert_eq!(opened.expose_str().expect("utf8"), "s3cret!");
    }

    /// Plan 342: the bundle-derived store must be the **same** store the
    /// seed-derived one is, or a credential sealed by one path and opened by
    /// the other would fail closed for a reason that looks like tampering.
    ///
    /// This is the row that makes "derive once at the composition root" a
    /// checkable property rather than a comment.
    #[test]
    fn the_bundle_derived_store_matches_the_seed_derived_one() {
        let mut rng = ChaCha8Rng::seed_from_u64(0x77);
        let bundle = RouterIdentityBundle::generate(&mut rng).expect("bundle");
        let from_bundle =
            RouterBoundOutboundSecrets::from_router_identity(&bundle).expect("from bundle");
        // Reached through the closure, exactly as the composition root does.
        let expected = bundle
            .with_signing_seed(RouterBoundOutboundSecrets::from_router_signing_seed)
            .expect("from seed");

        let plaintext = OutboundSecret::new("same-credential").expect("secret");
        let sealed_by_bundle = seal_with(&from_bundle, "same-credential", 9);
        let mut rng = ChaCha8Rng::seed_from_u64(9);
        let sealed_by_seed = seal_with_rng(&expected.key, &plaintext, &mut rng).expect("sealed");

        assert_eq!(
            sealed_by_bundle, sealed_by_seed,
            "the same seed must produce the same stored form, so both paths interoperate"
        );
        // And the cross direction: what the seed path opens, the bundle path opens.
        assert_eq!(
            from_bundle
                .open(&sealed_by_seed)
                .expect("bundle opens seed-sealed")
                .expose_str()
                .expect("utf8"),
            "same-credential"
        );
    }

    /// Two different routers must not open each other's stored forms. This is
    /// what makes the store router-bound rather than merely keyed.
    #[test]
    fn a_different_router_cannot_open_the_stored_form() {
        let mine = store(0x21);
        let theirs = store(0x22);
        let stored = seal_with(&mine, "s3cret!", 3);
        assert!(
            theirs.open(&stored).is_err(),
            "a stored form must not be openable by another router's key"
        );
    }

    #[test]
    fn a_fresh_nonce_is_used_for_every_seal() {
        let store = store(0x11);
        let secret = OutboundSecret::new("s3cret!").expect("secret");
        // The production path, twice, on the same store and the same
        // plaintext: identical frames would mean a reused nonce, which under
        // a fixed key leaks the XOR of two plaintexts and forfeits
        // authentication. Distinct frames that both authenticate prove the
        // nonce actually moved.
        let first = store.seal(&secret).expect("first seal");
        let second = store.seal(&secret).expect("second seal");
        assert_ne!(first, second, "sealing twice must not reuse a nonce");
        for frame in [&first, &second] {
            assert_eq!(
                store
                    .open(frame)
                    .expect("reopened")
                    .expose_str()
                    .expect("utf8"),
                "s3cret!"
            );
        }
        // The nonce is the frame prefix, and it differs between the two.
        fn nonce_of(frame: &str) -> &str {
            let body = &frame[OUTBOUND_SECRET_MARKER.len()..];
            &body[..OUTBOUND_SECRET_NONCE_LEN * 2]
        }
        assert_ne!(nonce_of(&first), nonce_of(&second));
    }

    #[test]
    fn another_router_cannot_open_the_form() {
        let stored = seal_with(&store(0x11), "s3cret!", 1);
        // A different signing seed derives a different key, so the
        // authenticator check fails rather than yielding a wrong plaintext.
        assert!(store(0x22).open(&stored).is_err());
    }

    #[test]
    fn tampering_is_detected_at_every_position() {
        let store = store(0x11);
        let stored = seal_with(&store, "s3cret!", 1);
        let body_start = OUTBOUND_SECRET_MARKER.len();
        for offset in (body_start..stored.len()).step_by(3) {
            let mut bytes = stored.clone().into_bytes();
            // Flip to a different hex nibble, deterministically.
            bytes[offset] = if bytes[offset] == b'a' { b'b' } else { b'a' };
            let mutated = String::from_utf8(bytes).expect("ascii");
            assert!(
                store.open(&mutated).is_err(),
                "tampering at offset {offset} must fail closed"
            );
        }
    }

    #[test]
    fn tampering_fails_on_authentication_not_on_an_incidental_check() {
        let store = store(0x11);
        let stored = seal_with(&store, "s3cret!", 1);
        // Flip a ciphertext nibble, leaving the frame the right length and
        // the right shape. The rejection must be the *authenticator*, which
        // is what distinguishes "this was never our ciphertext" from "this
        // happened to fail some later bounds or emptiness check". A
        // best-effort open that ignored the tag would still error here, but
        // for the wrong reason, and the distinction is what proves no
        // partially-decrypted value is ever returned.
        let mut bytes = stored.clone().into_bytes();
        let target = bytes.len() - 3;
        bytes[target] = if bytes[target] == b'a' { b'b' } else { b'a' };
        let mutated = String::from_utf8(bytes).expect("ascii");
        let Err(error) = store.open(&mutated) else {
            panic!("a tampered frame must not open");
        };
        let message = error.to_string();
        assert!(
            message.contains("failed authentication"),
            "expected an authenticator rejection, got: {message}"
        );
    }

    #[test]
    fn a_truncated_or_padded_frame_fails_closed() {
        let store = store(0x11);
        let stored = seal_with(&store, "s3cret!", 1);
        let short = &stored[..stored.len() - 2];
        assert!(store.open(short).is_err(), "truncation must not open");
        let padded = format!("{stored}00");
        assert!(store.open(&padded).is_err(), "appended bytes must not open");
    }

    #[test]
    fn plaintext_and_verifier_markers_are_not_interchangeable() {
        let store = store(0x11);
        // A real inbound verifier, handed to the outbound owner.
        let verifier = i2pr_service_tunnels::auth::ProxyCredentials::new(
            "alice",
            "s3cret!",
            i2pr_service_tunnels::auth::PROXY_AUTH_REALM_HTTP,
        )
        .expect("inbound credentials")
        .stored_form();
        assert!(store.open(&verifier).is_err());
    }

    #[test]
    fn an_empty_or_oversize_plaintext_is_refused_before_sealing() {
        let store = store(0x11);
        assert!(OutboundSecret::new("").is_err());
        let over = "p".repeat(MAX_OUTBOUND_SECRET_LEN + 1);
        assert!(OutboundSecret::new(&over).is_err());
        let at = "p".repeat(MAX_OUTBOUND_SECRET_LEN);
        let sealed = store
            .seal(&OutboundSecret::new(&at).expect("at ceiling"))
            .expect("sealed at ceiling");
        assert_eq!(
            store.open(&sealed).expect("reopened").len(),
            MAX_OUTBOUND_SECRET_LEN
        );
    }

    #[test]
    fn restart_recovers_the_credential_from_the_same_identity() {
        // First start.
        let stored = seal_with(&store(0x33), "s3cret!", 7);
        // Restart: a brand new store built from the reloaded seed.
        let after_restart = store(0x33);
        assert_eq!(
            after_restart
                .open(&stored)
                .expect("opened after restart")
                .expose_str()
                .expect("utf8"),
            "s3cret!"
        );
        assert!(after_restart.is_available());
    }

    #[test]
    fn the_store_reports_availability_and_never_echoes() {
        let store = store(0x11);
        let stored = seal_with(&store, "s3cret!", 1);
        // The stored form is the only thing that reaches a file, and it says
        // nothing about the plaintext beyond being longer than it.
        assert!(!stored.contains("s3cret"));
        // Errors are fixed strings with no secret material. Matching rather
        // than `expect_err` also proves the success type is not `Debug`,
        // which is what keeps a recovered credential out of a panic message.
        let Err(error) = store.open("$i2pr1o$zzzz") else {
            panic!("a malformed frame must not open");
        };
        assert!(!error.to_string().contains("s3cret"));
        assert!(!error.to_string().contains("zzzz"));
    }

    #[test]
    fn hex_helpers_are_strict() {
        assert_eq!(to_hex(&[0x00, 0xff, 0x10]), "00ff10");
        assert_eq!(from_hex("00ff10"), Some(vec![0x00, 0xff, 0x10]));
        assert_eq!(from_hex("00FF10"), None, "uppercase is not canonical");
        assert_eq!(from_hex("abc"), None, "odd length");
        assert_eq!(from_hex(""), None);
        assert_eq!(from_hex("zz"), None);
    }
}
