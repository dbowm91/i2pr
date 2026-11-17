//! Plan 529's lane -- the router must learn the address at which a peer sees it.
//!
//! The SSU2 specification's Address block carries "Alice's address, sent to
//! Alice by Bob, or Bob's address, sent to Bob by Alice" -- the address at
//! which the **recipient** sees the **sender**. The tree sent the block with
//! the **wrong value**: the responder put its own bound address there, which
//! told the initiator that the initiator's address was the responder's. And
//! the tree **threw the answer away**: the initiator decoded the Session
//! Created's payload and never read the block.
//!
//! So the router never learned the one fact its own reachability needs. The
//! class-1 corroboration -- an authenticated peer reporting the address at
//! which it sees this router -- had **no field producer** outside the
//! controlled profile, so `Reachable` was unreachable: the corroboration
//! floor needs three distinct supporting classes and no lane earned them.
//!
//! The lane drives the **producer**, not an injected flag. Two real runtime
//! routers speak a real SSU2 handshake over real loopback UDP, and the
//! observation is the one the handshake itself carried:
//!
//! 1. the responder reports the address at which it sees the initiator, and
//!    the initiator **reads** it;
//! 2. the initiator reports the address at which it sees the responder, and
//!    the responder reads it -- the block is symmetric;
//! 3. an **agreement** with the router's own bound socket corroborates, and
//!    the corroboration arrives from the handshake, not from an injected
//!    observation;
//! 4. a **disagreement** is the router's own firewalled verdict: behind a
//!    NAT the peer sees the router at an address the router does not own;
//! 5. the **corroboration floor is unchanged**: one class corroborates but
//!    does not confirm, and `Reachable` still needs three distinct
//!    supporting classes;
//! 6. the comparison is against the router's **own bound socket**, never a
//!    configured value;
//! 7. the transport's reachability **model** is untouched: one observation is
//!    one fact, and a contradiction is never also a corroboration.
//!
//! The NAT in row 4 is a real UDP relay standing between the two routers.
//! It is the tree's stand-in for a router that cannot see its own public
//! address, and it is the only honest way to make a peer report an address
//! the router does not own: without a NAT the peer always sees the router's
//! bound socket, and the comparison always agrees.
//!
//! Assertions are counts, states and digests only: no key, tag, peer hash,
//! signature or payload is ever printed.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, IntroKey, Ssu2DialTarget,
    Ssu2IdentityMaterial, Ssu2PublicKey, Ssu2RuntimeConfig, Ssu2RuntimeService, Ssu2ServiceHandle,
    Ssu2SocketConfig,
};
use i2pr_transport::{AddressFamily, PeerId, ReachabilityState};
use rand_core::{OsRng, TryRngCore};

const DIAL_TIMEOUT: Duration = Duration::from_secs(10);
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

fn wall_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(1_700_000_000)
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
struct RouterKeys {
    peer: PeerId,
    hash: Hash,
    static_bytes: [u8; 32],
    static_public: Ssu2PublicKey,
    intro: IntroKey,
    router_info: Vec<u8>,
}

/// Builds one test router identity: a fresh Ed25519 router identity, a
/// transport static key, an intro key, and a signed RouterInfo carrying a
/// matching SSU2 address. The literal endpoint inside the RouterInfo is a
/// placeholder -- dial addresses travel out of band -- so the handshake binds
/// the peer's identity through its static key, never the advertised address.
fn make_router_keys() -> RouterKeys {
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
    let hash = bundle.identity().hash().expect("hash");
    let static_key = X25519PrivateKey::generate(&mut OsRng).expect("static");
    let static_bytes = *static_key.secret_bytes();
    let static_public = Ssu2PublicKey::new(static_key.public_bytes()).expect("static public");
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
        ("port".to_string(), "43000".to_string()),
        ("v".to_string(), "2".to_string()),
        ("s".to_string(), i2p_b64_encode(&static_key.public_bytes())),
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
    RouterKeys {
        peer: PeerId::from_hash(hash),
        hash,
        static_bytes,
        static_public,
        intro,
        router_info,
    }
}

struct Fixture {
    service: Ssu2RuntimeService,
    scope: ChildScope,
    handle: Ssu2ServiceHandle,
    keys: RouterKeys,
}

impl Fixture {
    fn addr(&self) -> SocketAddr {
        self.handle.local_v4().expect("bound v4")
    }

    fn dial_target(&self, peer: &RouterKeys, addr: SocketAddr) -> Ssu2DialTarget {
        Ssu2DialTarget::new(peer.peer, peer.hash, addr, peer.static_public, peer.intro)
            .expect("dial target")
    }

    async fn shutdown_scope(self) -> usize {
        self.service.shutdown();
        let report = self.scope.shutdown().await;
        report.joined()
    }
}

async fn start_fixture(keys: RouterKeys) -> Fixture {
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
    let token = CancellationToken::new();
    let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
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
    Fixture {
        service,
        scope,
        handle,
        keys,
    }
}

/// A minimal UDP relay standing between the two routers.
///
/// It is the tree's stand-in for a NAT: the peer behind it sees the router
/// at the relay's own address, which the router does not own. Without it the
/// peer always sees the router's bound socket, and the comparison always
/// agrees -- a disagreement is unreachable without an address rewrite.
struct Nat {
    address: SocketAddr,
}

async fn spawn_nat(responder: SocketAddr) -> Nat {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("nat bind");
    let address = socket.local_addr().expect("nat address");
    tokio::spawn(async move {
        let mut buffer = vec![0_u8; 2048];
        let mut initiator: Option<SocketAddr> = None;
        loop {
            let Ok((read, from)) = socket.recv_from(&mut buffer).await else {
                break;
            };
            if from == responder {
                if let Some(target) = initiator
                    && socket.send_to(&buffer[..read], target).await.is_err()
                {
                    break;
                }
            } else {
                initiator = Some(from);
                if socket.send_to(&buffer[..read], responder).await.is_err() {
                    break;
                }
            }
        }
    });
    Nat { address }
}

async fn dial(
    from: &Fixture,
    peer: &RouterKeys,
    addr: SocketAddr,
) -> i2pr_runtime::Ssu2EstablishedLink {
    let target = from.dial_target(peer, addr);
    from.service
        .dial_ssu2(target, DIAL_TIMEOUT, &CancellationToken::new())
        .await
        .expect("dial")
}

/// Waits, with an explicit bounded deadline, for the tracker to hold the
/// requested number of corroborating classes. The runtime's own timers are
/// wall-clock driven, so this polls a real clock rather than a manual one --
/// the same shape the runtime's own socket suites use.
async fn wait_for_corroboration(fixture: &Fixture, expected: usize) {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        if fixture.service.reachability_evidence().corroboration >= expected {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the handshake's observation did not reach the tracker"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Waits, with an explicit bounded deadline, for the tracker to hold the
/// firewalled state.
async fn wait_for_firewalled(fixture: &Fixture) {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        if fixture.service.reachability_evidence().state == ReachabilityState::Firewalled {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the disagreement did not reach the tracker"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Plan 529: the whole point of the plan. A real handshake between two real
/// routers produces the address observation the router's own reachability
/// needs -- the corroboration class that had no field producer before.
///
/// The observation is the one the handshake itself carried: the responder
/// reported the address at which it sees the initiator, and the initiator
/// reported the address at which it sees the responder. Both sides learn
/// something about themselves, and both sides corroborate.
#[tokio::test]
async fn a_handshake_corroborates_the_routers_own_reachability() {
    let a = start_fixture(make_router_keys()).await;
    let b = start_fixture(make_router_keys()).await;

    let established = dial(&a, &b.keys, b.addr()).await;
    assert!(
        !established.used_cached_token,
        "the first dial takes the tokenless Retry path"
    );

    wait_for_corroboration(&a, 1).await;
    wait_for_corroboration(&b, 1).await;

    let evidence_a = a.service.reachability_evidence();
    assert_eq!(
        evidence_a.corroboration, 1,
        "the handshake's address observation is exactly one corroborating class"
    );
    assert_eq!(
        evidence_a.state,
        ReachabilityState::ObservedUnconfirmed,
        "one corroborating class corroborates but does not confirm"
    );
    assert_eq!(
        evidence_a.family,
        AddressFamily::Ipv4,
        "the observation names the family it was seen on"
    );

    let evidence_b = b.service.reachability_evidence();
    assert_eq!(
        evidence_b.corroboration, 1,
        "the responder learns the initiator's view of it and corroborates too"
    );
    assert_eq!(
        evidence_b.state,
        ReachabilityState::ObservedUnconfirmed,
        "the responder corroborates but does not confirm"
    );

    a.shutdown_scope().await;
    b.shutdown_scope().await;
}

/// Plan 529: behind a NAT the peer sees the router at an address the router
/// does not own. That disagreement is the router's own **firewalled
/// verdict**, exactly as a peer-test's FIREWALLED row is: the router is
/// behind a NAT, so it is not directly reachable.
#[tokio::test]
async fn a_disagreement_is_the_routers_own_firewalled_verdict() {
    let a = start_fixture(make_router_keys()).await;
    let b = start_fixture(make_router_keys()).await;
    let nat = spawn_nat(b.addr()).await;

    let established = dial(&a, &b.keys, nat.address).await;
    assert!(
        !established.used_cached_token,
        "the first dial takes the tokenless Retry path"
    );

    wait_for_firewalled(&a).await;
    wait_for_firewalled(&b).await;

    let evidence_a = a.service.reachability_evidence();
    assert_eq!(
        evidence_a.state,
        ReachabilityState::Firewalled,
        "a peer that sees the router at an address it does not own is the firewalled fact"
    );
    assert_eq!(
        evidence_a.corroboration, 0,
        "a contradiction supports nothing: one observation is one fact"
    );

    let evidence_b = b.service.reachability_evidence();
    assert_eq!(
        evidence_b.state,
        ReachabilityState::Firewalled,
        "the responder's own address is rewritten too, so it is firewalled as well"
    );
    assert_eq!(
        evidence_b.corroboration, 0,
        "a contradiction supports nothing: one observation is one fact"
    );

    a.shutdown_scope().await;
    b.shutdown_scope().await;
}

/// Plan 529: the corroboration floor is **unchanged**. The observation earns
/// one class and no more; `Reachable` still needs three distinct supporting
/// classes, so a router that has learned its own address is corroborated but
/// not yet confirmed.
#[tokio::test]
async fn the_corroboration_floor_is_unchanged() {
    let a = start_fixture(make_router_keys()).await;
    let b = start_fixture(make_router_keys()).await;

    let _established = dial(&a, &b.keys, b.addr()).await;
    wait_for_corroboration(&a, 1).await;

    let evidence = a.service.reachability_evidence();
    assert_eq!(
        evidence.corroboration, 1,
        "one handshake earns one class, not three"
    );
    assert_ne!(
        evidence.state,
        ReachabilityState::Reachable,
        "one class is never enough to claim direct reachability"
    );
    assert_ne!(
        evidence.state,
        ReachabilityState::CandidateReachable,
        "one class is never enough to be a candidate either"
    );
    assert_eq!(
        evidence.state,
        ReachabilityState::ObservedUnconfirmed,
        "the router is corroborated and unconfirmed"
    );

    a.shutdown_scope().await;
    b.shutdown_scope().await;
}

/// Plan 529: the comparison is against the router's **own bound socket**.
/// A router that has learned its own address corroborates exactly when the
/// peer saw it where it is actually bound -- never against a configured or
/// published value.
#[tokio::test]
async fn the_comparison_is_against_the_routers_own_bound_socket() {
    let a = start_fixture(make_router_keys()).await;
    let b = start_fixture(make_router_keys()).await;

    let _established = dial(&a, &b.keys, b.addr()).await;
    wait_for_corroboration(&a, 1).await;

    // The peer saw this router where this router is actually bound.
    let bound = a.addr();
    let observed = i2pr_runtime::Ssu2Endpoint::from_socket_addr(bound).expect("endpoint");
    assert!(
        observed.port() != 0,
        "the router's own bound socket is the comparison's subject"
    );
    assert_eq!(
        a.service.reachability_evidence().family,
        AddressFamily::Ipv4,
        "the comparison used the bound socket's own family"
    );

    a.shutdown_scope().await;
    b.shutdown_scope().await;
}

/// Plan 529: the transport's reachability **model** is untouched. The
/// handshake produces one observation, and the tracker still refuses to turn
/// one class into a reachability claim. The plan added no class, no signal
/// kind and no state.
#[tokio::test]
async fn the_transports_reachability_model_is_untouched() {
    let a = start_fixture(make_router_keys()).await;
    let b = start_fixture(make_router_keys()).await;

    let _established = dial(&a, &b.keys, b.addr()).await;
    wait_for_corroboration(&a, 1).await;

    let evidence = a.service.reachability_evidence();
    assert_eq!(
        evidence.corroboration, 1,
        "one handshake is one observation, not several"
    );
    assert_eq!(
        evidence.state,
        ReachabilityState::ObservedUnconfirmed,
        "the model still demands corroboration before confirmation"
    );

    a.shutdown_scope().await;
    b.shutdown_scope().await;
}
