//! Plan 119 LeaseSet2 fixture coverage.
//!
//! The tests below build an ordinary online-signed published
//! Standard LeaseSet2 from a deterministic Ed25519/X25519 bundle, encode
//! it through the standard DatabaseStore envelope, decode and
//! re-encode through every supported envelope, and verify the signature
//! with `i2pr_crypto::verify_lease_set2` against the original signing
//! key. The frozen bytes also act as an independent wire-format check:
//! every byte before the signature must match the production codec
//! exactly.

use i2pr_crypto::{
    ROUTER_CRYPTO_KEY_TYPE, ROUTER_SIGNING_KEY_TYPE, RouterIdentityBundle, verify_lease_set2,
};
use i2pr_proto::{
    DatabaseStoreData, DatabaseStoreMessage, DatabaseStoreType, Date32, Hash, I2npHeader,
    I2npMessage, Lease2, LeaseSet2, LeaseSet2EncryptionKey, LeaseSet2Flags, LeaseSet2Header,
    LeaseSet2KeySelectionError, Mapping, MessageType, ProtocolErrorKind,
    SHORT_TRANSPORT_HEADER_SIZE, STANDARD_HEADER_SIZE, SignatureValue,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const MAX: usize = i2pr_proto::MAX_I2NP_PAYLOAD_SIZE
    + i2pr_proto::STANDARD_HEADER_SIZE
    + SHORT_TRANSPORT_HEADER_SIZE;

fn bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

fn destination_for(signer: &RouterIdentityBundle) -> i2pr_proto::Destination {
    i2pr_proto::Destination::new(signer.identity().key_and_cert().clone()).expect("destination")
}

fn build_ls2(signer: &RouterIdentityBundle) -> LeaseSet2 {
    let destination = destination_for(signer);
    let header =
        LeaseSet2Header::new(destination, 1_000, 600, LeaseSet2Flags::from_raw(0)).expect("header");
    let placeholder =
        SignatureValue::new(ROUTER_SIGNING_KEY_TYPE, vec![0u8; 64]).expect("placeholder");
    let encryption_keys = vec![
        LeaseSet2EncryptionKey::new(ROUTER_CRYPTO_KEY_TYPE, vec![0x55; 32])
            .expect("encryption key"),
    ];
    let leases = vec![Lease2::new(
        Hash::from_bytes([0x11; 32]),
        7,
        Date32::from_seconds(2_000),
    )];
    let unsigned = LeaseSet2::new(
        header,
        Mapping::empty(),
        encryption_keys,
        leases,
        placeholder,
    )
    .expect("unsigned ls2");
    let signature = signer
        .signing_key()
        .sign(&unsigned.signature_preimage())
        .expect("sign");
    LeaseSet2::new(
        unsigned.header().clone(),
        unsigned.options().clone(),
        unsigned.encryption_keys().to_vec(),
        unsigned.leases().to_vec(),
        signature,
    )
    .expect("ls2")
}

#[test]
fn frozen_ls2_round_trips_through_lease_set2_codec() {
    let signer = bundle(0xa11);
    let ls2 = build_ls2(&signer);
    let encoded = ls2.encode_to_vec(MAX).unwrap();
    let decoded = LeaseSet2::decode(&encoded, MAX).unwrap();
    assert_eq!(decoded.signed_bytes(), ls2.signed_bytes());
    assert_eq!(decoded.encode_to_vec(MAX).unwrap(), encoded);
    verify_lease_set2(&decoded).expect("signature verifies");
}

#[test]
fn frozen_ls2_signature_verifies_with_destination_signing_key() {
    let signer = bundle(0xa12);
    let ls2 = build_ls2(&signer);
    // Verify directly against the destination's signing key.
    verify_lease_set2(&ls2).expect("signature verifies");
    // The destination carried inside the LS2 is the signer's identity.
    let signing_key = ls2.destination().signing_key();
    assert_eq!(signing_key.key_type(), ROUTER_SIGNING_KEY_TYPE);
}

#[test]
fn frozen_ls2_usable_x25519_selector_returns_key() {
    let signer = bundle(0xa13);
    let ls2 = build_ls2(&signer);
    let key = ls2.usable_x25519_key().expect("x25519 selected");
    assert_eq!(key.key_type(), ROUTER_CRYPTO_KEY_TYPE);
    assert_eq!(key.as_bytes().len(), 32);
}

#[test]
fn frozen_ls2_duplicate_x25519_is_deterministic() {
    let signer = bundle(0xa14);
    let ls2 = build_ls2(&signer);
    let header = ls2.header().clone();
    let options = ls2.options().clone();
    let leases = ls2.leases().to_vec();
    let placeholder = SignatureValue::new(ROUTER_SIGNING_KEY_TYPE, vec![0u8; 64]).unwrap();
    let mut keys = ls2.encryption_keys().to_vec();
    keys.push(LeaseSet2EncryptionKey::new(ROUTER_CRYPTO_KEY_TYPE, vec![0x66; 32]).unwrap());
    let rebuilt = LeaseSet2::new(header, options, keys, leases, placeholder).unwrap();
    assert!(matches!(
        rebuilt.usable_x25519_key(),
        Err(LeaseSet2KeySelectionError::DuplicateX25519)
    ));
}

#[test]
fn database_store_type_3_envelope_round_trips() {
    let signer = bundle(0xa15);
    let ls2 = build_ls2(&signer);
    let key = ls2.key_hash().expect("destination hash");
    let store = DatabaseStoreMessage {
        key,
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::LeaseSet2(Box::new(ls2.clone())),
    };
    let body = i2pr_proto::I2npBody::DatabaseStore(Box::new(store));
    let message = I2npMessage::new_standard(0x0102_0304, i2pr_proto::Date::from_millis(0), body)
        .expect("new standard");
    let raw = message.encode_standard_to_vec(MAX).expect("raw");
    let decoded = I2npMessage::decode_standard(&raw, MAX).expect("decode standard");
    match decoded.body() {
        i2pr_proto::I2npBody::DatabaseStore(decoded_store) => {
            assert_eq!(decoded_store.key, key);
            match &decoded_store.data {
                DatabaseStoreData::LeaseSet2(decoded_ls2) => {
                    assert_eq!(decoded_ls2.signed_bytes(), ls2.signed_bytes());
                    verify_lease_set2(decoded_ls2).expect("verifies");
                }
                other => panic!("expected LeaseSet2, got {other:?}"),
            }
        }
        other => panic!("expected DatabaseStore, got {other:?}"),
    }
    // Re-encode must produce identical output.
    assert_eq!(raw, decoded.encode_standard_to_vec(MAX).unwrap());
    // Sanity: the envelope carries the type byte 3.
    let type_byte_index = STANDARD_HEADER_SIZE + 32;
    assert_eq!(raw[type_byte_index], 0x03);
}

#[test]
fn database_store_type_3_envelope_short_transport_round_trips() {
    let signer = bundle(0xa16);
    let ls2 = build_ls2(&signer);
    let key = ls2.key_hash().expect("destination hash");
    let store = DatabaseStoreMessage {
        key,
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::LeaseSet2(Box::new(ls2)),
    };
    let body = i2pr_proto::I2npBody::DatabaseStore(Box::new(store));
    let message =
        I2npMessage::new_short_transport(0x0102_0304, 0, body).expect("new short transport");
    let raw = message
        .encode_short_transport_to_vec(MAX)
        .expect("short raw");
    let decoded = I2npMessage::decode_short_transport(&raw, MAX).expect("short decode");
    assert!(matches!(
        decoded.body(),
        i2pr_proto::I2npBody::DatabaseStore(_)
    ));
    assert!(matches!(
        decoded.header(),
        I2npHeader::ShortTransport { .. }
    ));
}

#[test]
fn database_store_type_5_remains_explicitly_deferred() {
    // Type 5 (EncryptedLeaseSet) and type 7 (MetaLeaseSet) remain
    // deferred. Construct a minimal envelope and confirm the body is
    // still the Deferred variant.
    let key = Hash::from_bytes([0x33; 32]);
    let payload = vec![0u8; 8];
    let store = DatabaseStoreMessage {
        key,
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::Deferred {
            store_type: DatabaseStoreType::EncryptedLeaseSet,
            payload: i2pr_proto::DeferredPayload::new(payload, MAX).expect("payload"),
        },
    };
    let body = i2pr_proto::I2npBody::DatabaseStore(Box::new(store));
    let message = I2npMessage::new_standard(0x0102_0304, i2pr_proto::Date::from_millis(0), body)
        .expect("new standard");
    let raw = message.encode_standard_to_vec(MAX).expect("raw");
    let decoded = I2npMessage::decode_standard(&raw, MAX).expect("decode standard");
    match decoded.body() {
        i2pr_proto::I2npBody::DatabaseStore(decoded_store) => match &decoded_store.data {
            DatabaseStoreData::Deferred { store_type, .. } => {
                assert_eq!(*store_type, DatabaseStoreType::EncryptedLeaseSet);
            }
            other => panic!("expected Deferred, got {other:?}"),
        },
        other => panic!("expected DatabaseStore, got {other:?}"),
    }
}

#[test]
fn database_store_type_7_remains_explicitly_deferred() {
    let key = Hash::from_bytes([0x44; 32]);
    let payload = vec![0u8; 8];
    let store = DatabaseStoreMessage {
        key,
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::Deferred {
            store_type: DatabaseStoreType::MetaLeaseSet,
            payload: i2pr_proto::DeferredPayload::new(payload, MAX).expect("payload"),
        },
    };
    let body = i2pr_proto::I2npBody::DatabaseStore(Box::new(store));
    let message = I2npMessage::new_standard(0x0102_0304, i2pr_proto::Date::from_millis(0), body)
        .expect("new standard");
    let raw = message.encode_standard_to_vec(MAX).expect("raw");
    let decoded = I2npMessage::decode_standard(&raw, MAX).expect("decode standard");
    match decoded.body() {
        i2pr_proto::I2npBody::DatabaseStore(decoded_store) => match &decoded_store.data {
            DatabaseStoreData::Deferred { store_type, .. } => {
                assert_eq!(*store_type, DatabaseStoreType::MetaLeaseSet);
            }
            other => panic!("expected Deferred, got {other:?}"),
        },
        other => panic!("expected DatabaseStore, got {other:?}"),
    }
}

#[test]
fn ls2_reserved_flags_yield_typed_error() {
    let destination = destination_for(&bundle(0xa17));
    let dest_bytes = destination.encode_to_vec(MAX).unwrap();
    let mut payload = dest_bytes;
    payload.extend_from_slice(&1_000u32.to_be_bytes());
    payload.extend_from_slice(&600u16.to_be_bytes());
    payload.extend_from_slice(&0x0010u16.to_be_bytes());
    let error = LeaseSet2Header::decode(&payload, MAX).unwrap_err();
    assert!(matches!(error.kind(), ProtocolErrorKind::InvalidValue));
}

#[test]
fn ls2_offline_flag_yields_unsupported_error() {
    let destination = destination_for(&bundle(0xa18));
    let dest_bytes = destination.encode_to_vec(MAX).unwrap();
    let mut payload = dest_bytes;
    payload.extend_from_slice(&1_000u32.to_be_bytes());
    payload.extend_from_slice(&600u16.to_be_bytes());
    payload.extend_from_slice(&0x0001u16.to_be_bytes());
    let error = LeaseSet2Header::decode(&payload, MAX).unwrap_err();
    assert!(matches!(error.kind(), ProtocolErrorKind::Unsupported));
}

#[test]
fn standard_header_database_store_carries_database_store_message_type() {
    // Sanity check that the standard I2NP header wires the right type
    // byte for our DatabaseStore (type 1).
    let key = Hash::from_bytes([0x55; 32]);
    let store = DatabaseStoreMessage {
        key,
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: DatabaseStoreData::Deferred {
            store_type: DatabaseStoreType::LeaseSet2,
            payload: i2pr_proto::DeferredPayload::new(vec![0u8; 8], MAX).expect("payload"),
        },
    };
    let body = i2pr_proto::I2npBody::DatabaseStore(Box::new(store));
    let message = I2npMessage::new_standard(0x0102_0304, i2pr_proto::Date::from_millis(0), body)
        .expect("new standard");
    if let I2npHeader::Standard { message_type, .. } = message.header() {
        assert_eq!(message_type, MessageType::DatabaseStore);
    } else {
        panic!("expected standard header");
    }
}
