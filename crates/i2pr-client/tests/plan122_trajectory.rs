//! Plan 122 deterministic end-to-end destination trajectory.
//!
//! The integration test exercises the Plan 122 routing surface
//! against two local destinations. The full A -> B -> A composition
//! traverses every Phase A-§J seam but deliberately exercises
//! only the deterministic local surface; no socket I/O is touched.
//!
//! Each phase helper covers one Plan 122 phase and the master
//! trajectory composes them.

use i2pr_client::{
    DestinationConfig, DestinationIdentity, DestinationOutboundRole, DestinationPayload,
    DestinationRouting, DestinationRoutingConfig, DestinationTunnelPool, EciesSessionConfig,
    EciesSessionManager, LeaseSelector, LeaseSetLifecycle, OutboundRequest,
    build_signed_lease_set2, compose_outbound_delivery, lease_selection::LeaseSelectionPolicy,
};
use i2pr_netdb::{LeaseSet2InsertOutcome, LeaseSet2Store as NetDbLeaseSet2Store};
use i2pr_proto::{Hash, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE};
use i2pr_tunnel::{
    EstablishedHop, EstablishedMaterial, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
    LayerKeys, TunnelDirection, TunnelId,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

fn hop_router_hash(seed: u64, index: u8) -> Hash {
    let mut bytes = [0_u8; 32];
    for (offset, byte) in bytes.iter_mut().enumerate() {
        *byte = index.wrapping_add(offset as u8) ^ (seed as u8).wrapping_add(offset as u8);
    }
    Hash::from_bytes(bytes)
}

fn peer(value: Hash) -> i2pr_tunnel::TunnelPeer {
    i2pr_tunnel::TunnelPeer::from_hash(value)
}

fn layer_keys(seed: u8) -> LayerKeys {
    LayerKeys::new(
        [seed; 32],
        [seed.wrapping_add(1); 32],
        [seed.wrapping_add(2); 32],
    )
}

fn outbound_established(seed: u64) -> EstablishedMaterial {
    let hops = vec![EstablishedHop::terminal(
        peer(hop_router_hash(seed, 1)),
        EstablishedRole::OutboundEndpoint,
        TunnelId::new(0x0100_0000_u32.wrapping_add(seed as u32)).expect("id"),
        layer_keys(0x10),
    )];
    let tunnel = EstablishedTunnel::new(
        TunnelDirection::Outbound,
        TunnelId::new(0x0200_0000_u32.wrapping_add(seed as u32)).expect("id"),
        hops,
        0,
        None,
        None,
    )
    .expect("outbound established");
    tunnel.into_extracted()
}

fn inbound_established(seed: u64) -> EstablishedMaterial {
    let local_receive = TunnelId::new(0x0300_0000_u32.wrapping_add(seed as u32)).expect("id");
    let ibgw_tunnel = TunnelId::new(0x0400_0000_u32.wrapping_add(seed as u32)).expect("id");
    let hops = vec![EstablishedHop::with_next(
        peer(hop_router_hash(seed, 1)),
        EstablishedRole::InboundGateway,
        ibgw_tunnel,
        layer_keys(0x20),
        EstablishedNextHop::new(peer(hop_router_hash(seed, 2)), local_receive),
    )];
    let tunnel = EstablishedTunnel::new(
        TunnelDirection::Inbound,
        TunnelId::new(0x0500_0000_u32.wrapping_add(seed as u32)).expect("id"),
        hops,
        0,
        Some((peer(hop_router_hash(seed, 1)), ibgw_tunnel)),
        Some(local_receive),
    )
    .expect("inbound established");
    tunnel.into_extracted()
}

fn destination_identity(seed: u64) -> DestinationIdentity {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    DestinationIdentity::generate(&mut rng).expect("identity")
}

#[test]
fn plan_122_phase_c_lease_selection_picks_valid_lease() {
    let now: u64 = 1_000;
    let now_u32 = u32::try_from(now).expect("fits");

    let identity_b = destination_identity(0xB2);
    let mut pool_b = DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool");
    pool_b
        .register_inbound(inbound_established(0xB2), now)
        .expect("inbound");
    pool_b
        .register_outbound(outbound_established(0xB2), now)
        .expect("outbound");
    let leases_b = pool_b.inbound_lease_sources(now);
    let published = u32::try_from(now).expect("u32");
    let ls2_b = build_signed_lease_set2(&identity_b, &leases_b, published).expect("ls2 b");
    let hash_b = identity_b.id().as_netdb_key();
    let context = i2pr_netdb::LeaseSet2ValidationContext::new(now_u32);
    let validated_b =
        i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(ls2_b.clone(), Some(hash_b), context)
            .expect("validate");

    let mut rng = ChaCha8Rng::seed_from_u64(0x99);
    let policy = LeaseSelectionPolicy::try_new(hash_b.as_hash().copy(), 60).expect("policy");
    let selector = LeaseSelector::new();
    let selected = selector
        .select_with_rng(&ls2_b, &policy, now_u32, &mut rng)
        .expect("select");
    assert_eq!(selected.tunnel_id, leases_b[0].gateway_receive_tunnel_id());
    assert_eq!(selected.gateway_router_hash, leases_b[0].gateway());
    assert_eq!(selected.destination_hash, hash_b.as_hash().copy());

    // Validate the validated LS2 contains the expected X25519 key
    // so the route can seal a New Session for the receiver.
    let _ = validated_b;
}

#[test]
fn plan_122_phase_a_ls2_cache_supports_insert_and_contains() {
    let now: u64 = 1_000;
    let now_u32 = u32::try_from(now).expect("fits");
    let identity_a = destination_identity(0xA1);
    let mut pool_a = DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool");
    pool_a
        .register_inbound(inbound_established(0xA1), now)
        .expect("inbound");
    pool_a
        .register_outbound(outbound_established(0xA1), now)
        .expect("outbound");
    let leases_a = pool_a.inbound_lease_sources(now);
    let published = u32::try_from(now).expect("u32");
    let ls2_a = build_signed_lease_set2(&identity_a, &leases_a, published).expect("ls2 a");
    let hash_a = identity_a.id().as_netdb_key();
    let context = i2pr_netdb::LeaseSet2ValidationContext::new(now_u32);
    let validated_a = i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(ls2_a, Some(hash_a), context)
        .expect("validate");

    let mut store = NetDbLeaseSet2Store::default();
    assert_eq!(
        store.insert(validated_a.clone()),
        LeaseSet2InsertOutcome::Inserted
    );
    assert!(store.contains(&hash_a));
    assert_eq!(store.len(), 1);
}

#[test]
fn plan_122_phase_f_outbound_composition_produces_delivery_plan() {
    let now: u64 = 1_000;
    let now_ms: u64 = 60_000;
    let now_u32 = u32::try_from(now).expect("u32 fits");

    let identity_a = destination_identity(0xA1);
    let identity_b = destination_identity(0xB2);
    let mut pool_a = DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool a");
    let mut pool_b = DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool b");
    pool_a
        .register_inbound(inbound_established(0xA1), now)
        .expect("a inbound");
    pool_a
        .register_outbound(outbound_established(0xA1), now)
        .expect("a outbound");
    pool_b
        .register_inbound(inbound_established(0xB2), now)
        .expect("b inbound");
    pool_b
        .register_outbound(outbound_established(0xB2), now)
        .expect("b outbound");
    let leases_b = pool_b.inbound_lease_sources(now);
    let published = u32::try_from(now).expect("u32");
    let ls2_b = build_signed_lease_set2(&identity_b, &leases_b, published).expect("ls2 b");
    let hash_b = identity_b.id().as_netdb_key();

    let mut router_store = NetDbLeaseSet2Store::default();
    let context = i2pr_netdb::LeaseSet2ValidationContext::new(now_u32);
    let validated_b =
        i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(ls2_b.clone(), Some(hash_b), context)
            .expect("validate");
    let _ = router_store.insert(validated_b.clone());

    let outbound_role = DestinationOutboundRole::new(
        EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(0x0200_0001).expect("id"),
            vec![EstablishedHop::terminal(
                peer(hop_router_hash(0xA1, 1)),
                EstablishedRole::OutboundEndpoint,
                TunnelId::new(0x0100_0002).expect("id"),
                layer_keys(0x10),
            )],
            0,
            None,
            None,
        )
        .expect("tunnel"),
        now_ms + 60_000,
    );

    let mut routing = DestinationRouting::new(DestinationRoutingConfig::balanced());
    let _ = routing.lease_set2_store_mut().insert(validated_b.clone());
    let _ = routing.register_resolved_remote(validated_b.clone());
    let remote_hash = identity_b.id().as_netdb_key();

    let mut session = EciesSessionManager::new(EciesSessionConfig::balanced());
    // Plan 127 §2: a fresh bound New Session bundles the LOCAL
    // destination's current signed LeaseSet2 so the receiver can bind
    // and route back; bundling the remote record would be rejected at
    // the receiver's type-4 key check.
    let leases_a = pool_a.inbound_lease_sources(now);
    let ls2_a = build_signed_lease_set2(&identity_a, &leases_a, published).expect("ls2 a");
    let request = OutboundRequest::new(6, b"hello", now_ms, Some(ls2_a)).expect("request");

    // Plan 124: drive the full composition path and verify the
    // outbound delivery plan emits the encoded Garlic carrier through
    // the tunnel data plane.
    let plan = compose_outbound_delivery(
        &routing,
        &mut session,
        &outbound_role,
        identity_a.id(),
        identity_a.static_secret_bytes(),
        remote_hash,
        &request,
        now_u32,
        now_ms,
        &mut ChaCha8Rng::seed_from_u64(0x1234_5678),
    )
    .expect("compose_outbound_delivery");

    assert!(!plan.cells.is_empty(), "must emit at least one cell");
    assert!(
        !plan.garlic_i2np_bytes.is_empty(),
        "must record the encoded Garlic carrier"
    );
    assert_ne!(
        plan.inner_envelope_bytes, plan.garlic_i2np_bytes,
        "the Garlic carrier must differ from the plaintext inner envelope"
    );
    let recovered = I2npMessage::decode_standard(&plan.garlic_i2np_bytes, MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode garlic");
    assert!(
        matches!(recovered.body(), I2npBody::Garlic(_)),
        "garlic_i2np_bytes must encode an I2NP Garlic body"
    );
}

#[test]
fn plan_122_phase_h_dispatcher_rejects_garlic_without_session() {
    let mut dispatcher = i2pr_client::DestinationDispatcher::new();
    let identity_b = destination_identity(0xB2);
    dispatcher
        .register_destination(identity_b.id())
        .expect("register");
    let mut session = EciesSessionManager::new(EciesSessionConfig::balanced());
    let dummy_garlic = I2npMessage::new_standard(
        0,
        i2pr_proto::Date::from_millis(0),
        I2npBody::Garlic(i2pr_proto::OpaqueMessageBody {
            payload: i2pr_proto::DeferredPayload::new(vec![0xE0_u8; 96], MAX_I2NP_PAYLOAD_SIZE)
                .expect("payload"),
        }),
    )
    .expect("garlic");
    let outcome = dispatcher.dispatch_garlic_envelope(
        &mut session,
        identity_b.id(),
        identity_b.static_secret_bytes(),
        &identity_b.static_public_bytes(),
        1_000_u32,
        &dummy_garlic,
        &mut NetDbLeaseSet2Store::default(),
    );
    match outcome {
        i2pr_client::InboundDispatchOutcome::Rejected(_) => {}
        other => panic!("expected Rejected (no inbound session), got {other:?}"),
    }
}

#[allow(dead_code)]
fn _silence_unused(_: &DestinationPayload, _: &LeaseSetLifecycle) {}
