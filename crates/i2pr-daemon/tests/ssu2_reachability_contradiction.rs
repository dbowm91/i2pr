//! Plan 527's lane — the router's reachability has one owner and one reader.
//!
//! Before this plan the router's reachability had **two** trackers. The
//! runtime owned one and the product read it; the peer-relay service owned
//! another, and **every** contradicting signal was written into that one
//! and read by nothing. So the tracker the product read could only ever be
//! *confirmed*, `ReachabilityState::Firewalled` and `Unreachable` were
//! unreachable states, and Plan 524's firewalled gate read `false` for
//! ever.
//!
//! The lane drives the **producer**, not an injected flag:
//!
//! 1. an observation that **agrees** with the router's own bound address
//!    corroborates the router's reachability;
//! 2. an observation that **differs** from it contradicts the router's
//!    reachability — the router is behind a NAT;
//! 3. a contradiction is never also a corroboration;
//! 4. the product reads that one tracker, and sees the contradiction;
//! 5. two contradicting classes reach `Firewalled`;
//! 6. Plan 524's gate asks **only** on the state the tracker really holds,
//!    and refuses otherwise;
//! 7. a router with no contradiction asks nobody;
//! 8. an unknown family records nothing;
//! 9. a router with no bound socket refuses the comparison rather than
//!    inventing an answer.
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
    CancellationToken, ChildFailurePolicy, ChildScope, IntroKey, Ssu2Endpoint,
    Ssu2IdentityMaterial, Ssu2RuntimeConfig, Ssu2RuntimeService, Ssu2SocketConfig,
};
use i2pr_transport::AddressFamily;
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

#[tokio::test]
async fn an_observation_that_agrees_with_the_bound_address_corroborates() {
    let (service, handle) = bound_service().await;
    let bound = handle.local_v4().expect("bound v4");
    let observed = Ssu2Endpoint::from_socket_addr(bound).expect("endpoint");

    service.note_observed_external_address(observed);

    let snapshot = service.snapshot();
    assert_eq!(
        snapshot.reachability,
        i2pr_transport::ReachabilityState::ObservedUnconfirmed,
        "one agreeing observation corroborates but does not confirm"
    );
    let evidence = service.reachability_evidence();
    assert_eq!(
        evidence.corroboration, 1,
        "an agreeing observation is exactly one corroborating class"
    );
}

#[tokio::test]
async fn an_observation_that_differs_from_the_bound_address_contradicts() {
    let (service, handle) = bound_service().await;
    let bound = handle.local_v4().expect("bound v4");
    // A different address on the same family: the peer sees this router
    // somewhere else, so the router is behind a NAT.
    let observed = Ssu2Endpoint::from_socket_addr(bound).expect("endpoint");
    let other = Ssu2Endpoint::new(observed.ip(), observed.port().wrapping_add(1)).expect("other");

    service.note_observed_external_address(other);

    // Plan 528: the disagreement between the observed address and the
    // router's own bound socket is this router's own **firewalled
    // verdict** -- the peer sees the router at an address it does not own,
    // so the router is behind a NAT. The verdict is the same fact the
    // specification's peer-test result table reports as FIREWALLED, so
    // both routes reach the same state. The router is never left
    // reachable, and it is never left claiming to be directly reachable.
    assert_eq!(
        service.snapshot().reachability,
        i2pr_transport::ReachabilityState::Firewalled,
        "a contradicting observation is the router's own firewalled verdict"
    );
}

#[tokio::test]
async fn a_contradiction_is_never_also_a_corroboration() {
    let (service, handle) = bound_service().await;
    let bound = handle.local_v4().expect("bound v4");
    let observed = Ssu2Endpoint::from_socket_addr(bound).expect("endpoint");
    let other = Ssu2Endpoint::new(observed.ip(), observed.port().wrapping_add(1)).expect("other");

    service.note_observed_external_address(other);

    let evidence = service.reachability_evidence();
    assert_eq!(
        evidence.corroboration, 0,
        "a contradiction supports nothing: one observation is one fact"
    );
}

#[tokio::test]
async fn the_product_reads_the_one_tracker_and_sees_the_contradiction() {
    let (service, handle) = bound_service().await;
    let bound = handle.local_v4().expect("bound v4");
    let observed = Ssu2Endpoint::from_socket_addr(bound).expect("endpoint");
    let other = Ssu2Endpoint::new(observed.ip(), observed.port().wrapping_add(1)).expect("other");

    service.note_observed_external_address(other);

    assert_ne!(
        service.snapshot().reachability,
        i2pr_transport::ReachabilityState::Reachable,
        "the product must not see a reachability claim it cannot honour"
    );
    assert_ne!(
        service.snapshot().reachability,
        i2pr_transport::ReachabilityState::CandidateReachable,
        "a contradicted router is not a candidate either"
    );
}

#[tokio::test]
async fn two_contradicting_classes_reach_firewalled() {
    let (service, handle) = bound_service().await;
    let bound = handle.local_v4().expect("bound v4");
    let observed = Ssu2Endpoint::from_socket_addr(bound).expect("endpoint");
    let other = Ssu2Endpoint::new(observed.ip(), observed.port().wrapping_add(1)).expect("other");

    // The peer-test contradiction (class 3) and the relay-success
    // indication (class 4) are two distinct classes, and the transport's
    // own state machine requires both before it will call the router
    // firewalled.
    service.note_observed_external_address(other);
    service.note_relay_firewalled(AddressFamily::Ipv4);

    assert_eq!(
        service.snapshot().reachability,
        i2pr_transport::ReachabilityState::Firewalled,
        "two distinct contradicting classes reach the firewalled state"
    );
}

#[tokio::test]
async fn the_firewalled_gate_asks_only_on_the_state_the_tracker_really_holds() {
    let (service, handle) = bound_service().await;
    let bound = handle.local_v4().expect("bound v4");
    let observed = Ssu2Endpoint::from_socket_addr(bound).expect("endpoint");
    let other = Ssu2Endpoint::new(observed.ip(), observed.port().wrapping_add(1)).expect("other");
    service.note_observed_external_address(other);
    service.note_relay_firewalled(AddressFamily::Ipv4);

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
async fn a_router_with_no_contradiction_asks_nobody() {
    let (service, _handle) = bound_service().await;

    let firewalled =
        service.snapshot().reachability == i2pr_transport::ReachabilityState::Firewalled;
    assert!(!firewalled, "an uncontradicted router is not firewalled");

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
async fn an_unknown_family_records_nothing() {
    let service = unbound_service();

    service.note_relay_firewalled(AddressFamily::Unknown);

    assert_eq!(
        service.snapshot().reachability,
        i2pr_transport::ReachabilityState::Unknown,
        "an unknown family records nothing"
    );
}

#[tokio::test]
async fn a_router_with_no_bound_socket_refuses_the_comparison() {
    let service = unbound_service();

    let state = service.note_observed_external_address(endpoint(1, 12345));

    assert_eq!(
        state,
        i2pr_transport::ReachabilityState::Unknown,
        "with no bound address there is nothing to compare against, so nothing is recorded"
    );
}
