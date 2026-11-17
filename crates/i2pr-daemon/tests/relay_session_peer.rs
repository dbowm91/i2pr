//! Plan 534 — the router names the peer it reached through a relay.
//!
//! The lane drives the **public** daemon API only: the driver, the
//! coordinator, the peer-profile store, the delivery seam, the local
//! observation table and the establishment's own recording seam. It reaches
//! into no private module and adds no accessor of its own.
//!
//! Properties of the lane:
//!
//! - **Fixed seeds.** Every random value the lane's own code draws comes
//!   from a seeded ChaCha8 stream; the identities, the transport keys and
//!   the build crypto are all reproducible.
//! - **An explicit `now_ms`.** Every decision the lane asserts is taken on
//!   the fixed [`NOW_MS`]: the coordinator's clock, the driver's clock, the
//!   peer-profile store's clock, the I2NP expiration and the inbound
//!   dispatch clock. The lane's outcomes therefore do not depend on the
//!   moment it runs.
//!
//!   The one exception is the controlled peer's **RouterInfo publication
//!   date**, which the lane reads from the wall clock — see
//!   [`published_ms`]. The SSU2 transport refuses a peer whose RouterInfo is
//!   older than `ESTABLISHMENT_ROUTER_INFO_MAX_AGE_SECONDS` (two hours)
//!   *against the transport's own clock*, and refuses a handshake payload
//!   whose timestamp is more than two minutes away from it. A constant
//!   publication date therefore cannot satisfy a real handshake.
//! - **No environment variable, no `#[ignore]`, no stock peer.** The only
//!   peer is the controlled loopback mesh the lane builds itself.
//! - **No unbounded waiting.** Every wait is bounded by a hard deadline; the
//!   lane never sleeps to make an assertion pass, it waits for a state and
//!   fails when the state does not arrive.
//!
//! The controlled mesh is the Plan 185 shape: two real SSU2 services on
//! loopback, one real dial, and a responder that applies the canonical
//! [`MessageHopProcessor`] to the inbound short tunnel build. The responder
//! answers with the peer's **identity** encryption key
//! (`bundle.encryption_key().secret_bytes()`) — the key the production
//! `peer_static_encryption_key` seam derives — so the lane proves the
//! production key derivation and not a self-consistent transport key.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::Duration;

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_netdb::{
    MIN_PEER_VERSION_FOR_TUNNEL_BUILD, ProfileState, ProfileStoreConfig, RouterHash,
    RouterInfoStore, RouterInfoStoreConfig, ValidatedRouterInfo,
};
use i2pr_proto::{ApiVersion, Date, Hash, I2npBody, I2npMessage, Mapping, RouterAddress};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, Ssu2IdentityMaterial, Ssu2InboundI2np,
};
use i2pr_transport::{LinkId, MAX_I2NP_MESSAGE_BYTES, PeerId};
use i2pr_tunnel::bridge::ShortBuildI2npBridge;
use i2pr_tunnel::build_crypto::{BuildCryptography, EciesX25519BuildCryptography};
use i2pr_tunnel::config::ExploratoryPoolConfig;
use i2pr_tunnel::multirecord::MessageHopProcessor;
use i2pr_tunnel::short_record::{ShortRequestRecord, ShortResponseCode};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

use i2pr_daemon::config::{Config, Ssu2Config};
use i2pr_daemon::exploratory_build::{BuildCoordinatorOutcome, ExploratoryBuildCoordinator};
use i2pr_daemon::exploratory_driver::{
    ExploratoryDriver, ExploratoryDriverAction, PeerBuildIdentityResolver, ProfileStorePeerResolver,
};
use i2pr_daemon::i2pcontrol_inspection::{InspectionHandles, ServiceEndpoint};
use i2pr_daemon::netdb_participation::{PeerParticipationDriver, PeerParticipationHandle};
use i2pr_daemon::relay_establishment_task::record_established_peer;
use i2pr_daemon::relay_session_peer::{
    MAX_RELAY_SESSION_PEERS, RELAY_SESSION_PEER_TTL_MS, RelaySessionPeerOwner,
    RelaySessionPeerRefusal, record_relay_session_peer,
};
use i2pr_daemon::router_i2np::{
    RouterDeliveryRequest, RouterDeliveryService, RouterI2npKind, RouterI2npOutcome,
    Ssu2DaemonHandle, Ssu2DaemonService, dispatch_router_i2np,
};

/// The lane's explicit clock, in milliseconds since the Unix epoch.
///
/// Every timestamp the lane's own decisions read or write comes from this
/// constant: the coordinator's clock, the driver's clock, the peer-profile
/// store's clock, the I2NP expiration and the inbound dispatch clock. A run
/// is therefore reproducible, and the I2NP expiration checks are evaluated
/// against the same instant the coordinator used to build the message.
const NOW_MS: u64 = 1_700_000_000_000;

/// The bounded dial timeout.
const DIAL_TIMEOUT: Duration = Duration::from_secs(20);

/// The bounded wait for a mesh state.
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);

/// The poll interval of a bounded wait.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// The controlled peer's RouterInfo publication date, in milliseconds.
///
/// The SSU2 transport's establishment freshness policy is relative to the
/// transport's own clock, so the publication date has to be current. The
/// value is used for the RouterInfo's publication date and for the
/// validation context that must agree with it, and for nothing else.
fn published_ms() -> u64 {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the system clock is after the epoch")
            .as_millis(),
    )
    .expect("the epoch milliseconds fit in a u64")
}

/// The seed of the local router's identity material.
const SEED_LOCAL: u64 = 0x5050_0001;

/// The seed of the controlled peer's identity material.
const SEED_PEER: u64 = 0x5050_0002;

/// The seed of the below-floor peer's identity material.
const SEED_BELOW_FLOOR: u64 = 0x5050_0003;

/// The seed of the driver's build crypto.
const SEED_DRIVER: u64 = 0x5050_0004;

/// The seed of the controlled peer's build crypto.
const SEED_RESPONDER: u64 = 0x5050_0005;

/// The published API version the tunnel-build floor demands.
///
/// The lane never restates the floor as a literal, so it cannot drift from
/// the product.
fn at_floor_build_version() -> String {
    MIN_PEER_VERSION_FOR_TUNNEL_BUILD.to_string()
}

/// A published API version exactly one patch below the tunnel-build floor.
///
/// The version is derived from the product's own floor rather than written
/// as a literal, so the lane still refuses a below-floor peer when the
/// product's floor moves.
fn below_floor_build_version() -> String {
    let floor = MIN_PEER_VERSION_FOR_TUNNEL_BUILD;
    let below = ApiVersion::new(
        floor.major(),
        floor.minor(),
        floor.patch().saturating_sub(1),
    );
    assert!(
        below < floor,
        "the derived version must be strictly below the floor"
    );
    below.to_string()
}

/// One controlled router's identity material.
struct LocalKeys {
    /// The router identity bundle. The bundle owns the **identity**
    /// encryption key the production key derivation uses.
    bundle: RouterIdentityBundle,
    /// The router hash.
    hash: Hash,
    /// The SSU2 static secret — the transport key, never the build key.
    static_bytes: [u8; 32],
    /// The SSU2 static public key.
    static_public: i2pr_runtime::Ssu2PublicKey,
    /// The SSU2 intro key.
    intro: i2pr_runtime::IntroKey,
    /// The signed RouterInfo.
    router_info: i2pr_proto::RouterInfo,
    /// The encoded RouterInfo.
    router_info_bytes: Vec<u8>,
}

/// Builds one controlled router's identity material from a fixed seed.
fn make_local_keys(seed: u64, published_version: &str) -> LocalKeys {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
    let hash = bundle.identity().hash().expect("hash");
    let static_secret = X25519PrivateKey::generate(&mut rng).expect("static");
    let static_bytes = *static_secret.secret_bytes();
    let static_public =
        i2pr_runtime::Ssu2PublicKey::new(static_secret.public_bytes()).expect("static public");
    let mut intro_bytes = [0_u8; 32];
    loop {
        rng.fill_bytes(&mut intro_bytes);
        if intro_bytes.iter().any(|byte| *byte != 0) {
            break;
        }
    }
    let intro = i2pr_runtime::IntroKey::new(intro_bytes);
    let options = Mapping::from_entries(vec![
        ("host".to_string(), "127.0.0.1".to_string()),
        ("port".to_string(), "44011".to_string()),
        ("v".to_string(), "2".to_string()),
        (
            "s".to_string(),
            i2pr_netdb::encode(&static_secret.public_bytes()).expect("the static key encodes"),
        ),
        (
            "i".to_string(),
            i2pr_netdb::encode(&intro_bytes).expect("the intro key encodes"),
        ),
    ])
    .expect("options");
    let address = RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .expect("address");
    let mut ri_options = Mapping::builder();
    // The peer publishes the API version the tunnel-build floor demands,
    // so the version-gated selection accepts it.
    ri_options
        .insert("router.version".to_string(), published_version.to_string())
        .expect("router.version");
    ri_options
        .insert("netId".to_string(), "2".to_string())
        .expect("netId");
    let router_info = bundle
        .sign_router_info(
            Date::from_millis(published_ms()),
            vec![address],
            Vec::new(),
            ri_options.build().expect("router info options"),
        )
        .expect("sign");
    let router_info_bytes = router_info
        .encode_to_vec(i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .expect("encode");
    LocalKeys {
        bundle,
        hash,
        static_bytes,
        static_public,
        intro,
        router_info,
        router_info_bytes,
    }
}

/// Validates one controlled router's RouterInfo against the lane's clock.
fn validated_peer(keys: &LocalKeys) -> ValidatedRouterInfo {
    ValidatedRouterInfo::from_router_info(
        keys.router_info.clone(),
        None,
        i2pr_netdb::ValidationContext::new(Date::from_millis(published_ms())),
    )
    .expect("the peer RouterInfo must validate")
}

/// Builds a peer-profile handle that holds the supplied validated peers.
///
/// The store is **adopted**, so the selection and the resolver read the very
/// record the selection was drawn from and can never disagree about which
/// peer exists.
fn profile_handle(peers: Vec<ValidatedRouterInfo>) -> PeerParticipationHandle {
    let mut store = RouterInfoStore::with_config(RouterInfoStoreConfig::new(64, 1 << 20));
    for peer in peers {
        store.insert(peer);
    }
    let participation =
        PeerParticipationDriver::adopt(ProfileStoreConfig::default(), store, NOW_MS);
    PeerParticipationHandle::new(participation)
}

/// Returns the reachability state the profile store holds for one peer.
fn profile_state(profiles: &PeerParticipationHandle, key: &i2pr_netdb::RouterHash) -> ProfileState {
    profiles
        .with(|participation| {
            participation
                .store()
                .profile(key)
                .map(|profile| profile.state())
        })
        .expect("the store lock")
        .unwrap_or(ProfileState::NeverTried)
}

/// Returns the number of receipts the peer-profile store holds.
/// The daemon's SSU2 configuration, parsed from a strict profile.
fn daemon_ssu2_config() -> Ssu2Config {
    let text = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n";
    Config::parse(text).expect("strict profile").ssu2
}

/// The controlled peer that answers a short tunnel build.
///
/// Plan 505: the hop is processed with the peer's **identity** encryption
/// key — the key the production `peer_static_encryption_key` seam publishes
/// as the peer's public static encryption key.
struct BuildResponder {
    /// The peer's identity encryption secret.
    identity_secret: [u8; 32],
    /// The peer's router hash.
    hash: Hash,
}

impl BuildResponder {
    /// Returns the message id the reply must carry.
    ///
    /// The reply is correlated by the *inner* message id the creator
    /// reserved, which the request record carries as its next message id.
    /// The responder therefore opens its own record and reads the id,
    /// instead of guessing one.
    fn reply_message_id(&self, payload: &[u8]) -> Option<u32> {
        let count = *payload.first()?;
        let records = payload.get(1..)?;
        let crypto = EciesX25519BuildCryptography::new();
        for index in 0..usize::from(count) {
            let start = index.checked_mul(i2pr_proto::SHORT_BUILD_RECORD_SIZE)?;
            let record =
                records.get(start..start.checked_add(i2pr_proto::SHORT_BUILD_RECORD_SIZE)?)?;
            if record[..i2pr_tunnel::build_crypto::HASH_PREFIX_LEN]
                != self.hash.as_bytes()[..i2pr_tunnel::build_crypto::HASH_PREFIX_LEN]
            {
                continue;
            }
            let opened = crypto
                .open_short_request(record, &self.identity_secret, self.hash.as_bytes())
                .ok()?;
            let decoded = ShortRequestRecord::decode(opened.plaintext.as_ref()).ok()?;
            return Some(decoded.next_message_id());
        }
        None
    }

    /// Processes one inbound build and returns the reply wire.
    fn process_build(&self, request_payload: &[u8]) -> Option<Vec<u8>> {
        let message_id = self.reply_message_id(request_payload)?;
        let crypto = EciesX25519BuildCryptography::new();
        let mut rng = ChaCha8Rng::seed_from_u64(SEED_RESPONDER);
        let (reply_payload, _result) = MessageHopProcessor::process_hop(
            &crypto,
            request_payload,
            &self.identity_secret,
            &self.hash,
            ShortResponseCode::Accepted,
            &mut rng,
        )
        .ok()?;
        let count = *reply_payload.first()?;
        let records = reply_payload.get(1..)?.to_vec();
        let deferred = i2pr_proto::DeferredBuildRecords::new(
            count,
            i2pr_proto::SHORT_BUILD_RECORD_SIZE,
            records,
        )
        .ok()?;
        let body = I2npBody::OutboundTunnelBuildReply(deferred);
        // The reply's expiration is derived from the lane's own clock, so
        // the receiver's expiry check reads the same instant the sender
        // used.
        let expiration_secs = u32::try_from(NOW_MS / 1000)
            .unwrap_or(u32::MAX)
            .saturating_add(60);
        let message =
            I2npMessage::new_short_transport(message_id, expiration_secs, body).expect("OTBRM");
        message
            .encode_short_transport_to_vec(MAX_I2NP_MESSAGE_BYTES)
            .ok()
    }
}

/// The controlled mesh: two real SSU2 services, one real session and the
/// responder that answers the inbound build.
struct Mesh {
    /// The local router's daemon handle.
    handle_a: Ssu2DaemonHandle,
    /// The local router's identity material.
    keys_a: LocalKeys,
    /// The controlled peer's identity material.
    keys_b: LocalKeys,
    /// The local scope.
    scope_a: ChildScope,
    /// The peer's scope.
    scope_b: ChildScope,
    /// The responder's cancellation token.
    responder_token: CancellationToken,
    /// The responder task.
    responder_task: tokio::task::JoinHandle<()>,
    /// The controlled peer's bound endpoint.
    addr_b: std::net::SocketAddr,
}

impl Mesh {
    /// Builds the mesh and establishes the one session the lane needs.
    async fn start(published_version: &str) -> Mesh {
        let keys_a = make_local_keys(SEED_LOCAL, &at_floor_build_version());
        let keys_b = make_local_keys(SEED_PEER, published_version);
        let config = daemon_ssu2_config();
        let service_a = Ssu2DaemonService::new(
            &config,
            Ssu2IdentityMaterial {
                router_hash: keys_a.hash,
                static_secret_bytes: keys_a.static_bytes,
                intro_key: keys_a.intro,
                router_info: keys_a.router_info_bytes.clone(),
            },
        )
        .expect("daemon service a");
        let service_b = Ssu2DaemonService::new(
            &config,
            Ssu2IdentityMaterial {
                router_hash: keys_b.hash,
                static_secret_bytes: keys_b.static_bytes,
                intro_key: keys_b.intro,
                router_info: keys_b.router_info_bytes.clone(),
            },
        )
        .expect("daemon service b");
        let token_a = CancellationToken::new();
        let token_b = CancellationToken::new();
        let scope_a = ChildScope::for_test(&token_a, ChildFailurePolicy::FailParent);
        let scope_b = ChildScope::for_test(&token_b, ChildFailurePolicy::FailParent);
        let handle_a = service_a
            .start(
                &scope_a,
                &Ssu2Config {
                    port: 0,
                    ..config.clone()
                },
            )
            .await
            .expect("bind a");
        let handle_b = service_b
            .start(
                &scope_b,
                &Ssu2Config {
                    port: 0,
                    ..config.clone()
                },
            )
            .await
            .expect("bind b");
        let addr_b = handle_b.local_v4().expect("b addr");
        let target = i2pr_runtime::Ssu2DialTarget::new(
            PeerId::from_hash(keys_b.hash),
            keys_b.hash,
            addr_b,
            keys_b.static_public,
            keys_b.intro,
        )
        .expect("dial target");
        let _established = handle_a
            .service()
            .dial_ssu2(target, DIAL_TIMEOUT, &CancellationToken::new())
            .await
            .expect("dial establishes");
        wait_for_active(&handle_a, 1).await;
        wait_for_active(&handle_b, 1).await;

        // The responder answers with the peer's **identity** encryption key.
        let responder = BuildResponder {
            identity_secret: *keys_b.bundle.encryption_key().secret_bytes(),
            hash: keys_b.hash,
        };
        let responder_delivery = handle_b.delivery().clone();
        let responder_token = CancellationToken::new();
        let responder_task = tokio::spawn(pump_responder_loop(
            handle_b,
            responder_delivery,
            responder,
            responder_token.clone(),
        ));
        Mesh {
            handle_a,
            keys_a,
            keys_b,
            scope_a,
            scope_b,
            responder_token,
            responder_task,
            addr_b,
        }
    }

    /// The local router's delivery seam.
    fn delivery(&self) -> RouterDeliveryService {
        self.handle_a.delivery().clone()
    }

    /// Drains the local router's inbound path until the coordinator
    /// produces an outcome.
    async fn drain_outcomes(
        &mut self,
        coordinator: &mut ExploratoryBuildCoordinator,
    ) -> Vec<BuildCoordinatorOutcome> {
        drain_build_outcomes(&mut self.handle_a, coordinator, self.keys_b.hash).await
    }

    /// Stops the responder and both daemons.
    async fn shutdown(self) {
        self.responder_token
            .cancel(i2pr_runtime::CancellationReason::OperatorRequest);
        let _ = self.responder_task.await;
        let _ = self.scope_a.shutdown().await;
        let _ = self.scope_b.shutdown().await;
    }
}

/// Waits, under a hard deadline, for the daemon's active-session count.
async fn wait_for_active(handle: &Ssu2DaemonHandle, expected: usize) {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let snapshot = handle.snapshot();
        if snapshot.active_sessions == expected {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "active sessions did not reach {expected}: {:?} {:?} {:?}",
            snapshot.active_sessions,
            snapshot.pending_outbound,
            snapshot.pending_inbound
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Returns the next inbound message, or `None` when the bounded wait ends.
async fn next_inbound_with_timeout(
    handle: &mut Ssu2DaemonHandle,
    timeout: Duration,
) -> Option<Ssu2InboundI2np> {
    tokio::time::timeout(timeout, handle.next_inbound())
        .await
        .unwrap_or_default()
}

/// The controlled peer's inbound pump.
async fn pump_responder_loop(
    mut handle: Ssu2DaemonHandle,
    delivery: RouterDeliveryService,
    responder: BuildResponder,
    shutdown: CancellationToken,
) {
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => return,
            inbound = handle.next_inbound() => {
                let Some(inbound) = inbound else {
                    return;
                };
                let dispatch = match dispatch_router_i2np(&inbound, NOW_MS) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                if !matches!(
                    dispatch,
                    RouterI2npOutcome::TunnelBuildReserved {
                        kind: RouterI2npKind::ShortTunnelBuild,
                        ..
                    }
                ) {
                    continue;
                }
                let message =
                    match I2npMessage::decode_short_transport(&inbound.bytes, MAX_I2NP_MESSAGE_BYTES)
                    {
                        Ok(message) => message,
                        Err(_) => continue,
                    };
                let body = match message.body() {
                    I2npBody::ShortTunnelBuild(records) => records,
                    _ => continue,
                };
                let expected = usize::from(body.count())
                    .saturating_mul(usize::from(body.record_size()));
                if body.records().len() != expected {
                    continue;
                }
                let mut payload = Vec::with_capacity(1 + expected);
                payload.push(body.count());
                payload.extend_from_slice(body.records());
                let Some(reply_wire) = responder.process_build(&payload) else {
                    continue;
                };
                let request = match RouterDeliveryRequest::new(
                    inbound.peer,
                    reply_wire,
                    Duration::from_secs(5),
                ) {
                    Ok(request) => request,
                    Err(_) => continue,
                };
                let _ = delivery.deliver(request, &CancellationToken::new());
            }
        }
    }
}

/// Drains the inbound path until the coordinator produces an outcome.
async fn drain_build_outcomes(
    handle: &mut Ssu2DaemonHandle,
    coordinator: &mut ExploratoryBuildCoordinator,
    peer_hash: Hash,
) -> Vec<BuildCoordinatorOutcome> {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        let Some(inbound) = next_inbound_with_timeout(handle, POLL_INTERVAL * 4).await else {
            continue;
        };
        let dispatch = match dispatch_router_i2np(&inbound, NOW_MS) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if !matches!(
            dispatch,
            RouterI2npOutcome::TunnelBuildReserved {
                kind: RouterI2npKind::OutboundTunnelBuildReply,
                ..
            }
        ) {
            continue;
        }
        let wrapped = Ssu2InboundI2np {
            link_id: LinkId::new(1).expect("link"),
            peer: PeerId::from_hash(peer_hash),
            bytes: inbound.bytes.clone(),
        };
        let routed = coordinator
            .route_inbound_i2np(&wrapped, NOW_MS)
            .expect("dispatcher outcome");
        if !routed.coordinator.is_empty() {
            return routed.coordinator;
        }
    }
    Vec::new()
}

/// Builds a coordinator whose clock is the lane's explicit clock.
fn new_coordinator() -> ExploratoryBuildCoordinator {
    let mut coordinator = ExploratoryBuildCoordinator::new(ExploratoryPoolConfig::balanced());
    coordinator.advance_time(NOW_MS);
    coordinator
}

/// Builds a delivery seam that owns a real, **never started** SSU2 runtime.
///
/// The seam is real, so a refusal cannot be an artefact of a stub: the
/// runtime simply holds no session, so the delivery is refused exactly as it
/// is on a router whose peer is not reachable.
fn unreachable_delivery_service(keys: &LocalKeys) -> RouterDeliveryService {
    let runtime = i2pr_runtime::Ssu2RuntimeService::new(
        i2pr_daemon::router_i2np::ssu2_runtime_config_from(&daemon_ssu2_config())
            .expect("runtime config"),
        Ssu2IdentityMaterial {
            router_hash: keys.hash,
            static_secret_bytes: keys.static_bytes,
            intro_key: keys.intro,
            router_info: keys.router_info_bytes.clone(),
        },
    )
    .expect("runtime service");
    RouterDeliveryService::new(runtime)
}

/// The driver under test, wired to a peer-profile handle and — when the
/// router has asked for the requester role — to the local observation
/// table.
fn driver_over(
    profiles: &PeerParticipationHandle,
    enabled: bool,
    relay_sessions: Option<i2pr_daemon::relay_session_peer::RelaySessionPeerHandle>,
) -> ExploratoryDriver {
    let resolver: Arc<dyn PeerBuildIdentityResolver + Send + Sync> =
        Arc::new(ProfileStorePeerResolver::new(profiles.clone()));
    ExploratoryDriver::with_relay_sessions(enabled, profiles.clone(), resolver, relay_sessions)
}

/// The lane's rows, one obligation each:
///
/// - the route **names** the peer the router reached through a relay, and
///   the build toward it installs through the existing coordinator;
/// - a named target is gated by the **same** API-version floor every drawn
///   candidate passes, and a refusal is counted under the same reasons;
/// - a named target the router does not hold is a **local** condition: it
///   is counted as such and never as the peer's missing version;
/// - the observation is **consumed** by the route that used it, and it
///   **expires**;
/// - the observation table is **bounded**, and a full table is refused
///   under its own reason;
/// - every refusal is typed and counted, and writes nothing into a peer's
///   profile;
/// - the counts reach the operator through the inspection seam, and the
///   seam exposes the counts only.
///
/// The driver's own version-gated selection, its single build path and its
/// refusal accounting are Plan 505's obligations and are proven by Plan
/// 505's own lane; this lane adds only the named target and the local
/// observation table on top of them.
///
/// Plan 534's obligation, in one row: the peer this router reached through
/// a relay is observed as a **local** fact, the route names it, and the
/// build toward it installs.
///
/// The observation is recorded through the establishment's **own** recording
/// seam — the same function the establishment's task calls when the
/// handshake succeeds — so the lane proves the production seam and not a
/// stand-in.
#[tokio::test]
async fn the_route_names_the_relayed_peer_and_the_build_installs() {
    let mesh = Mesh::start(&at_floor_build_version()).await;
    let peer = validated_peer(&mesh.keys_b);
    let peer_key = peer.key();
    let profiles = profile_handle(vec![peer]);
    let relay_sessions = RelaySessionPeerOwner::new(true).into_handle();
    let mut driver = driver_over(&profiles, true, Some(relay_sessions.clone()));
    let mut coordinator = new_coordinator();
    let delivery = mesh.delivery();
    let bridge = ShortBuildI2npBridge::new();
    let mut rng = ChaCha8Rng::seed_from_u64(SEED_DRIVER);

    // The establishment's own seam records the observation. The peer is the
    // one the router reached: the mesh's second router.
    let target = i2pr_runtime::Ssu2DialTarget::new(
        PeerId::from_hash(mesh.keys_b.hash),
        mesh.keys_b.hash,
        mesh.addr_b,
        mesh.keys_b.static_public,
        mesh.keys_b.intro,
    )
    .expect("the dial target");
    record_established_peer(&relay_sessions, target, NOW_MS).expect("the observation is recorded");
    let counters = i2pr_daemon::relay_session_peer::relay_session_peer_counters(&relay_sessions)
        .expect("the counters are readable");
    assert_eq!(counters.observations_recorded, 1);
    assert_eq!(counters.observations_taken, 0);

    // The observation is a **local** fact: the peer's profile carries no
    // corroborating class and the router's reachability is untouched.
    assert_eq!(
        profile_state(&profiles, &peer_key),
        ProfileState::NeverTried,
        "the observation must not write into the peer's profile"
    );

    // The route names the observed peer and submits the build toward it.
    let action = driver.drive(
        NOW_MS,
        &mut coordinator,
        &delivery,
        &bridge,
        Some(mesh.keys_a.hash),
        &mut rng,
    );
    assert_eq!(action, ExploratoryDriverAction::SubmittedOutbound);
    assert_eq!(
        driver.counters().named_targets_used,
        1,
        "the route must name the peer the router reached through the relay"
    );
    assert_eq!(driver.counters().named_targets_refused, 0);
    assert_eq!(driver.counters().attempts, 1);
    assert_eq!(coordinator.pending_len(), 1);

    // The route's **selection** writes nothing into the peer's profile: the
    // observation is a local fact, and naming the peer in a build corroborates
    // nothing about it. The assertion follows the drive, because that is
    // where a write would happen.
    assert_eq!(
        profile_state(&profiles, &peer_key),
        ProfileState::NeverTried,
        "the route's selection must write nothing into the relayed peer's profile"
    );

    // The real inbound path classifies the reply and the coordinator
    // installs the tunnel.
    let mut mesh = mesh;
    let outcomes = mesh.drain_outcomes(&mut coordinator).await;
    assert!(
        outcomes
            .iter()
            .any(|outcome| matches!(outcome, BuildCoordinatorOutcome::Installed { .. })),
        "the relayed build must install"
    );
    assert_eq!(driver.observe_outcomes(&outcomes), 1);
    assert_eq!(coordinator.outbound_pool_len(), 1);
    assert_eq!(driver.counters().installed, 1);

    // The observation was **consumed** by the route that used it.
    let counters = i2pr_daemon::relay_session_peer::relay_session_peer_counters(&relay_sessions)
        .expect("the counters are readable");
    assert_eq!(
        counters.observations_taken, 1,
        "the observation must be consumed by the route that used it"
    );
    mesh.shutdown().await;
}

/// The named target passes the **same** API-version floor every drawn
/// candidate passes. A peer below the floor is refused and counted, and no
/// build is submitted.
#[tokio::test]
async fn a_named_target_below_the_version_floor_is_refused_and_counted() {
    let below = make_local_keys(SEED_BELOW_FLOOR, &below_floor_build_version());
    let below_validated = validated_peer(&below);
    let profiles = profile_handle(vec![below_validated]);
    let relay_sessions = RelaySessionPeerOwner::new(true).into_handle();
    let mut driver = driver_over(&profiles, true, Some(relay_sessions.clone()));

    // The observation names the peer below the floor.
    let target = i2pr_runtime::Ssu2DialTarget::new(
        PeerId::from_hash(below.hash),
        below.hash,
        "127.0.0.1:1".parse().expect("an endpoint"),
        below.static_public,
        below.intro,
    )
    .expect("the dial target");
    record_established_peer(&relay_sessions, target, NOW_MS).expect("the observation is recorded");

    let local = make_local_keys(SEED_LOCAL, &at_floor_build_version());
    let mut coordinator = new_coordinator();
    let delivery = unreachable_delivery_service(&local);
    let bridge = ShortBuildI2npBridge::new();
    let mut rng = ChaCha8Rng::seed_from_u64(SEED_DRIVER);
    let _ = driver.drive(
        NOW_MS,
        &mut coordinator,
        &delivery,
        &bridge,
        Some(local.hash),
        &mut rng,
    );

    // The named target is refused under the driver's own version-refusal
    // counter, and the refusal is counted under its own reason.
    assert_eq!(
        driver.counters().named_targets_refused,
        1,
        "the below-floor named target must be refused"
    );
    assert_eq!(driver.counters().named_targets_used, 0);
    assert_eq!(
        driver.counters().refusals_below_floor,
        2,
        "the named target and the drawn candidate must both be refused"
    );
    assert_eq!(
        driver.counters().attempts,
        0,
        "no build may be submitted toward a peer below the floor"
    );
    assert_eq!(driver.counters().no_eligible_peer, 1);
    assert_eq!(coordinator.pending_len(), 0);
}

/// A named target the router does not hold is a **local** condition: it is
/// refused under the local reason and never under the peer's missing
/// version, and no build is submitted.
#[tokio::test]
async fn a_named_target_the_store_does_not_hold_is_refused_as_a_local_condition() {
    // The store holds one peer, below the version floor, so the selection
    // refuses it and no build is submitted. The observation names a
    // **different** peer, one the store does not hold.
    let known = make_local_keys(SEED_PEER, &below_floor_build_version());
    let known_validated = validated_peer(&known);
    let known_key = known_validated.key();
    let profiles = profile_handle(vec![known_validated]);
    let relay_sessions = RelaySessionPeerOwner::new(true).into_handle();
    let mut driver = driver_over(&profiles, true, Some(relay_sessions.clone()));

    let absent = make_local_keys(SEED_BELOW_FLOOR, &at_floor_build_version());
    let target = i2pr_runtime::Ssu2DialTarget::new(
        PeerId::from_hash(absent.hash),
        absent.hash,
        "127.0.0.1:1".parse().expect("an endpoint"),
        absent.static_public,
        absent.intro,
    )
    .expect("the dial target");
    record_established_peer(&relay_sessions, target, NOW_MS).expect("the observation is recorded");

    let local = make_local_keys(SEED_LOCAL, &at_floor_build_version());
    let mut coordinator = new_coordinator();
    let delivery = unreachable_delivery_service(&local);
    let bridge = ShortBuildI2npBridge::new();
    let mut rng = ChaCha8Rng::seed_from_u64(SEED_DRIVER);
    let _ = driver.drive(
        NOW_MS,
        &mut coordinator,
        &delivery,
        &bridge,
        Some(local.hash),
        &mut rng,
    );

    // The local condition is counted as a local condition: the router does
    // not hold the peer's record, which is **not** the peer's missing
    // version.
    assert_eq!(
        driver.counters().peer_material_absent,
        1,
        "the absent record must be counted as a local condition"
    );
    assert_eq!(
        driver.counters().refusals_unknown_version,
        0,
        "a local condition must never be written as the peer's missing version"
    );
    assert_eq!(driver.counters().named_targets_refused, 1);
    assert_eq!(driver.counters().named_targets_used, 0);
    assert_eq!(
        driver.counters().attempts,
        0,
        "no build may be submitted toward a peer the store does not hold"
    );
    // The known peer's profile is untouched by the refusal.
    assert_eq!(
        profile_state(&profiles, &known_key),
        ProfileState::NeverTried,
        "a refusal must write nothing into a peer's profile"
    );
}

/// The observation expires: a session that died while the router was not
/// looking must not be built toward, and the expiry is counted.
#[test]
fn the_observation_expires_and_the_expiry_is_counted() {
    let mut owner = RelaySessionPeerOwner::new(true);
    let peer = RouterHash::from_hash(Hash::from_bytes([7u8; 32]));
    owner
        .record(peer, NOW_MS)
        .expect("the observation is recorded");
    assert_eq!(owner.len(), 1);
    // Just inside the window the observation is still live.
    assert_eq!(
        owner.take(NOW_MS + RELAY_SESSION_PEER_TTL_MS - 1),
        Some(peer)
    );
    owner
        .record(peer, NOW_MS)
        .expect("the observation is recorded again");
    // Past the window it is dropped, and the drop is counted.
    assert_eq!(owner.take(NOW_MS + RELAY_SESSION_PEER_TTL_MS + 1), None);
    assert_eq!(owner.counters().observations_expired, 1);
    assert_eq!(owner.counters().observations_taken, 1);
}

/// The observation table is bounded, and a full table is refused under its
/// own reason rather than growing.
#[test]
fn the_observation_table_is_bounded_and_a_full_table_is_refused() {
    let mut owner = RelaySessionPeerOwner::new(true);
    for index in 0..MAX_RELAY_SESSION_PEERS {
        let peer = RouterHash::from_hash(Hash::from_bytes([index as u8; 32]));
        owner
            .record(peer, NOW_MS)
            .expect("the observation is recorded");
    }
    assert_eq!(owner.len(), MAX_RELAY_SESSION_PEERS);
    let overflow = RouterHash::from_hash(Hash::from_bytes([0xfeu8; 32]));
    assert_eq!(
        owner.record(overflow, NOW_MS),
        Err(RelaySessionPeerRefusal::TableFull),
        "a full table must be refused under its own reason"
    );
    assert_eq!(owner.counters().refusals_table_full, 1);
    assert_eq!(owner.len(), MAX_RELAY_SESSION_PEERS);

    // The same peer again refreshes the observation rather than growing the
    // table.
    let first = RouterHash::from_hash(Hash::from_bytes([0u8; 32]));
    owner
        .record(first, NOW_MS + 1)
        .expect("the observation is refreshed");
    assert_eq!(owner.len(), MAX_RELAY_SESSION_PEERS);
}

/// Every refusal is typed and counted, and none of them writes into a
/// peer's profile.
#[test]
fn every_refusal_is_typed_and_counted() {
    let mut owner = RelaySessionPeerOwner::new(false);
    let peer = RouterHash::from_hash(Hash::from_bytes([3u8; 32]));
    assert_eq!(
        owner.record(peer, NOW_MS),
        Err(RelaySessionPeerRefusal::Disabled),
        "a disabled owner must refuse every observation"
    );
    assert_eq!(owner.counters().refusals_disabled, 1);
    assert_eq!(owner.counters().observations_recorded, 0);
    assert!(owner.is_empty());
    // The reason keys are stable labels and carry no peer identity.
    assert_eq!(RelaySessionPeerRefusal::Disabled.reason(), "disabled");
    assert_eq!(RelaySessionPeerRefusal::TableFull.reason(), "table-full");
}

/// The counts reach the operator through the inspection seam, and the seam
/// exposes the **counts only**.
///
/// An unpublished seam is a **gap**, never a fabricated zero: the snapshot is
/// `None` until the composition root publishes the table.
#[test]
fn the_counts_reach_the_operator_through_the_inspection_seam() {
    let inspection = InspectionHandles::new(
        2,
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        ServiceEndpoint {
            enabled: false,
            bind: None,
        },
        Vec::new(),
    );
    assert!(
        inspection.relay_session_peer_counters_snapshot().is_none(),
        "an unpublished table must be a gap, never a fabricated zero"
    );

    let owner = RelaySessionPeerOwner::new(true);
    let handle = owner.into_handle();
    let peer = RouterHash::from_hash(Hash::from_bytes([11u8; 32]));
    record_relay_session_peer(&handle, peer, NOW_MS).expect("the observation is recorded");
    inspection.publish_relay_session_peers(handle.clone());

    let counters = inspection
        .relay_session_peer_counters_snapshot()
        .expect("the published table is readable");
    assert_eq!(counters.observations_recorded, 1);
    assert_eq!(counters.observations_taken, 0);
    assert_eq!(counters.observations_expired, 0);
    assert_eq!(counters.refusals_disabled, 0);
    assert_eq!(counters.refusals_table_full, 0);
}
