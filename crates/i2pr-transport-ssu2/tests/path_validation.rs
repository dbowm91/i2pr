//! Plan 159 path-validation trajectories through real sealed packets.
//!
//! Two paired [`Ssu2Session`] endpoints (built exactly like the Plan
//! 157 data-phase suite: a real Noise transcript dance to matching
//! directional keys) exchange byte-identical wire datagrams through
//! [`PathValidator`] machines. The harness simulates the runtime's
//! source tracking: a validator is consulted only when the receiving
//! session authenticated the datagram, and migration applies the
//! session's [`Ssu2Session::note_path_migrated`] congestion reset.
//!
//! Coverage maps to the plan's §§2–4, §10, and §11: spoof/replay
//! rejection without migration, bounded candidates, wrong-response
//! rejection, exact-once legitimate migration, timeout retention of
//! the old path, IPv4/IPv6 separation, conservative candidate MTU,
//! and minimum-MTU challenge/response wire fit.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use i2pr_crypto::X25519PrivateKey;
use i2pr_transport::AddressFamily;
use i2pr_transport_ssu2::{
    PATH_CHALLENGE_LENGTH, PathError, PathEvent, PathValidator, Role, SessionConfig, SessionEvent,
    Ssu2Endpoint, Ssu2PublicKey, Ssu2Session, Ssu2SplitKeys, Ssu2Transcript,
};
use i2pr_transport_ssu2::{constants, path::PATH_VALIDATION_TIMEOUT_MS};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const NOW_MS: u64 = 2_000_000;
const NOW_SECS: u64 = 1_700_000_000;
const LOCAL_CONN_A: u64 = 0xaaaa_aaaa_aaaa_aaaa;
const LOCAL_CONN_B: u64 = 0xbbbb_bbbb_bbbb_bbbb;

fn secret(seed: u64) -> X25519PrivateKey {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    X25519PrivateKey::generate(&mut rng).expect("deterministic secret")
}

fn public_of(key: &X25519PrivateKey) -> Ssu2PublicKey {
    Ssu2PublicKey::new(key.public_bytes()).expect("public")
}

fn intro(byte: u8) -> i2pr_transport_ssu2::IntroKey {
    i2pr_transport_ssu2::IntroKey::new([byte; 32])
}

/// Runs the Noise transcript to matching directional splits for both
/// roles (mirrors the data-phase suite without RouterInfo).
fn paired_splits() -> (Ssu2SplitKeys, Ssu2SplitKeys) {
    let bob_static = secret(5001);
    let bob_public = public_of(&bob_static);
    let alice_static = secret(5002);
    let alice_public = public_of(&alice_static);
    let alice_eph = secret(5003);
    let bob_eph = secret(5004);
    let alice_eph_public = public_of(&alice_eph);
    let bob_eph_public = public_of(&bob_eph);
    let request_header = [0x11_u8; 32];
    let created_header = [0x22_u8; 32];

    let alice = Ssu2Transcript::new(Role::Initiator, bob_public);
    let bob = Ssu2Transcript::new(Role::Responder, bob_public);
    let es_alice = secret(5003)
        .diffie_hellman(bob_public.as_bytes())
        .expect("es");
    let es_bob = bob_static
        .diffie_hellman(alice_eph_public.as_bytes())
        .expect("es");
    let (alice, request_ct) = alice
        .seal_session_request(&request_header, alice_eph_public, es_alice, &[9_u8; 16])
        .expect("seal");
    let (bob, _) = bob
        .accept_session_request(&request_header, alice_eph_public, es_bob, &request_ct)
        .expect("accept");
    let ee_bob = secret(5004)
        .diffie_hellman(alice_eph_public.as_bytes())
        .expect("ee");
    let ee_alice = secret(5003)
        .diffie_hellman(bob_eph_public.as_bytes())
        .expect("ee");
    let (bob, created_ct) = bob
        .seal_session_created(
            &request_ct,
            &created_header,
            bob_eph_public,
            ee_bob,
            &[7_u8; 16],
        )
        .expect("seal created");
    let (alice, _) = alice
        .accept_session_created(
            &request_ct,
            &created_header,
            bob_eph_public,
            ee_alice,
            &created_ct,
        )
        .expect("accept created");
    let (alice, frame) = alice.seal_confirmed_static(alice_public).expect("static");
    let (bob, _) = bob.accept_confirmed_static(&frame).expect("open static");
    let se_alice = alice_static
        .diffie_hellman(bob_eph_public.as_bytes())
        .expect("se");
    let se_bob = secret(5004)
        .diffie_hellman(alice_public.as_bytes())
        .expect("se");
    let (alice, confirmed_ct) = alice
        .seal_confirmed_payload(se_alice, &[5_u8; 16])
        .expect("seal confirmed");
    let (bob, _) = bob
        .open_confirmed_payload(se_bob, &confirmed_ct)
        .expect("open confirmed");
    (alice.split().expect("split"), bob.split().expect("split"))
}

fn paired_sessions() -> (Ssu2Session, Ssu2Session) {
    let (alice_keys, bob_keys) = paired_splits();
    let alice = Ssu2Session::new(
        SessionConfig {
            local_conn_id: LOCAL_CONN_A,
            remote_conn_id: LOCAL_CONN_B,
            local_intro: intro(0xA1),
            remote_intro: intro(0xB2),
            initial_send_packet_number: 0,
            max_payload_bytes: SessionConfig::max_payload_for_mtu(1280, false),
            idle_timeout_ms: 300_000,
        },
        alice_keys,
    )
    .expect("alice session");
    let bob = Ssu2Session::new(
        SessionConfig {
            local_conn_id: LOCAL_CONN_B,
            remote_conn_id: LOCAL_CONN_A,
            local_intro: intro(0xB2),
            remote_intro: intro(0xA1),
            initial_send_packet_number: 0,
            max_payload_bytes: SessionConfig::max_payload_for_mtu(1280, false),
            idle_timeout_ms: 300_000,
        },
        bob_keys,
    )
    .expect("bob session");
    (alice, bob)
}

fn v4(octet: u8, port: u16) -> Ssu2Endpoint {
    Ssu2Endpoint::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, octet)), port).expect("endpoint")
}

fn v6(id: u16, port: u16) -> Ssu2Endpoint {
    Ssu2Endpoint::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, id)), port).expect("endpoint")
}

fn i2np_bytes(message_type: u8, message_id: u32, body_len: usize, fill: u8) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(9 + body_len);
    bytes.push(message_type);
    bytes.extend_from_slice(&message_id.to_be_bytes());
    bytes.extend_from_slice(&(NOW_SECS as u32).to_be_bytes());
    bytes.extend(std::iter::repeat_n(fill, body_len));
    bytes
}

/// Seals one fresh datagram from `from`, delivering whatever controls
/// and fragments are pending.
fn seal(from: &mut Ssu2Session, now_ms: u64) -> Vec<u8> {
    from.poll_transmit(now_ms).expect("sealed datagram")
}

/// Receives `bytes` into `to`, returning the semantic events.
/// Panics when the session drops the datagram: the harness must only
/// consult the validator for authenticated packets.
fn receive(to: &mut Ssu2Session, bytes: &[u8], now_ms: u64) -> Vec<SessionEvent> {
    let outcome = to.receive_datagram(now_ms, NOW_SECS, bytes);
    assert!(
        outcome.dropped.is_none(),
        "harness must only validate authenticated packets, dropped={:?}",
        outcome.dropped
    );
    outcome.events
}

struct Pair {
    alice: Ssu2Session,
    bob: Ssu2Session,
    alice_path: PathValidator,
    bob_path: PathValidator,
    alice_addr: Ssu2Endpoint,
    bob_addr: Ssu2Endpoint,
    now_ms: u64,
}

impl Pair {
    fn new() -> Self {
        let (alice, bob) = paired_sessions();
        let alice_addr = v4(1, 10001);
        let bob_addr = v4(2, 10002);
        // Each validator tracks the PEER endpoint: Bob's validated
        // path is Alice's address and vice versa.
        Self {
            alice,
            bob,
            alice_path: PathValidator::new(bob_addr, 1280).expect("alice path"),
            bob_path: PathValidator::new(alice_addr, 1280).expect("bob path"),
            alice_addr,
            bob_addr,
            now_ms: NOW_MS,
        }
    }

    /// Moves one sealed Alice datagram to Bob as if it arrived from
    /// `source`: returns Bob's events only when the session
    /// authenticated it (mirrors the runtime ordering).
    fn alice_to_bob(&mut self, source: Ssu2Endpoint) -> Option<Vec<SessionEvent>> {
        let bytes = seal(&mut self.alice, self.now_ms);
        assert!(bytes.len() <= 1280, "sealed datagram fits the minimum MTU");
        let outcome = self.bob.receive_datagram(self.now_ms, NOW_SECS, &bytes);
        if outcome.dropped.is_some() {
            return None;
        }
        // Authenticated: the runtime now consults the validator.
        if source != self.bob_path.validated().endpoint() && !self.bob_path.is_known(source) {
            let mut challenge = [0_u8; PATH_CHALLENGE_LENGTH];
            challenge[0] = 0xC0;
            challenge[1] = (self.now_ms & 0xFF) as u8;
            challenge[2] = (self.now_ms >> 8 & 0xFF) as u8;
            // Fixed nonzero tail keeps the harness deterministic.
            for (index, byte) in challenge.iter_mut().enumerate().skip(3) {
                *byte = (index as u8).wrapping_add(0xA0);
            }
            let _ = self
                .bob_path
                .note_authenticated_packet(source, challenge, self.now_ms);
        }
        Some(outcome.events)
    }

    /// Moves one sealed Bob datagram to Alice from Bob's validated
    /// endpoint.
    fn bob_to_alice(&mut self) -> Vec<SessionEvent> {
        let bytes = seal(&mut self.bob, self.now_ms);
        receive(&mut self.alice, &bytes, self.now_ms)
    }
}

#[test]
fn unauthenticated_datagram_from_new_endpoint_does_nothing() {
    let mut pair = Pair::new();
    let garbage = vec![0x5A_u8; 64];
    let outcome = pair.bob.receive_datagram(pair.now_ms, NOW_SECS, &garbage);
    assert!(outcome.dropped.is_some());
    assert_eq!(pair.bob_path.candidate_count(), 0);
    assert_eq!(
        pair.bob_path.validated().endpoint(),
        pair.alice_addr,
        "no migration without authentication"
    );
}

#[test]
fn authenticated_replay_at_new_endpoint_does_not_migrate() {
    let mut pair = Pair::new();
    // Baseline delivery on the validated path first.
    pair.alice
        .queue_i2np_message(i2np_bytes(6, 0xA001, 64, 0x31))
        .expect("queue");
    let events = pair
        .alice_to_bob(pair.alice_addr)
        .expect("authenticated baseline");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, SessionEvent::I2npMessage(_))),
        "baseline I2NP delivers on the validated path"
    );
    // Reseal is impossible without fresh packet numbers, so replay the
    // exact bytes from a new endpoint: the session drops them before
    // any validator consult.
    pair.alice
        .queue_i2np_message(i2np_bytes(6, 0xA002, 64, 0x32))
        .expect("queue");
    let bytes = seal(&mut pair.alice, pair.now_ms);
    let first = pair.bob.receive_datagram(pair.now_ms, NOW_SECS, &bytes);
    assert!(first.dropped.is_none());
    let replay = pair.bob.receive_datagram(pair.now_ms, NOW_SECS, &bytes);
    assert!(
        replay.dropped.is_some(),
        "replayed bytes die in the session, never reaching validation"
    );
    assert_eq!(pair.bob_path.candidate_count(), 0);
    assert_eq!(pair.bob_path.validated().endpoint(), pair.alice_addr);
    assert_eq!(pair.bob_path.counters().migrations, 0);
}

#[test]
fn authenticated_new_packet_creates_only_bounded_candidate_state() {
    let mut pair = Pair::new();
    let candidate = v4(9, 20009);
    pair.alice
        .queue_i2np_message(i2np_bytes(6, 0xB001, 128, 0x41))
        .expect("queue");
    let events = pair
        .alice_to_bob(candidate)
        .expect("fresh packet authenticates from any source");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, SessionEvent::I2npMessage(_))),
        "payload still authenticates; only the path is unproven"
    );
    assert_eq!(pair.bob_path.candidate_count(), 1);
    assert_eq!(pair.bob_path.validated().endpoint(), pair.alice_addr);
    assert_eq!(pair.bob_path.counters().challenges_issued, 1);
    assert_eq!(pair.bob_path.counters().migrations, 0);
}

#[test]
fn wrong_path_response_does_not_migrate() {
    let mut pair = Pair::new();
    let candidate = v4(9, 20009);
    pair.alice
        .queue_i2np_message(i2np_bytes(6, 0xC001, 64, 0x51))
        .expect("queue");
    pair.alice_to_bob(candidate).expect("candidate");
    assert_eq!(
        pair.bob_path
            .on_path_response(candidate, &[0xFF_u8; PATH_CHALLENGE_LENGTH], pair.now_ms),
        Err(PathError::ChallengeMismatch)
    );
    assert_eq!(pair.bob_path.validated().endpoint(), pair.alice_addr);
    assert_eq!(pair.bob_path.candidate_count(), 1);
}

#[test]
fn correct_challenge_response_migrates_exactly_once() {
    let mut pair = Pair::new();
    let candidate = v4(9, 20009);
    pair.alice
        .queue_i2np_message(i2np_bytes(6, 0xD001, 64, 0x61))
        .expect("queue");
    pair.alice_to_bob(candidate).expect("candidate");
    let challenge = pair
        .bob_path
        .challenge_for(candidate)
        .expect("issued challenge");
    // Alice answers through her own session: the response is a real
    // sealed control packet.
    pair.alice
        .queue_path_response(challenge.to_vec())
        .expect("answer");
    let response_bytes = seal(&mut pair.alice, pair.now_ms);
    assert!(response_bytes.len() <= 1280);
    // Bob receives the genuine response from the candidate endpoint.
    let outcome = pair
        .bob
        .receive_datagram(pair.now_ms, NOW_SECS, &response_bytes);
    assert!(outcome.dropped.is_none());
    assert!(
        outcome
            .events
            .iter()
            .any(|event| matches!(event, SessionEvent::PathResponse(_))),
        "response arrives as a typed session event"
    );
    let event = pair
        .bob_path
        .on_path_response(candidate, &challenge, pair.now_ms)
        .expect("migrate");
    assert_eq!(
        event,
        PathEvent::Validated {
            previous: pair.alice_addr,
            current: candidate,
        }
    );
    pair.bob.note_path_migrated();
    assert_eq!(pair.bob_path.validated().endpoint(), candidate);
    assert_eq!(pair.bob_path.candidate_count(), 0);
    // The peer's own validator is unaffected by Bob's migration.
    assert_eq!(pair.alice_path.validated().endpoint(), pair.bob_addr);
    assert_eq!(pair.alice_path.candidate_count(), 0);
    // The proof is consumed: an identical replay migrates nothing.
    assert_eq!(
        pair.bob_path
            .on_path_response(candidate, &challenge, pair.now_ms),
        Err(PathError::NotACandidate)
    );
    assert_eq!(pair.bob_path.counters().migrations, 1);
    // Post-migration traffic on the new path delivers exactly once.
    pair.alice
        .queue_i2np_message(i2np_bytes(6, 0xD002, 64, 0x62))
        .expect("queue");
    let bytes = seal(&mut pair.alice, pair.now_ms);
    let outcome = pair.bob.receive_datagram(pair.now_ms, NOW_SECS, &bytes);
    assert!(outcome.dropped.is_none());
    assert!(
        outcome
            .events
            .iter()
            .any(|event| matches!(event, SessionEvent::I2npMessage(_))),
        "delivery continues on the migrated path"
    );
    // The reverse direction stays usable on the same migrated session.
    pair.bob
        .queue_i2np_message(i2np_bytes(6, 0xD003, 64, 0x63))
        .expect("queue");
    let events = pair.bob_to_alice();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, SessionEvent::I2npMessage(_))),
        "reverse delivery continues after migration"
    );
}

#[test]
fn candidate_timeout_retains_old_path() {
    let mut pair = Pair::new();
    let candidate = v4(9, 20009);
    pair.alice
        .queue_i2np_message(i2np_bytes(6, 0xE001, 64, 0x71))
        .expect("queue");
    pair.alice_to_bob(candidate).expect("candidate");
    let challenge = pair
        .bob_path
        .challenge_for(candidate)
        .expect("issued challenge");
    pair.now_ms += PATH_VALIDATION_TIMEOUT_MS + 1;
    let expired = pair.bob_path.poll_expired(pair.now_ms);
    assert_eq!(expired, vec![candidate]);
    assert_eq!(
        pair.bob_path
            .on_path_response(candidate, &challenge, pair.now_ms),
        Err(PathError::NotACandidate)
    );
    assert_eq!(pair.bob_path.validated().endpoint(), pair.alice_addr);
}

#[test]
fn v4_packet_cannot_validate_v6_candidate_with_real_seals() {
    let mut pair = Pair::new();
    let candidate_v6 = v6(7, 27007);
    pair.alice
        .queue_i2np_message(i2np_bytes(6, 0xF001, 64, 0x81))
        .expect("queue");
    pair.alice_to_bob(candidate_v6).expect("candidate");
    let challenge = pair
        .bob_path
        .challenge_for(candidate_v6)
        .expect("issued challenge");
    // Same bytes from a v4 endpoint prove nothing about the v6 path.
    assert_eq!(
        pair.bob_path
            .on_path_response(v4(9, 20009), &challenge, pair.now_ms),
        Err(PathError::NotACandidate)
    );
    assert_eq!(pair.bob_path.validated().endpoint(), pair.alice_addr);
    assert_eq!(
        candidate_v6.family(),
        AddressFamily::Ipv6,
        "candidate family is structurally separated"
    );
}

#[test]
fn migration_resets_flight_but_keeps_semantic_queues() {
    let mut pair = Pair::new();
    // Put bytes in flight on Bob's transmit side without delivery.
    pair.bob
        .queue_i2np_message(i2np_bytes(6, 0xB101, 900, 0x91))
        .expect("queue");
    let _held = seal(&mut pair.bob, pair.now_ms);
    assert!(pair.bob.bytes_in_flight() > 0);
    assert!(pair.bob.counters().bytes_in_flight > 0);
    // Migrate Bob's path on proof, then apply the congestion reset.
    let candidate = v4(9, 20009);
    pair.alice
        .queue_i2np_message(i2np_bytes(6, 0xB102, 64, 0x92))
        .expect("queue");
    pair.alice_to_bob(candidate).expect("candidate");
    let challenge = pair
        .bob_path
        .challenge_for(candidate)
        .expect("issued challenge");
    pair.bob_path
        .on_path_response(candidate, &challenge, pair.now_ms)
        .expect("migrate");
    pair.bob.note_path_migrated();
    assert_eq!(pair.bob.bytes_in_flight(), 0);
    assert_eq!(pair.bob.counters().bytes_in_flight, 0);
    assert_eq!(pair.bob.cwnd_bytes(), constants::DATA_MIN_CWND_BYTES);
    // The semantic message survives: a fresh seal still carries it and
    // Alice delivers it exactly once.
    let bytes = seal(&mut pair.bob, pair.now_ms);
    let outcome = pair.alice.receive_datagram(pair.now_ms, NOW_SECS, &bytes);
    assert!(outcome.dropped.is_none());
    assert!(
        outcome
            .events
            .iter()
            .any(|event| matches!(event, SessionEvent::I2npMessage(_))),
        "unacknowledged message retransmits fresh on the new path"
    );
}

#[test]
fn challenge_response_controls_fit_minimum_mtu() {
    let mut pair = Pair::new();
    pair.alice
        .queue_path_challenge(vec![0xAA_u8; 32])
        .expect("challenge");
    let challenge_bytes = seal(&mut pair.alice, pair.now_ms);
    assert!(
        challenge_bytes.len() <= 1280,
        "challenge control fits the candidate-path minimum MTU"
    );
    let events = receive(&mut pair.bob, &challenge_bytes, pair.now_ms);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, SessionEvent::PathChallenge(_))),
        "challenge decodes as a typed event"
    );
    let data = events
        .iter()
        .find_map(|event| match event {
            SessionEvent::PathChallenge(data) => Some(data.clone()),
            _ => None,
        })
        .expect("challenge data");
    pair.bob.queue_path_response(data).expect("answer");
    let response_bytes = seal(&mut pair.bob, pair.now_ms);
    assert!(
        response_bytes.len() <= 1280,
        "response control fits the candidate-path minimum MTU"
    );
}
