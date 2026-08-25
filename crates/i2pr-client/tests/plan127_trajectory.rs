//! Plan 127 Milestone 6 destination-session routing final closure.
//!
//! The master trajectory composes the Plan 126 corrected ECIES
//! ratchet with Standard LeaseSet2 binding, destination-owned tunnel
//! pools, the Plan 124 Garlic-through-tunnel path, destination
//! ownership, reverse routing, and application delivery:
//!
//! ```text
//! A bound NS (bundling A LS2)
//!  -> A outbound destination tunnel -> A OBEP
//!  -> authenticated-router-link-bypassed-local-seam
//!  -> B inbound destination tunnel -> B destination owner
//!  -> B ECIES open -> validate/bind A LeaseSet2
//!  -> exact A application payload
//! B NSR (retained Plan 126 reply context)
//!  -> B outbound tunnel -> seam -> A inbound tunnel
//!  -> A pending NSR context -> exact B application payload
//! A/B Existing Session traffic in both directions
//! ```
//!
//! No Streaming packet logic participates: every application payload
//! is an opaque I2NP `Data` body so destination/session correctness
//! stays isolated. Every trajectory uses real destination-owned
//! outbound/inbound tunnel roles and only the explicit post-OBEP
//! local seam; no external interoperability is claimed.

#![allow(clippy::too_many_lines)]

use i2pr_client::{
    ClassifiedInbound, DestinationConfig, DestinationDispatcher, DestinationIdentity,
    DestinationOutboundRole, DestinationRouting, DestinationRoutingConfig, DestinationTunnelPool,
    EciesOutboundMessage, EciesSessionConfig, EciesSessionManager, EncryptedOutbound,
    InboundDispatchError, InboundDispatchOutcome, MAX_ACTIVE_REMOTES, MAX_INBOUND_PENDING_MESSAGES,
    OutboundDeliveryPlan, OutboundRequest, PlannedOutboundForm, SendError, build_signed_lease_set2,
    compose_outbound_delivery, encode_new_session_payload,
};
use i2pr_crypto::{ExistingSessionMessage, X25519_KEY_LENGTH};
use i2pr_netdb::{DestinationHash, ValidatedLeaseSet2};
use i2pr_proto::{
    Date, DeferredPayload, GarlicCloveBlock, GarlicDelivery, Hash, I2npBody, I2npMessage,
    MAX_I2NP_PAYLOAD_SIZE, OpaqueMessageBody, TunnelGatewayMessage,
};
use i2pr_tunnel::{
    DuplicateWindow, EstablishedHop, EstablishedNextHop, EstablishedRole, EstablishedTunnel,
    InboundGatewayRole, InboundParticipantRole, LayerKeys, LocalInboundEndpointRole,
    OutboundEndpointRole, OutboundParticipantRole, RouterDeliveryAction, RouterDeliveryKind,
    TunnelDirection, TunnelId, TunnelPeer,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const A_SEED: u64 = 0x127A;
const B_SEED: u64 = 0x127B;
const C_SEED: u64 = 0x127C;
const NOW_SECONDS: u32 = 5_000;
const NOW_MS: u64 = 300_000;

// ---- Deterministic fixture helpers ----

fn peer(value: Hash) -> TunnelPeer {
    TunnelPeer::from_hash(value)
}

fn hop_router_hash(seed: u64, index: u8) -> Hash {
    let mut bytes = [0_u8; 32];
    for (offset, byte) in bytes.iter_mut().enumerate() {
        *byte = index.wrapping_add(offset as u8) ^ (seed as u8).wrapping_add(offset as u8);
    }
    Hash::from_bytes(bytes)
}

fn layer_keys(seed: u8) -> LayerKeys {
    LayerKeys::new(
        [seed; 32],
        [seed.wrapping_add(1); 32],
        [seed.wrapping_add(2); 32],
    )
}

fn destination_identity(seed: u64) -> DestinationIdentity {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    DestinationIdentity::generate(&mut rng).expect("destination identity")
}

/// Two-hop outbound chain: participant hop followed by the OBEP.
fn outbound_tunnel_direct(seed: u64) -> EstablishedTunnel {
    let hops = vec![
        EstablishedHop::with_next(
            peer(hop_router_hash(seed, 1)),
            EstablishedRole::Participant,
            TunnelId::new(0x0100_0000_u32.wrapping_add(seed as u32)).expect("id"),
            layer_keys(0x50),
            EstablishedNextHop::new(
                peer(hop_router_hash(seed, 2)),
                TunnelId::new(0x0100_0001_u32.wrapping_add(seed as u32)).expect("id"),
            ),
        ),
        EstablishedHop::terminal(
            peer(hop_router_hash(seed, 2)),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(0x0100_0001_u32.wrapping_add(seed as u32)).expect("id"),
            layer_keys(0x51),
        ),
    ];
    EstablishedTunnel::new(
        TunnelDirection::Outbound,
        TunnelId::new(0x0200_0000_u32.wrapping_add(seed as u32)).expect("id"),
        hops,
        0,
        None,
        None,
    )
    .expect("outbound established")
}

/// Two-hop inbound chain ending at the creator-local endpoint.
fn inbound_tunnel_direct(seed: u64) -> EstablishedTunnel {
    let local_receive = TunnelId::new(0x0300_0000_u32.wrapping_add(seed as u32)).expect("id");
    let ibgw_tunnel = TunnelId::new(0x0400_0000_u32.wrapping_add(seed as u32)).expect("id");
    let hops = vec![
        EstablishedHop::with_next(
            peer(hop_router_hash(seed, 1)),
            EstablishedRole::InboundGateway,
            ibgw_tunnel,
            layer_keys(0x60),
            EstablishedNextHop::new(
                peer(hop_router_hash(seed, 2)),
                TunnelId::new(0x0400_0001_u32.wrapping_add(seed as u32)).expect("id"),
            ),
        ),
        EstablishedHop::with_next(
            peer(hop_router_hash(seed, 2)),
            EstablishedRole::Participant,
            TunnelId::new(0x0400_0001_u32.wrapping_add(seed as u32)).expect("id"),
            layer_keys(0x61),
            EstablishedNextHop::new(peer(hop_router_hash(seed, 3)), local_receive),
        ),
    ];
    EstablishedTunnel::new(
        TunnelDirection::Inbound,
        TunnelId::new(0x0500_0000_u32.wrapping_add(seed as u32)).expect("id"),
        hops,
        0,
        Some((peer(hop_router_hash(seed, 1)), ibgw_tunnel)),
        Some(local_receive),
    )
    .expect("inbound established")
}

/// The receiver-side inbound chain roles.
struct InboundChain {
    ibgw: InboundGatewayRole,
    participant: InboundParticipantRole,
    endpoint: LocalInboundEndpointRole,
}

impl InboundChain {
    fn new(seed: u64) -> Self {
        let inbound_tunnel = inbound_tunnel_direct(seed);
        let ibgw_hop = inbound_tunnel.hops()[0].clone();
        let participant_hop = inbound_tunnel.hops()[1].clone();
        let ibgw = InboundGatewayRole::new(&ibgw_hop, DuplicateWindow::new(16), NOW_MS + 60_000)
            .expect("ibgw role");
        let participant = InboundParticipantRole::new(
            &participant_hop,
            DuplicateWindow::new(16),
            NOW_MS + 60_000,
        )
        .expect("inbound participant role");
        let endpoint = LocalInboundEndpointRole::new(
            inbound_tunnel_direct(seed),
            16,
            1 << 20,
            60_000,
            0,
            NOW_MS + 60_000,
        );
        Self {
            ibgw,
            participant,
            endpoint,
        }
    }
}

/// One local destination side owning every Plan 127 surface:
/// identity, current signed Standard LeaseSet2, tunnel pools with one
/// real established tunnel per direction, routing, ECIES session
/// manager, dispatcher, and the real tunnel roles.
struct Side {
    seed: u64,
    identity: DestinationIdentity,
    /// Current signed Standard LeaseSet2 (bundled on fresh bound New
    /// Sessions).
    lease_set2: i2pr_proto::LeaseSet2,
    routing: DestinationRouting,
    dispatcher: DestinationDispatcher,
    session: EciesSessionManager,
    outbound: DestinationOutboundRole,
    inbound: InboundChain,
}

impl Side {
    fn new(seed: u64) -> Self {
        let identity = destination_identity(seed);
        // Signed Standard LeaseSet2 over the real inbound tunnel set.
        let mut pool = DestinationTunnelPool::new(DestinationConfig::balanced()).expect("pool");
        pool.register_inbound(
            inbound_tunnel_direct(seed).into_extracted(),
            u64::from(NOW_SECONDS),
        )
        .expect("inbound registered");
        pool.register_outbound(
            outbound_tunnel_direct(seed).into_extracted(),
            u64::from(NOW_SECONDS),
        )
        .expect("outbound registered");
        let lease_sources = pool.inbound_lease_sources(u64::from(NOW_SECONDS));
        let lease_set2 =
            build_signed_lease_set2(&identity, &lease_sources, NOW_SECONDS).expect("signed ls2");

        let mut dispatcher = DestinationDispatcher::new();
        dispatcher
            .register_destination(identity.id())
            .expect("register destination");
        dispatcher
            .bind_destination_hash(identity.id(), identity.id().as_netdb_key())
            .expect("bind destination hash");

        Self {
            seed,
            identity,
            lease_set2,
            routing: DestinationRouting::new(DestinationRoutingConfig::balanced()),
            dispatcher,
            session: EciesSessionManager::new(EciesSessionConfig::balanced()),
            outbound: DestinationOutboundRole::new(outbound_tunnel_direct(seed), NOW_MS + 60_000),
            inbound: InboundChain::new(seed),
        }
    }

    fn hash(&self) -> DestinationHash {
        self.identity.id().as_netdb_key()
    }

    fn static_public(&self) -> [u8; X25519_KEY_LENGTH] {
        self.identity.static_public_bytes()
    }

    /// Composes one outbound delivery plan toward `remote_hash`,
    /// bundling this side's current signed LeaseSet2.
    fn compose(
        &mut self,
        remote_hash: DestinationHash,
        payload: &[u8],
        rng_seed: u64,
    ) -> Result<OutboundDeliveryPlan, SendError> {
        self.compose_with_bundled(
            remote_hash,
            payload,
            Some(self.lease_set2.clone()),
            rng_seed,
        )
    }

    /// Composes one outbound delivery plan with an explicit (possibly
    /// wrong or absent) bundled LeaseSet2 for negative tests.
    fn compose_with_bundled(
        &mut self,
        remote_hash: DestinationHash,
        payload: &[u8],
        bundled: Option<i2pr_proto::LeaseSet2>,
        rng_seed: u64,
    ) -> Result<OutboundDeliveryPlan, SendError> {
        let request = OutboundRequest::new(6, payload, NOW_MS, bundled).expect("outbound request");
        let Side {
            routing,
            session,
            outbound,
            identity,
            ..
        } = self;
        let mut rng = ChaCha8Rng::seed_from_u64(rng_seed);
        compose_outbound_delivery(
            routing,
            session,
            outbound,
            identity.id(),
            identity.static_secret_bytes(),
            remote_hash,
            &request,
            NOW_SECONDS,
            NOW_MS,
            &mut rng,
        )
    }

    /// Dispatches one recovered Garlic envelope through this side's
    /// owner/session/routing surfaces.
    fn dispatch(&mut self, envelope: &I2npMessage) -> InboundDispatchOutcome {
        let Side {
            dispatcher,
            session,
            routing,
            identity,
            ..
        } = self;
        dispatcher.dispatch_garlic_envelope(
            session,
            identity.id(),
            identity.static_secret_bytes(),
            &identity.static_public_bytes(),
            NOW_SECONDS,
            envelope,
            routing.lease_set2_store_mut(),
        )
    }

    /// Advances the side's deterministic clock surfaces (session
    /// lifecycle sweep).
    fn advance_session_time(&mut self, now_seconds: u32) {
        self.session.advance_time(now_seconds);
    }
}

fn garlic_envelope(bytes: Vec<u8>) -> I2npMessage {
    I2npMessage::new_standard(
        1,
        Date::from_millis(NOW_MS),
        I2npBody::Garlic(OpaqueMessageBody {
            payload: DeferredPayload::new(bytes, MAX_I2NP_PAYLOAD_SIZE).expect("payload"),
        }),
    )
    .expect("garlic envelope")
}

/// Runs the sender's real outbound tunnel roles (participant + OBEP),
/// asserts the OBEP delivery target equals the selected Lease2, then
/// crosses the `authenticated-router-link-bypassed-local-seam` (exact
/// bytes unchanged) into the receiver's real inbound chain (IBGW ->
/// participant -> local endpoint) and returns the raw recovered I2NP
/// carrier bytes. The seam passes the exact OBEP action unchanged
/// here; other tests may drop/reorder/duplicate at the seam, but it
/// never decrypts, re-encrypts, or rewrites the target gateway or
/// tunnel id.
fn seam_deliver(sender: &Side, receiver: &mut Side, plan: &OutboundDeliveryPlan) -> Vec<u8> {
    let outbound_hops = sender.outbound.role().established().hops();
    let mut out_participant =
        OutboundParticipantRole::new(&outbound_hops[0], DuplicateWindow::new(16), NOW_MS + 60_000)
            .expect("outbound participant role");
    let mut obep = OutboundEndpointRole::new(
        &outbound_hops[1],
        DuplicateWindow::new(16),
        16,
        1 << 20,
        60_000,
        NOW_MS + 60_000,
        0,
    );
    let mut actions: Vec<RouterDeliveryAction> = Vec::new();
    for cell in &plan.cells {
        let forwarded = out_participant
            .process(&hop_router_hash(sender.seed, 0), &cell.cell, 0)
            .expect("outbound participant forward");
        let delivered = obep
            .process(&outbound_hops[0].peer().hash(), &forwarded, 0)
            .expect("obep process");
        if let Some(action) = delivered {
            actions.push(action);
        }
    }
    assert_eq!(
        actions.len(),
        1,
        "OBEP must emit exactly one delivery action"
    );
    let action = &actions[0];
    // The seam target must be the selected Lease2 gateway/tunnel id.
    assert_eq!(
        action.target_router, plan.selected_lease.gateway_router_hash,
        "OBEP target router must equal the selected Lease2 gateway"
    );
    assert_eq!(
        action.tunnel_id.expect("tunnel id").get(),
        plan.selected_lease.tunnel_id,
        "OBEP target tunnel id must equal the selected Lease2 tunnel id"
    );
    assert!(
        matches!(action.kind, RouterDeliveryKind::TunnelGateway),
        "OBEP delivery kind must be TunnelGateway"
    );
    // authenticated-router-link-bypassed-local-seam: exact bytes.
    assert_eq!(action.message, plan.garlic_i2np_bytes);
    // Receiver inbound chain.
    let inner_i2np = I2npMessage::decode_standard(&action.message, MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode obep inner i2np");
    let gateway_msg = TunnelGatewayMessage {
        tunnel_id: action.tunnel_id.expect("tunnel id").get(),
        message: Box::new(inner_i2np),
    };
    // Deterministic routers rebuild replay windows on a bounded
    // schedule; the harness refreshes them per delivery.
    receiver.inbound = InboundChain::new(receiver.seed);
    let mut rng = ChaCha8Rng::seed_from_u64(0x51EA);
    let ibgw_out = receiver
        .inbound
        .ibgw
        .process(&gateway_msg, &mut rng, 0)
        .expect("ibgw process");
    let participant_cell = receiver
        .inbound
        .participant
        .process(&hop_router_hash(receiver.seed, 1), &ibgw_out.cell, 0)
        .expect("inbound participant forward");
    receiver
        .inbound
        .endpoint
        .process(&hop_router_hash(receiver.seed, 2), &participant_cell, 0)
        .expect("local endpoint process")
        .expect("endpoint recovered the Garlic carrier")
}

/// Decodes the recovered Garlic-carrier bytes into the envelope the
/// destination dispatcher consumes.
fn recovered_envelope(recovered: Vec<u8>) -> I2npMessage {
    let message =
        I2npMessage::decode_standard(&recovered, MAX_I2NP_PAYLOAD_SIZE).expect("decode carrier");
    assert!(
        matches!(message.body(), I2npBody::Garlic(_)),
        "the recovered carrier must be an I2NP Garlic body"
    );
    message
}

/// Registers `remote`'s validated LeaseSet2 in `side`'s routing state
/// (the pre-resolved path used ahead of first contact).
fn preresolve_remote(side: &mut Side, remote: &Side) -> DestinationHash {
    let validated = ValidatedLeaseSet2::from_lease_set2(
        remote.lease_set2.clone(),
        Some(remote.hash()),
        i2pr_netdb::LeaseSet2ValidationContext::new(NOW_SECONDS),
    )
    .expect("validated remote ls2");
    side.routing
        .install_remote_lease_set2(validated)
        .expect("install resolved remote ls2")
}

/// Drives one complete A->B then B->A session bootstrap and returns
/// nothing; used by tests that need an established paired session as
/// a starting point. Asserts every intermediate form on the way.
fn establish_pair(side_a: &mut Side, side_b: &mut Side) {
    establish_pair_seeded(side_a, side_b, 0x127B_0000);
}

fn establish_pair_seeded(side_a: &mut Side, side_b: &mut Side, seed_base: u64) {
    preresolve_remote(side_a, side_b);
    let hash_b = side_b.hash();

    // A -> B bound New Session.
    let plan_ns = side_a
        .compose(hash_b, b"bootstrap-a", seed_base + 1)
        .expect("compose ns");
    assert!(matches!(
        plan_ns.encrypted_message,
        EncryptedOutbound::NewSession { .. }
    ));
    let recovered_ns = seam_deliver(side_a, side_b, &plan_ns);
    let envelope_b = recovered_envelope(recovered_ns);
    let validated_a = match side_b.dispatch(&envelope_b) {
        InboundDispatchOutcome::NewSessionProcessed {
            validated_remote_lease_set2,
            ..
        } => *validated_remote_lease_set2,
        other => panic!("expected NewSessionProcessed, got {other:?}"),
    };
    // Reverse routing handoff: B installs the validated A LS2.
    side_b
        .routing
        .install_remote_lease_set2(validated_a)
        .expect("install validated sender ls2");

    // B -> A New Session Reply from the retained reply context.
    let hash_a = side_a.hash();
    let form_before = side_b
        .session
        .planned_outbound_form(&side_a.static_public(), NOW_SECONDS);
    assert_eq!(form_before, PlannedOutboundForm::NewSessionReply);
    let plan_nsr = side_b
        .compose(hash_a, b"bootstrap-b", seed_base + 2)
        .expect("compose nsr");
    assert_eq!(plan_nsr.encrypted_message.form_name(), "new-session-reply");
    assert_eq!(
        side_b
            .session
            .planned_outbound_form(&side_a.static_public(), NOW_SECONDS),
        PlannedOutboundForm::ExistingSession,
        "sealing the NSR promotes B's paired session"
    );
    let recovered_nsr = seam_deliver(side_b, side_a, &plan_nsr);
    let envelope_a = recovered_envelope(recovered_nsr);
    assert_eq!(
        side_a
            .session
            .classify(plan_nsr.encrypted_message.message_bytes()),
        ClassifiedInbound::NewSessionReply,
        "A matches the NSR through its pending reply tag window"
    );
    match side_a.dispatch(&envelope_a) {
        InboundDispatchOutcome::NewSessionReplyProcessed { .. } => {}
        other => panic!("expected NewSessionReplyProcessed, got {other:?}"),
    }
    assert_eq!(side_a.session.pending_handshake_count(), 0);
    assert_eq!(
        side_a
            .session
            .planned_outbound_form(&side_b.static_public(), NOW_SECONDS),
        PlannedOutboundForm::ExistingSession
    );
}

fn expect_rejected(outcome: InboundDispatchOutcome) -> InboundDispatchError {
    match outcome {
        InboundDispatchOutcome::Rejected(error) => error,
        other => panic!("expected Rejected, got {other:?}"),
    }
}

fn expect_processed_ok(outcome: InboundDispatchOutcome) -> InboundDispatchOutcome {
    if let InboundDispatchOutcome::Rejected(error) = &outcome {
        panic!("expected processed outcome, got rejection: {error:?}");
    }
    outcome
}

/// Encodes one ECIES outbound message into wire bytes.
fn encoded_wire(outbound: &EciesOutboundMessage) -> Vec<u8> {
    match outbound {
        EciesOutboundMessage::NewSession { message } => message
            .encode_to_vec(MAX_I2NP_PAYLOAD_SIZE)
            .expect("encode ns"),
        EciesOutboundMessage::NewSessionReply(message) => message
            .encode_to_vec(MAX_I2NP_PAYLOAD_SIZE)
            .expect("encode nsr"),
        EciesOutboundMessage::Existing(message) => message
            .encode_to_vec(MAX_I2NP_PAYLOAD_SIZE)
            .expect("encode es"),
    }
}

// ---- §7 Plan 127 master trajectory ----

/// The authoritative Plan 127 closure trajectory: bound NS (bundled
/// LS2) A->B through real tunnels and the seam, LS2 binding at B,
/// production reverse routing, retained-context NSR B->A, then two
/// Existing Session messages per direction with exact-once delivery
/// and advancing tags.
#[test]
fn plan_127_master_trajectory_ns_nsr_es_bidirectional_exact_once() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    let hash_a = side_a.hash();
    let hash_b = side_b.hash();

    // A resolved B's LS2 ahead of the first contact.
    preresolve_remote(&mut side_a, &side_b);

    // ---- §7.1 A -> B bound New Session ----
    let app_a1 = b"plan127-a1".to_vec();
    let plan_ns = side_a
        .compose(hash_b, &app_a1, 0x1270_0001)
        .expect("compose ns");
    assert_eq!(plan_ns.encrypted_message.form_name(), "new-session");
    assert!(matches!(
        plan_ns.encrypted_message,
        EncryptedOutbound::NewSession { .. }
    ));
    // A's NS carries its static key only cryptographically: no
    // cleartext 32-byte window equals Alice's static public key.
    let ns_wire = plan_ns.encrypted_message.message_bytes().to_vec();
    let alice_static = side_a.static_public();
    assert!(
        !ns_wire
            .windows(X25519_KEY_LENGTH)
            .any(|w| w == alice_static.as_slice()),
        "the NS wire form must never carry the static key in cleartext"
    );
    // The OBEP targets the selected Lease2 from B's LS2.
    assert_eq!(
        plan_ns.selected_lease.gateway_router_hash,
        hop_router_hash(B_SEED, 1),
        "selected lease gateway must be B's inbound gateway"
    );

    let recovered = seam_deliver(&side_a, &mut side_b, &plan_ns);
    assert_eq!(recovered, plan_ns.garlic_i2np_bytes);
    let envelope_b = recovered_envelope(recovered);
    let validated_a = match expect_processed_ok(side_b.dispatch(&envelope_b)) {
        InboundDispatchOutcome::NewSessionProcessed {
            local_destination,
            remote_destination_hash,
            validated_remote_lease_set2,
            clove_count,
        } => {
            assert_eq!(
                local_destination,
                side_b.identity.id(),
                "B owner dispatch selects B only"
            );
            assert_eq!(
                remote_destination_hash, hash_a,
                "remote identity derives from the bundled LS2's own Destination"
            );
            assert_eq!(clove_count, 2, "application clove plus DatabaseStore clove");
            validated_remote_lease_set2
        }
        other => panic!("unexpected outcome: {other:?}"),
    };
    // The bundled LS2 usable type-4 key equals the authenticated NS
    // static key (the binding enforced exact equality).
    let ls2_key = validated_a
        .lease_set2()
        .usable_x25519_key()
        .expect("usable x25519 key")
        .as_bytes()
        .to_vec();
    assert_eq!(ls2_key, alice_static.to_vec());
    // The validated record landed in B's dispatcher state under the
    // derived remote DestinationHash.
    assert_eq!(side_b.dispatcher.accepted_lease_set2_count(), 1);
    assert!(
        side_b
            .dispatcher
            .accepted_lease_set2_for(side_b.identity.id(), hash_a)
            .is_some()
    );
    // B received the exact application payload once.
    let queued = side_b
        .dispatcher
        .pop_payload(side_b.identity.id())
        .expect("B application payload");
    let queued_message = I2npMessage::decode_standard(queued.bytes(), MAX_I2NP_PAYLOAD_SIZE)
        .expect("decode queued message");
    match queued_message.body() {
        I2npBody::Data(body) => assert_eq!(body.payload.as_bytes(), app_a1),
        other => panic!("queued payload must be Data, got {other:?}"),
    }
    assert!(
        side_b
            .dispatcher
            .pop_payload(side_b.identity.id())
            .is_none()
    );

    // ---- §4 production reverse routing ----
    // B installs the validated A LS2 without reparsing raw payload
    // bytes and can select a non-expired A lease plus the type-4
    // static public key.
    let installed = side_b
        .routing
        .install_remote_lease_set2((*validated_a).clone())
        .expect("install validated sender ls2");
    assert_eq!(installed, hash_a);
    let mut rng = ChaCha8Rng::seed_from_u64(0x1270_0002);
    let selected_a = side_b
        .routing
        .select_lease(hash_a, NOW_SECONDS, &mut rng)
        .expect("select non-expired A lease");
    assert_eq!(selected_a.gateway_router_hash, hop_router_hash(A_SEED, 1));
    assert_eq!(
        side_b
            .routing
            .remote_static_public_key(hash_a)
            .expect("a static key"),
        alice_static
    );

    // ---- §7.2 B -> A New Session Reply from retained context ----
    let app_b1 = b"plan127-b1".to_vec();
    assert_eq!(
        side_b
            .session
            .planned_outbound_form(&alice_static, NOW_SECONDS),
        PlannedOutboundForm::NewSessionReply,
        "the first reply must use the retained Plan 126 reply context"
    );
    let plan_nsr = side_b
        .compose(hash_a, &app_b1, 0x1270_0003)
        .expect("compose nsr");
    assert!(matches!(
        plan_nsr.encrypted_message,
        EncryptedOutbound::NewSessionReply { .. }
    ));
    assert_eq!(
        side_b
            .session
            .planned_outbound_form(&alice_static, NOW_SECONDS),
        PlannedOutboundForm::ExistingSession,
        "sealing the NSR promotes B's paired session"
    );
    // The NSR rides a real B outbound tunnel toward A's selected
    // Lease2 gateway.
    assert_eq!(
        plan_nsr.selected_lease.gateway_router_hash,
        hop_router_hash(A_SEED, 1)
    );
    let recovered_nsr = seam_deliver(&side_b, &mut side_a, &plan_nsr);
    assert_eq!(recovered_nsr, plan_nsr.garlic_i2np_bytes);
    let envelope_a = recovered_envelope(recovered_nsr);
    assert_eq!(
        side_a
            .session
            .classify(plan_nsr.encrypted_message.message_bytes()),
        ClassifiedInbound::NewSessionReply,
        "A matches the NSR through its pending reply tag/context"
    );
    match expect_processed_ok(side_a.dispatch(&envelope_a)) {
        InboundDispatchOutcome::NewSessionReplyProcessed {
            remote_static_public,
            sender_destination,
            ..
        } => {
            assert_eq!(remote_static_public, side_b.static_public());
            assert_eq!(sender_destination, Some(hash_b));
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
    assert_eq!(side_a.session.pending_handshake_count(), 0);
    let queued_b = side_a
        .dispatcher
        .pop_payload(side_a.identity.id())
        .expect("A application payload from NSR");
    let queued_b_message =
        I2npMessage::decode_standard(queued_b.bytes(), MAX_I2NP_PAYLOAD_SIZE).expect("decode");
    match queued_b_message.body() {
        I2npBody::Data(body) => assert_eq!(body.payload.as_bytes(), app_b1),
        other => panic!("queued NSR payload must be Data, got {other:?}"),
    }

    // ---- §7.3 Existing Session traffic both directions ----
    let exchanges: [(&str, bool, &[u8]); 4] = [
        ("a1", true, b"es-a1"),
        ("b1", false, b"es-b1"),
        ("a2", true, b"es-a2"),
        ("b2", false, b"es-b2"),
    ];
    let mut captured_a_to_b_es: Option<Vec<u8>> = None;
    for (rng_tag, from_a, payload_marker) in exchanges {
        let mut rng_seed: u64 = 0x1270_0010;
        for byte in rng_tag.bytes() {
            rng_seed = (rng_seed << 8) | u64::from(byte);
        }
        let (sender_ref, receiver_ref) = if from_a {
            (&mut side_a, &mut side_b)
        } else {
            (&mut side_b, &mut side_a)
        };
        let target = if from_a { hash_b } else { hash_a };
        let plan = sender_ref
            .compose(target, payload_marker, rng_seed)
            .expect("compose es");
        assert_eq!(
            plan.encrypted_message.form_name(),
            "existing-session",
            "every post-handshake message must be Existing Session"
        );
        if from_a && captured_a_to_b_es.is_none() {
            captured_a_to_b_es = Some(plan.encrypted_message.message_bytes().to_vec());
        }
        let recovered_es = seam_deliver(sender_ref, receiver_ref, &plan);
        let es_envelope = recovered_envelope(recovered_es);
        match expect_processed_ok(receiver_ref.dispatch(&es_envelope)) {
            InboundDispatchOutcome::ExistingSessionProcessed {
                sender_destination, ..
            } => {
                assert_eq!(sender_destination, Some(sender_ref.hash()));
            }
            other => panic!("unexpected ES outcome: {other:?}"),
        }
        let queued_es = receiver_ref
            .dispatcher
            .pop_payload(receiver_ref.identity.id())
            .expect("ES application payload");
        let es_message =
            I2npMessage::decode_standard(queued_es.bytes(), MAX_I2NP_PAYLOAD_SIZE).expect("decode");
        match es_message.body() {
            I2npBody::Data(body) => assert_eq!(body.payload.as_bytes(), payload_marker),
            other => panic!("ES payload must be Data, got {other:?}"),
        }
        assert!(
            receiver_ref
                .dispatcher
                .pop_payload(receiver_ref.identity.id())
                .is_none()
        );
    }

    // Exact-once: replaying any consumed Existing Session envelope is
    // rejected typed and queues nothing.
    let replay_wire = captured_a_to_b_es.expect("captured A->B ES envelope");
    let replay_error = expect_rejected(side_b.dispatch(&garlic_envelope(replay_wire)));
    assert!(
        matches!(
            &replay_error,
            InboundDispatchError::Session(_) | InboundDispatchError::Codec(_)
        ),
        "replayed ES must be bounded-rejected typed, got {replay_error:?}"
    );
    assert!(
        side_b
            .dispatcher
            .pop_payload(side_b.identity.id())
            .is_none()
    );
}

// ---- §9 Failure and security cases ----

/// §9: a validly signed bundled LS2 whose type-4 key does not match
/// the authenticated NS static key is rejected; the binding fails,
/// no application plaintext is queued, and no NSR can be emitted.
#[test]
fn plan_127_ns_with_valid_ls2_but_mismatched_static_key_rejected() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    let side_c = Side::new(C_SEED);
    preresolve_remote(&mut side_a, &side_b);

    // A initiates but bundles C's validly signed LS2.
    let plan = side_a
        .compose_with_bundled(
            side_b.hash(),
            b"mismatch",
            Some(side_c.lease_set2.clone()),
            0x1270_0101,
        )
        .expect("compose ns with wrong ls2");
    let recovered = seam_deliver(&side_a, &mut side_b, &plan);
    let error = expect_rejected(side_b.dispatch(&recovered_envelope(recovered)));
    assert!(
        matches!(error, InboundDispatchError::SenderKeyMismatch),
        "expected SenderKeyMismatch, got {error:?}"
    );
    assert!(
        side_b
            .dispatcher
            .pop_payload(side_b.identity.id())
            .is_none()
    );
    assert_eq!(side_b.dispatcher.accepted_lease_set2_count(), 0);
    // The reply context was dropped: no NSR can be emitted for the
    // unbindable session (Plan 127 §2).
    assert!(
        !side_b
            .session
            .has_provisional_responder(&side_a.static_public())
    );
    assert_ne!(
        side_b
            .session
            .planned_outbound_form(&side_a.static_public(), NOW_SECONDS),
        PlannedOutboundForm::NewSessionReply
    );
}

/// §9: an invalid bundled LS2 signature is rejected before any
/// binding and without queueing application bytes.
#[test]
fn plan_127_ns_with_invalid_ls2_signature_rejected() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    preresolve_remote(&mut side_a, &side_b);

    // Corrupt the trailing signature bytes of A's encoded LS2.
    let encoded = side_a
        .lease_set2
        .encode_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .expect("encode");
    let mut tampered = encoded.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0xFF;
    let broken_ls2 =
        i2pr_proto::LeaseSet2::decode(&tampered, MAX_I2NP_PAYLOAD_SIZE).expect("decode tampered");

    let plan = side_a
        .compose_with_bundled(side_b.hash(), b"bad-sig", Some(broken_ls2), 0x1270_0102)
        .expect("compose ns with tampered ls2");
    let recovered = seam_deliver(&side_a, &mut side_b, &plan);
    let error = expect_rejected(side_b.dispatch(&recovered_envelope(recovered)));
    assert!(
        matches!(error, InboundDispatchError::LeaseSet2Validation(_)),
        "expected LeaseSet2Validation rejection, got {error:?}"
    );
    assert!(
        side_b
            .dispatcher
            .pop_payload(side_b.identity.id())
            .is_none()
    );
    assert!(
        !side_b
            .session
            .has_provisional_responder(&side_a.static_public())
    );
}

/// §9/§2: a bound New Session without exactly one bundled sender
/// LeaseSet2 is rejected; no reverse identity exists.
#[test]
fn plan_127_ns_without_bundled_ls2_rejected() {
    let side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);

    // Hand-craft a bound NS whose payload carries only the
    // application clove.
    let clove = GarlicCloveBlock {
        delivery: GarlicDelivery::Local,
        message: b"no-ls2".to_vec(),
    };
    let payload = encode_new_session_payload(NOW_SECONDS, &clove).expect("payload");
    let mut manager = EciesSessionManager::new(EciesSessionConfig::balanced());
    let mut rng = ChaCha8Rng::seed_from_u64(0x1270_0103);
    let outbound = manager
        .encrypt_to_remote(
            side_a.identity.id(),
            side_a.identity.static_secret_bytes(),
            &side_b.static_public(),
            &side_b.static_public(),
            &payload,
            NOW_SECONDS,
            &mut rng,
        )
        .expect("seal bare ns");
    let ns_wire = encoded_wire(&outbound);
    let error = expect_rejected(side_b.dispatch(&garlic_envelope(ns_wire)));
    assert!(
        matches!(error, InboundDispatchError::MissingSenderLeaseSet2),
        "expected MissingSenderLeaseSet2, got {error:?}"
    );
    assert!(
        side_b
            .dispatcher
            .pop_payload(side_b.identity.id())
            .is_none()
    );
    assert!(
        !side_b
            .session
            .has_provisional_responder(&side_a.static_public())
    );
}

/// §5 compose-side guard: a fresh bound New Session without the local
/// bundled LeaseSet2 fails closed at composition time.
#[test]
fn plan_127_missing_bundled_ls2_fails_composition() {
    let mut side_a = Side::new(A_SEED);
    let side_b = Side::new(B_SEED);
    preresolve_remote(&mut side_a, &side_b);
    let error = side_a
        .compose_with_bundled(side_b.hash(), b"unbundled", None, 0x1270_0104)
        .expect_err("composition must fail closed");
    assert!(
        matches!(error, SendError::MissingBundledLeaseSet2),
        "expected MissingBundledLeaseSet2, got {error:?}"
    );
}

/// §9: an expired sender LS2 rejects the binding, leaves no reverse
/// route, and prevents any NSR.
#[test]
fn plan_127_expired_sender_ls2_blocks_reverse_route_and_nsr() {
    let side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);

    // Craft the NS through the session primitive so the dispatcher
    // sees it under a far-future validation clock. The payload
    // bundles A's LS2 exactly like the composer would.
    let data_clove = GarlicCloveBlock {
        delivery: GarlicDelivery::Local,
        message: b"stale".to_vec(),
    };
    let database_store_clove = GarlicCloveBlock {
        delivery: GarlicDelivery::Local,
        message: encode_dbstore_for(side_a.lease_set2.clone()),
    };
    let mut sequence = i2pr_proto::EciesPayloadSequence::empty();
    sequence
        .push(i2pr_proto::EciesPayloadBlock::DateTime(NOW_SECONDS))
        .unwrap();
    sequence
        .push(i2pr_proto::EciesPayloadBlock::GarlicClove(data_clove))
        .unwrap();
    sequence
        .push(i2pr_proto::EciesPayloadBlock::GarlicClove(
            database_store_clove,
        ))
        .unwrap();
    let payload_two_clove = sequence
        .encode_to_vec(i2pr_client::MAX_DESTINATION_PAYLOAD_BYTES, true)
        .expect("payload");
    let mut manager = EciesSessionManager::new(EciesSessionConfig::balanced());
    let mut rng = ChaCha8Rng::seed_from_u64(0x1270_0105);
    let outbound = manager
        .encrypt_to_remote(
            side_a.identity.id(),
            side_a.identity.static_secret_bytes(),
            &side_b.static_public(),
            &side_b.static_public(),
            &payload_two_clove,
            NOW_SECONDS,
            &mut rng,
        )
        .expect("seal stale ns");
    let ns_wire = encoded_wire(&outbound);

    // Dispatch under a clock past every lease expiry.
    let future = NOW_SECONDS + 40 * 24 * 3600;
    let envelope = garlic_envelope(ns_wire);
    let outcome = {
        let Side {
            dispatcher,
            session,
            routing,
            identity,
            ..
        } = &mut side_b;
        dispatcher.dispatch_garlic_envelope(
            session,
            identity.id(),
            identity.static_secret_bytes(),
            &identity.static_public_bytes(),
            future,
            &envelope,
            routing.lease_set2_store_mut(),
        )
    };
    let error = expect_rejected(outcome);
    assert!(
        matches!(error, InboundDispatchError::LeaseSet2Validation(_)),
        "expected expired-LS2 validation rejection, got {error:?}"
    );
    assert!(
        side_b
            .dispatcher
            .pop_payload(side_b.identity.id())
            .is_none()
    );
    // No reverse route: nothing registered for A.
    let mut rng = ChaCha8Rng::seed_from_u64(1);
    assert!(matches!(
        side_b.routing.select_lease(side_a.hash(), future, &mut rng),
        Err(SendError::LeaseSet2LookupPending)
    ));
    // No NSR possible for the unbindable session.
    assert!(
        !side_b
            .session
            .has_provisional_responder(&side_a.static_public())
    );
}

/// Encodes a DatabaseStore short-transport I2NP envelope wrapping the
/// supplied LeaseSet2 (the canonical bundled-LS2 clove body).
fn encode_dbstore_for(ls2: i2pr_proto::LeaseSet2) -> Vec<u8> {
    let body = I2npBody::DatabaseStore(Box::new(i2pr_proto::DatabaseStoreMessage {
        key: ls2.key_hash().expect("key hash"),
        reply_token: 0,
        reply_tunnel_id: None,
        reply_gateway: None,
        data: i2pr_proto::DatabaseStoreData::LeaseSet2(Box::new(ls2)),
    }));
    let envelope = I2npMessage::new_short_transport(0, 0, body).expect("dbstore envelope");
    envelope
        .encode_short_transport_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .expect("encode dbstore")
}

/// §9: tampered NS ciphertext yields no plaintext and no binding.
#[test]
fn plan_127_tampered_ns_yields_no_plaintext_and_no_binding() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    preresolve_remote(&mut side_a, &side_b);
    let plan = side_a
        .compose(side_b.hash(), b"tamper-me", 0x1270_0106)
        .expect("compose ns");
    let mut wire = plan.encrypted_message.message_bytes().to_vec();
    let mid = wire.len() / 2;
    wire[mid] ^= 0x01;
    let error = expect_rejected(side_b.dispatch(&garlic_envelope(wire)));
    assert!(
        matches!(
            &error,
            InboundDispatchError::Session(i2pr_client::EciesSessionError::Ecies(_))
        ),
        "expected AEAD authentication failure, got {error:?}"
    );
    assert!(
        side_b
            .dispatcher
            .pop_payload(side_b.identity.id())
            .is_none()
    );
}

/// §9: an NSR whose leading tag matches no pending reply window is
/// rejected typed without plaintext, leaving the pending handshake
/// intact.
#[test]
fn plan_127_nsr_wrong_tag_rejected() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    preresolve_remote(&mut side_a, &side_b);
    // A initiates; the pending handshake holds the real reply window.
    let plan_ns = side_a
        .compose(side_b.hash(), b"pending", 0x1270_0107)
        .expect("compose ns");
    let recovered = seam_deliver(&side_a, &mut side_b, &plan_ns);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(recovered)));

    // A well-formed-length NSR with a wrong leading tag.
    let mut fake = vec![0xEE_u8; 8];
    fake.extend_from_slice(&[0x11_u8; 64]);
    let error = expect_rejected(side_a.dispatch(&garlic_envelope(fake)));
    assert!(
        matches!(
            error,
            InboundDispatchError::Codec(_) | InboundDispatchError::EnvelopeTooShort { .. }
        ),
        "expected bounded rejection for wrong-tag NSR, got {error:?}"
    );
    assert_eq!(
        side_a.session.pending_handshake_count(),
        1,
        "the genuine pending handshake must survive the bogus NSR"
    );
    assert!(
        side_a
            .dispatcher
            .pop_payload(side_a.identity.id())
            .is_none()
    );
}

/// §9: a replayed NSR installs no duplicate session and queues no
/// duplicate payload.
#[test]
fn plan_127_nsr_replay_no_duplicate_install_or_payload() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    establish_pair_with_captured_nsr(&mut side_a, &mut side_b, |nsr_wire, side_a, _side_b| {
        // The first acceptance happened inside the helper; replay it.
        let error = expect_rejected(side_a.dispatch(&garlic_envelope(nsr_wire.to_vec())));
        assert!(
            matches!(&error, InboundDispatchError::Session(_)),
            "replayed NSR must be rejected typed without duplicate install, got {error:?}"
        );
        assert_eq!(side_a.session.established_sessions(), 1);
        assert!(
            side_a
                .dispatcher
                .pop_payload(side_a.identity.id())
                .is_some()
        );
        assert!(
            side_a
                .dispatcher
                .pop_payload(side_a.identity.id())
                .is_none()
        );
    });
}

/// Variant of [`establish_pair`] that hands the captured NSR wire
/// bytes to a callback after the bootstrap completes.
fn establish_pair_with_captured_nsr(
    side_a: &mut Side,
    side_b: &mut Side,
    verify: impl FnOnce(&[u8], &mut Side, &mut Side),
) {
    preresolve_remote(side_a, side_b);
    let hash_b = side_b.hash();
    let plan_ns = side_a
        .compose(hash_b, b"bootstrap-a", 0x127B_0101)
        .expect("compose ns");
    let recovered_ns = seam_deliver(side_a, side_b, &plan_ns);
    let validated_a = match side_b.dispatch(&recovered_envelope(recovered_ns)) {
        InboundDispatchOutcome::NewSessionProcessed {
            validated_remote_lease_set2,
            ..
        } => validated_remote_lease_set2,
        other => panic!("expected NewSessionProcessed, got {other:?}"),
    };
    side_b
        .routing
        .install_remote_lease_set2(*validated_a)
        .expect("install validated sender ls2");

    let plan_nsr = side_b
        .compose(side_a.hash(), b"bootstrap-b", 0x127B_0102)
        .expect("compose nsr");
    let nsr_wire = plan_nsr.encrypted_message.message_bytes().to_vec();
    let recovered_nsr = seam_deliver(side_b, side_a, &plan_nsr);
    match side_a.dispatch(&recovered_envelope(recovered_nsr)) {
        InboundDispatchOutcome::NewSessionReplyProcessed { .. } => {}
        other => panic!("expected NewSessionReplyProcessed, got {other:?}"),
    }
    verify(&nsr_wire, side_a, side_b);
}

/// §9: unknown tags, tampered ES ciphertext, and consumed-tag replays
/// are each rejected typed with no application output.
#[test]
fn plan_127_es_unknown_tag_tamper_and_replay_rejected() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    establish_pair(&mut side_a, &mut side_b);

    // Unknown tag: an ES-shaped envelope whose leading tag matches no
    // window is indistinguishable from garbage at the dispatcher and
    // is bounded-rejected with no plaintext either way.
    let forged = ExistingSessionMessage::new([0xEE_u8; 8], vec![0_u8; 104]).expect("forged es");
    let forged_wire = forged.encode_to_vec(MAX_I2NP_PAYLOAD_SIZE).expect("encode");
    let error = expect_rejected(side_b.dispatch(&garlic_envelope(forged_wire)));
    assert!(
        matches!(
            &error,
            InboundDispatchError::Session(_)
                | InboundDispatchError::Codec(_)
                | InboundDispatchError::EnvelopeTooShort { .. }
        ),
        "unknown tag must be bounded-rejected typed, got {error:?}"
    );

    // Tampered ciphertext on a genuine ES: AEAD fails typed and the
    // remove-on-hit window consumes the attempted tag.
    let tampered_plan = side_a
        .compose(side_b.hash(), b"tamper-es", 0x1270_0108)
        .expect("compose tamper es");
    let mut wire = tampered_plan.encrypted_message.message_bytes().to_vec();
    let mid = wire.len() / 2;
    wire[mid] ^= 0x80;
    let error = expect_rejected(side_b.dispatch(&garlic_envelope(wire)));
    assert!(
        matches!(&error, InboundDispatchError::Session(_)),
        "tampered ES must fail typed, got {error:?}"
    );

    // A fresh genuine ES still delivers; replaying its consumed tag
    // is then rejected typed.
    let plan = side_a
        .compose(side_b.hash(), b"genuine-es", 0x1270_0109)
        .expect("compose genuine es");
    let recovered = seam_deliver(&side_a, &mut side_b, &plan);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(recovered)));
    let replay_error = expect_rejected(side_b.dispatch(&garlic_envelope(
        plan.encrypted_message.message_bytes().to_vec(),
    )));
    assert!(
        matches!(&replay_error, InboundDispatchError::Session(_)),
        "consumed tag must not be reusable, got {replay_error:?}"
    );
}

/// §6/§9: ciphertext addressed to B's inbound tunnel cannot be
/// decrypted by another destination's context; no trial decryption
/// across destination keys occurs and nothing is queued anywhere.
#[test]
fn plan_127_wrong_inbound_owner_no_trial_decryption() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    let mut side_c = Side::new(C_SEED);
    preresolve_remote(&mut side_a, &side_b);

    let plan = side_a
        .compose(side_b.hash(), b"for-b-only", 0x1270_0109)
        .expect("compose ns");
    let recovered = seam_deliver(&side_a, &mut side_b, &plan);
    // Deliver to C instead of B.
    let error = expect_rejected(side_c.dispatch(&recovered_envelope(recovered)));
    assert!(
        matches!(&error, InboundDispatchError::Session(_)),
        "C must fail authentication typed, got {error:?}"
    );
    assert!(
        side_c
            .dispatcher
            .pop_payload(side_c.identity.id())
            .is_none()
    );
    // B still owns the message: deliver correctly.
    let outcome =
        expect_processed_ok(side_b.dispatch(&recovered_envelope(plan.garlic_i2np_bytes.clone())));
    assert!(matches!(
        outcome,
        InboundDispatchOutcome::NewSessionProcessed { .. }
    ));
}

/// §9: removing the destination owner surfaces the typed
/// `UnknownDestination` rejection.
#[test]
fn plan_127_removed_destination_owner_typed_unknown_destination() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    preresolve_remote(&mut side_a, &side_b);
    side_b
        .dispatcher
        .unregister_destination(side_b.identity.id());

    let plan = side_a
        .compose(side_b.hash(), b"no-owner", 0x1270_010A)
        .expect("compose ns");
    let recovered = seam_deliver(&side_a, &mut side_b, &plan);
    let error = expect_rejected(side_b.dispatch(&recovered_envelope(recovered)));
    assert!(
        matches!(error, InboundDispatchError::UnknownDestination(hash) if hash == side_b.hash()),
        "expected UnknownDestination, got {error:?}"
    );
}

/// §9/§10: a full application queue fails typed and retains every
/// previously accepted payload intact.
#[test]
fn plan_127_full_application_queue_typed_failure() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    establish_pair(&mut side_a, &mut side_b);
    // Drop the bootstrap payload so the queue starts empty.
    while side_b
        .dispatcher
        .pop_payload(side_b.identity.id())
        .is_some()
    {}

    for index in 0..MAX_INBOUND_PENDING_MESSAGES {
        let payload = format!("q{index}").into_bytes();
        let plan = side_a
            .compose(side_b.hash(), &payload, 0x1270_1000 + index as u64)
            .expect("compose queue es");
        let recovered = seam_deliver(&side_a, &mut side_b, &plan);
        expect_processed_ok(side_b.dispatch(&recovered_envelope(recovered)));
    }
    assert_eq!(
        side_b.dispatcher.queued_payloads(side_b.identity.id()),
        MAX_INBOUND_PENDING_MESSAGES
    );

    // One more message must fail typed.
    let plan = side_a
        .compose(side_b.hash(), b"overflow", 0x1270_2000)
        .expect("compose overflow es");
    let recovered = seam_deliver(&side_a, &mut side_b, &plan);
    let error = expect_rejected(side_b.dispatch(&recovered_envelope(recovered)));
    assert!(
        matches!(error, InboundDispatchError::QueueFull(_)),
        "expected QueueFull, got {error:?}"
    );

    // Draining restores service exactly once per queued payload.
    let mut drained = 0_usize;
    while side_b
        .dispatcher
        .pop_payload(side_b.identity.id())
        .is_some()
    {
        drained += 1;
    }
    assert_eq!(drained, MAX_INBOUND_PENDING_MESSAGES);
    let plan = side_a
        .compose(side_b.hash(), b"after-drain", 0x1270_2001)
        .expect("compose post-drain es");
    let recovered = seam_deliver(&side_a, &mut side_b, &plan);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(recovered)));
}

/// §9/§10: session expiry makes stale ES tags fail while a deliberate
/// new outbound send re-establishes a fresh session.
#[test]
fn plan_127_session_expiry_stale_tags_fail_then_fresh_session_established() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    establish_pair(&mut side_a, &mut side_b);

    // Capture one more genuine ES before expiring everything.
    let plan_es = side_a
        .compose(side_b.hash(), b"pre-expiry", 0x1270_0110)
        .expect("compose pre-expiry es");
    let es_wire = plan_es.encrypted_message.message_bytes().to_vec();

    side_a.advance_session_time(NOW_SECONDS + 700);
    side_b.advance_session_time(NOW_SECONDS + 700);
    assert_eq!(side_a.session.established_sessions(), 0);
    assert_eq!(side_b.session.established_sessions(), 0);

    // Stale ES traffic fails typed with no plaintext output.
    let error = expect_rejected(side_b.dispatch(&garlic_envelope(es_wire)));
    assert!(
        matches!(&error, InboundDispatchError::Session(_)),
        "stale ES tags must fail typed, got {error:?}"
    );

    // A new outbound send deliberately establishes a fresh session.
    let form = side_a
        .session
        .planned_outbound_form(&side_b.static_public(), NOW_SECONDS + 700);
    assert_eq!(form, PlannedOutboundForm::BoundNewSession);
    establish_pair_seeded(&mut side_a, &mut side_b, 0x127E_0000);
    assert_eq!(side_a.session.established_sessions(), 1);
    assert_eq!(side_b.session.established_sessions(), 1);
}

/// §10: the active-remote LeaseSet2 cache enforces its configured
/// ceiling with a typed failure.
#[test]
fn plan_127_active_remote_ceiling_enforced() {
    assert_eq!(MAX_ACTIVE_REMOTES, 256);
    let config = DestinationRoutingConfig::try_new(64, 32, 60, 2).expect("small routing config");
    let mut routing = DestinationRouting::new(config);
    let mut rng = ChaCha8Rng::seed_from_u64(0x1270_0111);

    for seed in [A_SEED, B_SEED, C_SEED] {
        let side = Side::new(seed);
        let validated = ValidatedLeaseSet2::from_lease_set2(
            side.lease_set2.clone(),
            Some(side.hash()),
            i2pr_netdb::LeaseSet2ValidationContext::new(NOW_SECONDS),
        )
        .expect("validated ls2");
        let result = routing.register_resolved_remote(validated);
        if seed == C_SEED {
            match result {
                Err(i2pr_client::LookupIngestError::ActiveRemoteCapacity { maximum }) => {
                    assert_eq!(maximum, 2);
                }
                other => panic!("expected active-remote budget rejection, got {other:?}"),
            }
        } else {
            result.expect("register within ceiling");
        }
    }
    let _ = &mut rng;
}

/// §9: one malformed remote message must not poison an unrelated
/// valid session.
#[test]
fn plan_127_malformed_remote_does_not_poison_valid_session() {
    let mut side_a = Side::new(A_SEED);
    let mut side_b = Side::new(B_SEED);
    establish_pair(&mut side_a, &mut side_b);

    // Malformed remote input at B.
    let malformed = vec![0xFF_u8; 96];
    let _ = expect_rejected(side_b.dispatch(&garlic_envelope(malformed)));

    // A's valid session continues to work in both directions.
    let plan = side_a
        .compose(side_b.hash(), b"still-alive", 0x1270_0112)
        .expect("compose es after malformed");
    assert_eq!(plan.encrypted_message.form_name(), "existing-session");
    let recovered = seam_deliver(&side_a, &mut side_b, &plan);
    expect_processed_ok(side_b.dispatch(&recovered_envelope(recovered)));
    let queued = side_b
        .dispatcher
        .pop_payload(side_b.identity.id())
        .expect("payload after malformed input");
    let message = I2npMessage::decode_standard(queued.bytes(), MAX_I2NP_PAYLOAD_SIZE).expect("dec");
    match message.body() {
        I2npBody::Data(body) => assert_eq!(body.payload.as_bytes(), b"still-alive"),
        other => panic!("payload must be Data, got {other:?}"),
    }
}
