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
use thiserror::Error;
use zeroize::Zeroizing;

use crate::build_crypto::GARLIC_REPLY_TAG_LEN;

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
