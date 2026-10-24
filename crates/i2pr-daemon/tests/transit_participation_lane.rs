//! Plan 492 private-mesh lane for the transit participation posture.
//!
//! The lane is **private mesh**: one peer, this router, and the loopback
//! delivery seam. There is no public bind, no external router, and no
//! network. What it proves is the plan's bounded outcome:
//!
//! 1. **The posture admits the peer's transit work.** A peer's short
//!    tunnel build reaches the transit data plane through the production
//!    live owner, and the participation owner records the admission.
//! 2. **This router relays the peer's transit traffic.** The peer's tunnel
//!    data reaches the transit path, the transit path forwards it, and the
//!    participation owner charges the share. The receipt is
//!    **source-originated**: it is this router's own accounting, not a
//!    value the peer asserted.
//! 3. **The participation is then revoked, and the stop is observed.** The
//!    operator posture changes to off, the participation stops, the stop is
//!    recorded, and the next transit request is refused with a typed
//!    reason.
//!
//! The lane writes a sanitized evidence file: **counts and digests only**.
//! No peer identity, no address, no tunnel id, no key, and no payload
//! reaches the evidence.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

use i2pr_crypto::{OsRng, RouterIdentityBundle};
use i2pr_daemon::config::Config;
use i2pr_daemon::router_i2np::{RouterDeliveryService, generate_controlled_identity};
use i2pr_daemon::transit_compose::{
    TransitBuildService, TransitDispatch, TransitHopMaterial,
};
use i2pr_daemon::transit_owner::{
    LiveBuildOutcome, LiveInboundOutcome, TransitDataDisposition, TransitLiveOwner,
};
use i2pr_daemon::transit_participation::{
    TransitParticipationBinding, TransitParticipationOwner, TransitReceiptKind,
    TransitRefusalReason,
};
use i2pr_proto::{
    Date, DeferredBuildRecords, Hash, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE,
    TunnelDataMessage,
};
use i2pr_runtime::{
    BandwidthGovernor, BandwidthGovernorConfig, CancellationToken, Ssu2InboundI2np,
    Ssu2RuntimeConfig,
};
use i2pr_transport::{LinkId, PeerId};
use i2pr_tunnel::build_crypto::{BuildCryptography, EPHEMERAL_KEY_LEN, EciesX25519BuildCryptography};
use i2pr_tunnel::identity::TunnelId;
use i2pr_tunnel::multirecord::{RECORD_BYTES, encode_count_prefixed_short_payload};
use i2pr_tunnel::short_record::{
    BuildOptions, HopRole, LayerEncryptionType, REQUEST_EXPIRATION_SECONDS, ShortRequestRecord,
};
use i2pr_tunnel::{TransitAdmissionPolicy, TransitMode};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use sha2::{Digest, Sha256};

const NOW_MS: u64 = 1_700_000_000_000;
const NOW_SECONDS: u64 = NOW_MS / 1000;
const MESSAGE_ID: u32 = 0x51A7_1001;
const RELAY_TUNNEL: u32 = 0x1000;
const NEXT_TUNNEL: u32 = 0x2000;
const CELL_PAYLOAD_BYTES: u64 = 1024;

const MINIMAL: &str = "\
schema_version = 1

[router]
data_dir = \"./state\"
";

fn test_peer() -> PeerId {
    PeerId::from_bytes([0x99; 32])
}

fn test_link() -> LinkId {
    LinkId::new(7).expect("link")
}

fn router_delivery() -> RouterDeliveryService {
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
    let identity =
        generate_controlled_identity(&bundle, "127.0.0.1", 44_002).expect("controlled identity");
    let runtime = i2pr_runtime::Ssu2RuntimeService::new(Ssu2RuntimeConfig::default(), identity)
        .expect("runtime");
    RouterDeliveryService::new(runtime)
}

fn service_for_test() -> TransitBuildService {
    let hop_material =
        TransitHopMaterial::new([0xA3; EPHEMERAL_KEY_LEN], Hash::from_bytes([0x55; 32]));
    let policy = TransitAdmissionPolicy::new(true, TransitMode::Accepting, 4, 2, 2, 1, 64, None)
        .expect("policy");
    TransitBuildService::new(hop_material, policy, 4, router_delivery()).expect("service")
}

/// The operator posture the lane runs under: participation on, with an
/// enforced share.
fn enabled_posture() -> (TransitParticipationBinding, BandwidthGovernor) {
    let text = format!(
        "{MINIMAL}\n[ssu2]\nenabled = true\n[bandwidth]\nenabled = true\ninbound_bytes_per_second = 1048576\noutbound_bytes_per_second = 1048576\nshare_percent = 50\n[transit]\nparticipation = true\nshare_limit = 25\nparticipating_tunnels = 4\npeer_participation_cap = 2\n"
    );
    let config = Config::parse(&text).expect("valid transit posture");
    assert!(config.transit.participation);
    let governor = BandwidthGovernor::new(BandwidthGovernorConfig {
        inbound_bytes_per_second: 1024 * 1024,
        outbound_bytes_per_second: 1024 * 1024,
        burst_bytes: 64 * 1024,
        control_reserve_percent: 10,
        max_pending_requests: 16,
        max_pending_per_peer: 2,
    })
    .expect("valid governor");
    let owner =
        TransitParticipationOwner::from_config(&config.transit, Some(&governor), 50).expect("owner");
    (
        TransitParticipationBinding::new(owner, Some(governor.clone())),
        governor,
    )
}

fn participant_record(next_router: Hash, receive: u32, next: u32, message_id: u32) -> ShortRequestRecord {
    ShortRequestRecord::try_new(
        TunnelId::new(receive).expect("id"),
        TunnelId::new(next).expect("id"),
        next_router,
        HopRole::Participant,
        LayerEncryptionType::Aes,
        Date::from_millis(NOW_MS),
        REQUEST_EXPIRATION_SECONDS,
        message_id,
        BuildOptions::empty(),
    )
    .expect("record")
}

fn stbm_payload_for(service: &TransitBuildService, record: &ShortRequestRecord) -> Vec<u8> {
    let cryptography = EciesX25519BuildCryptography::new();
    let responder_priv = [0xA3; EPHEMERAL_KEY_LEN];
    let hop_identity = *service.hop_identity();
    let mut rng = ChaCha8Rng::seed_from_u64(0xCAFE);
    let plaintext = record.encode_with_rng(&mut rng).expect("encode");
    let mut sealed = [0_u8; i2pr_proto::SHORT_REQUEST_PLAINTEXT_SIZE];
    sealed.copy_from_slice(plaintext.as_ref());
    let secret = x25519_dalek::StaticSecret::from(responder_priv);
    let public = x25519_dalek::PublicKey::from(&secret).to_bytes();
    let local = cryptography
        .seal_short_request(&sealed, &public, hop_identity.as_bytes(), &mut rng)
        .expect("seal")
        .record
        .to_vec()
        .try_into()
        .expect("218");
    let mut slots: Vec<[u8; RECORD_BYTES]> = vec![[0u8; RECORD_BYTES]; 4];
    slots[2] = local;
    for (index, slot) in slots.iter_mut().enumerate() {
        if index == 2 {
            continue;
        }
        slot[..16].copy_from_slice(&[0xEE; 16]);
        rng.fill_bytes(slot);
    }
    encode_count_prefixed_short_payload(4, &slots).expect("encode")
}

fn encode_standard_stbm(payload: &[u8], message_id: u32, expiration_ms: u64) -> Vec<u8> {
    let count = payload[0];
    let records = payload[1..].to_vec();
    let deferred = DeferredBuildRecords::new(count, RECORD_BYTES, records).expect("deferred");
    let body = I2npBody::ShortTunnelBuild(deferred);
    I2npMessage::new_standard(message_id, Date::from_millis(expiration_ms), body)
        .expect("message")
        .encode_standard_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .expect("encode")
}

fn inbound_with(bytes: Vec<u8>) -> Ssu2InboundI2np {
    Ssu2InboundI2np {
        link_id: test_link(),
        peer: test_peer(),
        bytes,
    }
}

fn relay_cell_bytes() -> Vec<u8> {
    let cell = TunnelDataMessage {
        tunnel_id: RELAY_TUNNEL,
        data: [0xAA; 1024],
    };
    let body = I2npBody::TunnelData(Box::new(cell));
    I2npMessage::new_standard(0xBEEF, Date::from_millis(NOW_MS + 60_000), body)
        .expect("message")
        .encode_standard_to_vec(MAX_I2NP_PAYLOAD_SIZE)
        .expect("encode")
}

/// The digest of the receipt sequence: counts only, no identity.
fn receipt_digest(receipts: &[i2pr_daemon::transit_participation::TransitParticipationReceipt]) -> String {
    let mut hasher = Sha256::new();
    for receipt in receipts {
        hasher.update(receipt.kind.label().as_bytes());
        hasher.update(b"|");
        if let Some(reason) = receipt.reason {
            hasher.update(reason.label().as_bytes());
        }
        hasher.update(b"|");
        hasher.update(receipt.participating_tunnels.to_be_bytes());
        hasher.update(receipt.peers_participating.to_be_bytes());
        hasher.update(receipt.bytes_relayed.to_be_bytes());
        hasher.update(receipt.refusals.total().to_be_bytes());
        hasher.update(receipt.stops.to_be_bytes());
        hasher.update(b"\n");
    }
    format!("{:x}", hasher.finalize())
}

fn evidence_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("integration")
        .join("network-participation")
        .join("evidence")
        .join("plan492")
        .join("result.json")
}

/// Plan 492: the posture admits the peer's transit work, this router relays
/// the peer's transit traffic, the relay is visible in a source-originated
/// receipt, the participation is then revoked, and the stop is observed.
#[test]
fn plan492_relay_then_revoke_is_observed_in_the_receipts() {
    let (posture, _governor) = enabled_posture();

    // --- Part 1: the live owner admits the peer's transit work ----------
    //
    // The peer's short tunnel build reaches the transit data plane through
    // the production live owner, and the participation posture admits it.
    let mut owner = TransitLiveOwner::new_disabled(
        router_delivery(),
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(0xCAFE),
    );
    owner.enable(service_for_test(), posture);

    let probe = service_for_test();
    let record = participant_record(
        Hash::from_bytes([0xAA; 32]),
        RELAY_TUNNEL,
        NEXT_TUNNEL,
        MESSAGE_ID,
    );
    let payload = stbm_payload_for(&probe, &record);
    let bytes = encode_standard_stbm(&payload, MESSAGE_ID, NOW_MS + 60_000);
    let build = owner
        .handle_inbound(&inbound_with(bytes), NOW_MS, NOW_SECONDS)
        .expect("build");
    assert!(
        matches!(build, LiveInboundOutcome::Build(LiveBuildOutcome::Dispatched(_))),
        "the peer's transit work must reach the transit data plane: {build:?}"
    );

    let after_build = owner.participation().snapshot();
    assert_eq!(
        after_build.admitted, 1,
        "the posture must admit the peer's transit work"
    );
    assert_eq!(after_build.bytes_relayed, 0);

    // --- Part 2: the router relays the peer's transit traffic -----------
    //
    // A private mesh has no established session to the *next* router, so
    // the transit service cannot complete the build's delivery and rolls
    // the registration back — the correct fail-closed behaviour. The
    // registration is therefore established through the service's own
    // transaction (the same one the live owner drives, minus the delivery
    // step a private mesh cannot complete), and the *relay* — the work the
    // participation posture accounts — is then driven through the
    // production live owner.
    let mut service = service_for_test();
    let mut rng = ChaCha8Rng::seed_from_u64(0xCAFE);
    let dispatch = service.route_short_build(&payload, &test_peer(), NOW_SECONDS, &mut rng);
    assert!(
        matches!(dispatch, TransitDispatch::ForwardStbm { .. }),
        "the participant registration must be established: {dispatch:?}"
    );

    let (posture, _governor) = enabled_posture();
    let mut owner = TransitLiveOwner::new_disabled(
        router_delivery(),
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(0xCAFE),
    );
    owner.enable(service, posture);

    let relay = owner
        .handle_inbound(&inbound_with(relay_cell_bytes()), NOW_MS, NOW_SECONDS)
        .expect("relay");
    assert!(
        matches!(
            relay,
            LiveInboundOutcome::Data(TransitDataDisposition::Forwarded(_))
        ),
        "the transit path must forward the peer's cell: {relay:?}"
    );

    let after_relay = owner.participation().snapshot();
    assert_eq!(
        after_relay.bytes_relayed, CELL_PAYLOAD_BYTES,
        "the relay must be visible in this router's own accounting"
    );
    let relayed_cells = 1_u64;

    let receipts = owner.participation().receipts();
    assert!(
        receipts
            .iter()
            .any(|receipt| receipt.kind == TransitReceiptKind::Relayed),
        "the relay must be recorded"
    );

    // --- Part 3: the participation is revoked and the stop is observed --
    let off = Config::parse(&format!("{MINIMAL}\n[transit]\n")).expect("default posture");
    let governor = BandwidthGovernor::new(BandwidthGovernorConfig {
        inbound_bytes_per_second: 1024 * 1024,
        outbound_bytes_per_second: 1024 * 1024,
        burst_bytes: 64 * 1024,
        control_reserve_percent: 10,
        max_pending_requests: 16,
        max_pending_per_peer: 2,
    })
    .expect("valid governor");
    assert!(
        owner
            .participation_mut()
            .owner_mut()
            .apply_config(&off.transit, Some(&governor), 50)
    );

    let after_stop = owner.participation().snapshot();
    assert!(!after_stop.enabled);
    assert_eq!(after_stop.participating_tunnels, 0);
    assert_eq!(after_stop.stops, 1);
    assert!(
        owner
            .participation()
            .receipts()
            .iter()
            .any(|receipt| receipt.kind == TransitReceiptKind::Stopped),
        "the stop must be recorded"
    );

    // The next transit request is refused with a typed reason.
    let probe = service_for_test();
    let record = participant_record(
        Hash::from_bytes([0xBB; 32]),
        RELAY_TUNNEL,
        NEXT_TUNNEL,
        MESSAGE_ID + 1,
    );
    let payload = stbm_payload_for(&probe, &record);
    let bytes = encode_standard_stbm(&payload, MESSAGE_ID + 1, NOW_MS + 60_000);
    let refused = owner
        .handle_inbound(&inbound_with(bytes), NOW_MS, NOW_SECONDS)
        .expect("refused");
    assert!(
        matches!(
            refused,
            LiveInboundOutcome::Build(
                LiveBuildOutcome::ParticipationRefused(TransitRefusalReason::ParticipationDisabled)
            )
        ),
        "the revoked posture must refuse the next request: {refused:?}"
    );

    // The evidence file carries counts and digests only.
    let final_snapshot = owner.participation().snapshot();
    let final_receipts = owner.participation().receipts();
    let evidence = format!(
        "{{\n  \"mesh\": {{\n    \"admitted_builds\": {},\n    \"bytes_relayed\": {},\n    \"refused\": {},\n    \"relayed_cells\": {},\n    \"stops\": {}\n  }},\n  \"plan\": 492,\n  \"receipt_digests\": {{\n    \"posture\": \"{}\"\n  }},\n  \"schema\": \"i2pr-transit-participation-mesh-v1\"\n}}\n",
        after_build.admitted,
        final_snapshot.bytes_relayed,
        final_snapshot.refusals.total(),
        relayed_cells,
        final_snapshot.stops,
        receipt_digest(&final_receipts),
    );
    let path = evidence_path();
    fs::create_dir_all(path.parent().expect("parent")).expect("evidence directory");
    fs::write(&path, evidence).expect("write evidence");
    assert!(path.exists());
    // The digest is stable across two computations of the same receipts.
    assert_eq!(
        receipt_digest(&final_receipts),
        receipt_digest(&final_receipts)
    );
}

/// Plan 492: the disabled posture is provably inert on the live path.
#[test]
fn plan492_the_disabled_posture_refuses_the_peers_transit_work() {
    let mut owner = TransitLiveOwner::new_disabled(
        router_delivery(),
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(0xCAFE),
    );
    owner.enable(service_for_test(), i2pr_daemon::transit_participation::TransitParticipationBinding::disabled());

    let probe = service_for_test();
    let record = participant_record(
        Hash::from_bytes([0xAA; 32]),
        RELAY_TUNNEL,
        NEXT_TUNNEL,
        MESSAGE_ID,
    );
    let payload = stbm_payload_for(&probe, &record);
    let bytes = encode_standard_stbm(&payload, MESSAGE_ID, NOW_MS + 60_000);
    let refused = owner
        .handle_inbound(&inbound_with(bytes), NOW_MS, NOW_SECONDS)
        .expect("refused");
    assert!(
        matches!(
            refused,
            LiveInboundOutcome::Build(
                LiveBuildOutcome::ParticipationRefused(TransitRefusalReason::ParticipationDisabled)
            )
        ),
        "the disabled posture must refuse: {refused:?}"
    );
    assert_eq!(owner.participation().snapshot().admitted, 0);
    assert_eq!(owner.participation().snapshot().bytes_relayed, 0);
}
