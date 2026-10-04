//! Plan 333 post-freeze Emissary black-box differential for the encrypted
//! LeaseSet2 per-client authorization block.
//!
//! The fixture in `data/els2-auth-emissary-differential.json` records one case in
//! which i2pr and the pinned Emissary reference were driven from byte-identical
//! inputs — the same subcredential, the same salts, the same auth cookie, the
//! same client keys, the same ephemeral key — and both produced the same
//! 440-byte outer ciphertext, for the pre-shared-key scheme and the
//! Diffie-Hellman scheme alike.
//!
//! These rows do not simply re-read the recorded bytes. Each one rebuilds the
//! record from the case inputs and compares it against what the oracle produced,
//! so a regression in i2pr's authorization construction fails here rather than
//! being masked by a stale fixture. The reverse direction — the oracle decrypting
//! an i2pr record — cannot be re-executed in CI, because the oracle is not
//! committed and the fixture records it as an observation, not as a re-runnable
//! assertion; those rows therefore assert the recorded equality and say so.

use i2pr_crypto::X25519PrivateKey;
use i2pr_crypto::red25519::Red25519PublicKey;
use i2pr_netdb::{
    AuthBlock, AuthCookie, BlindingIdentity, Els2ClientAuth, Els2Credentials, Layer1Authorization,
    PskClientKey, build_dh_block, build_psk_block, encrypt_outer_ciphertext, recover_auth_cookie,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const FIXTURE: &str = include_str!("data/els2-auth-emissary-differential.json");

fn hex_to_bytes(text: &str) -> Vec<u8> {
    assert!(
        text.len().is_multiple_of(2),
        "hex string must be whole bytes"
    );
    (0..text.len() / 2)
        .map(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).expect("hex digit"))
        .collect()
}

fn hex_to_32(text: &str) -> [u8; 32] {
    hex_to_bytes(text).try_into().expect("32 bytes")
}

/// Decodes a fixture hex field into bytes, for comparison against a value the
/// test derived rather than read.
fn field_bytes(document: &str, name: &str) -> Vec<u8> {
    hex_to_bytes(field(document, name))
}

/// Minimal reader for the flat fixture. The document is small and fixed-shape,
/// so a full JSON dependency is not worth adding for it; `serde_json` is
/// available to this crate but the two helpers keep the test readable.
fn field<'a>(document: &'a str, name: &str) -> &'a str {
    let needle = format!("\"{name}\": \"");
    let at = document
        .find(&needle)
        .unwrap_or_else(|| panic!("fixture field {name}"));
    let rest = &document[at + needle.len()..];
    let end = rest.find('"').expect("closing quote");
    &rest[..end]
}

fn number(document: &str, name: &str) -> u64 {
    let needle = format!("\"{name}\": ");
    let at = document
        .find(&needle)
        .unwrap_or_else(|| panic!("fixture number {name}"));
    let rest = document[at + needle.len()..].trim_start();
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().expect("u64")
}

fn flag(document: &str, name: &str) -> bool {
    let needle = format!("\"{name}\": ");
    let at = document
        .find(&needle)
        .unwrap_or_else(|| panic!("fixture flag {name}"));
    let rest = document[at + needle.len()..].trim_start();
    rest.starts_with("true")
}

struct Case {
    published: u32,
    inner: Vec<u8>,
    inner_salt: [u8; 32],
    outer_salt: [u8; 32],
    auth_cookie: [u8; 32],
    subcredential: [u8; 32],
    psk: [u8; 32],
    dh_private: X25519PrivateKey,
    dh_public: [u8; 32],
    ephemeral: X25519PrivateKey,
    /// The credentials the record was published under, reconstructed from the
    /// recorded subcredential so the two sides are compared on the same input.
    credentials: Els2Credentials,
}

fn case() -> Case {
    let published = u32::try_from(number(FIXTURE, "published")).expect("u32 published");
    // The credentials are derived from the recorded service identity rather than
    // taken on trust, and the resulting subcredential is then checked against
    // the recorded one. That check is what ties this lane to the exact input the
    // oracle was given.
    let unblinded = Red25519PublicKey::decode(&hex_to_32(field(FIXTURE, "unblinded_public_key")))
        .expect("unblinded public key");
    let blinded = Red25519PublicKey::decode(&hex_to_32(field(FIXTURE, "blinded_public_key")))
        .expect("blinded public key");
    let identity = BlindingIdentity::new(
        unblinded,
        i2pr_proto::SigningKeyType::EdDsaSha512Ed25519,
        None,
    )
    .expect("identity");
    let credentials = identity.credentials_for(&blinded);
    let subcredential = hex_to_32(field(FIXTURE, "subcredential"));
    assert_eq!(
        credentials.subcredential(),
        &subcredential,
        "the recorded subcredential must be the one this identity derives"
    );
    Case {
        published,
        inner: hex_to_bytes(field(FIXTURE, "inner_record")),
        inner_salt: hex_to_32(field(FIXTURE, "inner_salt")),
        outer_salt: hex_to_32(field(FIXTURE, "outer_salt")),
        auth_cookie: hex_to_32(field(FIXTURE, "auth_cookie")),
        subcredential,
        psk: hex_to_32(field(FIXTURE, "psk")),
        dh_private: X25519PrivateKey::from_bytes(hex_to_32(field(FIXTURE, "dh_private"))),
        dh_public: hex_to_32(field(FIXTURE, "dh_public")),
        ephemeral: X25519PrivateKey::from_bytes(hex_to_32(field(FIXTURE, "ephemeral_private"))),
        credentials,
    }
}

// The pre-shared-key authorization block draws its `authSalt` from the supplied
// generator, so the generator state is part of the recorded case rather than a
// detail of the test. Both seeds are read from the fixture.
fn psk_seed() -> u64 {
    number(FIXTURE, "psk_block_rng_seed")
}

fn dh_seed() -> u64 {
    number(FIXTURE, "dh_block_rng_seed")
}

fn psk_block(case: &Case) -> AuthBlock {
    let key = PskClientKey::from_bytes(case.psk);
    let cookie = AuthCookie::from_bytes(case.auth_cookie);
    build_psk_block(
        &cookie,
        &case.subcredential,
        case.published,
        &[&key],
        &mut ChaCha8Rng::seed_from_u64(psk_seed()),
    )
    .expect("psk block")
}

fn dh_block(case: &Case) -> AuthBlock {
    let cookie = AuthCookie::from_bytes(case.auth_cookie);
    build_dh_block(
        &cookie,
        &case.subcredential,
        case.published,
        &[i2pr_netdb::AuthClientPublicKey::from_bytes(case.dh_public)],
        &case.ephemeral,
        &mut ChaCha8Rng::seed_from_u64(dh_seed()),
    )
    .expect("dh block")
}

fn publish(case: &Case, block: &AuthBlock) -> Vec<u8> {
    let encoded = block.encode_to_vec().expect("encode block");
    let authorization =
        Layer1Authorization::new(block.scheme().to_flags(), &encoded, &case.auth_cookie)
            .expect("authorization");
    encrypt_outer_ciphertext(
        &case.credentials,
        case.published,
        i2pr_proto::INNER_LEASE_SET2_STORE_TYPE,
        &case.inner,
        authorization,
        &case.outer_salt,
        &case.inner_salt,
    )
    .expect("publish")
}

#[test]
fn the_pre_shared_key_outer_ciphertext_is_byte_identical_to_the_oracle() {
    let case = case();
    let block = psk_block(&case);
    // The salt is the first 32 bytes of the serialized block, and a single
    // client makes the whole block deterministic, so the record is reproducible
    // from the case alone.
    assert_eq!(
        block.salt().as_slice(),
        &block.encode_to_vec().expect("encode")[..32],
        "the pre-shared-key salt must be the first 32 bytes of the block"
    );
    assert_eq!(
        block.len(),
        1,
        "the recorded case is a single authorized client"
    );
    let ours = publish(&case, &block);
    assert_eq!(
        field_bytes(FIXTURE, "psk_outer_ciphertext"),
        ours,
        "i2pr's pre-shared-key publication must still match the oracle byte-for-byte"
    );
    assert_eq!(ours.len(), 440, "the recorded record is 440 bytes");
}

#[test]
fn the_diffie_hellman_outer_ciphertext_is_byte_identical_to_the_oracle() {
    let case = case();
    let block = dh_block(&case);
    let ours = publish(&case, &block);
    assert_eq!(
        field_bytes(FIXTURE, "dh_outer_ciphertext"),
        ours,
        "i2pr's Diffie-Hellman publication must still match the oracle byte-for-byte"
    );
    assert_eq!(ours.len(), 440, "the recorded record is 440 bytes");
}

#[test]
fn the_derived_client_identifiers_are_the_recorded_ones() {
    let case = case();
    let psk = psk_block(&case);
    let dh = dh_block(&case);
    assert_eq!(
        field_bytes(FIXTURE, "psk_client_id"),
        psk.entries()[0].client_id().to_vec(),
    );
    assert_eq!(
        field_bytes(FIXTURE, "dh_client_id"),
        dh.entries()[0].client_id().to_vec(),
    );
    // The two schemes must not derive the same identifier for one client, or a
    // PSK block and a DH block would be interchangeable.
    assert_ne!(psk.entries()[0].client_id(), dh.entries()[0].client_id());
}

#[test]
fn the_recorded_record_releases_the_authorization_only_to_its_own_client() {
    // The recorded inner payload is a fixed filler string rather than a real
    // LeaseSet2, because this lane compares the *authorization* construction and
    // a filler keeps the two sides pinned to identical layer-2 bytes. So this row
    // asserts the authorization decision itself rather than a full parse: a full
    // end-to-end decrypt of a real LeaseSet2 is covered by the client
    // round-trip rows, and this oracle lane adds nothing to it.
    let case = case();
    let psk_key = PskClientKey::from_bytes(case.psk);
    let mut wrong_bytes = case.psk;
    wrong_bytes[0] ^= 0xff;
    let wrong_psk = PskClientKey::from_bytes(wrong_bytes);
    let dh_public = i2pr_netdb::AuthClientPublicKey::from_bytes(case.dh_public);
    for (block, credential, wrong) in [
        (
            psk_block(&case),
            Els2ClientAuth::Psk(&psk_key),
            Els2ClientAuth::Psk(&wrong_psk),
        ),
        (
            dh_block(&case),
            Els2ClientAuth::Dh {
                private: &case.dh_private,
                public: dh_public,
            },
            // A pre-shared key must not stand in for a Diffie-Hellman
            // credential, which is the cross-scheme refusal the oracle also
            // reported.
            Els2ClientAuth::Psk(&psk_key),
        ),
    ] {
        let recovered =
            recover_auth_cookie(&block, &case.subcredential, case.published, &credential)
                .expect("the recorded client must be authorized by the recorded record");
        assert_eq!(
            recovered.as_bytes(),
            &case.auth_cookie,
            "the recovered cookie must be the one both sides encrypted with"
        );
        assert!(
            recover_auth_cookie(&block, &case.subcredential, case.published, &wrong).is_err(),
            "a wrong credential must not be authorized by the recorded record"
        );
    }
}

#[test]
fn the_authorization_block_from_the_recorded_record_parses_and_republishes() {
    // A structural round trip: decode the block exactly as a receiver would,
    // then rebuild the same outer ciphertext from the decoded block. If the
    // codec and the builder ever disagree, the republication stops matching.
    let case = case();
    let block = psk_block(&case);
    let ours = publish(&case, &block);
    let decoded = AuthBlock::decode(
        block.scheme(),
        &block.encode_to_vec().expect("encode block"),
    )
    .expect("the recorded block must parse");
    assert_eq!(decoded, block);
    let republished = publish(&case, &decoded);
    assert_eq!(
        ours, republished,
        "a decode/encode round trip must not change the record"
    );
}

#[test]
fn the_oracle_recovered_the_exact_inner_record_from_both_i2pr_records() {
    // Recorded observation, not a re-run: the oracle is not committed, so this
    // asserts that the fixture still says what the post-freeze run observed,
    // and that the inner record it recovered is the inner record we published.
    let case = case();
    assert!(flag(FIXTURE, "emissary_recovers_inner_from_i2pr_psk"));
    assert!(flag(FIXTURE, "emissary_recovers_inner_from_i2pr_dh"));
    for scheme in ["psk_inner_of_i2pr_record", "dh_inner_of_i2pr_record"] {
        assert_eq!(
            field_bytes(FIXTURE, scheme),
            case.inner,
            "the oracle must have recovered exactly the inner record i2pr published"
        );
    }
}

#[test]
fn the_oracle_agrees_about_which_credentials_are_refused() {
    // Same caveat as the row above: these are recorded observations of the
    // oracle's behavior, kept as a regression tripwire for the fixture.
    assert!(flag(FIXTURE, "psk_credential_refuses_dh_record"));
    assert!(flag(FIXTURE, "dh_credential_refuses_psk_record"));
    assert!(flag(FIXTURE, "wrong_psk_refused"));
    assert!(flag(FIXTURE, "psk_outer_ciphertext_identical"));
    assert!(flag(FIXTURE, "dh_outer_ciphertext_identical"));
}

#[test]
fn the_fixture_provenance_states_the_freeze_and_the_public_api_limit() {
    // The authority for using this reference at all is a governance question, not
    // a cryptographic one, so it is asserted here rather than left in a comment.
    let provenance = field(FIXTURE, "oracle");
    assert_eq!(
        provenance, "eggstack/emissary@6885a945d25a5ae61bc68191d27c5816bc3df4c9",
        "the oracle pin must be recorded in the fixture"
    );
    assert!(
        FIXTURE.contains("\"freeze\": \"i2pr implementation commit f525578"),
        "the fixture must record the freeze commit that preceded the oracle run"
    );
    assert!(
        FIXTURE.contains("public API only"),
        "the fixture must record that only the public API was used"
    );
    assert!(
        FIXTURE.contains("not committed"),
        "the fixture must record that the driver lives outside the repository"
    );
}

#[test]
fn the_fixture_records_its_own_limitations() {
    // A differential that silently omitted its scope would be worse than none.
    assert!(
        FIXTURE.contains("Single authorized client per scheme"),
        "the single-client scope must stay recorded"
    );
    assert!(
        FIXTURE.contains("Emissary caps the client count at 99"),
        "the client-count divergence must stay recorded"
    );
    assert!(
        FIXTURE.contains("signature transcript divergence"),
        "the type-11 limitation must stay recorded so this lane is not read as \
         an interoperability claim"
    );
}
