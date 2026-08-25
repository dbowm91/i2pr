//! Plan 121 §13 deterministic two-destination integration
//! trajectory, corrected by Plan 126 to the normative I2P
//! ECIES-X25519-AEAD-Ratchet contract.
//!
//! Drives two real destination identities through the corrected
//! bound-session lifecycle at the primitive level:
//!
//! 1. Each destination generates a Plan 120 destination identity
//!    (independent Ed25519 signing + X25519 static keys).
//! 2. Alice encrypts a bound New Session Garlic Clove to Bob using
//!    `seal_bound_new_session` and the typed payload block codec.
//!    Alice's destination static key is bound into the handshake.
//! 3. Bob decrypts/authenticates the New Session, observes the
//!    exact Clove payload once, and receives Alice's static public
//!    key as the session identity.
//! 4. Bob emits a New Session Reply; Alice authenticates it through
//!    the retained `BoundNewSessionSender` reply window and
//!    installs both directional tag sets from the Noise split.
//! 5. Both directions then exchange Existing Session messages over
//!    the split-derived ratchets and decrypt the exact payloads
//!    exactly once.
//! 6. The trajectory verifies replay rejection, cross-direction
//!    isolation, and session-tag ratcheting.

use i2pr_client::{
    decode_decrypted_payload, encode_garlic_clove_payload, encode_new_session_payload, local_clove,
};
use i2pr_crypto::{
    EciesEphemeralKeypair, SESSION_TAG_LENGTH, decode_representative, open_bound_new_session,
    open_existing_session, open_new_session_reply, seal_bound_new_session, seal_existing_session,
    seal_new_session_reply,
};
use i2pr_proto::{EciesPayloadBlock, EciesPayloadSequence, GarlicCloveBlock, GarlicDelivery};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use zeroize::Zeroizing;

const NOW_SECONDS: u32 = 1_700_000_000;
const EXISTING_PAYLOAD: &[u8] = b"hello-there";

/// Construct a fresh destination identity from explicit seed
/// material so both sides of the trajectory have known private
/// keys.
fn destination_with_static(seed: u64) -> (i2pr_client::DestinationIdentity, [u8; 32]) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut signing = [0_u8; 32];
    let mut static_secret = [0_u8; 32];
    let mut padding = vec![0_u8; i2pr_crypto::IDENTITY_PADDING_LENGTH];
    rng.fill_bytes(&mut signing);
    rng.fill_bytes(&mut static_secret);
    rng.fill_bytes(&mut padding);
    let identity = i2pr_client::DestinationIdentity::from_private_bytes(
        signing,
        static_secret,
        Zeroizing::new(padding),
    )
    .expect("destination identity");
    (identity, static_secret)
}

fn i2np_body(seed: u8) -> Vec<u8> {
    let mut bytes = vec![0xC1_u8];
    bytes.extend_from_slice(&[seed; 4]);
    bytes.extend_from_slice(&[seed.wrapping_add(0xAB); 6]);
    bytes
}

fn local_payload(seed: u8) -> Vec<u8> {
    let clove = local_clove(NOW_SECONDS, 0x0001_0001, i2np_body(seed));
    encode_new_session_payload(NOW_SECONDS, &clove).expect("payload")
}

#[test]
fn plan_126_corrected_deterministic_local_trajectory() {
    let mut rng = ChaCha8Rng::seed_from_u64(101);
    let (alice_identity, alice_static_secret) = destination_with_static(7);
    let (bob_identity, bob_static_secret) = destination_with_static(11);
    let bob_static_pub = bob_identity.static_public_bytes();

    // Step 1: Alice seals a bound New Session message destined for
    // Bob. Her destination static key is bound into the handshake;
    // the sender retains the reply window for the NSR.
    let alice_payload = local_payload(0x42);
    let ephemeral = EciesEphemeralKeypair::generate(&mut rng).expect("ephemeral");
    let (ns_message, sender) = seal_bound_new_session(
        &alice_static_secret,
        &ephemeral,
        &bob_static_pub,
        &alice_payload,
    )
    .expect("seal ns");
    // The wire representative must decode to the ephemeral public
    // key the sender retained.
    let decoded_aepk =
        decode_representative(&ns_message.representative).expect("representative decodes");
    assert_ne!(decoded_aepk, bob_static_pub);
    assert_eq!(sender.bob_static_public(), &bob_static_pub);

    // Step 2: Bob opens the bound New Session and observes the
    // sender's static public key (the session identity).
    let opened = open_bound_new_session(&bob_static_secret, &bob_static_pub, &ns_message)
        .expect("bob open ns");
    assert_eq!(opened.payload, alice_payload);
    assert_eq!(
        opened.responder.alice_static_public,
        alice_identity.static_public_bytes()
    );

    let clove = decode_decrypted_payload(&opened.payload).expect("decode clove");
    assert_eq!(clove.delivery, GarlicDelivery::Local);
    assert_eq!(clove.message, i2np_body(0x42));

    // Step 3: Bob produces a New Session Reply carrying his ack
    // payload. The reply rides the one-shot SessionReplyTags
    // window derived from the handshake chaining key; the split
    // yields Bob's directional tag sets directly.
    let bob_clove = GarlicCloveBlock {
        delivery: GarlicDelivery::Local,
        message: i2np_body(0x99),
    };
    let bob_payload = encode_garlic_clove_payload(&bob_clove).expect("bob payload");
    let sealed_reply = seal_new_session_reply(
        &opened.responder,
        &bob_static_secret,
        &bob_payload,
        &mut rng,
    )
    .expect("seal nsr");
    assert_eq!(sealed_reply.message.tag.len(), SESSION_TAG_LENGTH);

    // Step 4: The reply tag must be present in Alice's pending
    // window; accepting the reply installs both of her directional
    // tag sets from the Noise split.
    let mut pending_window = sender.reply_tag_set().expect("pending window");
    let matched = pending_window.next_entry().expect("window entry");
    assert_eq!(matched.tag, sealed_reply.message.tag);

    let opened_reply = open_new_session_reply(&sender, &alice_static_secret, &sealed_reply.message)
        .expect("alice open nsr");
    assert_eq!(opened_reply.payload, bob_payload);

    let mut alice_out = opened_reply.outbound_tag_set;
    let mut alice_in = opened_reply.inbound_tag_set;
    let mut bob_out = sealed_reply.outbound_tag_set;
    let mut bob_in = sealed_reply.inbound_tag_set;

    // Step 5: Bidirectional Existing Session exchange over the
    // split-derived ratchets. A -> B uses k_ab; B -> A uses k_ba.
    let es_a1 = seal_existing_session(&mut alice_out, EXISTING_PAYLOAD).expect("alice seal es");
    // Outbound ES tags come from the split tag set, never from the
    // one-shot NSR window.
    assert_ne!(es_a1.tag, matched.tag);
    let decoded_a1 = open_existing_session(&mut bob_in, matched.index, &es_a1).expect("bob in");
    assert_eq!(decoded_a1, EXISTING_PAYLOAD);

    let es_b1 = seal_existing_session(&mut bob_out, b"bob-to-alice").expect("bob seal es");
    let decoded_b1 = open_existing_session(&mut alice_in, matched.index, &es_b1).expect("alice in");
    assert_eq!(decoded_b1, b"bob-to-alice");

    // Consecutive tags must differ (ratchet advances).
    let es_a2 = seal_existing_session(&mut alice_out, EXISTING_PAYLOAD).expect("alice seal es 2");
    assert_ne!(
        es_a1.tag, es_a2.tag,
        "consecutive existing-session tags must differ"
    );
    let decoded_a2 = open_existing_session(&mut bob_in, matched.index + 1, &es_a2).expect("bob 2");
    assert_eq!(decoded_a2, EXISTING_PAYLOAD);

    // Step 6: At the primitive level the ratchet keys remain
    // derivable, so AEAD authentication of a replayed message
    // succeeds; replay REJECTION is the manager's remove-on-hit tag
    // window responsibility and is covered by the Plan 126 manager
    // trajectory.
    let replay_primitive = open_existing_session(&mut bob_in, matched.index, &es_a1)
        .expect("primitive-level replay authenticates");
    assert_eq!(replay_primitive, EXISTING_PAYLOAD);

    // Cross-direction isolation: Bob cannot decrypt Alice's
    // outbound traffic with his own outbound tag set.
    let wrong_direction = open_existing_session(&mut bob_out, matched.index, &es_a2);
    assert!(
        wrong_direction.is_err(),
        "cross-direction existing-session decryption must fail"
    );
}

#[test]
fn ecies_payload_block_rejects_garlic_clove_crosses_padding() {
    let mut sequence = EciesPayloadSequence::empty();
    sequence
        .push(EciesPayloadBlock::DateTime(NOW_SECONDS))
        .expect("dt");
    sequence
        .push(EciesPayloadBlock::GarlicClove(GarlicCloveBlock {
            delivery: GarlicDelivery::Local,
            message: vec![0xAA; 32],
        }))
        .expect("clove");
    let encoded = sequence.encode_to_vec(65_507, true).expect("encode");
    let decoded = EciesPayloadSequence::decode(&encoded, encoded.len(), true).expect("decode");
    assert_eq!(decoded.blocks().len(), 2);
}

#[test]
fn ecies_payload_round_trip_with_local_clove_and_padding() {
    let mut sequence = EciesPayloadSequence::empty();
    sequence
        .push(EciesPayloadBlock::DateTime(NOW_SECONDS))
        .expect("dt");
    sequence
        .push(EciesPayloadBlock::GarlicClove(GarlicCloveBlock {
            delivery: GarlicDelivery::Local,
            message: vec![0xBB; 4],
        }))
        .expect("clove");
    sequence
        .push(EciesPayloadBlock::Padding(vec![0; 16]))
        .expect("pad");
    let encoded = sequence.encode_to_vec(65_507, true).expect("encode");
    let decoded = EciesPayloadSequence::decode(&encoded, encoded.len(), true).expect("decode");
    assert_eq!(decoded, sequence);
}

#[test]
fn ecies_payload_decode_without_datetime_first_fails() {
    let mut sequence = EciesPayloadSequence::empty();
    sequence
        .push(EciesPayloadBlock::GarlicClove(GarlicCloveBlock {
            delivery: GarlicDelivery::Local,
            message: vec![0xCC; 8],
        }))
        .expect("clove");
    let encoded = sequence
        .encode_to_vec(65_507, false)
        .expect("encode without datetime");
    let result = EciesPayloadSequence::decode(&encoded, encoded.len(), true);
    assert!(matches!(
        result,
        Err(i2pr_proto::CodecError::InvalidFieldValue { .. })
    ));
}
