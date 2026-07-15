#![no_main]

use i2pr_crypto::X25519SharedSecret;
use i2pr_transport_ntcp2::crypto::{Ntcp2CryptoError, PublicKeyBytes, Role, Transcript};
use libfuzzer_sys::fuzz_target;

const SHARED: X25519SharedSecret = X25519SharedSecret::from_bytes([0x11; 32]);
const RESPONDER_STATIC: PublicKeyBytes = PublicKeyBytes::from_bytes_for_test([0x42; 32]);
const INITIATOR_STATIC: PublicKeyBytes = PublicKeyBytes::from_bytes_for_test([0x24; 32]);
const EPHEMERAL_X: PublicKeyBytes = PublicKeyBytes::from_bytes_for_test([0x13; 32]);
const EPHEMERAL_Y: PublicKeyBytes = PublicKeyBytes::from_bytes_for_test([0x31; 32]);

fn valid_transcript(input: &[u8]) -> Result<(Transcript, Transcript, Vec<u8>), Ntcp2CryptoError> {
    let request_options = &input[..input.len().min(16)];
    let created_options = &input[input.len().min(16)..input.len().min(32)];
    let confirmed_payload = &input[input.len().min(32)..input.len().min(64)];

    let (alice, request) = Transcript::new(Role::Initiator, RESPONDER_STATIC)
        .session_request(EPHEMERAL_X, SHARED, request_options)?;
    let (bob, _) = Transcript::new(Role::Responder, RESPONDER_STATIC)
        .accept_session_request(EPHEMERAL_X, SHARED, &request)?;
    let alice = alice.mix_padding(&[])?;
    let bob = bob.mix_padding(&[])?;
    let (bob, created) = bob.session_created(EPHEMERAL_Y, SHARED, created_options)?;
    let (alice, _) = alice.accept_session_created(EPHEMERAL_Y, SHARED, &created)?;
    let alice = alice.mix_padding(&[])?;
    let bob = bob.mix_padding(&[])?;
    let (alice, static_ciphertext) = alice.encrypt_static(INITIATOR_STATIC, SHARED)?;
    let (bob, _) = bob.decrypt_static(INITIATOR_STATIC, SHARED, &static_ciphertext)?;
    let (alice, confirmed) = alice.encrypt_confirmed_payload(confirmed_payload)?;
    let (bob, _) = bob.decrypt_confirmed_payload(&confirmed)?;
    Ok((alice, bob, confirmed))
}

fuzz_target!(|input: &[u8]| {
    match input.first().copied().unwrap_or_default() % 4 {
        0 => {
            let _ = valid_transcript(input);
        }
        1 => {
            let _ = Transcript::new(Role::Responder, RESPONDER_STATIC).session_request(
                EPHEMERAL_X,
                SHARED,
                &[],
            );
        }
        2 => {
            if let Ok((alice, _, _)) = valid_transcript(input) {
                let _ = alice.mix_padding(&[]);
            }
        }
        _ => {
            if let Ok((_, bob, mut confirmed)) = valid_transcript(input) {
                if let Some(byte) = confirmed.first_mut() {
                    *byte ^= 1;
                }
                let _ = bob.decrypt_confirmed_payload(&confirmed);
            }
        }
    }
});
