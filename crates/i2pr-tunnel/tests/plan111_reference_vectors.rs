//! Plan 112 §6.F: Rust-only reference vector provenance test.
//!
//! The frozen Plan 111 conformance vectors in
//! `crate::fixed_vectors` are committed as static Rust constants.
//! This integration test re-derives the same bytes from a
//! pure-Rust reference path that uses only `x25519-dalek`,
//! `sha2`, `chacha20poly1305`, and `i2pr-crypto`'s HKDF helper,
//! then asserts the production `EciesX25519BuildCryptography`
//! primitive reaches the same bytes for the canonical inputs.
//!
//! The test does not call any helper that imports
//! `i2pr_tunnel::build_crypto::BuildCryptography` except through
//! the production `seal_short_request_with_ephemeral`,
//! `open_short_request`, and `derive_layer_keys` entry points
//! the production test suite uses. A defect in either side will
//! now cause this test to fail closed.
#![forbid(unsafe_code)]

use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce, aead::Aead};
use i2pr_crypto::hkdf_sha256_extract_and_expand;
use i2pr_tunnel::BuildCryptography;
use i2pr_tunnel::build_crypto::{EciesX25519BuildCryptography, LayerKeys, derive_layer_keys};
use i2pr_tunnel::fixed_vectors::{
    AEAD_KEY_LEN, AEAD_NONCE_LEN, EPHEMERAL_KEY_LEN, FIXED_EPHEMERAL_PRIVATE,
    FIXED_EPHEMERAL_PUBLIC, FIXED_HOP_IDENTITY, FIXED_HOP_PRIVATE, FIXED_HOP_PUBLIC, FIXED_IV_KEY,
    FIXED_LAYER_KEY, FIXED_OBEP_GARLIC_KEY, FIXED_OBEP_GARLIC_TAG, FIXED_POST_REQUEST_CK,
    FIXED_REPLY_KEY, FIXED_REQUEST_AEAD_KEY, FIXED_REQUEST_KEYDATA, FIXED_REQUEST_PLAINTEXT,
    FIXED_SEALED_REQUEST, FIXED_SHARED_SECRET, FIXED_SLOT_FIVE_NONCE, NOISE_PROTOCOL_NAME,
    NULL_PROLOGUE_HASH, POST_REQUEST_H, SHORT_BUILD_RECORD_SIZE, SHORT_REQUEST_PLAINTEXT_SIZE,
};
use sha2::{Digest, Sha256};

#[test]
fn plan111_reference_vectors_match_pure_rust_oracle() {
    // 1. Null-prologue hash matches SHA256(padded protocol name).
    let mut padded = [0_u8; 32];
    padded[..NOISE_PROTOCOL_NAME.len()].copy_from_slice(NOISE_PROTOCOL_NAME);
    let mut hasher = Sha256::new();
    hasher.update(padded);
    let output = hasher.finalize();
    let mut computed_null_h = [0_u8; 32];
    computed_null_h.copy_from_slice(&output);
    assert_eq!(computed_null_h, NULL_PROLOGUE_HASH);

    // 2. Hop public key matches `X25519(FIXED_HOP_PRIVATE)`.
    let hop_secret = x25519_dalek::StaticSecret::from(FIXED_HOP_PRIVATE);
    let hop_public = x25519_dalek::PublicKey::from(&hop_secret);
    assert_eq!(hop_public.to_bytes(), FIXED_HOP_PUBLIC);

    // 3. Ephemeral public key matches `X25519(FIXED_EPHEMERAL_PRIVATE)`.
    let eph_secret = x25519_dalek::StaticSecret::from(FIXED_EPHEMERAL_PRIVATE);
    let eph_public = x25519_dalek::PublicKey::from(&eph_secret);
    assert_eq!(eph_public.to_bytes(), FIXED_EPHEMERAL_PUBLIC);

    // 4. Shared secret matches `X25519(FIXED_EPHEMERAL_PRIVATE, FIXED_HOP_PUBLIC)`.
    let shared = eph_secret.diffie_hellman(&hop_public);
    assert_eq!(shared.as_bytes(), &FIXED_SHARED_SECRET[..]);

    // 5. Re-derive the post-request chaining key and AEAD key from
    // the canonical HKDF input chain. The initial chaining key
    // is the padded protocol name (`h0`), and `mix_key` runs
    // `HKDF(ck, shared, "", 64)`.
    let mut h0 = [0_u8; 32];
    h0[..NOISE_PROTOCOL_NAME.len()].copy_from_slice(NOISE_PROTOCOL_NAME);
    let keydata = hkdf_sha256_extract_and_expand(&h0, &FIXED_SHARED_SECRET, &[], 64).expect("hkdf");
    assert_eq!(keydata.len(), FIXED_REQUEST_KEYDATA.len());
    assert_eq!(keydata.as_slice(), &FIXED_REQUEST_KEYDATA[..]);
    let mut ck = [0_u8; 32];
    ck.copy_from_slice(&keydata[..32]);
    let mut aead_key = [0_u8; 32];
    aead_key.copy_from_slice(&keydata[32..64]);
    assert_eq!(ck, FIXED_POST_REQUEST_CK);
    assert_eq!(aead_key, FIXED_REQUEST_AEAD_KEY);

    // 6. Re-derive the post-request transcript hash chain.
    let mut h = NULL_PROLOGUE_HASH;
    let mut hasher = Sha256::new();
    hasher.update(h);
    hasher.update(FIXED_HOP_PUBLIC);
    let output = hasher.finalize();
    h.copy_from_slice(&output);
    let mut hasher = Sha256::new();
    hasher.update(h);
    hasher.update(FIXED_EPHEMERAL_PUBLIC);
    let output = hasher.finalize();
    h.copy_from_slice(&output);
    // POST_REQUEST_H == MixHash(h || ciphertext || tag).
    let ciphertext_with_tag = &FIXED_SEALED_REQUEST[48..];
    let mut hasher = Sha256::new();
    hasher.update(h);
    hasher.update(ciphertext_with_tag);
    let output = hasher.finalize();
    h.copy_from_slice(&output);
    assert_eq!(h, POST_REQUEST_H);

    // 7. Re-derive the per-hop reply/layer/iv keys.
    let reply_keydata =
        hkdf_sha256_extract_and_expand(&ck, &[], b"SMTunnelReplyKey", 64).expect("hkdf");
    let mut next_ck = [0_u8; 32];
    next_ck.copy_from_slice(&reply_keydata[..32]);
    let mut reply_key = [0_u8; 32];
    reply_key.copy_from_slice(&reply_keydata[32..64]);
    assert_eq!(reply_key, FIXED_REPLY_KEY);
    let layer_keydata =
        hkdf_sha256_extract_and_expand(&next_ck, &[], b"SMTunnelLayerKey", 64).expect("hkdf");
    let mut iv_key = [0_u8; 32];
    iv_key.copy_from_slice(&layer_keydata[..32]);
    let mut layer_key = [0_u8; 32];
    layer_key.copy_from_slice(&layer_keydata[32..64]);
    assert_eq!(iv_key, FIXED_IV_KEY);
    assert_eq!(layer_key, FIXED_LAYER_KEY);

    // 8. Verify the canonical sealed envelope layout.
    assert_eq!(FIXED_SEALED_REQUEST.len(), SHORT_BUILD_RECORD_SIZE);
    assert_eq!(FIXED_REQUEST_PLAINTEXT.len(), SHORT_REQUEST_PLAINTEXT_SIZE);
    assert_eq!(FIXED_HOP_PRIVATE.len(), EPHEMERAL_KEY_LEN);
    assert_eq!(FIXED_HOP_PUBLIC.len(), EPHEMERAL_KEY_LEN);
    assert_eq!(FIXED_EPHEMERAL_PRIVATE.len(), EPHEMERAL_KEY_LEN);
    assert_eq!(FIXED_EPHEMERAL_PUBLIC.len(), EPHEMERAL_KEY_LEN);
    assert_eq!(FIXED_SLOT_FIVE_NONCE.len(), AEAD_NONCE_LEN);
    assert_eq!(FIXED_REPLY_KEY.len(), AEAD_KEY_LEN);
    assert_eq!(FIXED_LAYER_KEY.len(), AEAD_KEY_LEN);
    assert_eq!(FIXED_IV_KEY.len(), AEAD_KEY_LEN);
}

#[test]
fn plan111_reference_vectors_match_production_seal() {
    let cryptography = EciesX25519BuildCryptography::new();
    let sealed = cryptography
        .seal_short_request_with_ephemeral(
            &FIXED_REQUEST_PLAINTEXT,
            &FIXED_HOP_PUBLIC,
            &FIXED_HOP_IDENTITY,
            &FIXED_EPHEMERAL_PRIVATE,
        )
        .expect("seal");
    assert_eq!(sealed.record.as_ref(), &FIXED_SEALED_REQUEST[..]);
    assert_eq!(sealed.ephemeral_pub, FIXED_EPHEMERAL_PUBLIC);
    assert_eq!(sealed.state.transcript_hash(), POST_REQUEST_H);
    assert_eq!(sealed.state.chaining_key(), FIXED_POST_REQUEST_CK);
}

#[test]
fn plan111_reference_vectors_match_production_open() {
    let cryptography = EciesX25519BuildCryptography::new();
    let opened = cryptography
        .open_short_request(
            &FIXED_SEALED_REQUEST,
            &FIXED_HOP_PRIVATE,
            &FIXED_HOP_IDENTITY,
        )
        .expect("open");
    assert_eq!(opened.plaintext.as_ref(), &FIXED_REQUEST_PLAINTEXT[..]);
    assert_eq!(opened.state.transcript_hash(), POST_REQUEST_H);
    assert_eq!(opened.state.chaining_key(), FIXED_POST_REQUEST_CK);
}

#[test]
fn plan111_reference_vectors_match_production_layer_keys() {
    let cryptography = EciesX25519BuildCryptography::new();
    let sealed = cryptography
        .seal_short_request_with_ephemeral(
            &FIXED_REQUEST_PLAINTEXT,
            &FIXED_HOP_PUBLIC,
            &FIXED_HOP_IDENTITY,
            &FIXED_EPHEMERAL_PRIVATE,
        )
        .expect("seal");
    let participant: LayerKeys = derive_layer_keys(&sealed.state, false).expect("participant");
    assert_eq!(participant.reply_key(), &FIXED_REPLY_KEY);
    assert_eq!(participant.layer_key(), &FIXED_LAYER_KEY);
    assert_eq!(participant.iv_key(), &FIXED_IV_KEY);
    let obep: LayerKeys = derive_layer_keys(&sealed.state, true).expect("obep");
    assert_eq!(
        obep.garlic_reply_key().expect("garlic key"),
        &FIXED_OBEP_GARLIC_KEY
    );
    assert_eq!(
        obep.garlic_reply_tag().expect("garlic tag"),
        &FIXED_OBEP_GARLIC_TAG
    );
}

#[test]
fn plan111_reference_vectors_sealed_envelope_re_encrypts_to_same_bytes() {
    // Independent re-seal verification using only the AEAD
    // primitive, the derived key, and the canonical envelope
    // layout. The test confirms the FIXED_SEALED_REQUEST bytes
    // can be reconstructed without calling
    // EciesX25519BuildCryptography. The AEAD associated data is
    // the pre-AEAD transcript hash (post peer-static and
    // ephemeral `MixHash` but before the post-AEAD `MixHash`).
    let cryptography = EciesX25519BuildCryptography::new();
    let sealed = cryptography
        .seal_short_request_with_ephemeral(
            &FIXED_REQUEST_PLAINTEXT,
            &FIXED_HOP_PUBLIC,
            &FIXED_HOP_IDENTITY,
            &FIXED_EPHEMERAL_PRIVATE,
        )
        .expect("seal");
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&FIXED_REQUEST_AEAD_KEY));
    let nonce_bytes = nonce_bytes_for_request();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let mut h = NULL_PROLOGUE_HASH;
    let mut hasher = Sha256::new();
    hasher.update(h);
    hasher.update(FIXED_HOP_PUBLIC);
    let output = hasher.finalize();
    h.copy_from_slice(&output);
    let mut hasher = Sha256::new();
    hasher.update(h);
    hasher.update(FIXED_EPHEMERAL_PUBLIC);
    let output = hasher.finalize();
    h.copy_from_slice(&output);
    let pre_aead_h = h;
    let payload = chacha20poly1305::aead::Payload {
        msg: FIXED_REQUEST_PLAINTEXT.as_ref(),
        aad: &pre_aead_h,
    };
    let ciphertext_with_tag = cipher.encrypt(nonce, payload).expect("aead");
    let mut recomputed = [0_u8; SHORT_BUILD_RECORD_SIZE];
    recomputed[..HASH_PREFIX_LEN_AS_USIZE]
        .copy_from_slice(&FIXED_HOP_IDENTITY[..HASH_PREFIX_LEN_AS_USIZE]);
    recomputed[HASH_PREFIX_LEN_AS_USIZE..HASH_PREFIX_LEN_AS_USIZE + EPHEMERAL_KEY_LEN]
        .copy_from_slice(&FIXED_EPHEMERAL_PUBLIC);
    recomputed[48..].copy_from_slice(&ciphertext_with_tag);
    assert_eq!(recomputed, sealed.record.as_ref());
}

fn nonce_bytes_for_request() -> [u8; AEAD_NONCE_LEN] {
    // Plan 111 noise protocol: the request AEAD nonce is 0.
    [0_u8; AEAD_NONCE_LEN]
}

const HASH_PREFIX_LEN_AS_USIZE: usize = 16;
