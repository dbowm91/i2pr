//! Plan 502 — the transit data-plane owner in the product composition.
//!
//! The module owns the derivation the production composition needs and
//! constructs the live transit owner. It is a pure function of the
//! normalized configuration, the already-loaded router identity bundle,
//! and the transport seams the SSU2 service already owns: it opens no
//! socket, spawns no task, starts no timer, and **decodes no I2NP**.
//!
//! ```text
//! Config + governor + identity bundle + delivery seam
//!   -> admission policy (derived, ceiling-checked)
//!   -> hop material (bundle static private key + this router's hash)
//!   -> participation binding (the enforced bandwidth share)
//!   -> ChaCha8Rng seeded from OS entropy
//!   -> Option<TransitLiveOwner<ChaCha8Rng>>
//! ```
//!
//! `None` is the default posture: when the operator did not opt in the
//! composition constructs **nothing at all** — no service, no counters,
//! no owner, and no entropy is drawn.
//!
//! The module also owns the production inbound dispatch
//! ([`dispatch_transit_inbound`]): the installed owner is consulted
//! first and the controlled disabled probe is the **fallback for the
//! no-owner case only**, so the two can never run beside each other.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_proto::Hash;
use i2pr_runtime::{BandwidthGovernor, CancellationToken, Ssu2InboundI2np};
use i2pr_transport::PeerId;
use i2pr_tunnel::build_crypto::EPHEMERAL_KEY_LEN;
use i2pr_tunnel::transit::{
    MAX_TRANSIT_ACTIVE, MAX_TRANSIT_PEER_ACTIVE, MAX_TRANSIT_PEER_PENDING, MAX_TRANSIT_PENDING,
    TransitAdmissionConfigError, TransitAdmissionPolicy, TransitMode,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use thiserror::Error;

use crate::config::{BandwidthConfig, TransitConfig};
use crate::router_i2np::{RouterDeliveryService, RouterI2npOutcome};
use crate::transit_compose::{TransitBuildService, TransitHopMaterial, TransitServiceError};
use crate::transit_owner::{
    LiveInboundOutcome, TransitLiveError, TransitLiveOwner, controlled_transit_disabled_probe,
};
use crate::transit_participation::{
    TransitParticipationBinding, TransitParticipationError, TransitParticipationOwner,
};

/// A typed failure of the transit composition.
///
/// Every variant is a **refusal**: the caller keeps the router running
/// without a transit owner rather than substituting a value that would
/// imply participation.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TransitCompositionError {
    /// The operator opted in but a required input was not supplied.
    #[error("the transit composition is missing the {0} input")]
    MissingInput(&'static str),
    /// The derived admission policy was refused by the policy constructor.
    #[error("the transit admission policy was refused: {0}")]
    AdmissionPolicy(#[from] TransitAdmissionConfigError),
    /// The participation posture was refused by its own constructor.
    #[error("the transit participation posture was refused: {0}")]
    Participation(#[from] TransitParticipationError),
    /// The transit build service was refused by its own constructor.
    #[error("the transit build service was refused: {0}")]
    Service(#[from] TransitServiceError),
    /// The router identity could not be hashed.
    #[error("the router identity hash was refused")]
    IdentityHash,
    /// The operating-system entropy source refused a seed.
    #[error("operating-system randomness is unavailable")]
    Entropy,
    /// A derived value exceeded a documented ceiling.
    #[error("the derived {0} count {1} exceeds the documented ceiling {2}")]
    CeilingExceeded(&'static str, u32, u32),
    /// The live owner refused the inbound message.
    #[error("the transit live owner refused the inbound message: {0}")]
    Live(#[from] TransitLiveError),
}

/// Everything the composition needs to construct the owner.
///
/// A `None` input is a refusal, never a substituted measurement: the
/// composition then constructs nothing and the router keeps running
/// without a transit owner.
pub struct TransitCompositionInputs<'a> {
    /// The normalized `[transit]` section of the daemon configuration.
    pub transit: TransitConfig,
    /// The normalized `[bandwidth]` section of the daemon configuration.
    pub bandwidth: BandwidthConfig,
    /// The process-wide bandwidth governor, when one is installed.
    pub governor: Option<&'a BandwidthGovernor>,
    /// The narrow outbound delivery capability of the SSU2 service.
    pub delivery: Option<RouterDeliveryService>,
    /// The owner's cancellation token: the **outer** owner's real token.
    pub cancellation: Option<CancellationToken>,
    /// The persistent router identity bundle the hop key comes from.
    pub identity: Option<&'a RouterIdentityBundle>,
}

/// The derived admission-policy inputs.
///
/// The values are exposed read-only so an evidence lane can observe the
/// derivation itself, without a live owner and without reaching into the
/// tunnel crate's private state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitAdmissionDerivation {
    /// The operator's participating-tunnel count, as the active ceiling.
    pub max_active: u16,
    /// The pending-admission ceiling.
    pub max_pending: u16,
    /// The operator's per-peer participation cap.
    pub max_active_per_peer: u16,
    /// The per-peer pending-admission ceiling.
    pub max_pending_per_peer: u16,
    /// The enforced transit share, in KB/s.
    pub available_bandwidth_kbps: u32,
    /// The transit registry capacity.
    pub registry_capacity: u16,
}

/// Derives the transit admission policy from the normalized configuration.
///
/// The derivation never widens a configured value to make a policy fit:
/// the operator's counts are converted with a checked conversion and the
/// policy constructor's own ceilings are the authority. A value the
/// constructor rejects is a derivation bug, not a value to shrink.
pub fn derive_admission_policy(
    transit: &TransitConfig,
    bandwidth: &BandwidthConfig,
    governor: Option<&BandwidthGovernor>,
) -> Result<TransitAdmissionDerivation, TransitCompositionError> {
    // The transit posture may only spend an enforced share, so a router
    // without a bandwidth governor has no share to spend and the
    // composition refuses rather than inventing one.
    let Some(governor) = governor else {
        return Err(TransitCompositionError::MissingInput("bandwidth governor"));
    };
    let max_active = u16::try_from(transit.participating_tunnels)
        .map_err(|_| TransitCompositionError::MissingInput("participating tunnel count"))?;
    let max_active_per_peer = u16::try_from(transit.peer_participation_cap)
        .map_err(|_| TransitCompositionError::MissingInput("peer participation cap"))?;
    // The pending counts have no configuration knob and none is added
    // here: they are the tunnel crate's documented ceilings.
    let max_pending = MAX_TRANSIT_PENDING;
    let max_pending_per_peer = MAX_TRANSIT_PEER_PENDING;
    // The available bandwidth is the share the governor permits the
    // transit posture to spend — the same arithmetic the participation
    // posture charges against, not an invented number.
    let available_bandwidth_kbps = u32::try_from(
        enforced_transit_share_bytes_per_second(transit, bandwidth, governor)? / 1024,
    )
    .unwrap_or(u32::MAX);
    let derivation = TransitAdmissionDerivation {
        max_active,
        max_pending,
        max_active_per_peer,
        max_pending_per_peer,
        available_bandwidth_kbps,
        // The registry holds one entry per active transit tunnel.
        registry_capacity: max_active,
    };
    // The documented ceilings are the tunnel crate's authority. A derived
    // value outside one is a derivation bug, never a value to shrink.
    if derivation.max_active > MAX_TRANSIT_ACTIVE {
        return Err(TransitCompositionError::CeilingExceeded(
            "active transit tunnels",
            u32::from(derivation.max_active),
            u32::from(MAX_TRANSIT_ACTIVE),
        ));
    }
    if derivation.max_pending > MAX_TRANSIT_PENDING {
        return Err(TransitCompositionError::CeilingExceeded(
            "pending transit tunnels",
            u32::from(derivation.max_pending),
            u32::from(MAX_TRANSIT_PENDING),
        ));
    }
    if derivation.max_active_per_peer > MAX_TRANSIT_PEER_ACTIVE {
        return Err(TransitCompositionError::CeilingExceeded(
            "active transit tunnels per peer",
            u32::from(derivation.max_active_per_peer),
            u32::from(MAX_TRANSIT_PEER_ACTIVE),
        ));
    }
    if derivation.max_pending_per_peer > MAX_TRANSIT_PEER_PENDING {
        return Err(TransitCompositionError::CeilingExceeded(
            "pending transit tunnels per peer",
            u32::from(derivation.max_pending_per_peer),
            u32::from(MAX_TRANSIT_PEER_PENDING),
        ));
    }
    Ok(derivation)
}

/// Returns the enforced transit share in bytes per second.
///
/// The share is the governor's outbound limit reduced by the operator's
/// configured share and then by the transit posture's own share limit.
/// The participation posture computes the same number, so the admission
/// policy and the accounting can never disagree.
fn enforced_transit_share_bytes_per_second(
    transit: &TransitConfig,
    bandwidth: &BandwidthConfig,
    governor: &BandwidthGovernor,
) -> Result<u64, TransitCompositionError> {
    let owner =
        TransitParticipationOwner::from_config(transit, Some(governor), bandwidth.share_percent)?;
    Ok(owner.share_bytes_per_second())
}

/// Derives the transit hop material from the persistent router identity.
///
/// Both values come from the router identity bundle: the bundle's
/// static private key and **this router's own** identity hash. The
/// material zeroizes on drop and redacts its private key in
/// diagnostics.
fn derive_hop_material(
    identity: &RouterIdentityBundle,
) -> Result<TransitHopMaterial, TransitCompositionError> {
    let hop_identity = identity
        .identity()
        .hash()
        .map_err(|_| TransitCompositionError::IdentityHash)?;
    let hop_static_priv: [u8; EPHEMERAL_KEY_LEN] = *identity.encryption_key().secret_bytes();
    Ok(TransitHopMaterial::new(hop_static_priv, hop_identity))
}

/// Constructs the transit data-plane owner for the product composition.
///
/// `Ok(None)` is the default posture: the operator did not opt in and
/// **nothing at all** was constructed — no service, no counters, no
/// owner, and no entropy was drawn.
///
/// A refusal is an `Err`: the caller keeps the router running without an
/// owner and never falls back to something that implies participation.
pub(crate) fn build_transit_owner(
    inputs: &TransitCompositionInputs<'_>,
) -> Result<Option<TransitLiveOwner<ChaCha8Rng>>, TransitCompositionError> {
    if !inputs.transit.participation {
        return Ok(None);
    }
    let Some(governor) = inputs.governor else {
        return Err(TransitCompositionError::MissingInput("bandwidth governor"));
    };
    let Some(delivery) = inputs.delivery.clone() else {
        return Err(TransitCompositionError::MissingInput(
            "router delivery seam",
        ));
    };
    let Some(cancellation) = inputs.cancellation.clone() else {
        return Err(TransitCompositionError::MissingInput("cancellation token"));
    };
    let Some(identity) = inputs.identity else {
        return Err(TransitCompositionError::MissingInput(
            "router identity bundle",
        ));
    };
    let derivation = derive_admission_policy(&inputs.transit, &inputs.bandwidth, Some(governor))?;
    let policy = TransitAdmissionPolicy::new(
        true,
        TransitMode::Accepting,
        derivation.max_active,
        derivation.max_pending,
        derivation.max_active_per_peer,
        derivation.max_pending_per_peer,
        derivation.available_bandwidth_kbps,
        None,
    )?;
    let hop_material = derive_hop_material(identity)?;
    let participation = TransitParticipationOwner::from_config(
        &inputs.transit,
        Some(governor),
        inputs.bandwidth.share_percent,
    )?;
    let binding = TransitParticipationBinding::new(participation, Some(governor.clone()));
    let service = TransitBuildService::new(
        hop_material,
        policy,
        derivation.registry_capacity,
        delivery.clone(),
    )?;
    // The owner's RNG must be seeded from operating-system entropy: a
    // fixed or derived seed is not acceptable.
    let rng = ChaCha8Rng::try_from_os_rng().map_err(|_| TransitCompositionError::Entropy)?;
    let mut owner = TransitLiveOwner::new_disabled(delivery, cancellation, rng);
    owner.enable(service, binding);
    Ok(Some(owner))
}

/// Composes the transit owner for the supplied inputs.
///
/// This is the public face of the composition: an out-of-crate evidence
/// lane observes the product's own decision through it rather than
/// through a proxy. It runs exactly the code path the production
/// composition runs.
pub fn compose_transit_owner(
    inputs: &TransitCompositionInputs<'_>,
) -> Result<Option<TransitLiveOwner<ChaCha8Rng>>, TransitCompositionError> {
    build_transit_owner(inputs)
}

/// Dispatches one inbound message through the transit composition.
///
/// The **installed owner** is consulted first. The controlled disabled
/// probe is the fallback for the **no-owner case only**, so the probe
/// can never run beside a live owner.
///
/// The raw inbound is handed to the owner unchanged: the owner performs
/// its own single canonical decode, so this seam never decodes I2NP.
pub(crate) fn dispatch_transit_inbound(
    owner: &mut Option<TransitLiveOwner<ChaCha8Rng>>,
    outcome: &RouterI2npOutcome,
    inbound: &Ssu2InboundI2np,
    now_ms: u64,
    now_seconds: u64,
) -> Result<LiveInboundOutcome, TransitCompositionError> {
    match owner {
        Some(owner) => Ok(owner.handle_inbound(inbound, now_ms, now_seconds)?),
        None => {
            let _ = controlled_transit_disabled_probe(outcome);
            Ok(LiveInboundOutcome::Ignored)
        }
    }
}

/// Reconciles the owner's peer index against the live session set and
/// sweeps expired registrations.
///
/// The index is the owner's record of which routers it can reach:
/// without it a forward decides to relay and then reports that it
/// reached nobody. The reconcile is a bounded pass over the live
/// session set (bounded by `MAX_SSU2_ACTIVE_CEILING`) and over the
/// installed set (bounded by the transit peer index's own capacity); it
/// runs on the daemon's existing pump tick, never per message.
pub(crate) fn reconcile_transit_peers(
    owner: &mut TransitLiveOwner<ChaCha8Rng>,
    installed: &mut BTreeSet<Hash>,
    live_hashes: &[Hash],
    now_seconds: u64,
) {
    // Install every live peer that is not already indexed. `PeerId` is a
    // newtype over the router identity hash, so the mapping is exact.
    for hash in live_hashes {
        if !installed.insert(*hash) {
            continue;
        }
        // A refused install is a counted outcome inside the service, not
        // a reason to abort the sweep: the index stays bounded and the
        // router keeps running.
        let _ = owner.install_peer(*hash, PeerId::from_hash(*hash));
    }
    // Remove every peer whose session is gone, so a later forward
    // reports `NoActiveSession` instead of dispatching to a stale peer.
    let departed: Vec<Hash> = installed
        .iter()
        .copied()
        .filter(|hash| !live_hashes.contains(hash))
        .collect();
    for hash in departed {
        owner.note_session_closed(&PeerId::from_hash(hash));
        installed.remove(&hash);
    }
    // Sweep the registrations whose lifetime expired.
    owner.expire(now_seconds);
}

#[cfg(test)]
mod tests {
    use super::*;

    use i2pr_crypto::OsRng;
    use i2pr_proto::{Date, I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE};
    use i2pr_runtime::{BandwidthGovernorConfig, Ssu2RuntimeConfig, Ssu2RuntimeService};
    use i2pr_transport::LinkId;
    use i2pr_tunnel::build_crypto::{BuildCryptography, EciesX25519BuildCryptography};
    use i2pr_tunnel::identity::TunnelId;
    use i2pr_tunnel::multirecord::{RECORD_BYTES, encode_count_prefixed_short_payload};
    use i2pr_tunnel::short_record::{
        BuildOptions, HopRole, LayerEncryptionType, REQUEST_EXPIRATION_SECONDS, ShortRequestRecord,
    };
    use rand_chacha::ChaCha8Rng;
    use rand_core::{RngCore, SeedableRng};

    use crate::Config;
    use crate::router_i2np::{RouterDeliveryOutcome, generate_controlled_identity};
    use crate::transit_owner::LiveBuildOutcome;

    const NOW_MS: u64 = 1_700_000_000_000;
    const NOW_SECONDS: u64 = NOW_MS / 1000;
    const MESSAGE_ID: u32 = 0x51A7_1001;
    const RELAY_TUNNEL: u32 = 0x1000;
    const NEXT_TUNNEL: u32 = 0x2000;

    const MINIMAL: &str = "\
schema_version = 1

[router]
data_dir = \"./state\"
";

    /// The default configuration: the operator did not opt in.
    fn default_config() -> Config {
        Config::parse(MINIMAL).expect("default config")
    }

    /// The opted-in configuration with an enforced bandwidth share.
    fn opted_in_config() -> Config {
        let text = format!(
            "{MINIMAL}\n[ssu2]\nenabled = true\n[bandwidth]\nenabled = true\ninbound_bytes_per_second = 1048576\noutbound_bytes_per_second = 1048576\nshare_percent = 50\n[transit]\nparticipation = true\nshare_limit = 25\nparticipating_tunnels = 4\npeer_participation_cap = 2\n"
        );
        Config::parse(&text).expect("valid transit posture")
    }

    fn governor() -> BandwidthGovernor {
        BandwidthGovernor::new(BandwidthGovernorConfig {
            inbound_bytes_per_second: 1024 * 1024,
            outbound_bytes_per_second: 1024 * 1024,
            burst_bytes: 64 * 1024,
            control_reserve_percent: 10,
            max_pending_requests: 16,
            max_pending_per_peer: 2,
        })
        .expect("valid governor")
    }

    fn identity() -> RouterIdentityBundle {
        RouterIdentityBundle::generate(&mut OsRng).expect("identity")
    }

    fn delivery(bundle: &RouterIdentityBundle) -> RouterDeliveryService {
        let controlled =
            generate_controlled_identity(bundle, "127.0.0.1", 44_002).expect("controlled identity");
        let runtime =
            Ssu2RuntimeService::new(Ssu2RuntimeConfig::default(), controlled).expect("runtime");
        RouterDeliveryService::new(runtime)
    }

    fn test_peer() -> PeerId {
        PeerId::from_bytes([0x99; 32])
    }

    fn test_link() -> LinkId {
        LinkId::new(7).expect("link")
    }

    fn inbound_with(bytes: Vec<u8>) -> Ssu2InboundI2np {
        Ssu2InboundI2np {
            link_id: test_link(),
            peer: test_peer(),
            bytes,
        }
    }

    /// The count-prefixed short-build payload the transit service
    /// decodes, sealed for this router's hop identity.
    fn stbm_payload(bundle: &RouterIdentityBundle) -> Vec<u8> {
        let cryptography = EciesX25519BuildCryptography::new();
        let record = ShortRequestRecord::try_new(
            TunnelId::new(RELAY_TUNNEL).expect("id"),
            TunnelId::new(NEXT_TUNNEL).expect("id"),
            Hash::from_bytes([0xAA; 32]),
            HopRole::Participant,
            LayerEncryptionType::Aes,
            Date::from_millis(NOW_MS),
            REQUEST_EXPIRATION_SECONDS,
            MESSAGE_ID,
            BuildOptions::empty(),
        )
        .expect("record");
        let mut rng = ChaCha8Rng::seed_from_u64(0xCAFE);
        let plaintext = record.encode_with_rng(&mut rng).expect("encode");
        let mut sealed = [0_u8; i2pr_proto::SHORT_REQUEST_PLAINTEXT_SIZE];
        sealed.copy_from_slice(plaintext.as_ref());
        let responder_priv = bundle.encryption_key().secret_bytes().to_owned();
        let secret = x25519_dalek::StaticSecret::from(responder_priv);
        let public = x25519_dalek::PublicKey::from(&secret).to_bytes();
        let hop_identity = bundle.identity().hash().expect("hash");
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
        let deferred =
            i2pr_proto::DeferredBuildRecords::new(count, RECORD_BYTES, records).expect("deferred");
        let body = I2npBody::ShortTunnelBuild(deferred);
        I2npMessage::new_standard(message_id, Date::from_millis(expiration_ms), body)
            .expect("message")
            .encode_standard_to_vec(MAX_I2NP_PAYLOAD_SIZE)
            .expect("encode")
    }

    /// The default configuration constructs **nothing at all**: no
    /// owner, no service, no counters, and no entropy.
    #[test]
    fn the_default_configuration_constructs_no_owner() {
        let config = default_config();
        assert!(!config.transit.participation);
        // The inputs are deliberately absent: a default configuration
        // must not even need them.
        let inputs = TransitCompositionInputs {
            transit: config.transit,
            bandwidth: config.bandwidth,
            governor: None,
            delivery: None,
            cancellation: None,
            identity: None,
        };
        let owner = build_transit_owner(&inputs).expect("the default path constructs nothing");
        assert!(
            owner.is_none(),
            "the default configuration must install no transit owner"
        );
    }

    /// The opted-in configuration constructs an **enabled** owner.
    #[test]
    fn the_opted_in_configuration_constructs_an_enabled_owner() {
        let config = opted_in_config();
        let governor = governor();
        let bundle = identity();
        let inputs = TransitCompositionInputs {
            transit: config.transit,
            bandwidth: config.bandwidth,
            governor: Some(&governor),
            delivery: Some(delivery(&bundle)),
            cancellation: Some(CancellationToken::new()),
            identity: Some(&bundle),
        };
        let mut owner = build_transit_owner(&inputs)
            .expect("the opted-in path constructs an owner")
            .expect("the opted-in path installs an owner");
        assert!(
            owner.is_enabled(),
            "the opted-in owner must be enabled, not a disabled shell"
        );
        assert!(
            owner.volume_counters().is_some(),
            "an installed owner owns the service's own volume counters"
        );
    }

    /// The derived policy values stay inside the documented ceilings and
    /// the available bandwidth is the enforced share, not an invented
    /// number.
    #[test]
    fn the_derived_policy_stays_inside_the_documented_ceilings() {
        let config = opted_in_config();
        let governor = governor();
        let derivation =
            derive_admission_policy(&config.transit, &config.bandwidth, Some(&governor))
                .expect("the derivation succeeds");
        assert_eq!(derivation.max_active, 4);
        assert_eq!(derivation.max_pending, MAX_TRANSIT_PENDING);
        assert_eq!(derivation.max_active_per_peer, 2);
        assert_eq!(derivation.max_pending_per_peer, MAX_TRANSIT_PEER_PENDING);
        assert!(derivation.max_active <= MAX_TRANSIT_ACTIVE);
        assert!(derivation.max_pending <= MAX_TRANSIT_PENDING);
        assert!(derivation.max_active_per_peer <= MAX_TRANSIT_PEER_ACTIVE);
        assert!(derivation.max_pending_per_peer <= MAX_TRANSIT_PEER_PENDING);
        assert_eq!(derivation.registry_capacity, derivation.max_active);
        // 1 MiB/s outbound, half of it the operator's share, a quarter of
        // that the transit posture: 128 KiB/s.
        assert_eq!(derivation.available_bandwidth_kbps, 128);
    }

    /// A router without an enforced share has no share to spend, so the
    /// derivation refuses instead of inventing one.
    #[test]
    fn a_missing_governor_is_a_refusal_not_a_substituted_share() {
        let config = opted_in_config();
        let refusal = derive_admission_policy(&config.transit, &config.bandwidth, None)
            .expect_err("the governor is required");
        assert_eq!(
            refusal,
            TransitCompositionError::MissingInput("bandwidth governor")
        );
    }

    /// The hop material carries **this router's own** identity hash.
    #[test]
    fn the_hop_material_carries_this_router_identity_hash() {
        let bundle = identity();
        let material = derive_hop_material(&bundle).expect("the hop material is derived");
        assert_eq!(
            material.hop_identity(),
            &bundle.identity().hash().expect("the router's own hash"),
            "the hop material must carry this router's own identity hash"
        );
    }

    /// The relay outcome names the concrete delivery outcome. A private
    /// mesh has no established session to the next router, so the
    /// delivery reports `NoActiveSession`: the build reply reached no
    /// peer. The test states that in its own words rather than asserting
    /// the wildcard `Dispatched(_)`.
    #[test]
    fn the_build_outcome_names_the_delivery_outcome_explicitly() {
        let config = opted_in_config();
        let governor = governor();
        let bundle = identity();
        let inputs = TransitCompositionInputs {
            transit: config.transit,
            bandwidth: config.bandwidth,
            governor: Some(&governor),
            delivery: Some(delivery(&bundle)),
            cancellation: Some(CancellationToken::new()),
            identity: Some(&bundle),
        };
        let mut owner = build_transit_owner(&inputs)
            .expect("the opted-in path constructs an owner")
            .expect("the opted-in path installs an owner");
        let payload = stbm_payload(&bundle);
        let bytes = encode_standard_stbm(&payload, MESSAGE_ID, NOW_MS + 60_000);
        let outcome = owner
            .handle_inbound(&inbound_with(bytes), NOW_MS, NOW_SECONDS)
            .expect("the inbound is handled");
        let LiveInboundOutcome::Build(LiveBuildOutcome::Dispatched(evidence)) = &outcome else {
            panic!("the peer's transit work must reach the transit data plane: {outcome:?}");
        };
        // The concrete delivery outcome, asserted explicitly. The build
        // reply reached no peer: the private mesh has no established
        // session to the responder.
        assert_eq!(
            evidence.delivery,
            RouterDeliveryOutcome::NoActiveSession,
            "the build reply reached no peer: the private mesh has no established session to the \
             responder"
        );
        assert_eq!(evidence.receive_tunnel, RELAY_TUNNEL);
        assert!(
            !evidence.rejected,
            "the request was admitted, not refused by the admission policy"
        );
        let snapshot = owner.participation().snapshot();
        assert_eq!(
            snapshot.admitted, 1,
            "the participation posture admitted the peer's transit work"
        );
        assert_eq!(
            snapshot.bytes_relayed, 0,
            "an admitted build relays no tunnel bytes yet"
        );
    }

    /// Plan 502: the peer index is reconciled against the **live** session
    /// set.
    ///
    /// The index is the owner's record of which routers it can reach.
    /// Without the reconcile, a forward would decide to relay and then
    /// report that it reached nobody; with a stale entry, a forward would
    /// dispatch to a peer whose session is gone. This row pins both
    /// directions: a departed session is removed, a live one is installed,
    /// and a second pass changes nothing.
    #[test]
    fn the_peer_index_is_reconciled_against_the_live_session_set() {
        let config = opted_in_config();
        let governor = governor();
        let bundle = identity();
        let inputs = TransitCompositionInputs {
            transit: config.transit,
            bandwidth: config.bandwidth,
            governor: Some(&governor),
            delivery: Some(delivery(&bundle)),
            cancellation: Some(CancellationToken::new()),
            identity: Some(&bundle),
        };
        let mut owner = build_transit_owner(&inputs)
            .expect("the opted-in path constructs an owner")
            .expect("the opted-in path installs an owner");
        let departed = Hash::from_bytes([0x41; 32]);
        let live = Hash::from_bytes([0x42; 32]);
        owner
            .install_peer(departed, PeerId::from_hash(departed))
            .expect("the departed peer is indexed");
        assert!(owner.has_peer(&departed), "the peer starts indexed");
        let mut installed = BTreeSet::new();
        installed.insert(departed);
        // The live session set no longer holds `departed`: its session
        // closed. `live` is the only peer with a session now.
        reconcile_transit_peers(&mut owner, &mut installed, &[live], NOW_SECONDS);
        assert!(
            !owner.has_peer(&departed),
            "a peer whose session is gone must not stay indexed: a later forward would dispatch to \
             a stale peer instead of reporting NoActiveSession"
        );
        assert!(
            owner.has_peer(&live),
            "a peer with a live session must be indexed"
        );
        assert_eq!(
            installed.len(),
            1,
            "the installed set must mirror the live session set, not accumulate"
        );
        // A second pass over the same live set changes nothing.
        reconcile_transit_peers(&mut owner, &mut installed, &[live], NOW_SECONDS);
        assert!(owner.has_peer(&live), "the live peer stays indexed");
        assert_eq!(installed.len(), 1, "the reconcile is idempotent");
    }
}
