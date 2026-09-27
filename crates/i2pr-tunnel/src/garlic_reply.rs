//! Plan 188 narrow build-reply Garlic unwrap.
//!
//! Exact-pinned i2pd 2.61.0 garlic-wraps one-hop outbound endpoint
//! replies (`ShortTunnelBuildReply` type 26) inside a `TunnelGateway`
//! (type 19) addressed to the creator-supplied `next_tunnel`, with
//! the Garlic (type 11) encrypted under the OBEP `RGarlicKeyAndTag`
//! (`garlic_reply_key` + 8-byte `garlic_reply_tag`) the creator
//! derived during request preparation.
//!
//! The previous coordinator dropped those frames as `Unsupported`
//! (`unsupported={11,19}`, `kind_reply=0`) and recorded the
//! `m6-build-reply-interop-gap`. This module owns the narrow,
//! bounded, runtime-neutral unwrap so the coordinator can feed the
//! recovered OTBRM payload into the existing
//! `ShortBuildStateMachine::handle_event(BuildEvent::BuildReply)`
//! seam without any wire-format change or authentication weakening.
//!
//! Reference: `/tmp/i2pd-src` at `635b013a612ff47278ef02acf8580a28e10e26c5`
//! (`TransitTunnel::HandleShortTransitTunnelBuildMsg` endpoint arm +
//! `ECIESX25519AEADRatchetSession::WrapECIESX25519Message`).
//!
//! The module is runtime-neutral: no sockets, no Tokio, no DNS, no
//! filesystem. All inputs are untrusted and bounded.

#![forbid(unsafe_code)]

use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce, aead::Aead};
use sha2::{Digest, Sha256};
use thiserror::Error;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
use zeroize::{Zeroize, Zeroizing};

use crate::build_crypto::{EPHEMERAL_KEY_LEN, GARLIC_REPLY_TAG_LEN, NOISE_PROTOCOL_NAME};

/// ECIES GarlicClove block type (`eECIESx25519BlkGalicClove`).
const GARLIC_CLOVE_BLOCK_TYPE: u8 = 11;
/// Maximum Garlic blocks parsed from one decrypted payload. The
/// reference emits at most DateTime + GarlicClove + Padding; the
/// bound keeps parsing O(1) over untrusted input.
const MAX_GARLIC_BLOCKS: usize = 16;
/// Maximum decrypted Garlic payload accepted. The reference wraps
/// one OTBRM (max `1 + 8*218 = 1745` bytes) plus clove framing and
/// padding well below this bound; anything larger is rejected.
const MAX_DECRYPTED_GARLIC_BYTES: usize = 4096;
/// Minimum Garlic opaque payload: 8-byte tag + 16-byte Poly tag +
/// minimal block header.
const MIN_GARLIC_OPAQUE_BYTES: usize = 8 + 16 + 3;
/// Inner GarlicClove fixed overhead: flag(1) + I2NP type(1) +
/// msgID(4) + expiration(4).
const GARLIC_CLOVE_INNER_OVERHEAD: usize = 10;
/// Expected inner I2NP type for a build reply (`eI2NPShortTunnelBuildReply`).
const SHORT_TUNNEL_BUILD_REPLY_TYPE: u8 = 26;

/// Typed Garlic build-reply unwrap failures. No variant carries
/// key material or plaintext.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GarlicReplyError {
    /// The opaque Garlic payload is too short to carry tag + Poly.
    #[error("garlic reply payload is too short")]
    TooShort,
    /// The opaque Garlic payload exceeds the bounded ceiling.
    #[error("garlic reply payload exceeds its bound")]
    TooLarge,
    /// The 8-byte session tag does not match the pending attempt.
    #[error("garlic reply tag does not match the pending attempt")]
    TagMismatch,
    /// AEAD authentication failed; the payload is not for this key.
    #[error("garlic reply AEAD authentication failed")]
    AuthenticationFailed,
    /// The decrypted payload exceeds the bounded ceiling.
    #[error("decrypted garlic payload exceeds its bound")]
    DecryptedTooLarge,
    /// The decrypted payload carries no complete block framing.
    #[error("decrypted garlic payload has no complete block")]
    TruncatedBlocks,
    /// Too many blocks; the payload is not a build-reply Garlic.
    #[error("decrypted garlic payload has too many blocks")]
    TooManyBlocks,
    /// No GarlicClove block was present.
    #[error("decrypted garlic payload carries no GarlicClove")]
    NoClove,
    /// The GarlicClove inner framing is truncated or mistyped.
    #[error("garlic clove inner framing is invalid")]
    InvalidClove,
    /// The inner I2NP type is not ShortTunnelBuildReply.
    #[error("garlic clove does not carry a ShortTunnelBuildReply")]
    UnexpectedInnerType,
    /// The recovered OTBRM payload fails the count/record contract.
    #[error("recovered build-reply payload fails its record contract")]
    InvalidReplyPayload,
    /// Garlic relay encryption failed; nothing was emitted.
    #[error("garlic relay AEAD encryption failed")]
    EncryptionFailed,
}

/// Recovered build-reply material. Lengths only; no secret material
/// leaves this struct beyond the reply records the state machine
/// already expects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecryptedBuildReply {
    /// Inner I2NP message id (must equal the pending attempt's
    /// `next_message_id` / header `message_id`).
    pub inner_message_id: u32,
    /// Inner expiration in seconds since the Unix epoch.
    pub inner_expiration_seconds: u32,
    /// Canonical count-prefixed OTBRM payload (`1 + count*218`).
    pub reply_payload: Vec<u8>,
}

/// Unwraps one OBEP Garlic reply using the pending attempt's
/// `RGarlicKeyAndTag` material.
///
/// `garlic_opaque` is the Garlic `OpaqueMessageBody` payload bytes
/// (after the 4-byte I2NP Garlic length prefix): 8-byte tag +
/// AEAD ciphertext + 16-byte Poly tag. The function verifies the
/// tag equals `expected_tag` before attempting AEAD decrypt with
/// `key`, nonce zero, and AD = tag, then parses the decrypted
/// cloves for the single `ShortTunnelBuildReply` clove.
///
/// All failures are typed and bounded; no secret material is logged
/// or exposed through `Debug` (the error enum carries no bytes).
pub fn decrypt_build_reply_garlic(
    key: &[u8; 32],
    expected_tag: &[u8; GARLIC_REPLY_TAG_LEN],
    garlic_opaque: &[u8],
) -> Result<DecryptedBuildReply, GarlicReplyError> {
    if garlic_opaque.len() < MIN_GARLIC_OPAQUE_BYTES {
        return Err(GarlicReplyError::TooShort);
    }
    // Bound the opaque input against the I2NP ceiling plus framing;
    // the caller already bounds via I2NP decoding, this is defense
    // in depth so decrypt never allocates beyond a small multiple.
    if garlic_opaque.len() > MAX_DECRYPTED_GARLIC_BYTES + 8 + 16 {
        return Err(GarlicReplyError::TooLarge);
    }
    let (tag_bytes, ciphertext) = garlic_opaque.split_at(GARLIC_REPLY_TAG_LEN);
    if tag_bytes != expected_tag {
        return Err(GarlicReplyError::TagMismatch);
    }
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = Nonce::from_slice(&[0_u8; 12]);
    let plaintext = cipher
        .decrypt(
            nonce,
            chacha20poly1305::aead::Payload {
                msg: ciphertext,
                aad: tag_bytes,
            },
        )
        .map_err(|_| GarlicReplyError::AuthenticationFailed)?;
    let plaintext = Zeroizing::new(plaintext);
    if plaintext.len() > MAX_DECRYPTED_GARLIC_BYTES {
        return Err(GarlicReplyError::DecryptedTooLarge);
    }
    parse_garlic_cloves(&plaintext)
}

/// Parses decrypted Garlic blocks for the build-reply clove.
fn parse_garlic_cloves(plaintext: &[u8]) -> Result<DecryptedBuildReply, GarlicReplyError> {
    let mut offset = 0_usize;
    let mut blocks_seen = 0_usize;
    while offset < plaintext.len() {
        if blocks_seen >= MAX_GARLIC_BLOCKS {
            return Err(GarlicReplyError::TooManyBlocks);
        }
        if plaintext.len().saturating_sub(offset) < 3 {
            return Err(GarlicReplyError::TruncatedBlocks);
        }
        let block_type = plaintext[offset];
        let size = u16::from_be_bytes([plaintext[offset + 1], plaintext[offset + 2]]) as usize;
        offset = offset.saturating_add(3);
        if plaintext.len().saturating_sub(offset) < size {
            return Err(GarlicReplyError::TruncatedBlocks);
        }
        let block = &plaintext[offset..offset.saturating_add(size)];
        offset = offset.saturating_add(size);
        blocks_seen = blocks_seen.saturating_add(1);
        if block_type != GARLIC_CLOVE_BLOCK_TYPE {
            continue;
        }
        if let Some(reply) = parse_garlic_clove(block)? {
            return Ok(reply);
        }
    }
    Err(GarlicReplyError::NoClove)
}

/// Parses one GarlicClove block body. Returns `Ok(None)` when the
/// block is a non-local delivery clove the build-reply path must
/// skip (never a build reply); `Ok(Some)` for the local
/// ShortTunnelBuildReply clove; `Err` for truncated framing.
fn parse_garlic_clove(block: &[u8]) -> Result<Option<DecryptedBuildReply>, GarlicReplyError> {
    if block.len() < GARLIC_CLOVE_INNER_OVERHEAD + 1 {
        return Err(GarlicReplyError::InvalidClove);
    }
    let flag = block[0];
    let delivery_type = (flag >> 5) & 0x03;
    // Local delivery (0) is the only shape the reference emits for
    // build replies (`CreateGarlicPayload` with `alwaysLocal`
    // false but no destination set). Tunnel (3) and Destination
    // (1) cloves are skipped, never accepted as replies.
    if delivery_type != 0 {
        return Ok(None);
    }
    let inner_type = block[1];
    if inner_type != SHORT_TUNNEL_BUILD_REPLY_TYPE {
        return Err(GarlicReplyError::UnexpectedInnerType);
    }
    let inner_message_id = u32::from_be_bytes([block[2], block[3], block[4], block[5]]);
    let inner_expiration_seconds = u32::from_be_bytes([block[6], block[7], block[8], block[9]]);
    let payload = &block[GARLIC_CLOVE_INNER_OVERHEAD..];
    // Validate the OTBRM count/record contract before returning so
    // the caller never feeds malformed bytes to the state machine.
    let (count, _) = crate::multirecord::validate_count_prefixed_short_payload(payload)
        .map_err(|_| GarlicReplyError::InvalidReplyPayload)?;
    if count == 0 {
        return Err(GarlicReplyError::InvalidReplyPayload);
    }
    Ok(Some(DecryptedBuildReply {
        inner_message_id,
        inner_expiration_seconds,
        reply_payload: payload.to_vec(),
    }))
}

/// Maximum count-prefixed OTBRM body accepted for Garlic relay
/// (`1 + 8*218` per the multi-record ceiling).
const MAX_RELAY_OTBRM_BODY: usize = 1 + 8 * 218;
/// Fixed Garlic padding bytes appended after the clove. The
/// reference emits a random 0-15 byte padding block; a fixed
/// 8-zero-byte block parses identically on every compliant
/// implementation and keeps the relay deterministic without
/// spending caller RNG.
const RELAY_PADDING_BYTES: usize = 8;
/// ECIES Padding block type (`eECIESx25519BlkPadding`).
const GARLIC_PADDING_BLOCK_TYPE: u8 = 254;

/// Wraps one count-prefixed OTBRM body in a symmetric Garlic
/// message for tunnel-relay delivery, mirroring the reference
/// `WrapECIESX25519Message` (`datetime=false`).
///
/// Layout: standard I2NP Garlic message whose opaque payload is
/// `tag(8) || AEAD(key, nonce=0, AD=tag, clove||padding) || poly(16)`.
/// The clove carries LOCAL delivery (flag 0), the OTBRM
/// type/message-id/expiration-seconds, and the raw body, followed
/// by the fixed padding block. The caller supplies the outer
/// Garlic message id and expiration; the inner clove reuses the
/// OTBRM reply id and expiration so the creator's pending lookup
/// matches after the relay unwraps.
///
/// Returns the complete standard-encoded Garlic message bytes
/// ready to nest inside a `TunnelGateway` envelope addressed to
/// the reply tunnel. All failures are typed and bounded; no key
/// material is logged or exposed.
pub fn wrap_obep_reply_garlic(
    payload: &[u8],
    message_id: u32,
    expiration_seconds: u32,
    outer_message_id: u32,
    outer_expiration: i2pr_proto::Date,
    key: &[u8; 32],
    tag: &[u8; GARLIC_REPLY_TAG_LEN],
) -> Result<Vec<u8>, GarlicReplyError> {
    if payload.len() > MAX_RELAY_OTBRM_BODY {
        return Err(GarlicReplyError::TooLarge);
    }
    // Validate the OTBRM shape before wrapping so malformed bytes
    // never enter a Garlic relay.
    let (count, _) = crate::multirecord::validate_count_prefixed_short_payload(payload)
        .map_err(|_| GarlicReplyError::InvalidReplyPayload)?;
    if count == 0 {
        return Err(GarlicReplyError::InvalidReplyPayload);
    }
    if message_id == 0 {
        return Err(GarlicReplyError::InvalidReplyPayload);
    }
    let mut clove_body = Vec::with_capacity(GARLIC_CLOVE_INNER_OVERHEAD + payload.len());
    clove_body.push(0_u8); // flag: local delivery
    clove_body.push(SHORT_TUNNEL_BUILD_REPLY_TYPE);
    clove_body.extend_from_slice(&message_id.to_be_bytes());
    clove_body.extend_from_slice(&expiration_seconds.to_be_bytes());
    clove_body.extend_from_slice(payload);
    let mut plaintext = Vec::with_capacity(3 + clove_body.len() + 3 + RELAY_PADDING_BYTES);
    plaintext.push(GARLIC_CLOVE_BLOCK_TYPE);
    plaintext.extend_from_slice(
        &u16::try_from(clove_body.len())
            .map_err(|_| GarlicReplyError::TooLarge)?
            .to_be_bytes(),
    );
    plaintext.extend_from_slice(&clove_body);
    plaintext.push(GARLIC_PADDING_BLOCK_TYPE);
    plaintext.extend_from_slice(
        &u16::try_from(RELAY_PADDING_BYTES)
            .map_err(|_| GarlicReplyError::TooLarge)?
            .to_be_bytes(),
    );
    plaintext.extend_from_slice(&[0_u8; RELAY_PADDING_BYTES]);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = Nonce::from_slice(&[0_u8; 12]);
    let ciphertext = cipher
        .encrypt(
            nonce,
            chacha20poly1305::aead::Payload {
                msg: &plaintext,
                aad: tag,
            },
        )
        .map_err(|_| GarlicReplyError::EncryptionFailed)?;
    let mut opaque = Vec::with_capacity(GARLIC_REPLY_TAG_LEN + ciphertext.len());
    opaque.extend_from_slice(tag);
    opaque.extend_from_slice(&ciphertext);
    let body = i2pr_proto::I2npBody::Garlic(i2pr_proto::OpaqueMessageBody {
        payload: i2pr_proto::DeferredPayload::new(opaque, i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
            .map_err(|_| GarlicReplyError::TooLarge)?,
    });
    let message = i2pr_proto::I2npMessage::new_standard(outer_message_id, outer_expiration, body)
        .map_err(|_| GarlicReplyError::TooLarge)?;
    message
        .encode_standard_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
        .map_err(|_| GarlicReplyError::TooLarge)
}

/// Inner I2NP type for a tunnel-build request
/// (`eI2NPShortTunnelBuild`).
pub const SHORT_TUNNEL_BUILD_TYPE: u8 = 25;
/// Maximum router-garlic opaque payload accepted for the responder
/// unwrap. A wrapped STBM (`eph(32) + clove framing + at most
/// `1 + 8*218` body + padding + poly(16)`) stays far below this
/// bound; anything larger is rejected without allocating.
const MAX_ROUTER_GARLIC_OPAQUE: usize = 8192;
/// Minimum router-garlic opaque payload: ephemeral key + Poly tag +
/// minimal block header.
const MIN_ROUTER_GARLIC_OPAQUE: usize = EPHEMERAL_KEY_LEN + 16 + 3;

/// Non-secret GarlicClove facts recovered from a decrypted Garlic
/// payload. Carries only the framing the reference already
/// publishes (inner type, message id, expiration, raw inner
/// payload); no key material.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GarlicClove {
    /// Inner I2NP message type (25 = STBM, 26 = OTBRM).
    pub inner_type: u8,
    /// Inner I2NP message id.
    pub message_id: u32,
    /// Inner expiration in seconds since the Unix epoch.
    pub expiration_seconds: u32,
    /// Raw inner payload bytes (count-prefixed records for
    /// build messages).
    pub payload: Vec<u8>,
}

/// Extracts the first local-delivery GarlicClove from a decrypted
/// Garlic payload. Skips DateTime/Padding blocks exactly like the
/// build-reply parse; non-local cloves are rejected (the relay
/// path only accepts router-local delivery). Truncated framing
/// fails closed; the caller validates the payload shape (STBM
/// vs OTBRM record contract) before any state mutation.
pub fn extract_garlic_clove(plaintext: &[u8]) -> Result<GarlicClove, GarlicReplyError> {
    if plaintext.len() > MAX_DECRYPTED_GARLIC_BYTES {
        return Err(GarlicReplyError::DecryptedTooLarge);
    }
    let mut offset = 0_usize;
    let mut blocks_seen = 0_usize;
    while offset < plaintext.len() {
        if blocks_seen >= MAX_GARLIC_BLOCKS {
            return Err(GarlicReplyError::TooManyBlocks);
        }
        if plaintext.len().saturating_sub(offset) < 3 {
            return Err(GarlicReplyError::TruncatedBlocks);
        }
        let block_type = plaintext[offset];
        let size = u16::from_be_bytes([plaintext[offset + 1], plaintext[offset + 2]]) as usize;
        offset = offset.saturating_add(3);
        if plaintext.len().saturating_sub(offset) < size {
            return Err(GarlicReplyError::TruncatedBlocks);
        }
        let block = &plaintext[offset..offset.saturating_add(size)];
        offset = offset.saturating_add(size);
        blocks_seen = blocks_seen.saturating_add(1);
        if block_type != GARLIC_CLOVE_BLOCK_TYPE {
            continue;
        }
        if block.len() < GARLIC_CLOVE_INNER_OVERHEAD + 1 {
            return Err(GarlicReplyError::InvalidClove);
        }
        if (block[0] >> 5) & 0x03 != 0 {
            return Err(GarlicReplyError::InvalidClove);
        }
        return Ok(GarlicClove {
            inner_type: block[1],
            message_id: u32::from_be_bytes([block[2], block[3], block[4], block[5]]),
            expiration_seconds: u32::from_be_bytes([block[6], block[7], block[8], block[9]]),
            payload: block[GARLIC_CLOVE_INNER_OVERHEAD..].to_vec(),
        });
    }
    Err(GarlicReplyError::NoClove)
}

/// Opens one Noise-N router Garlic (`WrapECIESX25519MessageForRouter`
/// shape) addressed to this hop's static key and returns the
/// decrypted Garlic payload.
///
/// Layout: `ephemeral_pub(32) || AEAD(h, key, nonce=0)
/// (clove plaintext) || poly(16)`, where the transcript starts
/// from the canonical Noise-N prologue mixed with this hop's
/// static public key and then the ephemeral key, and the AEAD key
/// comes from `MixKey(DH(static_priv, ephemeral))`. This mirrors
/// the pinned reference responder arm byte-for-byte; the KDF
/// inputs were validated against the frozen Plan 111 vectors
/// (same `mix_key` chain).
///
/// `static_priv` is this hop's ECIES static secret (the same key
/// that opens short-build request envelopes). All failures map
/// to bounded typed errors with no oracle detail; key material
/// never leaves the return value on failure.
pub fn open_router_garlic(
    opaque: &[u8],
    static_priv: &[u8; EPHEMERAL_KEY_LEN],
) -> Result<Zeroizing<Vec<u8>>, GarlicReplyError> {
    if opaque.len() < MIN_ROUTER_GARLIC_OPAQUE {
        return Err(GarlicReplyError::TooShort);
    }
    if opaque.len() > MAX_ROUTER_GARLIC_OPAQUE {
        return Err(GarlicReplyError::TooLarge);
    }
    let mut ephemeral_pub = [0_u8; EPHEMERAL_KEY_LEN];
    ephemeral_pub.copy_from_slice(&opaque[..EPHEMERAL_KEY_LEN]);
    if ephemeral_pub.iter().all(|byte| *byte == 0) {
        return Err(GarlicReplyError::AuthenticationFailed);
    }
    let ciphertext = &opaque[EPHEMERAL_KEY_LEN..];
    // Noise-N responder transcript: h0, MixHash(static_pub),
    // MixHash(ephemeral_pub).
    let secret = StaticSecret::from(*static_priv);
    let local_pub = X25519PublicKey::from(&secret);
    let mut h0 = [0_u8; 32];
    h0[..NOISE_PROTOCOL_NAME.len()].copy_from_slice(NOISE_PROTOCOL_NAME);
    let mut hasher = Sha256::new();
    hasher.update(h0);
    let null_h = hasher.finalize();
    let mut hasher = Sha256::new();
    hasher.update(null_h);
    hasher.update(local_pub.as_bytes());
    let mut h = hasher.finalize().to_vec();
    let mut hasher = Sha256::new();
    hasher.update(&h);
    hasher.update(ephemeral_pub);
    h = hasher.finalize().to_vec();
    // MixKey(DH(static_priv, ephemeral_pub)).
    let peer = X25519PublicKey::from(ephemeral_pub);
    let shared = secret.diffie_hellman(&peer);
    if shared.as_bytes().iter().all(|byte| *byte == 0) {
        return Err(GarlicReplyError::AuthenticationFailed);
    }
    let keydata = i2pr_crypto::hkdf_sha256_extract_and_expand(&h0, shared.as_bytes(), &[], 64)
        .map_err(|_| GarlicReplyError::AuthenticationFailed)?;
    let mut aead_key = [0_u8; 32];
    aead_key.copy_from_slice(&keydata[32..64]);
    // `keydata` is `Zeroizing` and wipes on drop.
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&aead_key));
    aead_key.zeroize();
    let nonce = Nonce::from_slice(&[0_u8; 12]);
    let plaintext = cipher
        .decrypt(
            nonce,
            chacha20poly1305::aead::Payload {
                msg: ciphertext,
                aad: &h,
            },
        )
        .map_err(|_| GarlicReplyError::AuthenticationFailed)?;
    if plaintext.len() > MAX_DECRYPTED_GARLIC_BYTES {
        return Err(GarlicReplyError::DecryptedTooLarge);
    }
    Ok(Zeroizing::new(plaintext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha8Rng;
    use rand_core::{RngCore, SeedableRng};

    fn test_key_tag() -> ([u8; 32], [u8; 8]) {
        let mut rng = ChaCha8Rng::seed_from_u64(0x188);
        let mut key = [0_u8; 32];
        let mut tag = [0_u8; 8];
        rng.fill_bytes(&mut key);
        rng.fill_bytes(&mut tag);
        (key, tag)
    }

    fn build_garlic_opaque(
        key: &[u8; 32],
        tag: &[u8; 8],
        inner_msg_id: u32,
        reply_payload: &[u8],
    ) -> Vec<u8> {
        // Build the reference-shaped GarlicClove + Padding payload,
        // then AEAD-encrypt with nonce zero and AD = tag.
        let mut clove_body = Vec::new();
        clove_body.push(0_u8); // flag: local delivery
        clove_body.push(SHORT_TUNNEL_BUILD_REPLY_TYPE);
        clove_body.extend_from_slice(&inner_msg_id.to_be_bytes());
        clove_body.extend_from_slice(&1_700_000_060_u32.to_be_bytes());
        clove_body.extend_from_slice(reply_payload);
        let mut decrypted = Vec::new();
        decrypted.push(GARLIC_CLOVE_BLOCK_TYPE);
        decrypted.extend_from_slice(&(clove_body.len() as u16).to_be_bytes());
        decrypted.extend_from_slice(&clove_body);
        // Minimal padding block to match reference shape.
        decrypted.push(254_u8);
        decrypted.extend_from_slice(&0_u16.to_be_bytes());
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
        let nonce = Nonce::from_slice(&[0_u8; 12]);
        let ciphertext = cipher
            .encrypt(
                nonce,
                chacha20poly1305::aead::Payload {
                    msg: &decrypted,
                    aad: tag,
                },
            )
            .expect("encrypt");
        let mut opaque = Vec::with_capacity(8 + ciphertext.len());
        opaque.extend_from_slice(tag);
        opaque.extend_from_slice(&ciphertext);
        opaque
    }

    fn sample_reply_payload() -> Vec<u8> {
        let mut payload = Vec::with_capacity(1 + 218);
        payload.push(1_u8);
        payload.extend(std::iter::repeat_n(0x42_u8, 218));
        payload
    }

    #[test]
    fn decrypt_recovers_inner_reply_with_matching_tag() {
        let (key, tag) = test_key_tag();
        let reply = sample_reply_payload();
        let opaque = build_garlic_opaque(&key, &tag, 0x51A7_5001, &reply);
        let decrypted = decrypt_build_reply_garlic(&key, &tag, &opaque).expect("decrypt");
        assert_eq!(decrypted.inner_message_id, 0x51A7_5001);
        assert_eq!(decrypted.reply_payload, reply);
    }

    #[test]
    fn tag_mismatch_is_typed_without_decrypt() {
        let (key, tag) = test_key_tag();
        let reply = sample_reply_payload();
        let opaque = build_garlic_opaque(&key, &tag, 1, &reply);
        let mut wrong_tag = tag;
        wrong_tag[0] ^= 0xFF;
        assert!(matches!(
            decrypt_build_reply_garlic(&key, &wrong_tag, &opaque),
            Err(GarlicReplyError::TagMismatch)
        ));
    }

    #[test]
    fn tampered_ciphertext_fails_authentication() {
        let (key, tag) = test_key_tag();
        let reply = sample_reply_payload();
        let mut opaque = build_garlic_opaque(&key, &tag, 1, &reply);
        let last = opaque.len() - 1;
        opaque[last] ^= 0x01;
        assert!(matches!(
            decrypt_build_reply_garlic(&key, &tag, &opaque),
            Err(GarlicReplyError::AuthenticationFailed)
        ));
    }

    #[test]
    fn too_short_opaque_is_rejected() {
        let (key, tag) = test_key_tag();
        assert!(matches!(
            decrypt_build_reply_garlic(&key, &tag, &[0_u8; 10]),
            Err(GarlicReplyError::TooShort)
        ));
    }

    /// Test-only initiator mirror of the reference
    /// `WrapECIESX25519MessageForRouter` (Noise-N, `datetime=true`):
    /// ephemeral key, transcript `h0/MixHash(recipient_pub)/
    /// MixHash(eph_pub)`, `MixKey(DH(eph_priv, recipient_pub))`,
    /// AEAD with AD = h and nonce zero over DateTime + clove +
    /// padding. Layout: `eph(32) || ct || poly(16)`.
    fn wrap_for_router(
        recipient_pub: &[u8; 32],
        inner_type: u8,
        inner_msgid: u32,
        inner_exp_sec: u32,
        inner_payload: &[u8],
        eph_priv: &[u8; 32],
    ) -> Vec<u8> {
        use chacha20poly1305::aead::Aead as _;
        let eph_secret = StaticSecret::from(*eph_priv);
        let eph_pub = X25519PublicKey::from(&eph_secret);
        let mut h0 = [0_u8; 32];
        h0[..NOISE_PROTOCOL_NAME.len()].copy_from_slice(NOISE_PROTOCOL_NAME);
        let mut hasher = Sha256::new();
        hasher.update(h0);
        let null_h = hasher.finalize();
        let mut hasher = Sha256::new();
        hasher.update(null_h);
        hasher.update(recipient_pub);
        let mut h = hasher.finalize().to_vec();
        let mut hasher = Sha256::new();
        hasher.update(&h);
        hasher.update(eph_pub.as_bytes());
        h = hasher.finalize().to_vec();
        let peer = X25519PublicKey::from(*recipient_pub);
        let shared = eph_secret.diffie_hellman(&peer);
        let keydata = i2pr_crypto::hkdf_sha256_extract_and_expand(&h0, shared.as_bytes(), &[], 64)
            .expect("hkdf");
        let mut aead_key = [0_u8; 32];
        aead_key.copy_from_slice(&keydata[32..64]);
        // DateTime block + clove + fixed padding, mirroring the
        // reference `CreateGarlicPayload(datetime=true)`.
        let mut clove_body = Vec::new();
        clove_body.push(0_u8);
        clove_body.push(inner_type);
        clove_body.extend_from_slice(&inner_msgid.to_be_bytes());
        clove_body.extend_from_slice(&inner_exp_sec.to_be_bytes());
        clove_body.extend_from_slice(inner_payload);
        let mut plaintext = vec![0_u8, 0, 4, 0, 0, 0, 60];
        plaintext.push(GARLIC_CLOVE_BLOCK_TYPE);
        plaintext.extend_from_slice(
            &u16::try_from(clove_body.len())
                .expect("clove len")
                .to_be_bytes(),
        );
        plaintext.extend_from_slice(&clove_body);
        plaintext.push(254_u8);
        plaintext.extend_from_slice(&8_u16.to_be_bytes());
        plaintext.extend_from_slice(&[0_u8; 8]);
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&aead_key));
        let nonce = Nonce::from_slice(&[0_u8; 12]);
        let ciphertext = cipher
            .encrypt(
                nonce,
                chacha20poly1305::aead::Payload {
                    msg: &plaintext,
                    aad: &h,
                },
            )
            .expect("encrypt");
        let mut opaque = Vec::with_capacity(32 + ciphertext.len());
        opaque.extend_from_slice(eph_pub.as_bytes());
        opaque.extend_from_slice(&ciphertext);
        opaque
    }

    fn router_keypair() -> ([u8; 32], [u8; 32], [u8; 32]) {
        // Returns (responder_priv, recipient_pub, eph_priv) from a
        // fixed seed so tests stay deterministic.
        let mut rng = ChaCha8Rng::seed_from_u64(0x904712);
        let mut responder_priv = [0_u8; 32];
        let mut eph_priv = [0_u8; 32];
        rng.fill_bytes(&mut responder_priv);
        rng.fill_bytes(&mut eph_priv);
        let publ = X25519PublicKey::from(&StaticSecret::from(responder_priv));
        (responder_priv, *publ.as_bytes(), eph_priv)
    }

    #[test]
    fn router_garlic_round_trips_stbm_clove() {
        let (responder_priv, recipient_pub, eph_priv) = router_keypair();
        let mut body = vec![4_u8];
        body.extend(std::iter::repeat_n(0x42_u8, 4 * 218));
        let opaque = wrap_for_router(
            &recipient_pub,
            SHORT_TUNNEL_BUILD_TYPE,
            0x77AA,
            1_800_000_060,
            &body,
            &eph_priv,
        );
        let plaintext = open_router_garlic(&opaque, &responder_priv).expect("open");
        let clove = extract_garlic_clove(&plaintext).expect("clove");
        assert_eq!(clove.inner_type, SHORT_TUNNEL_BUILD_TYPE);
        assert_eq!(clove.message_id, 0x77AA);
        assert_eq!(clove.expiration_seconds, 1_800_000_060);
        assert_eq!(clove.payload, body);
    }

    #[test]
    fn router_garlic_wrong_key_fails_authentication() {
        let (responder_priv, recipient_pub, eph_priv) = router_keypair();
        let body = vec![1_u8, 0x42, 0x43];
        let opaque = wrap_for_router(
            &recipient_pub,
            SHORT_TUNNEL_BUILD_TYPE,
            1,
            2,
            &body,
            &eph_priv,
        );
        let mut wrong = responder_priv;
        // Flip a bit the X25519 clamp preserves (clamping clears
        // the low bits of byte 0 and sets the high bit of byte
        // 31, so a bit-0 flip would silently round-trip).
        wrong[16] ^= 0x40;
        assert!(matches!(
            open_router_garlic(&opaque, &wrong),
            Err(GarlicReplyError::AuthenticationFailed)
        ));
        let mut tampered = opaque.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        assert!(matches!(
            open_router_garlic(&tampered, &responder_priv),
            Err(GarlicReplyError::AuthenticationFailed)
        ));
    }

    #[test]
    fn router_garlic_rejects_short_and_oversize() {
        let responder_priv = [0x11_u8; 32];
        assert!(matches!(
            open_router_garlic(&[0_u8; 10], &responder_priv),
            Err(GarlicReplyError::TooShort)
        ));
        assert!(matches!(
            open_router_garlic(&vec![0_u8; MAX_ROUTER_GARLIC_OPAQUE + 1], &responder_priv),
            Err(GarlicReplyError::TooLarge)
        ));
    }

    #[test]
    fn wrap_round_trips_through_decrypt() {
        use i2pr_proto::{Date, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE};
        let (key, tag) = test_key_tag();
        let reply = sample_reply_payload();
        let bytes = wrap_obep_reply_garlic(
            &reply,
            0xA11CE,
            1_800_000_060,
            0xBEEF,
            Date::from_millis(1_800_000_000_000),
            &key,
            &tag,
        )
        .expect("wrap");
        let decoded =
            I2npMessage::decode_standard(&bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode garlic");
        assert_eq!(
            decoded.header().message_type(),
            i2pr_proto::MessageType::Garlic
        );
        let I2npBody::Garlic(opaque) = decoded.body() else {
            panic!("expected garlic body");
        };
        let decrypted =
            decrypt_build_reply_garlic(&key, &tag, opaque.payload.as_bytes()).expect("decrypt");
        assert_eq!(decrypted.inner_message_id, 0xA11CE);
        assert_eq!(decrypted.inner_expiration_seconds, 1_800_000_060);
        assert_eq!(decrypted.reply_payload, reply);
    }

    #[test]
    fn wrap_rejects_oversize_and_zero_msgid() {
        use i2pr_proto::Date;
        let (key, tag) = test_key_tag();
        let big = vec![0xAA_u8; MAX_RELAY_OTBRM_BODY + 1];
        assert!(matches!(
            wrap_obep_reply_garlic(
                &big,
                1,
                1_800_000_060,
                2,
                Date::from_millis(1_800_000_000_000),
                &key,
                &tag
            ),
            Err(GarlicReplyError::TooLarge)
        ));
        let reply = sample_reply_payload();
        assert!(matches!(
            wrap_obep_reply_garlic(
                &reply,
                0,
                1_800_000_060,
                2,
                Date::from_millis(1_800_000_000_000),
                &key,
                &tag
            ),
            Err(GarlicReplyError::InvalidReplyPayload)
        ));
    }

    #[test]
    fn wrong_inner_type_is_rejected() {
        let (key, tag) = test_key_tag();
        let mut clove_body = vec![0_u8, 25_u8, 0, 0, 0, 1, 0, 0, 0, 0, 1_u8];
        clove_body.extend(std::iter::repeat_n(0x11_u8, 218));
        let mut decrypted = vec![GARLIC_CLOVE_BLOCK_TYPE];
        decrypted.extend_from_slice(&(clove_body.len() as u16).to_be_bytes());
        decrypted.extend_from_slice(&clove_body);
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
        let nonce = Nonce::from_slice(&[0_u8; 12]);
        let ciphertext = cipher
            .encrypt(
                nonce,
                chacha20poly1305::aead::Payload {
                    msg: &decrypted,
                    aad: &tag,
                },
            )
            .expect("encrypt");
        let mut opaque = Vec::new();
        opaque.extend_from_slice(&tag);
        opaque.extend_from_slice(&ciphertext);
        assert!(matches!(
            decrypt_build_reply_garlic(&key, &tag, &opaque),
            Err(GarlicReplyError::UnexpectedInnerType)
        ));
    }
}
