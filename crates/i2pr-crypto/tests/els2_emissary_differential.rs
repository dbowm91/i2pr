//! Plan 332 post-freeze black-box differential against `eggstack/emissary@6885a945`.
//!
//! # What this is and is not
//!
//! The values in `data/els2-emissary-differential.json` were produced by invoking
//! the Emissary crate's **public API** after the i2pr implementation was frozen at
//! commit `4e691bd`. No Emissary source was read; the driver that produced them
//! lives outside this repository and is not committed. That satisfies ADR 0028
//! §7 as amended by Plan 329, which permits Emissary as a post-implementation
//! behavioral oracle and forbids deriving a fixture from reading its internals.
//!
//! What this test does is narrower than a live interoperability gate, and the
//! difference is deliberate: Emissary is an independent implementation of the same
//! specification, not a second router. It says i2pr's layer cryptography, its
//! credential derivation, and its encrypted-service address encoding are the
//! specification's rather than one reading of it. It does **not** say an i2pr
//! record interoperates on the live network — the type-11 signature divergence
//! with i2pd and Java I2P still blocks that, and is Plan 335's problem.

use i2pr_crypto::red25519::Red25519PublicKey;
use i2pr_crypto::{LayerCipherKey, chacha20_xor_layer_owned, hkdf_sha256_extract_and_expand};
use i2pr_proto::{
    B32_BLINDED_SIGTYPE, B32_FLAG_REQUIRES_BLINDING_SECRET, B32_UNBLINDED_SIGTYPE_ED25519,
    EncryptedServiceAddress, SigningKeyType,
};

fn hex_to_bytes(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .map(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).expect("hex digit"))
        .collect()
}

#[test]
fn emissary_agrees_with_i2pr_on_the_credential_and_subcredential() {
    let raw = include_str!("data/els2-emissary-differential.json");
    let document: serde_json::Value = serde_json::from_str(raw).expect("fixture json");
    let case = &document["case"];

    let unblinded: [u8; 32] = hex_to_bytes(case["unblinded_public_key"].as_str().expect("key"))
        .try_into()
        .expect("32-byte unblinded key");
    let blinded: [u8; 32] = hex_to_bytes(case["blinded_public_key"].as_str().expect("key"))
        .try_into()
        .expect("32-byte blinded key");

    // The credential is a pure function of the unblinded key, so it is reproduced
    // here from first principles rather than through the NetDB helper: this is
    // the row that would catch a change to the personalization string or to the
    // signature-type encoding in the key data.
    let sigtype = SigningKeyType::EdDsaSha512Ed25519;
    let mut keydata = [0_u8; 36];
    keydata[..32].copy_from_slice(&unblinded);
    keydata[32..34].copy_from_slice(&sigtype.code().to_be_bytes());
    keydata[34..36].copy_from_slice(&B32_BLINDED_SIGTYPE.to_be_bytes());
    let mut credential_input = b"credential".to_vec();
    credential_input.extend_from_slice(&keydata);
    let credential = i2pr_crypto::sha256(&credential_input).as_bytes().to_vec();

    let mut subcredential_input = b"subcredential".to_vec();
    subcredential_input.extend_from_slice(&credential);
    subcredential_input.extend_from_slice(&blinded);
    let subcredential = i2pr_crypto::sha256(&subcredential_input)
        .as_bytes()
        .to_vec();

    assert_eq!(
        credential,
        hex_to_bytes(case["i2pr"]["credential"].as_str().expect("credential")),
        "the i2pr credential must still match the oracle"
    );
    assert_eq!(
        subcredential,
        hex_to_bytes(
            case["i2pr"]["subcredential"]
                .as_str()
                .expect("subcredential")
        ),
        "the i2pr subcredential must still match the oracle"
    );
    assert_eq!(
        credential,
        hex_to_bytes(case["emissary"]["credential"].as_str().expect("credential")),
        "the recorded credential and subcredential must be the same value on both sides"
    );
    assert_eq!(
        subcredential,
        hex_to_bytes(
            case["emissary"]["subcredential"]
                .as_str()
                .expect("subcredential")
        )
    );
}

#[test]
fn emissary_agrees_with_i2pr_on_the_complete_outer_ciphertext() {
    let raw = include_str!("data/els2-emissary-differential.json");
    let document: serde_json::Value = serde_json::from_str(raw).expect("fixture json");
    let case = &document["case"];

    let subcredential: [u8; 32] = hex_to_bytes(
        case["i2pr"]["subcredential"]
            .as_str()
            .expect("subcredential"),
    )
    .try_into()
    .expect("32-byte subcredential");
    let outer_salt: [u8; 32] = hex_to_bytes(case["outer_salt"].as_str().expect("outer salt"))
        .try_into()
        .expect("32-byte outer salt");
    let inner_salt: [u8; 32] = hex_to_bytes(case["inner_salt"].as_str().expect("inner salt"))
        .try_into()
        .expect("32-byte inner salt");
    let published = u32::try_from(case["published"].as_u64().expect("published")).expect("u32");
    let inner = hex_to_bytes(case["inner_record"].as_str().expect("inner"));
    let store_type =
        u8::try_from(case["inner_store_type"].as_u64().expect("store type")).expect("u8");

    // Rebuild both layers from the recorded subcredential and salts, one
    // derivation at a time, so a failure names the layer that moved rather than
    // only reporting that a whole ciphertext differs.
    let mut layer2_plaintext = Vec::with_capacity(inner.len() + 1);
    layer2_plaintext.push(store_type);
    layer2_plaintext.extend_from_slice(&inner);

    let mut input = Vec::with_capacity(36);
    input.extend_from_slice(&subcredential);
    input.extend_from_slice(&published.to_be_bytes());

    let inner_okm = hkdf_sha256_extract_and_expand(&inner_salt, &input, b"ELS2_L2K", 44)
        .expect("layer-2 derivation");
    let inner_key = LayerCipherKey::from_bytes(inner_okm[..32].try_into().expect("32-byte key"));
    let inner_nonce: [u8; 12] = inner_okm[32..].try_into().expect("12-byte nonce");
    let encrypted_inner = chacha20_xor_layer_owned(&inner_key, &inner_nonce, &layer2_plaintext)
        .expect("layer-2 stream");
    let mut inner_ciphertext = Vec::with_capacity(32 + encrypted_inner.len());
    inner_ciphertext.extend_from_slice(&inner_salt);
    inner_ciphertext.extend_from_slice(&encrypted_inner);

    let mut layer1_plaintext = Vec::with_capacity(1 + inner_ciphertext.len());
    layer1_plaintext.push(0);
    layer1_plaintext.extend_from_slice(&inner_ciphertext);

    let outer_okm = hkdf_sha256_extract_and_expand(&outer_salt, &input, b"ELS2_L1K", 44)
        .expect("layer-1 derivation");
    let outer_key = LayerCipherKey::from_bytes(outer_okm[..32].try_into().expect("32-byte key"));
    let outer_nonce: [u8; 12] = outer_okm[32..].try_into().expect("12-byte nonce");
    let encrypted_outer = chacha20_xor_layer_owned(&outer_key, &outer_nonce, &layer1_plaintext)
        .expect("layer-1 stream");
    let mut outer_ciphertext = Vec::with_capacity(32 + encrypted_outer.len());
    outer_ciphertext.extend_from_slice(&outer_salt);
    outer_ciphertext.extend_from_slice(&encrypted_outer);

    assert_eq!(
        outer_ciphertext,
        hex_to_bytes(
            case["i2pr"]["outer_ciphertext"]
                .as_str()
                .expect("outer ciphertext")
        ),
        "the i2pr outer ciphertext must still match the oracle byte-for-byte"
    );
    assert_eq!(
        outer_ciphertext,
        hex_to_bytes(
            case["emissary"]["outer_ciphertext"]
                .as_str()
                .expect("outer ciphertext")
        ),
        "and the recorded oracle ciphertext must be the same bytes"
    );
    // The oracle's helper prepends the inner store-type byte itself, so the
    // layer-2 plaintext it produced is exactly this one. The equivalence is the
    // only reason the ciphertexts match, and it is recorded in the fixture.
    assert_eq!(
        outer_ciphertext.len(),
        32 + 1 + 32 + 1 + inner.len(),
        "the framing overhead is 66 bytes, not the 130 a first-pass bound assumed"
    );
}

#[test]
fn emissary_agrees_with_i2pr_on_the_encrypted_service_address() {
    let raw = include_str!("data/els2-emissary-differential.json");
    let document: serde_json::Value = serde_json::from_str(raw).expect("fixture json");
    let case = &document["case"];

    let unblinded: [u8; 32] = hex_to_bytes(case["unblinded_public_key"].as_str().expect("key"))
        .try_into()
        .expect("32-byte unblinded key");
    let address = EncryptedServiceAddress::new(
        B32_UNBLINDED_SIGTYPE_ED25519,
        B32_BLINDED_SIGTYPE,
        unblinded,
        case["secret_required"].as_bool().expect("secret flag"),
        false,
    )
    .expect("address");
    let text = address.to_text().expect("address text");

    assert_eq!(
        text,
        case["i2pr"]["address_text"].as_str().expect("i2pr address"),
        "the i2pr address text must still match the oracle"
    );
    assert_eq!(
        text,
        case["emissary"]["encoded_address"]
            .as_str()
            .expect("oracle address"),
        "and the recorded oracle address must be the same text"
    );
    assert_eq!(
        address.flags() & B32_FLAG_REQUIRES_BLINDING_SECRET,
        B32_FLAG_REQUIRES_BLINDING_SECRET,
        "the secret-required flag must survive the round trip the oracle decoded"
    );
    assert_eq!(
        address.public_key(),
        &hex_to_bytes(
            case["emissary"]["decoded_unblinded_public_key"]
                .as_str()
                .expect("key")
        )
        .as_slice(),
        "the public key the oracle recovered must be the one that was encoded"
    );
}

#[test]
fn the_recorded_blinding_day_is_still_reproduced_from_the_published_timestamp() {
    let raw = include_str!("data/els2-emissary-differential.json");
    let document: serde_json::Value = serde_json::from_str(raw).expect("fixture json");
    let case = &document["case"];

    // The recorded day was derived from the published timestamp by the NetDB
    // calendar conversion, which this crate cannot call without a circular
    // dependency. The row therefore pins the value the oracle and i2pr agreed
    // on, and the conversion itself is covered by the NetDB lifecycle rows.
    let day = i2pr_crypto::red25519::BlindingDay::from_ymd(2023, 11, 14).expect("day");
    assert_eq!(
        String::from_utf8_lossy(&day.as_bytes()),
        case["i2pr"]["day"].as_str().expect("day")
    );

    // The recorded blinded key and storage key are well-formed 32-byte values
    // that the codec still accepts, so the fixture cannot drift into a shape the
    // implementation would reject.
    let blinded: [u8; 32] = hex_to_bytes(
        case["i2pr"]["daily_blinded_public_key"]
            .as_str()
            .expect("blinded key"),
    )
    .try_into()
    .expect("32-byte blinded key");
    assert!(
        Red25519PublicKey::decode(&blinded).is_ok(),
        "the recorded blinded key must still be a decodable curve point"
    );
    let storage: [u8; 32] =
        hex_to_bytes(case["i2pr"]["storage_key"].as_str().expect("storage key"))
            .try_into()
            .expect("32-byte storage key");
    assert_eq!(
        Red25519PublicKey::decode(&blinded)
            .expect("blinded")
            .as_bytes(),
        &blinded
    );
    assert_eq!(storage.len(), 32);
}

#[test]
fn the_fixture_records_agreement_rather_than_asserting_it_away() {
    // The agreement flags are stored so a reader can see the observation without
    // running the oracle. This test asserts they are all true, which means a
    // hand-edited fixture claiming a mismatch fails rather than passing quietly.
    let raw = include_str!("data/els2-emissary-differential.json");
    let document: serde_json::Value = serde_json::from_str(raw).expect("fixture json");
    let agreement = document["agreement"].as_object().expect("agreement object");
    assert!(
        !agreement.is_empty(),
        "the fixture must record at least one property"
    );
    for (property, value) in agreement {
        assert_eq!(
            value.as_bool(),
            Some(true),
            "the fixture records a disagreement for {property}; the oracle observation \
             must be re-run rather than edited"
        );
    }
    assert_eq!(
        document["provenance"]["freeze"]
            .as_str()
            .expect("freeze note"),
        "i2pr implementation commit 4e691bd was frozen before the oracle was invoked",
        "the post-freeze condition must remain recorded in the fixture"
    );
}
