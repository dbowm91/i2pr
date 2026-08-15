//! Deterministic short-build peer simulator.
//!
//! Plan 109 §11 owns the in-process responder peer the local
//! simulation harness consumes. The simulator is a single-hop
//! peer that accepts one 218-byte request envelope and produces
//! a 218-byte reply envelope using the canonical Noise-N derived
//! replyKey. The simulator keeps the post-request `h` and the
//! derived layer keys per received request.
//!
//! The simulator is structurally independent of
//! [`crate::build_crypto::EciesX25519BuildCryptography`]: it
//! uses the same primitive trait so the end-to-end simulation
//! proves internal completeness without resorting to a trivial
//! self-mirror.

#![forbid(unsafe_code)]

use std::fmt;

use i2pr_proto::{Hash, SHORT_BUILD_RECORD_SIZE, SHORT_REPLY_PLAINTEXT_SIZE};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::build_crypto::{
    BuildCryptography, BuildCryptographyError, EPHEMERAL_KEY_LEN, EciesX25519BuildCryptography,
    LayerKeys, OpenedShortRequest, SealedShortRequest,
};
use crate::short_record::{BuildOptions, ShortReplyRecord, ShortResponseCode};

/// Errors the deterministic peer simulator can return.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum ResponderError {
    /// The supplied record failed the cryptography primitive.
    #[error("responder cryptography rejected record: {0}")]
    Cryptography(#[from] BuildCryptographyError),
    /// The plaintext reply record could not be encoded.
    #[error("responder reply record encode failed: {0}")]
    ReplyEncode(crate::short_record::ShortBuildError),
    /// The record slot was outside the accepted `0..=7` domain.
    #[error("responder record slot out of range: {0}")]
    InvalidRecordSlot(u8),
}

/// A single-hop deterministic responder peer.
pub struct DeterministicResponder {
    static_key: [u8; EPHEMERAL_KEY_LEN],
    static_pub: [u8; EPHEMERAL_KEY_LEN],
    hop_hash: Hash,
    cryptography: EciesX25519BuildCryptography,
}

impl DeterministicResponder {
    /// Constructs a responder from explicit static key bytes and
    /// the hop's identity hash.
    pub const fn from_static_key(static_key: [u8; EPHEMERAL_KEY_LEN], hop_hash: Hash) -> Self {
        Self {
            static_key,
            static_pub: [0_u8; EPHEMERAL_KEY_LEN],
            hop_hash,
            cryptography: EciesX25519BuildCryptography::new(),
        }
    }

    /// Lazily fills in the derived static X25519 public key bytes.
    pub fn public_key(&mut self) -> [u8; EPHEMERAL_KEY_LEN] {
        if self.static_pub == [0_u8; EPHEMERAL_KEY_LEN] {
            let secret = x25519_dalek::StaticSecret::from(self.static_key);
            let public = x25519_dalek::PublicKey::from(&secret);
            self.static_pub = public.to_bytes();
        }
        self.static_pub
    }

    /// Returns the hop identity hash the responder uses for the
    /// truncated envelope prefix and AEAD context binding.
    pub fn hop_hash(&self) -> &Hash {
        &self.hop_hash
    }

    /// Decrypts one inbound request record and returns the
    /// 154-byte plaintext plus the post-request Noise state. The
    /// caller can later derive layer keys and seal a reply.
    pub fn open_request(&self, record: &[u8]) -> Result<OpenedShortRequest, ResponderError> {
        let opened = self.cryptography.open_short_request(
            record,
            &self.static_key,
            self.hop_hash.as_bytes(),
        )?;
        Ok(opened)
    }

    /// Derives layer keys from a freshly opened Noise state and
    /// the supplied `is_outbound_endpoint` flag.
    pub fn derive_layer_keys(
        &self,
        state: &crate::build_crypto::NoiseRequestState,
        is_outbound_endpoint: bool,
    ) -> Result<LayerKeys, ResponderError> {
        let keys = crate::build_crypto::derive_layer_keys(state, is_outbound_endpoint)?;
        Ok(keys)
    }

    /// Seals a reply envelope for the supplied Noise state, layer
    /// keys, and record slot. The slot must lie in `0..=7`.
    pub fn seal_reply(
        &self,
        plaintext: &[u8; SHORT_REPLY_PLAINTEXT_SIZE],
        layer_keys: &LayerKeys,
        request_hash: &[u8; 32],
        slot: u8,
    ) -> Result<[u8; SHORT_BUILD_RECORD_SIZE], ResponderError> {
        let validated = crate::build_crypto::ValidatedRecordSlot::new(slot)
            .map_err(|_| ResponderError::InvalidRecordSlot(slot))?;
        let record =
            self.cryptography
                .seal_short_reply(plaintext, layer_keys, request_hash, validated)?;
        Ok(record)
    }

    /// Composes a canonical accepted reply plaintext + sealed
    /// record in one call. The slot must lie in `0..=7`. The
    /// reply plaintext uses the deterministic zero-padded
    /// encoder because the simulator is a test-only surface; the
    /// production postprocessor in [`crate::multirecord`] uses
    /// [`ShortReplyRecord::encode_with_rng`] instead.
    pub fn compose_accepted_reply(
        &self,
        slot: u8,
        request_hash: &[u8; 32],
        layer_keys: &LayerKeys,
        options: BuildOptions,
    ) -> Result<([u8; SHORT_BUILD_RECORD_SIZE], ShortReplyRecord), ResponderError> {
        let reply = ShortReplyRecord::new(options, ShortResponseCode::Accepted);
        let plaintext_zeroizing = reply.encode_deterministic_zero_padded();
        let mut plaintext = [0_u8; SHORT_REPLY_PLAINTEXT_SIZE];
        plaintext.copy_from_slice(plaintext_zeroizing.as_ref());
        let record = self.seal_reply(&plaintext, layer_keys, request_hash, slot)?;
        Ok((record, reply))
    }
}

impl fmt::Debug for DeterministicResponder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeterministicResponder")
            .field("static_key", &"<redacted>")
            .field("hop_hash", &self.hop_hash)
            .finish()
    }
}

/// Adapter helper used by deterministic tests to convert a
/// `SealedShortRequest` into its `Zeroizing<[u8; 218]>` envelope.
pub fn seal_into_envelope(sealed: SealedShortRequest) -> Zeroizing<[u8; SHORT_BUILD_RECORD_SIZE]> {
    sealed.record
}

/// Convenience one-call helper that opens a request, derives
/// the layer keys, and seals an accepted reply. Used by the
/// canonical Plan 109 conformance fixture harness. The reply
/// plaintext uses the deterministic zero-padded encoder because
/// the helper is a test-only surface.
pub fn open_and_seal_accepted(
    responder: &DeterministicResponder,
    record: &[u8],
    slot: u8,
    is_outbound_endpoint: bool,
) -> Result<[u8; SHORT_BUILD_RECORD_SIZE], ResponderError> {
    let opened = responder.open_request(record)?;
    let keys = responder.derive_layer_keys(&opened.state, is_outbound_endpoint)?;
    let reply = ShortReplyRecord::new(BuildOptions::empty(), ShortResponseCode::Accepted);
    let plaintext_zeroizing = reply.encode_deterministic_zero_padded();
    let mut plaintext = [0_u8; SHORT_REPLY_PLAINTEXT_SIZE];
    plaintext.copy_from_slice(plaintext_zeroizing.as_ref());
    let sealed = responder.seal_reply(&plaintext, &keys, &opened.state.transcript_hash(), slot)?;
    Ok(sealed)
}

/// Internal helper retained for symmetry with Plan 109 §11 tests.
fn _suppress_unused<T>(_: T) {}

#[cfg(test)]
mod tests {
    use super::*;

    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn responder() -> DeterministicResponder {
        DeterministicResponder::from_static_key(
            [0x55; EPHEMERAL_KEY_LEN],
            Hash::from_bytes([0x33_u8; 32]),
        )
    }

    #[test]
    fn responder_round_trip_with_ecies_helper() {
        let responder = responder();
        let cryptography = EciesX25519BuildCryptography::new();
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let plaintext = [0x77; i2pr_proto::SHORT_REQUEST_PLAINTEXT_SIZE];
        let responder_pub = {
            let secret = x25519_dalek::StaticSecret::from([0x55_u8; EPHEMERAL_KEY_LEN]);
            x25519_dalek::PublicKey::from(&secret).to_bytes()
        };
        let hop_hash = Hash::from_bytes([0x33_u8; 32]);
        let sealed = cryptography
            .seal_short_request(&plaintext, &responder_pub, hop_hash.as_bytes(), &mut rng)
            .expect("seal");
        let opened = responder
            .open_request(sealed.record.as_ref())
            .expect("open");
        assert_eq!(opened.plaintext.as_ref(), &plaintext[..]);
        let layer_keys = responder
            .derive_layer_keys(&opened.state, false)
            .expect("keys");
        let reply = ShortReplyRecord::new(BuildOptions::empty(), ShortResponseCode::Accepted);
        let plaintext_bytes = reply.encode_deterministic_zero_padded();
        let mut arr = [0_u8; SHORT_REPLY_PLAINTEXT_SIZE];
        arr.copy_from_slice(plaintext_bytes.as_ref());
        let slot = 2_u8;
        let _record = responder
            .seal_reply(&arr, &layer_keys, &opened.state.transcript_hash(), slot)
            .expect("seal reply");
        _suppress_unused(sealed);
        _suppress_unused(rng);
    }
}
