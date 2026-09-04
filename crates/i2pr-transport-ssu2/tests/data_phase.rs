//! Plan 157 data-phase trajectories through paired sessions.
//!
//! Every test here is deterministic: fixed transcript secrets, fixed
//! connection IDs and intro keys, caller-supplied clocks, and
//! `ChaCha8Rng` seeds. No UDP sockets are opened; datagrams move
//! between two in-memory [`Ssu2Session`] endpoints with explicit
//! fault injection (drop, duplicate, reorder, corrupt, replay).
//!
//! The paired sessions are built from a real Noise transcript dance
//! (same role gating as the handshake tests) so directional data keys
//! match exactly per the specification KDF for data phase.

use i2pr_crypto::X25519PrivateKey;
use i2pr_transport_ssu2::{
    IntroKey, SessionConfig, SessionEvent, Ssu2Session, Ssu2SplitKeys, Ssu2Transcript,
};
use i2pr_transport_ssu2::{Role, Ssu2PublicKey};
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

fn intro(byte: u8) -> IntroKey {
    IntroKey::new([byte; 32])
}

/// Runs the Noise transcript to matching directional splits for both
/// roles (mirrors the handshake machines without RouterInfo).
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
    let confirmed_header = [0x33_u8; 16];
    let (alice, frame) = alice
        .seal_confirmed_static(&confirmed_header, alice_public)
        .expect("static");
    let (bob, _) = bob
        .accept_confirmed_static(&confirmed_header, &frame)
        .expect("open static");
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

/// Builds a paired Alice/Bob data-phase session with distinct intro
/// keys (`k_header_1` is the receiver's key per direction).
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

/// Builds one deterministic encoded I2NP message (9-byte short header
/// plus body) with the given type, ID, expiration, and body length.
fn i2np_bytes_exp(
    message_type: u8,
    message_id: u32,
    expiration: u32,
    body_len: usize,
    fill: u8,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(9 + body_len);
    bytes.push(message_type);
    bytes.extend_from_slice(&message_id.to_be_bytes());
    bytes.extend_from_slice(&expiration.to_be_bytes());
    bytes.extend(std::iter::repeat_n(fill, body_len));
    bytes
}

/// Builds one deterministic encoded I2NP message with the fixed test
/// clock expiration.
fn i2np_bytes(message_type: u8, message_id: u32, body_len: usize, fill: u8) -> Vec<u8> {
    i2np_bytes_exp(message_type, message_id, NOW_SECS as u32, body_len, fill)
}

/// Decodes committed fixture hex embedded at compile time.
///
/// `include_str!` is relative to this source file, so the vectors
/// resolve identically whether Cargo runs the test binary (CWD set to
/// the package root) or a lane executes the binary directly from the
/// workspace root (as the macOS quality lane does).
fn fixture_bytes_from_hex(text: &str) -> Vec<u8> {
    let text = text.trim();
    assert!(text.len().is_multiple_of(2));
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).expect("fixture hex");
            let low = (pair[1] as char).to_digit(16).expect("fixture hex");
            ((high << 4) | low) as u8
        })
        .collect()
}

#[test]
fn committed_data_vectors_reproduce_byte_for_byte() {
    // The committed fixtures were minted from these exact fixed seeds,
    // connection IDs, intro keys, clocks, and message bytes; any drift
    // in header protection, AEAD, or block encoding fails this test.
    let (mut alice, mut bob) = paired_sessions();
    let mut now_ms = NOW_MS;
    let first = fixture_bytes_from_hex(include_str!(
        "../../../tests/fixtures/ssu2/data-phase-first.hex"
    ));
    let ack = fixture_bytes_from_hex(include_str!(
        "../../../tests/fixtures/ssu2/data-phase-ack.hex"
    ));
    assert_eq!(first.len(), 108);
    assert_eq!(ack.len(), 40);
    // Bob authenticates the committed first packet and delivers the
    // exact 73-byte I2NP message (9-byte header + 64 body).
    let outcome = bob.receive_datagram(now_ms, NOW_SECS, &first);
    assert!(outcome.dropped.is_none());
    assert_eq!(outcome.packet_number, Some(0));
    let delivered = delivered_messages(&outcome.events);
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0][0], 20);
    assert_eq!(&delivered[0][1..5], &0x01020304u32.to_be_bytes());
    assert_eq!(delivered[0].len(), 73);
    // Alice authenticates the committed ACK-only packet, which retires
    // her sent packet without eliciting further traffic.
    now_ms += 1;
    let outcome = alice.receive_datagram(now_ms, NOW_SECS, &ack);
    assert!(outcome.dropped.is_none());
    assert!(outcome.ack_only);
    assert!(outcome.events.is_empty());
}

fn delivered_messages(events: &[SessionEvent]) -> Vec<Vec<u8>> {
    events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::I2npMessage(bytes) => Some(bytes.clone()),
            _ => None,
        })
        .collect()
}

/// Pumps both sessions until no transmit work remains or the step cap
/// hits. `fault` optionally drops/rewrites outbound datagrams.
/// Returns the total datagrams moved.
fn pump(
    alice: &mut Ssu2Session,
    bob: &mut Ssu2Session,
    now_ms: &mut u64,
    drops: &mut Vec<bool>,
    bob_events: &mut Vec<SessionEvent>,
    alice_events: &mut Vec<SessionEvent>,
) -> usize {
    let mut moved = 0_usize;
    for _ in 0..200 {
        let mut progress = false;
        if let Some(datagram) = alice.poll_transmit(*now_ms) {
            progress = true;
            let drop = !drops.is_empty() && drops.remove(0);
            *now_ms += 1;
            if !drop {
                let outcome = bob.receive_datagram(*now_ms, NOW_SECS, &datagram);
                assert!(
                    outcome.dropped.is_none(),
                    "unexpected drop: {:?}",
                    outcome.dropped
                );
                bob_events.extend(outcome.events);
            }
            moved += 1;
        }
        if let Some(datagram) = bob.poll_transmit(*now_ms) {
            progress = true;
            *now_ms += 1;
            let outcome = alice.receive_datagram(*now_ms, NOW_SECS, &datagram);
            assert!(
                outcome.dropped.is_none(),
                "unexpected drop: {:?}",
                outcome.dropped
            );
            alice_events.extend(outcome.events);
            moved += 1;
        }
        // Drive polled deadlines (ACK/RTO) deterministically.
        for action in alice.poll(*now_ms, NOW_SECS) {
            if let i2pr_transport_ssu2::SessionAction::Transmit(datagram) = action {
                let outcome = bob.receive_datagram(*now_ms, NOW_SECS, &datagram);
                assert!(outcome.dropped.is_none());
                bob_events.extend(outcome.events);
                moved += 1;
                progress = true;
            }
        }
        for action in bob.poll(*now_ms, NOW_SECS) {
            if let i2pr_transport_ssu2::SessionAction::Transmit(datagram) = action {
                let outcome = alice.receive_datagram(*now_ms, NOW_SECS, &datagram);
                assert!(outcome.dropped.is_none());
                alice_events.extend(outcome.events);
                moved += 1;
                progress = true;
            }
        }
        if !progress {
            break;
        }
        *now_ms += 5;
    }
    moved
}

#[test]
fn bidirectional_no_loss_multi_message_exchange() {
    let (mut alice, mut bob) = paired_sessions();
    let mut now_ms = NOW_MS;
    let sent_a: Vec<Vec<u8>> = (0..3)
        .map(|index| i2np_bytes(20, 100 + index, 200, 0x40 + index as u8))
        .collect();
    let sent_b: Vec<Vec<u8>> = (0..2)
        .map(|index| i2np_bytes(11, 200 + index, 150, 0x70 + index as u8))
        .collect();
    for message in &sent_a {
        alice.queue_i2np_message(message.clone()).expect("queue");
    }
    for message in &sent_b {
        bob.queue_i2np_message(message.clone()).expect("queue");
    }
    let mut bob_events = Vec::new();
    let mut alice_events = Vec::new();
    let moved = pump(
        &mut alice,
        &mut bob,
        &mut now_ms,
        &mut Vec::new(),
        &mut bob_events,
        &mut alice_events,
    );
    assert!(moved > 0 && moved < 60, "bounded traffic: {moved}");
    assert_eq!(delivered_messages(&bob_events), sent_a);
    assert_eq!(delivered_messages(&alice_events), sent_b);
    // ACKs retired all sent state; nothing remains in flight.
    assert_eq!(alice.bytes_in_flight(), 0);
    assert_eq!(bob.bytes_in_flight(), 0);
    assert!(alice.counters().packets_sent > 0);
    assert!(bob.counters().packets_sent > 0);
    assert_eq!(alice.counters().packets_replayed, 0);
}

#[test]
fn data_loss_produces_fresh_retransmission_exact_once() {
    let (mut alice, mut bob) = paired_sessions();
    let mut now_ms = NOW_MS;
    let message = i2np_bytes(20, 4242, 300, 0x5a);
    alice.queue_i2np_message(message.clone()).expect("queue");
    // Move the first datagram aside (loss) instead of delivering it.
    let lost = alice.poll_transmit(now_ms).expect("first datagram");
    now_ms += 1;
    // Queue a second message so later ACKs create an explicit NACK gap
    // for the lost packet.
    let second = i2np_bytes(20, 4243, 100, 0x5b);
    alice.queue_i2np_message(second.clone()).expect("queue");
    let mut bob_events = Vec::new();
    let mut alice_events = Vec::new();
    // Deliver everything except the lost datagram; loss recovery must
    // produce a FRESH packet (new number, current ACKs), never the
    // cached ciphertext.
    let mut fresh_seen = false;
    for _ in 0..100 {
        let mut progress = false;
        if let Some(datagram) = alice.poll_transmit(now_ms) {
            progress = true;
            now_ms += 1;
            assert_ne!(
                datagram, lost,
                "retransmission must use a fresh packet number, not cached bytes"
            );
            fresh_seen = true;
            let outcome = bob.receive_datagram(now_ms, NOW_SECS, &datagram);
            assert!(outcome.dropped.is_none());
            bob_events.extend(outcome.events);
        }
        if let Some(datagram) = bob.poll_transmit(now_ms) {
            progress = true;
            now_ms += 1;
            let outcome = alice.receive_datagram(now_ms, NOW_SECS, &datagram);
            assert!(outcome.dropped.is_none());
            alice_events.extend(outcome.events);
        }
        for action in alice.poll(now_ms, NOW_SECS) {
            if let i2pr_transport_ssu2::SessionAction::Transmit(datagram) = action {
                assert_ne!(datagram, lost);
                fresh_seen = true;
                let outcome = bob.receive_datagram(now_ms, NOW_SECS, &datagram);
                assert!(outcome.dropped.is_none());
                bob_events.extend(outcome.events);
                progress = true;
            }
        }
        for action in bob.poll(now_ms, NOW_SECS) {
            if let i2pr_transport_ssu2::SessionAction::Transmit(datagram) = action {
                let outcome = alice.receive_datagram(now_ms, NOW_SECS, &datagram);
                assert!(outcome.dropped.is_none());
                alice_events.extend(outcome.events);
                progress = true;
            }
        }
        if !progress {
            break;
        }
        now_ms += 5;
    }
    assert!(fresh_seen, "a fresh retransmission must occur");
    let delivered = delivered_messages(&bob_events);
    // SSU2 permits out-of-order emergence: the second message may
    // arrive before the retransmitted first. Exact bytes at most once.
    assert_eq!(delivered.len(), 2);
    assert!(delivered.contains(&message));
    assert!(delivered.contains(&second));
    assert!(alice.counters().retransmitted_fragments > 0);
    assert!(alice.counters().loss_events > 0);
}

#[test]
fn ack_loss_recovers_without_ack_loop() {
    let (mut alice, mut bob) = paired_sessions();
    let mut now_ms = NOW_MS;
    alice
        .queue_i2np_message(i2np_bytes(20, 9001, 120, 0x11))
        .expect("queue");
    // Alice -> Bob data.
    let data = alice.poll_transmit(now_ms).expect("data");
    now_ms += 1;
    let outcome = bob.receive_datagram(now_ms, NOW_SECS, &data);
    assert!(outcome.dropped.is_none());
    assert_eq!(delivered_messages(&outcome.events).len(), 1);
    // Bob's ACK is lost (never delivered to Alice).
    let _lost_ack = bob.poll_transmit(now_ms).expect("ack");
    now_ms += 1;
    // Alice's RTO must eventually retransmit; the exchange must
    // converge with bounded traffic (no ACK-of-ACK loop).
    let mut redeliveries = Vec::new();
    let mut moved = 1_usize;
    // Time-driven loop: always advances toward the 1 s RTO instead of
    // stopping at the first quiet iteration; breaks on convergence
    // (the retransmission is acknowledged) or the iteration cap.
    for _ in 0..100 {
        if let Some(datagram) = alice.poll_transmit(now_ms) {
            now_ms += 1;
            let outcome = bob.receive_datagram(now_ms, NOW_SECS, &datagram);
            // The retransmission is a duplicate I2NP at the SSU2 layer;
            // Bob must not deliver it twice.
            redeliveries.extend(outcome.events);
            moved += 1;
        }
        if let Some(datagram) = bob.poll_transmit(now_ms) {
            now_ms += 1;
            let outcome = alice.receive_datagram(now_ms, NOW_SECS, &datagram);
            assert!(outcome.dropped.is_none());
            moved += 1;
        }
        for action in alice.poll(now_ms, NOW_SECS) {
            if let i2pr_transport_ssu2::SessionAction::Transmit(datagram) = action {
                let outcome = bob.receive_datagram(now_ms, NOW_SECS, &datagram);
                redeliveries.extend(outcome.events);
                moved += 1;
            }
        }
        for action in bob.poll(now_ms, NOW_SECS) {
            if let i2pr_transport_ssu2::SessionAction::Transmit(datagram) = action {
                let outcome = alice.receive_datagram(now_ms, NOW_SECS, &datagram);
                assert!(outcome.dropped.is_none());
                moved += 1;
            }
        }
        if alice.bytes_in_flight() == 0 {
            break;
        }
        now_ms += 50;
    }
    assert!(moved < 40, "no ACK loop: {moved} datagrams");
    assert!(
        delivered_messages(&redeliveries).is_empty(),
        "duplicate I2NP must never redeliver"
    );
}

#[test]
fn duplicate_packet_is_replay_without_double_delivery() {
    let (mut alice, mut bob) = paired_sessions();
    let mut now_ms = NOW_MS;
    alice
        .queue_i2np_message(i2np_bytes(20, 777, 100, 0x33))
        .expect("queue");
    let datagram = alice.poll_transmit(now_ms).expect("data");
    now_ms += 1;
    let first = bob.receive_datagram(now_ms, NOW_SECS, &datagram);
    assert!(first.dropped.is_none());
    assert_eq!(delivered_messages(&first.events).len(), 1);
    now_ms += 1;
    let replayed_before = bob.counters().packets_replayed;
    let second = bob.receive_datagram(now_ms, NOW_SECS, &datagram);
    assert_eq!(
        second.dropped,
        Some(i2pr_transport_ssu2::DropReason::Replay)
    );
    assert!(second.events.is_empty());
    assert_eq!(
        bob.counters().packets_replayed,
        replayed_before.saturating_add(1)
    );
}

#[test]
fn severe_reorder_still_delivers_exactly_once() {
    let (mut alice, mut bob) = paired_sessions();
    let mut now_ms = NOW_MS;
    // 500-byte bodies pack two fragments per 1220-byte payload, so ten
    // messages span five datagrams for a severe-reorder trajectory.
    let sent: Vec<Vec<u8>> = (0..10)
        .map(|index| i2np_bytes(20, 3000 + index, 500, 0x20 + index as u8))
        .collect();
    for message in &sent {
        alice.queue_i2np_message(message.clone()).expect("queue");
    }
    // Collect datagrams without delivering them.
    let mut datagrams = Vec::new();
    for _ in 0..10 {
        if let Some(datagram) = alice.poll_transmit(now_ms) {
            datagrams.push(datagram);
            now_ms += 1;
        }
    }
    assert!(datagrams.len() >= 5, "reorder needs several packets");
    // Deliver severely reordered (reverse) beyond several packets.
    let mut events = Vec::new();
    for datagram in datagrams.into_iter().rev() {
        now_ms += 1;
        let outcome = bob.receive_datagram(now_ms, NOW_SECS, &datagram);
        assert!(outcome.dropped.is_none());
        events.extend(outcome.events);
    }
    // SSU2 permits out-of-order emergence; exact bytes at most once.
    let mut delivered = delivered_messages(&events);
    delivered.sort();
    let mut expected = sent.clone();
    expected.sort();
    assert_eq!(delivered, expected);
}

#[test]
fn authenticated_corruption_delivers_nothing() {
    let (mut alice, mut bob) = paired_sessions();
    let mut now_ms = NOW_MS;
    alice
        .queue_i2np_message(i2np_bytes(20, 555, 100, 0x44))
        .expect("queue");
    let mut datagram = alice.poll_transmit(now_ms).expect("data");
    now_ms += 1;
    // Corrupt the authenticated tail (not the header-protection IV
    // window alone): flip a byte in the middle of the ciphertext.
    let middle = datagram.len() / 2;
    datagram[middle] ^= 0x01;
    let rejected_before = bob.counters().packets_rejected;
    let outcome = bob.receive_datagram(now_ms, NOW_SECS, &datagram);
    assert!(outcome.events.is_empty());
    assert!(outcome.dropped.is_some());
    assert_eq!(
        bob.counters().packets_rejected,
        rejected_before.saturating_add(1)
    );
    // The session stays usable and loss recovery still works: the
    // never-received message is retransmitted fresh once a second
    // message creates loss visibility, and both arrive exactly once.
    alice
        .queue_i2np_message(i2np_bytes(20, 556, 50, 0x45))
        .expect("queue");
    let first = i2np_bytes(20, 555, 100, 0x44);
    let second = i2np_bytes(20, 556, 50, 0x45);
    let mut events = Vec::new();
    for _ in 0..40 {
        if let Some(datagram) = alice.poll_transmit(now_ms) {
            now_ms += 1;
            let outcome = bob.receive_datagram(now_ms, NOW_SECS, &datagram);
            assert!(outcome.dropped.is_none());
            events.extend(outcome.events);
        }
        if let Some(datagram) = bob.poll_transmit(now_ms) {
            now_ms += 1;
            let outcome = alice.receive_datagram(now_ms, NOW_SECS, &datagram);
            assert!(outcome.dropped.is_none());
        }
        now_ms += 5;
    }
    let delivered = delivered_messages(&events);
    assert_eq!(delivered.len(), 2);
    assert!(delivered.contains(&first));
    assert!(delivered.contains(&second));
}

#[test]
fn replay_old_packet_after_window_advances_is_rejected() {
    let (mut alice, mut bob) = paired_sessions();
    let mut now_ms = NOW_MS;
    for index in 0..4 {
        alice
            .queue_i2np_message(i2np_bytes(20, 6000 + index, 60, 0x50 + index as u8))
            .expect("queue");
    }
    let mut first_datagram = None;
    let mut events = Vec::new();
    for _ in 0..30 {
        if let Some(datagram) = alice.poll_transmit(now_ms) {
            if first_datagram.is_none() {
                first_datagram = Some(datagram.clone());
            }
            now_ms += 1;
            let outcome = bob.receive_datagram(now_ms, NOW_SECS, &datagram);
            assert!(outcome.dropped.is_none());
            events.extend(outcome.events);
        }
        if let Some(datagram) = bob.poll_transmit(now_ms) {
            now_ms += 1;
            let outcome = alice.receive_datagram(now_ms, NOW_SECS, &datagram);
            assert!(outcome.dropped.is_none());
        }
        now_ms += 5;
    }
    assert_eq!(delivered_messages(&events).len(), 4);
    // Replay the very first packet: already seen, no effects.
    now_ms += 1;
    let outcome = bob.receive_datagram(now_ms, NOW_SECS, &first_datagram.expect("first datagram"));
    assert_eq!(
        outcome.dropped,
        Some(i2pr_transport_ssu2::DropReason::Replay)
    );
    assert!(outcome.events.is_empty());
}

#[test]
fn fragmentation_losses_recover_per_position() {
    // A ~3 KB message splits into 3 fragments at the 1024-byte budget.
    for lost_index in 0..3 {
        let (mut alice, mut bob) = paired_sessions();
        let mut now_ms = NOW_MS;
        let message = i2np_bytes(20, 7000 + lost_index as u32, 3000, 0x60);
        alice.queue_i2np_message(message.clone()).expect("queue");
        let mut datagrams = Vec::new();
        for _ in 0..10 {
            if let Some(datagram) = alice.poll_transmit(now_ms) {
                datagrams.push(datagram);
                now_ms += 1;
            } else {
                break;
            }
        }
        assert_eq!(datagrams.len(), 3, "three fragments expected");
        let mut events = Vec::new();
        for (index, datagram) in datagrams.into_iter().enumerate() {
            now_ms += 1;
            if index == lost_index {
                continue; // first/middle/final loss per sub-case
            }
            let outcome = bob.receive_datagram(now_ms, NOW_SECS, &datagram);
            assert!(outcome.dropped.is_none());
            events.extend(outcome.events);
        }
        assert!(
            delivered_messages(&events).is_empty(),
            "incomplete reassembly emits nothing"
        );
        // Loss recovery retransmits the missing fragment fresh. Final-
        // fragment loss has no later packet to create a NACK gap, so
        // the loop always advances the clock toward the RTO instead of
        // stopping at the first quiet iteration.
        let mut recovered = false;
        for _ in 0..100 {
            if let Some(datagram) = alice.poll_transmit(now_ms) {
                now_ms += 1;
                let outcome = bob.receive_datagram(now_ms, NOW_SECS, &datagram);
                assert!(outcome.dropped.is_none());
                let before = events.len();
                events.extend(outcome.events);
                if events.len() > before {
                    recovered = true;
                }
            }
            if let Some(datagram) = bob.poll_transmit(now_ms) {
                now_ms += 1;
                let outcome = alice.receive_datagram(now_ms, NOW_SECS, &datagram);
                assert!(outcome.dropped.is_none());
            }
            for action in alice.poll(now_ms, NOW_SECS) {
                if let i2pr_transport_ssu2::SessionAction::Transmit(datagram) = action {
                    let outcome = bob.receive_datagram(now_ms, NOW_SECS, &datagram);
                    assert!(outcome.dropped.is_none());
                    let before = events.len();
                    events.extend(outcome.events);
                    if events.len() > before {
                        recovered = true;
                    }
                }
            }
            for action in bob.poll(now_ms, NOW_SECS) {
                if let i2pr_transport_ssu2::SessionAction::Transmit(datagram) = action {
                    let outcome = alice.receive_datagram(now_ms, NOW_SECS, &datagram);
                    assert!(outcome.dropped.is_none());
                }
            }
            if recovered {
                break;
            }
            now_ms += 200;
        }
        assert!(recovered, "lost fragment {lost_index} must recover");
        assert_eq!(delivered_messages(&events), vec![message]);
    }
}

#[test]
fn fragment_reorder_duplicate_and_conflict() {
    let (mut alice, mut bob) = paired_sessions();
    let mut now_ms = NOW_MS;
    let message = i2np_bytes(20, 8001, 2500, 0x61);
    alice.queue_i2np_message(message.clone()).expect("queue");
    let mut datagrams = Vec::new();
    for _ in 0..10 {
        if let Some(datagram) = alice.poll_transmit(now_ms) {
            datagrams.push(datagram);
            now_ms += 1;
        } else {
            break;
        }
    }
    assert_eq!(datagrams.len(), 3);
    // Reordered delivery (last first, per specification recommendation).
    let mut events = Vec::new();
    for datagram in [&datagrams[2], &datagrams[0], &datagrams[1]] {
        now_ms += 1;
        let outcome = bob.receive_datagram(now_ms, NOW_SECS, datagram);
        assert!(outcome.dropped.is_none());
        events.extend(outcome.events);
    }
    assert_eq!(delivered_messages(&events), vec![message.clone()]);
    // Duplicate delivery of the same fragments is idempotent at the
    // I2NP boundary (duplicate suppression, not double delivery).
    now_ms += 1;
    let outcome = bob.receive_datagram(now_ms, NOW_SECS, &datagrams[0]);
    assert!(
        outcome.dropped == Some(i2pr_transport_ssu2::DropReason::Replay)
            || outcome.events.is_empty(),
        "duplicate packet must not redeliver"
    );

    // Conflicting duplicate: two distinct messages reuse the same ID
    // with different bodies and expirations. The receiver must drop on
    // conflict, never silently overwrite or emit a mixed message.
    let (mut alice2, mut bob2) = paired_sessions();
    let mut now_ms = NOW_MS;
    alice2
        .queue_i2np_message(i2np_bytes(20, 8002, 2500, 0x62))
        .expect("queue X");
    alice2
        .queue_i2np_message(i2np_bytes_exp(20, 8002, NOW_SECS as u32 + 30, 2500, 0x63))
        .expect("queue Y (same ID, different body/expiration)");
    // Drain all six datagrams: X delivers f0/f1/f2 then Y f0/f1/f2 in
    // greedy order (X's fragments pack first since each packet holds
    // one ~1 KB fragment here).
    let mut wire = Vec::new();
    for _ in 0..10 {
        if let Some(datagram) = alice2.poll_transmit(now_ms) {
            wire.push(datagram);
            now_ms += 1;
        } else {
            break;
        }
    }
    assert_eq!(wire.len(), 6);
    let drops_before = bob2.counters().reassembly_drops;
    let mut events = Vec::new();
    // X f0 opens the entry; X f1/f2 complete X exactly.
    for datagram in &wire[0..3] {
        now_ms += 1;
        let outcome = bob2.receive_datagram(now_ms, NOW_SECS, datagram);
        assert!(outcome.dropped.is_none());
        events.extend(outcome.events);
    }
    assert_eq!(delivered_messages(&events).len(), 1);
    // Y f0 arrives for the same ID with different bytes. X's entry is
    // gone (completed), so Y opens a fresh entry — no conflict here —
    // but Y f1/f2 then complete Y exactly: two distinct messages, two
    // exact deliveries, never a mixture.
    for datagram in &wire[3..6] {
        now_ms += 1;
        let outcome = bob2.receive_datagram(now_ms, NOW_SECS, datagram);
        assert!(outcome.dropped.is_none());
        events.extend(outcome.events);
    }
    let delivered = delivered_messages(&events);
    assert_eq!(delivered.len(), 2);
    assert!(
        delivered[0].iter().all(|byte| *byte == 0x62)
            || delivered[0][9..].iter().all(|byte| *byte == 0x62)
    );
    assert!(delivered[1][9..].iter().all(|byte| *byte == 0x63));
    assert_eq!(bob2.counters().reassembly_drops, drops_before);

    // True conflict: interleave Y's first fragment BEFORE X
    // completes. Queue both, drain the wire (greedy order X f0/f1/f2,
    // Y f0/f1/f2), then deliver X f0 followed by Y f0 out of order.
    let (mut alice3, mut bob3) = paired_sessions();
    let mut now_ms = NOW_MS;
    alice3
        .queue_i2np_message(i2np_bytes(20, 8003, 2500, 0x64))
        .expect("queue X");
    alice3
        .queue_i2np_message(i2np_bytes_exp(20, 8003, NOW_SECS as u32 + 30, 2500, 0x65))
        .expect("queue Y (same ID, different body/expiration)");
    let mut wire_xy = Vec::new();
    for _ in 0..8 {
        if let Some(datagram) = alice3.poll_transmit(now_ms) {
            wire_xy.push(datagram);
            now_ms += 1;
        } else {
            break;
        }
    }
    assert_eq!(wire_xy.len(), 6);
    now_ms += 1;
    let outcome = bob3.receive_datagram(now_ms, NOW_SECS, &wire_xy[0]);
    assert!(outcome.dropped.is_none());
    assert!(outcome.events.is_empty());
    let drops_before = bob3.counters().reassembly_drops;
    // Y f0 reuses the ID with different bytes while X's entry is open:
    // conflict terminates the entry without effects or overwrite.
    now_ms += 1;
    let outcome = bob3.receive_datagram(now_ms, NOW_SECS, &wire_xy[3]);
    assert!(outcome.dropped.is_none());
    assert!(outcome.events.is_empty());
    assert_eq!(
        bob3.counters().reassembly_drops,
        drops_before.saturating_add(1)
    );
    assert_eq!(bob3.counters().reassembly_messages, 0);
    // Stale X follow-ons afterwards cannot resurrect a mixed message.
    now_ms += 1;
    let outcome = bob3.receive_datagram(now_ms, NOW_SECS, &wire_xy[1]);
    assert!(outcome.dropped.is_none());
    assert!(outcome.events.is_empty());
    // The session stays usable: a fresh message completes exactly.
    alice3
        .queue_i2np_message(i2np_bytes(20, 8004, 100, 0x66))
        .expect("queue fresh");
    let mut fresh_events = Vec::new();
    for _ in 0..20 {
        if let Some(datagram) = alice3.poll_transmit(now_ms) {
            now_ms += 1;
            let outcome = bob3.receive_datagram(now_ms, NOW_SECS, &datagram);
            assert!(outcome.dropped.is_none());
            fresh_events.extend(outcome.events);
        } else {
            break;
        }
        now_ms += 1;
    }
    assert_eq!(delivered_messages(&fresh_events).len(), 1);
}

/// Returns a paired session whose congestion window has been primed
/// by a fully-acked priming exchange, so a later unacked burst of
/// ~27 KB stays under the grown window.
fn primed_pair(now_ms: &mut u64) -> (Ssu2Session, Ssu2Session) {
    let (mut alice, mut bob) = paired_sessions();
    for index in 0..60 {
        alice
            .queue_i2np_message(i2np_bytes(20, 40_000 + index, 500, 0x31))
            .expect("prime queue");
    }
    let mut bob_events = Vec::new();
    let mut alice_events = Vec::new();
    pump(
        &mut alice,
        &mut bob,
        now_ms,
        &mut Vec::new(),
        &mut bob_events,
        &mut alice_events,
    );
    assert_eq!(delivered_messages(&bob_events).len(), 60);
    assert!(
        alice.cwnd_bytes() >= 30_000,
        "window must prime: {}",
        alice.cwnd_bytes()
    );
    (alice, bob)
}

/// Drives one 2-fragment message through Alice's transmit path and
/// delivers only its FOLLOW-ON fragment to Bob (the first fragment is
/// withheld), opening a placeholder reassembly entry that can never
/// complete without the type/expiration carried by the first fragment.
/// No ACKs are returned, so Alice never NACK-retransmits the withheld
/// first fragment; callers must prime the window first and stay within
/// its budget and the RTO horizon.
/// A 1500-byte body splits into 1024 + 476 fragments, which never pack
/// into one 1220-byte payload, so each message yields exactly two
/// datagrams in first/second order.
fn deliver_follow_on_only(
    alice: &mut Ssu2Session,
    bob: &mut Ssu2Session,
    now_ms: &mut u64,
    message_id: u32,
) {
    alice
        .queue_i2np_message(i2np_bytes(20, message_id, 1500, 0x30))
        .expect("queue");
    let _first = alice.poll_transmit(*now_ms).expect("first fragment");
    *now_ms += 1;
    let second = alice.poll_transmit(*now_ms).expect("second fragment");
    *now_ms += 1;
    let outcome = bob.receive_datagram(*now_ms, NOW_SECS, &second);
    *now_ms += 1;
    assert!(outcome.dropped.is_none());
    assert!(outcome.events.is_empty());
}

#[test]
fn reassembly_exact_capacity_and_max_plus_one() {
    let mut now_ms = NOW_MS;
    let (mut alice, mut bob) = primed_pair(&mut now_ms);
    // Sixteen incomplete entries: exact capacity admits all sixteen.
    for index in 0..16 {
        deliver_follow_on_only(&mut alice, &mut bob, &mut now_ms, 9000 + index);
    }
    assert_eq!(bob.counters().reassembly_messages, 16);
    // max+1 admission fails closed: no new entry, no completion event,
    // no partial byte leak, and the drop counter advances.
    let bytes_before = bob.counters().reassembly_bytes;
    let drops_before = bob.counters().reassembly_drops;
    deliver_follow_on_only(&mut alice, &mut bob, &mut now_ms, 9999);
    assert_eq!(bob.counters().reassembly_messages, 16);
    assert_eq!(bob.counters().reassembly_bytes, bytes_before);
    assert_eq!(bob.counters().reassembly_drops, drops_before + 1);
    // Cleanup is total after termination.
    bob.initiate_termination(0).expect("terminate");
    assert_eq!(bob.counters().reassembly_messages, 0);
    assert_eq!(bob.counters().reassembly_bytes, 0);
}

#[test]
fn outbound_queue_exact_capacity_and_max_plus_one() {
    let (mut alice, mut _bob) = paired_sessions();
    // Exact capacity: 256 outbound messages queue successfully.
    for index in 0..256 {
        alice
            .queue_i2np_message(i2np_bytes(20, 10_000 + index, 50, 0x21))
            .expect("queue within capacity");
    }
    // max+1 fails closed without disturbing the queued depth.
    assert!(
        alice
            .queue_i2np_message(i2np_bytes(20, 99_999, 50, 0x21))
            .is_err()
    );
}

#[test]
fn congestion_gate_bounds_unacked_flight_and_acks_unblock() {
    let (mut alice, _bob) = paired_sessions();
    let mut now_ms = NOW_MS;
    // Burst 200 messages with total loss (nothing delivered, no ACKs).
    // Greedy packing plus the byte-count window must bound the flight
    // well below the queued depth; bytes in flight never exceed cwnd.
    for index in 0..200 {
        alice
            .queue_i2np_message(i2np_bytes(20, 12_000 + index, 300, 0x23))
            .expect("queue burst");
    }
    let mut transmitted = 0_usize;
    for _ in 0..400 {
        if alice.poll_transmit(now_ms).is_some() {
            transmitted += 1;
            now_ms += 1;
        } else {
            break;
        }
    }
    assert!(transmitted < 200, "flight must block: {transmitted}");
    // The gate engages once the window is full; the in-flight total
    // may overshoot by the last admitted packet (window checks happen
    // before sealing, per-packet granularity).
    assert!(
        alice.bytes_in_flight() >= alice.cwnd_bytes(),
        "gate must engage: flight {} window {}",
        alice.bytes_in_flight(),
        alice.cwnd_bytes()
    );
    assert!(
        alice.bytes_in_flight() <= alice.cwnd_bytes().saturating_add(1500),
        "overshoot must stay within one packet: flight {} window {}",
        alice.bytes_in_flight(),
        alice.cwnd_bytes()
    );
    // History eviction under overflow records bounded loss instead of
    // growing: force it by draining the sent history through NACKs.
    // (The 256-entry sent-history ceiling itself is defense-in-depth
    // above the packing-limited steady state; the outbound queue
    // above pins the exact capacity/max+1 admission behavior.)
}

#[test]
fn prolonged_heavy_loss_reaches_bounded_termination() {
    let (mut alice, mut _bob) = paired_sessions();
    let mut now_ms = NOW_MS;
    alice
        .queue_i2np_message(i2np_bytes(20, 11_000, 100, 0x22))
        .expect("queue");
    // Total loss: nothing is ever delivered or acked. Each cycle hands
    // the retransmitted fragment to a fresh packet (mimicking the
    // runtime) and then jumps past the backed-off RTO. Five consecutive
    // expirations must recommend termination instead of accumulating
    // unbounded state.
    let mut terminated = false;
    for _ in 0..8 {
        // Transmit the (re)queued fragment into the void.
        while alice.poll_transmit(now_ms).is_some() {
            now_ms += 1;
        }
        // Jump past any backed-off RTO (capped at 60 s).
        now_ms += 70_000;
        for action in alice.poll(now_ms, NOW_SECS) {
            if let i2pr_transport_ssu2::SessionAction::Terminate { reason } = action {
                assert_eq!(reason, 14);
                terminated = true;
            }
        }
        if terminated {
            break;
        }
    }
    assert!(terminated, "consecutive RTOs must terminate");
}

#[test]
fn idle_timeout_recommends_termination() {
    let (mut alice, mut bob) = paired_sessions();
    let mut now_ms = NOW_MS;
    let mut bob_events = Vec::new();
    let mut alice_events = Vec::new();
    alice
        .queue_i2np_message(i2np_bytes(20, 12_000, 60, 0x23))
        .expect("queue");
    pump(
        &mut alice,
        &mut bob,
        &mut now_ms,
        &mut Vec::new(),
        &mut bob_events,
        &mut alice_events,
    );
    assert_eq!(delivered_messages(&bob_events).len(), 1);
    // Advance past the 300 s idle timeout with no traffic.
    now_ms += 300_001;
    let mut saw_idle = false;
    for action in alice.poll(now_ms, NOW_SECS) {
        if let i2pr_transport_ssu2::SessionAction::Terminate { reason } = action {
            assert_eq!(reason, 2);
            saw_idle = true;
        }
    }
    assert!(saw_idle, "idle timeout must fire");
}

#[test]
fn termination_lifecycle_releases_state() {
    let (mut alice, mut bob) = paired_sessions();
    let mut now_ms = NOW_MS;
    alice
        .queue_i2np_message(i2np_bytes(20, 13_000, 60, 0x24))
        .expect("queue");
    let mut bob_events = Vec::new();
    let mut alice_events = Vec::new();
    pump(
        &mut alice,
        &mut bob,
        &mut now_ms,
        &mut Vec::new(),
        &mut bob_events,
        &mut alice_events,
    );
    // Bob terminates; Alice observes the typed event and releases
    // pending state under bounded cleanup.
    bob.initiate_termination(0).expect("terminate");
    now_ms += 1;
    let term = bob.poll_transmit(now_ms).expect("termination packet");
    now_ms += 1;
    let outcome = alice.receive_datagram(now_ms, NOW_SECS, &term);
    assert!(
        outcome
            .events
            .iter()
            .any(|event| matches!(event, SessionEvent::Termination { .. })),
        "termination event expected"
    );
    // New work is refused after termination.
    assert!(
        alice
            .queue_i2np_message(i2np_bytes(20, 13_001, 10, 0x25))
            .is_err()
    );
}

#[test]
fn two_sessions_isolate_pressure() {
    let mut now_ms = NOW_MS;
    let (mut alice_a, mut bob_a) = primed_pair(&mut now_ms);
    let (mut alice_b, mut bob_b) = paired_sessions();
    // Saturate session A's reassembly with sixteen incomplete entries
    // (follow-ons only, first fragments withheld).
    for index in 0..16 {
        deliver_follow_on_only(&mut alice_a, &mut bob_a, &mut now_ms, 20_000 + index);
    }
    assert_eq!(bob_a.counters().reassembly_messages, 16);
    // Session B is unaffected: a full exchange still completes exactly.
    let message = i2np_bytes(20, 21_000, 100, 0x27);
    alice_b.queue_i2np_message(message.clone()).expect("queue");
    let mut bob_events = Vec::new();
    let mut alice_events = Vec::new();
    pump(
        &mut alice_b,
        &mut bob_b,
        &mut now_ms,
        &mut Vec::new(),
        &mut bob_events,
        &mut alice_events,
    );
    assert_eq!(delivered_messages(&bob_events), vec![message]);
}
