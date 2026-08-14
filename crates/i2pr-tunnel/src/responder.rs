//! Deterministic short-build peer simulator.
//!
//! Plan 108 §3.8 owns the in-process responder peer the local
//! simulation harness consumes. The simulator produces a
//! standards-shaped accepted/rejected response record for each
//! supplied request record and exposes the operations a future
//! stand-alone responder would expose.
//!
//! The simulator is structurally independent of
//! [`crate::build_crypto::EciesX25519BuildCryptography`]: it
//! uses the same primitive traits directly so the end-to-end
//! simulation proves internal completeness without resorting to a
//! trivial self-mirror.

#![forbid(unsafe_code)]

use thiserror::Error;

use crate::build_crypto::{
    BuildCryptography, BuildCryptographyError, EciesX25519BuildCryptography,
};
use crate::identity::TunnelId;
use crate::short_record::{ShortReplyRecord, ShortResponseCode};

/// Errors the deterministic peer simulator can return.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum ResponderError {
    /// The supplied record failed the cryptography primitive.
    #[error("responder cryptography rejected record: {0}")]
    Cryptography(#[from] BuildCryptographyError),
    /// The plaintext reply record could not be encoded.
    #[error("responder reply record encode failed: {0}")]
    ReplyEncode(crate::short_record::ShortBuildError),
}

/// A single-hop deterministic responder peer.
///
/// The responder holds a private static X25519 key and the
/// cryptography primitive the local harness uses to validate
/// inbound short request records.
#[derive(Debug)]
pub struct DeterministicResponder {
    static_key: [u8; 32],
    cryptography: EciesX25519BuildCryptography,
}

impl DeterministicResponder {
    /// Constructs a responder from explicit static key bytes.
    pub const fn from_static_key(static_key: [u8; 32]) -> Self {
        Self {
            static_key,
            cryptography: EciesX25519BuildCryptography::new(),
        }
    }

    /// Returns the responder's static X25519 public key as bytes.
    pub fn public_key(&self) -> [u8; 32] {
        let secret = x25519_dalek::StaticSecret::from(self.static_key);
        let public = x25519_dalek::PublicKey::from(&secret);
        public.to_bytes()
    }

    /// Decrypts one inbound request record and verifies the
    /// authentication tag. The cryptography primitive is shared
    /// with the ECIES-X25519 build helper. The responder uses the
    /// `request_key_seed` to reproduce the per-hop AEAD key.
    pub fn open_request(
        &self,
        record: &[u8],
        request_key_seed: &[u8; 32],
    ) -> Result<zeroize::Zeroizing<[u8; 154]>, ResponderError> {
        let plaintext =
            self.cryptography
                .open_short_request(record, &self.static_key, request_key_seed)?;
        Ok(plaintext)
    }

    /// Composes and seals a reply record that the creator can
    /// decrypt.
    pub fn seal_reply<R: rand_core::CryptoRng + rand_core::RngCore>(
        &self,
        creator_static_pub: &[u8; 32],
        next_message_id: u32,
        tunnel_id: TunnelId,
        expiration_ms: u64,
        response_code: ShortResponseCode,
        rng: &mut R,
    ) -> Result<zeroize::Zeroizing<Vec<u8>>, ResponderError> {
        let reply = ShortReplyRecord::try_new(
            response_code,
            expiration_ms,
            next_message_id,
            tunnel_id.get(),
        )
        .map_err(ResponderError::ReplyEncode)?;
        let plaintext = reply.encode();
        let mut reply_key_seed = [0_u8; 32];
        rng.try_fill_bytes(&mut reply_key_seed)
            .map_err(|_| ResponderError::Cryptography(BuildCryptographyError::InvalidPeerKey))?;
        let sealed = self.cryptography.seal_short_reply(
            plaintext.as_ref(),
            creator_static_pub,
            &reply_key_seed,
            rng,
        )?;
        // reply_key_seed must be zeroized; Zeroizing does that on drop
        reply_key_seed.zeroize();
        Ok(sealed)
    }
}

use rand_core::TryRngCore;
use zeroize::Zeroize;

#[cfg(test)]
mod tests {
    use super::*;

    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn responder() -> DeterministicResponder {
        DeterministicResponder::from_static_key([0x55; 32])
    }

    #[test]
    fn responder_accepts_records_sealed_by_ecies_helper() {
        let responder = responder();
        let creator_pub = responder.public_key();
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let request_key_seed = [0x66; 32];
        let plaintext = [0x77; 154];
        let record = cryptography
            .seal_short_request(&plaintext, &creator_pub, &request_key_seed, &mut rng)
            .expect("seal");
        let opened = responder
            .open_request(record.as_ref(), &request_key_seed)
            .expect("open");
        assert_eq!(&opened[..], &plaintext[..]);
    }
}
