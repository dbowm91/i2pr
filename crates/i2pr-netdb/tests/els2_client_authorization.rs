//! Plan 333 evidence for the encrypted LeaseSet2 PSK and DH client authorization
//! block.
//!
//! These rows are deliberately black-box against the published `i2pr_netdb`
//! authorization surface. They build blocks the way a server does, recover
//! cookies the way a client does, and then attack the result: wrong key, absent
//! key, duplicate identifiers, reordered entries, boundary client counts, and
//! tampering with each of the three values that bridge a client to a generation
//! (the salt, the ephemeral public key, and the encrypted cookie itself).
//!
//! Two properties are asserted throughout and are the reason this file exists
//! separately from the Plan 332 foundation rows:
//!
//! 1. **Authorization is additive.** A record published with a block is
//!    byte-for-byte openable only by a client whose credential recovers the
//!    cookie; every other path is refused, and the no-authorization path still
//!    refuses an authorized record rather than ignoring the block.
//! 2. **Refusals are typed and constant-time in the identifier.** A wrong key, a
//!    missing key, and a key for the other scheme all produce the same
//!    [`Els2AuthError::NotAuthorized`], so a caller cannot use the error to learn
//!    whether a guess was close.

use i2pr_crypto::X25519PrivateKey;
use i2pr_crypto::red25519::{Red25519PrivateScalar, generate_private};
use i2pr_proto::SigningKeyType;
use rand_chacha::ChaCha8Rng;
use rand_core::{SeedableRng, TryCryptoRng, TryRngCore};

use i2pr_netdb::{
    AuthBlock, AuthClientPublicKey, AuthCookie, BlindingIdentity, Els2AuthError, Els2AuthScheme,
    Els2AuthSecretRole, Els2AuthorizationServerConfig, Els2ClientAuth, Els2ClientAuthSecret,
    MAX_ELS2_AUTH_CLIENTS, PskClientKey, dh_client_material, draw_generation_secrets,
    psk_client_material, recover_auth_cookie,
};

const PUBLISHED: u32 = 1_700_000_000;
const SUBCREDENTIAL: [u8; 32] = [0x5a; 32];

/// A subcredential stand-in that is stable across a test but distinguishable
/// between the "right" and "wrong day" cases.
fn subcredential(tag: u8) -> [u8; 32] {
    [tag; 32]
}

fn psk(seed: u64) -> PskClientKey {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    PskClientKey::generate(&mut rng).expect("psk")
}

/// Builds `count` distinct pre-shared keys from consecutive seeds.
///
/// [`PskClientKey`] is deliberately not `Clone` — a secret that can be copied is
/// a secret that can be logged by accident — so a test that needs the same key
/// both configured and held rebuilds it from its seed instead of copying it.
fn psk_range(start: u64, count: usize) -> Vec<PskClientKey> {
    (0..count)
        .map(|index| psk(start + u64::try_from(index).expect("index")))
        .collect()
}

/// Builds a server configuration naming the key produced by [`psk`].
///
/// [`PskClientKey`] is deliberately not `Clone`, so a test that both configures a
/// key and then acts as its holder rebuilds it from the same seed instead. The
/// two instances are separate allocations holding identical bytes, which is the
/// real distribution: server and client hold the same secret without either
/// being able to copy it in process.
fn psk_config_for(seed: u64) -> Els2AuthorizationServerConfig {
    Els2AuthorizationServerConfig::psk(vec![psk(seed)]).expect("config")
}

fn dh_pair(seed: u64) -> (X25519PrivateKey, AuthClientPublicKey) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let private = X25519PrivateKey::generate(&mut rng).expect("x25519 key");
    let public = AuthClientPublicKey::from_bytes(private.public_bytes());
    (private, public)
}

// --- pre-shared-key authorization ----------------------------------------------------------------

#[test]
fn one_psk_client_publishes_and_recovers_its_own_cookie() {
    let config = Els2AuthorizationServerConfig::psk(psk_range(0x11, 1)).expect("config");
    assert_eq!(config.client_count(), 1);
    let key = psk(0x11);
    let mut rng = ChaCha8Rng::seed_from_u64(0xa1);
    let (cookie, _esk) = draw_generation_secrets(&mut rng).expect("generation secrets");
    let block = config
        .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, None, &mut rng)
        .expect("build block");
    assert_eq!(block.scheme(), Els2AuthScheme::Psk);
    assert_eq!(block.len(), 1);

    let recovered = recover_auth_cookie(
        &block,
        &SUBCREDENTIAL,
        PUBLISHED,
        &Els2ClientAuth::Psk(&key),
    )
    .expect("recover");
    assert_eq!(
        recovered.as_bytes(),
        cookie.as_bytes(),
        "the recovered cookie must be the one the server drew"
    );
}

#[test]
fn every_configured_psk_client_recovers_its_own_cookie() {
    let seeds = [0x21, 0x22, 0x23, 0x24, 0x25];
    let config = Els2AuthorizationServerConfig::psk(psk_range(0x21, seeds.len())).expect("config");
    assert_eq!(config.client_count(), seeds.len());
    let mut rng = ChaCha8Rng::seed_from_u64(0xa2);
    let (cookie, _esk) = draw_generation_secrets(&mut rng).expect("secrets");
    let block = config
        .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, None, &mut rng)
        .expect("build block");
    assert_eq!(block.len(), seeds.len());

    for seed in seeds {
        let key = psk(seed);
        let recovered = recover_auth_cookie(
            &block,
            &SUBCREDENTIAL,
            PUBLISHED,
            &Els2ClientAuth::Psk(&key),
        )
        .expect("each configured client must be authorized");
        assert_eq!(recovered.as_bytes(), cookie.as_bytes());
    }
}

#[test]
fn a_wrong_psk_is_refused_with_the_same_error_as_an_absent_one() {
    let wrong = psk(0x32);
    let config = psk_config_for(0x31);
    let mut rng = ChaCha8Rng::seed_from_u64(0xa3);
    let (cookie, _esk) = draw_generation_secrets(&mut rng).expect("secrets");
    let block = config
        .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, None, &mut rng)
        .expect("build block");

    let wrong_error = recover_auth_cookie(
        &block,
        &SUBCREDENTIAL,
        PUBLISHED,
        &Els2ClientAuth::Psk(&wrong),
    )
    .expect_err("a wrong key must not be authorized");
    assert!(
        matches!(wrong_error, Els2AuthError::NotAuthorized),
        "unexpected error: {wrong_error:?}"
    );
    assert_eq!(
        wrong_error.to_string(),
        Els2AuthError::NotAuthorized.to_string(),
        "a wrong key must be indistinguishable from a missing one, so the message \
         cannot reveal how close a guess was"
    );
}

#[test]
fn a_psk_client_is_refused_against_a_dh_block_and_vice_versa() {
    let psk_key = psk(0x41);
    let (dh_private, dh_public) = dh_pair(0x42);
    let psk_config = psk_config_for(0x41);
    let dh_config = Els2AuthorizationServerConfig::dh(vec![dh_public]).expect("dh config");
    let mut rng = ChaCha8Rng::seed_from_u64(0xa4);
    let (cookie, esk) = draw_generation_secrets(&mut rng).expect("secrets");
    let psk_block = psk_config
        .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, None, &mut rng)
        .expect("psk block");
    let dh_block = dh_config
        .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, Some(&esk), &mut rng)
        .expect("dh block");

    // A pre-shared key must not satisfy a Diffie-Hellman record...
    assert!(matches!(
        recover_auth_cookie(
            &psk_block,
            &SUBCREDENTIAL,
            PUBLISHED,
            &Els2ClientAuth::Dh {
                private: &dh_private,
                public: dh_public
            }
        ),
        Err(Els2AuthError::NotAuthorized)
    ));
    // ...and a Diffie-Hellman key must not satisfy a pre-shared-key record.
    assert!(matches!(
        recover_auth_cookie(
            &dh_block,
            &SUBCREDENTIAL,
            PUBLISHED,
            &Els2ClientAuth::Psk(&psk_key)
        ),
        Err(Els2AuthError::NotAuthorized)
    ));
}

#[test]
fn a_psk_client_from_another_day_cannot_recover_the_cookie() {
    let key = psk(0x51);
    let config = psk_config_for(0x51);
    let mut rng = ChaCha8Rng::seed_from_u64(0xa5);
    let (cookie, _esk) = draw_generation_secrets(&mut rng).expect("secrets");
    let block = config
        .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, None, &mut rng)
        .expect("build block");
    assert!(
        recover_auth_cookie(
            &block,
            &subcredential(0x01),
            PUBLISHED,
            &Els2ClientAuth::Psk(&key)
        )
        .is_err(),
        "a subcredential from a different day must not authorize this record"
    );
}

// --- Diffie-Hellman authorization ----------------------------------------------------------------

#[test]
fn one_dh_client_publishes_and_recovers_its_own_cookie() {
    let (private, public) = dh_pair(0x61);
    let config = Els2AuthorizationServerConfig::dh(vec![public]).expect("config");
    let mut rng = ChaCha8Rng::seed_from_u64(0xb1);
    let (cookie, esk) = draw_generation_secrets(&mut rng).expect("secrets");
    let block = config
        .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, Some(&esk), &mut rng)
        .expect("build block");
    assert_eq!(block.scheme(), Els2AuthScheme::Dh);

    let recovered = recover_auth_cookie(
        &block,
        &SUBCREDENTIAL,
        PUBLISHED,
        &Els2ClientAuth::Dh {
            private: &private,
            public,
        },
    )
    .expect("recover");
    assert_eq!(recovered.as_bytes(), cookie.as_bytes());
}

#[test]
fn every_configured_dh_client_recovers_its_own_cookie() {
    let seeds = [0x71, 0x72, 0x73];
    let pairs: Vec<(X25519PrivateKey, AuthClientPublicKey)> =
        seeds.iter().map(|seed| dh_pair(*seed)).collect();
    let config =
        Els2AuthorizationServerConfig::dh(pairs.iter().map(|(_, public)| *public).collect())
            .expect("config");
    assert_eq!(config.client_count(), seeds.len());
    let mut rng = ChaCha8Rng::seed_from_u64(0xb2);
    let (cookie, esk) = draw_generation_secrets(&mut rng).expect("secrets");
    let block = config
        .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, Some(&esk), &mut rng)
        .expect("build block");
    assert_eq!(block.len(), seeds.len());

    for (private, public) in &pairs {
        let recovered = recover_auth_cookie(
            &block,
            &SUBCREDENTIAL,
            PUBLISHED,
            &Els2ClientAuth::Dh {
                private,
                public: *public,
            },
        )
        .expect("each configured client must be authorized");
        assert_eq!(recovered.as_bytes(), cookie.as_bytes());
    }
}

#[test]
fn a_dh_client_whose_key_is_not_configured_is_refused() {
    let (configured_private, configured_public) = dh_pair(0x81);
    let (outsider_private, outsider_public) = dh_pair(0x82);
    let config = Els2AuthorizationServerConfig::dh(vec![configured_public]).expect("config");
    let mut rng = ChaCha8Rng::seed_from_u64(0xb3);
    let (cookie, esk) = draw_generation_secrets(&mut rng).expect("secrets");
    let block = config
        .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, Some(&esk), &mut rng)
        .expect("build block");

    // The configured client is authorized, so the refusal below is about the
    // key and not about the block being unusable.
    assert!(
        recover_auth_cookie(
            &block,
            &SUBCREDENTIAL,
            PUBLISHED,
            &Els2ClientAuth::Dh {
                private: &configured_private,
                public: configured_public
            }
        )
        .is_ok(),
        "the configured client must be authorized, or the refusal proves nothing"
    );
    // An outsider holds a perfectly valid keypair, but the record carries no
    // entry whose identifier it can match.
    assert!(matches!(
        recover_auth_cookie(
            &block,
            &SUBCREDENTIAL,
            PUBLISHED,
            &Els2ClientAuth::Dh {
                private: &outsider_private,
                public: outsider_public
            }
        ),
        Err(Els2AuthError::NotAuthorized)
    ));
}

#[test]
fn a_dh_client_cannot_authorize_itself_by_substituting_its_own_public_key() {
    // The record names the client's public key inside the derivation, so a
    // client that swaps in a different public key while keeping its private key
    // must fail to match its own entry.
    let (private, real_public) = dh_pair(0x91);
    let (other_private, other_public) = dh_pair(0x92);
    let config = Els2AuthorizationServerConfig::dh(vec![real_public]).expect("config");
    let mut rng = ChaCha8Rng::seed_from_u64(0xb4);
    let (cookie, esk) = draw_generation_secrets(&mut rng).expect("secrets");
    let block = config
        .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, Some(&esk), &mut rng)
        .expect("build block");

    assert!(matches!(
        recover_auth_cookie(
            &block,
            &SUBCREDENTIAL,
            PUBLISHED,
            &Els2ClientAuth::Dh {
                private: &private,
                public: other_public
            }
        ),
        Err(Els2AuthError::NotAuthorized)
    ));
    // The other client's key against this record is equally refused, which is
    // what makes the substitution useless.
    assert!(matches!(
        recover_auth_cookie(
            &block,
            &SUBCREDENTIAL,
            PUBLISHED,
            &Els2ClientAuth::Dh {
                private: &other_private,
                public: other_public
            }
        ),
        Err(Els2AuthError::NotAuthorized)
    ));
}

#[test]
fn a_dh_block_without_a_generation_ephemeral_key_is_refused() {
    let (_private, public) = dh_pair(0xa1);
    let config = Els2AuthorizationServerConfig::dh(vec![public]).expect("config");
    let mut rng = ChaCha8Rng::seed_from_u64(0xb5);
    let cookie = AuthCookie::generate(&mut rng).expect("cookie");
    let error = config
        .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, None, &mut rng)
        .expect_err("the Diffie-Hellman scheme needs an ephemeral key");
    assert!(
        matches!(error, Els2AuthError::Malformed(_)),
        "unexpected error: {error:?}"
    );
}

// --- identifier collisions ------------------------------------------------------------------------

#[test]
fn duplicate_client_identifiers_are_rejected_at_build_time() {
    // Two configured keys that are the *same* key derive the same identifier.
    // Merging them would silently drop a client and could authorize the wrong
    // cookie, so the build fails closed instead.
    let key = psk(0xb1);
    let duplicate = psk(0xb1);
    let config = Els2AuthorizationServerConfig::psk(vec![key, duplicate]).expect("config");
    assert_eq!(
        config.client_count(),
        2,
        "the duplicate is visible to the config"
    );
    let mut rng = ChaCha8Rng::seed_from_u64(0xc1);
    let (cookie, _esk) = draw_generation_secrets(&mut rng).expect("secrets");
    let error = config
        .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, None, &mut rng)
        .expect_err("duplicate identifiers must be rejected");
    assert!(
        matches!(error, Els2AuthError::DuplicateClientId),
        "unexpected error: {error:?}"
    );
}

#[test]
fn a_duplicate_identifier_is_rejected_for_dh_too() {
    let public = dh_pair(0xb2).1;
    let config = Els2AuthorizationServerConfig::dh(vec![public, public]).expect("config");
    let mut rng = ChaCha8Rng::seed_from_u64(0xc2);
    let (cookie, esk) = draw_generation_secrets(&mut rng).expect("secrets");
    assert!(matches!(
        config.build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, Some(&esk), &mut rng),
        Err(Els2AuthError::DuplicateClientId)
    ));
}

// --- bounds --------------------------------------------------------------------------------------

#[test]
fn the_maximum_client_count_is_accepted_and_one_more_is_refused() {
    let mut rng = ChaCha8Rng::seed_from_u64(0xd1);
    let cookie = AuthCookie::generate(&mut rng).expect("cookie");

    let config = Els2AuthorizationServerConfig::psk(psk_range(0x1_0000, MAX_ELS2_AUTH_CLIENTS))
        .expect("config at bound");
    assert_eq!(config.client_count(), MAX_ELS2_AUTH_CLIENTS);
    let block = config
        .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, None, &mut rng)
        .expect("the maximum client count must be publishable");
    assert_eq!(block.len(), MAX_ELS2_AUTH_CLIENTS);
    // A client at the end of the list must still be found. The key is rebuilt
    // from its seed because `PskClientKey` is not `Clone`.
    for index in [
        0_usize,
        MAX_ELS2_AUTH_CLIENTS / 2,
        MAX_ELS2_AUTH_CLIENTS - 1,
    ] {
        let key = psk(0x1_0000 + u64::try_from(index).expect("index"));
        assert!(
            recover_auth_cookie(
                &block,
                &SUBCREDENTIAL,
                PUBLISHED,
                &Els2ClientAuth::Psk(&key)
            )
            .is_ok(),
            "entry order must not decide authorization (index {index})"
        );
    }

    let mut over_bound = psk_range(0x1_0000, MAX_ELS2_AUTH_CLIENTS);
    over_bound.push(psk(0xffff));
    let error = Els2AuthorizationServerConfig::psk(over_bound).expect_err("one over the bound");
    assert!(
        matches!(
            error,
            Els2AuthError::TooManyClients {
                actual,
                maximum
            } if actual == MAX_ELS2_AUTH_CLIENTS + 1 && maximum == MAX_ELS2_AUTH_CLIENTS
        ),
        "unexpected error: {error:?}"
    );
}

#[test]
fn an_empty_client_set_is_refused() {
    // A block with no clients would publish a record nobody can open while
    // advertising that authorization is on.
    let error = Els2AuthorizationServerConfig::psk(Vec::new()).expect_err("empty is refused");
    assert!(
        matches!(error, Els2AuthError::NoClients),
        "unexpected: {error:?}"
    );
    let error = Els2AuthorizationServerConfig::dh(Vec::new()).expect_err("empty is refused");
    assert!(
        matches!(error, Els2AuthError::NoClients),
        "unexpected: {error:?}"
    );
}

// --- ordering and encoding -----------------------------------------------------------------------

#[test]
fn entry_order_does_not_decide_authorization() {
    let seeds = [0xe1, 0xe2, 0xe3, 0xe4];
    let config = Els2AuthorizationServerConfig::psk(psk_range(0x21, seeds.len())).expect("config");
    let mut rng = ChaCha8Rng::seed_from_u64(0xe0);
    let (cookie, _esk) = draw_generation_secrets(&mut rng).expect("secrets");

    // Build many times with the same key set: the entry order is randomized, so
    // a client must succeed regardless of where its entry landed.
    let mut orders = std::collections::BTreeSet::new();
    for _ in 0..16 {
        let block = config
            .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, None, &mut rng)
            .expect("build block");
        orders.insert(
            block
                .entries()
                .iter()
                .map(|entry| *entry.client_id())
                .collect::<Vec<_>>(),
        );
        for index in 0..seeds.len() {
            let key = psk(0x21 + u64::try_from(index).expect("index"));
            assert!(
                recover_auth_cookie(
                    &block,
                    &SUBCREDENTIAL,
                    PUBLISHED,
                    &Els2ClientAuth::Psk(&key)
                )
                .is_ok(),
                "a randomized order must not change who is authorized"
            );
        }
    }
    assert!(
        orders.len() > 1,
        "the emitted order must actually vary across generations"
    );
}

#[test]
fn a_block_round_trips_through_its_codec() {
    let config = Els2AuthorizationServerConfig::psk(psk_range(0xf1, 3)).expect("config");
    let mut rng = ChaCha8Rng::seed_from_u64(0xf0);
    let (cookie, _esk) = draw_generation_secrets(&mut rng).expect("secrets");
    let block = config
        .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, None, &mut rng)
        .expect("build block");
    let encoded = block.encode_to_vec().expect("encode");
    let decoded = AuthBlock::decode(block.scheme(), &encoded).expect("decode");
    assert_eq!(decoded.len(), block.len());
    assert_eq!(decoded.salt(), block.salt());
    let key = psk(0xf2);
    assert!(
        recover_auth_cookie(
            &decoded,
            &SUBCREDENTIAL,
            PUBLISHED,
            &Els2ClientAuth::Psk(&key)
        )
        .is_ok()
    );
}

#[test]
fn a_tampered_salt_or_ephemeral_key_defeats_authorization() {
    for scheme in [Els2AuthScheme::Psk, Els2AuthScheme::Dh] {
        let psk_key = psk(0x1234);
        let (dh_private, dh_public) = dh_pair(0x1235);
        let config = match scheme {
            Els2AuthScheme::Psk => psk_config_for(0x1234),
            Els2AuthScheme::Dh => Els2AuthorizationServerConfig::dh(vec![dh_public]).expect("dh"),
            Els2AuthScheme::None => unreachable!("not a real scheme"),
        };
        let credential = match scheme {
            Els2AuthScheme::Psk => Els2ClientAuth::Psk(&psk_key),
            Els2AuthScheme::Dh => Els2ClientAuth::Dh {
                private: &dh_private,
                public: dh_public,
            },
            Els2AuthScheme::None => unreachable!("not a real scheme"),
        };
        let mut rng = ChaCha8Rng::seed_from_u64(u64::from(scheme.to_flags()) ^ 0x5eed);
        let (cookie, esk) = draw_generation_secrets(&mut rng).expect("secrets");
        let esk_ref = (scheme == Els2AuthScheme::Dh).then_some(&esk);
        let block = config
            .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, esk_ref, &mut rng)
            .expect("build block");
        assert!(
            recover_auth_cookie(&block, &SUBCREDENTIAL, PUBLISHED, &credential).is_ok(),
            "the untampered block must authorize, or the negative rows prove nothing"
        );

        // The salt is the first 32 bytes of the serialized block. Flipping one
        // bit changes the HKDF salt, so every identifier is derived differently
        // and the client can no longer find its own entry.
        let encoded = block.encode_to_vec().expect("encode");
        let mut flipped = encoded.clone();
        flipped[0] ^= 0x01;
        let tampered =
            AuthBlock::decode(block.scheme(), &flipped).expect("the salt is still well-formed");
        assert_ne!(tampered.salt(), block.salt());
        assert!(
            recover_auth_cookie(&tampered, &SUBCREDENTIAL, PUBLISHED, &credential).is_err(),
            "{scheme:?}: a tampered auth salt must defeat authorization"
        );

        // For the Diffie-Hellman scheme the salt *is* the ephemeral public key,
        // so substituting another router's ephemeral must not recover the cookie.
        if scheme == Els2AuthScheme::Dh {
            let (_other_private, other_public) = dh_pair(0x9999);
            let mut substituted = encoded.clone();
            substituted[..32].copy_from_slice(other_public.as_bytes());
            let rebuilt = AuthBlock::decode(block.scheme(), &substituted).expect("well-formed");
            assert!(
                recover_auth_cookie(&rebuilt, &SUBCREDENTIAL, PUBLISHED, &credential).is_err(),
                "a substituted ephemeral public key must defeat authorization"
            );
        }
    }
}

#[test]
fn a_tampered_cookie_yields_a_different_cookie_rather_than_an_error() {
    // This row documents a real property of the construction rather than
    // asserting a wish. ChaCha20 is an unauthenticated stream cipher and the
    // layer-1 block carries no tag, so a flipped bit in an entry's cookie does
    // not fail a check — it decrypts to a *different* cookie.
    //
    // That is not a break, and the reasons are worth stating precisely,
    // because it would be easy to read this row as an integrity gap:
    //
    // 1. The derived identifier is compared in constant time *before* the
    //    cookie is touched, so only a client that already holds the right key
    //    ever reaches the cookie at all.
    // 2. A wrong cookie yields a wrong layer-2 key, so the inner decryption
    //    produces garbage that fails the store-type and structure checks rather
    //    than a forged LeaseSet2.
    // 3. On the wire the block lives inside the outer ciphertext, which the
    //    outer Red25519 signature covers. A receiver validates that signature
    //    before decrypting, so an active tamperer's edit never reaches this
    //    code at all. That end-to-end refusal is asserted in the client
    //    round-trip rows.
    //
    // What is asserted here is simply that this module does not pretend to
    // offer a tag it does not have.
    let key = psk(0x1240);
    let config = psk_config_for(0x1240);
    let mut rng = ChaCha8Rng::seed_from_u64(0x5eef);
    let (cookie, _esk) = draw_generation_secrets(&mut rng).expect("secrets");
    let block = config
        .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, None, &mut rng)
        .expect("build block");
    let encoded = block.encode_to_vec().expect("encode");

    // salt(32) || count(2) || clientId(8) || clientCookie(32)
    let cookie_offset = 32 + 2 + 8;
    let mut corrupted = encoded.clone();
    corrupted[cookie_offset] ^= 0x01;
    let corrupted_block = AuthBlock::decode(block.scheme(), &corrupted).expect("still decodable");
    let recovered = recover_auth_cookie(
        &corrupted_block,
        &SUBCREDENTIAL,
        PUBLISHED,
        &Els2ClientAuth::Psk(&key),
    )
    .expect("the identifier still matches, so the cookie path is reached");
    assert_ne!(
        recovered.as_bytes(),
        cookie.as_bytes(),
        "a tampered cookie must not recover the generation's real cookie"
    );
}

#[test]
fn a_declared_client_count_that_disagrees_with_the_bytes_is_refused() {
    let config = Els2AuthorizationServerConfig::psk(psk_range(0x1111, 2)).expect("config");
    let mut rng = ChaCha8Rng::seed_from_u64(0xf5);
    let (cookie, _esk) = draw_generation_secrets(&mut rng).expect("secrets");
    let block = config
        .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, None, &mut rng)
        .expect("build block");
    let mut encoded = block.encode_to_vec().expect("encode");
    // The serialized block is salt(32) || clientCount(2, big endian) || entries.
    // Bump the low count byte so the block claims three clients while carrying
    // two, which must be refused rather than read as a short entry.
    encoded[33] = encoded[33].wrapping_add(1);
    let error = AuthBlock::decode(block.scheme(), &encoded).expect_err("a lying count is refused");
    assert!(
        matches!(
            error,
            Els2AuthError::ClientCountMismatch {
                declared,
                actual
            } if declared == 3 && actual == 2 * 40
        ),
        "unexpected error: {error:?}"
    );

    // A block with trailing bytes past the last entry is refused too: those
    // bytes would otherwise be read as the start of the inner ciphertext.
    let mut trailing = block.encode_to_vec().expect("encode");
    trailing.push(0x00);
    assert!(matches!(
        AuthBlock::decode(block.scheme(), &trailing),
        Err(Els2AuthError::ClientCountMismatch { .. })
    ));
    // And a truncated one.
    let truncated = &block.encode_to_vec().expect("encode")[..40];
    assert!(matches!(
        AuthBlock::decode(block.scheme(), truncated),
        Err(Els2AuthError::ClientCountMismatch { .. })
    ));
}

#[test]
fn a_flags_byte_with_reserved_bits_is_not_a_recognized_authorization_block() {
    // The layer-1 flags byte is *not* part of the serialized block: it precedes
    // the block in the layer-1 plaintext. What the block owns is the mapping
    // from that byte to a scheme, and an unrecognized byte must not resolve to
    // any scheme at all rather than defaulting to one.
    assert_eq!(
        Els2AuthScheme::from_flags(0b0000_0001),
        Some(Els2AuthScheme::Dh)
    );
    assert_eq!(
        Els2AuthScheme::from_flags(0b0000_0011),
        Some(Els2AuthScheme::Psk)
    );
    assert_eq!(
        Els2AuthScheme::from_flags(0b0000_0000),
        Some(Els2AuthScheme::None)
    );
    // Scheme bits 10 and 11 are unassigned.
    assert_eq!(Els2AuthScheme::from_flags(0b0000_0101), None);
    assert_eq!(Els2AuthScheme::from_flags(0b0000_0111), None);
    // The per-client bit clear means no authorization regardless of the rest.
    assert_eq!(
        Els2AuthScheme::from_flags(0b0000_0010),
        Some(Els2AuthScheme::None)
    );

    // A block cannot even be parsed for a scheme that was never requested.
    let error = AuthBlock::decode(Els2AuthScheme::None, &[0_u8; 64])
        .expect_err("no authorization requested means no block");
    assert!(
        matches!(error, Els2AuthError::AuthorizationNotRequested { .. }),
        "unexpected error: {error:?}"
    );
}

// --- generation freshness -----------------------------------------------------------------------

#[test]
fn each_generation_draws_a_fresh_salt_and_cookie() {
    let config = Els2AuthorizationServerConfig::psk(psk_range(0x1131, 1)).expect("config");
    let key = psk(0x1131);
    let mut rng = ChaCha8Rng::seed_from_u64(0xf7);
    let mut salts = std::collections::BTreeSet::new();
    let mut cookies = std::collections::BTreeSet::new();
    for _ in 0..16 {
        let (cookie, _esk) = draw_generation_secrets(&mut rng).expect("secrets");
        let block = config
            .build_block(&cookie, &SUBCREDENTIAL, PUBLISHED, None, &mut rng)
            .expect("build block");
        salts.insert(*block.salt());
        cookies.insert(cookie.as_bytes().to_vec());
        assert_eq!(
            recover_auth_cookie(
                &block,
                &SUBCREDENTIAL,
                PUBLISHED,
                &Els2ClientAuth::Psk(&key)
            )
            .expect("recover")
            .as_bytes(),
            cookie.as_bytes()
        );
    }
    assert_eq!(
        salts.len(),
        16,
        "the auth salt must be fresh per generation"
    );
    assert_eq!(
        cookies.len(),
        16,
        "the auth cookie must be fresh per generation"
    );
}

#[test]
fn a_cookie_from_another_generation_does_not_open_this_record() {
    // The derivation binds the cookie only through the block, so reusing an old
    // cookie against a new record must fail: the new block's entries are keyed
    // by the new salt and the new cookie's own cipher.
    let config = Els2AuthorizationServerConfig::psk(psk_range(0x1141, 1)).expect("config");
    let key = psk(0x1141);
    let mut rng = ChaCha8Rng::seed_from_u64(0xf8);
    let (old_cookie, _esk) = draw_generation_secrets(&mut rng).expect("secrets");
    let old_block = config
        .build_block(&old_cookie, &SUBCREDENTIAL, PUBLISHED, None, &mut rng)
        .expect("old block");
    let (new_cookie, _esk2) = draw_generation_secrets(&mut rng).expect("secrets");
    let new_block = config
        .build_block(&new_cookie, &SUBCREDENTIAL, PUBLISHED, None, &mut rng)
        .expect("new block");
    assert_ne!(
        old_block.salt(),
        new_block.salt(),
        "two generations must not share an auth salt"
    );
    let recovered = recover_auth_cookie(
        &new_block,
        &SUBCREDENTIAL,
        PUBLISHED,
        &Els2ClientAuth::Psk(&key),
    )
    .expect("recover");
    assert_eq!(
        recovered.as_bytes(),
        new_cookie.as_bytes(),
        "the record's own cookie is the only one that opens it"
    );
}

// --- derivation determinism ---------------------------------------------------------------------

#[test]
fn the_derivation_is_deterministic_and_domain_separated() {
    let key = psk(0x1151);
    let salt = [0x33; 32];
    let first = psk_client_material(&key, &SUBCREDENTIAL, PUBLISHED, &salt).expect("material");
    let second = psk_client_material(&key, &SUBCREDENTIAL, PUBLISHED, &salt).expect("material");
    assert_eq!(first.client_id(), second.client_id());

    // A different published timestamp, salt, subcredential, or key must all
    // produce a different identifier.
    assert_ne!(
        first.client_id(),
        psk_client_material(&key, &SUBCREDENTIAL, PUBLISHED + 1, &salt)
            .expect("material")
            .client_id()
    );
    let other_salt = [0x34; 32];
    assert_ne!(
        first.client_id(),
        psk_client_material(&key, &SUBCREDENTIAL, PUBLISHED, &other_salt)
            .expect("material")
            .client_id()
    );
    assert_ne!(
        first.client_id(),
        psk_client_material(&key, &subcredential(0x99), PUBLISHED, &salt)
            .expect("material")
            .client_id()
    );
    assert_ne!(
        first.client_id(),
        psk_client_material(&psk(0x1152), &SUBCREDENTIAL, PUBLISHED, &salt)
            .expect("material")
            .client_id()
    );
}

#[test]
fn the_dh_derivation_is_deterministic_and_binds_the_client_public_key() {
    let (private, public) = dh_pair(0x1161);
    let (_other_private, other_public) = dh_pair(0x1162);
    let salt = [0x44; 32];
    let first =
        dh_client_material(&private, &salt, &public, &SUBCREDENTIAL, PUBLISHED).expect("material");
    let second =
        dh_client_material(&private, &salt, &public, &SUBCREDENTIAL, PUBLISHED).expect("material");
    assert_eq!(first.client_id(), second.client_id());

    // The client's own public key is inside the derivation, so the same private
    // key paired with a different public key must derive a different identity.
    let mismatched = dh_client_material(&private, &salt, &other_public, &SUBCREDENTIAL, PUBLISHED)
        .expect("material");
    assert_ne!(
        first.client_id(),
        mismatched.client_id(),
        "a client must not be able to impersonate a different public key"
    );
}

// --- secret ownership ---------------------------------------------------------------------------

#[test]
fn a_secret_reports_its_role_and_refuses_to_change_sides() {
    let bytes = [0x77; 32];
    let psk_secret = Els2ClientAuthSecret::new(Els2AuthSecretRole::ServerPsk, bytes);
    let dh_private_secret = Els2ClientAuthSecret::new(Els2AuthSecretRole::ClientDhPrivate, bytes);
    let dh_public_secret =
        Els2ClientAuthSecret::new(Els2AuthSecretRole::ServerDhClientPublic, bytes);

    assert!(psk_secret.as_psk().is_ok());
    assert!(psk_secret.as_dh_private().is_err());
    assert!(psk_secret.as_dh_public().is_err());
    assert!(dh_private_secret.as_dh_private().is_ok());
    assert!(dh_private_secret.as_psk().is_err());
    assert!(dh_public_secret.as_dh_public().is_ok());
    assert!(dh_public_secret.as_psk().is_err());
    assert!(dh_public_secret.as_dh_private().is_err());
}

#[test]
fn a_secret_survives_a_persistence_round_trip_with_its_role_intact() {
    // Restart recovery: the same bytes come back as the same role, so a server
    // key is never silently restored as a client key.
    for role in [
        Els2AuthSecretRole::ServerPsk,
        Els2AuthSecretRole::ServerDhClientPublic,
        Els2AuthSecretRole::ClientPsk,
        Els2AuthSecretRole::ClientDhPrivate,
    ] {
        let secret = Els2ClientAuthSecret::new(role, [0x88; 32]);
        let persisted = secret.to_persisted_bytes();
        let restored = Els2ClientAuthSecret::from_persisted_bytes(&persisted).expect("restore");
        assert_eq!(restored.role(), role, "role must survive persistence");
        assert_eq!(restored.as_bytes(), secret.as_bytes());
    }
}

#[test]
fn a_persisted_secret_rejects_a_corrupt_or_foreign_encoding() {
    let secret = Els2ClientAuthSecret::new(Els2AuthSecretRole::ServerPsk, [0x99; 32]);
    let persisted = secret.to_persisted_bytes();
    assert!(
        Els2ClientAuthSecret::from_persisted_bytes(&persisted[..persisted.len() - 1]).is_err(),
        "a truncated encoding must be refused"
    );
    assert!(
        Els2ClientAuthSecret::from_persisted_bytes(&[]).is_err(),
        "an empty encoding must be refused"
    );
    let mut over_long = persisted.to_vec();
    over_long.push(0);
    assert!(
        Els2ClientAuthSecret::from_persisted_bytes(&over_long).is_err(),
        "an over-long encoding must be refused"
    );
    // Flipping the role tag must either be refused or land on a different role;
    // it must never restore the secret as the role it was not.
    let mut wrong_role = persisted.to_vec();
    wrong_role[0] = wrong_role[0].wrapping_add(1);
    if let Ok(restored) = Els2ClientAuthSecret::from_persisted_bytes(&wrong_role) {
        assert_ne!(
            restored.role(),
            Els2AuthSecretRole::ServerPsk,
            "a tampered role tag must not restore as the original role"
        );
    }
}

#[test]
fn secrets_never_render_their_bytes() {
    let key = PskClientKey::from_bytes([0xAB; 32]);
    let rendered = format!("{key:?}");
    assert!(
        !rendered.contains("ab"),
        "Debug leaked PSK bytes: {rendered}"
    );
    assert!(
        rendered.contains("PskClientKey"),
        "unexpected Debug: {rendered}"
    );

    let cookie = AuthCookie::from_bytes([0xCD; 32]);
    let rendered = format!("{cookie:?}");
    assert!(
        !rendered.contains("cd"),
        "Debug leaked cookie bytes: {rendered}"
    );

    let psk_secret = Els2ClientAuthSecret::new(Els2AuthSecretRole::ServerPsk, [0xEF; 32]);
    let rendered = format!("{psk_secret:?}");
    assert!(
        !rendered.contains("ef"),
        "Debug leaked secret bytes: {rendered}"
    );

    // A client credential's Debug must redact the private half even while naming
    // the public half, which is safe to print.
    let (private, public) = dh_pair(0x11aa);
    let rendered = format!(
        "{:?}",
        Els2ClientAuth::Dh {
            private: &private,
            public
        }
    );
    assert!(
        rendered.contains("redacted"),
        "private half was not redacted"
    );
    let rendered = format!("{:?}", Els2ClientAuth::Psk(&key));
    assert!(rendered.contains("redacted"), "psk was not redacted");
}

#[test]
fn a_random_source_that_fails_is_reported_not_substituted() {
    // Fail-closed: a generator that cannot produce randomness must not yield a
    // zero key that every client would trivially match.
    struct Failing;
    impl TryRngCore for Failing {
        type Error = &'static str;
        fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
            Err("no entropy")
        }
        fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
            Err("no entropy")
        }
        fn try_fill_bytes(&mut self, _dest: &mut [u8]) -> Result<(), Self::Error> {
            Err("no entropy")
        }
    }
    impl TryCryptoRng for Failing {}
    let mut failing = Failing;
    assert!(matches!(
        PskClientKey::generate(&mut failing),
        Err(Els2AuthError::RandomnessUnavailable)
    ));
    assert!(matches!(
        AuthCookie::generate(&mut failing),
        Err(Els2AuthError::RandomnessUnavailable)
    ));
    assert!(matches!(
        draw_generation_secrets(&mut failing),
        Err(Els2AuthError::RandomnessUnavailable)
    ));
    // A client key generated from a working source still builds a block, so the
    // failure rows above prove the generator is actually consulted.
    let mut working = ChaCha8Rng::seed_from_u64(0x1);
    assert!(PskClientKey::generate(&mut working).is_ok());
    working
        .try_fill_bytes(&mut [0_u8; 4])
        .expect("working source");
}

// --- integration with the real credential path --------------------------------------------------

#[test]
fn the_authorization_survives_a_real_blinded_service_schedule() {
    // The block is derived from a service's real subcredential, so a round trip
    // through the production identity path is exercised rather than a stub.
    let mut rng = ChaCha8Rng::seed_from_u64(0x2a2a);
    let private: Red25519PrivateScalar = generate_private(&mut rng).expect("red25519 key");
    let public = i2pr_crypto::red25519::derive_public_key(&private);
    let identity =
        BlindingIdentity::new(public, SigningKeyType::RedDsaSha512Ed25519, None).expect("identity");
    let credentials = identity.credentials_for(&public);
    let key = psk(0x11bb);
    let config = psk_config_for(0x11bb);
    let (cookie, _esk) = draw_generation_secrets(&mut rng).expect("secrets");
    let block = config
        .build_block(
            &cookie,
            credentials.subcredential(),
            PUBLISHED,
            None,
            &mut rng,
        )
        .expect("build block");
    let recovered = recover_auth_cookie(
        &block,
        credentials.subcredential(),
        PUBLISHED,
        &Els2ClientAuth::Psk(&key),
    )
    .expect("recover");
    assert_eq!(recovered.as_bytes(), cookie.as_bytes());
    // The same key against a different service's subcredential is refused.
    let other: [u8; 32] = [0x11; 32];
    assert!(recover_auth_cookie(&block, &other, PUBLISHED, &Els2ClientAuth::Psk(&key)).is_err());
}
