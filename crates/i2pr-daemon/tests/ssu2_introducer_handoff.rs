//! Plan 498 — the SSU2 introducer (Bob) role owner's relay handoff.
//!
//! Plan 490 built the role owner and the runtime's relay handoff, and
//! recorded that the daemon's ordinary inbound pump never drained the
//! handoff: a Relay Request that a peer really sent was decoded,
//! authenticated and then dropped.
//!
//! This lane drives the **daemon's drain** — the same
//! `drain_relay_events` the daemon's live inbound loop calls — over a
//! daemon-side pull seam, so the handoff can be exercised without binding
//! a socket. The pull seam cannot enqueue into the runtime's relay
//! channel: that channel's sender is private to the runtime and is fed
//! only by the authenticated transport path.
//!
//! The lane asserts:
//!
//! - **the discriminator** — a relay block in the handoff reaches the
//!   role owner, and the role owner's receipt moves 0→1;
//! - **the control** — an empty handoff delivers nothing and counts
//!   nothing;
//! - **the refusal** — a requester the NetDB does not hold is refused,
//!   the refusal is counted by reason, and no state is left behind;
//! - **the bound** — one drain pass processes at most the per-pass bound.
//!
//! Every event is a real, signed Relay Request over the specification's
//! preimage. Nothing here reaches a socket, a listener, or the network.

use std::collections::{HashMap, VecDeque};

use i2pr_crypto::{RouterIdentityBundle, SigningPrivateKey};
use i2pr_daemon::router_i2np::generate_controlled_identity;
use i2pr_daemon::ssu2_introducer::{
    IntroducerSignerResolver, Ssu2IntroducerConfig, Ssu2IntroducerRefusal, Ssu2IntroducerService,
};
use i2pr_daemon::ssu2_relay_drain::{
    MAX_RELAY_DRAIN_PER_PASS, RelayDrainOutcome, RelayEventSource, drain_relay_events,
};
use i2pr_proto::{Hash, SigningPublicKey};
use i2pr_runtime::{
    Ssu2Endpoint, Ssu2InboundRelay, Ssu2RelayEvent, Ssu2RuntimeConfig, Ssu2RuntimeService,
    relay_request_preimage,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const TARGET_HASH: [u8; 32] = [0x7A; 32];
const REQUESTER_HASH: [u8; 32] = [0xC1; 32];
const NOW_MS: u64 = 1_000;
const NOW_SECS: u64 = 1_000;
const TAG: u32 = 0xABCD;
const NONCE: u32 = 21;

/// A daemon-side pull seam over a bounded queue.
///
/// Plan 498: this is the *source* half of the drain's seam. It hands the
/// drain relay events; it cannot enqueue into the runtime's relay
/// channel, and it cannot bypass the role owner's verification — an
/// event still needs a valid signature from a key the resolver supplies.
struct QueueSource {
    events: VecDeque<Ssu2InboundRelay>,
}

impl QueueSource {
    fn new(events: Vec<Ssu2InboundRelay>) -> Self {
        Self {
            events: events.into(),
        }
    }
}

impl RelayEventSource for QueueSource {
    fn try_next_relay_event(&mut self) -> Option<Ssu2InboundRelay> {
        self.events.pop_front()
    }
}

/// A resolver that answers from a fixed set of keys.
///
/// The **production** resolver is `NetDbSignerResolver`, which answers only
/// from the NetDB's validated store; its own rows live in the bootstrap
/// module's unit tests. This lane's resolver is the same shape the
/// existing private mesh uses, so the drain's behaviour is what is under
/// test here.
struct MeshSigners {
    keys: HashMap<[u8; 32], SigningPublicKey>,
}

impl IntroducerSignerResolver for MeshSigners {
    fn signing_key(&self, peer_hash: &[u8; 32]) -> Option<SigningPublicKey> {
        self.keys.get(peer_hash).cloned()
    }
}

fn endpoint() -> Ssu2Endpoint {
    Ssu2Endpoint::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
        9_999,
    )
    .expect("endpoint")
}

fn inbound(peer: [u8; 32], event: Ssu2RelayEvent) -> Ssu2InboundRelay {
    Ssu2InboundRelay {
        link_id: i2pr_transport::LinkId::new(1).expect("link id"),
        peer: i2pr_transport::PeerId::from_hash(Hash::from_bytes(peer)),
        event,
    }
}

/// Builds the role owner, the requester's key, and the requester's
/// private key. The request event is built on demand because
/// `Ssu2InboundRelay` is deliberately not `Clone`.
fn lane() -> (
    Ssu2IntroducerService,
    MeshSigners,
    SigningPrivateKey,
    [u8; 32],
) {
    let mut rng = ChaCha8Rng::seed_from_u64(0x498);
    let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
    let identity =
        generate_controlled_identity(&bundle, "127.0.0.1", 34_567).expect("ssu2 identity");
    let bob = *identity.router_hash.as_bytes();

    let runtime = Ssu2RuntimeService::new_with_bandwidth(
        Ssu2RuntimeConfig {
            introducer_service: true,
            ..Default::default()
        },
        identity,
        None,
    )
    .expect("runtime");
    let mut owner = Ssu2IntroducerService::new(
        Ssu2IntroducerConfig {
            enabled: true,
            ..Default::default()
        },
        runtime,
        bob,
    );

    let alice_key = SigningPrivateKey::from_bytes([0x11; 32]);
    let charlie_key = SigningPrivateKey::from_bytes([0x22; 32]);
    let mut signers = MeshSigners {
        keys: HashMap::new(),
    };
    signers.keys.insert(
        REQUESTER_HASH,
        alice_key.public_key().expect("alice public"),
    );
    signers.keys.insert(
        TARGET_HASH,
        charlie_key.public_key().expect("charlie public"),
    );

    // The firewalled target asks this router for an introduction tag.
    owner.issue_tag(TARGET_HASH, TAG, NOW_SECS).expect("tag");

    (owner, signers, alice_key, bob)
}

/// Builds one signed Relay Request naming the tag's target.
fn request_event(bob: &[u8; 32], alice_key: &SigningPrivateKey) -> Ssu2InboundRelay {
    let preimage = relay_request_preimage(
        bob,
        &TARGET_HASH,
        NONCE,
        TAG,
        NOW_SECS as u32,
        2,
        endpoint(),
    );
    let signature = alice_key.sign(&preimage).expect("sign");
    let request = i2pr_runtime::RelayRequestBlock::new(
        NONCE,
        TAG,
        NOW_SECS as u32,
        2,
        endpoint(),
        signature.as_bytes().to_vec(),
    )
    .expect("request");
    inbound(REQUESTER_HASH, Ssu2RelayEvent::Request(request))
}

#[test]
fn the_drain_delivers_a_relay_block_to_the_role_owner() {
    // The discriminator: the block reaches the role owner, and the role
    // owner's own receipt moves 0→1. A drain that never reached the role
    // owner would leave every counter at zero.
    let (mut owner, signers, alice_key, bob) = lane();
    assert_eq!(owner.receipt().requests_admitted, 0);
    assert_eq!(owner.receipt().intros_produced, 0);

    let mut source = QueueSource::new(vec![request_event(&bob, &alice_key)]);
    let outcome = drain_relay_events(&mut source, &mut owner, &signers, NOW_MS, NOW_SECS);

    assert_eq!(outcome.drained, 1, "one event was pulled from the handoff");
    assert_eq!(outcome.admitted, 1, "the role owner admitted the request");
    assert_eq!(outcome.refused, 0);
    assert_eq!(
        outcome.awaiting_carriage, 1,
        "the produced intro is counted, not silently dropped"
    );
    assert_eq!(
        owner.receipt().requests_admitted,
        1,
        "the role owner's receipt must move 0->1"
    );
    assert_eq!(owner.receipt().intros_produced, 1);
    assert!(
        owner.receipt().refusals.is_empty(),
        "an admitted request must not be counted as a refusal"
    );
}

#[test]
fn the_drain_delivers_nothing_when_the_handoff_is_empty() {
    // The control: without an event in the handoff nothing is delivered
    // and nothing is counted. This is what makes the discriminator sharp.
    let (mut owner, signers, _, _) = lane();
    let mut source = QueueSource::new(Vec::new());
    let outcome = drain_relay_events(&mut source, &mut owner, &signers, NOW_MS, NOW_SECS);

    assert_eq!(outcome, RelayDrainOutcome::default());
    assert_eq!(owner.receipt().requests_admitted, 0);
    assert_eq!(owner.receipt().intros_produced, 0);
}

#[test]
fn the_drain_refuses_a_requester_the_netdb_does_not_hold() {
    // The refusal row: a requester the resolver cannot answer is refused,
    // the refusal is counted by reason, and no state is left behind — so
    // replaying the identical event is refused the same way rather than
    // becoming a duplicate.
    let (mut owner, _, alice_key, bob) = lane();
    let empty = MeshSigners {
        keys: HashMap::new(),
    };

    let mut source = QueueSource::new(vec![request_event(&bob, &alice_key)]);
    let outcome = drain_relay_events(&mut source, &mut owner, &empty, NOW_MS, NOW_SECS);

    assert_eq!(outcome.drained, 1);
    assert_eq!(outcome.admitted, 0);
    assert_eq!(outcome.refused, 1);
    assert_eq!(outcome.awaiting_carriage, 0);
    assert_eq!(owner.receipt().requests_admitted, 0);
    assert_eq!(owner.receipt().intros_produced, 0);
    assert_eq!(
        owner
            .receipt()
            .refusals
            .get(Ssu2IntroducerRefusal::UnknownRequester.receipt_key()),
        Some(&1),
        "the refusal must be counted by its reason"
    );

    // The replay is refused the same way. A tracked request would have
    // turned the replay into a different reason.
    let mut source = QueueSource::new(vec![request_event(&bob, &alice_key)]);
    let outcome = drain_relay_events(&mut source, &mut owner, &empty, NOW_MS, NOW_SECS);
    assert_eq!(outcome.refused, 1);
    assert_eq!(
        owner
            .receipt()
            .refusals
            .get(Ssu2IntroducerRefusal::UnknownRequester.receipt_key()),
        Some(&2),
        "the replay must be refused for the same reason, not as a duplicate"
    );
    assert_eq!(
        owner
            .receipt()
            .refusals
            .get(Ssu2IntroducerRefusal::DuplicateRequest.receipt_key()),
        None,
        "a refused request must leave no tracked state behind"
    );
}

#[test]
fn the_drain_is_bounded_per_pass() {
    // The bound row: one pass processes at most the per-pass bound, even
    // when the handoff holds more.
    let (mut owner, signers, alice_key, bob) = lane();
    let events: Vec<Ssu2InboundRelay> = (0..MAX_RELAY_DRAIN_PER_PASS + 8)
        .map(|_| request_event(&bob, &alice_key))
        .collect();
    let mut source = QueueSource::new(events);
    let outcome = drain_relay_events(&mut source, &mut owner, &signers, NOW_MS, NOW_SECS);

    assert_eq!(
        outcome.drained, MAX_RELAY_DRAIN_PER_PASS as u64,
        "one pass must not exceed the per-pass bound"
    );
    assert!(
        !source.events.is_empty(),
        "the events beyond the bound stay in the handoff"
    );
}

/// Plan 498: the daemon's **live** inbound loop drains the relay handoff.
///
/// `ServiceProduct::poll_inbound` is not on the live path — the daemon's
/// own source says so at `service_product.rs` — so the live drain in the
/// composition root is the mandatory half of the fix.
///
/// This is a **static** guard. It proves the wiring is present in the live
/// loop; it does not prove that a live daemon executed it, because the
/// runtime's relay channel admits no external injection and the role
/// requires the non-strict operator profile. The drain's *behaviour* is
/// proved by the rows above.
#[test]
fn the_daemon_live_loop_drains_the_relay_handoff() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
    )
    .expect("read the daemon composition root");

    assert!(
        source.contains("impl crate::ssu2_relay_drain::RelayEventSource for NormalSsu2Owner"),
        "the live owner must be the drain's event source"
    );
    assert!(
        source.contains("_ = relay_tick.tick()"),
        "the live loop must wake on the relay drain tick"
    );
    assert!(
        source.contains("crate::ssu2_relay_drain::drain_relay_events("),
        "the live loop must drain the relay handoff into the role owner"
    );
    assert!(
        source.contains("NetDbSignerResolver::new("),
        "the live role owner must answer requester keys from the validated store"
    );
}
