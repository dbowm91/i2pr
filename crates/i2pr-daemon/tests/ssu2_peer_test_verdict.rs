//! Plan 528's lane -- the router's peer-test verdict must not be thrown away.
//!
//! The tree runs a **complete** three-party SSU2 peer test whose verdict
//! carries exactly what the third party observed, and then throws every
//! verdict away except "the observation matched the router's own bound
//! address and two peers corroborated". A **disagreement** became an
//! **error**, not a **fact**.
//!
//! The SSU2 specification states the peer-test result as a table over the
//! messages Alice received -- 4, 5 and 7 -- and the row "4 received, 5 not
//! received" is FIREWALLED. The pinned i2pd 2.61.0 concludes FIREWALLED
//! from a **single** peer test. So the tree's model has to be able to
//! express that, and the verdict has to reach the one tracker the product
//! reads.
//!
//! The lane drives the **producer**, not an injected flag:
//!
//! 1. the specification's result table is the tree's authority, and every
//!    combination of the three messages is a row of it;
//! 2. the FIREWALLED row is this router's own firewalled fact, and it
//!    reaches the tracker;
//! 3. an OK row proves nothing **by itself**: the corroboration is the
//!    address agreement, and that arrives through the comparison;
//! 4. the specification's ambiguous row and its unknown row claim nothing;
//! 5. a disagreement between the observed address and the router's own
//!    bound socket is the same firewalled fact;
//! 6. the daemon's firewalled gate fires on the state the tracker really
//!    holds, and the flag is never injected;
//! 7. a router with no verdict asks nobody;
//! 8. the **corroboration** floor is unchanged: three distinct supporting
//!    classes are still needed for `Reachable`.
//!
//! Assertions are counts, states and digests only: no key, tag, peer hash,
//! signature or payload is ever printed.

#![forbid(unsafe_code)]

use std::time::{SystemTime, UNIX_EPOCH};

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::introducer_request::{
    IntroducerRequestDecision, IntroducerRequestOwner, IntroducerRequestRefusal,
};
use i2pr_proto::{Date, Mapping, RouterAddress};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, IntroKey, PEER_TEST_RESULT_TABLE,
    PeerTestResult, Ssu2Endpoint, Ssu2IdentityMaterial, Ssu2PeerTestMessages, Ssu2PeerTestVerdict,
    Ssu2RuntimeConfig, Ssu2RuntimeService, Ssu2SocketConfig, peer_test_result,
};
use i2pr_transport::{AddressFamily, ReachabilityPolicy};

use rand_core::{OsRng, TryRngCore};

fn wall_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(1_700_000_000)
}

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(1_700_000_000_000)
}

fn i2p_b64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let mut n: u32 = 0;
        for byte in chunk {
            n = (n << 8) | u32::from(*byte);
        }
        n <<= 8 * (3 - chunk.len());
        let digits = match chunk.len() {
            1 => 2,
            2 => 3,
            _ => 4,
        };
        for index in 0..digits {
            output.push(ALPHABET[((n >> (18 - 6 * index)) & 0x3f) as usize] as char);
        }
        for _ in digits..4 {
            output.push('=');
        }
    }
    output
}

/// One router's own keys and its signed record.
struct LocalKeys {
    hash: i2pr_proto::Hash,
    static_bytes: [u8; 32],
    intro: IntroKey,
    router_info: Vec<u8>,
}

fn make_local_keys(port_placeholder: u16) -> LocalKeys {
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
    let hash = bundle.identity().hash().expect("hash");
    let static_secret = X25519PrivateKey::generate(&mut OsRng).expect("static");
    let static_bytes = *static_secret.secret_bytes();
    let mut intro_bytes = [0_u8; 32];
    loop {
        OsRng.try_fill_bytes(&mut intro_bytes).expect("rng");
        if intro_bytes.iter().any(|byte| *byte != 0) {
            break;
        }
    }
    let intro = IntroKey::new(intro_bytes);
    let options = Mapping::from_entries(vec![
        ("host".to_string(), "127.0.0.1".to_string()),
        ("port".to_string(), port_placeholder.to_string()),
        ("v".to_string(), "2".to_string()),
        (
            "s".to_string(),
            i2p_b64_encode(&static_secret.public_bytes()),
        ),
        ("i".to_string(), i2p_b64_encode(&intro_bytes)),
    ])
    .expect("options");
    let address = RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .expect("address");
    let info = bundle
        .sign_router_info(
            Date::from_millis(wall_secs().saturating_mul(1000)),
            vec![address],
            Vec::new(),
            Mapping::empty(),
        )
        .expect("sign");
    let router_info = info
        .encode_to_vec(i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .expect("encode");
    LocalKeys {
        hash,
        static_bytes,
        intro,
        router_info,
    }
}

/// A runtime service that owns no socket: the comparison has no bound
/// address to make against.
fn unbound_service() -> Ssu2RuntimeService {
    let keys = make_local_keys(1);
    Ssu2RuntimeService::new(
        Ssu2RuntimeConfig::default(),
        Ssu2IdentityMaterial {
            router_hash: keys.hash,
            static_secret_bytes: keys.static_bytes,
            intro_key: keys.intro,
            router_info: keys.router_info.clone(),
        },
    )
    .expect("service")
}

/// A runtime service bound to a real loopback IPv4 socket.
async fn bound_service() -> (Ssu2RuntimeService, i2pr_runtime::Ssu2ServiceHandle) {
    let keys = make_local_keys(1);
    let service = Ssu2RuntimeService::new(
        Ssu2RuntimeConfig::default(),
        Ssu2IdentityMaterial {
            router_hash: keys.hash,
            static_secret_bytes: keys.static_bytes,
            intro_key: keys.intro,
            router_info: keys.router_info.clone(),
        },
    )
    .expect("service");
    let token: &'static CancellationToken = Box::leak(Box::new(CancellationToken::new()));
    let scope = ChildScope::for_test(token, ChildFailurePolicy::FailParent);
    let handle = service
        .start(
            &scope,
            Ssu2SocketConfig {
                ipv4: Some("127.0.0.1:0".parse().expect("loopback")),
                ipv6: None,
            },
        )
        .await
        .expect("bind");
    (service, handle)
}

fn endpoint(last: u8, port: u16) -> Ssu2Endpoint {
    Ssu2Endpoint::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, last)),
        port,
    )
    .expect("endpoint")
}

/// The three messages the specification's result table is over.
fn messages(msg4: bool, msg5: bool, msg7: bool) -> Ssu2PeerTestMessages {
    Ssu2PeerTestMessages { msg4, msg5, msg7 }
}

#[tokio::test]
async fn the_specification_table_is_the_trees_authority() {
    // Every combination of the three messages is a row of the
    // specification's table, and the tree's lookup is total.
    assert_eq!(PEER_TEST_RESULT_TABLE.len(), 8);
    for msg4 in [false, true] {
        for msg5 in [false, true] {
            for msg7 in [false, true] {
                let row = PEER_TEST_RESULT_TABLE
                    .iter()
                    .find(|row| row.msg4 == msg4 && row.msg5 == msg5 && row.msg7 == msg7)
                    .expect("every combination is a row");
                assert_eq!(
                    peer_test_result(msg4, msg5, msg7),
                    row.result,
                    "the lookup must answer from the table, never by guesswork"
                );
            }
        }
    }
    // The specification's FIREWALLED row: message 4 arrived and message 5
    // did not, so the helper could not reach this router directly.
    assert_eq!(
        peer_test_result(true, false, false),
        PeerTestResult::Firewalled,
        "the specification's FIREWALLED row"
    );
    // The specification's ambiguous row is read conservatively.
    assert_eq!(
        peer_test_result(true, false, true),
        PeerTestResult::Ambiguous,
        "the specification's FIREWALLED-or-SYMNAT row"
    );
}

#[tokio::test]
async fn a_firewalled_row_is_the_routers_firewalled_fact() {
    let (service, _handle) = bound_service().await;

    let verdict = service.note_peer_test_verdict(AddressFamily::Ipv4, messages(true, false, false));

    assert_eq!(
        verdict,
        Ssu2PeerTestVerdict::Firewalled,
        "the specification's FIREWALLED row is the router's own firewalled fact"
    );
    assert_eq!(
        service.snapshot().reachability,
        i2pr_transport::ReachabilityState::Firewalled,
        "the verdict reaches the one tracker the product reads"
    );
    let evidence = service.reachability_evidence();
    assert_eq!(
        evidence.corroboration, 0,
        "a contradiction supports nothing: one observation is one fact"
    );
}

#[tokio::test]
async fn an_ok_row_proves_nothing_by_itself() {
    let (service, _handle) = bound_service().await;

    let verdict = service.note_peer_test_verdict(AddressFamily::Ipv4, messages(true, true, true));

    assert_eq!(
        verdict,
        Ssu2PeerTestVerdict::Reachable,
        "the helper could reach this router directly"
    );
    assert_eq!(
        service.snapshot().reachability,
        i2pr_transport::ReachabilityState::Unknown,
        "the OK row is not a corroboration: the address agreement is"
    );
}

#[tokio::test]
async fn an_ambiguous_row_claims_nothing() {
    let (service, _handle) = bound_service().await;

    let verdict = service.note_peer_test_verdict(AddressFamily::Ipv4, messages(true, false, true));

    assert_eq!(
        verdict,
        Ssu2PeerTestVerdict::Undetermined,
        "the specification's ambiguous row decides nothing"
    );
    assert_eq!(
        service.snapshot().reachability,
        i2pr_transport::ReachabilityState::Unknown,
        "an ambiguous row records nothing"
    );
}

#[tokio::test]
async fn an_unknown_row_claims_nothing() {
    let (service, _handle) = bound_service().await;

    let verdict =
        service.note_peer_test_verdict(AddressFamily::Ipv4, messages(false, false, false));

    assert_eq!(
        verdict,
        Ssu2PeerTestVerdict::Undetermined,
        "no message at all decides nothing"
    );
    assert_eq!(
        service.snapshot().reachability,
        i2pr_transport::ReachabilityState::Unknown,
        "an unknown row records nothing"
    );
}

#[tokio::test]
async fn the_disagreement_is_the_routers_own_firewalled_verdict() {
    let (service, handle) = bound_service().await;
    let bound = handle.local_v4().expect("bound v4");
    let observed = Ssu2Endpoint::from_socket_addr(bound).expect("endpoint");
    // A different address on the same family: the peer sees this router
    // somewhere else, so the router is behind a NAT.
    let other = Ssu2Endpoint::new(observed.ip(), observed.port().wrapping_add(1)).expect("other");

    service.note_observed_external_address(other);

    // The same fact the specification's FIREWALLED row reports, reached by
    // the other route, so both routes reach the same state.
    assert_eq!(
        service.snapshot().reachability,
        i2pr_transport::ReachabilityState::Firewalled,
        "a disagreement with the router's own bound address is the firewalled verdict"
    );
}

#[tokio::test]
async fn a_comparison_with_no_bound_socket_is_refused() {
    // The router never invents a contradiction it cannot support: with no
    // bound socket for the family there is nothing to compare against, so
    // the observation is refused and nothing is recorded.
    let service = unbound_service();
    let observed = endpoint(7, 4242);

    let state = service.note_observed_external_address(observed);

    assert_eq!(
        state,
        i2pr_transport::ReachabilityState::Unknown,
        "a comparison with no bound socket is refused, not guessed"
    );
    assert_eq!(
        service.reachability_evidence().corroboration,
        0,
        "a refused comparison supports nothing"
    );
}

#[tokio::test]
async fn the_firewalled_gate_asks_only_on_the_state_the_tracker_really_holds() {
    let (service, _handle) = bound_service().await;

    service.note_peer_test_verdict(AddressFamily::Ipv4, messages(true, false, false));

    // The flag the daemon's gate uses is read from the **real** tracker,
    // never injected.
    let firewalled =
        service.snapshot().reachability == i2pr_transport::ReachabilityState::Firewalled;
    assert!(firewalled, "the tracker really holds the firewalled state");

    let mut owner = IntroducerRequestOwner::new(true);
    let eligible = [i2pr_transport::PeerId::from_bytes([0x5A; 32])];
    let decision = owner.decide(&eligible, firewalled, wall_ms());
    assert!(
        matches!(decision, IntroducerRequestDecision::Ask(_)),
        "a firewalled router asks one eligible peer"
    );
}

#[tokio::test]
async fn a_router_with_no_verdict_asks_nobody() {
    let (service, _handle) = bound_service().await;

    let firewalled =
        service.snapshot().reachability == i2pr_transport::ReachabilityState::Firewalled;
    assert!(!firewalled, "a router with no verdict is not firewalled");

    let mut owner = IntroducerRequestOwner::new(true);
    let eligible = [i2pr_transport::PeerId::from_bytes([0x5A; 32])];
    let decision = owner.decide(&eligible, firewalled, wall_ms());
    assert_eq!(
        decision,
        IntroducerRequestDecision::Refuse(IntroducerRequestRefusal::NotFirewalled),
        "a router that is not firewalled asks nobody"
    );
    assert_eq!(owner.counters().requests_refused_not_firewalled, 1);
}

#[tokio::test]
async fn the_corroboration_floor_is_unchanged() {
    // Plan 528 corrects the **negative** side only. The positive claim is
    // exactly as strict as it was: three distinct supporting classes are
    // still needed for `Reachable`, and the corroboration floor is still
    // two.
    let policy = ReachabilityPolicy::default();
    assert_eq!(
        policy.min_corroboration(),
        2,
        "the corroboration floor is untouched"
    );
    assert_eq!(
        policy.min_contradiction(),
        1,
        "the contradiction floor follows the specification's single-test rule"
    );
}
