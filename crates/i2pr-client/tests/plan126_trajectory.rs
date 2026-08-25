//! Plan 126 manager-level deterministic trajectory.
//!
//! Exercises the corrected ECIES-X25519-AEAD-Ratchet destination
//! session manager end to end at the manager boundary:
//!
//! 1. Alice's manager encrypts a bound New Session payload to Bob.
//! 2. Bob's manager classifies and accepts the bound New Session;
//!    the provisional responder is installed under Alice's static
//!    public key.
//! 3. Bob seals the New Session Reply through the manager; the
//!    paired session is promoted on both sides.
//! 4. Both directions exchange Existing Session messages through
//!    `classify` + `accept_existing_session` with exact-once
//!    delivery.
//! 5. Negative controls: ES replay rejection, unknown-tag
//!    rejection, NSR-after-acceptance rejection, duplicate bound
//!    New Session rejection, cross-manager isolation, capacity
//!    ceilings, and idle expiry.

#![allow(clippy::too_many_lines)]

use i2pr_client::{
    ClassifiedInbound, ClassifiedUnknown, DestinationId, EciesSessionConfig, EciesSessionError,
    EciesSessionManager, decode_decrypted_payload, encode_garlic_clove_payload,
};
use i2pr_crypto::{BoundNewSessionMessage, ExistingSessionMessage, NewSessionReplyMessage};
use i2pr_proto::{
    DeferredPayload, GarlicCloveBlock, GarlicDelivery, Hash, I2npBody, I2npMessage,
    MAX_I2NP_PAYLOAD_SIZE, OpaqueMessageBody,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use zeroize::Zeroizing;

const NOW_SECONDS: u32 = 1_000;

/// One destination identity split into its manager-facing parts.
struct TestIdentity {
    id: DestinationId,
    public: [u8; 32],
    secret: [u8; 32],
}

fn identity_with_static(seed: u64) -> TestIdentity {
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
    TestIdentity {
        id: identity.id(),
        public: identity.static_public_bytes(),
        secret: *identity.static_secret_bytes(),
    }
}

fn garlic_envelope(bytes: Vec<u8>) -> I2npMessage {
    I2npMessage::new_standard(
        1,
        i2pr_proto::Date::from_millis(0),
        I2npBody::Garlic(OpaqueMessageBody {
            payload: DeferredPayload::new(bytes, MAX_I2NP_PAYLOAD_SIZE).expect("payload"),
        }),
    )
    .expect("garlic envelope")
}

fn payload_for(marker: u8) -> Vec<u8> {
    let clove = GarlicCloveBlock {
        delivery: GarlicDelivery::Local,
        message: vec![marker; 12],
    };
    encode_garlic_clove_payload(&clove).expect("payload")
}

fn encoded_new_session(outbound: &i2pr_client::EciesOutboundMessage) -> Vec<u8> {
    match outbound {
        i2pr_client::EciesOutboundMessage::NewSession { message } => message
            .encode_to_vec(MAX_I2NP_PAYLOAD_SIZE)
            .expect("encode ns"),
        i2pr_client::EciesOutboundMessage::Existing(_)
        | i2pr_client::EciesOutboundMessage::NewSessionReply(_) => {
            panic!("expected a fresh bound New Session")
        }
    }
}

fn encoded_existing(outbound: &i2pr_client::EciesOutboundMessage) -> Vec<u8> {
    match outbound {
        i2pr_client::EciesOutboundMessage::Existing(message) => message
            .encode_to_vec(MAX_I2NP_PAYLOAD_SIZE)
            .expect("encode es"),
        i2pr_client::EciesOutboundMessage::NewSession { .. }
        | i2pr_client::EciesOutboundMessage::NewSessionReply(_) => {
            panic!("expected an Existing Session message")
        }
    }
}

/// Drives one full bound handshake between two fresh managers and
/// returns both managers plus the encoded New Session bytes.
fn established_pair(
    rng: &mut ChaCha8Rng,
    alice: &TestIdentity,
    bob: &TestIdentity,
) -> (EciesSessionManager, EciesSessionManager, Vec<u8>) {
    let mut alice_manager = EciesSessionManager::new(EciesSessionConfig::balanced());
    let outbound = alice_manager
        .encrypt_to_remote(
            alice.id,
            &alice.secret,
            &bob.public,
            &bob.public,
            &payload_for(0x01),
            NOW_SECONDS,
            rng,
        )
        .expect("initiate");
    let ns_bytes = encoded_new_session(&outbound);

    let mut bob_manager = EciesSessionManager::new(EciesSessionConfig::balanced());
    let ns = BoundNewSessionMessage::decode(&ns_bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode ns");
    let accepted = bob_manager
        .accept_new_session(bob.id, &bob.secret, &bob.public, &ns, NOW_SECONDS)
        .expect("accept ns");
    let reply = bob_manager
        .seal_new_session_reply_for(
            bob.id,
            &bob.secret,
            &accepted.alice_static_public,
            &payload_for(0x02),
            NOW_SECONDS,
            rng,
        )
        .expect("seal reply");
    let reply_bytes = reply
        .message
        .encode_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .expect("encode nsr");

    let parsed =
        NewSessionReplyMessage::decode(&reply_bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode nsr");
    alice_manager
        .accept_new_session_reply(alice.id, &alice.secret, &parsed, NOW_SECONDS)
        .expect("accept reply");

    (alice_manager, bob_manager, ns_bytes)
}

/// The full bidirectional lifecycle with exact-once Existing
/// Session delivery in both directions.
#[test]
fn plan_126_full_manager_lifecycle_bidirectional_exact_once() {
    let mut rng = ChaCha8Rng::seed_from_u64(0x126_001);
    let alice = identity_with_static(0xA6);
    let bob = identity_with_static(0xB6);

    // Step 1: Alice initiates; her manager retains the pending
    // reply window internally.
    let mut alice_manager = EciesSessionManager::new(EciesSessionConfig::balanced());
    let outbound = alice_manager
        .encrypt_to_remote(
            alice.id,
            &alice.secret,
            &bob.public,
            &bob.public,
            &payload_for(0x11),
            NOW_SECONDS,
            &mut rng,
        )
        .expect("alice initiate");
    let ns_bytes = encoded_new_session(&outbound);
    assert_eq!(alice_manager.pending_handshake_count(), 1);

    // Step 2: Bob classifies and accepts.
    let mut bob_manager = EciesSessionManager::new(EciesSessionConfig::balanced());
    assert_eq!(
        bob_manager.classify(&ns_bytes),
        ClassifiedInbound::CandidateNewSession
    );
    let garlic = garlic_envelope(ns_bytes.clone());
    match garlic.body() {
        I2npBody::Garlic(body) => {
            assert_eq!(body.payload.as_bytes(), ns_bytes.as_slice())
        }
        other => panic!("expected garlic body, got {other:?}"),
    }
    let ns = BoundNewSessionMessage::decode(&ns_bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode ns");
    let accepted = bob_manager
        .accept_new_session(bob.id, &bob.secret, &bob.public, &ns, NOW_SECONDS)
        .expect("bob accept ns");
    assert_eq!(accepted.alice_static_public, alice.public);
    let inbound_clove = decode_decrypted_payload(&accepted.payload).expect("clove");
    assert_eq!(inbound_clove.message, vec![0x11_u8; 12]);
    assert_eq!(bob_manager.provisional_responder_count(), 1);

    // Step 3: Bob seals the reply; the paired session is promoted.
    let reply_outbound = bob_manager
        .seal_new_session_reply_for(
            bob.id,
            &bob.secret,
            &accepted.alice_static_public,
            &payload_for(0x22),
            NOW_SECONDS,
            &mut rng,
        )
        .expect("bob seal nsr");
    assert_eq!(bob_manager.provisional_responder_count(), 0);
    assert_eq!(bob_manager.established_sessions(), 1);
    let reply_bytes = reply_outbound
        .message
        .encode_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .expect("encode nsr");

    // Step 4: Alice classifies and accepts the reply.
    assert_eq!(
        alice_manager.classify(&reply_bytes),
        ClassifiedInbound::NewSessionReply
    );
    let reply_message =
        NewSessionReplyMessage::decode(&reply_bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode nsr");
    let accepted_reply = alice_manager
        .accept_new_session_reply(alice.id, &alice.secret, &reply_message, NOW_SECONDS)
        .expect("alice accept nsr");
    assert_eq!(accepted_reply.remote_static_public, bob.public);
    assert_eq!(accepted_reply.payload, payload_for(0x22));
    assert_eq!(alice_manager.pending_handshake_count(), 0);
    assert_eq!(alice_manager.established_sessions(), 1);

    // Step 5: Bidirectional Existing Session exchange. A -> B now
    // rides the paired session instead of a new handshake.
    let es_a = alice_manager
        .encrypt_to_remote(
            alice.id,
            &alice.secret,
            &bob.public,
            &bob.public,
            &payload_for(0x33),
            NOW_SECONDS,
            &mut rng,
        )
        .expect("alice es");
    let es_a_bytes = encoded_existing(&es_a);
    assert_eq!(
        bob_manager.classify(&es_a_bytes),
        ClassifiedInbound::ExistingSession
    );
    let es_a_message =
        ExistingSessionMessage::decode(&es_a_bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode es");
    let received_a = bob_manager
        .accept_existing_session(&es_a_message)
        .expect("bob decrypt es");
    let received_clove = decode_decrypted_payload(&received_a.payload).expect("clove");
    assert_eq!(received_clove.message, vec![0x33_u8; 12]);
    assert_eq!(received_a.remote_static_public, alice.public);

    // B -> A direction over the opposite tag set.
    let es_b = bob_manager
        .encrypt_to_remote(
            bob.id,
            &bob.secret,
            &alice.public,
            &alice.public,
            &payload_for(0x44),
            NOW_SECONDS,
            &mut rng,
        )
        .expect("bob es");
    let es_b_bytes = encoded_existing(&es_b);
    let es_b_message =
        ExistingSessionMessage::decode(&es_b_bytes, MAX_I2NP_PAYLOAD_SIZE).expect("decode es");
    let received_b = alice_manager
        .accept_existing_session(&es_b_message)
        .expect("alice decrypt es");
    assert_eq!(received_b.remote_static_public, bob.public);
    assert_eq!(
        decode_decrypted_payload(&received_b.payload)
            .expect("clove")
            .message,
        vec![0x44_u8; 12]
    );
}

/// Replaying an Existing Session message must be rejected because
/// the remove-on-hit window already consumed its tag.
#[test]
fn plan_126_existing_session_replay_is_rejected() {
    let mut rng = ChaCha8Rng::seed_from_u64(0x126_002);
    let alice = identity_with_static(0xA7);
    let bob = identity_with_static(0xB7);
    let (mut alice_manager, mut bob_manager, _ns_bytes) = established_pair(&mut rng, &alice, &bob);

    let outbound = alice_manager
        .encrypt_to_remote(
            alice.id,
            &alice.secret,
            &bob.public,
            &bob.public,
            &payload_for(0x55),
            NOW_SECONDS,
            &mut rng,
        )
        .expect("alice es");
    let bytes = encoded_existing(&outbound);
    let message = ExistingSessionMessage::decode(&bytes, MAX_I2NP_PAYLOAD_SIZE).expect("dec");

    bob_manager
        .accept_existing_session(&message)
        .expect("first delivery accepted");
    let replay = bob_manager.accept_existing_session(&message).unwrap_err();
    assert_eq!(replay, EciesSessionError::UnknownSessionTag);
}

/// An Existing Session message whose tag matches no window is
/// rejected typed without consuming state.
#[test]
fn plan_126_unknown_tag_is_rejected() {
    let mut rng = ChaCha8Rng::seed_from_u64(0x126_003);
    let alice = identity_with_static(0xA8);
    let bob = identity_with_static(0xB8);
    let (mut _alice_manager, mut bob_manager, _ns_bytes) = established_pair(&mut rng, &alice, &bob);

    let forged = ExistingSessionMessage::new([0xEE_u8; 8], vec![0_u8; 16]).expect("forged");
    let forged_bytes = forged.encode_to_vec(MAX_I2NP_PAYLOAD_SIZE).expect("enc");
    assert_eq!(
        bob_manager.classify(&forged_bytes),
        ClassifiedInbound::Unknown(ClassifiedUnknown::UnmatchedTag)
    );
    let outcome = bob_manager.accept_existing_session(&forged).unwrap_err();
    assert_eq!(outcome, EciesSessionError::UnknownSessionTag);
}

/// A replayed New Session Reply (after acceptance consumed the
/// pending slot) is rejected typed.
#[test]
fn plan_126_new_session_reply_after_acceptance_is_rejected() {
    let mut rng = ChaCha8Rng::seed_from_u64(0x126_004);
    let alice = identity_with_static(0xA9);
    let bob = identity_with_static(0xB9);

    let mut alice_manager = EciesSessionManager::new(EciesSessionConfig::balanced());
    let outbound = alice_manager
        .encrypt_to_remote(
            alice.id,
            &alice.secret,
            &bob.public,
            &bob.public,
            &payload_for(0x66),
            NOW_SECONDS,
            &mut rng,
        )
        .expect("initiate");
    let ns_bytes = encoded_new_session(&outbound);

    let mut bob_manager = EciesSessionManager::new(EciesSessionConfig::balanced());
    let ns = BoundNewSessionMessage::decode(&ns_bytes, MAX_I2NP_PAYLOAD_SIZE).expect("dec");
    let accepted = bob_manager
        .accept_new_session(bob.id, &bob.secret, &bob.public, &ns, NOW_SECONDS)
        .expect("accept");
    let reply = bob_manager
        .seal_new_session_reply_for(
            bob.id,
            &bob.secret,
            &accepted.alice_static_public,
            &payload_for(0x77),
            NOW_SECONDS,
            &mut rng,
        )
        .expect("seal reply");
    let reply_bytes = reply
        .message
        .encode_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .expect("enc");

    let parsed = NewSessionReplyMessage::decode(&reply_bytes, MAX_I2NP_PAYLOAD_SIZE).expect("dec");
    alice_manager
        .accept_new_session_reply(alice.id, &alice.secret, &parsed, NOW_SECONDS)
        .expect("first reply accepted");
    let second = alice_manager
        .accept_new_session_reply(alice.id, &alice.secret, &parsed, NOW_SECONDS)
        .unwrap_err();
    assert_eq!(second, EciesSessionError::NoPendingHandshake);
}

/// A duplicate bound New Session (same ephemeral representative)
/// is rejected typed.
#[test]
fn plan_126_duplicate_bound_new_session_is_rejected() {
    let mut rng = ChaCha8Rng::seed_from_u64(0x126_005);
    let alice = identity_with_static(0xAA);
    let bob = identity_with_static(0xBB);

    let mut alice_manager = EciesSessionManager::new(EciesSessionConfig::balanced());
    let outbound = alice_manager
        .encrypt_to_remote(
            alice.id,
            &alice.secret,
            &bob.public,
            &bob.public,
            &payload_for(0x88),
            NOW_SECONDS,
            &mut rng,
        )
        .expect("initiate");
    let ns_bytes = encoded_new_session(&outbound);
    let message = BoundNewSessionMessage::decode(&ns_bytes, MAX_I2NP_PAYLOAD_SIZE).expect("dec");

    let mut bob_manager = EciesSessionManager::new(EciesSessionConfig::balanced());
    bob_manager
        .accept_new_session(bob.id, &bob.secret, &bob.public, &message, NOW_SECONDS)
        .expect("first accept");
    let duplicate = bob_manager
        .accept_new_session(bob.id, &bob.secret, &bob.public, &message, NOW_SECONDS)
        .unwrap_err();
    assert_eq!(duplicate, EciesSessionError::DuplicateNewSession);
}

/// A third destination's manager cannot accept traffic addressed
/// to another destination's static key pair.
#[test]
fn plan_126_cross_destination_isolation() {
    let mut rng = ChaCha8Rng::seed_from_u64(0x126_006);
    let alice = identity_with_static(0xAC);
    let bob = identity_with_static(0xBC);
    let mallory = identity_with_static(0xCC);

    let mut alice_manager = EciesSessionManager::new(EciesSessionConfig::balanced());
    let outbound = alice_manager
        .encrypt_to_remote(
            alice.id,
            &alice.secret,
            &bob.public,
            &bob.public,
            &payload_for(0x99),
            NOW_SECONDS,
            &mut rng,
        )
        .expect("initiate");
    let ns_bytes = encoded_new_session(&outbound);
    let message = BoundNewSessionMessage::decode(&ns_bytes, MAX_I2NP_PAYLOAD_SIZE).expect("dec");

    let mut mallory_manager = EciesSessionManager::new(EciesSessionConfig::balanced());
    let outcome = mallory_manager.accept_new_session(
        mallory.id,
        &mallory.secret,
        &mallory.public,
        &message,
        NOW_SECONDS,
    );
    assert!(matches!(
        outcome.unwrap_err(),
        EciesSessionError::Ecies(i2pr_crypto::EciesError::AuthenticationFailed)
    ));
    assert_eq!(mallory_manager.provisional_responder_count(), 0);
}

/// Pending-handshake capacity is enforced with a typed error.
#[test]
fn plan_126_pending_capacity_is_typed() {
    let mut rng = ChaCha8Rng::seed_from_u64(0x126_007);
    let alice = identity_with_static(0xAD);
    let config = EciesSessionConfig::try_new(8, 2, 4, 600).expect("config");
    let mut manager = EciesSessionManager::new(config);
    let remote_a = identity_with_static(0xBD);
    let remote_b = identity_with_static(0xCD);
    let remote_c = identity_with_static(0xDD);

    for remote in [&remote_a, &remote_b] {
        let outbound = manager.encrypt_to_remote(
            alice.id,
            &alice.secret,
            &remote.public,
            &remote.public,
            &payload_for(0x10),
            NOW_SECONDS,
            &mut rng,
        );
        encoded_new_session(&outbound.expect("initiate should fit"));
    }
    let exhausted = manager.encrypt_to_remote(
        alice.id,
        &alice.secret,
        &remote_c.public,
        &remote_c.public,
        &payload_for(0x10),
        NOW_SECONDS,
        &mut rng,
    );
    assert_eq!(
        exhausted.unwrap_err(),
        EciesSessionError::PendingHandshakeCapacity { maximum: 2 }
    );
}

/// Idle expiry drops stale sessions and pendings; a subsequent
/// initiate allocates a fresh handshake.
#[test]
fn plan_126_advance_time_expires_state() {
    let mut rng = ChaCha8Rng::seed_from_u64(0x126_008);
    let alice = identity_with_static(0xAE);
    let bob = identity_with_static(0xBE);
    let (mut alice_manager, mut bob_manager, _ns_bytes) = established_pair(&mut rng, &alice, &bob);

    let report = alice_manager.advance_time(NOW_SECONDS + 700);
    assert_eq!(report.expired_sessions, 1);
    assert_eq!(report.pending_handshakes, 0);
    let report_bob = bob_manager.advance_time(NOW_SECONDS + 700);
    assert_eq!(report_bob.expired_sessions, 1);

    let outbound = alice_manager
        .encrypt_to_remote(
            alice.id,
            &alice.secret,
            &bob.public,
            &bob.public,
            &payload_for(0x20),
            NOW_SECONDS + 800,
            &mut rng,
        )
        .expect("re-initiate after expiry");
    assert!(matches!(
        outbound,
        i2pr_client::EciesOutboundMessage::NewSession { .. }
    ));
}

/// Envelopes shorter than the smallest ECIES message classify as
/// too-short typed unknowns.
#[test]
fn plan_126_classify_short_envelope() {
    let manager = EciesSessionManager::new(EciesSessionConfig::balanced());
    assert_eq!(
        manager.classify(&[0_u8; 23]),
        ClassifiedInbound::Unknown(ClassifiedUnknown::TooShort {
            actual: 23,
            minimum: 24
        })
    );
    let _ = Hash::from_bytes([0_u8; 32]);
}
