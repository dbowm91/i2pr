//! Plan 120 §12 deterministic local trajectory.
//!
//! Drives a real destination through:
//! 1. construct inbound + outbound `ShortBuildStateMachine`s
//! 2. reach `Established` through the real `i2pr-tunnel` short-build seam
//! 3. transfer real `EstablishedMaterial` into the destination pool
//! 4. derive Lease2 entries from the pool's public routing metadata
//! 5. construct and sign a Standard LeaseSet2
//! 6. self-validate the LeaseSet2 through `i2pr-netdb`
//! 7. advance time toward tunnel expiry
//! 8. replace an inbound tunnel
//! 9. generate a newer LeaseSet2
//! 10. shut down and verify resource cleanup

use i2pr_client::{
    DestinationConfig, DestinationIdentity, DestinationRegistry, DestinationRuntime,
    DestinationState, LeaseSetDecision, LeaseSetRotationCause, RegistryConfig,
    build_signed_lease_set2,
};
use i2pr_proto::Lease2;
use i2pr_tunnel::{
    BuildEvent, EciesX25519BuildCryptography, MessageHopProcessor, ShortBuildOutcome,
    ShortBuildStateMachine, ShortResponseCode, TunnelDirection,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use zeroize::Zeroizing;

const FIXTURE_HOP_COUNT: u8 = 2;

fn hop_private_key(seed: u64, index: u8) -> [u8; 32] {
    let mut bytes = [0_u8; 32];
    let mut cursor = (seed as usize)
        .wrapping_mul(31)
        .wrapping_add(index as usize);
    for byte in bytes.iter_mut() {
        cursor = cursor.wrapping_mul(17).wrapping_add(11) % 251;
        *byte = cursor as u8;
    }
    bytes
}

fn hop_public_key(seed: u64, index: u8) -> [u8; 32] {
    let secret = x25519_dalek::StaticSecret::from(hop_private_key(seed, index));
    x25519_dalek::PublicKey::from(&secret).to_bytes()
}

fn hop_router_hash(seed: u64, index: u8) -> i2pr_proto::Hash {
    let mut bytes = [0_u8; 32];
    for (offset, byte) in bytes.iter_mut().enumerate() {
        *byte = index.wrapping_add(offset as u8) ^ (seed as u8).wrapping_add(offset as u8);
    }
    i2pr_proto::Hash::from_bytes(bytes)
}

fn drive_to_established(
    path: i2pr_tunnel::ShortBuildPath,
    seed: u64,
) -> i2pr_tunnel::EstablishedMaterial {
    let mut machine = ShortBuildStateMachine::new(path, 60_000);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let message = machine.prepare(&mut rng).expect("prepare");
    let _action = machine.deliver_action(message).expect("deliver action");
    machine.mark_dispatched().expect("dispatch");
    let cryptography = EciesX25519BuildCryptography::new();
    let mut payload = machine.last_payload().expect("payload").to_vec();
    for index in 1..=FIXTURE_HOP_COUNT {
        let (next_payload, _result) = MessageHopProcessor::process_hop(
            &cryptography,
            &payload,
            &hop_private_key(seed, index),
            &hop_router_hash(seed, index),
            ShortResponseCode::Accepted,
            &mut rng,
        )
        .expect("hop processing");
        payload = next_payload;
    }
    let outcome = machine
        .handle_event(BuildEvent::BuildReply {
            reply: Zeroizing::new(payload),
        })
        .expect("event");
    assert!(matches!(
        outcome,
        Some(ShortBuildOutcome::Established { .. })
    ));
    machine.take_established_material(0).expect("material")
}

fn inbound_path(seed: u64) -> i2pr_tunnel::ShortBuildPath {
    use i2pr_tunnel::{BuildAttemptId, BuildOptions, HopRole, HopSpec, TunnelId};
    let mut hops = Vec::new();
    for index in 1..=FIXTURE_HOP_COUNT {
        let receive = TunnelId::new(
            0x0020_0000_u32
                .wrapping_add((seed as u32).wrapping_mul(0x100))
                .wrapping_add(u32::from(index)),
        )
        .expect("nonzero receive tunnel id");
        let role = if index == 1 {
            HopRole::InboundGateway
        } else {
            HopRole::Participant
        };
        hops.push(HopSpec::new(
            hop_router_hash(seed, index),
            hop_public_key(seed, index),
            role,
            receive,
            receive,
        ));
    }
    for index in 0..hops.len().saturating_sub(1) {
        hops[index].next_tunnel = hops[index + 1].receive_tunnel;
    }
    i2pr_tunnel::ShortBuildPath {
        attempt_id: BuildAttemptId::new(seed),
        direction: TunnelDirection::Inbound,
        originator_hash: Some(hop_router_hash(seed, 0xAB)),
        outbound_reply_router: None,
        creator_tunnel_id: TunnelId::new(0x0200_0000_u32.wrapping_add(seed as u32))
            .expect("nonzero creator tunnel id"),
        hops,
        request_time: i2pr_proto::Date::from_millis(60_000),
        next_message_id: 0x1234_5678,
        options: BuildOptions::empty(),
    }
}

fn outbound_path(seed: u64) -> i2pr_tunnel::ShortBuildPath {
    use i2pr_tunnel::{BuildAttemptId, BuildOptions, HopRole, HopSpec, TunnelId};
    let mut hops = Vec::new();
    for index in 1..=FIXTURE_HOP_COUNT {
        let receive = TunnelId::new(
            0x0040_0000_u32
                .wrapping_add((seed as u32).wrapping_mul(0x100))
                .wrapping_add(u32::from(index)),
        )
        .expect("nonzero receive tunnel id");
        let role = if index == FIXTURE_HOP_COUNT {
            HopRole::OutboundEndpoint
        } else {
            HopRole::Participant
        };
        hops.push(HopSpec::new(
            hop_router_hash(seed, index),
            hop_public_key(seed, index),
            role,
            receive,
            receive,
        ));
    }
    for index in 0..hops.len().saturating_sub(1) {
        hops[index].next_tunnel = hops[index + 1].receive_tunnel;
    }
    i2pr_tunnel::ShortBuildPath {
        attempt_id: BuildAttemptId::new(seed),
        direction: TunnelDirection::Outbound,
        originator_hash: None,
        outbound_reply_router: Some(hop_router_hash(seed, 0xCD)),
        creator_tunnel_id: TunnelId::new(0x0400_0000_u32.wrapping_add(seed as u32))
            .expect("nonzero creator tunnel id"),
        hops,
        request_time: i2pr_proto::Date::from_millis(60_000),
        next_message_id: 0x1234_5678,
        options: BuildOptions::empty(),
    }
}

fn destination(seed: u64) -> DestinationIdentity {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut signing = [0_u8; 32];
    let mut static_secret = [0_u8; 32];
    let mut padding = vec![0_u8; i2pr_crypto::IDENTITY_PADDING_LENGTH];
    rng.fill_bytes(&mut signing);
    rng.fill_bytes(&mut static_secret);
    rng.fill_bytes(&mut padding);
    DestinationIdentity::from_private_bytes(
        signing,
        static_secret,
        zeroize::Zeroizing::new(padding),
    )
    .expect("destination identity")
}

#[test]
fn plan_120_deterministic_local_trajectory() {
    let config = DestinationConfig::balanced();
    let now = 1_000_u64;
    let identity = destination(7);

    // 1. Construct inbound and outbound `ShortBuildPath`s and reach Established.
    let inbound = drive_to_established(inbound_path(701), 701);
    let outbound = drive_to_established(outbound_path(703), 703);
    assert_eq!(inbound.direction(), TunnelDirection::Inbound);
    assert_eq!(outbound.direction(), TunnelDirection::Outbound);

    // 2. Transfer real `EstablishedMaterial` into the destination runtime.
    let mut runtime = DestinationRuntime::new(identity, config).expect("runtime");
    runtime.admit_inbound(inbound, now).expect("inbound");
    runtime.admit_outbound(outbound, now).expect("outbound");
    runtime.refresh_lease_set(now).expect("refresh");
    assert_eq!(runtime.state(), DestinationState::Usable);
    let id = runtime.id();

    // 3. Derive Lease2 entries from the public routing metadata.
    let leases = runtime.inbound_lease_sources(now);
    assert_eq!(leases.len(), 1);
    let lease_source = leases[0];
    let record = build_signed_lease_set2(runtime.identity(), &leases, u32::try_from(now).unwrap())
        .expect("signed");
    let lease2: &Lease2 = &record.leases()[0];
    assert_eq!(lease2.tunnel_gateway(), lease_source.gateway());
    assert_eq!(lease2.tunnel_id(), lease_source.gateway_receive_tunnel_id());
    assert!(
        record
            .expires_seconds()
            .saturating_sub(record.published_seconds())
            > 0
    );

    // 4. The finalized record self-validates through the Plan 119 path.
    i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(
        record.clone(),
        Some(id.as_netdb_key()),
        i2pr_netdb::LeaseSet2ValidationContext::new(u32::try_from(now).unwrap()),
    )
    .expect("self-validates");
    i2pr_crypto::verify_lease_set2(&record).expect("signature verifies");

    // 5. Advance time toward tunnel expiry; the first record is replaced.
    let lifetime = u64::from(config.tunnel_lifetime_seconds());
    let rotation = u64::from(config.lease_rotation_margin_seconds());
    let publication = u64::from(config.lease_publication_margin_seconds());
    // The earliest advertised lease end is `now + lifetime - publication`. The
    // rotation threshold is `earliest - rotation`. Pick a time that is well
    // before the rotation threshold so the first call returns `Retain`.
    let before_rotation = now + 50;
    runtime
        .advance_time(before_rotation)
        .expect("advance before rotation");
    assert_eq!(
        runtime.refresh_lease_set(before_rotation).expect("refresh"),
        LeaseSetDecision::Retain
    );

    // After the original lease is past the rotation margin, refresh returns
    // `Regenerate(ApproachingExpiry)`. The replacement tunnel must replace the
    // now-expired one.
    let now_approaching = now + lifetime - publication - rotation + 5;
    let decision = runtime
        .refresh_lease_set(now_approaching)
        .expect("approaching refresh");
    assert_eq!(
        decision,
        LeaseSetDecision::Regenerate(LeaseSetRotationCause::ApproachingExpiry),
        "approaching expiry rotates the lease set"
    );

    // Advance past the original tunnel expiry so the slot is evicted and the
    // inbound tunnel pool becomes empty.
    let now_expiry = now + lifetime + 5;
    let replacement = drive_to_established(inbound_path(705), 705);
    let replacement_outbound = drive_to_established(outbound_path(707), 707);
    let evicted = runtime.advance_time(now_expiry).expect("evict");
    assert_eq!(
        evicted.evicted_slots.len(),
        2,
        "both original tunnels expired"
    );
    // Admit the replacement and refresh; the lease set is rebuilt with the
    // new lease.
    runtime
        .admit_inbound(replacement, now_expiry)
        .expect("replacement");
    runtime
        .admit_outbound(replacement_outbound, now_expiry)
        .expect("replacement outbound");
    let _ = runtime
        .refresh_lease_set(now_expiry)
        .expect("replacement refresh");
    let after = runtime
        .inbound_lease_sources(now_expiry)
        .into_iter()
        .find(|source| {
            source.advertised_expires_seconds()
                == now_expiry + lifetime - u64::from(config.lease_publication_margin_seconds())
        })
        .expect("replacement lease is advertised");

    // 6. The replacement lease set is publication-ready.
    let refreshed = runtime.lease_set().expect("lease set");
    let _ = refreshed;
    let refreshed_record = build_signed_lease_set2(
        runtime.identity(),
        &runtime.inbound_lease_sources(now_expiry),
        refreshed.published_seconds(),
    )
    .expect("refreshed record");
    i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(
        refreshed_record,
        Some(id.as_netdb_key()),
        i2pr_netdb::LeaseSet2ValidationContext::new(u32::try_from(now_expiry).unwrap()),
    )
    .expect("refreshed self-validates");
    assert!(
        after
            .tunnel_expires_seconds()
            .saturating_sub(after.advertised_expires_seconds())
            >= u64::from(config.lease_publication_margin_seconds())
    );

    // 7. Shutdown and verify resource cleanup.
    let mut registry = DestinationRegistry::new(RegistryConfig::default());
    let id = registry.insert(runtime).expect("insert");
    let removed = registry.remove(&id).expect("removed");
    assert_eq!(removed.released_pool_slots, 2);
    assert!(registry.is_empty());
    assert!(registry.get(&id).is_none());
}
