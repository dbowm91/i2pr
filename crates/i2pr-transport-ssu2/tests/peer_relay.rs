//! Plan 160 sealed-packet trajectories for PeerTest and relay.
//!
//! Two paired [`Ssu2Session`] endpoints (same Noise-dance helper as
//! the Plan 159 path suite) exchange byte-identical wire datagrams
//! carrying in-session RelayRequest/RelayResponse/RelayIntro and
//! PeerTest blocks. The harness feeds the decoded full-block
//! [`SessionEvent`]s into the runtime-neutral tables
//! ([`PeerTestTable`], [`RelayRequester`], [`RelayIntroducer`],
//! [`RelayTarget`], [`IntroducerTable`]) exactly as the runtime does
//! after session authentication: correlation/role/sender/signature/
//! freshness/endpoint checks first, reachability evidence only from
//! authenticated typed outcomes.
//!
//! Out-of-session PeerTest (Msgs 5–7) and HolePunch (type 11) use the
//! intro-key AEAD codecs directly (no session), mirroring the runtime's
//! out-of-session path. Publication and reachability integration close
//! the loop without sockets; real-UDP NAT-like acceptance lives in
//! `i2pr-runtime` (Plan 160 §8).

use std::net::{IpAddr, Ipv4Addr};

use i2pr_crypto::SigningPrivateKey;
use i2pr_crypto::X25519PrivateKey;
use i2pr_proto::SigningPublicKey;
use i2pr_transport::{AddressFamily, PeerTestOutcomeKind, ReachabilitySignal, ReachabilityState};
use i2pr_transport_ssu2::{
    IntroKey, IntroducerProvenance, IntroducerRecord, IntroducerTable, PeerTestBlock,
    PeerTestOutcome, PeerTestRole, PeerTestTable, PublicationPolicy, PublicationRequest,
    RelayIntroBlock, RelayIntroducer, RelayRequestBlock, RelayRequester, RelayResponseBlock,
    RelayTarget, Role, SessionConfig, SessionEvent, Ssu2Capabilities, Ssu2Endpoint, Ssu2PublicKey,
    Ssu2Session, Ssu2SplitKeys, Ssu2Transcript, build_hole_punch, build_out_of_session_peer_test,
    hole_punch_conn_ids, parse_hole_punch, parse_out_of_session_peer_test, peer_test_conn_ids,
    peer_test_preimage, relay_request_preimage, relay_response_preimage,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const NOW_MS: u64 = 2_000_000;
const NOW_SECS: u64 = 1_700_000_000;
const LOCAL_CONN_A: u64 = 0xaaaa_aaaa_aaaa_aaaa;
const LOCAL_CONN_B: u64 = 0xbbbb_bbbb_bbbb_bbbb;

const ALICE_HASH: [u8; 32] = [0xA1; 32];
const BOB_HASH: [u8; 32] = [0x0B; 32];
const CHARLIE_HASH: [u8; 32] = [0xC4; 32];

fn secret(seed: u64) -> X25519PrivateKey {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    X25519PrivateKey::generate(&mut rng).expect("deterministic secret")
}

fn public_of(key: &X25519PrivateKey) -> Ssu2PublicKey {
    Ssu2PublicKey::new(key.public_bytes()).expect("public")
}

fn intro(byte: u8) -> IntroKey {
    IntroKey::new([byte; 32])
}

fn address_intro(byte: u8) -> i2pr_transport_ssu2::address::IntroKey {
    i2pr_transport_ssu2::address::IntroKey::new([byte; 32]).expect("address intro")
}

fn paired_splits() -> (Ssu2SplitKeys, Ssu2SplitKeys) {
    let bob_static = secret(5001);
    let bob_public = public_of(&bob_static);
    let alice_static = secret(5002);
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
    let alice_public = public_of(&secret(5002));
    let (alice, frame) = alice.seal_confirmed_static(alice_public).expect("static");
    let (bob, _) = bob.accept_confirmed_static(&frame).expect("open static");
    let se_alice = secret(5002)
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
    // Silence unused warning for alice_static (key material is consumed
    // through the transcript above).
    let _ = alice_static;
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

fn alice_key() -> SigningPrivateKey {
    SigningPrivateKey::from_bytes([0x11; 32])
}

fn charlie_key() -> SigningPrivateKey {
    SigningPrivateKey::from_bytes([0x33; 32])
}

fn alice_pub() -> SigningPublicKey {
    alice_key().public_key().expect("alice pub")
}

fn charlie_pub() -> SigningPublicKey {
    charlie_key().public_key().expect("charlie pub")
}

fn sign_request(nonce: u32, tag: u32, timestamp: u32, endpoint: Ssu2Endpoint) -> Vec<u8> {
    let preimage =
        relay_request_preimage(&BOB_HASH, &CHARLIE_HASH, nonce, tag, timestamp, 2, endpoint);
    alice_key()
        .sign(&preimage)
        .expect("sign")
        .as_bytes()
        .to_vec()
}

fn sign_response(nonce: u32, timestamp: u32, endpoint: Ssu2Endpoint) -> Vec<u8> {
    let preimage = relay_response_preimage(&BOB_HASH, nonce, timestamp, 2, Some(endpoint));
    charlie_key()
        .sign(&preimage)
        .expect("sign")
        .as_bytes()
        .to_vec()
}

fn sign_peer4(nonce: u32, timestamp: u32, endpoint: Ssu2Endpoint) -> Vec<u8> {
    let preimage = peer_test_preimage(
        4,
        &BOB_HASH,
        Some(&ALICE_HASH),
        2,
        nonce,
        timestamp,
        endpoint,
    );
    charlie_key()
        .sign(&preimage)
        .expect("sign")
        .as_bytes()
        .to_vec()
}

/// Moves one sealed datagram from `from` to `to`, returning the
/// authenticated events (panics on drop: the harness only validates
/// authenticated packets, mirroring the runtime ordering).
fn carry(from: &mut Ssu2Session, to: &mut Ssu2Session, now_ms: u64) -> Vec<SessionEvent> {
    let bytes = from.poll_transmit(now_ms).expect("sealed datagram");
    assert!(bytes.len() <= 1280, "fits the minimum MTU");
    let outcome = to.receive_datagram(now_ms, NOW_SECS, &bytes);
    assert!(
        outcome.dropped.is_none(),
        "authenticated packet must not drop, dropped={:?}",
        outcome.dropped
    );
    outcome.events
}

#[test]
fn relay_request_traverses_session_to_introducer() {
    let (mut alice, mut bob) = paired_sessions();
    let mut introducer = RelayIntroducer::enabled_for_tests();
    introducer.issue_tag(7, ALICE_HASH, NOW_SECS).expect("tag");
    let endpoint = v4(10, 40000);
    let block = RelayRequestBlock::new(
        11,
        7,
        NOW_SECS as u32,
        2,
        endpoint,
        sign_request(11, 7, NOW_SECS as u32, endpoint),
    )
    .expect("request");
    alice.queue_relay_request(block.clone()).expect("queue");
    let events = carry(&mut alice, &mut bob, NOW_MS);
    let request = events
        .iter()
        .find_map(|event| match event {
            SessionEvent::RelayRequest(block) => Some(block.clone()),
            _ => None,
        })
        .expect("relay request event carries the full block");
    assert_eq!(request.nonce(), 11);
    assert_eq!(request.relay_tag(), 7);
    // Introducer admits the authenticated request and would emit one
    // RelayIntro (no amplification on replay tested below).
    assert!(
        introducer
            .on_request(
                &request,
                &ALICE_HASH,
                &BOB_HASH,
                &CHARLIE_HASH,
                &alice_pub(),
                200,
                NOW_SECS,
                NOW_MS
            )
            .expect("admit")
    );
    assert!(
        !introducer
            .on_request(
                &request,
                &ALICE_HASH,
                &BOB_HASH,
                &CHARLIE_HASH,
                &alice_pub(),
                200,
                NOW_SECS,
                NOW_MS + 10
            )
            .expect("replay")
    );
}

#[test]
fn relay_response_returns_and_second_tag_does_not_cross_contaminate() {
    let (mut alice, mut bob) = paired_sessions();
    let mut requester = RelayRequester::new();
    let alice_endpoint = v4(10, 40000);
    requester
        .start(11, 7, BOB_HASH, CHARLIE_HASH, alice_endpoint, NOW_MS)
        .expect("first");
    requester
        .start(12, 8, BOB_HASH, CHARLIE_HASH, v4(11, 40001), NOW_MS)
        .expect("second");
    let charlie_endpoint = v4(30, 50000);
    let first = RelayResponseBlock::accept(
        11,
        NOW_SECS as u32,
        2,
        charlie_endpoint,
        sign_response(11, NOW_SECS as u32, charlie_endpoint),
        99,
    )
    .expect("accept");
    bob.queue_relay_response(first.clone()).expect("queue");
    let events = carry(&mut bob, &mut alice, NOW_MS + 5);
    let response = events
        .iter()
        .find_map(|event| match event {
            SessionEvent::RelayResponse(block) => Some(block.clone()),
            _ => None,
        })
        .expect("relay response event");
    assert_eq!(response.nonce(), 11);
    assert_eq!(
        requester.on_response(
            &response,
            &BOB_HASH,
            &BOB_HASH,
            Some(&charlie_pub()),
            NOW_SECS,
            NOW_MS + 5
        ),
        Ok(i2pr_transport_ssu2::relay::RequesterState::AwaitingHolePunch)
    );
    // Second request is untouched: its own response advances it
    // independently (distinct-tag isolation).
    let second = RelayResponseBlock::accept(
        12,
        NOW_SECS as u32,
        2,
        charlie_endpoint,
        sign_response(12, NOW_SECS as u32, charlie_endpoint),
        100,
    )
    .expect("accept");
    assert_eq!(
        requester.on_response(
            &second,
            &BOB_HASH,
            &BOB_HASH,
            Some(&charlie_pub()),
            NOW_SECS,
            NOW_MS + 6
        ),
        Ok(i2pr_transport_ssu2::relay::RequesterState::AwaitingHolePunch)
    );
}

#[test]
fn relay_intro_reaches_target_and_replay_does_not_reamplify() {
    let (mut bob, mut charlie) = paired_sessions();
    let mut target = RelayTarget::new();
    let endpoint = v4(10, 40000);
    let request = RelayRequestBlock::new(
        31,
        7,
        NOW_SECS as u32,
        2,
        endpoint,
        sign_request(31, 7, NOW_SECS as u32, endpoint),
    )
    .expect("request");
    let intro = RelayIntroBlock::new(
        ALICE_HASH,
        request.nonce(),
        request.relay_tag(),
        request.timestamp(),
        request.version(),
        request.endpoint(),
        request.signature().to_vec(),
    )
    .expect("intro");
    bob.queue_relay_intro(intro.clone()).expect("queue");
    let events = carry(&mut bob, &mut charlie, NOW_MS);
    let received = events
        .iter()
        .find_map(|event| match event {
            SessionEvent::RelayIntro(block) => Some(block.clone()),
            _ => None,
        })
        .expect("relay intro event");
    assert!(
        target
            .on_intro(
                &received,
                &BOB_HASH,
                &CHARLIE_HASH,
                &alice_pub(),
                NOW_SECS,
                NOW_MS
            )
            .expect("admit")
    );
    // Replay of the same intro never triggers a second HolePunch.
    assert!(
        !target
            .on_intro(
                &received,
                &BOB_HASH,
                &CHARLIE_HASH,
                &alice_pub(),
                NOW_SECS,
                NOW_MS + 10
            )
            .expect("replay")
    );
    // Stale intro fails closed without state.
    let stale = RelayIntroBlock::new(
        ALICE_HASH,
        32,
        7,
        (NOW_SECS - 10_000) as u32,
        2,
        endpoint,
        {
            let preimage = relay_request_preimage(
                &BOB_HASH,
                &CHARLIE_HASH,
                32,
                7,
                (NOW_SECS - 10_000) as u32,
                2,
                endpoint,
            );
            alice_key()
                .sign(&preimage)
                .expect("sign")
                .as_bytes()
                .to_vec()
        },
    )
    .expect("stale intro");
    assert!(
        target
            .on_intro(
                &stale,
                &BOB_HASH,
                &CHARLIE_HASH,
                &alice_pub(),
                NOW_SECS,
                NOW_MS + 20
            )
            .is_err()
    );
}

#[test]
fn peer_test_msg4_traverses_session_and_invalid_inputs_are_bounded() {
    let (mut bob, mut alice) = paired_sessions();
    let mut table = PeerTestTable::new();
    let observed = v4(10, 40000);
    table
        .start(
            111,
            PeerTestRole::Alice,
            ALICE_HASH,
            BOB_HASH,
            CHARLIE_HASH,
            observed,
            NOW_MS,
        )
        .expect("start");
    let block = PeerTestBlock::new(
        4,
        0,
        Some(CHARLIE_HASH),
        2,
        111,
        NOW_SECS as u32,
        observed,
        sign_peer4(111, NOW_SECS as u32, observed),
    )
    .expect("msg4");
    bob.queue_peer_test(block.clone()).expect("queue");
    let events = carry(&mut bob, &mut alice, NOW_MS);
    let received = events
        .iter()
        .find_map(|event| match event {
            SessionEvent::PeerTest(block) => Some(block.clone()),
            _ => None,
        })
        .expect("peer-test event carries the full block");
    assert_eq!(received.message(), 4);
    // Feed to the table as the runtime does after session auth.
    let outcome = table
        .ingest(
            &received,
            &BOB_HASH,
            v4(20, 5000),
            &BOB_HASH,
            Some(&ALICE_HASH),
            Some(&charlie_pub()),
            received.signature(),
            NOW_SECS,
            NOW_MS,
        )
        .expect("ingest");
    assert_eq!(outcome, None, "Msg 4 alone never confirms");
    // Invalid signature over the same sealed shape fails closed.
    let bad = PeerTestBlock::new(
        4,
        0,
        Some(CHARLIE_HASH),
        2,
        111,
        NOW_SECS as u32,
        observed,
        vec![0xEE; 64],
    )
    .expect("bad shape");
    assert!(
        table
            .ingest(
                &bad,
                &BOB_HASH,
                v4(20, 5000),
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                bad.signature(),
                NOW_SECS,
                NOW_MS
            )
            .is_err()
    );
}

#[test]
fn out_of_session_peer_test_and_hole_punch_round_trip() {
    // Msgs 5/7 under Alice's intro key; Msg 6 under Charlie's.
    let alice_intro = intro(0xA0);
    let charlie_intro = intro(0xC0);
    let observed = v4(10, 40000);
    let msg5 = PeerTestBlock::new(5, 0, None, 2, 77, NOW_SECS as u32, observed, {
        let preimage = peer_test_preimage(
            5,
            &BOB_HASH,
            Some(&ALICE_HASH),
            2,
            77,
            NOW_SECS as u32,
            observed,
        );
        charlie_key()
            .sign(&preimage)
            .expect("sign")
            .as_bytes()
            .to_vec()
    })
    .expect("msg5");
    let (dest, src) = peer_test_conn_ids(77, false);
    let mut datagram =
        build_out_of_session_peer_test(&alice_intro, src, dest, 0x12345678, msg5.clone())
            .expect("build msg5");
    let (header, parsed) =
        parse_out_of_session_peer_test(&mut datagram, &alice_intro).expect("parse msg5");
    assert_eq!(header.dst_conn_id(), dest);
    assert_eq!(parsed.nonce(), 77);
    // Wrong intro key fails closed (no amplification to victims).
    let mut tampered = datagram.clone();
    assert!(parse_out_of_session_peer_test(&mut tampered, &charlie_intro).is_err());
    // HolePunch under Alice's intro key with the relay token.
    let charlie_endpoint = v4(30, 50000);
    let response = RelayResponseBlock::accept(
        77,
        NOW_SECS as u32,
        2,
        charlie_endpoint,
        sign_response(77, NOW_SECS as u32, charlie_endpoint),
        0x0102030405060708,
    )
    .expect("response");
    let (hdest, hsrc) = hole_punch_conn_ids(77);
    let mut hole = build_hole_punch(
        &alice_intro,
        hsrc,
        hdest,
        0x9ABCDEF0,
        NOW_SECS as u32,
        charlie_endpoint,
        response,
    )
    .expect("holepunch");
    let parsed_hole = parse_hole_punch(&mut hole, &alice_intro).expect("parse hole");
    assert_eq!(parsed_hole.response.token(), Some(0x0102030405060708));
    assert!(parse_hole_punch(&mut hole.clone(), &charlie_intro).is_err());
}

#[test]
fn introducer_records_feed_publication_and_expire_cleanly() {
    let mut table = IntroducerTable::new();
    let endpoint = v4(20, 20000);
    table
        .insert(
            IntroducerRecord::new(
                [0xB0; 32],
                endpoint,
                address_intro(0xB0),
                7,
                NOW_SECS + 600,
                IntroducerProvenance::AuthenticatedDirect,
            )
            .expect("record"),
            NOW_SECS,
        )
        .expect("insert");
    let validated = table.validated_introducers(NOW_SECS);
    assert_eq!(validated.len(), 1);
    // Publication with validated introducers renders the
    // direct-with-introducers form; without opt-in it fails closed.
    let caps = Ssu2Capabilities::empty();
    let request = |allow_introducers: bool| PublicationRequest {
        policy: PublicationPolicy::new(true, allow_introducers),
        static_public: [0x42; 32],
        intro_public: [0x24; 32],
        endpoint: Some(endpoint),
        reachability: i2pr_transport::ReachabilitySnapshot {
            state: i2pr_transport::ReachabilityState::Reachable,
            corroboration: 3,
            expires_at: std::time::Duration::from_secs(NOW_SECS + 600),
            family: AddressFamily::Ipv4,
        },
        mtu: 1280,
        caps: &caps,
        introducers: &validated,
        now_secs: NOW_SECS,
        evidence_expires_secs: NOW_SECS + 600,
    };
    assert!(matches!(
        i2pr_transport_ssu2::build_publication_snapshot(request(true)),
        Ok(i2pr_transport_ssu2::PublicationOutcome::Direct(_))
    ));
    assert!(i2pr_transport_ssu2::build_publication_snapshot(request(false)).is_err());
    // Expiry withdraws: past the lifetime nothing validates.
    assert!(table.validated_introducers(NOW_SECS + 601).is_empty());
}

#[test]
fn reachability_consumes_typed_outcomes_conservatively() {
    use i2pr_transport::{ReachabilityPolicy, ReachabilityTracker};
    let mut tracker = ReachabilityTracker::new(ReachabilityPolicy::default()).expect("policy");
    let now = std::time::Duration::from_secs(1000);
    // One peer observation can never publish Reachable.
    tracker.record(
        ReachabilitySignal::AuthenticatedPeerObservedExternalAddress {
            family: AddressFamily::Ipv4,
        },
        now,
    );
    assert_eq!(tracker.state(), ReachabilityState::ObservedUnconfirmed);
    // Corroborated path + confirmed peer-test promote to candidate,
    // and a third class (configured, opted-in tracker) to Reachable.
    tracker.record(
        ReachabilitySignal::ValidatedPath {
            family: AddressFamily::Ipv4,
        },
        now + std::time::Duration::from_secs(1),
    );
    assert_eq!(tracker.state(), ReachabilityState::CandidateReachable);
    tracker.record(
        ReachabilitySignal::PeerTestResult {
            family: AddressFamily::Ipv4,
            outcome: PeerTestOutcomeKind::Confirmed,
        },
        now + std::time::Duration::from_secs(2),
    );
    // Default policy (no configured-direct) needs 3 classes for
    // Reachable; peer-test is the third class here only when the
    // first two are distinct — path + peer-observed + peer-test = 3.
    assert_eq!(tracker.state(), ReachabilityState::Reachable);
    // Contradictory mismatch downgrades without last-write-wins flip.
    tracker.record(
        ReachabilitySignal::PeerTestResult {
            family: AddressFamily::Ipv4,
            outcome: PeerTestOutcomeKind::AddressMismatch,
        },
        now + std::time::Duration::from_secs(3),
    );
    assert_eq!(tracker.state(), ReachabilityState::ObservedUnconfirmed);
    // Inconclusive never flips arbitrarily (stays unconfirmed here).
    tracker.record(
        ReachabilitySignal::PeerTestResult {
            family: AddressFamily::Ipv4,
            outcome: PeerTestOutcomeKind::Inconclusive,
        },
        now + std::time::Duration::from_secs(4),
    );
    assert_eq!(tracker.state(), ReachabilityState::ObservedUnconfirmed);
    // Relay success signals firewalled, never direct.
    let mut firewalled = ReachabilityTracker::new(ReachabilityPolicy::default()).expect("policy");
    firewalled.record(
        ReachabilitySignal::PeerTestResult {
            family: AddressFamily::Ipv4,
            outcome: PeerTestOutcomeKind::FirewalledLikely,
        },
        now,
    );
    firewalled.record(
        ReachabilitySignal::RelayFirewalledSignal {
            family: AddressFamily::Ipv4,
        },
        now + std::time::Duration::from_secs(1),
    );
    assert_eq!(firewalled.state(), ReachabilityState::Firewalled);
    // Full Alice trajectory outcome maps to the Confirmed kind.
    let mut alice_table = PeerTestTable::new();
    let observed = v4(10, 40000);
    alice_table
        .start(
            900,
            PeerTestRole::Alice,
            ALICE_HASH,
            BOB_HASH,
            CHARLIE_HASH,
            observed,
            NOW_MS,
        )
        .expect("start");
    let m4 = PeerTestBlock::new(
        4,
        0,
        Some(CHARLIE_HASH),
        2,
        900,
        NOW_SECS as u32,
        observed,
        sign_peer4(900, NOW_SECS as u32, observed),
    )
    .expect("m4");
    let sig4 = m4.signature().to_vec();
    alice_table
        .ingest(
            &m4,
            &BOB_HASH,
            v4(20, 5000),
            &BOB_HASH,
            Some(&ALICE_HASH),
            Some(&charlie_pub()),
            &sig4,
            NOW_SECS,
            NOW_MS,
        )
        .expect("m4");
    let m5 = PeerTestBlock::new(5, 0, None, 2, 900, NOW_SECS as u32 + 1, observed, {
        let preimage = peer_test_preimage(
            5,
            &BOB_HASH,
            Some(&ALICE_HASH),
            2,
            900,
            NOW_SECS as u32 + 1,
            observed,
        );
        charlie_key()
            .sign(&preimage)
            .expect("sign")
            .as_bytes()
            .to_vec()
    })
    .expect("m5");
    let sig5 = m5.signature().to_vec();
    alice_table
        .ingest(
            &m5,
            &CHARLIE_HASH,
            observed,
            &BOB_HASH,
            Some(&ALICE_HASH),
            Some(&charlie_pub()),
            &sig5,
            NOW_SECS + 1,
            NOW_MS + 100,
        )
        .expect("m5");
    let m7 = PeerTestBlock::new(7, 0, None, 2, 900, NOW_SECS as u32 + 2, observed, {
        let preimage = peer_test_preimage(
            7,
            &BOB_HASH,
            Some(&ALICE_HASH),
            2,
            900,
            NOW_SECS as u32 + 2,
            observed,
        );
        charlie_key()
            .sign(&preimage)
            .expect("sign")
            .as_bytes()
            .to_vec()
    })
    .expect("m7");
    let sig7 = m7.signature().to_vec();
    let outcome = alice_table
        .ingest(
            &m7,
            &CHARLIE_HASH,
            observed,
            &BOB_HASH,
            Some(&ALICE_HASH),
            Some(&charlie_pub()),
            &sig7,
            NOW_SECS + 2,
            NOW_MS + 200,
        )
        .expect("m7")
        .expect("outcome");
    assert_eq!(
        outcome,
        PeerTestOutcome::DirectReachabilityConfirmed {
            family: AddressFamily::Ipv4,
            observed,
            evidence_peers: 2
        }
    );
}

#[test]
fn privacy_regression_tables_and_outcomes_expose_no_secrets() {
    let table = PeerTestTable::new();
    let rendered = format!("{table:?}");
    assert!(!rendered.contains("A1A1"));
    let requester = RelayRequester::new();
    assert!(!format!("{requester:?}").contains("A1A1"));
    let introducer = RelayIntroducer::disabled();
    let rendered = format!("{introducer:?}");
    assert!(rendered.contains("enabled"));
    assert!(!rendered.contains("A1A1"));
    assert!(!rendered.contains("0xA1"));
    let target = RelayTarget::new();
    assert!(!format!("{target:?}").contains("A1A1"));
    let records = IntroducerTable::new();
    assert!(!format!("{records:?}").contains("B0B0"));
}
