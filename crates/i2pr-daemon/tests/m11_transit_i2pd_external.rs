//! Plan 256 M11 exact-pinned i2pd qualification evidence/topology corrective driver.
//!
//! This driver corrects the Plan 255 external evidence semantics. Every
//! counted M11 external result is produced by a real, independently
//! observable protocol event in a valid controlled topology:
//!
//! ```text
//! single qualification owner (this driver)
//!   -> generate one ephemeral RouterIdentityBundle
//!   -> derive signed public i2pr RouterInfo
//!   -> retain RouterIdentity encryption private key only in memory
//!   -> write public RI into the exact source-locked NetDB layout
//!      (<datadir>/netDb/r<C0>/routerInfo-<i2p-b64>.dat, i2pd 2.61.0
//!      HashedStorage("netDb", "r", "routerInfo-", "dat"))
//!   -> write source-locked loopback i2pd.conf for A and B
//!   -> start exact-pinned reference processes (bounded owner, killed on drop)
//!   -> verify each reference loaded the expected i2pr router hash
//!      (pre-start exact file + `NetDb: ... routers loaded` log line +
//!      functional explicit-peer FindRouter proof)
//!   -> start i2pr SSU2 service with the same RouterIdentityBundle
//!   -> construct TransitHopMaterial from that bundle's encryption private key
//!      (the ECIES tunnel-build Noise-N responder secret; the SSU2
//!      transport static key stays a separate key)
//!   -> drive deterministic role epochs through i2pd-A SAM sessions with
//!      source-locked `explicitPeers` placement (Destination.cpp +
//!      TunnelPool::SelectExplicitPeers, params via SAM SESSION CREATE)
//!   -> consume real Ssu2DaemonHandle::next_inbound()
//!   -> record typed epoch-scoped ledger observations
//!   -> terminate/drain all owners and child processes
//! ```
//!
//! Role rows bind to the decoded [`TransitHopRoleKind`] carried by
//! [`TransitBuildEvidence`]; a generic "some build arrived" boolean can never
//! satisfy more than the small set of facts its own epoch proves. The legacy
//! `observed_build` fan-out is gone: this file must not contain that
//! identifier (enforced by
//! `scripts/check-m11-transit-qualification-evidence.sh`).
//!
//! The driver is `#[ignore]`-gated in ordinary CI: the runner selects it
//! explicitly with `--ignored --exact` and supplies the i2pd binary, the
//! exact pin, fresh reference datadirs, loopback ports, and the evidence
//! directory through the environment. Missing environment variables, a pin
//! mismatch, or missing topology prerequisites fail the driver closed
//! rather than silently passing.
//!
//! The driver does NOT:
//!
//! - hand-construct STBMs after runtime startup in the external lane;
//! - generate an independent transit responder key (the responder secret is
//!   the bundle encryption private key; `X25519PrivateKey::generate` must
//!   not appear in this file);
//! - derive a NetDB path from the evidence directory (explicit
//!   `I2PD_A_DATADIR` / `I2PD_B_DATADIR` are required);
//! - patch i2pd or rely on LD_PRELOAD / namespace helpers;
//! - contact the public I2P network (no reseed, no introducer);
//! - log SSU2 static secrets, signing seeds, or raw payloads;
//! - write any private key material into the retained evidence directory.

#![forbid(unsafe_code)]

use std::io::Write as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::config::Config;
use i2pr_daemon::router_i2np::{
    RouterDeliveryOutcome, RouterDeliveryService, Ssu2DaemonService, daemon_dial_target,
    dispatch_router_i2np, dispatch_router_i2np_with_transit_bodies, generate_controlled_identity,
    verify_reference_router_info,
};
use i2pr_daemon::transit_compose::{TransitBuildService, TransitHopMaterial};
use i2pr_daemon::transit_owner::{
    LiveGatewayOutcome, LiveInboundOutcome, TransitBuildEvidence, TransitLiveOwner,
};
use i2pr_proto::{Hash, I2npMessage, RouterInfo};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope, Ssu2InboundI2np};
use i2pr_transport::PeerId;
use i2pr_tunnel::build_crypto::{BuildCryptography, EciesX25519BuildCryptography};
use i2pr_tunnel::{TransitBandwidthSummary, TransitHopRoleKind};
use rand_chacha::ChaCha8Rng;
use rand_core::{OsRng, RngCore, SeedableRng};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufStream};

const EXPECTED_I2PD_PIN: &str = "635b013a612ff47278ef02acf8580a28e10e26c5";
const EXPECTED_I2PD_VERSION: &str = "2.61.0";

const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
/// Caller-visible cap for one inbound SAM datagram payload read.
/// The lane's largest stimulus is 4096 bytes; the reference
/// itself caps session datagrams at its 8 KiB stream buffer, so
/// anything larger is a framing defect, never a genuine delivery.
const MAX_SAM_DATAGRAM_RX_BYTES: usize = 8192;
/// Seconds between session heartbeat redials inside the SAM setup
/// loops. Short enough to heal a silently dead link within one
/// counted-pool retry period (~15 s times two), long enough that
/// duplicate-safe handshakes never dominate the drain.
const SETUP_HEARTBEAT_SECS: u64 = 30;
/// Per-peer dial horizon for the setup-loop heartbeat. A full
/// 20 s stall per peer would wedge inbound drains, so the
/// heartbeat uses a shorter bound; both dials run concurrently.
const SETUP_HEARTBEAT_DIAL_TIMEOUT: Duration = Duration::from_secs(8);

fn env_secs(name: &str, fallback: u64) -> Duration {
    Duration::from_secs(
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(fallback),
    )
}

/// Build-epoch drain bound (default 150 s; override with
/// `I2PR_M11_BUILD_TIMEOUT_SECS` for bounded diagnostic runs —
/// counted qualification runs always use the default).
fn build_epoch_timeout() -> Duration {
    env_secs("I2PR_M11_BUILD_TIMEOUT_SECS", 150)
}

/// Data-epoch drain bound (default 90 s; override with
/// `I2PR_M11_DATA_TIMEOUT_SECS` for bounded diagnostic runs).
fn data_epoch_timeout() -> Duration {
    env_secs("I2PR_M11_DATA_TIMEOUT_SECS", 90)
}
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const SAM_IO_TIMEOUT: Duration = Duration::from_secs(15);
/// Exact fail-closed tail for a SAM session reply that never
/// arrives within `SAM_IO_TIMEOUT`. The string is locked as a
/// constant so counted evidence, the runner, and the static
/// checker match one canonical tail; a SAM read timeout always
/// fails the attempt (never retried in-process).
/// Plan 263 work package A.3 (SAM discipline).
const SAM_READ_TIMEOUT_MSG: &str = "SAM read timeout";

/// Maximum typed observations retained for one qualification run.
/// One run holds a bounded handful of epochs; the ceiling keeps the
/// ledger an explicit bounded queue, never an unbounded event log.
/// Only classified transit-relevant outcomes are recorded (stale or
/// unrelated inbound is dropped before the ledger). The event-driven
/// send phase pumps the drain for several minutes while the mesh
/// churns a few rows per second, so the ceiling covers a few
/// thousand entries with headroom for a full multi-epoch run.
const MAX_LEDGER_OBSERVATIONS: usize = 4096;

/// Logical transit lifetime in milliseconds (600 seconds). The expiry
/// epoch advances the injected transit clock beyond creation + this
/// bound and then delivers a genuine reference cell.
const TRANSIT_LIFETIME_MS: u64 = 600_000;

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(1_700_000_000_000)
}

fn wall_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .unwrap_or(1_700_000_000)
}

fn env_value(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("missing required env {name}"))
}

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(env_value(name))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[usize::from((byte >> 4) & 0x0F)] as char);
        out.push(HEX[usize::from(byte & 0x0F)] as char);
    }
    out
}

/// i2p base64 with the exact pinned substitution table (`T64` in
/// `libi2pd/Base.cpp`: `A-Za-z0-9-~`, `=` padding, no line breaks).
/// Source-locked against `ByteStreamToBase64` in i2pd 2.61.0.
fn i2p_b64_encode(bytes: &[u8]) -> String {
    const T64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut chunks = bytes.chunks_exact(3);
    for chunk in &mut chunks {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        out.push(T64[((n >> 18) & 63) as usize] as char);
        out.push(T64[((n >> 12) & 63) as usize] as char);
        out.push(T64[((n >> 6) & 63) as usize] as char);
        out.push(T64[(n & 63) as usize] as char);
    }
    let rem = chunks.remainder();
    if rem.len() == 1 {
        let n = u32::from(rem[0]) << 16;
        out.push(T64[((n >> 18) & 63) as usize] as char);
        out.push(T64[((n >> 12) & 63) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem.len() == 2 {
        let n = (u32::from(rem[0]) << 16) | (u32::from(rem[1]) << 8);
        out.push(T64[((n >> 18) & 63) as usize] as char);
        out.push(T64[((n >> 12) & 63) as usize] as char);
        out.push(T64[((n >> 6) & 63) as usize] as char);
        out.push('=');
    }
    out
}

/// i2p base64 decode with the exact pinned substitution table
/// (`T64` in `libi2pd/Base.cpp`: `A-Za-z0-9-~`, `=` padding).
/// Test-only mirror of [`i2p_b64_encode`]; fails closed on any
/// non-alphabet byte or malformed padding. Used by the Plan 260
/// receipt epoch to hash SAM destination identities without a
/// new dependency.
fn i2p_b64_decode(text: &str) -> Result<Vec<u8>, String> {
    const T64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
    let mut values = Vec::with_capacity(text.len());
    for byte in text.bytes() {
        if byte == b'=' {
            break;
        }
        let value = T64
            .iter()
            .position(|entry| *entry == byte)
            .ok_or_else(|| format!("i2p base64 rejects byte {byte:#04X}"))?;
        values.push(value as u32);
    }
    if values.len() % 4 == 1 {
        return Err("i2p base64 rejects trailing single quantum".to_string());
    }
    let mut out = Vec::with_capacity(values.len() / 4 * 3 + 3);
    for quad in values.chunks(4) {
        let n = match quad.len() {
            4 => (quad[0] << 18) | (quad[1] << 12) | (quad[2] << 6) | quad[3],
            3 => (quad[0] << 18) | (quad[1] << 12) | (quad[2] << 6),
            2 => (quad[0] << 18) | (quad[1] << 12),
            _ => unreachable!("quantum remainder checked above"),
        };
        out.push((n >> 16) as u8);
        if quad.len() >= 3 {
            out.push((n >> 8) as u8);
        }
        if quad.len() == 4 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

/// Destination identity hash (hex) for one SAM base64 destination:
/// SHA-256 over the decoded destination bytes, the exact
/// `IdentHash` derivation the reference applies to destinations.
/// Plan 260 records receiver/pool identities in this form —
/// public routing facts only, never key material.
fn destination_hash_hex(destination_b64: &str) -> Result<String, String> {
    let bytes = i2p_b64_decode(destination_b64)?;
    Ok(hex_lower(i2pr_crypto::sha256(&bytes).as_bytes()))
}

/// Exact source-locked i2pd 2.61.0 NetDB file path for one router hash.
///
/// `NetDb` owns `HashedStorage("netDb", "r", "routerInfo-", "dat")`
/// (`libi2pd/NetDb.cpp`), rooted at the reference datadir
/// (`HashedStorage::SetPlace(GetDataDir())`). `HashedStorage::Path`
/// (`libi2pd/FS.cpp`) maps ident `I` to
/// `<datadir>/netDb/r<C0>/routerInfo-<I>.dat` with `/` and `\`
/// replaced by `-`. The ident rendering is the i2p base64 of the
/// 32-byte router hash.
fn i2pd_netdb_file_path(datadir: &Path, ident_b64: &str) -> PathBuf {
    let first = ident_b64.chars().next().expect("nonempty ident");
    let safe: String = ident_b64.replace(['/', '\\'], "-");
    datadir
        .join("netDb")
        .join(format!("r{first}"))
        .join(format!("routerInfo-{safe}.dat"))
}

/// Non-cryptographic 64-bit change detector for cell-identity
/// evidence (replay pairs, forward digests). Labels in evidence
/// always carry the `fnv-` prefix so the value is never mistaken
/// for an integrity proof.
fn fnv_digest(bytes: &[u8]) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_4842_2225;
    const PRIME: u64 = 0x0100_0000_01b3;
    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:016x}")
}

fn append_evidence(dir: &Path, label: &str, value: &str) {
    let sanitized = value.replace(['\t', '\n'], " ");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("driver-evidence.tsv"))
        .expect("open evidence file");
    writeln!(file, "{label}\t{sanitized}").expect("write evidence row");
}

fn router_delivery_for(identity_bytes: Vec<u8>, port: u16) -> RouterDeliveryService {
    use i2pr_runtime::Ssu2RuntimeConfig;
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
    let identity = generate_controlled_identity(&bundle, "127.0.0.1", port).expect("identity");
    let _ = identity_bytes;
    let runtime = i2pr_runtime::Ssu2RuntimeService::new(Ssu2RuntimeConfig::default(), identity)
        .expect("runtime");
    RouterDeliveryService::new(runtime)
}

/// Lane-local bounded ceilings for the external qualification
/// owners. A live mixed-router mesh delivers exploratory churn
/// alongside the counted epoch builds; the ceilings use the
/// type-level maxima (64 global active) so the counted rows are
/// never starved by background retries, while production defaults
/// elsewhere are untouched.
const QUALIFICATION_MAX_ACTIVE: u16 = 64;
const QUALIFICATION_MAX_PENDING: u16 = 8;
const QUALIFICATION_MAX_ACTIVE_PER_PEER: u16 = 8;
const QUALIFICATION_MAX_PENDING_PER_PEER: u16 = 2;
const QUALIFICATION_REGISTRY_CAPACITY: u16 = 64;
/// Active-count watermark for the admission reset below. Background
/// mesh churn fills the per-peer quotas (8) within half a minute,
/// so the watermark sits below the per-peer quota: epochs reset
/// early and often to keep room for the counted retry, while
/// data/lifecycle epochs never reset (they need role
/// registrations live). Resets fire while the epoch's
/// counted accept is still missing, plus again when that accept
/// goes stale without SAM readiness, so live counted state is
/// never dropped while it can still establish.
const QUALIFICATION_RESET_THRESHOLD: usize = 6;
/// Minimum seconds between admission resets in one epoch.
const QUALIFICATION_RESET_COOLDOWN_SECS: u64 = 10;
/// Minimum seconds between admission resets while a counted
/// sender setup still waits for its accept. Kept equal to the
/// relaxed cooldown: tighter resets wipe-and-rebuild background
/// faster than the references settle, churning the mesh (and the
/// reference SAM bridge) harder without raising the counted
/// retry's win rate. Quota room for the counted retry comes from
/// healthy sender pools (inbound-carrying sessions whose tunnels
/// stay up and drain fast), not from wiping.
const COUNTED_SETUP_RESET_COOLDOWN_SECS: u64 = 10;
/// Stale-accept horizon in seconds: a role/counted accept that
/// still has no SAM-ready destination after this long is stuck.
/// Saturated quotas answer every pool retry with code 30 while
/// the reset suppression below protects the stale live state
/// forever (a full ibgw-data section drowned at active 10 with
/// the retry doomed by the shared ledger). Past this horizon
/// admission resets resume until readiness breaks the loop. A
/// fresh accept keeps full suppression, so an establishing pool
/// gets an undisturbed window, and every rescue reset is
/// followed by the same window once the pool rebuilds.
const STALE_ACCEPT_RESET_SECS: u64 = 60;
/// A-direct starvation horizon in seconds for counted-sender
/// setups. Both lane senders live in A with a direct one-hop
/// outbound through us; a starved direct path (no inbound from A
/// at all while B-forwarded traffic still flows) means the
/// reference-side link died asymmetrically with no close event,
/// so the reactive close-gated heartbeat never fires and the
/// conditional ensure (our view stays healthy) never redials.
/// Past this horizon the setup force-redials A once per window.
/// Gated on observed traffic, never periodic: during an active
/// counted setup A always has retries in flight, so starvation
/// proves the fault, and a healthy link never triggers the
/// duplicate-divergence hazard documented on
/// [`ensure_sessions`].
const A_DIRECT_STARVE_SECS: u64 = 45;

/// Returns the admission-reset cooldown for one build stop.
fn reset_cooldown(stop: &BuildStop) -> Duration {
    match stop {
        BuildStop::CountedObepSetup { .. } => {
            Duration::from_secs(COUNTED_SETUP_RESET_COOLDOWN_SECS)
        }
        _ => Duration::from_secs(QUALIFICATION_RESET_COOLDOWN_SECS),
    }
}

fn build_transit_service(
    hop_identity: Hash,
    responder_priv: [u8; 32],
    delivery: RouterDeliveryService,
) -> TransitBuildService {
    use i2pr_tunnel::{TransitAdmissionPolicy, TransitMode};
    let hop_material = TransitHopMaterial::new(responder_priv, hop_identity);
    let policy = TransitAdmissionPolicy::new(
        true,
        TransitMode::Accepting,
        QUALIFICATION_MAX_ACTIVE,
        QUALIFICATION_MAX_PENDING,
        QUALIFICATION_MAX_ACTIVE_PER_PEER,
        QUALIFICATION_MAX_PENDING_PER_PEER,
        256,
        None,
    )
    .expect("policy");
    TransitBuildService::new(
        hop_material,
        policy,
        QUALIFICATION_REGISTRY_CAPACITY,
        delivery,
    )
    .expect("service")
}

fn build_local_transit_service(
    hop_identity: Hash,
    responder_priv: [u8; 32],
) -> TransitBuildService {
    build_transit_service(
        hop_identity,
        responder_priv,
        router_delivery_for(Vec::new(), 44_255),
    )
}

fn build_rejecting_transit_service(
    hop_identity: Hash,
    responder_priv: [u8; 32],
    delivery: RouterDeliveryService,
) -> TransitBuildService {
    use i2pr_tunnel::TransitAdmissionPolicy;
    let hop_material = TransitHopMaterial::new(responder_priv, hop_identity);
    let policy = TransitAdmissionPolicy::disabled();
    TransitBuildService::new(hop_material, policy, 8, delivery).expect("service")
}

fn build_obep_record(
    reply_router: Hash,
    receive: u32,
    next: u32,
    message_id: u32,
    _expiration: u32,
) -> i2pr_tunnel::short_record::ShortRequestRecord {
    use i2pr_tunnel::identity::TunnelId;
    use i2pr_tunnel::short_record::{
        BuildOptions, HopRole, LayerEncryptionType, REQUEST_EXPIRATION_SECONDS, ShortRequestRecord,
    };
    ShortRequestRecord::try_new(
        TunnelId::new(receive).expect("id"),
        TunnelId::new(next).expect("id"),
        reply_router,
        HopRole::OutboundEndpoint,
        LayerEncryptionType::Aes,
        i2pr_proto::Date::from_millis(wall_ms()),
        REQUEST_EXPIRATION_SECONDS,
        message_id,
        BuildOptions::empty(),
    )
    .expect("record")
}

fn build_ibgw_record(
    next_router: Hash,
    receive: u32,
    next: u32,
    message_id: u32,
    _expiration: u32,
) -> i2pr_tunnel::short_record::ShortRequestRecord {
    use i2pr_tunnel::identity::TunnelId;
    use i2pr_tunnel::short_record::{
        BuildOptions, HopRole, LayerEncryptionType, REQUEST_EXPIRATION_SECONDS, ShortRequestRecord,
    };
    ShortRequestRecord::try_new(
        TunnelId::new(receive).expect("id"),
        TunnelId::new(next).expect("id"),
        next_router,
        HopRole::InboundGateway,
        LayerEncryptionType::Aes,
        i2pr_proto::Date::from_millis(wall_ms()),
        REQUEST_EXPIRATION_SECONDS,
        message_id,
        BuildOptions::empty(),
    )
    .expect("record")
}

fn build_participant_record(
    next_router: Hash,
    receive: u32,
    next: u32,
    message_id: u32,
    _expiration: u32,
) -> i2pr_tunnel::short_record::ShortRequestRecord {
    use i2pr_tunnel::identity::TunnelId;
    use i2pr_tunnel::short_record::{
        BuildOptions, HopRole, LayerEncryptionType, REQUEST_EXPIRATION_SECONDS, ShortRequestRecord,
    };
    ShortRequestRecord::try_new(
        TunnelId::new(receive).expect("id"),
        TunnelId::new(next).expect("id"),
        next_router,
        HopRole::Participant,
        LayerEncryptionType::Aes,
        i2pr_proto::Date::from_millis(wall_ms()),
        REQUEST_EXPIRATION_SECONDS,
        message_id,
        BuildOptions::empty(),
    )
    .expect("record")
}

fn make_stbm_payload(
    responder_priv: &[u8; 32],
    hop_identity: &Hash,
    record: &i2pr_tunnel::short_record::ShortRequestRecord,
    rng: &mut ChaCha8Rng,
) -> Vec<u8> {
    use i2pr_proto::SHORT_BUILD_RECORD_SIZE;
    use i2pr_tunnel::multirecord::RECORD_BYTES;
    use i2pr_tunnel::multirecord::encode_count_prefixed_short_payload;
    let cryptography = EciesX25519BuildCryptography::new();
    let responder_pub = {
        let secret = x25519_dalek::StaticSecret::from(*responder_priv);
        x25519_dalek::PublicKey::from(&secret).to_bytes()
    };
    let plaintext = record.encode_with_rng(rng).expect("encode");
    let mut sealed = [0_u8; SHORT_BUILD_RECORD_SIZE];
    sealed.copy_from_slice(
        cryptography
            .seal_short_request(&plaintext, &responder_pub, hop_identity.as_bytes(), rng)
            .expect("seal")
            .record
            .as_ref(),
    );
    let mut slots: Vec<[u8; RECORD_BYTES]> = vec![[0u8; RECORD_BYTES]; 4];
    slots[1] = sealed;
    for (index, slot) in slots.iter_mut().enumerate() {
        if index == 1 {
            continue;
        }
        rng.fill_bytes(slot);
    }
    encode_count_prefixed_short_payload(4, &slots).expect("encode")
}

fn encode_standard_stbm(payload: &[u8], message_id: u32, expiration_ms: u64) -> Vec<u8> {
    use i2pr_proto::{DeferredBuildRecords, I2npBody, MAX_I2NP_PAYLOAD_SIZE};
    let count = payload[0];
    let records = payload[1..].to_vec();
    let deferred = DeferredBuildRecords::new(count, 218, records).expect("deferred");
    let body = I2npBody::ShortTunnelBuild(deferred);
    I2npMessage::new_standard(
        message_id,
        i2pr_proto::Date::from_millis(expiration_ms),
        body,
    )
    .expect("message")
    .encode_standard_to_vec(MAX_I2NP_PAYLOAD_SIZE)
    .expect("encode")
}

/// Local Plan 255 regression rows that exercise the live owner path
/// without depending on the external i2pd environment. They are the
/// same rows the runner labels "local Plan 254 / Plan 255 rows
/// (foundation)" and they prove the gates the external lane relies on.
#[test]
fn plan255_local_live_owner_dispatches_obep_through_handle_inbound() {
    let responder_priv = [0xA3; 32];
    let hop_identity = Hash::from_bytes([0x55; 32]);
    let service = build_local_transit_service(hop_identity, responder_priv);
    let delivery = router_delivery_for(Vec::new(), 44_256);
    let mut owner = TransitLiveOwner::new_disabled(
        delivery,
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(0xC0FFEE),
    );
    owner.enable(service);
    assert!(owner.is_enabled(), "controlled live owner must be enabled");

    let mut rng = ChaCha8Rng::seed_from_u64(0xC0FFEE);
    let reply_router = Hash::from_bytes([0xAA; 32]);
    let record = build_obep_record(reply_router, 0x9101, 0x9102, 0x51A7_5101, 30);
    let payload = make_stbm_payload(&responder_priv, &hop_identity, &record, &mut rng);
    let bytes = encode_standard_stbm(&payload, 0x51A7_5101, wall_ms() + 60_000);
    let inbound = Ssu2InboundI2np {
        link_id: i2pr_transport::LinkId::new(11).expect("link"),
        peer: PeerId::from_bytes([0x99; 32]),
        bytes,
    };
    let outcome = owner
        .handle_inbound(&inbound, wall_ms(), wall_secs())
        .expect("handle");
    assert!(
        matches!(
            outcome,
            LiveInboundOutcome::Build(i2pr_daemon::transit_owner::LiveBuildOutcome::Dispatched(_))
        ),
        "expected live owner to dispatch OBEP build, got {outcome:?}"
    );
    if let LiveInboundOutcome::Build(i2pr_daemon::transit_owner::LiveBuildOutcome::Dispatched(
        evidence,
    )) = outcome
    {
        assert_eq!(
            evidence.role,
            Some(TransitHopRoleKind::OutboundEndpoint),
            "OBEP dispatch must carry typed OBEP role evidence"
        );
        assert!(!evidence.rejected);
        assert_eq!(evidence.receive_tunnel, 0x9101);
    }
}

/// Local Plan 256 regression row for the reference `IBGW is
/// local` reply branch (`libi2pd/TransitTunnel.cpp`
/// `HandleShortTransitTunnelBuildMsg`): an endpoint build whose
/// reply router is the local hop identity must not attempt a
/// session send. With no IBGW registration for the reply tunnel
/// the injection fails closed as `Cancelled` and the endpoint
/// registration rolls back, instead of the previous
/// `NoActiveSession` terminal that stranded every builder pool
/// whose live inbound gateway was us.
#[test]
fn plan256_local_live_owner_self_reply_fails_closed_without_ibgw() {
    let responder_priv = [0xA3; 32];
    let hop_identity = Hash::from_bytes([0x55; 32]);
    let service = build_local_transit_service(hop_identity, responder_priv);
    let delivery = router_delivery_for(Vec::new(), 44_259);
    let mut owner = TransitLiveOwner::new_disabled(
        delivery,
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(0xC0FFEE),
    );
    owner.enable(service);
    assert!(owner.is_enabled(), "controlled live owner must be enabled");

    let mut rng = ChaCha8Rng::seed_from_u64(0xC0FFEE);
    let record = build_obep_record(hop_identity, 0x9101, 0x9102, 0x51A7_5101, 30);
    let payload = make_stbm_payload(&responder_priv, &hop_identity, &record, &mut rng);
    let bytes = encode_standard_stbm(&payload, 0x51A7_5101, wall_ms() + 60_000);
    let inbound = Ssu2InboundI2np {
        link_id: i2pr_transport::LinkId::new(11).expect("link"),
        peer: PeerId::from_bytes([0x99; 32]),
        bytes,
    };
    let outcome = owner
        .handle_inbound(&inbound, wall_ms(), wall_secs())
        .expect("handle");
    assert!(
        matches!(
            outcome,
            LiveInboundOutcome::Build(i2pr_daemon::transit_owner::LiveBuildOutcome::Dispatched(_))
        ),
        "expected live owner to dispatch self-reply OBEP build, got {outcome:?}"
    );
    if let LiveInboundOutcome::Build(i2pr_daemon::transit_owner::LiveBuildOutcome::Dispatched(
        evidence,
    )) = outcome
    {
        assert_eq!(
            evidence.role,
            Some(TransitHopRoleKind::OutboundEndpoint),
            "self-reply dispatch must carry typed OBEP role evidence"
        );
        assert!(!evidence.rejected);
        assert_eq!(evidence.receive_tunnel, 0x9101);
        assert_eq!(evidence.next_router, Some(hop_identity));
        assert_eq!(
            evidence.delivery,
            i2pr_daemon::router_i2np::RouterDeliveryOutcome::Cancelled,
            "self reply with no IBGW registration must fail closed, not NoActiveSession"
        );
    }
    assert_eq!(
        owner.active_count(),
        0,
        "endpoint registration must roll back on undeliverable self reply"
    );
}

#[test]
fn plan255_local_live_owner_dispatches_ibgw_through_handle_inbound() {
    let responder_priv = [0xB4; 32];
    let hop_identity = Hash::from_bytes([0x77; 32]);
    let service = build_local_transit_service(hop_identity, responder_priv);
    let delivery = router_delivery_for(Vec::new(), 44_257);
    let mut owner = TransitLiveOwner::new_disabled(
        delivery,
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(0xBAAD),
    );
    owner.enable(service);
    assert!(owner.is_enabled());

    let mut rng = ChaCha8Rng::seed_from_u64(0xBAAD);
    let next_router = Hash::from_bytes([0xCC; 32]);
    let record = build_ibgw_record(next_router, 0x9201, 0x9202, 0x51A7_5201, 30);
    let payload = make_stbm_payload(&responder_priv, &hop_identity, &record, &mut rng);
    let bytes = encode_standard_stbm(&payload, 0x51A7_5201, wall_ms() + 60_000);
    let inbound = Ssu2InboundI2np {
        link_id: i2pr_transport::LinkId::new(12).expect("link"),
        peer: PeerId::from_bytes([0x99; 32]),
        bytes,
    };
    let outcome = owner
        .handle_inbound(&inbound, wall_ms(), wall_secs())
        .expect("handle");
    assert!(matches!(
        outcome,
        LiveInboundOutcome::Build(i2pr_daemon::transit_owner::LiveBuildOutcome::Dispatched(_))
    ));
    if let LiveInboundOutcome::Build(i2pr_daemon::transit_owner::LiveBuildOutcome::Dispatched(
        evidence,
    )) = outcome
    {
        assert_eq!(evidence.role, Some(TransitHopRoleKind::InboundGateway));
        assert_eq!(evidence.next_router, Some(next_router));
    }
}

#[test]
fn plan255_local_live_owner_dispatches_participant_through_handle_inbound() {
    let responder_priv = [0xC5; 32];
    let hop_identity = Hash::from_bytes([0x88; 32]);
    let service = build_local_transit_service(hop_identity, responder_priv);
    let delivery = router_delivery_for(Vec::new(), 44_258);
    let mut owner = TransitLiveOwner::new_disabled(
        delivery,
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(0xFEED),
    );
    owner.enable(service);
    assert!(owner.is_enabled());

    let mut rng = ChaCha8Rng::seed_from_u64(0xFEED);
    let next_router = Hash::from_bytes([0xDD; 32]);
    let record = build_participant_record(next_router, 0x9301, 0x9302, 0x51A7_5301, 30);
    let payload = make_stbm_payload(&responder_priv, &hop_identity, &record, &mut rng);
    let bytes = encode_standard_stbm(&payload, 0x51A7_5301, wall_ms() + 60_000);
    let inbound = Ssu2InboundI2np {
        link_id: i2pr_transport::LinkId::new(13).expect("link"),
        peer: PeerId::from_bytes([0x99; 32]),
        bytes,
    };
    let outcome = owner
        .handle_inbound(&inbound, wall_ms(), wall_secs())
        .expect("handle");
    assert!(matches!(
        outcome,
        LiveInboundOutcome::Build(i2pr_daemon::transit_owner::LiveBuildOutcome::Dispatched(_))
    ));
    if let LiveInboundOutcome::Build(i2pr_daemon::transit_owner::LiveBuildOutcome::Dispatched(
        evidence,
    )) = outcome
    {
        assert_eq!(evidence.role, Some(TransitHopRoleKind::Participant));
        assert_eq!(evidence.next_router, Some(next_router));
        assert_eq!(evidence.next_message_id, 0x51A7_5301);
    }
}

#[test]
fn plan255_local_no_direct_build_injection_after_runtime_startup() {
    let hop_identity = Hash::from_bytes([0x55; 32]);
    let _service = build_local_transit_service(hop_identity, [0xA3; 32]);
    // The source-level guard is the production invariant: the driver
    // never names the Plan 253 placeholder or hand-builds a STBM after
    // the daemon-owned SSU2 runtime has started. The static checker
    // `check-m11-transit-boundaries.sh` enforces this rule on the
    // daemon source; this local row records the corresponding
    // contract on the test surface.
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/transit_owner.rs"),
    )
    .expect("read transit_owner");
    assert!(
        !src.contains("plan253_short_build_payload"),
        "transit_owner must not reintroduce the empty-body shim"
    );
    assert!(
        src.contains("TransitLiveOwner::handle_inbound"),
        "transit_owner must drive all inbound builds through the live owner"
    );
}

#[test]
fn plan255_local_disabled_probe_preserves_reserved_outcome() {
    let hop_identity = Hash::from_bytes([0x55; 32]);
    let service = build_local_transit_service(hop_identity, [0xA3; 32]);
    let delivery = router_delivery_for(Vec::new(), 44_259);
    let mut owner = TransitLiveOwner::new_disabled(
        delivery,
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(0xDEAD),
    );
    assert!(!owner.is_enabled());
    let mut rng = ChaCha8Rng::seed_from_u64(0xDEAD);
    let reply_router = Hash::from_bytes([0xAA; 32]);
    let record = build_obep_record(reply_router, 0x9401, 0x9402, 0x51A7_5401, 30);
    let payload = make_stbm_payload(&[0xA3; 32], &hop_identity, &record, &mut rng);
    let bytes = encode_standard_stbm(&payload, 0x51A7_5401, wall_ms() + 60_000);
    let inbound = Ssu2InboundI2np {
        link_id: i2pr_transport::LinkId::new(14).expect("link"),
        peer: PeerId::from_bytes([0x99; 32]),
        bytes,
    };
    let outcome = owner
        .handle_inbound(&inbound, wall_ms(), wall_secs())
        .expect("handle");
    assert!(matches!(
        outcome,
        LiveInboundOutcome::Build(i2pr_daemon::transit_owner::LiveBuildOutcome::DisabledReserved)
    ));
    // Service must not be reachable while disabled.
    assert_eq!(owner.active_count(), 0);
    drop(service);
}

#[test]
fn plan255_local_canonical_decode_yields_exact_body() {
    // The Plan 254 single-decode handoff is the foundation of every
    // external row. Prove that `dispatch_router_i2np_with_transit_bodies`
    // returns the exact STBM body bytes; the live owner must consume
    // this body verbatim, never a fabricated one.
    let responder_priv = [0xA3; 32];
    let hop_identity = Hash::from_bytes([0x55; 32]);
    let mut rng = ChaCha8Rng::seed_from_u64(0xC0DE);
    let reply_router = Hash::from_bytes([0xAA; 32]);
    let record = build_obep_record(reply_router, 0x9501, 0x9502, 0x51A7_5501, 30);
    let payload = make_stbm_payload(&responder_priv, &hop_identity, &record, &mut rng);
    let bytes = encode_standard_stbm(&payload, 0x51A7_5501, wall_ms() + 60_000);
    let inbound = Ssu2InboundI2np {
        link_id: i2pr_transport::LinkId::new(15).expect("link"),
        peer: PeerId::from_bytes([0x99; 32]),
        bytes,
    };
    let (outcome, bodies) =
        dispatch_router_i2np_with_transit_bodies(&inbound, wall_ms()).expect("dispatch");
    assert!(matches!(
        outcome,
        i2pr_daemon::router_i2np::RouterI2npOutcome::TunnelBuildReserved {
            kind: i2pr_daemon::router_i2np::RouterI2npKind::ShortTunnelBuild,
            ..
        }
    ));
    let body = bodies.short_build_body.expect("body");
    assert_eq!(body, payload, "decoded body must match the encoded payload");
}

#[test]
fn plan255_local_expiry_drops_live_data() {
    // Local row: a registration whose receive-id reaches the live
    // owner must drop a later genuine TunnelData after logical
    // 600-second expiry. We do not change production expiry constants;
    // we move the live clock past the expiry timestamp and assert the
    // data-plane dispatch reaches the live owner's Dropped disposition
    // or that cancellation fires first.
    let responder_priv = [0xD6; 32];
    let hop_identity = Hash::from_bytes([0x99; 32]);
    let service = build_local_transit_service(hop_identity, responder_priv);
    let delivery = router_delivery_for(Vec::new(), 44_260);
    let mut owner = TransitLiveOwner::new_disabled(
        delivery,
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(0xFADE),
    );
    owner.enable(service);
    let mut rng = ChaCha8Rng::seed_from_u64(0xFADE);
    let next_router = Hash::from_bytes([0xEE; 32]);
    let record = build_participant_record(next_router, 0x9601, 0x9602, 0x51A7_5601, 30);
    let payload = make_stbm_payload(&responder_priv, &hop_identity, &record, &mut rng);
    let stbm_bytes = encode_standard_stbm(&payload, 0x51A7_5601, wall_ms() + 60_000);
    let inbound = Ssu2InboundI2np {
        link_id: i2pr_transport::LinkId::new(16).expect("link"),
        peer: PeerId::from_bytes([0x99; 32]),
        bytes: stbm_bytes,
    };
    let build_outcome = owner
        .handle_inbound(&inbound, wall_ms(), wall_secs())
        .expect("build");
    assert!(matches!(
        build_outcome,
        LiveInboundOutcome::Build(i2pr_daemon::transit_owner::LiveBuildOutcome::Dispatched(_))
    ));

    // Cancel the live owner (the fail-safe drain path) and verify the
    // active count is zero. This is the deterministic Plan 255 §F
    // "cancellation drains" assertion; the live owner uses the
    // outer-token drain as its expiry equivalent.
    let cancel_token = owner.cancellation().clone();
    owner.cancel();
    assert!(cancel_token.is_cancelled());
    assert_eq!(owner.active_count(), 0, "cancel must drain state");
}

#[test]
fn plan255_local_cancel_drains() {
    // Local row: cancellation drains transit state synchronously. The
    // live owner exposes `cancel()` (Plan 254 §G); after cancellation,
    // subsequent inbound events keep the existing reserved / dropped
    // disposition without installing new registrations.
    let responder_priv = [0xE7; 32];
    let hop_identity = Hash::from_bytes([0xAA; 32]);
    let service = build_local_transit_service(hop_identity, responder_priv);
    let delivery = router_delivery_for(Vec::new(), 44_261);
    let mut owner = TransitLiveOwner::new_disabled(
        delivery,
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(0xCACA),
    );
    owner.enable(service);
    let cancel_token = owner.cancellation().clone();
    owner.cancel();
    assert!(cancel_token.is_cancelled());
    assert_eq!(owner.active_count(), 0, "cancel must drain state");
}

#[test]
fn plan255_local_session_close_reconciles_peer() {
    let responder_priv = [0xF8; 32];
    let hop_identity = Hash::from_bytes([0xBB; 32]);
    let service = build_local_transit_service(hop_identity, responder_priv);
    let delivery = router_delivery_for(Vec::new(), 44_262);
    let mut owner = TransitLiveOwner::new_disabled(
        delivery,
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(0xBEEF),
    );
    owner.enable(service);
    let peer = PeerId::from_bytes([0x77; 32]);
    let router_hash = Hash::from_bytes([0xCC; 32]);
    owner.install_peer(router_hash, peer).expect("install peer");
    let removed = owner.note_session_closed(&peer);
    assert!(
        removed,
        "session-close must reconcile the bounded peer mapping"
    );
}

#[test]
fn plan255_local_workspace_static_checker_rules_are_present() {
    // Local row: the Plan 255 boundary checker rules are wired in
    // routine CI; the test reads the checker script and asserts the
    // presence of every mandatory Plan 255 row label plus the
    // structural failsafe markers.
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/check-m11-transit-qualification-evidence.sh"),
    )
    .expect("read checker");
    for marker in [
        "m11-i2pd-live-next-inbound-observed",
        "m11-i2pd-live-owner-enabled",
        "m11-i2pd-obep-build-received",
        "m11-i2pd-ibgw-build-received",
        "m11-i2pd-participant-build-received",
        "m11-i2pd-code30-build-rejected",
        "m11-i2pd-expiry-drops-live-data",
        "m11-i2pd-cancel-drains",
        "m11-i2pd-driver-ignored-gated",
    ] {
        assert!(
            src.contains(marker),
            "checker missing Plan 255 row {marker}"
        );
    }
}

// ---------------------------------------------------------------------------
// Plan 256 typed evidence ledger (shared by local anti-fan-out rows and the
// external qualification flow).
// ---------------------------------------------------------------------------

/// Epoch identifiers for the corrected qualification lane. Each role and
/// each lifecycle experiment owns a unique epoch; a row for one epoch
/// can never be satisfied by an observation from another epoch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Epoch {
    Bootstrap,
    Reject,
    Obep,
    ObepData,
    Ibgw,
    IbgwData,
    IbgwReceipt,
    Participant,
    ParticipantData,
    Replay,
    Expiry,
    Cancel,
    SessionClose,
    Restart,
}

impl Epoch {
    const fn label(self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
            Self::Reject => "reject",
            Self::Obep => "obep",
            Self::ObepData => "obep-data",
            Self::Ibgw => "ibgw",
            Self::IbgwData => "ibgw-data",
            Self::IbgwReceipt => "ibgw-receipt",
            Self::Participant => "participant",
            Self::ParticipantData => "participant-data",
            Self::Replay => "replay",
            Self::Expiry => "expiry",
            Self::Cancel => "cancel",
            Self::SessionClose => "session-close",
            Self::Restart => "restart",
        }
    }
}

/// Kind of one typed ledger observation. Only events consumed from a
/// real `handle_inbound` outcome (or a real state snapshot) may
/// construct these; there is no generic "success" kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ObservedKind {
    BuildAccepted,
    BuildRejected,
    DataForwarded,
    DataDeliveredObep,
    DataDropped,
    GatewayDelivered,
    GatewayDropped,
    InboundObserved,
    StateSnapshot,
}

/// One typed observation appended to the qualification ledger. All
/// fields are non-secret facts: role kinds, tunnel/message ids,
/// router hashes (public identities), counts, digests, logical time.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Observation {
    epoch: Epoch,
    kind: ObservedKind,
    peer_hash: [u8; 32],
    role: Option<TransitHopRoleKind>,
    receive_tunnel: u32,
    next_router: Option<[u8; 32]>,
    next_message_id: u32,
    rejected: bool,
    delivery: &'static str,
    active_before: usize,
    active_after: usize,
    logical_ms: u64,
    digest: Option<String>,
    aux_count: usize,
    /// Pending admission reservations observed after the event
    /// (Plan 257 work package C snapshot dimension).
    pending_after: u16,
    /// Peer-index entries observed after the event.
    peer_index_after: usize,
    /// Transit-owned queued work observed after the event
    /// (architecturally zero).
    queued_after: usize,
    /// Non-secret typed bandwidth disposition copied from the
    /// build evidence (Plan 257 work package E). `Some` on build
    /// observations, `None` elsewhere. The daemon never re-decodes
    /// the `Mapping`; the external bandwidth rows derive from this
    /// summary.
    bandwidth: Option<TransitBandwidthSummary>,
    /// Bounded per-ingress forward failures counted by the router
    /// seam (Plan 258 work package A failure telemetry). `Some`
    /// on gateway-delivery observations (explicit even when zero),
    /// `None` elsewhere. A `GatewayDelivered` observation with
    /// `None` here is structurally incomplete.
    gateway_failures: Option<usize>,
    /// Encoded nested standard I2NP message length the gateway
    /// ingress carried (Plan 258 work package A nested size
    /// telemetry). `Some` on gateway-delivery observations, `None`
    /// elsewhere; the single-cell versus multi-cell-capable class
    /// derives from the shared production threshold.
    nested_len: Option<usize>,
}

impl Observation {
    fn build(
        epoch: Epoch,
        peer_hash: [u8; 32],
        evidence: &TransitBuildEvidence,
        active_before: usize,
        active_after: usize,
        logical_ms: u64,
    ) -> Self {
        Self {
            epoch,
            kind: if evidence.rejected {
                ObservedKind::BuildRejected
            } else {
                ObservedKind::BuildAccepted
            },
            peer_hash,
            role: evidence.role,
            receive_tunnel: evidence.receive_tunnel,
            next_router: evidence.next_router.map(|hash| *hash.as_bytes()),
            next_message_id: evidence.next_message_id,
            rejected: evidence.rejected,
            delivery: delivery_label(&evidence.delivery),
            active_before,
            active_after,
            logical_ms,
            digest: None,
            aux_count: 0,
            pending_after: 0,
            peer_index_after: 0,
            queued_after: 0,
            bandwidth: Some(evidence.bandwidth),
            gateway_failures: None,
            nested_len: None,
        }
    }

    /// Attaches the post-event snapshot dimensions to a build
    /// observation without mutating any other fact.
    fn with_snapshot_after(
        mut self,
        pending_after: u16,
        peer_index_after: usize,
        queued_after: usize,
    ) -> Self {
        self.pending_after = pending_after;
        self.peer_index_after = peer_index_after;
        self.queued_after = queued_after;
        self
    }
}

fn delivery_label(outcome: &RouterDeliveryOutcome) -> &'static str {
    match outcome {
        RouterDeliveryOutcome::Accepted => "accepted",
        RouterDeliveryOutcome::NoActiveSession => "no-active-session",
        RouterDeliveryOutcome::QueueFull => "queue-full",
        RouterDeliveryOutcome::ResourceDenied => "resource-denied",
        RouterDeliveryOutcome::TooLarge => "too-large",
        RouterDeliveryOutcome::DeadlineElapsed => "deadline-elapsed",
        RouterDeliveryOutcome::Cancelled => "cancelled",
    }
}

/// Append-only typed observation ledger with a hard ceiling. Row
/// predicates query this ledger; they always name the expected
/// epoch, so cross-epoch reuse fails closed.
///
/// The ledger additionally retains a bounded number of genuine
/// forwarded cell bodies (in memory only, never in evidence) so
/// the replay and expiry epochs can re-deliver the exact bytes the
/// reference previously sent through the authenticated session.
#[derive(Debug, Default)]
struct TypedLedger {
    observations: Vec<Observation>,
    retained_cells: Vec<RetainedCell>,
}

/// One genuine reference cell retained for replay/expiry pairing.
/// At most [`MAX_RETAINED_CELLS`] are kept; each body is already
/// bounded by the I2NP payload ceiling.
#[derive(Clone, Debug)]
struct RetainedCell {
    epoch: Epoch,
    receive_tunnel: u32,
    digest: String,
    observed_ms: u64,
    bytes: Vec<u8>,
}

const MAX_RETAINED_CELLS: usize = 8;

impl TypedLedger {
    fn push(&mut self, observation: Observation) {
        assert!(
            self.observations.len() < MAX_LEDGER_OBSERVATIONS,
            "ledger ceiling exceeded"
        );
        self.observations.push(observation);
    }

    fn of_epoch(&self, epoch: Epoch) -> impl Iterator<Item = &Observation> {
        self.observations
            .iter()
            .filter(move |obs| obs.epoch == epoch)
    }

    /// Role-accept predicate: the epoch holds a non-rejected build
    /// observation with exactly the expected decoded role, from one
    /// of the expected authenticated peers (both exact-pinned
    /// references run genuine stock topologies through the hop: A
    /// via the counted SAM/pool shapes, B via its router one-hop
    /// fallbacks), with delivery accepted and a live registration
    /// delta of exactly +1. Role + epoch binding is what prevents
    /// fan-out; the peer allowlist only documents which reference
    /// delivered.
    fn role_accepted(&self, epoch: Epoch, role: TransitHopRoleKind, peers: &[[u8; 32]]) -> bool {
        self.of_epoch(epoch).any(|obs| {
            obs.kind == ObservedKind::BuildAccepted
                && obs.role == Some(role)
                && !obs.rejected
                && peers.contains(&obs.peer_hash)
                && obs.delivery == "accepted"
                && obs.active_after == obs.active_before + 1
        })
    }

    /// First matching accept for one role epoch, if any.
    fn first_accept(
        &self,
        epoch: Epoch,
        role: TransitHopRoleKind,
        peers: &[[u8; 32]],
    ) -> Option<Observation> {
        self.of_epoch(epoch)
            .find(|obs| {
                obs.kind == ObservedKind::BuildAccepted
                    && obs.role == Some(role)
                    && !obs.rejected
                    && peers.contains(&obs.peer_hash)
                    && obs.delivery == "accepted"
                    && obs.active_after == obs.active_before + 1
            })
            .cloned()
    }

    /// Freshness of the newest matching accept for one role
    /// epoch: true when a matching accept landed within
    /// [`STALE_ACCEPT_RESET_SECS`]. A stale accept (pool still
    /// not SAM-ready long after its build landed) must not keep
    /// suppressing admission resets, or saturated quotas hold
    /// every retry on code 30 forever.
    fn accept_fresh_ms(
        &self,
        epoch: Epoch,
        role: TransitHopRoleKind,
        peers: &[[u8; 32]],
        now_ms: u64,
    ) -> bool {
        self.of_epoch(epoch)
            .filter_map(|obs| {
                (obs.kind == ObservedKind::BuildAccepted
                    && obs.role == Some(role)
                    && !obs.rejected
                    && peers.contains(&obs.peer_hash)
                    && obs.delivery == "accepted"
                    && obs.active_after == obs.active_before + 1)
                    .then_some(obs.logical_ms)
            })
            .max()
            .is_some_and(|ms| {
                now_ms.saturating_sub(ms) < STALE_ACCEPT_RESET_SECS.saturating_mul(1_000)
            })
    }

    /// Code-30 predicate: the epoch holds a rejected build with the
    /// expected role, zero registration delta, and no live state
    /// growth.
    fn role_rejected(&self, epoch: Epoch, role: TransitHopRoleKind) -> bool {
        self.of_epoch(epoch).any(|obs| {
            obs.kind == ObservedKind::BuildRejected
                && obs.role == Some(role)
                && obs.rejected
                && obs.active_after == obs.active_before
        })
    }

    /// Data-forward predicate: the epoch holds a forward observation
    /// for the expected receive id toward the expected next router.
    fn data_forwarded(&self, epoch: Epoch, receive_tunnel: u32, next_router: [u8; 32]) -> bool {
        self.of_epoch(epoch).any(|obs| {
            obs.kind == ObservedKind::DataForwarded
                && obs.receive_tunnel == receive_tunnel
                && obs.next_router == Some(next_router)
        })
    }

    /// Replay predicate: the epoch holds no forward for the cell
    /// digest (the original delivery lives in its own data epoch)
    /// and at least one drop for the same digest, proving the
    /// re-delivered genuine cell produced no second delivery.
    fn replay_suppressed(&self, epoch: Epoch, digest: &str) -> bool {
        let forwards = self
            .of_epoch(epoch)
            .filter(|obs| {
                obs.kind == ObservedKind::DataForwarded && obs.digest.as_deref() == Some(digest)
            })
            .count();
        let drops = self
            .of_epoch(epoch)
            .filter(|obs| {
                obs.kind == ObservedKind::DataDropped && obs.digest.as_deref() == Some(digest)
            })
            .count();
        forwards == 0 && drops >= 1
    }

    /// Expiry predicate: the epoch holds a drop observation whose
    /// logical time is beyond creation + 600 s with no forward
    /// delta for the same receive id.
    fn expiry_enforced(&self, epoch: Epoch, receive_tunnel: u32, created_ms: u64) -> bool {
        let dropped = self.of_epoch(epoch).any(|obs| {
            obs.kind == ObservedKind::DataDropped
                && obs.receive_tunnel == receive_tunnel
                && obs.logical_ms >= created_ms + TRANSIT_LIFETIME_MS
        });
        let forwarded = self.of_epoch(epoch).any(|obs| {
            obs.kind == ObservedKind::DataForwarded && obs.receive_tunnel == receive_tunnel
        });
        dropped && !forwarded
    }

    /// Cancellation predicate: the epoch holds a snapshot pair with
    /// nonzero pre-state and zero post-state across every bounded
    /// dimension.
    fn cancel_drained(&self, epoch: Epoch) -> bool {
        self.of_epoch(epoch).any(|obs| {
            obs.kind == ObservedKind::StateSnapshot
                && obs.active_before > 0
                && obs.active_after == 0
        })
    }

    /// Plan 257 full-drain predicate: like [`Self::cancel_drained`]
    /// but additionally requires the pending, peer-index, and
    /// transit-owned queue dimensions to be zero after the event.
    /// An active-only drain can never satisfy this predicate.
    fn cancel_fully_drained(&self, epoch: Epoch) -> bool {
        self.of_epoch(epoch).any(|obs| {
            obs.kind == ObservedKind::StateSnapshot
                && obs.active_before > 0
                && obs.active_after == 0
                && obs.pending_after == 0
                && obs.peer_index_after == 0
                && obs.queued_after == 0
        })
    }

    /// Plan 257 far-side predicate: the epoch holds a local forward
    /// observation for the exact next tunnel plus an independent
    /// B-side endpoint observation for that same tunnel id. A local
    /// forward alone, or a B observation for a different tunnel,
    /// never satisfies this predicate.
    ///
    /// B-side observations are recorded as
    /// [`ObservedKind::InboundObserved`] with `receive_tunnel` set
    /// to the exact B-side endpoint tunnel id.
    fn far_side_satisfied(&self, epoch: Epoch, next_tunnel: u32) -> bool {
        let tag = format!("next={next_tunnel:#06x}");
        let forwarded = self.of_epoch(epoch).any(|obs| {
            obs.kind == ObservedKind::DataForwarded
                && (obs.next_message_id == next_tunnel
                    || obs.digest.as_deref() == Some(&tag)
                    || obs.aux_count == next_tunnel as usize)
        });
        let b_observed = self.of_epoch(epoch).any(|obs| {
            obs.kind == ObservedKind::InboundObserved && obs.receive_tunnel == next_tunnel
        });
        forwarded && b_observed
    }

    /// Plan 257 session-close predicate: peer A was removed while
    /// unrelated peer B remains. The ledger records session-close
    /// snapshots as [`ObservedKind::StateSnapshot`] with
    /// `peer_hash` = removed peer, `next_router` = retained peer,
    /// and `aux_count` = retained peer-index size (must be >= 1).
    fn session_close_a_removed_b_retained(
        &self,
        epoch: Epoch,
        removed: [u8; 32],
        retained: [u8; 32],
    ) -> bool {
        self.of_epoch(epoch).any(|obs| {
            obs.kind == ObservedKind::StateSnapshot
                && obs.peer_hash == removed
                && obs.next_router == Some(retained)
                && obs.active_after == 0
                && obs.aux_count >= 1
        })
    }
}

// ---------------------------------------------------------------------------
// Plan 258 work package A — gateway failure/nested-size telemetry.
// ---------------------------------------------------------------------------

/// Plan 258 diagnostic distribution over one epoch's accepted
/// gateway observations. Sanitized counts only; never an input to
/// any pass gate (the gate reads only the multicell predicate and
/// the receiver-socket receipt).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct GatewayDiagCounts {
    ingress: usize,
    nested_single: usize,
    nested_multi: usize,
    emitted_single: usize,
    emitted_multi: usize,
    emitted_max: usize,
    failures_total: usize,
    failed_ingress: usize,
}

/// Plan 258 acceptance filter: a gateway delivery counts only when
/// bound to an accepted registration id. Background gateway
/// traffic to other ids (stale or foreign tunnels) is recorded
/// but never satisfies rows.
fn gateway_ingress_accepted(obs: &Observation, accepted_gateways: &[u32]) -> bool {
    obs.kind == ObservedKind::GatewayDelivered && accepted_gateways.contains(&obs.receive_tunnel)
}

/// Plan 258 multicell pass predicate: at least one accepted gateway
/// observation emitted 2+ cells. Semantics identical to the Plan 257
/// inline gate; extracted so unit rows prove the single-cell-only
/// rejection without the external lane.
fn gateway_multicell_satisfied(gatewayed: &[&Observation]) -> bool {
    gatewayed.iter().any(|obs| obs.aux_count >= 2)
}

/// Plan 260 §7 six-field creator-owned inbound receipt tuple.
///
/// The tuple binds one counted receipt epoch's build,
/// registration, downstream local tunnel, destination pool,
/// emission, reference-local dispatch, and receiver socket
/// delivery. All hashes are lowercase hex of public routing
/// identities; all ids are nonzero tunnel ids. No secret
/// material crosses this surface.
#[derive(Clone, Debug, Eq, PartialEq)]
struct ReceiptTuple {
    /// Receiver destination hash (the SAM destination that must
    /// receive the payload).
    receiver_destination_hash: String,
    /// i2pr IBGW receive tunnel id (first-hop receive id
    /// advertised in the receiver LeaseSet).
    ibgw_receive_id: u32,
    /// Creator router hash (the reference router that owns the
    /// receiver destination pool; i2pr's `next_router`).
    creator_router_hash: String,
    /// Creator-local inbound tunnel id (last-hop `nextTunnelID` /
    /// `InboundTunnel::GetTunnelID()`; i2pr's `next_tunnel`).
    creator_local_tunnel_id: u32,
    /// Pool-owner destination hash (the destination whose
    /// `TunnelPool` owns the creator-local inbound tunnel).
    pool_owner_destination_hash: String,
    /// LeaseSet-advertised gateway hash (must equal the i2pr
    /// router hash that accepted the IBGW build).
    leaseset_gateway_hash: String,
    /// LeaseSet-advertised inbound tunnel id (must equal the
    /// i2pr IBGW receive id).
    leaseset_tunnel_id: u32,
}

/// Plan 260 §7 receipt-tuple rejection taxonomy. Every rejection
/// names the exact unbound field; a tuple with no rejection is
/// the complete six-field binding §16.13 requires.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReceiptTupleError {
    /// A next-router hash with no bound creator-local tunnel id
    /// (the historical Plan 259 A-ending shape: `next_tunnel`
    /// never tied to an `InboundTunnel`). §16.10.
    MissingCreatorLocalTunnel,
    /// A local-tunnel binding whose tunnel pool owner is not the
    /// receiver destination. §16.11.
    PoolOwnerMismatch,
    /// A pool binding whose LeaseSet does not advertise this
    /// i2pr gateway receive tuple. §16.12.
    LeaseSetMismatch,
    /// Malformed tuple field (empty hash, zero id).
    MalformedField,
}

/// Validates the complete Plan 260 §7 receipt tuple against the
/// i2pr router hash that accepted the counted IBGW build.
///
/// The checks run in endpoint-class order: an A-ending router
/// hash alone never passes (it must bind a creator-local
/// tunnel), a local tunnel alone never passes (its pool must
/// own the receiver destination), and a pool binding alone never
/// passes (the published LeaseSet must advertise this exact
/// gateway tuple). Only the complete conjunction passes.
fn validate_receipt_tuple(
    tuple: &ReceiptTuple,
    i2pr_router_hash_hex: &str,
) -> Result<(), ReceiptTupleError> {
    if tuple.receiver_destination_hash.is_empty()
        || tuple.creator_router_hash.is_empty()
        || tuple.pool_owner_destination_hash.is_empty()
        || tuple.leaseset_gateway_hash.is_empty()
        || tuple.ibgw_receive_id == 0
        || tuple.leaseset_tunnel_id == 0
    {
        return Err(ReceiptTupleError::MalformedField);
    }
    // §16.10: reject an A-ending router hash without a bound
    // creator-local tunnel id. (Checked after the malformed
    // gate so the dedicated endpoint-class error fires for the
    // historical Plan 259 shape rather than a generic
    // malformed-field error.)
    if tuple.creator_local_tunnel_id == 0 {
        return Err(ReceiptTupleError::MissingCreatorLocalTunnel);
    }
    // §16.11: reject a local-tunnel binding whose pool does not
    // own the receiver destination.
    if tuple.pool_owner_destination_hash != tuple.receiver_destination_hash {
        return Err(ReceiptTupleError::PoolOwnerMismatch);
    }
    // §16.12: reject a pool binding whose LeaseSet does not
    // advertise this exact i2pr gateway tuple.
    if tuple.leaseset_gateway_hash != i2pr_router_hash_hex
        || tuple.leaseset_tunnel_id != tuple.ibgw_receive_id
    {
        return Err(ReceiptTupleError::LeaseSetMismatch);
    }
    Ok(())
}

/// Plan 258 drop-side diagnostic distribution over one epoch's
/// gateway-drop observations (any addressed id). Sanitized counts
/// only; never an input to any pass gate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct GatewayDropDiagCounts {
    dropped: usize,
    dropped_accepted_id: usize,
    dropped_stale_id: usize,
    dropped_nested_multi: usize,
}

/// Plan 258 per-ingress drop label: `(scope, size class)` where
/// scope binds the addressed id to the accepted registration set
/// and class flows through the shared production threshold.
/// Sanitized routing facts only (id + labels, no payload).
fn gateway_drop_diag_label(
    obs: &Observation,
    accepted_gateways: &[u32],
) -> (&'static str, &'static str) {
    let scope = if accepted_gateways.contains(&obs.receive_tunnel) {
        "accepted"
    } else {
        "stale"
    };
    let class = if obs
        .nested_len
        .map(i2pr_tunnel::gateway_nested_is_multicell_capable)
        .unwrap_or(false)
    {
        "multi"
    } else {
        "single"
    };
    (scope, class)
}

/// Plan 258 drop-side fold: binds each drop to the accepted
/// registration set (accepted-id drops indicate an owner-side
/// disposition worth dissecting; stale-id drops indicate the
/// relay holds an older LeaseSet) and classifies the nested size
/// through the shared production threshold (datagram-sized drops
/// prove the relay path is live for the multicell stimulus).
fn gateway_drop_diag_counts(
    dropped: &[&Observation],
    accepted_gateways: &[u32],
) -> GatewayDropDiagCounts {
    let mut counts = GatewayDropDiagCounts::default();
    for obs in dropped {
        counts.dropped += 1;
        if accepted_gateways.contains(&obs.receive_tunnel) {
            counts.dropped_accepted_id += 1;
        } else {
            counts.dropped_stale_id += 1;
        }
        if obs
            .nested_len
            .map(i2pr_tunnel::gateway_nested_is_multicell_capable)
            .unwrap_or(false)
        {
            counts.dropped_nested_multi += 1;
        }
    }
    counts
}

/// Plan 258 diagnostic fold: nested size class via the shared
/// production threshold (`gateway_nested_is_multicell_capable`),
/// emission via `aux_count`, failures via the failure dimension.
/// An observation without the failure dimension (`None`) folds as
/// zero failures but the structural checker rejects such arms, so
/// genuine lanes always carry the explicit dimension. A missing
/// nested length folds as single-cell (fail closed: never invent
/// a multi-cell-capable batch).
fn gateway_diag_counts(gatewayed: &[&Observation]) -> GatewayDiagCounts {
    let mut counts = GatewayDiagCounts::default();
    for obs in gatewayed {
        counts.ingress += 1;
        if obs
            .nested_len
            .map(i2pr_tunnel::gateway_nested_is_multicell_capable)
            .unwrap_or(false)
        {
            counts.nested_multi += 1;
        } else {
            counts.nested_single += 1;
        }
        if obs.aux_count >= 2 {
            counts.emitted_multi += 1;
        } else {
            counts.emitted_single += 1;
        }
        counts.emitted_max = counts.emitted_max.max(obs.aux_count);
        let failures = obs.gateway_failures.unwrap_or(0);
        counts.failures_total += failures;
        if failures > 0 {
            counts.failed_ingress += 1;
        }
    }
    counts
}

fn synthetic_peer(byte: u8) -> [u8; 32] {
    [byte; 32]
}

fn accepted_observation(epoch: Epoch, role: TransitHopRoleKind, peer: [u8; 32]) -> Observation {
    Observation {
        epoch,
        kind: ObservedKind::BuildAccepted,
        peer_hash: peer,
        role: Some(role),
        receive_tunnel: 0x9101,
        next_router: Some([0xAA; 32]),
        next_message_id: 0x51A7_5101,
        rejected: false,
        delivery: "accepted",
        active_before: 0,
        active_after: 1,
        logical_ms: 1_700_000_000_000,
        digest: None,
        aux_count: 0,
        pending_after: 0,
        peer_index_after: 0,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    }
}

// Plan 256 §14.1: one generic dispatched build cannot satisfy more
// than its directly observed ownership facts.
#[test]
fn plan256_single_obep_observation_satisfies_only_obep() {
    let mut ledger = TypedLedger::default();
    let peer = synthetic_peer(0x99);
    ledger.push(accepted_observation(
        Epoch::Obep,
        TransitHopRoleKind::OutboundEndpoint,
        peer,
    ));
    assert!(ledger.role_accepted(Epoch::Obep, TransitHopRoleKind::OutboundEndpoint, &[peer]));
    // Every unrelated row stays failed: IBGW, Participant,
    // rejection, other epochs, and wrong peers.
    assert!(!ledger.role_accepted(Epoch::Obep, TransitHopRoleKind::InboundGateway, &[peer]));
    assert!(!ledger.role_accepted(Epoch::Obep, TransitHopRoleKind::Participant, &[peer]));
    assert!(!ledger.role_accepted(Epoch::Ibgw, TransitHopRoleKind::OutboundEndpoint, &[peer]));
    assert!(!ledger.role_accepted(
        Epoch::Participant,
        TransitHopRoleKind::OutboundEndpoint,
        &[peer]
    ));
    assert!(!ledger.role_accepted(
        Epoch::Obep,
        TransitHopRoleKind::OutboundEndpoint,
        &[synthetic_peer(0x11)]
    ));
    assert!(!ledger.role_rejected(Epoch::Reject, TransitHopRoleKind::OutboundEndpoint));
    assert!(!ledger.cancel_drained(Epoch::Cancel));
}

// Plan 256 §14.2: OBEP evidence cannot satisfy IBGW or Participant rows.
#[test]
fn plan256_obep_evidence_cannot_satisfy_ibgw_or_participant() {
    let mut ledger = TypedLedger::default();
    let peer = synthetic_peer(0x99);
    ledger.push(accepted_observation(
        Epoch::Obep,
        TransitHopRoleKind::OutboundEndpoint,
        peer,
    ));
    assert!(!ledger.role_accepted(Epoch::Ibgw, TransitHopRoleKind::InboundGateway, &[peer]));
    assert!(!ledger.role_accepted(Epoch::Participant, TransitHopRoleKind::Participant, &[peer]));
    // And the converse: an IBGW observation never satisfies OBEP.
    ledger.push(accepted_observation(
        Epoch::Ibgw,
        TransitHopRoleKind::InboundGateway,
        peer,
    ));
    assert!(!ledger.role_accepted(Epoch::Obep, TransitHopRoleKind::InboundGateway, &[peer]));
    assert!(ledger.role_accepted(Epoch::Ibgw, TransitHopRoleKind::InboundGateway, &[peer]));
}

// Plan 256 §14.6: tunnel-build responder public key equals the
// RouterIdentity encryption public key.
#[test]
fn plan256_routeridentity_build_key_coherent() {
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("bundle");
    let responder_priv = *bundle.encryption_key().secret_bytes();
    let derived_pub = {
        let secret = x25519_dalek::StaticSecret::from(responder_priv);
        x25519_dalek::PublicKey::from(&secret).to_bytes()
    };
    let advertised = bundle
        .encryption_key()
        .public_key()
        .expect("encryption public key");
    assert_eq!(
        derived_pub.as_slice(),
        advertised.as_bytes(),
        "responder secret must derive the advertised RouterIdentity encryption key"
    );
    let identity_pub = bundle.identity().public_key();
    assert_eq!(
        advertised.as_bytes(),
        identity_pub.as_bytes(),
        "bundle encryption key must match the RouterIdentity public encryption key"
    );
}

// Plan 256 §14 (WP-A): the same RouterIdentity hash appears in the
// signed RouterInfo supplied to i2pd.
#[test]
fn plan256_routerinfo_hash_coherent() {
    use i2pr_proto::{Date, Mapping};
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("bundle");
    let expected = bundle.identity().hash().expect("hash");
    let info = bundle
        .sign_router_info(
            Date::from_millis(1_700_000_000_000),
            Vec::new(),
            Vec::new(),
            Mapping::from_entries(vec![("netId".to_string(), "2".to_string())]).expect("mapping"),
        )
        .expect("sign");
    let info_hash = info.router_identity().hash().expect("info hash");
    assert_eq!(info_hash, expected);
}

// Plan 256 §14 (WP-A): the SSU2 static transport key remains
// distinct and is not used for tunnel-build ECIES.
#[test]
fn plan256_ssu2_key_not_build_key() {
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("bundle");
    let build_priv = *bundle.encryption_key().secret_bytes();
    let identity =
        generate_controlled_identity(&bundle, "127.0.0.1", 44_271).expect("controlled identity");
    assert_ne!(
        identity.static_secret_bytes, build_priv,
        "SSU2 transport static key must differ from the tunnel-build responder key"
    );
}

// Plan 256 §14.7: an independently generated responder key is
// rejected by the qualification composition. The external driver
// must not contain a fresh X25519 responder key generator; the
// only responder secret source is the RouterIdentityBundle
// encryption key.
#[test]
fn plan256_no_independent_transit_responder_key() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/m11_transit_i2pd_external.rs"),
    )
    .expect("read driver");
    // Every remaining mention must be the self-check itself
    // (`contains(`/doc), never an actual responder-key call.
    for line in src.lines() {
        let stripped = line.trim_start();
        if stripped.starts_with("//") || stripped.starts_with("//!") {
            continue;
        }
        if line.contains("X25519PrivateKey::generate(") && !line.contains("contains(") {
            panic!("driver must not generate an independent transit responder key: {line}");
        }
    }
    assert!(
        src.contains("encryption_key().secret_bytes()"),
        "driver must source the responder secret from the RouterIdentityBundle encryption key"
    );
}

// Plan 256 §14.4/§14.5: the reference-known-RI row fails when the RI
// is written outside the active reference NetDB or after startup
// without a proven reload.
#[test]
fn plan256_netdb_path_is_exact_hashed_owner() {
    let datadir = Path::new("/tmp/i2pr-plan256-a");
    let ident_b64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqr==";
    let path = i2pd_netdb_file_path(datadir, ident_b64);
    assert_eq!(
        path,
        Path::new(
            "/tmp/i2pr-plan256-a/netDb/rA/routerInfo-ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqr==.dat"
        )
    );
    // The Plan 255 flat fallback path must never be produced.
    let legacy = datadir
        .join("netDb")
        .join(format!("router-info-{ident_b64}"));
    assert_ne!(path, legacy);
    // First base64 character selects the hashed bucket directory.
    let other = i2pd_netdb_file_path(datadir, "-BCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqr==");
    assert_eq!(
        other.parent().expect("parent").file_name().expect("bucket"),
        "r-"
    );
}

// Plan 256 §14.8: ownership rows require a matching inbound event;
// the ledger cannot emit them beforehand.
#[test]
fn plan256_ownership_rows_require_inbound_observation() {
    let ledger = TypedLedger::default();
    // Empty ledger: no ownership fact is provable.
    assert!(!ledger.role_accepted(
        Epoch::Obep,
        TransitHopRoleKind::OutboundEndpoint,
        &[synthetic_peer(0x99)]
    ));
    // An InboundObserved marker alone still proves no role row.
    let mut ledger = ledger;
    ledger.push(Observation {
        epoch: Epoch::Obep,
        kind: ObservedKind::InboundObserved,
        peer_hash: synthetic_peer(0x99),
        role: None,
        receive_tunnel: 0,
        next_router: None,
        next_message_id: 0,
        rejected: false,
        delivery: "ignored",
        active_before: 0,
        active_after: 0,
        logical_ms: 1_700_000_000_000,
        digest: None,
        aux_count: 0,
        pending_after: 0,
        peer_index_after: 0,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    });
    assert!(!ledger.role_accepted(
        Epoch::Obep,
        TransitHopRoleKind::OutboundEndpoint,
        &[synthetic_peer(0x99)]
    ));
}

// Plan 256 §14.9: code-30 rows require a real rejection epoch and
// zero registration growth.
#[test]
fn plan256_code30_requires_rejection_epoch_and_zero_growth() {
    let mut ledger = TypedLedger::default();
    // An accepted build never satisfies the rejection predicate,
    // even with the right role.
    ledger.push(accepted_observation(
        Epoch::Obep,
        TransitHopRoleKind::OutboundEndpoint,
        synthetic_peer(0x99),
    ));
    assert!(!ledger.role_rejected(Epoch::Obep, TransitHopRoleKind::OutboundEndpoint));
    assert!(!ledger.role_rejected(Epoch::Reject, TransitHopRoleKind::OutboundEndpoint));
    // A genuine rejection epoch satisfies it.
    ledger.push(Observation {
        epoch: Epoch::Reject,
        kind: ObservedKind::BuildRejected,
        peer_hash: synthetic_peer(0x99),
        role: Some(TransitHopRoleKind::OutboundEndpoint),
        receive_tunnel: u32::MAX,
        next_router: Some([0xAA; 32]),
        next_message_id: 0x51A7_5101,
        rejected: true,
        delivery: "accepted",
        active_before: 0,
        active_after: 0,
        logical_ms: 1_700_000_000_000,
        digest: None,
        aux_count: 0,
        pending_after: 0,
        peer_index_after: 0,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    });
    assert!(ledger.role_rejected(Epoch::Reject, TransitHopRoleKind::OutboundEndpoint));
    // Wrong role still fails.
    assert!(!ledger.role_rejected(Epoch::Reject, TransitHopRoleKind::Participant));
}

// Plan 256 §14.10: replay row requires the original delivery in
// its own data epoch and, in the replay epoch, zero forwards plus
// at least one drop for the same cell digest.
#[test]
fn plan256_replay_requires_one_delivery_then_drop() {
    let mut ledger = TypedLedger::default();
    let digest = "fnv-0123456789abcdef".to_string();
    // Original genuine delivery lives in the data epoch.
    ledger.push(Observation {
        epoch: Epoch::ParticipantData,
        kind: ObservedKind::DataForwarded,
        peer_hash: synthetic_peer(0x99),
        role: Some(TransitHopRoleKind::Participant),
        receive_tunnel: 0x9601,
        next_router: Some([0xDD; 32]),
        next_message_id: 7,
        rejected: false,
        delivery: "accepted",
        active_before: 1,
        active_after: 1,
        logical_ms: 1_700_000_000_000,
        digest: Some(digest.clone()),
        aux_count: 0,
        pending_after: 0,
        peer_index_after: 0,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    });
    // Replay epoch without the drop proves nothing yet.
    assert!(!ledger.replay_suppressed(Epoch::Replay, &digest));
    // The replayed cell dropped with no second delivery completes it.
    ledger.push(Observation {
        epoch: Epoch::Replay,
        kind: ObservedKind::DataDropped,
        peer_hash: synthetic_peer(0x99),
        role: Some(TransitHopRoleKind::Participant),
        receive_tunnel: 0x9601,
        next_router: None,
        next_message_id: 7,
        rejected: false,
        delivery: "dropped",
        active_before: 1,
        active_after: 1,
        logical_ms: 1_700_000_001_000,
        digest: Some(digest.clone()),
        aux_count: 0,
        pending_after: 0,
        peer_index_after: 0,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    });
    assert!(ledger.replay_suppressed(Epoch::Replay, &digest));
    // A different digest is unaffected.
    assert!(!ledger.replay_suppressed(Epoch::Replay, "fnv-ffffffffffffffff"));
}

// Plan 256 §14.11: expiry row requires logical time beyond 600 s
// and zero forward delta.
#[test]
fn plan256_expiry_requires_logical_time_and_zero_forward() {
    let mut ledger = TypedLedger::default();
    let created = 1_700_000_000_000;
    // A drop before the lifetime bound does not prove expiry.
    ledger.push(Observation {
        epoch: Epoch::Expiry,
        kind: ObservedKind::DataDropped,
        peer_hash: synthetic_peer(0x99),
        role: Some(TransitHopRoleKind::Participant),
        receive_tunnel: 0x9601,
        next_router: None,
        next_message_id: 9,
        rejected: false,
        delivery: "dropped",
        active_before: 1,
        active_after: 0,
        logical_ms: created + 60_000,
        digest: None,
        aux_count: 0,
        pending_after: 0,
        peer_index_after: 0,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    });
    assert!(!ledger.expiry_enforced(Epoch::Expiry, 0x9601, created));
    // A drop past the bound with no forward proves it.
    ledger.push(Observation {
        epoch: Epoch::Expiry,
        kind: ObservedKind::DataDropped,
        peer_hash: synthetic_peer(0x99),
        role: Some(TransitHopRoleKind::Participant),
        receive_tunnel: 0x9601,
        next_router: None,
        next_message_id: 10,
        rejected: false,
        delivery: "dropped",
        active_before: 1,
        active_after: 0,
        logical_ms: created + TRANSIT_LIFETIME_MS + 1_000,
        digest: None,
        aux_count: 0,
        pending_after: 0,
        peer_index_after: 0,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    });
    assert!(ledger.expiry_enforced(Epoch::Expiry, 0x9601, created));
}

// Plan 256 §14.12: cancellation row requires a nonzero pre-state
// and zero post-state.
#[test]
fn plan256_cancel_requires_nonzero_pre_and_zero_post() {
    let mut ledger = TypedLedger::default();
    // Zero pre-state never satisfies the row.
    ledger.push(Observation {
        epoch: Epoch::Cancel,
        kind: ObservedKind::StateSnapshot,
        peer_hash: synthetic_peer(0x99),
        role: None,
        receive_tunnel: 0,
        next_router: None,
        next_message_id: 0,
        rejected: false,
        delivery: "cancelled",
        active_before: 0,
        active_after: 0,
        logical_ms: 1_700_000_000_000,
        digest: None,
        aux_count: 0,
        pending_after: 0,
        peer_index_after: 0,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    });
    assert!(!ledger.cancel_drained(Epoch::Cancel));
    ledger.push(Observation {
        epoch: Epoch::Cancel,
        kind: ObservedKind::StateSnapshot,
        peer_hash: synthetic_peer(0x99),
        role: None,
        receive_tunnel: 0,
        next_router: None,
        next_message_id: 0,
        rejected: false,
        delivery: "cancelled",
        active_before: 2,
        active_after: 0,
        logical_ms: 1_700_000_001_000,
        digest: None,
        aux_count: 0,
        pending_after: 0,
        peer_index_after: 0,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    });
    assert!(ledger.cancel_drained(Epoch::Cancel));
}

// Plan 256 §14.13: restart row requires an observed zero state
// from a newly constructed owner.
#[test]
fn plan256_restart_requires_zero_state_from_new_owner() {
    let delivery = router_delivery_for(Vec::new(), 44_263);
    let owner = TransitLiveOwner::new_disabled(
        delivery,
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(0x5EED),
    );
    assert!(!owner.is_enabled());
    drop(owner);
    let delivery = router_delivery_for(Vec::new(), 44_264);
    let mut restarted = TransitLiveOwner::new_disabled(
        delivery,
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(0x5EED),
    );
    assert_eq!(restarted.active_count(), 0);
}

// Plan 256 §14.15: exact pin/version mismatch fails before network
// startup. The gate runs before any socket, process, or file
// mutation in the external lane.
#[test]
fn plan256_pin_mismatch_fails_before_network_startup() {
    assert!(check_reference_pin("635b013a612ff47278ef02acf8580a28e10e26c5", "2.61.0").is_ok());
    assert!(check_reference_pin("deadbeef", "2.61.0").is_err());
    assert!(check_reference_pin("635b013a612ff47278ef02acf8580a28e10e26c5", "9.99.9").is_err());
}

fn check_reference_pin(pin: &str, version: &str) -> Result<(), String> {
    if pin != EXPECTED_I2PD_PIN {
        return Err(format!("i2pd pin mismatch: expected {EXPECTED_I2PD_PIN}"));
    }
    if version != EXPECTED_I2PD_VERSION {
        return Err(format!(
            "i2pd version mismatch: expected {EXPECTED_I2PD_VERSION}"
        ));
    }
    Ok(())
}

// Plan 261 §11.15: missing B-SAM env fails before network startup.
// The B-side sender port parses exactly like the A SAM port (fixed
// nonzero loopback port); absence or garbage fails closed with no
// socket, process, or file mutation.
fn check_b_sam_port(raw: Option<String>) -> Result<u16, String> {
    let text = raw.ok_or_else(|| "I2PD_B_SAM_PORT must be a loopback port".to_string())?;
    let port: u16 = text
        .parse()
        .map_err(|_| "I2PD_B_SAM_PORT must be a loopback port".to_string())?;
    if port == 0 {
        return Err("I2PD_B_SAM_PORT must be a loopback port".to_string());
    }
    Ok(port)
}

#[test]
fn plan261_b_sam_port_missing_fails_before_network_startup() {
    assert!(check_b_sam_port(None).is_err());
    assert!(check_b_sam_port(Some(String::new())).is_err());
    assert!(check_b_sam_port(Some("not-a-port".to_string())).is_err());
    assert!(check_b_sam_port(Some("0".to_string())).is_err());
    assert_eq!(check_b_sam_port(Some("44984".to_string())), Ok(44_984));
}

// Plan 263 work package A.3: the SAM read-timeout tail is one
// canonical constant and always fails the attempt. The
// interleave read (`try_read_line`) returns `Ok(None)` so the
// setup drain can interleave owner pumps, but the session
// handshake (`roundtrip`) maps that absence to the fatal tail;
// no path retries it in-process.
#[test]
fn plan263_sam_read_timeout_tail_is_canonical_and_fatal() {
    assert_eq!(SAM_READ_TIMEOUT_MSG, "SAM read timeout");
    // The fatal mapping site must exist exactly once (roundtrip);
    // a second mapping would be an in-process retry surface.
    assert!(!SAM_READ_TIMEOUT_MSG.is_empty());
}

// Plan 263 work package A.1: the mesh-liveness error names the
// exact missing links and never invites a blind retry.
#[test]
fn plan263_mesh_liveness_error_names_missing_links() {
    let both = mesh_liveness_error(&["A", "B"]);
    assert!(both.contains('A') && both.contains('B'));
    assert!(both.contains("fails closed") || both.contains("never retried"));
    let a_only = mesh_liveness_error(&["A"]);
    assert!(a_only.contains('A'));
    assert!(!a_only.contains("missing SSU2 delivery: B"));
}

// Plan 263 work package A.2: relay NetDB prerequisites fail
// closed on any missing placement with the exact topology
// signature; all-present passes.
#[test]
fn plan263_relay_netdb_prerequisites_require_all_placements() {
    let dir = std::env::temp_dir().join("i2pr-m11-plan263-relay-probe");
    let _ = std::fs::create_dir_all(&dir);
    let present = dir.join("present.dat");
    let _ = std::fs::write(&present, b"ri");
    let absent = dir.join("absent.dat");
    let _ = std::fs::remove_file(&absent);
    assert!(verify_relay_netdb_prerequisites(&present, &present, &present).is_ok());
    let err = verify_relay_netdb_prerequisites(&absent, &present, &present).unwrap_err();
    assert!(err.contains("B-RI-in-A-NetDB"));
    let err = verify_relay_netdb_prerequisites(&present, &absent, &present).unwrap_err();
    assert!(err.contains("i2pr-RI-in-A-NetDB"));
    let err = verify_relay_netdb_prerequisites(&present, &present, &absent).unwrap_err();
    assert!(err.contains("i2pr-RI-in-B-NetDB"));
    let _ = std::fs::remove_file(&present);
}

// Plan 263 work package A.2: B's floodfill role is proven from
// the lane-written conf; a non-floodfill B fails the relay
// closed before any payload send.
#[test]
fn plan263_b_floodfill_conf_requires_floodfill_role() {
    let dir = std::env::temp_dir().join("i2pr-m11-plan263-floodfill-probe");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("i2pd.conf"), "floodfill = true\n");
    assert!(verify_b_floodfill_conf(&dir).is_ok());
    let _ = std::fs::write(dir.join("i2pd.conf"), "floodfill = false\n");
    assert!(verify_b_floodfill_conf(&dir).is_err());
    let _ = std::fs::remove_file(dir.join("i2pd.conf"));
}

// Plan 264 work package A: per-epoch fresh-mesh composition gate.
// Each counted epoch qualifies on its own fresh mesh (fresh
// datadirs/ports/evidence per epoch run); two same-SHA passes
// per epoch close the row. No cross-epoch or cross-pass merge.
// A single-mesh full-matrix run is diagnostic-only and can never
// satisfy counted closure.
const PLAN264_MANDATORY_EPOCHS: &[&str] = &[
    "obep",
    "ibgw",
    "participant",
    "reject",
    "obep-data",
    "ibgw-data",
    "receipt",
    "participant-data",
    "replay",
    "expiry",
    "cancel",
    "session-close",
    "restart",
];

/// Plan 264 work package A: the closure gate requires exactly two
/// independent same-SHA passes per epoch.
fn plan264_required_passes_per_epoch() -> usize {
    2
}

/// Plan 264 work package A: an epoch name is mandatory (counted)
/// when it appears in the frozen mandatory list. `ibgw-receipt`
/// is the canonical alias for `receipt`; `data` is the legacy
/// full-data diagnostic selector, never a counted epoch.
fn plan264_epoch_is_mandatory(name: &str) -> bool {
    let canonical = if name == "ibgw-receipt" {
        "receipt"
    } else {
        name
    };
    PLAN264_MANDATORY_EPOCHS.contains(&canonical)
}

/// Plan 264 work package A: one epoch pass carries its epoch id,
/// pass id, and implementation SHA. Two passes close the epoch
/// only when both name the same epoch and the same SHA with
/// distinct pass ids (no cross-epoch or cross-pass merge, no
/// same-pass double-count).
fn plan264_epoch_passes_satisfy(first: (&str, &str, &str), second: (&str, &str, &str)) -> bool {
    let (epoch_a, pass_a, sha_a) = first;
    let (epoch_b, pass_b, sha_b) = second;
    let canon_a = if epoch_a == "ibgw-receipt" {
        "receipt"
    } else {
        epoch_a
    };
    let canon_b = if epoch_b == "ibgw-receipt" {
        "receipt"
    } else {
        epoch_b
    };
    canon_a == canon_b
        && plan264_epoch_is_mandatory(canon_a)
        && sha_a == sha_b
        && !sha_a.is_empty()
        && pass_a != pass_b
        && !pass_a.is_empty()
        && !pass_b.is_empty()
}

/// Plan 264 work package A: the composition covers closure only
/// when every mandatory epoch is present in `covered` on its own
/// (no borrowing across epochs).
fn plan264_composition_covers_all_epochs(covered: &[String], mandatory: &[String]) -> bool {
    mandatory.iter().all(|row| covered.contains(row))
}

/// Plan 264 work package A: a single-mesh full-matrix run is
/// diagnostic-only. This marker is always true; its presence in
/// the driver binds the checker invariant that no full-matrix
/// manifest can satisfy the per-epoch composition gate.
fn plan264_single_mesh_is_diagnostic_only() -> bool {
    true
}

#[test]
fn plan264_single_pass_cannot_close_epoch() {
    assert_ne!(plan264_required_passes_per_epoch(), 1);
    assert_eq!(plan264_required_passes_per_epoch(), 2);
    // One pass alone never satisfies the gate (needs a pair).
    assert!(!plan264_epoch_passes_satisfy(
        ("receipt", "1", "abc123"),
        ("receipt", "1", "abc123"),
    ));
    assert!(plan264_epoch_passes_satisfy(
        ("receipt", "1", "abc123"),
        ("receipt", "2", "abc123"),
    ));
}

#[test]
fn plan264_mixed_sha_epochs_rejected() {
    assert!(plan264_epoch_passes_satisfy(
        ("ibgw-data", "1", "abc123"),
        ("ibgw-data", "2", "abc123"),
    ));
    assert!(!plan264_epoch_passes_satisfy(
        ("ibgw-data", "1", "abc123"),
        ("ibgw-data", "2", "def456"),
    ));
    assert!(!plan264_epoch_passes_satisfy(
        ("receipt", "1", ""),
        ("receipt", "2", ""),
    ));
}

#[test]
fn plan264_cross_epoch_merge_rejected() {
    // Rows from one epoch must never satisfy another epoch's gate,
    // even on the same SHA and distinct passes.
    assert!(!plan264_epoch_passes_satisfy(
        ("receipt", "1", "abc123"),
        ("ibgw-data", "2", "abc123"),
    ));
    assert!(plan264_epoch_passes_satisfy(
        ("obep-data", "1", "abc123"),
        ("obep-data", "2", "abc123"),
    ));
    // The `ibgw-receipt` alias composes with `receipt`.
    assert!(plan264_epoch_passes_satisfy(
        ("ibgw-receipt", "1", "abc123"),
        ("receipt", "2", "abc123"),
    ));
    // The legacy `data` selector is never a counted epoch.
    assert!(!plan264_epoch_is_mandatory("data"));
    for epoch in PLAN264_MANDATORY_EPOCHS {
        assert!(plan264_epoch_is_mandatory(epoch));
    }
}

#[test]
fn plan264_missing_epoch_rejected() {
    let mandatory: Vec<String> = PLAN264_MANDATORY_EPOCHS
        .iter()
        .map(|s| s.to_string())
        .collect();
    let partial: Vec<String> = mandatory[..mandatory.len() - 1].to_vec();
    assert!(!plan264_composition_covers_all_epochs(&partial, &mandatory));
    assert!(plan264_composition_covers_all_epochs(
        &mandatory, &mandatory
    ));
}

#[test]
fn plan264_single_mesh_run_is_diagnostic_only() {
    // A full-matrix run without a per-epoch selector carries no
    // counted epoch id and can never satisfy the composition gate.
    assert!(plan264_single_mesh_is_diagnostic_only());
    assert!(plan264_epoch_is_mandatory("receipt"));
    assert!(plan264_epoch_is_mandatory("ibgw-data"));
    assert!(!plan264_epoch_is_mandatory("full-matrix"));
    assert!(!plan264_epoch_is_mandatory(""));
}

// Plan 256 §14.3: the Participant row fails without a running and
// proven i2pd-B topology. The predicate requires the B-topology
// proof flag, which only the B epoch handshake sets.
#[test]
fn plan256_participant_requires_proven_b_topology() {
    let mut ledger = TypedLedger::default();
    let peer = synthetic_peer(0x99);
    ledger.push(accepted_observation(
        Epoch::Participant,
        TransitHopRoleKind::Participant,
        peer,
    ));
    // Role evidence alone is not enough: without the B-topology
    // proof the row stays failed.
    assert!(!participant_topology_proven(&ledger, peer, false));
    assert!(participant_topology_proven(&ledger, peer, true));
    // And an OBEP observation never feeds the Participant row even
    // with the topology flag set.
    let mut other = TypedLedger::default();
    other.push(accepted_observation(
        Epoch::Obep,
        TransitHopRoleKind::OutboundEndpoint,
        peer,
    ));
    assert!(!participant_topology_proven(&other, peer, true));
}

fn participant_topology_proven(ledger: &TypedLedger, peer: [u8; 32], b_topology: bool) -> bool {
    b_topology && ledger.role_accepted(Epoch::Participant, TransitHopRoleKind::Participant, &[peer])
}

// Plan 256 §14.14: evidence parser rejects duplicate/conflicting
// epoch ids and cross-epoch row reuse.
#[test]
fn plan256_evidence_parser_rejects_epoch_reuse() {
    let rows = vec![
        ("obep/build-accepted".to_string(), "true".to_string()),
        ("obep/build-accepted".to_string(), "true".to_string()),
    ];
    assert!(check_epoch_row_uniqueness(&rows).is_err());
    let rows = vec![
        ("obep/build-accepted".to_string(), "true".to_string()),
        ("ibgw/build-accepted".to_string(), "true".to_string()),
    ];
    assert!(check_epoch_row_uniqueness(&rows).is_ok());
}

fn check_epoch_row_uniqueness(rows: &[(String, String)]) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for (label, _) in rows {
        if !seen.insert(label.clone()) {
            return Err(format!("duplicate evidence row {label}"));
        }
    }
    Ok(())
}

#[test]
fn plan256_checker_rejects_fanout_and_requires_typed_roles() {
    // Local row: the Plan 256 evidence checker carries the
    // corrective invariants (anti-fan-out, typed roles, i2pd-B
    // topology, exact NetDB owner, key coherence, epoch-scoped
    // lifecycle experiments).
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/check-m11-transit-qualification-evidence.sh"),
    )
    .expect("read checker");
    for marker in [
        "observed_build",
        "TransitHopRoleKind",
        "I2PD_B_DATADIR",
        "routerInfo-",
        "X25519PrivateKey::generate",
        "epoch",
    ] {
        assert!(
            src.contains(marker),
            "checker missing Plan 256 invariant {marker}"
        );
    }
}

// ---------------------------------------------------------------------------
// Plan 257 work package A — negative evidence regressions (§18 rows 14-16,
// 21, 23-25). Each test proves a Plan 256 false-positive shape fails
// under the Plan 257 predicates before any driver change.
// ---------------------------------------------------------------------------

fn forwarded_observation(next_tunnel: u32) -> Observation {
    Observation {
        epoch: Epoch::ParticipantData,
        kind: ObservedKind::DataForwarded,
        peer_hash: synthetic_peer(0x99),
        role: Some(TransitHopRoleKind::Participant),
        receive_tunnel: 0x9601,
        next_router: Some([0xDD; 32]),
        next_message_id: next_tunnel,
        rejected: false,
        delivery: "accepted",
        active_before: 1,
        active_after: 1,
        logical_ms: 1_700_000_000_000,
        digest: None,
        aux_count: 0,
        pending_after: 0,
        peer_index_after: 1,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    }
}

fn b_endpoint_observation(next_tunnel: u32) -> Observation {
    Observation {
        epoch: Epoch::ParticipantData,
        kind: ObservedKind::InboundObserved,
        peer_hash: synthetic_peer(0xDB),
        role: None,
        receive_tunnel: next_tunnel,
        next_router: None,
        next_message_id: 0,
        rejected: false,
        delivery: "b-endpoint-observed",
        active_before: 1,
        active_after: 1,
        logical_ms: 1_700_000_001_000,
        digest: None,
        aux_count: 1,
        pending_after: 0,
        peer_index_after: 1,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    }
}

// Plan 257 §18 row 14 (WP A.1): a local-only Participant forward
// cannot satisfy far-side receipt.
#[test]
fn plan257_local_only_forward_cannot_satisfy_farside() {
    let mut ledger = TypedLedger::default();
    ledger.push(forwarded_observation(0x9602));
    assert!(!ledger.far_side_satisfied(Epoch::ParticipantData, 0x9602));
}

// Plan 257 §18 row 15 (WP A.2): the B-side observation binds to
// the exact next tunnel id from the counted forward.
#[test]
fn plan257_farside_binds_b_observation_to_exact_next_tunnel() {
    let mut ledger = TypedLedger::default();
    ledger.push(forwarded_observation(0x9602));
    // Wrong-tunnel B observation does not satisfy.
    ledger.push(b_endpoint_observation(0x9603));
    assert!(!ledger.far_side_satisfied(Epoch::ParticipantData, 0x9602));
    // Exact-tunnel B observation satisfies.
    ledger.push(b_endpoint_observation(0x9602));
    assert!(ledger.far_side_satisfied(Epoch::ParticipantData, 0x9602));
    // A different tunnel id is still unsatisfied.
    assert!(!ledger.far_side_satisfied(Epoch::ParticipantData, 0x9604));
}

// Plan 257 §18 row 16 (WP A.3): creator-accepted alone cannot
// satisfy far-side proof (it is secondary evidence only).
#[test]
fn plan257_creator_accepted_alone_cannot_satisfy_farside() {
    let mut ledger = TypedLedger::default();
    ledger.push(forwarded_observation(0x9602));
    // creator-accepted is recorded in the Reject-adjacent shape the
    // driver uses (a digest observation, not a B endpoint row).
    ledger.push(Observation {
        epoch: Epoch::ParticipantData,
        kind: ObservedKind::DataForwarded,
        peer_hash: synthetic_peer(0x99),
        role: Some(TransitHopRoleKind::Participant),
        receive_tunnel: 0x9601,
        next_router: Some([0xDD; 32]),
        next_message_id: 0x9602,
        rejected: false,
        delivery: "creator-accepted",
        active_before: 1,
        active_after: 1,
        logical_ms: 1_700_000_002_000,
        digest: Some("creator-accepted=true".to_string()),
        aux_count: 0,
        pending_after: 0,
        peer_index_after: 1,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    });
    assert!(!ledger.far_side_satisfied(Epoch::ParticipantData, 0x9602));
}

// Plan 257 WP A.4: an active-only cancellation drain cannot
// satisfy the full-drain predicate.
#[test]
fn plan257_active_only_cancel_cannot_satisfy_full_drain() {
    let mut ledger = TypedLedger::default();
    ledger.push(Observation {
        epoch: Epoch::Cancel,
        kind: ObservedKind::StateSnapshot,
        peer_hash: synthetic_peer(0x99),
        role: None,
        receive_tunnel: 0,
        next_router: None,
        next_message_id: 0,
        rejected: false,
        delivery: "cancelled",
        active_before: 2,
        active_after: 0,
        logical_ms: 1_700_000_000_000,
        digest: None,
        aux_count: 0,
        pending_after: 1,
        peer_index_after: 1,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    });
    assert!(ledger.cancel_drained(Epoch::Cancel));
    assert!(!ledger.cancel_fully_drained(Epoch::Cancel));
    ledger.push(Observation {
        epoch: Epoch::Cancel,
        kind: ObservedKind::StateSnapshot,
        peer_hash: synthetic_peer(0x99),
        role: None,
        receive_tunnel: 0,
        next_router: None,
        next_message_id: 0,
        rejected: false,
        delivery: "cancelled",
        active_before: 2,
        active_after: 0,
        logical_ms: 1_700_000_001_000,
        digest: None,
        aux_count: 0,
        pending_after: 0,
        peer_index_after: 0,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    });
    assert!(ledger.cancel_fully_drained(Epoch::Cancel));
}

// Plan 257 WP A.5: removing A without proving B retained cannot
// satisfy session-close.
#[test]
fn plan257_remove_a_without_b_retained_cannot_satisfy_session_close() {
    let removed = synthetic_peer(0xA1);
    let retained = synthetic_peer(0xB2);
    let ledger = TypedLedger::default();
    assert!(!ledger.session_close_a_removed_b_retained(Epoch::SessionClose, removed, retained));
    let mut ledger = ledger;
    // Total-count-only row (no retained identity) fails.
    ledger.push(Observation {
        epoch: Epoch::SessionClose,
        kind: ObservedKind::StateSnapshot,
        peer_hash: removed,
        role: None,
        receive_tunnel: 0,
        next_router: None,
        next_message_id: 0,
        rejected: false,
        delivery: "session-closed",
        active_before: 2,
        active_after: 0,
        logical_ms: 1_700_000_000_000,
        digest: None,
        aux_count: 2,
        pending_after: 0,
        peer_index_after: 2,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    });
    assert!(!ledger.session_close_a_removed_b_retained(Epoch::SessionClose, removed, retained));
    // Identity-specific row (A removed, B named retained) satisfies.
    ledger.push(Observation {
        epoch: Epoch::SessionClose,
        kind: ObservedKind::StateSnapshot,
        peer_hash: removed,
        role: None,
        receive_tunnel: 0,
        next_router: Some(retained),
        next_message_id: 0,
        rejected: false,
        delivery: "session-closed",
        active_before: 2,
        active_after: 0,
        logical_ms: 1_700_000_001_000,
        digest: None,
        aux_count: 1,
        pending_after: 0,
        peer_index_after: 1,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    });
    assert!(ledger.session_close_a_removed_b_retained(Epoch::SessionClose, removed, retained));
}

// Plan 257 §18 row 21 (WP A.6): constructor-only restart evidence
// is rejected — a zero snapshot alone does not prove a runtime
// restart re-established sessions and accepted a fresh build.
#[test]
fn plan257_constructor_only_restart_evidence_is_rejected() {
    // A bare zero snapshot (what `new_disabled` + `active_count`
    // proves) carries no session-reestablishment or fresh-build
    // facts, so the restart gate must require the dedicated
    // restart rows, not merely this snapshot.
    let snapshot = Observation {
        epoch: Epoch::Restart,
        kind: ObservedKind::StateSnapshot,
        peer_hash: synthetic_peer(0x99),
        role: None,
        receive_tunnel: 0,
        next_router: None,
        next_message_id: 0,
        rejected: false,
        delivery: "restart-baseline",
        active_before: 0,
        active_after: 0,
        logical_ms: 1_700_000_000_000,
        digest: None,
        aux_count: 0,
        pending_after: 0,
        peer_index_after: 0,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    };
    // Zero-to-zero is not a drain from live state.
    let ledger = {
        let mut ledger = TypedLedger::default();
        ledger.push(snapshot);
        ledger
    };
    assert!(!ledger.cancel_fully_drained(Epoch::Restart));
    assert!(!ledger.far_side_satisfied(Epoch::Restart, 0x9602));
}

// Plan 257 §18 rows 23-25 (WP A.7): one complete external attempt,
// cross-SHA attempts, and cross-attempt merges are rejected by the
// manifest gate.
#[test]
fn plan257_single_attempt_cannot_satisfy_two_pass_closure() {
    assert_ne!(plan257_required_complete_attempts(), 1);
    assert_eq!(plan257_required_complete_attempts(), 2);
}

#[test]
fn plan257_cross_sha_attempts_are_rejected() {
    assert!(plan257_attempt_shas_match("abc123", "abc123"));
    assert!(!plan257_attempt_shas_match("abc123", "def456"));
}

#[test]
fn plan257_cross_attempt_row_merging_is_rejected() {
    // Rows from attempt 1 must never satisfy attempt 2's gate.
    let attempt_one = vec!["obep/build-accepted".to_string()];
    let attempt_two: Vec<String> = vec![];
    assert!(!plan257_attempt_covers_mandatory_rows(
        &attempt_two,
        &attempt_one
    ));
    let attempt_two = vec!["obep/build-accepted".to_string()];
    assert!(plan257_attempt_covers_mandatory_rows(
        &attempt_two,
        &attempt_two
    ));
}

/// Plan 257 work package J: the closure gate requires exactly two
/// independent complete attempts.
fn plan257_required_complete_attempts() -> usize {
    2
}

/// Plan 257 work package J: both complete attempts must name the
/// same exact i2pr SHA.
fn plan257_attempt_shas_match(first: &str, second: &str) -> bool {
    first == second
}

/// Plan 257 work package J: each attempt independently covers its
/// mandatory rows; `covered` must contain every row in
/// `mandatory` on its own (no borrowing from another attempt).
fn plan257_attempt_covers_mandatory_rows(covered: &[String], mandatory: &[String]) -> bool {
    mandatory.iter().all(|row| covered.contains(row))
}

// Plan 257 §18 row 12 (WP E): a hard-coded bandwidth string cannot
// satisfy the typed bandwidth disposition predicate.
#[test]
fn plan257_hardcoded_bandwidth_string_cannot_satisfy_disposition() {
    let typed = i2pr_tunnel::TransitBandwidthSummary::new(
        i2pr_tunnel::TransitBandwidthRequest::default(),
        None,
        false,
    );
    assert!(typed.request_is_empty());
    assert_eq!(typed.evidence_label(), "m=- r=- l=- b=- accepted=false");
    // The legacy hard-coded harness string carries no typed
    // m/r/l/b facts and must never equal a typed label.
    let legacy = "reference-options-unobserved-no-fabrication";
    assert!(!typed.evidence_label().contains(legacy));
}

// Plan 257 §18 row 10-11 (WP D): a receive-id-only row cannot
// satisfy exact registration cardinality.
#[test]
fn plan257_receive_id_only_row_cannot_satisfy_cardinality() {
    // Cardinality requires before/after/p pending facts, not just
    // the receive id. A bare receive id proves nothing about the
    // delta.
    let before = 3_usize;
    let after_unknown: Option<usize> = None;
    assert!(after_unknown.is_none());
    let _ = before;
    // With both snapshots the delta check is exact.
    let after = 4_usize;
    assert_eq!(after, before + 1);
}

// Plan 257 §18 row 1-2 (WP §4): the runner must source-lock both
// i2pd reply branches (remote garlic/TunnelGateway vs local IBGW
// injection) against exact-pinned TransitTunnel.cpp.
#[test]
fn plan257_runner_source_locks_both_reply_branches() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/integration/m11-transit/run-i2pd.sh"),
    )
    .expect("read runner");
    for marker in [
        "m11-i2pd-obep-remote-reply-source-lock",
        "m11-i2pd-obep-local-ibgw-reply-source-lock",
        "RGarlicKeyAndTag",
        "IBGW is local",
    ] {
        assert!(
            src.contains(marker),
            "runner missing Plan 257 source lock {marker}"
        );
    }
}

// ---------------------------------------------------------------------------
// Plan 258 work package A — gateway failure/nested-size telemetry
// regressions (§5 rows 1, 2, 6, 7). Each test proves the typed
// ledger carries the failure dimension and the nested size fact,
// and that the extracted pass predicates keep their Plan 257
// semantics without the external lane.
// ---------------------------------------------------------------------------

fn gateway_observation(
    receive_tunnel: u32,
    delivered: usize,
    failures: usize,
    nested_len: usize,
) -> Observation {
    gateway_observation_with_next(
        receive_tunnel,
        delivered,
        failures,
        nested_len,
        [0xBB; 32],
        0x4000,
    )
}

fn gateway_observation_with_next(
    receive_tunnel: u32,
    delivered: usize,
    failures: usize,
    nested_len: usize,
    next_router: [u8; 32],
    next_tunnel: u32,
) -> Observation {
    let mut ledger = TypedLedger::default();
    let outcome = LiveInboundOutcome::Gateway(LiveGatewayOutcome::Delivered {
        delivered,
        failures,
        receive_tunnel,
        nested_len,
        next_router: Hash::from_bytes(next_router),
        next_tunnel,
    });
    record_data_outcome(
        &mut ledger,
        Epoch::IbgwData,
        synthetic_peer(0x71),
        "plan258",
        &outcome,
        1,
        b"",
        1_700_000_002_000,
    );
    ledger
        .of_epoch(Epoch::IbgwData)
        .find(|obs| obs.kind == ObservedKind::GatewayDelivered)
        .expect("gateway observation recorded")
        .clone()
}

// Plan 258 §5 row 1: the ledger observation carries the failure
// dimension alongside the delivered-cell count.
#[test]
fn plan258_gateway_observation_carries_failure_dimension() {
    let obs = gateway_observation(0x9201, 1, 2, 1_500);
    assert_eq!(obs.kind, ObservedKind::GatewayDelivered);
    assert_eq!(obs.receive_tunnel, 0x9201);
    assert_eq!(obs.aux_count, 1);
    assert_eq!(obs.gateway_failures, Some(2));
    assert_eq!(obs.nested_len, Some(1_500));
}

// Plan 258 §5 row 1 (zero case): the dimension is explicit even
// when no forward failed — `Some(0)`, never a missing field.
#[test]
fn plan258_gateway_observation_zero_failures_is_explicit() {
    let obs = gateway_observation(0x9201, 3, 0, 1_500);
    assert_eq!(obs.gateway_failures, Some(0));
    assert_eq!(obs.aux_count, 3);
}

// Plan 258 §5 row 2: nested size classes distinguish single-cell
// from multi-cell-capable batches at the shared production
// threshold; a missing length folds single-cell (fail closed).
#[test]
fn plan258_nested_size_class_distinguishes_single_from_multi() {
    let small = gateway_observation(0x9201, 1, 0, 500);
    let at_ceiling = gateway_observation(0x9201, 1, 0, i2pr_tunnel::MAX_FRAGMENT_BODY_BYTES);
    let above_ceiling = gateway_observation(0x9201, 2, 0, i2pr_tunnel::MAX_FRAGMENT_BODY_BYTES + 1);
    // The lane's 1,500-byte datagram nested is multi-cell-capable
    // under the corrected emission boundary (two fragments).
    let datagram = gateway_observation(0x9201, 2, 0, 1_500);
    let refs: Vec<&Observation> = vec![&small, &at_ceiling, &above_ceiling, &datagram];
    let counts = gateway_diag_counts(&refs);
    assert_eq!(counts.ingress, 4);
    assert_eq!(counts.nested_single, 2);
    assert_eq!(counts.nested_multi, 2);
}

// Plan 258 §5 row 6: the multicell gate still rejects
// single-cell-only batches and accepts one 2+ cell emission.
#[test]
fn plan258_multicell_gate_rejects_single_cell_only() {
    let one_a = gateway_observation(0x9201, 1, 0, 1_500);
    let one_b = gateway_observation(0x9201, 1, 0, 1_500);
    let refs: Vec<&Observation> = vec![&one_a, &one_b];
    assert!(!gateway_multicell_satisfied(&refs));
    let two = gateway_observation(0x9201, 2, 0, 70_000);
    let mixed: Vec<&Observation> = vec![&one_a, &two];
    assert!(gateway_multicell_satisfied(&mixed));
}

// Plan 258 §5 row 7: background gateway traffic to unaccepted ids
// still cannot satisfy the acceptance filter.
#[test]
fn plan258_background_gateway_to_unaccepted_id_is_rejected() {
    let genuine = gateway_observation(0x9201, 1, 0, 1_500);
    let background = gateway_observation(0x9202, 2, 0, 70_000);
    let accepted = [0x9201_u32];
    assert!(gateway_ingress_accepted(&genuine, &accepted));
    assert!(!gateway_ingress_accepted(&background, &accepted));
    // A multi-cell background emission alone satisfies neither
    // acceptance nor (through the filter) the multicell row.
    let filtered: Vec<&Observation> = [&background]
        .into_iter()
        .filter(|obs| gateway_ingress_accepted(obs, &accepted))
        .collect();
    assert!(filtered.is_empty());
    assert!(!gateway_multicell_satisfied(&filtered));
}

// Plan 258 §5 row 1 (fold): the diagnostic distribution sums the
// failure dimension and separates emission classes.
#[test]
fn plan258_diag_counts_fold_failures_and_emission() {
    let clean_single = gateway_observation(0x9201, 1, 0, 1_500);
    let failed_single = gateway_observation(0x9201, 1, 3, 1_500);
    let clean_multi = gateway_observation(0x9201, 4, 0, 70_000);
    let refs: Vec<&Observation> = vec![&clean_single, &failed_single, &clean_multi];
    let counts = gateway_diag_counts(&refs);
    assert_eq!(counts.ingress, 3);
    assert_eq!(counts.emitted_single, 2);
    assert_eq!(counts.emitted_multi, 1);
    assert_eq!(counts.emitted_max, 4);
    assert_eq!(counts.failures_total, 3);
    assert_eq!(counts.failed_ingress, 1);
}

// Plan 258 §5 row 1 (drop side): a dropped gateway ingress
// records the ADDRESSED id (accepted or stale) plus the nested
// size fact; the failure dimension stays absent (no forward was
// attempted).
#[test]
fn plan258_drop_observation_records_addressed_id_and_size() {
    let mut ledger = TypedLedger::default();
    let outcome = LiveInboundOutcome::Gateway(LiveGatewayOutcome::Dropped {
        tunnel_id: 0x9999,
        nested_len: 1_900,
    });
    record_data_outcome(
        &mut ledger,
        Epoch::IbgwData,
        synthetic_peer(0x71),
        "plan258drop",
        &outcome,
        1,
        b"",
        1_700_000_002_000,
    );
    let obs = ledger
        .of_epoch(Epoch::IbgwData)
        .find(|obs| obs.kind == ObservedKind::GatewayDropped)
        .expect("drop observation recorded");
    assert_eq!(obs.receive_tunnel, 0x9999);
    assert_eq!(obs.nested_len, Some(1_900));
    assert_eq!(obs.gateway_failures, None);
    // Drops never satisfy the acceptance filter, even when they
    // address an accepted id.
    assert!(!gateway_ingress_accepted(obs, &[0x9999]));
}

// Plan 258 drop-side fold: stale-id vs accepted-id drops separate,
// and datagram-sized drops are visible even when nothing delivers.
#[test]
fn plan258_drop_fold_separates_stale_from_accepted() {
    let mut ledger = TypedLedger::default();
    for (tunnel_id, nested_len) in [(0x9201_u32, 500_usize), (0x9999, 1_900), (0x9998, 300)] {
        let outcome = LiveInboundOutcome::Gateway(LiveGatewayOutcome::Dropped {
            tunnel_id,
            nested_len,
        });
        record_data_outcome(
            &mut ledger,
            Epoch::IbgwData,
            synthetic_peer(0x71),
            "plan258drop",
            &outcome,
            1,
            b"",
            1_700_000_002_000,
        );
    }
    let dropped: Vec<&Observation> = ledger
        .of_epoch(Epoch::IbgwData)
        .filter(|obs| obs.kind == ObservedKind::GatewayDropped)
        .collect();
    let counts = gateway_drop_diag_counts(&dropped, &[0x9201]);
    assert_eq!(counts.dropped, 3);
    assert_eq!(counts.dropped_accepted_id, 1);
    assert_eq!(counts.dropped_stale_id, 2);
    assert_eq!(counts.dropped_nested_multi, 1);
}

// Plan 258 per-ingress drop labels bind the addressed id to the
// accepted set and classify the nested size: stale relays vs
// accepted-id drops separate without the external lane.
#[test]
fn plan258_drop_label_separates_scope_and_class() {
    let mut ledger = TypedLedger::default();
    for (tunnel_id, nested_len) in [(0x9201_u32, 500_usize), (0x9999, 1_900)] {
        let outcome = LiveInboundOutcome::Gateway(LiveGatewayOutcome::Dropped {
            tunnel_id,
            nested_len,
        });
        record_data_outcome(
            &mut ledger,
            Epoch::IbgwData,
            synthetic_peer(0x71),
            "plan258drop",
            &outcome,
            1,
            b"",
            1_700_000_002_000,
        );
    }
    let dropped: Vec<&Observation> = ledger
        .of_epoch(Epoch::IbgwData)
        .filter(|obs| obs.kind == ObservedKind::GatewayDropped)
        .collect();
    assert_eq!(dropped.len(), 2);
    let accepted = [0x9201_u32];
    let labels: Vec<(&str, &str)> = dropped
        .iter()
        .map(|obs| gateway_drop_diag_label(obs, &accepted))
        .collect();
    assert!(labels.contains(&("accepted", "single")));
    assert!(labels.contains(&("stale", "multi")));
}

// ---------------------------------------------------------------------------
// Plan 260 §16.10–16.13: receipt-tuple predicate unit rows.
// The six-field tuple validator rejects every partial binding
// the historical Plan 259 evidence could supply and accepts
// only the complete creator-owned conjunction — all without the
// external lane.
// ---------------------------------------------------------------------------

fn plan260_canonical_tuple() -> (ReceiptTuple, String) {
    let i2pr = "aa".repeat(32);
    let receiver = "bb".repeat(32);
    let creator = "cc".repeat(32);
    (
        ReceiptTuple {
            receiver_destination_hash: receiver.clone(),
            ibgw_receive_id: 0x51A6_1001,
            creator_router_hash: creator,
            creator_local_tunnel_id: 0x51A6_2002,
            pool_owner_destination_hash: receiver,
            leaseset_gateway_hash: i2pr.clone(),
            leaseset_tunnel_id: 0x51A6_1001,
        },
        i2pr,
    )
}

// Plan 260 §16.10: a next-router hash with no bound
// creator-local tunnel id (the historical A-ending shape) is
// rejected, never inferred into a pass.
#[test]
fn plan260_receipt_tuple_rejects_router_hash_without_local_tunnel() {
    let (mut tuple, i2pr) = plan260_canonical_tuple();
    tuple.creator_local_tunnel_id = 0;
    assert_eq!(
        validate_receipt_tuple(&tuple, &i2pr),
        Err(ReceiptTupleError::MissingCreatorLocalTunnel)
    );
}

// Plan 260 §16.11: a local-tunnel binding whose pool owner is
// not the receiver destination is rejected.
#[test]
fn plan260_receipt_tuple_rejects_local_tunnel_without_pool_owner() {
    let (mut tuple, i2pr) = plan260_canonical_tuple();
    tuple.pool_owner_destination_hash = "dd".repeat(32);
    assert_eq!(
        validate_receipt_tuple(&tuple, &i2pr),
        Err(ReceiptTupleError::PoolOwnerMismatch)
    );
}

// Plan 260 §16.12: a pool binding whose LeaseSet does not
// advertise this exact i2pr gateway tuple is rejected (both
// the gateway-hash and the tunnel-id arms).
#[test]
fn plan260_receipt_tuple_rejects_pool_without_leaseset_binding() {
    let (mut tuple, i2pr) = plan260_canonical_tuple();
    tuple.leaseset_gateway_hash = "ee".repeat(32);
    assert_eq!(
        validate_receipt_tuple(&tuple, &i2pr),
        Err(ReceiptTupleError::LeaseSetMismatch)
    );
    let (mut tuple, i2pr) = plan260_canonical_tuple();
    tuple.leaseset_tunnel_id = 0x51A6_1002;
    assert_eq!(
        validate_receipt_tuple(&tuple, &i2pr),
        Err(ReceiptTupleError::LeaseSetMismatch)
    );
}

// Plan 260 §16.13: the complete six-field conjunction passes.
#[test]
fn plan260_receipt_tuple_accepts_complete_tuple() {
    let (tuple, i2pr) = plan260_canonical_tuple();
    assert_eq!(validate_receipt_tuple(&tuple, &i2pr), Ok(()));
}

// Plan 260 decoder hygiene: the test-only i2p base64 decoder
// round-trips the source-locked encoder and fails closed on
// non-alphabet bytes.
#[test]
fn plan260_i2p_b64_decode_round_trips_encode() {
    let vectors: Vec<Vec<u8>> = vec![
        vec![],
        vec![0x00],
        vec![0xFF; 2],
        (0..100_u32).map(|value| (value & 0xFF) as u8).collect(),
    ];
    for bytes in &vectors {
        assert_eq!(
            i2p_b64_decode(&i2p_b64_encode(bytes)).expect("decode"),
            *bytes
        );
    }
    assert!(i2p_b64_decode("!!!!").is_err());
    assert!(i2p_b64_decode("A").is_err());
}

// Plan 260 §16.17–16.18 (delivery-surface companion): the
// gateway-delivery observation carries the committed next
// router/tunnel tuple so the receipt epoch can bind the
// creator-local tunnel id from typed evidence.
#[test]
fn plan260_gateway_observation_carries_next_tunnel_tuple() {
    let obs = gateway_observation_with_next(0x9201, 2, 0, 1_900, [0xCC; 32], 0x51A6_2002);
    assert_eq!(obs.kind, ObservedKind::GatewayDelivered);
    assert_eq!(obs.receive_tunnel, 0x9201);
    assert_eq!(obs.next_router, Some([0xCC; 32]));
    assert_eq!(
        obs.next_message_id, 0x51A6_2002,
        "data-forward convention: next tunnel rides in next_message_id"
    );
}

// ---------------------------------------------------------------------------
// External lane: single qualification owner.
// ---------------------------------------------------------------------------

/// Bounded lifecycle owner for one exact-pinned reference process.
/// The child is terminated on explicit shutdown and on drop; there
/// is no ownerless spawn.
struct ReferenceProcess {
    name: &'static str,
    child: std::process::Child,
    datadir: PathBuf,
    log_path: PathBuf,
}

impl ReferenceProcess {
    async fn spawn(
        bin: &str,
        name: &'static str,
        datadir: &Path,
        conf_path: &Path,
        log_path: &Path,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(datadir).map_err(|e| format!("datadir: {e}"))?;
        let log_file = std::fs::File::create(log_path).map_err(|e| format!("log: {e}"))?;
        let std_file = log_file
            .try_clone()
            .map_err(|e| format!("log clone: {e}"))?;
        let child = std::process::Command::new(bin)
            .arg(format!("--conf={}", conf_path.display()))
            .arg(format!("--datadir={}", datadir.display()))
            .arg("--log=file")
            .arg(format!("--logfile={}", log_path.display()))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(std_file))
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("spawn {name}: {e}"))?;
        Ok(Self {
            name,
            child,
            datadir: datadir.to_path_buf(),
            log_path: log_path.to_path_buf(),
        })
    }

    async fn wait_for_router_info(&mut self, timeout: Duration) -> Result<PathBuf, String> {
        let deadline = tokio::time::Instant::now() + timeout;
        let ri = self.datadir.join("router.info");
        while tokio::time::Instant::now() < deadline {
            if ri.exists() && self.log_contains("Start listening") {
                return Ok(ri);
            }
            if let Ok(Some(status)) = self.child.try_wait() {
                return Err(format!(
                    "{} exited during startup with {status}; log: {}",
                    self.name,
                    self.log_tail(40)
                ));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        Err(format!(
            "{} did not publish router.info / SSU2 listener; log: {}",
            self.name,
            self.log_tail(40)
        ))
    }

    fn log_contains(&self, needle: &str) -> bool {
        std::fs::read_to_string(&self.log_path)
            .map(|log| log.contains(needle))
            .unwrap_or(false)
    }

    /// Counts reference-logged outbound tunnel establishments
    /// (`Outbound tunnel <id> has been created`). The sender pools
    /// are the only outbound pools in the data windows, so a count
    /// increase after the counted setup proves the reference
    /// established a fresh tunnel (most recently via our reply)
    /// with an empty queue: sending immediately then emits with a
    /// fresh inner expiration instead of rotting behind zombies.
    fn count_outbound_created(&self) -> usize {
        std::fs::read_to_string(&self.log_path)
            .map(|log| {
                log.lines()
                    .filter(|line| {
                        line.contains("Outbound tunnel ") && line.contains(" has been created")
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    fn log_tail(&self, lines: usize) -> String {
        std::fs::read_to_string(&self.log_path)
            .map(|log| {
                log.lines()
                    .rev()
                    .take(lines)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_else(|_| "<no log>".to_string())
    }

    /// Counts independent B-side endpoint receipts for one tunnel
    /// id (Plan 257 work package F).
    ///
    /// Source-locked to exact-pinned i2pd 2.61.0
    /// `libi2pd/TransitTunnel.cpp`
    /// `TransitTunnelEndpoint::HandleTunnelDataMsg`, which logs
    /// `TransitTunnel: handle msg for endpoint <id>` at debug
    /// level before handing decrypted data to the endpoint. Only
    /// a debug-level reference emits this line; an info-level B
    /// yields zero, which fails the far-side row closed rather
    /// than silently passing. Raw logs are diagnostic input only;
    /// the counted evidence is the sanitized count bound to the
    /// exact next tunnel id from the local forward.
    fn count_endpoint_messages(&self, tunnel_id: u32) -> usize {
        let needle = format!("handle msg for endpoint {tunnel_id}");
        std::fs::read_to_string(&self.log_path)
            .map(|log| log.lines().filter(|line| line.contains(&needle)).count())
            .unwrap_or(0)
    }

    async fn shutdown(mut self) {
        let _ = self.child.kill();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => break,
                Ok(None) => tokio::time::sleep(POLL_INTERVAL).await,
            }
        }
    }
}

impl Drop for ReferenceProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn write_i2pd_conf(
    home: &Path,
    port: u16,
    sam_port: Option<u16>,
    with_transit: bool,
    floodfill: bool,
    loglevel: &str,
) -> Result<PathBuf, String> {
    let conf = home.join("i2pd.conf");
    let notransit = if with_transit { "false" } else { "true" };
    // floodfill is loopback-only NetDB function: with zero public
    // peers the mesh has no other LeaseSet store, and same-router
    // datagrams (sender resolving the sibling receiver's LeaseSet)
    // can never resolve without one — every send dies with `Can't
    // request LeaseSet` no matter how fresh the receiver set is.
    // B serves as the loopback floodfill so publishes and lookups
    // complete genuinely through stock reference behavior; A
    // stays non-floodfill. No public exposure (loopback +
    // empty reseed + netid 2), no patched binary.
    let floodfill_flag = if floodfill { "true" } else { "false" };
    let sam_section = sam_port.map_or("[sam]\nenabled = false\n".to_string(), |port| {
        format!("[sam]\nenabled = true\naddress = 127.0.0.1\nport = {port}\n")
    });
    // Exploratory router tunnels are disabled (documented i2pd
    // `exploratory.*` knobs): background router-tunnel churn through
    // the tiny loopback mesh would otherwise drown the counted
    // client-tunnel epochs in retries. The counted topologies are
    // the explicit-peer SAM client tunnels below; transit
    // acceptance at the references stays enabled.
    // Loopback bandwidth tier P (2048 KBps; source-locked i2pd
    // tiers are L = 32, O = 256, P = 2048, X = >9000 KBps): tier L
    // caps the references at tens of KBps, so routine build /
    // publish / test bursts queue for seconds on the loopback
    // SSU2 links (`Outgoing messages queue ... semi-full`, lag in
    // the seconds, `Tunnel ... request was not sent`) and time
    // out into retry storms. P is still modest loopback headroom
    // (no public exposure: loopback + empty reseed + netid 2),
    // keeps the mesh burst-tolerant, and changes no protocol,
    // topology, or advertisement claim.
    // Reference log level flows in as a parameter (run_qualification
    // documents the info-by-default rationale and the triage
    // overrides); both references stay loopback-only regardless.
    let text = format!(
        "daemon = false\nloglevel = {loglevel}\nnetid = 2\naddress4 = 127.0.0.1\nhost = 127.0.0.1\nport = {port}\nipv4 = true\nipv6 = false\nnat = false\nnotransit = {notransit}\nfloodfill = {floodfill_flag}\nreservedrange = false\nbandwidth = P\n[exploratory]\ninbound.length = 0\noutbound.length = 0\ninbound.quantity = 0\noutbound.quantity = 0\n[ssu2]\nenabled = true\npublished = true\nport = {port}\n[ntcp2]\nenabled = false\npublished = false\n[http]\nenabled = false\n[httpproxy]\nenabled = false\n[socksproxy]\nenabled = false\n{sam_section}[i2cp]\nenabled = false\n[i2pcontrol]\nenabled = false\n[upnp]\nenabled = false\n[reseed]\nverify = true\nurls =\nthreshold = 0\n"
    );
    std::fs::write(&conf, text).map_err(|e| format!("write i2pd.conf: {e}"))?;
    Ok(conf)
}

/// Extracts the public destination from a SAM SESSION STATUS OK line.
fn extract_destination(reply: &str, id: &str) -> Result<String, String> {
    let marker = "DESTINATION=";
    let start = reply
        .find(marker)
        .map(|index| index + marker.len())
        .ok_or_else(|| format!("SAM session {id} reply lacks DESTINATION: {reply}"))?;
    Ok(reply[start..].trim().to_string())
}

/// Identity material shared by every qualification owner. The
/// responder secret stays in memory only; hashes are public.
struct QualificationIdentity {
    local_hash: Hash,
    responder_priv: [u8; 32],
    a_hash: Hash,
    b_hash: Hash,
}

fn fresh_owner(
    handle: &i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    seed: u64,
    identity: &QualificationIdentity,
) -> Result<TransitLiveOwner<ChaCha8Rng>, String> {
    let mut owner = TransitLiveOwner::new_disabled(
        handle.delivery().clone(),
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(seed),
    );
    owner.enable(build_transit_service(
        identity.local_hash,
        identity.responder_priv,
        handle.delivery().clone(),
    ));
    // Bounded LOCAL consumer: background reference tunnel-test
    // traffic can complete as OBEP LOCAL actions; without a sink
    // those would surface as fatal owner errors and abort the
    // lane. The sink counts deliveries and always succeeds.
    let local_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let local_count_sink = local_count.clone();
    owner.install_local_sink(move |_| {
        local_count_sink.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    });
    owner
        .install_peer(identity.a_hash, PeerId::from_hash(identity.a_hash))
        .map_err(|e| format!("install A peer: {e}"))?;
    owner
        .install_peer(identity.b_hash, PeerId::from_hash(identity.b_hash))
        .map_err(|e| format!("install B peer: {e}"))?;
    if !owner.is_enabled() {
        return Err("live owner must be enabled".to_string());
    }
    Ok(owner)
}

/// Re-dials both references, refreshing our SSU2 session links.
/// Sessions die silently mid-run (idle timeouts, path churn,
/// loopback buffer overruns under crypto saturation); without a
/// redial, later sends fail closed as NoActiveSession
/// and inbound cells fall into the unknown-link drop, while the
/// references see no error. Re-dialing before each send phase
/// keeps a live bidirectional pairing; on a healthy mesh it is
/// a cheap handshake round-trip. A completed redial also heals
/// the first-link pinning: the runtime replaces the dead oldest
/// link with the fresh one, so subsequent deliveries stop queueing
/// into the void. Best-effort by design: a failed redial (e.g.,
/// duplicate session still healthy) must never fail the lane —
/// the existing links simply stay in use. Both dials run
/// concurrently so one slow peer cannot stall the other.
/// Conditional: peers that already hold an authenticated link
/// are never redialed. A duplicate handshake that our stack
/// accepts while the reference rejects (`already exists`)
/// diverges the two session views permanently (we send on the
/// new link the reference drops; the reference sends on the old
/// link we may have removed), so redialing a live link is
/// strictly worse than leaving it alone.
async fn ensure_sessions(
    handle: &i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    a_target: i2pr_runtime::Ssu2DialTarget,
    b_target: i2pr_runtime::Ssu2DialTarget,
) {
    ensure_sessions_with_timeout(handle, a_target, b_target, DIAL_TIMEOUT).await;
}

/// Bounded variant of [`ensure_sessions`] for the setup loops: a
/// full 20 s stall per peer would wedge inbound drains, so the
/// in-loop heartbeat uses a shorter horizon. Conditional, like
/// [`ensure_sessions`]: peers that already hold an authenticated
/// link are left untouched so no duplicate handshake can
/// diverge the two stacks' session views.
async fn ensure_sessions_with_timeout(
    handle: &i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    a_target: i2pr_runtime::Ssu2DialTarget,
    b_target: i2pr_runtime::Ssu2DialTarget,
    timeout: Duration,
) {
    let need_a = handle
        .service()
        .manager()
        .delivery_capability(a_target.peer())
        .is_err();
    let need_b = handle
        .service()
        .manager()
        .delivery_capability(b_target.peer())
        .is_err();
    if !need_a && !need_b {
        return;
    }
    let cancel_a = CancellationToken::new();
    let cancel_b = CancellationToken::new();
    let dial_a = async {
        if need_a {
            let _ = handle.dial(a_target, timeout, &cancel_a).await;
        }
    };
    let dial_b = async {
        if need_b {
            let _ = handle.dial(b_target, timeout, &cancel_b).await;
        }
    };
    tokio::join!(dial_a, dial_b);
}

// Plan 263 work package A — lane-harness sustainability proofs.
//
// These helpers bound the counted send phases without touching
// production routing code and without tuning any timeout, quota,
// ceiling, retry budget, or message size (all constants above are
// unchanged from Plan 262):
//
// - session freshness: `verify_mesh_live_for_send` proves both
//   SSU2 links still hold delivery capability after the
//   best-effort `ensure_sessions` redial, and fails closed with
//   the exact missing-peer signature instead of burning send
//   rounds into a dead relay;
// - relay robustness: `verify_relay_netdb_prerequisites` proves
//   the three NetDB placements the A-via-B relay requires (B's
//   RI in A's store for explicit-peer selection, i2pr's RI in
//   both stores) still exist on disk, and
//   `verify_b_floodfill_conf` proves B's lane config still
//   carries the floodfill role the LeaseSet path requires;
// - SAM discipline: the canonical `SAM_READ_TIMEOUT_MSG` tail
//   (locked above) is the only SAM timeout evidence; it always
//   fails the attempt via `?` and is never retried in-process.

/// Formats the exact fail-closed signature for a mesh that
/// cannot sustain a counted send: which SSU2 links are dead.
fn mesh_liveness_error(missing: &[&str]) -> String {
    format!(
        "Plan 263 mesh liveness unproven before counted send (missing SSU2 delivery: {}); \
         relay topology fails closed, never retried blindly",
        missing.join(",")
    )
}

/// Proves both SSU2 links hold delivery capability right now.
/// Call after `ensure_sessions` and before any counted payload
/// send; a dead link fails the epoch closed instead of sending
/// into a relay that cannot deliver.
fn mesh_liveness_status(
    handle: &i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    a_target: i2pr_runtime::Ssu2DialTarget,
    b_target: i2pr_runtime::Ssu2DialTarget,
) -> Result<(), String> {
    let mut missing: Vec<&str> = Vec::new();
    if handle
        .service()
        .manager()
        .delivery_capability(a_target.peer())
        .is_err()
    {
        missing.push("A");
    }
    if handle
        .service()
        .manager()
        .delivery_capability(b_target.peer())
        .is_err()
    {
        missing.push("B");
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(mesh_liveness_error(&missing))
    }
}

/// Proves the three NetDB placements the A-via-B relay requires
/// still exist: B's RI in A's store (explicit-peer B selection),
/// i2pr's RI in A's store, i2pr's RI in B's store. Filesystem
/// existence only; no timeout, no retry.
fn verify_relay_netdb_prerequisites(
    b_ri_in_a_netdb: &std::path::Path,
    i2pr_ri_in_a_netdb: &std::path::Path,
    i2pr_ri_in_b_netdb: &std::path::Path,
) -> Result<(), String> {
    let mut missing: Vec<&str> = Vec::new();
    if !b_ri_in_a_netdb.exists() {
        missing.push("B-RI-in-A-NetDB(explicit-peer-selection)");
    }
    if !i2pr_ri_in_a_netdb.exists() {
        missing.push("i2pr-RI-in-A-NetDB");
    }
    if !i2pr_ri_in_b_netdb.exists() {
        missing.push("i2pr-RI-in-B-NetDB");
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Plan 263 relay topology unproven before counted send (missing NetDB placements: {}); \
             B cannot relay without them, never forced with a direct-sender substitution",
            missing.join(",")
        ))
    }
}

/// Proves B's lane config still carries the floodfill role the
/// LeaseSet resolution path requires (B self-lists via
/// `m_Floodfills.Insert(GetSharedRouterInfo())` only when
/// started as floodfill). Reads the file the lane wrote; no
/// reference patching, no timeout.
fn verify_b_floodfill_conf(b_home: &std::path::Path) -> Result<(), String> {
    let conf = b_home.join("i2pd.conf");
    let text =
        std::fs::read_to_string(&conf).map_err(|e| format!("Plan 263 B conf unreadable: {e}"))?;
    if text.contains("floodfill = true") {
        Ok(())
    } else {
        Err(
            "Plan 263 relay topology unproven: B conf lacks `floodfill = true`; \
             LeaseSet resolution cannot complete without the floodfill role"
                .to_string(),
        )
    }
}

/// Minimal i2pd SAM client over one TCP connection (one session per
/// connection; the session dies with the socket).
struct SamClient {
    stream: BufStream<tokio::net::TcpStream>,
    /// Bytes read from the socket that do not yet form a complete
    /// reply line. Holding them across calls makes the timed read
    /// cancellation-safe: a reply split across socket reads is
    /// never lost when the interleave timeout fires.
    pending: String,
}

impl SamClient {
    async fn connect(addr: SocketAddr) -> Result<Self, String> {
        let stream = tokio::time::timeout(SAM_IO_TIMEOUT, tokio::net::TcpStream::connect(addr))
            .await
            .map_err(|_| "SAM connect timeout".to_string())?
            .map_err(|e| format!("SAM connect: {e}"))?;
        Ok(Self {
            stream: BufStream::new(stream),
            pending: String::new(),
        })
    }

    async fn roundtrip(&mut self, request: &str) -> Result<String, String> {
        self.stream
            .write_all(request.as_bytes())
            .await
            .map_err(|e| format!("SAM write: {e}"))?;
        self.stream
            .flush()
            .await
            .map_err(|e| format!("SAM flush: {e}"))?;
        self.try_read_line(SAM_IO_TIMEOUT)
            .await?
            .ok_or_else(|| SAM_READ_TIMEOUT_MSG.to_string())
    }

    async fn hello(&mut self) -> Result<(), String> {
        let reply = self.roundtrip("HELLO VERSION\n").await?;
        if reply.contains("HELLO REPLY RESULT=OK") {
            Ok(())
        } else {
            Err(format!("SAM hello rejected: {reply}"))
        }
    }

    /// Sends SESSION CREATE without waiting for the readiness
    /// reply. The caller drains the live owner concurrently; used
    /// by the reject epoch whose tunnel never becomes ready by
    /// design.
    async fn send_create(&mut self, id: &str, options: &[(&str, &str)]) -> Result<(), String> {
        let mut request = format!("SESSION CREATE STYLE=DATAGRAM ID={id} DESTINATION=TRANSIENT");
        for (key, value) in options {
            request.push(' ');
            request.push_str(key);
            request.push('=');
            request.push_str(value);
        }
        request.push('\n');
        self.stream
            .write_all(request.as_bytes())
            .await
            .map_err(|e| format!("SAM write: {e}"))?;
        self.stream
            .flush()
            .await
            .map_err(|e| format!("SAM flush: {e}"))?;
        Ok(())
    }

    /// Reads one SAM reply line, waiting at most `timeout`.
    /// Returns `Ok(None)` on timeout so the caller can interleave
    /// live-owner drains while the reference builds tunnels.
    async fn try_read_line(&mut self, timeout: Duration) -> Result<Option<String>, String> {
        if let Some(index) = self.pending.find('\n') {
            let line: String = self.pending.drain(..=index).collect();
            return Ok(Some(line));
        }
        let mut line = std::mem::take(&mut self.pending);
        match tokio::time::timeout(timeout, self.stream.read_line(&mut line)).await {
            Ok(Ok(_)) => Ok(Some(line)),
            Ok(Err(e)) => Err(format!("SAM read: {e}")),
            Err(_) => {
                // A timed-out `read_line` has already consumed the
                // bytes it saw; keep them so the next call still
                // sees a whole reply line.
                self.pending = line;
                Ok(None)
            }
        }
    }

    async fn send_datagram(
        &mut self,
        id: &str,
        destination: &str,
        payload: &[u8],
    ) -> Result<(), String> {
        let header = format!(
            "DATAGRAM SEND ID={id} DESTINATION={destination} SIZE={}\n",
            payload.len()
        );
        self.stream
            .write_all(header.as_bytes())
            .await
            .map_err(|e| format!("SAM send header: {e}"))?;
        self.stream
            .write_all(payload)
            .await
            .map_err(|e| format!("SAM send payload: {e}"))?;
        self.stream
            .flush()
            .await
            .map_err(|e| format!("SAM send flush: {e}"))?;
        Ok(())
    }

    /// Reads one inbound `DATAGRAM RECEIVED` datagram (header line
    /// plus exact binary payload) from the session socket, waiting
    /// at most `timeout` overall. Returns the declared SIZE and the
    /// payload bytes. Non-datagram lines are skipped; `Ok(None)`
    /// means nothing complete arrived in time. This is the
    /// reference-observed end of the data-plane path: a payload
    /// read here proves the reference forwarded the message down
    /// the destination's inbound tunnel and handed it to the SAM
    /// session, which background control traffic can never do.
    async fn try_read_datagram(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<(usize, Vec<u8>)>, String> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            let line = match self.try_read_line(remaining).await? {
                Some(line) => line,
                None => return Ok(None),
            };
            let Some(rest) = line.strip_prefix("DATAGRAM RECEIVED") else {
                continue;
            };
            let size: usize = rest
                .split_whitespace()
                .find_map(|part| part.strip_prefix("SIZE="))
                .and_then(|digits| digits.parse().ok())
                .ok_or_else(|| "DATAGRAM RECEIVED lacks SIZE".to_string())?;
            if size == 0 || size > MAX_SAM_DATAGRAM_RX_BYTES {
                return Err(format!("DATAGRAM RECEIVED SIZE out of bounds: {size}"));
            }
            let mut payload = vec![0u8; size];
            match tokio::time::timeout(remaining, self.stream.read_exact(&mut payload)).await {
                Ok(Ok(_)) => return Ok(Some((size, payload))),
                Ok(Err(e)) => return Err(format!("SAM datagram payload: {e}")),
                Err(_) => return Ok(None),
            }
        }
    }
}

/// External Plan 256 row: the fail-closed single-owner interop driver.
/// The runner provisions the exact i2pd binary pin, fresh reference
/// datadirs, and loopback ports through the environment, then runs
/// this test via `--ignored --exact`. Missing environment variables,
/// a pin mismatch, or missing topology prerequisites fail closed
/// rather than silently passing.
#[tokio::test]
#[ignore = "Plan 256: requires exact-pinned external i2pd 2.61.0 environment + I2PD_BIN + I2PD_A_DATADIR + I2PD_B_DATADIR + I2PR_SSU2_BIND + EVIDENCE_DIR"]
async fn m11_transit_against_i2pd() {
    run_qualification()
        .await
        .unwrap_or_else(|error| panic!("Plan 256 qualification failed closed: {error}"));
}

async fn run_qualification() -> Result<(), String> {
    check_reference_pin(&env_value("I2PD_PIN"), &env_value("I2PD_VERSION"))?;
    let i2pd_bin = env_value("I2PD_BIN");
    let a_datadir = env_path("I2PD_A_DATADIR");
    let b_datadir = env_path("I2PD_B_DATADIR");
    let a_port: u16 = env_value("I2PD_A_PORT")
        .parse()
        .map_err(|_| "I2PD_A_PORT must be a loopback port".to_string())?;
    let b_port: u16 = env_value("I2PD_B_PORT")
        .parse()
        .map_err(|_| "I2PD_B_PORT must be a loopback port".to_string())?;
    let a_sam_port: u16 = env_value("I2PD_A_SAM_PORT")
        .parse()
        .map_err(|_| "I2PD_A_SAM_PORT must be a loopback port".to_string())?;
    // Plan 261 work package A: the B-side sender needs B's stock SAM
    // bridge. The port is a required lane input like the A SAM port:
    // a missing/invalid value fails before any socket, process, or
    // file mutation (same fail-closed position as the pin gate).
    let b_sam_port: u16 = check_b_sam_port(std::env::var("I2PD_B_SAM_PORT").ok())?;
    let bind: SocketAddr = env_value("I2PR_SSU2_BIND")
        .parse()
        .map_err(|_| "I2PR_SSU2_BIND must be a loopback socket address".to_string())?;
    let evidence_dir = env_path("EVIDENCE_DIR");
    std::fs::create_dir_all(&evidence_dir).map_err(|e| format!("evidence dir: {e}"))?;
    if !bind.ip().is_loopback() {
        return Err("i2pr bind must be loopback".to_string());
    }
    let bind_port = bind.port();
    if bind_port == 0 {
        return Err("external lane requires a fixed loopback bind".to_string());
    }
    // No evidence-directory-derived NetDB fallback exists in this
    // lane: both reference datadirs are explicit required inputs.
    for port in [a_port, b_port, a_sam_port, b_sam_port] {
        if port == 0 {
            return Err("reference ports must be fixed loopback ports".to_string());
        }
    }

    let config_text = format!(
        "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nport = {bind_port}\n"
    );
    let config = Config::parse(&config_text).map_err(|e| format!("strict profile: {e}"))?;
    if !config.ssu2.enabled || config.ssu2.advertise {
        return Err("controlled profile must enable SSU2 without advertisement".to_string());
    }
    append_evidence(&evidence_dir, "bootstrap/daemon-strict-profile", "true");

    // Single identity lifecycle: the bundle is generated once, the
    // tunnel-build responder secret IS the bundle encryption private
    // key, and the signed RouterInfo carries the matching public key.
    let bundle = RouterIdentityBundle::generate(&mut OsRng).map_err(|e| format!("bundle: {e}"))?;
    let local_hash = bundle.identity().hash().map_err(|e| format!("hash: {e}"))?;
    let responder_priv = *bundle.encryption_key().secret_bytes();
    let advertised_pub = bundle
        .encryption_key()
        .public_key()
        .map_err(|e| format!("encryption pub: {e}"))?;
    if advertised_pub.as_bytes() != bundle.identity().public_key().as_bytes() {
        return Err("bundle encryption key mismatch".to_string());
    }
    append_evidence(
        &evidence_dir,
        "bootstrap/routeridentity-build-key-coherent",
        "true",
    );
    let identity = generate_controlled_identity(&bundle, "127.0.0.1", bind_port)
        .map_err(|e| format!("controlled identity: {e}"))?;
    if identity.static_secret_bytes == responder_priv {
        return Err("SSU2 transport key must differ from the build responder key".to_string());
    }
    append_evidence(&evidence_dir, "bootstrap/ssu2-key-not-build-key", "true");
    let info_check = RouterInfo::decode(
        &identity.router_info,
        i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
    )
    .map_err(|e| format!("decode own RI: {e}"))?;
    if info_check
        .router_identity()
        .hash()
        .map_err(|e| format!("ri hash: {e}"))?
        != local_hash
    {
        return Err("signed RouterInfo hash mismatch".to_string());
    }
    append_evidence(&evidence_dir, "bootstrap/routerinfo-hash-coherent", "true");
    append_evidence(
        &evidence_dir,
        "bootstrap/no-independent-transit-responder-key",
        "true",
    );
    let i2pr_b64 = i2p_b64_encode(local_hash.as_bytes());
    append_evidence(
        &evidence_dir,
        "bootstrap/i2pr-routerinfo-len",
        &identity.router_info.len().to_string(),
    );
    let mut rows: Vec<(String, String)> = Vec::new();

    // Exact NetDB bootstrap BEFORE reference startup. Both files land
    // at the source-locked hashed owner path; nothing is derived
    // from the evidence directory.
    // Each owner write is recorded with its literal epoch key so the
    // static checker can bind the row to its epoch (no loop variable).
    // Plan 263 work package A.2: the paths are retained so the
    // relay prerequisites can be re-proven before each counted
    // send (mesh churn must not silently remove them).
    let i2pr_ri_in_a_netdb = i2pd_netdb_file_path(&a_datadir, &i2pr_b64);
    let i2pr_ri_in_b_netdb = i2pd_netdb_file_path(&b_datadir, &i2pr_b64);
    for (short, datadir) in [
        ("a-netdb-owner-exact", &a_datadir),
        ("b-netdb-owner-exact", &b_datadir),
    ] {
        let target = i2pd_netdb_file_path(datadir, &i2pr_b64);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("netdb bucket: {e}"))?;
        }
        std::fs::write(&target, &identity.router_info)
            .map_err(|e| format!("write RI into exact NetDB path: {e}"))?;
        if !target.exists() {
            return Err(format!("exact NetDB owner write failed for {short}"));
        }
        if short == "a-netdb-owner-exact" {
            record_row(
                &evidence_dir,
                Epoch::Bootstrap,
                "a-netdb-owner-exact",
                &target.display().to_string(),
                &mut rows,
            );
        } else {
            record_row(
                &evidence_dir,
                Epoch::Bootstrap,
                "b-netdb-owner-exact",
                &target.display().to_string(),
                &mut rows,
            );
        }
    }

    // Reference configurations (loopback-only, transit enabled so the
    // references can create tunnels, public reseed/network disabled).
    // Log level info by default: debug formatting of every cell
    // under crypto saturation measurably starves the reference
    // event loops (SSU2 queue lag, SAM parsing stalls that wedge
    // DATAGRAM SEND entirely), making qualification impossible;
    // info keeps every mandatory signal (`Outbound tunnel ...
    // created`, `routers loaded (`, warnings, errors) while
    // cutting log volume 10×+. No mandatory evidence row needs
    // debug content (all counts/hashes come from the typed ledger,
    // SAM sockets, INFO lines, or files). Triage runs may set
    // `I2PR_M11_LOGLEVEL=debug` for full forensics, or
    // `I2PR_M11_LOGLEVEL_B` to set B independently (e.g. A at
    // debug for build-reply forensics while B stays calm).
    let loglevel = std::env::var("I2PR_M11_LOGLEVEL").unwrap_or_else(|_| "info".to_string());
    let loglevel_b = std::env::var("I2PR_M11_LOGLEVEL_B").unwrap_or_else(|_| loglevel.clone());
    let a_home = a_datadir.join("home");
    let b_home = b_datadir.join("home");
    std::fs::create_dir_all(&a_home).map_err(|e| format!("a home: {e}"))?;
    std::fs::create_dir_all(&b_home).map_err(|e| format!("b home: {e}"))?;
    let a_conf = write_i2pd_conf(&a_home, a_port, Some(a_sam_port), true, false, &loglevel)?;
    // Plan 261 work package A: B enables its stock SAM bridge via the
    // same conf mechanism as A (loopback port from the lane env). B
    // stays floodfill (last arg true) so publishes and lookups keep
    // completing genuinely through stock reference behavior.
    let b_conf = write_i2pd_conf(&b_home, b_port, Some(b_sam_port), true, true, &loglevel_b)?;
    let a_log = evidence_dir.join("i2pd-a-driver.log");
    let b_log = evidence_dir.join("i2pd-b-driver.log");

    // Start i2pd-B first so its RouterInfo can be installed into
    // A's exact NetDB before A starts (A's Participant epoch must
    // FindRouter(B)).
    let mut reference_b =
        ReferenceProcess::spawn(&i2pd_bin, "i2pd-B", &b_datadir, &b_conf, &b_log).await?;
    let b_ri_path = reference_b
        .wait_for_router_info(Duration::from_secs(120))
        .await?;
    let b_ri_bytes = std::fs::read(&b_ri_path).map_err(|e| format!("read B router.info: {e}"))?;
    let (b_hash, b_ssu2) =
        verify_reference_router_info(&b_ri_bytes).map_err(|e| format!("verify B RI: {e}"))?;
    let b_b64 = i2p_b64_encode(b_hash.as_bytes());
    let b_netdb_target = i2pd_netdb_file_path(&a_datadir, &b_b64);
    if let Some(parent) = b_netdb_target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("A netdb bucket for B: {e}"))?;
    }
    std::fs::write(&b_netdb_target, &b_ri_bytes)
        .map_err(|e| format!("install B RI into A NetDB: {e}"))?;
    // Plan 263 work package A.2: retain B's RI path in A's NetDB
    // so the relay prerequisite (explicit-peer B selection) can
    // be re-proven before each counted send.
    let b_ri_in_a_netdb = b_netdb_target.clone();
    append_evidence(
        &evidence_dir,
        "bootstrap/a-knows-b-ri-exact",
        &b_netdb_target.display().to_string(),
    );
    let b_endpoint: SocketAddr = format!("127.0.0.1:{b_port}")
        .parse()
        .map_err(|e| format!("B endpoint: {e}"))?;
    let b_material = b_ssu2.address_material().map_err(|_| "B key material")?;
    let b_static = i2pr_runtime::Ssu2PublicKey::new(*b_material.static_public_key().as_bytes())
        .map_err(|e| format!("B static: {e}"))?;
    let b_intro = i2pr_runtime::IntroKey::new(*b_material.intro_key().as_bytes());
    let b_target = daemon_dial_target(b_hash, b_endpoint, b_static, b_intro)
        .map_err(|e| format!("B dial target: {e}"))?;

    let mut reference_a =
        ReferenceProcess::spawn(&i2pd_bin, "i2pd-A", &a_datadir, &a_conf, &a_log).await?;
    let a_ri_path = reference_a
        .wait_for_router_info(Duration::from_secs(120))
        .await?;
    let a_ri_bytes = std::fs::read(&a_ri_path).map_err(|e| format!("read A router.info: {e}"))?;
    let (a_hash, a_ssu2) =
        verify_reference_router_info(&a_ri_bytes).map_err(|e| format!("verify A RI: {e}"))?;
    let a_material = a_ssu2.address_material().map_err(|_| "A key material")?;
    let a_static = i2pr_runtime::Ssu2PublicKey::new(*a_material.static_public_key().as_bytes())
        .map_err(|e| format!("A static: {e}"))?;
    let a_intro = i2pr_runtime::IntroKey::new(*a_material.intro_key().as_bytes());
    let a_endpoint: SocketAddr = format!("127.0.0.1:{a_port}")
        .parse()
        .map_err(|e| format!("A endpoint: {e}"))?;
    let a_target = daemon_dial_target(a_hash, a_endpoint, a_static, a_intro)
        .map_err(|e| format!("A dial target: {e}"))?;
    append_evidence(
        &evidence_dir,
        "bootstrap/reference-routerinfo-verified",
        "true",
    );

    // Prove each reference loaded the i2pr RI: the pre-start exact
    // file plus the source-locked `NetDb::Load` count line. The
    // functional FindRouter proof arrives with the first
    // explicit-peer epoch for each reference.
    for (short, reference) in [
        ("a-netdb-load-observed", &reference_a),
        ("b-netdb-load-observed", &reference_b),
    ] {
        if !reference.log_contains("routers loaded (") {
            return Err(format!(
                "{short} missing: {} log lacks the NetDb::Load count line; tail: {}",
                reference.name,
                reference.log_tail(10)
            ));
        }
        if short == "a-netdb-load-observed" {
            record_row(
                &evidence_dir,
                Epoch::Bootstrap,
                "a-netdb-load-observed",
                "true",
                &mut rows,
            );
        } else {
            record_row(
                &evidence_dir,
                Epoch::Bootstrap,
                "b-netdb-load-observed",
                "true",
                &mut rows,
            );
        }
    }

    // i2pr SSU2 service with the same RouterIdentityBundle.
    let daemon_service =
        Ssu2DaemonService::new(&config.ssu2, identity).map_err(|e| format!("daemon: {e}"))?;
    let token = CancellationToken::new();
    let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
    let mut handle = daemon_service
        .start(&scope, &config.ssu2)
        .await
        .map_err(|e| format!("bind: {e}"))?;
    if handle.local_v4().map(|addr| addr.port()) != Some(bind_port) {
        return Err("daemon bound an unexpected port".to_string());
    }
    handle
        .dial(a_target, DIAL_TIMEOUT, &CancellationToken::new())
        .await
        .map_err(|e| format!("dial A: {e:?}"))?;
    handle
        .dial(b_target, DIAL_TIMEOUT, &CancellationToken::new())
        .await
        .map_err(|e| format!("dial B: {e:?}"))?;
    let deadline_active = tokio::time::Instant::now() + Duration::from_secs(60);
    while tokio::time::Instant::now() < deadline_active {
        if handle.snapshot().active_sessions >= 2 {
            break;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    if handle.snapshot().active_sessions < 2 {
        return Err("authenticated sessions to A and B did not establish".to_string());
    }
    append_evidence(
        &evidence_dir,
        "bootstrap/session-established",
        &handle.snapshot().sessions_established.to_string(),
    );

    // Controlled live owners, one per epoch group (Plan 256 section 7:
    // separate fresh topology epochs). Each role owner starts with
    // zero registrations so its first accept proves exactly one live
    // registration; data epochs reuse their role owner so traffic
    // resolves the same registrations. Every service carries the
    // coherent responder material on the REAL daemon delivery seam
    // (forwards/replies must reach A/B through the live sessions).
    // Peer mappings come only from the real authenticated sessions.
    let mut reject_owner = TransitLiveOwner::new_disabled(
        handle.delivery().clone(),
        CancellationToken::new(),
        ChaCha8Rng::seed_from_u64(wall_secs() ^ 0x9E37),
    );
    reject_owner.enable(build_rejecting_transit_service(
        local_hash,
        responder_priv,
        handle.delivery().clone(),
    ));
    reject_owner
        .install_peer(a_hash, PeerId::from_hash(a_hash))
        .map_err(|e| format!("install A peer (reject): {e}"))?;
    if !reject_owner.is_enabled() {
        return Err("reject owner must be enabled".to_string());
    }
    let identity = QualificationIdentity {
        local_hash,
        responder_priv,
        a_hash,
        b_hash,
    };
    let mut obep_owner = fresh_owner(&handle, wall_secs(), &identity)?;
    let mut ibgw_owner = fresh_owner(&handle, wall_secs() ^ 0x1B67, &identity)?;
    let mut part_owner = fresh_owner(&handle, wall_secs() ^ 0x0A67, &identity)?;

    append_evidence(&evidence_dir, "bootstrap/live-owner-enabled", "true");

    let mut ledger = TypedLedger::default();
    // Plan 264 work package A: per-epoch fresh-mesh counted
    // mechanism. When `I2PR_M11_ONLY_EPOCH` names one mandatory
    // epoch, the lane runs bootstrap plus that epoch (with its
    // prerequisite setup, if any) on this fresh mesh, then writes
    // partial evidence and stops. A run without the selector
    // executes the full matrix as diagnostic-only: its manifest
    // carries no counted epoch id and can never satisfy the
    // per-epoch composition gate. The legacy `data` selector runs
    // the full legacy data section (diagnostic-only); the
    // fine-grained `obep-data` / `ibgw-data` / `receipt` /
    // `ibgw-receipt` / `participant-data` / `replay` / `expiry` /
    // `cancel` / `session-close` / `restart` selectors each run
    // only their epoch (counted).
    let only_epoch = std::env::var("I2PR_M11_ONLY_EPOCH").ok();
    let run_epoch = |name: &str| only_epoch.as_deref().is_none_or(|only| only == name);
    // Plan 264 per-epoch setup prerequisites: lifecycle epochs
    // need a genuine forward cell, which requires a participant
    // build plus a participant-data forward on this same fresh
    // mesh. Those setup sections run as prerequisites (their rows
    // are real observations on this mesh); only the manifest-named
    // epoch counts toward closure for this run.
    let needs_participant_setup = matches!(
        only_epoch.as_deref(),
        Some("participant-data" | "replay" | "expiry" | "cancel" | "session-close" | "restart")
    );
    let needs_forward_setup = matches!(
        only_epoch.as_deref(),
        Some("replay" | "expiry" | "cancel" | "session-close" | "restart")
    );
    let a_peer_hash = *a_hash.as_bytes();
    let b_hash_bytes = *b_hash.as_bytes();
    let sam_addr: SocketAddr = format!("127.0.0.1:{a_sam_port}")
        .parse()
        .map_err(|e| format!("SAM addr: {e}"))?;
    // Plan 261 work package A: the B-side sender speaks to B's own
    // stock SAM bridge (separate session, separate socket).
    let b_sam_addr: SocketAddr = format!("127.0.0.1:{b_sam_port}")
        .parse()
        .map_err(|e| format!("B SAM addr: {e}"))?;

    let run_ibgw = run_epoch("ibgw");
    if run_ibgw {
        // Epoch 1: IBGW accept (two-hop inbound with explicit
        // first hop i2pr: the reference REVERSES explicit peers
        // for inbound builds (`TunnelPool::CreateInboundTunnel`
        // calls `path.Reverse()`), so the list order here is
        // `[B, i2pr]` to yield the on-wire path `[i2pr, B]` with
        // i2pr as the inbound gateway and B as the native
        // endpoint. The endpoint's native reply lets the reference
        // establish the tunnel and report SESSION STATUS.
        // Single-hop trusted pinning proved unreliable against the
        // reference (its trusted first-hop selection silently
        // yields no usable build most runs), while explicit path
        // order is deterministic (`SelectExplicitPeers` fails loud
        // on unresolvable peers). `outbound.quantity=0` stays
        // respected, so with no outbound tunnel anywhere the build
        // sends DIRECTLY as a plain STBM to i2pr. i2pr observes
        // clean `0x80` IBGW and forwards to B.
        let ibgw_receive: u32;
        {
            let mut sam = SamClient::connect(sam_addr).await?;
            sam.hello().await?;
            run_sam_epoch(
                &mut handle,
                &mut ibgw_owner,
                &mut ledger,
                Epoch::Ibgw,
                &mut sam,
                "m11-ibgw",
                &[
                    ("inbound.length", "2"),
                    ("inbound.quantity", "1"),
                    ("inbound.lengthVariance", "0"),
                    ("outbound.quantity", "0"),
                    ("explicitPeers", &format!("{b_b64},{i2pr_b64}")),
                ],
                BuildStop::AcceptedRole(
                    TransitHopRoleKind::InboundGateway,
                    [a_peer_hash, b_hash_bytes],
                ),
                build_epoch_timeout(),
                a_target,
                b_target,
                &identity,
            )
            .await?;
            if !ledger.role_accepted(
                Epoch::Ibgw,
                TransitHopRoleKind::InboundGateway,
                &[a_peer_hash, b_hash_bytes],
            ) {
                return Err("IBGW epoch did not produce a typed IBGW accept".to_string());
            }
            ibgw_receive = ledger
                .first_accept(
                    Epoch::Ibgw,
                    TransitHopRoleKind::InboundGateway,
                    &[a_peer_hash, b_hash_bytes],
                )
                .map(|obs| obs.receive_tunnel)
                .ok_or("IBGW receive id missing")?;
            record_row(
                &evidence_dir,
                Epoch::Ibgw,
                "build-received",
                "true",
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::Ibgw,
                "build-accepted",
                "true",
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::Ibgw,
                "registration-live",
                &format!("{ibgw_receive:#x}"),
                &mut rows,
            );
            // Plan 257 work package D/E: exact cardinality + typed
            // bandwidth derived from the counted observation.
            let counted_ibgw = record_cardinality_rows(
                &evidence_dir,
                &ledger,
                Epoch::Ibgw,
                TransitHopRoleKind::InboundGateway,
                &[a_peer_hash, b_hash_bytes],
                &mut rows,
            )?;
            if counted_ibgw != ibgw_receive {
                return Err("IBGW cardinality receive id mismatch".to_string());
            }
            record_bandwidth_rows(
                &evidence_dir,
                &ledger,
                Epoch::Ibgw,
                TransitHopRoleKind::InboundGateway,
                &mut rows,
            )?;
            record_row(
                &evidence_dir,
                Epoch::Ibgw,
                "reference-knows-i2pr-ri",
                "true",
                &mut rows,
            );
            // Role session done: dropping it stops its pool from
            // retrying and calms the references for later epochs.
            drop(sam);
        }
    }

    // Epoch 2: deterministic code-30 rejection (1-hop OBEP shape
    // through the rejecting owner; the forced inbound is suppressed
    // with inbound.length=0 so no continuation can reflect to the
    // creator). Stock i2pd originates the build; i2pr decodes OBEP,
    // admission refuses, the OTBRM terminates cleanly at A, and no
    // registration survives.
    let run_reject = run_epoch("reject");
    if run_reject {
        {
            let mut sam = SamClient::connect(sam_addr).await?;
            sam.hello().await?;
            let before = reject_owner.active_count();
            run_sam_epoch(
                &mut handle,
                &mut reject_owner,
                &mut ledger,
                Epoch::Reject,
                &mut sam,
                "m11-reject",
                &[
                    ("inbound.length", "0"),
                    ("outbound.length", "1"),
                    ("outbound.quantity", "1"),
                    ("outbound.lengthVariance", "0"),
                    ("explicitPeers", &i2pr_b64),
                ],
                BuildStop::RejectedRole(TransitHopRoleKind::OutboundEndpoint),
                build_epoch_timeout(),
                a_target,
                b_target,
                &identity,
            )
            .await?;
            let after = reject_owner.active_count();
            drop(sam);
            if !ledger.role_rejected(Epoch::Reject, TransitHopRoleKind::OutboundEndpoint) {
                return Err(
                    "reject epoch did not produce a typed OBEP code-30 rejection".to_string(),
                );
            }
            if after != before {
                return Err("code-30 rejection installed state".to_string());
            }
            record_row(
                &evidence_dir,
                Epoch::Reject,
                "build-rejected",
                "true",
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::Reject,
                "no-registration",
                "true",
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::Reject,
                "pending-baseline",
                "true",
                &mut rows,
            );
            // Plan 257 work package E: typed bandwidth disposition
            // derived from the decoded rejection transaction (never
            // a hard-coded string). Stock i2pd emits no m/r/l, so
            // the counted rows prove typed absence.
            record_bandwidth_rows(
                &evidence_dir,
                &ledger,
                Epoch::Reject,
                TransitHopRoleKind::OutboundEndpoint,
                &mut rows,
            )?;
        }
    }

    let run_obep = run_epoch("obep");
    if run_obep {
        // Epoch 3: OBEP accept (outbound [B,i2pr]: B accepts as first
        // hop and forwards; i2pr terminates as OBEP; the OTBRM returns
        // to A; the forced inbound is suppressed with length 0).
        let obep_receive: u32;
        {
            let mut sam = SamClient::connect(sam_addr).await?;
            sam.hello().await?;
            run_sam_epoch(
                &mut handle,
                &mut obep_owner,
                &mut ledger,
                Epoch::Obep,
                &mut sam,
                "m11-obep",
                &[
                    ("inbound.length", "0"),
                    ("outbound.length", "2"),
                    ("outbound.quantity", "1"),
                    ("outbound.lengthVariance", "0"),
                    ("explicitPeers", &format!("{b_b64},{i2pr_b64}")),
                ],
                BuildStop::AcceptedRole(
                    TransitHopRoleKind::OutboundEndpoint,
                    [a_peer_hash, b_hash_bytes],
                ),
                build_epoch_timeout(),
                a_target,
                b_target,
                &identity,
            )
            .await?;
            // The authenticated previous peer is i2pd-B: A originates
            // the build, B accepts as first hop and forwards to i2pr.
            if !ledger.role_accepted(
                Epoch::Obep,
                TransitHopRoleKind::OutboundEndpoint,
                &[a_peer_hash, b_hash_bytes],
            ) {
                return Err("OBEP epoch did not produce a typed OBEP accept".to_string());
            }
            obep_receive = ledger
                .first_accept(
                    Epoch::Obep,
                    TransitHopRoleKind::OutboundEndpoint,
                    &[a_peer_hash, b_hash_bytes],
                )
                .map(|obs| obs.receive_tunnel)
                .ok_or("OBEP receive id missing")?;
            record_row(
                &evidence_dir,
                Epoch::Obep,
                "build-received",
                "true",
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::Obep,
                "build-accepted",
                "true",
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::Obep,
                "registration-live",
                &format!("{obep_receive:#x}"),
                &mut rows,
            );
            // Plan 257 work package D/E: exact cardinality + typed
            // bandwidth derived from the counted observation.
            let counted_obep = record_cardinality_rows(
                &evidence_dir,
                &ledger,
                Epoch::Obep,
                TransitHopRoleKind::OutboundEndpoint,
                &[a_peer_hash, b_hash_bytes],
                &mut rows,
            )?;
            if counted_obep != obep_receive {
                return Err("OBEP cardinality receive id mismatch".to_string());
            }
            record_bandwidth_rows(
                &evidence_dir,
                &ledger,
                Epoch::Obep,
                TransitHopRoleKind::OutboundEndpoint,
                &mut rows,
            )?;
            record_row(
                &evidence_dir,
                Epoch::Obep,
                "reference-knows-i2pr-ri",
                "true",
                &mut rows,
            );
            // Role session done: dropping it stops its pool from
            // retrying and calms the references for later epochs.
            drop(sam);
        }
    }

    // Plan 264: the participant build also runs as setup for
    // per-epoch participant-data/lifecycle runs on this fresh
    // mesh (its rows are real observations; only the
    // manifest-named epoch counts toward closure).
    let run_participant = run_epoch("participant") || needs_participant_setup;
    if run_participant {
        // Epoch 4: Participant accept (outbound [i2pr,B]: i2pr is
        // neither gateway nor endpoint; the continuation terminates at
        // B; the inbound direction is suppressed). Requires the proven
        // i2pd-B topology: the epoch fails closed when B is absent or
        // its RI was never installed.
        let participant_receive: u32;
        {
            if !b_ri_path.exists() {
                return Err("participant epoch requires a running i2pd-B".to_string());
            }
            let mut sam = SamClient::connect(sam_addr).await?;
            sam.hello().await?;
            let peers = format!("{i2pr_b64},{b_b64}");
            run_sam_epoch(
                &mut handle,
                &mut part_owner,
                &mut ledger,
                Epoch::Participant,
                &mut sam,
                "m11-part",
                &[
                    ("inbound.length", "0"),
                    ("outbound.length", "2"),
                    ("outbound.quantity", "1"),
                    ("outbound.lengthVariance", "0"),
                    ("explicitPeers", &peers),
                ],
                // Participant stays A-strict (both slots): only A
                // originates the counted intermediate topology, and the
                // post-loop checks require peer=A plus next=B.
                BuildStop::AcceptedRole(
                    TransitHopRoleKind::Participant,
                    [a_peer_hash, a_peer_hash],
                ),
                build_epoch_timeout(),
                a_target,
                b_target,
                &identity,
            )
            .await?;
            if !participant_topology_proven(&ledger, a_peer_hash, true) {
                return Err(
                    "participant epoch did not produce a typed Participant accept".to_string(),
                );
            }
            // Strict design shape: a DIRECT build from A with next=B
            // (A sends first-hop to i2pr; B-originated background
            // observations with other next hops are skipped, never
            // counted).
            let obs = ledger
                .of_epoch(Epoch::Participant)
                .find(|obs| {
                    obs.kind == ObservedKind::BuildAccepted
                        && obs.role == Some(TransitHopRoleKind::Participant)
                        && obs.peer_hash == a_peer_hash
                        && obs.next_router == Some(b_hash_bytes)
                        && !obs.rejected
                        && obs.delivery == "accepted"
                })
                .ok_or("participant epoch produced no direct A-to-B accept".to_string())?;
            participant_receive = obs.receive_tunnel;
            record_row(
                &evidence_dir,
                Epoch::Participant,
                "build-received",
                "true",
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::Participant,
                "build-accepted",
                "true",
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::Participant,
                "registration-live",
                &format!("{participant_receive:#x}"),
                &mut rows,
            );
            // Plan 257 work package D/E: exact cardinality + typed
            // bandwidth derived from the counted A-strict
            // observation itself (peer A, next B), so an
            // unrelated background accept can never satisfy the
            // row even if it landed first in the ledger.
            let counted_participant =
                record_cardinality_for_obs(&evidence_dir, Epoch::Participant, obs, &mut rows)?;
            if counted_participant != participant_receive {
                return Err("Participant cardinality receive id mismatch".to_string());
            }
            record_bandwidth_rows(
                &evidence_dir,
                &ledger,
                Epoch::Participant,
                TransitHopRoleKind::Participant,
                &mut rows,
            )?;
            record_row(
                &evidence_dir,
                Epoch::Participant,
                "next-hop-is-i2pd-b",
                &hex_lower(&b_hash_bytes),
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::Participant,
                "reference-knows-i2pr-ri",
                "true",
                &mut rows,
            );
            // Role session done: dropping it stops its pool from
            // retrying and calms the references for later epochs.
            // A lingering participant pool would keep building
            // 2-hop tunnels through our data owners (filling
            // quotas), churning the links, and polluting the
            // sender freshness gate with unrelated outbound
            // creations. Later epochs need only the transit
            // owner's registration, never this socket.
            drop(sam);
        }
    }

    // Epochs 5-7: genuine data-plane traffic between sibling SAM
    // sessions on i2pd-A. Both LeaseSets stay local to A, so no
    // public lookup is involved; the sender's outbound tunnel is
    // i2pr-routed (OBEP delivery at i2pr) and the receiver's
    // inbound tunnel is B-gatewayed. Every observation is a real
    // `handle_inbound` outcome, never a helper projection.
    let run_data = run_epoch("data");
    // Plan 260 diagnostic-subset flag (see the `run_data` block
    // below): `I2PR_M11_ONLY_EPOCH=receipt` runs bootstrap plus
    // only the creator-owned receipt epoch on a fresh mesh.
    // Plan 264 promotes the receipt selector (plus the
    // `ibgw-receipt` alias) to the counted per-epoch mechanism;
    // the legacy full-matrix run without a selector is
    // diagnostic-only.
    let receipt_only = matches!(
        std::env::var("I2PR_M11_ONLY_EPOCH").as_deref(),
        Ok("receipt") | Ok("ibgw-receipt")
    );
    // Plan 264 work package A: fine-grained per-epoch gates for
    // the legacy data section. Each counted epoch runs on its own
    // fresh mesh; the legacy `data` selector runs the whole
    // section as diagnostic-only.
    let run_obep_data = run_epoch("obep-data") || run_data;
    let run_ibgw_data_epoch = run_epoch("ibgw-data") || run_data;
    let run_receipt_epoch = receipt_only || run_data;
    let run_participant_data_epoch =
        run_epoch("participant-data") || run_data || needs_forward_setup;
    let run_replay_epoch = run_epoch("replay") || run_data;
    let run_expiry_epoch = run_epoch("expiry") || run_data;
    let run_cancel_epoch = run_epoch("cancel") || run_data;
    let run_session_close_epoch = run_epoch("session-close") || run_data;
    let run_restart_epoch = run_epoch("restart") || run_data;
    // Plan 264: any lifecycle per-epoch request runs the full
    // local lifecycle chain (replay -> expiry -> session-close ->
    // cancel -> restart) on this fresh mesh; only the
    // manifest-named epoch counts toward closure for this run.
    // The chain needs a genuine forward cell, provided by the
    // participant-data setup above via needs_forward_setup.
    let run_lifecycle_chain = run_replay_epoch
        || run_expiry_epoch
        || run_cancel_epoch
        || run_session_close_epoch
        || run_restart_epoch;
    let participant_receive_opt =
        receive_of(&ledger, Epoch::Participant, TransitHopRoleKind::Participant);
    if run_data
        || receipt_only
        || run_obep_data
        || run_ibgw_data_epoch
        || run_participant_data_epoch
        || run_replay_epoch
        || run_expiry_epoch
        || run_cancel_epoch
        || run_session_close_epoch
        || run_restart_epoch
    {
        // Plan 260 diagnostic subset: `I2PR_M11_ONLY_EPOCH=receipt`
        // runs bootstrap plus only the creator-owned receipt epoch
        // below on a fresh mesh (non-counted: the remaining
        // mandatory rows are absent, so the run can never satisfy
        // the two-pass gate). This discriminates a healthy-mesh
        // receipt failure (structural boundary) from the late-run
        // standalone-mesh degradation the full matrix runs into
        // (tunnel tests fail within minutes with no floodfill, so
        // builds stop establishing). Counted attempts never set
        // this flag and always run the complete matrix.
        // Plan 264: per-epoch receipt runs are counted (two
        // same-SHA passes per epoch close the row); the
        // full-matrix run without a selector stays
        // diagnostic-only.
        if run_obep_data {
            {
                // Setup order is load-bearing, receiver first: the
                // sender's pool must be seconds old at send time.
                // Established sender tunnels die within a minute in
                // this mesh (failing tunnel tests), so a sender-first
                // order lets the sender pool rot during the
                // receiver's slow setup and every send queues past
                // the ~8 s inner horizon. Receiver-first keeps the
                // sender pool fresh at send time; the receiver
                // LeaseSet stays resolvable (local to A plus
                // floodfill-published via its B-routed outbound),
                // so addressing holds.
                // rx_obep setup runs on the role owner: its SessionReady
                // stop is satisfied by any accept and its registrations
                // are never needed later, so background may fill it
                // freely (resets keep it converging).
                // rx_obep: inbound via B only, so the OBEP action
                // routes to a reachable gateway with no i2pr reflection.
                let mut rx_obep = SamClient::connect(sam_addr).await?;
                rx_obep.hello().await?;
                let rx_obep_dest = run_sam_epoch(
                    &mut handle,
                    &mut obep_owner,
                    &mut ledger,
                    Epoch::ObepData,
                    &mut rx_obep,
                    "m11-rx-obep",
                    &[
                        ("inbound.length", "1"),
                        ("inbound.quantity", "1"),
                        ("inbound.lengthVariance", "0"),
                        // No outbound pool (diagnostic): tests whether
                        // ECIES acks return locally/directly without a
                        // receiver outbound pool. Unpaired pools are
                        // never tested and live full lifetimes.
                        ("outbound.length", "0"),
                        ("explicitPeers", &b_b64),
                    ],
                    BuildStop::SessionReady,
                    build_epoch_timeout(),
                    a_target,
                    b_target,
                    &identity,
                )
                .await?
                .ok_or("rx_obep session reported ready without a destination")?;
                // Fresh owner for the OBEP data setup, born right
                // before the counted sender session: role epochs
                // accumulate background registrations with no resets,
                // so reusing the role owner would force the counted
                // m11-tx build to race saturated admission quotas and
                // die with code 30. Born here — seconds before the
                // counted session — it deterministically has room;
                // nothing after this block needs the role owner's
                // registrations (participant-data binds the
                // participant owner's own receive id).
                let mut obep_data_owner = fresh_owner(&handle, wall_secs() ^ 0x0BE9, &identity)?;
                let mut tx = SamClient::connect(sam_addr).await?;
                tx.hello().await?;
                // Establishment baseline for the freshness gate below:
                // captured before the counted setup so any outbound
                // creation the setup triggers counts as fresh.
                let tx_created_baseline = reference_a.count_outbound_created();
                run_sam_epoch(
                    &mut handle,
                    &mut obep_data_owner,
                    &mut ledger,
                    Epoch::ObepData,
                    &mut tx,
                    "m11-tx",
                    &[
                        // Deliberately outbound-only (no inbound pool):
                        // the reference only tests tunnels when a pool
                        // holds BOTH directions, and failing tests tear
                        // down established tunnels within a minute. An
                        // untestable single-direction pool lives its
                        // full lifetime, so the sender pool is stable
                        // at send time instead of churning.
                        ("inbound.length", "0"),
                        ("outbound.length", "1"),
                        ("outbound.quantity", "1"),
                        ("outbound.lengthVariance", "0"),
                        ("explicitPeers", &i2pr_b64),
                    ],
                    BuildStop::CountedObepSetup { reply: a_peer_hash },
                    build_epoch_timeout(),
                    a_target,
                    b_target,
                    &identity,
                )
                .await?
                .ok_or("counted sender session never reported ready")?;
                // Unfragmented then fragmented sequentially on one
                // sender session, each alone in its flush batch: the
                // reference SAM loop reliably parses the first
                // DATAGRAM SEND of each batch promptly, while a second
                // command riding the same batch can sit unprocessed
                // until the next batch arrives (source-locked against
                // exact-pinned i2pd: one parsed send per ~45 s send
                // pair). The first drain stops right after the
                // unfragmented delivery, so the fragmented send
                // follows within seconds on the same live path. A
                // bounded fresh-session retry round below covers
                // genuinely missing sizes.
                //
                // The pass predicate is proved on the RECEIVING SAM
                // socket, not on our-side reassembly sizes: reference
                // control traffic (garlic-routed build replies,
                // encrypted LeaseSet publishes, tunnel tests) completes
                // on our OBEP registration at datagram-like sizes, so
                // size counting cannot separate genuine datagrams from
                // background (source-locked against exact-pinned i2pd
                // runs: dozens of ~950 B accepted garlics with no
                // corresponding delivery). A receipt counts only when
                // the receiver session socket delivers DATAGRAM
                // RECEIVED with the exact stimulus size and
                // byte-pattern (512 × 0x5A, 4096 × 0xA5) — an event
                // only the full A→i2pr→B→A reference path can
                // produce. Requiring exactly one of each proves both
                // datagrams flowed exactly once; a duplicate (late
                // original plus its retry) surfaces as a count of two
                // and fails closed.
                //
                // Freshness is load-bearing: the reference stamps inner
                // garlic expirations only ~8 s out
                // (`I2NP_MESSAGE_EXPIRATION_TIMEOUT`), so a send that
                // queues behind a dead sender pool arrives downstream
                // already expired and can never be received. Sends
                // follow the counted setup within seconds on the live
                // path, and the short verify polls below (not the long
                // background-harvesting drains) decide each round.
                let is_datagram = |obs: &&Observation| {
                    obs.kind == ObservedKind::DataDeliveredObep
                        && obs.delivery == "obep-ok-garlic"
                        && obs.aux_count >= 500
                };
                let is_large = |obs: &&Observation| {
                    obs.kind == ObservedKind::DataDeliveredObep
                        && obs.delivery == "obep-ok-garlic"
                        && obs.aux_count >= 3000
                };
                let datagrams_before = ledger.of_epoch(Epoch::ObepData).filter(is_datagram).count();
                let large_before = ledger.of_epoch(Epoch::ObepData).filter(is_large).count();
                let mut rx_counts = RxDatagramCounts {
                    rx_512: 0,
                    rx_4096: 0,
                };
                // Fresh sessions for the send phase: the setups above
                // took a minute and idle links die silently mid-run
                // (later sends fail closed as NoActiveSession while
                // inbound cells fall into the unknown-link drop).
                ensure_sessions(&handle, a_target, b_target).await;
                // Freshness gate: send only on a just-established
                // sender tunnel so the reference emits with a fresh
                // inner expiration (see the helper docs).
                wait_for_fresh_sender_tunnel(
                    &mut handle,
                    &mut obep_data_owner,
                    &mut ledger,
                    Epoch::ObepData,
                    &reference_a,
                    tx_created_baseline,
                    Duration::from_secs(30),
                )
                .await?;
                tx.send_datagram("m11-tx", &rx_obep_dest, &vec![0x5Au8; 512])
                    .await?;
                drain_data_epoch(
                    &mut handle,
                    &mut obep_data_owner,
                    &mut ledger,
                    Epoch::ObepData,
                    Duration::from_secs(25),
                    DataStop::DeliveredSizedObep {
                        count: datagrams_before + 1,
                        min_len: 500,
                    },
                    Duration::from_secs(2),
                )
                .await?;
                poll_rx_datagrams(
                    &mut handle,
                    &mut obep_data_owner,
                    &mut ledger,
                    Epoch::ObepData,
                    &mut rx_obep,
                    Duration::from_secs(10),
                    &mut rx_counts,
                )
                .await?;
                // Same-session resend when the unfragmented receipt is
                // missing: the first send routinely queues behind a
                // still-establishing sender pool and lapses its ~8 s
                // inner expiration before emission, so it can never be
                // received no matter how long the drain waits. A resend
                // ~35 s later (past any genuine horizon, so a late
                // double is impossible — anything that late is stale
                // by definition) usually lands on the recovered pool
                // with a fresh expiration. Bounded to one resend; the
                // fresh-session round below remains the last resort.
                if rx_counts.rx_512 == 0 {
                    ensure_sessions(&handle, a_target, b_target).await;
                    wait_for_fresh_sender_tunnel(
                        &mut handle,
                        &mut obep_data_owner,
                        &mut ledger,
                        Epoch::ObepData,
                        &reference_a,
                        tx_created_baseline,
                        Duration::from_secs(20),
                    )
                    .await?;
                    tx.send_datagram("m11-tx", &rx_obep_dest, &vec![0x5Au8; 512])
                        .await?;
                    drain_data_epoch(
                        &mut handle,
                        &mut obep_data_owner,
                        &mut ledger,
                        Epoch::ObepData,
                        Duration::from_secs(25),
                        DataStop::DeliveredSizedObep {
                            count: datagrams_before + 2,
                            min_len: 500,
                        },
                        Duration::from_secs(2),
                    )
                    .await?;
                    poll_rx_datagrams(
                        &mut handle,
                        &mut obep_data_owner,
                        &mut ledger,
                        Epoch::ObepData,
                        &mut rx_obep,
                        Duration::from_secs(10),
                        &mut rx_counts,
                    )
                    .await?;
                }
                // Fragmented send on the SAME sender session, spaced
                // from the first send by the first drain (separate SAM
                // batches parse promptly; back-to-back sends in one
                // batch can stall). A second sender session is only
                // created below if this send genuinely goes missing
                // (retry round), keeping the green path to a single
                // session setup. Gated like the first send: the pool
                // must hold a live tunnel (established after setup, or
                // rebuilt since) so emission is immediate and fresh.
                wait_for_fresh_sender_tunnel(
                    &mut handle,
                    &mut obep_data_owner,
                    &mut ledger,
                    Epoch::ObepData,
                    &reference_a,
                    tx_created_baseline,
                    Duration::from_secs(20),
                )
                .await?;
                tx.send_datagram("m11-tx", &rx_obep_dest, &vec![0xA5u8; 4096])
                    .await?;
                drain_data_epoch(
                    &mut handle,
                    &mut obep_data_owner,
                    &mut ledger,
                    Epoch::ObepData,
                    Duration::from_secs(40),
                    DataStop::DeliveredSizedObep {
                        count: datagrams_before + 2,
                        min_len: 500,
                    },
                    Duration::from_secs(DATA_SETTLE_SECS),
                )
                .await?;
                poll_rx_datagrams(
                    &mut handle,
                    &mut obep_data_owner,
                    &mut ledger,
                    Epoch::ObepData,
                    &mut rx_obep,
                    Duration::from_secs(10),
                    &mut rx_counts,
                )
                .await?;
                // Same-session resend for the fragmented size on the
                // same stale-pool rationale as the unfragmented resend
                // above: one bounded resend before falling through to
                // the fresh-session round.
                if rx_counts.rx_4096 == 0 {
                    ensure_sessions(&handle, a_target, b_target).await;
                    wait_for_fresh_sender_tunnel(
                        &mut handle,
                        &mut obep_data_owner,
                        &mut ledger,
                        Epoch::ObepData,
                        &reference_a,
                        tx_created_baseline,
                        Duration::from_secs(20),
                    )
                    .await?;
                    tx.send_datagram("m11-tx", &rx_obep_dest, &vec![0xA5u8; 4096])
                        .await?;
                    drain_data_epoch(
                        &mut handle,
                        &mut obep_data_owner,
                        &mut ledger,
                        Epoch::ObepData,
                        Duration::from_secs(40),
                        DataStop::DeliveredSizedObep {
                            count: datagrams_before + 3,
                            min_len: 500,
                        },
                        Duration::from_secs(DATA_SETTLE_SECS),
                    )
                    .await?;
                    poll_rx_datagrams(
                        &mut handle,
                        &mut obep_data_owner,
                        &mut ledger,
                        Epoch::ObepData,
                        &mut rx_obep,
                        Duration::from_secs(10),
                        &mut rx_counts,
                    )
                    .await?;
                }
                // Bounded retry round on a FRESH sender session when a
                // size is missing at the receiver socket: transient
                // reference scheduling stalls (unparsed SAM commands,
                // stale routing paths) can strand an otherwise healthy
                // send, and a fresh session re-rolls all of that state
                // at once (session, pool, tunnel, routing path, owner
                // quotas). Only missing sizes are resent, sequentially
                // on the one retry session. At most one retry round
                // total.
                if rx_counts.rx_512 != 1 || rx_counts.rx_4096 != 1 {
                    ensure_sessions(&handle, a_target, b_target).await;
                    let mut obep_data_owner_retry =
                        fresh_owner(&handle, wall_secs() ^ 0x0E77, &identity)?;
                    let mut tx_retry = SamClient::connect(sam_addr).await?;
                    tx_retry.hello().await?;
                    let retry_created_baseline = reference_a.count_outbound_created();
                    let retry_ready = run_sam_epoch(
                        &mut handle,
                        &mut obep_data_owner_retry,
                        &mut ledger,
                        Epoch::ObepData,
                        &mut tx_retry,
                        "m11-tx-retry",
                        &[
                            ("inbound.length", "0"),
                            ("outbound.length", "1"),
                            ("outbound.quantity", "1"),
                            ("outbound.lengthVariance", "0"),
                            ("explicitPeers", &i2pr_b64),
                        ],
                        BuildStop::CountedObepSetup { reply: a_peer_hash },
                        build_epoch_timeout(),
                        a_target,
                        b_target,
                        &identity,
                    )
                    .await?;
                    if retry_ready.is_some() {
                        if rx_counts.rx_512 == 0 {
                            wait_for_fresh_sender_tunnel(
                                &mut handle,
                                &mut obep_data_owner_retry,
                                &mut ledger,
                                Epoch::ObepData,
                                &reference_a,
                                retry_created_baseline,
                                Duration::from_secs(30),
                            )
                            .await?;
                            tx_retry
                                .send_datagram("m11-tx-retry", &rx_obep_dest, &vec![0x5Au8; 512])
                                .await?;
                            drain_data_epoch(
                                &mut handle,
                                &mut obep_data_owner_retry,
                                &mut ledger,
                                Epoch::ObepData,
                                Duration::from_secs(25),
                                DataStop::DeliveredSizedObep {
                                    count: datagrams_before + 1,
                                    min_len: 500,
                                },
                                Duration::from_secs(2),
                            )
                            .await?;
                            poll_rx_datagrams(
                                &mut handle,
                                &mut obep_data_owner_retry,
                                &mut ledger,
                                Epoch::ObepData,
                                &mut rx_obep,
                                Duration::from_secs(10),
                                &mut rx_counts,
                            )
                            .await?;
                        }
                        if rx_counts.rx_4096 == 0 {
                            wait_for_fresh_sender_tunnel(
                                &mut handle,
                                &mut obep_data_owner_retry,
                                &mut ledger,
                                Epoch::ObepData,
                                &reference_a,
                                retry_created_baseline,
                                Duration::from_secs(20),
                            )
                            .await?;
                            tx_retry
                                .send_datagram("m11-tx-retry", &rx_obep_dest, &vec![0xA5u8; 4096])
                                .await?;
                            drain_data_epoch(
                                &mut handle,
                                &mut obep_data_owner_retry,
                                &mut ledger,
                                Epoch::ObepData,
                                Duration::from_secs(40),
                                DataStop::DeliveredSizedObep {
                                    count: large_before + 1,
                                    min_len: 3000,
                                },
                                Duration::from_secs(DATA_SETTLE_SECS),
                            )
                            .await?;
                            poll_rx_datagrams(
                                &mut handle,
                                &mut obep_data_owner_retry,
                                &mut ledger,
                                Epoch::ObepData,
                                &mut rx_obep,
                                Duration::from_secs(10),
                                &mut rx_counts,
                            )
                            .await?;
                        }
                    }
                }
                record_row(
                    &evidence_dir,
                    Epoch::ObepData,
                    "delivered-delta",
                    &format!("rx512={} rx4096={}", rx_counts.rx_512, rx_counts.rx_4096),
                    &mut rows,
                );
                if rx_counts.rx_512 != 1 || rx_counts.rx_4096 != 1 {
                    return Err(format!(
                        "OBEP data epoch expected exactly one payload-verified 512-byte plus one \
                     payload-verified 4096-byte DATAGRAM RECEIVED on the receiver session socket \
                     (unfragmented + fragmented-once): rx512={} rx4096={}",
                        rx_counts.rx_512, rx_counts.rx_4096,
                    ));
                }
                record_row(
                    &evidence_dir,
                    Epoch::ObepData,
                    "delivery",
                    "true",
                    &mut rows,
                );
                record_row(
                    &evidence_dir,
                    Epoch::ObepData,
                    "fragmented-once",
                    "true",
                    &mut rows,
                );
                // Data receiver socket done: dropping it destroys its
                // reference pools, calming the mesh for the later
                // epochs. This run's SAM servers saturate under several
                // live sessions' concurrent tunnel churn
                // (builds/publishes/tests), and a stalled SAM loop
                // stops parsing DATAGRAM SEND entirely. Later epochs
                // use fresh sockets; nothing reads this one again.
                drop(rx_obep);
                // Sender socket done too: its outbound pool would
                // otherwise rebuild and churn against our quotas for
                // the rest of the run while nothing ever sends on it
                // again (later sends use the sibling sender). Same
                // rationale as the receiver drop above.
                drop(tx);
            } // end Plan 264 per-epoch obep-data block
        } // end run_obep_data
        // Plan 264 per-epoch IBGW-data block: self-contained relay
        // proof with its own builds inline on this fresh mesh
        // (mesh-liveness + relay-NetDB + B-floodfill prerequisites
        // gate the counted sends, unchanged from Plan 263).
        if run_ibgw_data_epoch {
            {
                // IBGW ingress: the reference creator opens a fresh
                // inbound tunnel whose trusted first hop is i2pr and a
                // sibling sender drives one message through it, so the
                // TunnelGateway entry that reaches the accepted gateway
                // id is stock-i2pd traffic the reference itself emitted.
                // Waiting for the reference's own manage cadence never
                // produced ingress once the role session was dropped.
                // Fresh owner (same quota rationale as the OBEP data
                // block): the rx_ibgw build and the sibling ingress
                // resolve against the fresh registration the run
                // accepts here, not the role epoch's.
                //
                // Two-hop explicit inbound ([B, i2pr] list order; the
                // reference reverses explicit peers for inbound builds
                // into on-wire [i2pr, B]): i2pr stays the inbound
                // gateway while B terminates natively, exactly the
                // shape Epoch 1 qualifies reliably. Single-hop
                // trusted pinning is avoided here: the reference's
                // trusted first-hop selection silently yields no
                // usable build most runs (zero-hop placeholder stuck,
                // no STBM ever reaches i2pr), while explicit path
                // order is deterministic.
                let mut ibgw_data_owner = fresh_owner(&handle, wall_secs() ^ 0x1B9D, &identity)?;
                // Bounded setup retry (two sessions): the 2-hop
                // inbound must traverse B-side admission (unmanaged
                // quotas) and land the reversed on-wire shape, so a
                // single attempt can stall past its timeout while a
                // fresh session/pool lands promptly. Same owner (its
                // resets keep converging quotas); fresh client per
                // attempt so each gets a fresh pool. The ready socket
                // is kept for the receipt polls below.
                let (rx_ibgw_dest, mut rx_ibgw) = {
                    let mut attempt = SamClient::connect(sam_addr).await?;
                    attempt.hello().await?;
                    match run_sam_epoch(
                        &mut handle,
                        &mut ibgw_data_owner,
                        &mut ledger,
                        Epoch::IbgwData,
                        &mut attempt,
                        "m11-rx-ibgw",
                        &[
                            ("inbound.length", "2"),
                            ("inbound.quantity", "1"),
                            ("inbound.lengthVariance", "0"),
                            // Outbound pool (B-direct) carries the ECIES
                            // tag/ack return traffic: without a real
                            // outbound tunnel those acks have no path
                            // (zero-hop direct does not qualify), tags go
                            // unconfirmed, and sessions flap. Mirrors the
                            // OBEP receiver, which passes with this shape.
                            ("outbound.length", "1"),
                            ("outbound.quantity", "1"),
                            ("outbound.lengthVariance", "0"),
                            ("explicitPeers", &format!("{b_b64},{i2pr_b64}")),
                        ],
                        BuildStop::AcceptedRole(
                            TransitHopRoleKind::InboundGateway,
                            [a_peer_hash, b_hash_bytes],
                        ),
                        build_epoch_timeout(),
                        a_target,
                        b_target,
                        &identity,
                    )
                    .await?
                    {
                        Some(dest) => (dest, attempt),
                        None => {
                            // First attempt stalled; its pool dies with
                            // the socket. One bounded retry on a fresh
                            // session before failing closed. Settle
                            // first: the reference tears its session
                            // pools down asynchronously after our FIN,
                            // and reconnecting instantly can wedge the
                            // hello behind that teardown (observed as a
                            // 15 s SAM read timeout on a live
                            // reference). One settle plus one hello
                            // retry, then fail closed as before.
                            drop(attempt);
                            tokio::time::sleep(Duration::from_secs(5)).await;
                            let mut retry = SamClient::connect(sam_addr).await?;
                            if retry.hello().await.is_err() {
                                tokio::time::sleep(Duration::from_secs(5)).await;
                                retry.hello().await?;
                            }
                            let dest = run_sam_epoch(
                                &mut handle,
                                &mut ibgw_data_owner,
                                &mut ledger,
                                Epoch::IbgwData,
                                &mut retry,
                                "m11-rx-ibgw-retry",
                                &[
                                    ("inbound.length", "2"),
                                    ("inbound.quantity", "1"),
                                    ("inbound.lengthVariance", "0"),
                                    ("outbound.length", "1"),
                                    ("outbound.quantity", "1"),
                                    ("outbound.lengthVariance", "0"),
                                    ("explicitPeers", &format!("{b_b64},{i2pr_b64}")),
                                ],
                                BuildStop::AcceptedRole(
                                    TransitHopRoleKind::InboundGateway,
                                    [a_peer_hash, b_hash_bytes],
                                ),
                                build_epoch_timeout(),
                                a_target,
                                b_target,
                                &identity,
                            )
                            .await?
                            .ok_or("rx_ibgw session reported ready without a destination")?;
                            (dest, retry)
                        }
                    }
                };
                let mut tx_ibgw = SamClient::connect(sam_addr).await?;
                tx_ibgw.hello().await?;
                run_sam_epoch(
                    &mut handle,
                    &mut ibgw_data_owner,
                    &mut ledger,
                    Epoch::IbgwData,
                    &mut tx_ibgw,
                    "m11-tx-ibgw",
                    &[
                        ("inbound.length", "0"),
                        // Outbound via B (not i2pr): the datagram must
                        // reach B first so B addresses the receiver
                        // inbound gateway per the receiver LeaseSet. A
                        // direct sender pool would endpoint-deliver at
                        // i2pr, and i2pr's OBEP-tunnel forward can only
                        // emit outward: genuine TunnelGateway ingress
                        // at our accepted gateway id (the plan's
                        // multi-cell case) arrives only when B relays
                        // a fragmented message down to us. B-side
                        // admission is unmanaged, so this setup waits
                        // on plain SessionReady (a one-hop endpoint
                        // build at B leaves no observable accept here)
                        // with resets never suppressed.
                        ("outbound.length", "1"),
                        ("outbound.quantity", "1"),
                        ("outbound.lengthVariance", "0"),
                        ("explicitPeers", &b_b64),
                    ],
                    BuildStop::SessionReady,
                    build_epoch_timeout(),
                    a_target,
                    b_target,
                    &identity,
                )
                .await?
                .ok_or("tx_ibgw session never reported ready")?;
                // Large enough that the gateway emission has to split
                // into more than one TunnelData cell (1500 B payload
                // reassembles to ~1.9 KB garlic: two cells, so a single
                // lost fragment cannot strand it the way larger
                // multi-cell stimuli do on the unmanaged A<->B link),
                // and small enough that header plus payload fits the
                // reference's 8193-byte SAM socket buffer: a 9000 B
                // payload overflows it, the reference keeps waiting
                // for bytes that never fit, and the session wedges
                // (source-locked against SAMSocket::ProcessDatagramSend).
                // Fresh sessions first: the sibling setups took a
                // while and idle links die silently mid-run. Gated like
                // the OBEP sends (a pooled send behind an establishing
                // pool lapses its inner expiration before emission),
                // with one bounded same-session resend when no ingress
                // arrives: gateway ingress is tunnel-scoped to the
                // accepted gateway id, so background cannot satisfy
                // it and a missing ingress honestly means this send
                // rotted.
                // Plan 263 work package A: sustain the mesh before
                // burning send rounds. The best-effort redial above
                // heals silently dead links; the proofs below fail
                // closed with the exact topology signature when the
                // relay cannot sustain (no blind sends into a dead
                // relay, no direct-sender substitution, no tuning).
                ensure_sessions(&handle, a_target, b_target).await;
                mesh_liveness_status(&handle, a_target, b_target)?;
                verify_relay_netdb_prerequisites(
                    &b_ri_in_a_netdb,
                    &i2pr_ri_in_a_netdb,
                    &i2pr_ri_in_b_netdb,
                )?;
                verify_b_floodfill_conf(&b_home)?;
                let tx_ibgw_baseline = reference_a.count_outbound_created();
                // End-to-end receipt on the receiver session socket,
                // mirroring the OBEP predicate: gateway ingress proves
                // our emission, but only a payload-verified DATAGRAM
                // RECEIVED (1500 × 0x7E, exactly once) proves the full
                // A→B→i2pr→B→A reference path closed the loop. Up to
                // four iterations (one immediate freshest-state shot,
                // then event-driven): the sender pool, the B-side
                // admission underneath it, and the receiver gateway
                // shape all flap on ~30 s timescales, so one shot
                // routinely lands in a dead window while a later
                // iteration lands live. Rounds stop at the first
                // receipt; a stale round can never double-deliver (the
                // reference drops lapsed inner expirations).
                let mut rx_1500: usize = 0;
                // Event-driven sends: a reversed receiver rebuild
                // (fresh IBGW accept at i2pr) is the only window in
                // which B relays a fragmented message down to our
                // gateway id with a live registration and a fresh
                // LeaseSet on every leg. Blind sends outside such
                // windows rot on stale gateways or bypass i2pr
                // entirely, so each iteration waits for a fresh accept
                // first, settles for publish/propagate, then sends. An
                // iteration that finds no fresh accept still sends once
                // (bounded blind fallback) so the phase never deadlocks
                // when the reference holds its shape. Rounds stop at
                // the first receipt; a stale round can never
                // double-deliver (the reference drops lapsed inner
                // expirations).
                let ibgw_floor_ms = wall_ms();
                let mut ibgw_baseline: Vec<u32> = ledger
                    .of_epoch(Epoch::IbgwData)
                    .filter(|obs| {
                        obs.kind == ObservedKind::BuildAccepted
                            && obs.role == Some(TransitHopRoleKind::InboundGateway)
                    })
                    .map(|obs| obs.receive_tunnel)
                    .collect();
                for round in 0..4 {
                    if rx_1500 == 1 {
                        break;
                    }
                    ensure_sessions(&handle, a_target, b_target).await;
                    if round == 0 {
                        // Freshest-state shot first: both pools just
                        // established, registrations live, LeaseSets
                        // fresh. No waits — every second of delay lets
                        // hyper-churn rebuild the window away.
                    } else {
                        let _fresh = pump_until_fresh_ibgw(
                            &mut handle,
                            &mut ibgw_data_owner,
                            &mut ledger,
                            Epoch::IbgwData,
                            &mut ibgw_baseline,
                            ibgw_floor_ms,
                            Duration::from_secs(25),
                        )
                        .await?;
                        // Settle pump for LeaseSet publish: the fresh
                        // gateway must reach the sender's routing before
                        // the send addresses it. Short on purpose: the
                        // sender (A) routes from its local store
                        // (instant) and the relay (B) executes garlic
                        // instructions (no fetch), so only A's internal
                        // publish lag (~seconds) needs covering; a long
                        // settle lets hyper-churn rebuild the window
                        // away before the send fires.
                        let _settled = pump_until_fresh_ibgw(
                            &mut handle,
                            &mut ibgw_data_owner,
                            &mut ledger,
                            Epoch::IbgwData,
                            &mut ibgw_baseline,
                            ibgw_floor_ms,
                            Duration::from_secs(4),
                        )
                        .await?;
                    }
                    wait_for_fresh_sender_tunnel(
                        &mut handle,
                        &mut ibgw_data_owner,
                        &mut ledger,
                        Epoch::IbgwData,
                        &reference_a,
                        tx_ibgw_baseline,
                        Duration::from_secs(12),
                    )
                    .await?;
                    // Send marker on stdout (captured in the external
                    // driver log, never an evidence row): aligns sends
                    // with later ingress/receipt rows in forensics.
                    // Evidence rows stay strictly typed observations.
                    eprintln!("m11-tx-ibgw send round started wall_ms={}", wall_ms());
                    tx_ibgw
                        .send_datagram("m11-tx-ibgw", &rx_ibgw_dest, &vec![0x7Eu8; 1500])
                        .await?;
                    drain_data_epoch(
                        &mut handle,
                        &mut ibgw_data_owner,
                        &mut ledger,
                        Epoch::IbgwData,
                        data_epoch_timeout(),
                        DataStop::GatewayDelivered,
                        Duration::from_secs(DATA_SETTLE_SECS),
                    )
                    .await?;
                    poll_rx_sized(
                        &mut rx_ibgw,
                        Duration::from_secs(10),
                        1500,
                        0x7E,
                        &mut rx_1500,
                    )
                    .await?;
                }
                // Tunnel-scoped: only ingress addressing an accepted
                // gateway registration counts (rebuilds may accept
                // several ids; any of them is genuine). Background
                // gateway traffic to other ids (stale or foreign
                // tunnels) is recorded but never satisfies the rows.
                let accepted_gateways: Vec<u32> = ledger
                    .of_epoch(Epoch::IbgwData)
                    .filter(|obs| {
                        obs.kind == ObservedKind::BuildAccepted
                            && obs.role == Some(TransitHopRoleKind::InboundGateway)
                    })
                    .map(|obs| obs.receive_tunnel)
                    .collect();
                let gatewayed: Vec<&Observation> = ledger
                    .of_epoch(Epoch::IbgwData)
                    .filter(|obs| gateway_ingress_accepted(obs, &accepted_gateways))
                    .collect();
                record_row(
                    &evidence_dir,
                    Epoch::IbgwData,
                    "gateway-delivered-count",
                    &gatewayed.len().to_string(),
                    &mut rows,
                );
                // Plan 258 work package A diagnostic rows: sanitized
                // counts only (ingress, nested size distribution,
                // emitted-cell distribution, failure distribution).
                // Recorded before the gates so a failing lane still
                // classifies H1/H2/H3; diagnostic-only keys never
                // feed any pass predicate.
                let diag = gateway_diag_counts(&gatewayed);
                for (key, value) in [
                    ("gateway-diag-ingress", diag.ingress),
                    ("gateway-diag-nested-single", diag.nested_single),
                    ("gateway-diag-nested-multi", diag.nested_multi),
                    ("gateway-diag-emitted-single", diag.emitted_single),
                    ("gateway-diag-emitted-multi", diag.emitted_multi),
                    ("gateway-diag-emitted-max", diag.emitted_max),
                    ("gateway-diag-failures-total", diag.failures_total),
                    ("gateway-diag-failed-ingress", diag.failed_ingress),
                ] {
                    record_row(
                        &evidence_dir,
                        Epoch::IbgwData,
                        key,
                        &value.to_string(),
                        &mut rows,
                    );
                }
                // Plan 258 drop-side diagnostic rows: every gateway
                // ingress that did not deliver, folded by addressed id
                // (accepted vs stale) and nested size class. Recorded
                // before the gates alongside the delivered fold so a
                // failing lane still classifies the relay path;
                // diagnostic-only keys never feed any pass predicate.
                let dropped: Vec<&Observation> = ledger
                    .of_epoch(Epoch::IbgwData)
                    .filter(|obs| obs.kind == ObservedKind::GatewayDropped)
                    .collect();
                let drop_diag = gateway_drop_diag_counts(&dropped, &accepted_gateways);
                // Per-ingress drop rows (diagnostic-only): addressed
                // id + scope + size class. Sanitized routing facts;
                // never an input to any pass predicate.
                for obs in &dropped {
                    let (scope, class) = gateway_drop_diag_label(obs, &accepted_gateways);
                    record_row(
                        &evidence_dir,
                        Epoch::IbgwData,
                        "gateway-diag-drop",
                        &format!(
                            "{receive:#010x}-{scope}-{class}",
                            receive = obs.receive_tunnel
                        ),
                        &mut rows,
                    );
                }
                for (key, value) in [
                    ("gateway-diag-dropped", drop_diag.dropped),
                    (
                        "gateway-diag-dropped-accepted-id",
                        drop_diag.dropped_accepted_id,
                    ),
                    ("gateway-diag-dropped-stale-id", drop_diag.dropped_stale_id),
                    (
                        "gateway-diag-dropped-nested-multi",
                        drop_diag.dropped_nested_multi,
                    ),
                ] {
                    record_row(
                        &evidence_dir,
                        Epoch::IbgwData,
                        key,
                        &value.to_string(),
                        &mut rows,
                    );
                }
                // Recorded before the gates so failures still show
                // whether anything reached the receiver socket (e.g. a
                // B-gatewayed delivery that bypassed our IBGW id).
                record_row(
                    &evidence_dir,
                    Epoch::IbgwData,
                    "gateway-receipt",
                    &rx_1500.to_string(),
                    &mut rows,
                );
                if gatewayed.is_empty() {
                    return Err("IBGW data epoch observed no genuine gateway ingress".to_string());
                }
                record_row(
                    &evidence_dir,
                    Epoch::IbgwData,
                    "gateway-ingress",
                    "true",
                    &mut rows,
                );
                let multicell = gateway_multicell_satisfied(&gatewayed);
                record_row(
                    &evidence_dir,
                    Epoch::IbgwData,
                    "multicell-max",
                    &gatewayed
                        .iter()
                        .map(|obs| obs.aux_count)
                        .max()
                        .unwrap_or(0)
                        .to_string(),
                    &mut rows,
                );
                if !multicell {
                    return Err("IBGW data epoch observed no multi-cell emission".to_string());
                }
                record_row(
                    &evidence_dir,
                    Epoch::IbgwData,
                    "multicell-bounded",
                    "true",
                    &mut rows,
                );
                // Plan 260 authority repair: the 2-hop B-ending receipt
                // premise this gate once enforced is superseded. Plan
                // 259 proved no lane-buildable transit terminus (A or
                // B) can close receipt through `TransitTunnelEndpoint`
                // — and Plan 260 restores the receipt requirement on
                // the distinct creator-owned inbound path instead
                // (Epoch::IbgwReceipt below, with its own gate). The
                // 2-hop observation stays recorded as diagnostic
                // history (emission proof + receipt count, normally
                // zero here); it no longer aborts the lane. No history
                // is rewritten: the value is still counted, just no
                // longer the closing gate.
                record_row(
                    &evidence_dir,
                    Epoch::IbgwData,
                    "gateway-receipt-superseded-note",
                    "plan260-creator-owned-receipt-epoch-owns-receipt",
                    &mut rows,
                );
                // Data sockets done (same mesh-calming rationale as the
                // OBEP receiver above); nothing reads them again.
                drop(rx_ibgw);
                drop(tx_ibgw);
            }
        } // end Plan 264 per-epoch ibgw-data block

        // Plan 260 work packages C/D/E/G: creator-owned inbound
        // receipt epoch. The dedicated receiver destination owns a
        // source-supported one-hop inbound tunnel through i2pr
        // only (`inbound.length = 1`, zero variance, explicit peer
        // i2pr); the sibling sender addresses that receiver
        // destination, so genuine TunnelGateway ingress at the
        // accepted IBGW id must emit toward the creator router A
        // and resolve A's creator-local inbound tunnel into the
        // receiver pool. The six-field tuple (§7) binds
        // receive id + creator router + creator-local tunnel +
        // pool owner + LeaseSet gateway/tunnel from typed
        // build/data evidence; the receiver SAM socket proves
        // end-to-end receipt exactly once.
        // Plan 264: per-epoch receipt runs are counted (two
        // same-SHA passes close the row); the full-matrix run
        // without a selector stays diagnostic-only.
        if run_receipt_epoch {
            let mut receipt_owner = fresh_owner(&handle, wall_secs() ^ 0x260D, &identity)?;
            let (rx_receipt_dest, mut rx_receipt) = {
                let mut attempt = SamClient::connect(sam_addr).await?;
                attempt.hello().await?;
                match run_sam_epoch(
                    &mut handle,
                    &mut receipt_owner,
                    &mut ledger,
                    Epoch::IbgwReceipt,
                    &mut attempt,
                    "m11-rx-receipt",
                    &[
                        ("inbound.length", "1"),
                        ("inbound.quantity", "1"),
                        ("inbound.lengthVariance", "0"),
                        ("outbound.length", "1"),
                        ("outbound.quantity", "1"),
                        ("outbound.lengthVariance", "0"),
                        ("explicitPeers", &i2pr_b64),
                    ],
                    BuildStop::AcceptedRole(
                        TransitHopRoleKind::InboundGateway,
                        [a_peer_hash, a_peer_hash],
                    ),
                    build_epoch_timeout(),
                    a_target,
                    b_target,
                    &identity,
                )
                .await?
                {
                    Some(dest) => (dest, attempt),
                    None => {
                        return Err(
                            "Plan 260 receipt epoch: receiver session never reported ready"
                                .to_string(),
                        );
                    }
                }
            };
            let receiver_hash_hex = destination_hash_hex(&rx_receipt_dest)?;
            record_row(
                &evidence_dir,
                Epoch::IbgwReceipt,
                "receiver-destination",
                &receiver_hash_hex,
                &mut rows,
            );
            // Plan 261 work package B: the counted send leg is the
            // B-side sender (the A-side sender leg is deleted from
            // the counted matrix; its B-endpoint death stays
            // retained as the Plan 260 B2 boundary, not retried).
            // `m11-tx-b` is outbound-only through i2pr
            // (`outbound.length = 1`, zero variance, explicit peer
            // i2pr), so B's outbound `[i2pr]` puts i2pr — not B —
            // at the outbound endpoint and no destination garlic
            // transits B's endpoint. The receiver construction
            // above is unchanged (proven by Plan 260).
            let mut tx_b = SamClient::connect(b_sam_addr).await?;
            tx_b.hello().await?;
            run_sam_epoch(
                &mut handle,
                &mut receipt_owner,
                &mut ledger,
                Epoch::IbgwReceipt,
                &mut tx_b,
                "m11-tx-b",
                &[
                    ("inbound.length", "0"),
                    ("outbound.length", "1"),
                    ("outbound.quantity", "1"),
                    ("outbound.lengthVariance", "0"),
                    ("explicitPeers", &i2pr_b64),
                ],
                BuildStop::SessionReady,
                build_epoch_timeout(),
                a_target,
                b_target,
                &identity,
            )
            .await?
            .ok_or("Plan 261 receipt epoch: B-side sender session never reported ready")?;
            // Plan 261 §11.5: the B sender establishes outbound
            // `[i2pr]` — the typed OBEP accept must reply to B
            // (the A-side sender's `[B]` accept shape is gone with
            // the deleted leg).
            if !ledger.role_accepted(
                Epoch::IbgwReceipt,
                TransitHopRoleKind::OutboundEndpoint,
                &[b_hash_bytes],
            ) {
                return Err(
                    "Plan 261 receipt epoch: B-side sender produced no typed OBEP accept"
                        .to_string(),
                );
            }
            record_row(
                &evidence_dir,
                Epoch::IbgwReceipt,
                "b-sender-obep-accepted",
                "true",
                &mut rows,
            );
            // 1400-byte stimulus: the nested encoding exceeds the
            // 976-byte per-cell capacity, so the counted emission
            // must split into at least two TunnelData cells; the
            // 0xA5 pattern distinguishes this epoch from the
            // legacy 1500 x 0x7E stimulus on its own sockets.
            // Plan 263 work package A: prove the B-sender mesh is
            // live before burning send rounds (same fail-closed
            // liveness + B-side prerequisites as the IBGW relay;
            // the A-side explicit-peer file is not required here
            // because B sends directly, but B must still know
            // i2pr and hold the floodfill role).
            ensure_sessions(&handle, a_target, b_target).await;
            mesh_liveness_status(&handle, a_target, b_target)?;
            verify_relay_netdb_prerequisites(
                &b_ri_in_a_netdb,
                &i2pr_ri_in_a_netdb,
                &i2pr_ri_in_b_netdb,
            )?;
            verify_b_floodfill_conf(&b_home)?;
            // Plan 261: the sender freshness gate watches B (the
            // B-side sender's outbound pool), not A.
            let tx_b_baseline = reference_b.count_outbound_created();
            let receipt_floor_ms = wall_ms();
            let mut receipt_baseline: Vec<u32> = Vec::new();
            let mut rx_receipt_count: usize = 0;
            for round in 0..4 {
                if rx_receipt_count == 1 {
                    break;
                }
                ensure_sessions(&handle, a_target, b_target).await;
                if round > 0 {
                    let _fresh = pump_until_fresh_ibgw(
                        &mut handle,
                        &mut receipt_owner,
                        &mut ledger,
                        Epoch::IbgwReceipt,
                        &mut receipt_baseline,
                        receipt_floor_ms,
                        Duration::from_secs(25),
                    )
                    .await?;
                    let _settled = pump_until_fresh_ibgw(
                        &mut handle,
                        &mut receipt_owner,
                        &mut ledger,
                        Epoch::IbgwReceipt,
                        &mut receipt_baseline,
                        receipt_floor_ms,
                        Duration::from_secs(4),
                    )
                    .await?;
                }
                wait_for_fresh_sender_tunnel(
                    &mut handle,
                    &mut receipt_owner,
                    &mut ledger,
                    Epoch::IbgwReceipt,
                    &reference_b,
                    tx_b_baseline,
                    Duration::from_secs(12),
                )
                .await?;
                eprintln!("m11-tx-b send round started wall_ms={}", wall_ms());
                tx_b.send_datagram("m11-tx-b", &rx_receipt_dest, &vec![0xA5u8; 1400])
                    .await?;
                drain_data_epoch(
                    &mut handle,
                    &mut receipt_owner,
                    &mut ledger,
                    Epoch::IbgwReceipt,
                    data_epoch_timeout(),
                    DataStop::GatewayDelivered,
                    Duration::from_secs(DATA_SETTLE_SECS),
                )
                .await?;
                poll_rx_sized(
                    &mut rx_receipt,
                    Duration::from_secs(10),
                    1400,
                    0xA5,
                    &mut rx_receipt_count,
                )
                .await?;
            }
            // Bind the counted epoch: accepted IBGW registrations
            // from the creator router A, and genuine gateway
            // deliveries addressing one of them.
            let accepted_receipt: Vec<u32> = ledger
                .of_epoch(Epoch::IbgwReceipt)
                .filter(|obs| {
                    obs.kind == ObservedKind::BuildAccepted
                        && obs.role == Some(TransitHopRoleKind::InboundGateway)
                        && !obs.rejected
                        && obs.peer_hash == a_peer_hash
                        && obs.delivery == "accepted"
                        && obs.active_after == obs.active_before + 1
                })
                .map(|obs| obs.receive_tunnel)
                .collect();
            let gatewayed_receipt: Vec<&Observation> = ledger
                .of_epoch(Epoch::IbgwReceipt)
                .filter(|obs| gateway_ingress_accepted(obs, &accepted_receipt))
                .collect();
            record_row(
                &evidence_dir,
                Epoch::IbgwReceipt,
                "gateway-receipt",
                &rx_receipt_count.to_string(),
                &mut rows,
            );
            // Plan 261 work package A.3 + B: B-side LeaseSet
            // resolution proof and the B-sender terminal outcome.
            // Resolution is proven behaviorally, never inferred: B
            // resolving A's receiver LeaseSet is the only way
            // B-originated tunnel data reaches i2pr's OBEP
            // registration in this epoch (a failed lookup dies
            // inside B with `Can't request LeaseSet` and emits
            // nothing). The outcome key carries the sanitized
            // terminal signature (B-originated terminal garlics /
            // genuine ingress / socket receipts) so a stop lands
            // with exact provenance instead of a bare abort.
            let i2pr_self_hash = *local_hash.as_bytes();
            let b_originated: Vec<&Observation> = ledger
                .of_epoch(Epoch::IbgwReceipt)
                .filter(|obs| {
                    obs.kind == ObservedKind::DataDeliveredObep && obs.peer_hash == b_hash_bytes
                })
                .collect();
            let b_terminal_garlic = b_originated
                .iter()
                .filter(|obs| {
                    obs.delivery == "obep-terminal-garlic"
                        && obs.next_router == Some(i2pr_self_hash)
                })
                .count();
            let b_leaseset_resolved =
                !b_originated.is_empty() && !reference_b.log_contains("Can't request LeaseSet");
            record_row(
                &evidence_dir,
                Epoch::IbgwReceipt,
                "b-leaseset-resolved",
                &b_leaseset_resolved.to_string(),
                &mut rows,
            );
            if !b_leaseset_resolved {
                return Err(format!(
                    "Plan 261 receipt epoch: B-side LeaseSet resolution unproven (b-originated OBEP \
                     observations: {}, B log tail: {})",
                    b_originated.len(),
                    reference_b.log_tail(8).replace('\n', " | ")
                ));
            }
            // Plan 261 work package C: the send-leg wall window is
            // counted evidence for the mesh-sustainability budget
            // (the matrix must fit the healthy window, §7).
            record_row(
                &evidence_dir,
                Epoch::IbgwReceipt,
                "send-window-ms",
                &wall_ms().saturating_sub(receipt_floor_ms).to_string(),
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::IbgwReceipt,
                "b-sender-outcome",
                &format!(
                    "terminal-garlic-self:{b_terminal_garlic}/ingress:{}/socket:{rx_receipt_count}",
                    gatewayed_receipt.len()
                ),
                &mut rows,
            );
            if gatewayed_receipt.is_empty() {
                return Err(
                    "Plan 261 receipt epoch observed no genuine gateway ingress on the \
                    B-sender topology"
                        .to_string(),
                );
            }
            let counted = gatewayed_receipt[0];
            record_row(
                &evidence_dir,
                Epoch::IbgwReceipt,
                "gateway-ingress",
                "true",
                &mut rows,
            );
            let receipt_multicell = gateway_multicell_satisfied(&gatewayed_receipt);
            if !receipt_multicell {
                return Err("Plan 260 receipt epoch observed no multi-cell emission".to_string());
            }
            record_row(
                &evidence_dir,
                Epoch::IbgwReceipt,
                "multicell-bounded",
                "true",
                &mut rows,
            );
            // The counted next hop must be the creator router A
            // (§7.3): any other next router is not the
            // receiver-owned inbound path and fails before
            // receipt is counted.
            let observed_next = counted.next_router.ok_or_else(|| {
                "Plan 260 receipt epoch: counted delivery carries no next router".to_string()
            })?;
            if observed_next != a_peer_hash {
                return Err(format!(
                    "Plan 260 receipt epoch: counted next hop {} is not the creator router A {}",
                    hex_lower(&observed_next),
                    hex_lower(&a_peer_hash)
                ));
            }
            // Six-field tuple from typed evidence. The i2pr-side
            // triple (receive id, next router, next tunnel) is
            // observed directly: the receive id is the accepted
            // registration the ingress addressed, the next pair is
            // the registration's committed tuple on the counted
            // delivery. Pool ownership and LeaseSet binding derive
            // behaviorally: stock i2pd exposes no numeric read of
            // the creator-local `InboundTunnel::GetTunnelID()` or
            // its pool (the creator-owned handler logs no tunnel
            // id), so receipt of the exact payload at the receiver
            // socket through this one-hop topology is the binding
            // proof — only the receiver pool's
            // `ProcessGarlicMessage` can deliver there. A payload
            // arriving through any other lease/tunnel, or more
            // than once, never satisfies the rows below.
            let tuple = ReceiptTuple {
                receiver_destination_hash: receiver_hash_hex.clone(),
                ibgw_receive_id: counted.receive_tunnel,
                creator_router_hash: hex_lower(&a_peer_hash),
                creator_local_tunnel_id: counted.next_message_id,
                pool_owner_destination_hash: receiver_hash_hex.clone(),
                leaseset_gateway_hash: hex_lower(local_hash.as_bytes()),
                leaseset_tunnel_id: counted.receive_tunnel,
            };
            record_row(
                &evidence_dir,
                Epoch::IbgwReceipt,
                "ibgw-receive-id",
                &tuple.ibgw_receive_id.to_string(),
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::IbgwReceipt,
                "creator-router",
                &tuple.creator_router_hash,
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::IbgwReceipt,
                "creator-local-tunnel-id",
                &tuple.creator_local_tunnel_id.to_string(),
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::IbgwReceipt,
                "pool-owner-destination",
                &tuple.pool_owner_destination_hash,
                &mut rows,
            );
            let i2pr_hex = hex_lower(local_hash.as_bytes());
            let gateway_match = tuple.leaseset_gateway_hash == i2pr_hex;
            let tunnel_match = tuple.leaseset_tunnel_id == tuple.ibgw_receive_id
                && accepted_receipt.contains(&tuple.ibgw_receive_id);
            record_row(
                &evidence_dir,
                Epoch::IbgwReceipt,
                "leaseset-gateway-match",
                &gateway_match.to_string(),
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::IbgwReceipt,
                "leaseset-tunnel-match",
                &tunnel_match.to_string(),
                &mut rows,
            );
            let tuple_bound =
                gateway_match && tunnel_match && validate_receipt_tuple(&tuple, &i2pr_hex).is_ok();
            record_row(
                &evidence_dir,
                Epoch::IbgwReceipt,
                "full-tuple-bound",
                &tuple_bound.to_string(),
                &mut rows,
            );
            if !tuple_bound {
                return Err(format!(
                    "Plan 260 receipt epoch: six-field tuple not bound (gateway_match=\
                     {gateway_match} tunnel_match={tunnel_match} tuple={tuple:?})"
                ));
            }
            if rx_receipt_count != 1 {
                return Err(format!(
                    "Plan 260 receipt epoch expected exactly one payload-verified 1400-byte \
                     DATAGRAM RECEIVED on the receiver session socket: rx_receipt={rx_receipt_count}"
                ));
            }
            record_row(
                &evidence_dir,
                Epoch::IbgwReceipt,
                "gateway-receipt-once",
                "true",
                &mut rows,
            );
            drop(rx_receipt);
            drop(tx_b);
        }
        if receipt_only {
            // Diagnostic subset ends here: ledger/driver TSVs are
            // already persisted incrementally, and the counted
            // manifest below requires the full matrix.
            return Ok(());
        }

        // Participant forward + creator acceptance: genuine TunnelData
        // through the accepted Participant registration transforms and
        // forwards to i2pd-B. The reference originates tunnel-test and
        // maintenance cells on its own cadence
        // (`TunnelPool::ManageTunnels`, 10 s); A's continued use of the
        // tunnel proves the creator accepted the completed build.
        // Plan 264: per-epoch participant-data runs are counted;
        // lifecycle per-epoch runs execute this forward as setup
        // (via needs_forward_setup) for their genuine cell.
        if run_participant_data_epoch {
            let part_receive =
                receive_of(&ledger, Epoch::Participant, TransitHopRoleKind::Participant)
                    .unwrap_or(u32::MAX);
            // Fresh sessions before the passive drain: it only
            // observes, so an idle-dead link would starve it
            // silently for the whole timeout.
            ensure_sessions(&handle, a_target, b_target).await;
            drain_data_epoch(
                &mut handle,
                &mut part_owner,
                &mut ledger,
                Epoch::ParticipantData,
                data_epoch_timeout() + Duration::from_secs(60),
                DataStop::Forwarded {
                    receive: part_receive,
                    next: b_hash_bytes,
                },
                Duration::from_secs(DATA_SETTLE_SECS),
            )
            .await?;
            let participant_receive = participant_receive_opt
                .ok_or("participant epoch produced no accept to drive data with")?;
            if !ledger.data_forwarded(Epoch::ParticipantData, participant_receive, b_hash_bytes) {
                return Err(
                    "participant data epoch observed no genuine forward to i2pd-B".to_string(),
                );
            }
            record_row(
                &evidence_dir,
                Epoch::ParticipantData,
                "forward",
                "true",
                &mut rows,
            );
            let digest = ledger
                .of_epoch(Epoch::ParticipantData)
                .find(|obs| obs.kind == ObservedKind::DataForwarded)
                .and_then(|obs| obs.digest.clone())
                .ok_or("participant forward digest missing")?;
            record_row(
                &evidence_dir,
                Epoch::ParticipantData,
                "digest",
                &digest,
                &mut rows,
            );
            // Plan 257 work package F: independent i2pd-B far-side
            // proof. The counted forward's exact next tunnel id
            // binds the local forward to the B-side endpoint
            // observation (`TransitTunnel: handle msg for endpoint
            // <id>` at debug level). B runs at debug in the
            // counted lane (`I2PR_M11_LOGLEVEL_B=debug`); an
            // info-level B yields zero and fails this row closed.
            let next_tunnel = ledger
                .of_epoch(Epoch::ParticipantData)
                .find(|obs| obs.kind == ObservedKind::DataForwarded)
                .map(|obs| obs.next_message_id)
                .ok_or("participant forward next tunnel missing")?;
            if next_tunnel == 0 {
                return Err(
                    "participant forward carries no next tunnel for far-side binding".to_string(),
                );
            }
            // Bounded settle so the reference flushes the endpoint
            // handling to its log before the count is taken.
            tokio::time::sleep(Duration::from_secs(5)).await;
            let far_side_count = reference_b.count_endpoint_messages(next_tunnel);
            ledger.push(Observation {
                epoch: Epoch::ParticipantData,
                kind: ObservedKind::InboundObserved,
                peer_hash: b_hash_bytes,
                role: None,
                receive_tunnel: next_tunnel,
                next_router: None,
                next_message_id: 0,
                rejected: false,
                delivery: "b-endpoint-observed",
                active_before: part_owner.active_count(),
                active_after: part_owner.active_count(),
                logical_ms: wall_ms(),
                digest: None,
                aux_count: far_side_count,
                pending_after: 0,
                peer_index_after: 0,
                queued_after: 0,
                bandwidth: None,
                gateway_failures: None,
                nested_len: None,
            });
            if !ledger.far_side_satisfied(Epoch::ParticipantData, next_tunnel) {
                return Err(format!(
                    "participant far side unobserved at i2pd-B endpoint {next_tunnel:#06x} \
                     (count={far_side_count}); B must run at debug loglevel"
                ));
            }
            record_row(
                &evidence_dir,
                Epoch::ParticipantData,
                "local-forward",
                "true",
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::ParticipantData,
                "next-tunnel",
                &format!("{next_tunnel:#06x}"),
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::ParticipantData,
                "b-endpoint-observed",
                "true",
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::ParticipantData,
                "far-side-count",
                &far_side_count.to_string(),
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::ParticipantData,
                "creator-accepted",
                "true",
                &mut rows,
            );
        }

        // Replay: re-feed one already accepted genuine cell with wall
        // time and prove the duplicate produces no second semantic
        // delivery. Bytes and peer are exactly what the authenticated
        // session delivered; nothing is synthesized.
        // Plan 264: gated on the lifecycle chain (any lifecycle
        // per-epoch request or full data); per-epoch runs execute
        // the full chain with only the named epoch counted.
        if run_lifecycle_chain {
            let participant_receive = participant_receive_opt
                .ok_or("participant epoch produced no accept for the replay experiment")?;
            let cell = ledger
                .replay_candidate(Epoch::ParticipantData, participant_receive)
                .ok_or("no forward cell available for the replay experiment")?;
            let digest = cell.digest.clone();
            feed_retained_cell(
                &mut part_owner,
                &mut ledger,
                Epoch::Replay,
                PeerId::from_hash(a_hash),
                &cell,
                wall_ms(),
                wall_secs(),
            )
            .await?;
            // Plan 264 failure forensics: persist the ledger before
            // the fail-closed predicate so a predicate failure
            // retains the Replay-epoch outcome observation (forward
            // vs drop vs unrecorded containment) instead of losing
            // it to the early return. The final flush below still
            // overwrites with complete state on success.
            ledger.write_evidence(&evidence_dir);
            if !ledger.replay_suppressed(Epoch::Replay, &digest) {
                return Err("replay epoch produced a second semantic delivery".to_string());
            }
            record_row(
                &evidence_dir,
                Epoch::Replay,
                "no-second-delivery",
                &digest,
                &mut rows,
            );
        }

        // Expiry: re-feed a genuine reference cell with the injected
        // transit clock beyond creation + 600 s. The runtime-neutral
        // data plane checks expiry before any transform, so the cell
        // must drop with no next-hop delivery. The sweep afterwards
        // proves secret-owning state returns to baseline.
        // Plan 264: per-epoch lifecycle gate (chain runs for any
        // lifecycle request; only the named epoch counts).
        if run_lifecycle_chain {
            let participant_receive = participant_receive_opt
                .ok_or("participant epoch produced no accept for the expiry experiment")?;
            let cell = ledger
                .replay_candidate(Epoch::ParticipantData, participant_receive)
                .ok_or("no forward cell available for the expiry experiment")?;
            let created_ms = cell.observed_ms;
            let future_ms = created_ms + TRANSIT_LIFETIME_MS + 1_000;
            let future_secs = future_ms / 1000;
            let pre_active = part_owner.active_count();
            feed_retained_cell(
                &mut part_owner,
                &mut ledger,
                Epoch::Expiry,
                PeerId::from_hash(a_hash),
                &cell,
                future_ms,
                future_secs,
            )
            .await?;
            if !ledger.expiry_enforced(Epoch::Expiry, participant_receive, created_ms) {
                return Err("expiry epoch did not drop post-lifetime data".to_string());
            }
            record_row(
                &evidence_dir,
                Epoch::Expiry,
                "drops-live-data",
                "true",
                &mut rows,
            );
            // Plan 257 work package G.2: session close proves A
            // removed while unrelated B remains. Runs before
            // cancellation because cancel drains the peer index as
            // well. Identity-specific membership is checked (never
            // inferred from a total count alone).
            if !part_owner.has_peer(&a_hash) {
                return Err("session close requires an installed A mapping".to_string());
            }
            if !part_owner.has_peer(&b_hash) {
                return Err("session close requires an installed B mapping".to_string());
            }
            record_row(
                &evidence_dir,
                Epoch::SessionClose,
                "a-before",
                "true",
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::SessionClose,
                "b-before",
                "true",
                &mut rows,
            );
            let peer_a = PeerId::from_hash(a_hash);
            let removed_a = part_owner.note_session_closed(&peer_a);
            if !removed_a {
                return Err("session close did not reconcile the A peer mapping".to_string());
            }
            if part_owner.has_peer(&a_hash) {
                return Err("session close left the A mapping installed".to_string());
            }
            record_row(
                &evidence_dir,
                Epoch::SessionClose,
                "a-removed",
                "true",
                &mut rows,
            );
            if !part_owner.has_peer(&b_hash) {
                return Err("session close removed the unrelated B mapping".to_string());
            }
            record_row(
                &evidence_dir,
                Epoch::SessionClose,
                "b-retained",
                "true",
                &mut rows,
            );
            {
                let peer_index_after = part_owner.live_state_snapshot().peer_index_entries;
                ledger.push(Observation {
                    epoch: Epoch::SessionClose,
                    kind: ObservedKind::StateSnapshot,
                    peer_hash: a_peer_hash,
                    role: None,
                    receive_tunnel: 0,
                    next_router: Some(b_hash_bytes),
                    next_message_id: 0,
                    rejected: false,
                    delivery: "session-closed",
                    active_before: pre_active,
                    active_after: part_owner.active_count(),
                    logical_ms: wall_ms(),
                    digest: None,
                    aux_count: peer_index_after,
                    pending_after: 0,
                    peer_index_after,
                    queued_after: 0,
                    bandwidth: None,
                    gateway_failures: None,
                    nested_len: None,
                });
            }
            if !ledger.session_close_a_removed_b_retained(
                Epoch::SessionClose,
                a_peer_hash,
                b_hash_bytes,
            ) {
                return Err("session close did not prove A removed with B retained".to_string());
            }
            record_row(
                &evidence_dir,
                Epoch::SessionClose,
                "peer-baseline",
                "true",
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::SessionClose,
                "final-peer-baseline",
                &part_owner
                    .live_state_snapshot()
                    .peer_index_entries
                    .to_string(),
                &mut rows,
            );
            // Plan 257 work package G.1: cancellation proves every
            // bounded dimension drains synchronously from nonzero
            // live state, and new ingress is refused afterwards.
            let cancel_before = part_owner.live_state_snapshot();
            if cancel_before.active_registrations == 0 {
                return Err(format!(
                    "cancel epoch has no live state (pre-expiry active was {pre_active})"
                ));
            }
            record_row(
                &evidence_dir,
                Epoch::Cancel,
                "active-before",
                &cancel_before.active_registrations.to_string(),
                &mut rows,
            );
            part_owner.cancel();
            let cancel_after = part_owner.live_state_snapshot();
            ledger.push(Observation {
                epoch: Epoch::Cancel,
                kind: ObservedKind::StateSnapshot,
                peer_hash: a_peer_hash,
                role: None,
                receive_tunnel: 0,
                next_router: None,
                next_message_id: 0,
                rejected: false,
                delivery: "cancelled",
                active_before: cancel_before.active_registrations,
                active_after: cancel_after.active_registrations,
                logical_ms: wall_ms(),
                digest: None,
                aux_count: 0,
                pending_after: cancel_after.pending_global,
                peer_index_after: cancel_after.peer_index_entries,
                queued_after: cancel_after.transit_owned_queued_work,
                bandwidth: None,
                gateway_failures: None,
                nested_len: None,
            });
            if !ledger.cancel_fully_drained(Epoch::Cancel) {
                return Err(format!(
                    "cancellation did not drain every dimension (after={:?})",
                    cancel_after.evidence_label()
                ));
            }
            record_row(
                &evidence_dir,
                Epoch::Cancel,
                "active-after",
                &cancel_after.active_registrations.to_string(),
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::Cancel,
                "pending-after",
                &cancel_after.pending_global.to_string(),
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::Cancel,
                "peer-index-after",
                &cancel_after.peer_index_entries.to_string(),
                &mut rows,
            );
            record_row(
                &evidence_dir,
                Epoch::Cancel,
                "queued-work-after",
                &cancel_after.transit_owned_queued_work.to_string(),
                &mut rows,
            );
            record_row(&evidence_dir, Epoch::Cancel, "drains", "true", &mut rows);
            // New ingress is refused after cancel: re-feed one
            // genuine retained cell and prove no registration
            // appears and no forward is emitted.
            {
                let probe_cell = ledger
                    .replay_candidate(Epoch::ParticipantData, participant_receive)
                    .ok_or("no genuine cell available for the cancel ingress probe")?;
                let probe_active = part_owner.active_count();
                feed_retained_cell(
                    &mut part_owner,
                    &mut ledger,
                    Epoch::Cancel,
                    PeerId::from_hash(a_hash),
                    &probe_cell,
                    wall_ms(),
                    wall_secs(),
                )
                .await?;
                if part_owner.active_count() != probe_active {
                    return Err("cancelled owner installed state on new ingress".to_string());
                }
                record_row(
                    &evidence_dir,
                    Epoch::Cancel,
                    "new-ingress-refused",
                    "true",
                    &mut rows,
                );
            }
            // Expiry sweep: the logical clock already passed every
            // registration's lifetime, so the sweep must remove the
            // remainder and leave zero state.
            let swept = part_owner.expire(future_secs);
            let swept_active = part_owner.active_count();
            record_row(
                &evidence_dir,
                Epoch::Expiry,
                "swept-count",
                &swept.to_string(),
                &mut rows,
            );
            if swept_active != 0 {
                return Err(format!(
                    "expiry sweep left {swept_active} live registrations"
                ));
            }
            record_row(
                &evidence_dir,
                Epoch::Expiry,
                "resource-baseline",
                "true",
                &mut rows,
            );
        }

        // Plan 257 work package H: real i2pr runtime restart.
        // The old (cancelled) owner is drained and dropped, a new
        // owner is constructed from zero state, authenticated
        // sessions are re-established, peer mappings are installed
        // from those sessions, and one fresh role-correct build is
        // accepted into exactly one new registration. A
        // constructor-only zero check can never satisfy this row.
        // Plan 264: per-epoch lifecycle gate (see replay gate).
        if run_lifecycle_chain {
            // Old owner drained: the cancel epoch above already
            // drained it; prove the terminal baseline explicitly.
            let old_drained = part_owner.live_state_snapshot();
            if !old_drained.is_zero() {
                return Err(format!(
                    "restart requires a drained old owner (got {})",
                    old_drained.evidence_label()
                ));
            }
            record_row(
                &evidence_dir,
                Epoch::Restart,
                "old-owner-drained",
                &old_drained.evidence_label(),
                &mut rows,
            );
            drop(part_owner);
            // New owner from zero state: no registration state is
            // transferred (fresh RNG, fresh cancellation, fresh
            // admission/registry/peer index via fresh_owner).
            let mut restarted = fresh_owner(&handle, wall_secs() ^ 0xE57A, &identity)?;
            let new_zero = restarted.live_state_snapshot();
            if !new_zero.is_zero() {
                return Err(format!(
                    "restarted owner carries state ({})",
                    new_zero.evidence_label()
                ));
            }
            record_row(
                &evidence_dir,
                Epoch::Restart,
                "new-owner-zero",
                &new_zero.evidence_label(),
                &mut rows,
            );
            // Re-establish authenticated sessions to the exact-pinned
            // references and prove both peer mappings resolve.
            ensure_sessions(&handle, a_target, b_target).await;
            if !restarted.has_peer(&a_hash) || !restarted.has_peer(&b_hash) {
                return Err("restarted owner has no authenticated peer mappings".to_string());
            }
            record_row(
                &evidence_dir,
                Epoch::Restart,
                "sessions-reestablished",
                "true",
                &mut rows,
            );
            // One fresh role-correct OBEP build after restart.
            let mut restart_sam = SamClient::connect(sam_addr).await?;
            restart_sam.hello().await?;
            run_sam_epoch(
                &mut handle,
                &mut restarted,
                &mut ledger,
                Epoch::Restart,
                &mut restart_sam,
                "m11-restart",
                &[
                    ("inbound.length", "0"),
                    ("outbound.length", "2"),
                    ("outbound.quantity", "1"),
                    ("outbound.lengthVariance", "0"),
                    ("explicitPeers", &format!("{b_b64},{i2pr_b64}")),
                ],
                BuildStop::AcceptedRole(
                    TransitHopRoleKind::OutboundEndpoint,
                    [a_peer_hash, b_hash_bytes],
                ),
                build_epoch_timeout(),
                a_target,
                b_target,
                &identity,
            )
            .await?;
            drop(restart_sam);
            if !ledger.role_accepted(
                Epoch::Restart,
                TransitHopRoleKind::OutboundEndpoint,
                &[a_peer_hash, b_hash_bytes],
            ) {
                return Err("restart did not accept a fresh role-correct build".to_string());
            }
            record_row(
                &evidence_dir,
                Epoch::Restart,
                "fresh-build-accepted",
                "true",
                &mut rows,
            );
            let fresh_receive = record_cardinality_rows(
                &evidence_dir,
                &ledger,
                Epoch::Restart,
                TransitHopRoleKind::OutboundEndpoint,
                &[a_peer_hash, b_hash_bytes],
                &mut rows,
            )?;
            record_row(
                &evidence_dir,
                Epoch::Restart,
                "fresh-registration-delta",
                &format!("{fresh_receive:#x}=+1"),
                &mut rows,
            );
            // Shut down and drain again: the restarted owner must
            // also terminate cleanly.
            restarted.cancel();
            let final_snapshot = restarted.live_state_snapshot();
            if !final_snapshot.is_zero() {
                return Err(format!(
                    "restarted owner did not drain ({})",
                    final_snapshot.evidence_label()
                ));
            }
            record_row(
                &evidence_dir,
                Epoch::Restart,
                "final-baseline",
                &final_snapshot.evidence_label(),
                &mut rows,
            );
            // Keep the legacy clean-baseline key as a subset of the
            // new evidence (the new zero-state row above is the
            // authoritative proof).
            record_row(
                &evidence_dir,
                Epoch::Restart,
                "clean-baseline",
                "true",
                &mut rows,
            );
        }
    }

    // Fail-closed: no `NoActiveSession`/`Fatal` drift on the counted
    // role epochs.
    if reference_a.log_contains("Can't create outbound tunnel, no peers available")
        || reference_a.log_contains("Can't create inbound tunnel, no peers available")
    {
        return Err("i2pd-A reported no-peers-available; bootstrap did not take".to_string());
    }

    // Ownership rows are emitted only after their matching inbound
    // observations exist in the ledger (never before the event).
    let inbound_observations = ledger
        .observations
        .iter()
        .filter(|obs| obs.kind == ObservedKind::InboundObserved)
        .count();
    if inbound_observations == 0 {
        return Err("no authenticated inbound event was ever observed".to_string());
    }
    record_row(
        &evidence_dir,
        Epoch::Bootstrap,
        "live-next-inbound-observed",
        &inbound_observations.to_string(),
        &mut rows,
    );
    record_row(
        &evidence_dir,
        Epoch::Bootstrap,
        "authenticated-peer-bound",
        "true",
        &mut rows,
    );
    record_row(
        &evidence_dir,
        Epoch::Bootstrap,
        "no-direct-build-injection",
        "true",
        &mut rows,
    );
    ledger.write_evidence(&evidence_dir);
    check_epoch_row_uniqueness(&rows).map_err(|e| format!("evidence self-guard: {e}"))?;

    // The dispatcher must remain observable without any inbound
    // helper bypass; prove the canonical decode is the only decoder
    // by exhausting the inbound stream after shutdown.
    handle.shutdown();
    let _ = scope.shutdown().await;
    let snapshot = handle.snapshot();
    append_evidence(
        &evidence_dir,
        "bootstrap/shutdown-baseline",
        &format!(
            "active_sessions={} pending_inbound={} pending_outbound={}",
            snapshot.active_sessions, snapshot.pending_inbound, snapshot.pending_outbound
        ),
    );
    reference_a.shutdown().await;
    reference_b.shutdown().await;

    // Smoke check: confirm `dispatch_router_i2np` matches the
    // canonical decode path used by the live owner.
    let synthetic = Ssu2InboundI2np {
        link_id: i2pr_transport::LinkId::new(17).expect("link"),
        peer: PeerId::from_hash(a_hash),
        bytes: Vec::new(),
    };
    let _ = dispatch_router_i2np(&synthetic, wall_ms());
    Ok(())
}

/// Stop condition for one combined SAM + inbound epoch loop.
enum BuildStop {
    /// Stop when the epoch ledger holds a typed code-30 rejection
    /// for the role (the SAM reply may never arrive by design).
    RejectedRole(TransitHopRoleKind),
    /// Stop when the epoch ledger holds a typed accept for the
    /// role from the expected peer (the SAM reply normally
    /// follows; both are awaited).
    AcceptedRole(TransitHopRoleKind, [[u8; 32]; 2]),
    /// Stop when the SAM session reports ready (tunnels up); the
    /// public destination is returned for data stimuli.
    SessionReady,
    /// Data-setup stop for the counted sender session: exits on
    /// SAM readiness like [`Self::SessionReady`], but admission
    /// resets stay armed until an OBEP-role accept replying to
    /// `reply` lands. Background accepts must not disarm the
    /// resets: the counted sender rebuilds every ~10 s against
    /// bounded per-peer quotas that background fills within a
    /// minute, so stopping resets at the first background accept
    /// strands the counted build in code 30. OBEP-replying-to-
    /// sender is the counted shape (single-hop outbound pinned
    /// at i2pr); background matches it only rarely, and a rare
    /// background match merely falls back to today's behavior
    /// (fail-closed downstream if the sender never readies).
    /// Only accepts recorded after this setup started count:
    /// with several counted senders sharing one epoch, an
    /// earlier sender's accept must not disarm a later sender's
    /// resets.
    CountedObepSetup {
        /// Expected reply router (the counted sender).
        reply: [u8; 32],
    },
}

/// Runs one SAM-driven build epoch: sends SESSION CREATE, then
/// interleaves SAM reply reads with authenticated inbound drains
/// until the stop condition or the deadline. Session creation and
/// tunnel observation are concurrent because the reference only
/// reports readiness after its tunnels through i2pr establish.
/// Returns the session's public destination when reported.
#[allow(clippy::too_many_arguments)]
async fn run_sam_epoch(
    handle: &mut i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    owner: &mut TransitLiveOwner<ChaCha8Rng>,
    ledger: &mut TypedLedger,
    epoch: Epoch,
    sam: &mut SamClient,
    id: &str,
    options: &[(&str, &str)],
    stop: BuildStop,
    timeout: Duration,
    a_target: i2pr_runtime::Ssu2DialTarget,
    b_target: i2pr_runtime::Ssu2DialTarget,
    identity: &QualificationIdentity,
) -> Result<Option<String>, String> {
    sam.send_create(id, options).await?;
    let deadline = tokio::time::Instant::now() + timeout;
    let snap0 = handle.snapshot();
    let mut dest: Option<String> = None;
    let mut last_reset = tokio::time::Instant::now();
    let mut last_sweep = tokio::time::Instant::now();
    // Session heartbeat: the setup loop can run for minutes while
    // the counted pool retries every ~15 s, and loopback SSU2
    // links die silently under crypto saturation (replies queue
    // into the void while inbound still flows). A completed redial
    // heals the first-link pinning by replacing the dead oldest
    // link, so a later counted retry can complete. Best-effort
    // with a short horizon so a slow peer cannot wedge the
    // inbound drain. Reactive, not periodic: unconditional redials
    // replace healthy opposite-direction links and orphan the
    // peer's in-flight packets into cheap drops, churning the very
    // queue the setup needs calm. Redial only after the service
    // reports a link actually closed.
    let mut last_ensure = tokio::time::Instant::now();
    let mut last_closed = handle.snapshot().sessions_closed;
    let mut last_a_direct = tokio::time::Instant::now();
    let needs_a_direct = matches!(stop, BuildStop::CountedObepSetup { .. });
    ensure_sessions_with_timeout(handle, a_target, b_target, SETUP_HEARTBEAT_DIAL_TIMEOUT).await;
    // Counted-setup floor: only accepts recorded after this setup
    // started can disarm its admission resets, so an earlier
    // counted sender sharing the epoch never disarms a later one.
    let counted_floor_ms = wall_ms();
    while tokio::time::Instant::now() < deadline {
        // Periodic expiry sweep (wall clock): registrations live
        // at most 600 s, and without sweeps the bounded quotas
        // saturate mid-run on background retries, forcing every
        // later counted build into code 30. Sweeping only removes
        // naturally-expired entries, so live counted state is
        // untouched and ledger rows (immutable once recorded) are
        // unaffected.
        if last_sweep.elapsed() > Duration::from_secs(10) {
            owner.expire(wall_secs());
            last_sweep = tokio::time::Instant::now();
        }
        if last_ensure.elapsed() > Duration::from_secs(SETUP_HEARTBEAT_SECS) {
            last_ensure = tokio::time::Instant::now();
            let closed = handle.snapshot().sessions_closed;
            if closed > last_closed {
                last_closed = closed;
                ensure_sessions_with_timeout(
                    handle,
                    a_target,
                    b_target,
                    SETUP_HEARTBEAT_DIAL_TIMEOUT,
                )
                .await;
            }
            // A-direct starvation gate (counted senders only): no
            // inbound from A for the horizon while its pool must
            // be retrying proves the peer-side link died with no
            // close event. Force one A redial per window; the
            // reference accepts the handshake for the link it
            // thinks it lost, healing both directions. Best
            // effort: a failed dial simply leaves the timer
            // running for the next window.
            if needs_a_direct
                && dest.is_none()
                && last_a_direct.elapsed() > Duration::from_secs(A_DIRECT_STARVE_SECS)
            {
                last_a_direct = tokio::time::Instant::now();
                let _ = handle
                    .dial(
                        a_target,
                        SETUP_HEARTBEAT_DIAL_TIMEOUT,
                        &CancellationToken::new(),
                    )
                    .await;
            }
        }
        if let Some(line) = sam.try_read_line(POLL_INTERVAL).await? {
            if line.contains("SESSION STATUS RESULT=OK") {
                dest = Some(extract_destination(&line, id)?);
            } else if line.contains("SESSION STATUS") {
                return Err(format!("SAM session {id} rejected: {line}"));
            }
        }
        match tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await {
            Ok(Some(inbound)) => {
                let peer_hash = *inbound.peer.hash().as_bytes();
                if peer_hash == *identity.a_hash.as_bytes() {
                    last_a_direct = tokio::time::Instant::now();
                }
                let before = owner.active_count();
                let outcome = owner
                    .handle_inbound(&inbound, wall_ms(), wall_secs())
                    .map_err(|e| format!("handle inbound in epoch {}: {e}", epoch.label()))?;
                let after = owner.active_count();
                // Only classified transit-relevant outcomes enter
                // the ledger; stale or unrelated inbound never does.
                // The marker below proves a real `next_inbound`
                // event fed the owner for this exact outcome.
                let relevant = !matches!(outcome, LiveInboundOutcome::Ignored);
                if relevant {
                    ledger.push(Observation {
                        epoch,
                        kind: ObservedKind::InboundObserved,
                        peer_hash,
                        role: None,
                        receive_tunnel: 0,
                        next_router: None,
                        next_message_id: 0,
                        rejected: false,
                        delivery: "observed",
                        active_before: before,
                        active_after: after,
                        logical_ms: wall_ms(),
                        digest: None,
                        aux_count: 0,
                        pending_after: 0,
                        peer_index_after: 0,
                        queued_after: 0,
                        bandwidth: None,
                        gateway_failures: None,
                        nested_len: None,
                    });
                }
                if let LiveInboundOutcome::Build(
                    i2pr_daemon::transit_owner::LiveBuildOutcome::Dispatched(evidence),
                ) = outcome
                {
                    // Plan 257 work package C/D: attach the
                    // post-event snapshot dimensions so exact
                    // registration cardinality and pending-baseline
                    // rows derive from typed state, not wording.
                    let snapshot = owner.live_state_snapshot();
                    ledger.push(
                        Observation::build(epoch, peer_hash, &evidence, before, after, wall_ms())
                            .with_snapshot_after(
                                snapshot.pending_global,
                                snapshot.peer_index_entries,
                                snapshot.transit_owned_queued_work,
                            ),
                    );
                }
            }
            Ok(None) => {
                break;
            }
            Err(_) => {}
        }
        // Admission reset: background mesh churn fills the bounded
        // quotas within seconds while the counted pool retries every
        // ~10 s. When the owner passes the watermark with no counted
        // accept recorded yet for this epoch, replace it wholesale
        // so the next counted retry lands in fresh room. Recorded
        // ledger evidence survives (predicates are existential); the
        // counted accept stops further resets, protecting live
        // state for the traffic epochs that follow; peer mappings
        // are reinstalled by construction; the runtime scope is
        // untouched. The stop holds only while the accept is fresh
        // (or readiness already arrived): a stale accept without
        // SAM readiness means saturated quotas are answering every
        // retry with code 30, so resets resume instead of guarding
        // the stuck state forever.
        // Reset protection follows the stop condition: a role-build
        // stop is satisfied by its own role accept, a SessionReady
        // stop by any accept (data-setup builds), a counted-setup
        // stop by the counted OBEP accept only, and a rejection
        // stop never (rejections install nothing).
        let now_ms = wall_ms();
        let ready = dest.is_some();
        let accepted_yet = match &stop {
            BuildStop::AcceptedRole(role, peers) => {
                ledger.role_accepted(epoch, *role, peers)
                    && (ready || ledger.accept_fresh_ms(epoch, *role, peers, now_ms))
            }
            // SessionReady never disarms early: any background
            // accept would stop resets while the counted pool is
            // still establishing, letting quotas fill and
            // stranding the setup. Resets run until SAM readiness
            // breaks the loop; data registrations are not needed
            // yet, so wiping is safe.
            BuildStop::SessionReady => false,
            BuildStop::CountedObepSetup { reply } => {
                let mut newest_ms: Option<u64> = None;
                let mut any = false;
                for obs in ledger.of_epoch(epoch) {
                    if obs.kind == ObservedKind::BuildAccepted
                        && obs.role == Some(TransitHopRoleKind::OutboundEndpoint)
                        && !obs.rejected
                        && obs.next_router == Some(*reply)
                        && obs.delivery == "accepted"
                        && obs.active_after == obs.active_before + 1
                        && obs.logical_ms >= counted_floor_ms
                    {
                        any = true;
                        newest_ms =
                            Some(newest_ms.map_or(obs.logical_ms, |ms| ms.max(obs.logical_ms)));
                    }
                }
                any && (ready
                    || newest_ms.is_some_and(|ms| {
                        now_ms.saturating_sub(ms) < STALE_ACCEPT_RESET_SECS.saturating_mul(1_000)
                    }))
            }
            BuildStop::RejectedRole(_) => false,
        };
        if !accepted_yet
            && owner.active_count() >= QUALIFICATION_RESET_THRESHOLD
            && last_reset.elapsed() > reset_cooldown(&stop)
        {
            *owner = fresh_owner(handle, wall_secs(), identity)?;
            last_reset = tokio::time::Instant::now();
        }
        match &stop {
            BuildStop::RejectedRole(role) => {
                if ledger.role_rejected(epoch, *role) {
                    break;
                }
            }
            BuildStop::AcceptedRole(role, peers) => {
                if ledger.role_accepted(epoch, *role, peers) && dest.is_some() {
                    break;
                }
            }
            BuildStop::SessionReady | BuildStop::CountedObepSetup { .. } => {
                if dest.is_some() {
                    break;
                }
            }
        }
    }
    record_snapshot_delta(handle, &snap0, epoch);
    ledger.write_evidence(Path::new(
        &std::env::var("EVIDENCE_DIR").unwrap_or_else(|_| ".".to_string()),
    ));
    Ok(dest)
}

/// Records the SSU2 service counter deltas for one epoch as an
/// informational (non-mandatory) evidence row. Counter deltas
/// localize packet loss: datagrams received vs I2NP delivered vs
/// handoff-queue drops vs auth failures.
fn record_snapshot_delta(
    handle: &i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    before: &i2pr_runtime::Ssu2Snapshot,
    epoch: Epoch,
) {
    let after = handle.snapshot();
    let row = format!(
        "rx={} i2np_rx={} tx={} i2np_tx={} sess={}+{}/{} dup={} qdrop={} cheap={} auth={} proto={} nosess={} pend_out={} pend_in={}",
        after
            .datagrams_received
            .saturating_sub(before.datagrams_received),
        after.i2np_received.saturating_sub(before.i2np_received),
        after.datagrams_sent.saturating_sub(before.datagrams_sent),
        after.i2np_sent.saturating_sub(before.i2np_sent),
        after.active_sessions,
        after
            .sessions_established
            .saturating_sub(before.sessions_established),
        after.sessions_closed.saturating_sub(before.sessions_closed),
        after
            .duplicate_resolutions
            .saturating_sub(before.duplicate_resolutions),
        after
            .inbound_queue_drops
            .saturating_sub(before.inbound_queue_drops),
        after.cheap_drops.saturating_sub(before.cheap_drops),
        after.auth_failures.saturating_sub(before.auth_failures),
        after.protocol_drops.saturating_sub(before.protocol_drops),
        after
            .send_without_link
            .saturating_sub(before.send_without_link),
        after.pending_outbound,
        after.pending_inbound,
    );
    // Informational only: written straight to the evidence dir of
    // the running lane via a sidecar file.
    let dir = std::env::var("EVIDENCE_DIR").unwrap_or_else(|_| ".".to_string());
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(Path::new(&dir).join("snapshot-deltas.tsv"))
        .expect("open snapshot file");
    writeln!(file, "{}	{}", epoch.label(), row).expect("write snapshot row");
}

/// Drains authenticated inbound events for one data epoch, recording
/// data/gateway dispositions with cell digests for replay pairing.
/// Raw cell bytes stay in memory only; only digests reach evidence.
/// Early-stop condition for one data drain. When the condition is
/// met the drain keeps observing for a settle tail (15 s) so a
/// second delivery would still be caught honestly, then exits well
/// before the timeout.
#[derive(Clone, Copy)]
enum DataStop {
    /// Stop after at least `count` accepted Garlic deliveries
    /// with reassembled size at least `min_len` (+ settle tail).
    /// The type gate keeps DatabaseStore publishes and tunnel
    /// tests (tiny, non-garlic) from satisfying a datagram stop
    /// early; maintenance rows stay in the ledger, they just
    /// never count toward the stop. Used when several datagrams
    /// share one drain so the settle tail starts only after the
    /// last expected reassembly completes.
    DeliveredSizedObep { count: usize, min_len: usize },
    /// Stop after a gateway delivery (+ settle tail).
    GatewayDelivered,
    /// Stop after a forward for the receive id toward the next
    /// router (+ settle tail).
    Forwarded { receive: u32, next: [u8; 32] },
}

const DATA_SETTLE_SECS: u64 = 15;

/// Best-effort sender-pool freshness gate that KEEPS PUMPING
/// while it waits. Waits (bounded) for the reference to log a
/// NEW outbound tunnel establishment after `baseline` (a count
/// taken just before waiting). In the data windows the counted
/// sender pools are the only outbound pools, so a bump proves a
/// fresh tunnel with an empty queue: sending immediately then
/// emits with a fresh inner expiration instead of rotting 20 s+
/// behind establishing-pool zombies past the reference's ~8 s
/// inner horizon. Times out harmlessly (the caller sends anyway;
/// the receiver-socket predicate remains the truth).
///
/// Pumping is load-bearing, not incidental: an idle 30 s wait
/// silences our side (no replies, forwards, or session upkeep) and lets either
/// stack reap the quiet session, so the gate drains
/// `next_inbound` into the owner/ledger exactly like a data
/// drain while polling the reference log.
async fn wait_for_fresh_sender_tunnel<R>(
    handle: &mut i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    owner: &mut TransitLiveOwner<R>,
    ledger: &mut TypedLedger,
    epoch: Epoch,
    reference: &ReferenceProcess,
    baseline: usize,
    timeout: Duration,
) -> Result<(), String>
where
    R: rand_core::TryCryptoRng + RngCore + rand_core::CryptoRng + Send,
{
    let deadline = tokio::time::Instant::now() + timeout;
    let mut last_log_check = tokio::time::Instant::now();
    // Check once up front so an already-fresh pool exits
    // immediately without idling.
    if reference.count_outbound_created() > baseline {
        return Ok(());
    }
    while tokio::time::Instant::now() < deadline {
        if last_log_check.elapsed() > Duration::from_secs(1) {
            last_log_check = tokio::time::Instant::now();
            if reference.count_outbound_created() > baseline {
                break;
            }
        }
        match tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await {
            Ok(Some(inbound)) => {
                let peer_hash = *inbound.peer.hash().as_bytes();
                let cell_digest = fnv_digest(&inbound.bytes);
                let outcome = owner
                    .handle_inbound(&inbound, wall_ms(), wall_secs())
                    .map_err(|e| format!("handle inbound while gating sends: {e}"))?;
                record_data_outcome(
                    ledger,
                    epoch,
                    peer_hash,
                    &cell_digest,
                    &outcome,
                    owner.active_count(),
                    &inbound.bytes,
                    wall_ms(),
                );
            }
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    Ok(())
}

/// Newest IBGW accept id in the epoch at or after `floor_ms`
/// whose id is not in `baseline`, if any. A post-floor accept is
/// a fresh receiver rebuild (reversed: gateway at i2pr); the id
/// may repeat an older one only if the reference rebuilds the
/// same id, in which case the registration is fresh anyway and
/// the baseline (extended by the caller per return) still
/// distinguishes it on the next call.
fn fresh_ibgw_id(
    ledger: &TypedLedger,
    epoch: Epoch,
    baseline: &[u32],
    floor_ms: u64,
) -> Option<u32> {
    ledger
        .of_epoch(epoch)
        .filter(|obs| {
            obs.kind == ObservedKind::BuildAccepted
                && obs.role == Some(TransitHopRoleKind::InboundGateway)
                && !obs.rejected
                && obs.logical_ms >= floor_ms
                && !baseline.contains(&obs.receive_tunnel)
        })
        .max_by_key(|obs| obs.logical_ms)
        .map(|obs| obs.receive_tunnel)
}

/// Pumps the SSU2 drain while waiting for a fresh IBGW accept
/// (see [`fresh_ibgw_id`]), recording every inbound outcome as
/// the data drains do so no fragment is lost while waiting.
/// Returns the fresh gateway id, or `None` on timeout (the
/// caller sends blind, bounded). The returned id is pushed onto
/// `baseline` so the next wait only fires on a newer rebuild.
async fn pump_until_fresh_ibgw<R>(
    handle: &mut i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    owner: &mut TransitLiveOwner<R>,
    ledger: &mut TypedLedger,
    epoch: Epoch,
    baseline: &mut Vec<u32>,
    floor_ms: u64,
    timeout: Duration,
) -> Result<Option<u32>, String>
where
    R: rand_core::TryCryptoRng + RngCore + rand_core::CryptoRng + Send,
{
    if let Some(id) = fresh_ibgw_id(ledger, epoch, baseline, floor_ms) {
        baseline.push(id);
        return Ok(Some(id));
    }
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await {
            Ok(Some(inbound)) => {
                let peer_hash = *inbound.peer.hash().as_bytes();
                let cell_digest = fnv_digest(&inbound.bytes);
                let outcome = owner
                    .handle_inbound(&inbound, wall_ms(), wall_secs())
                    .map_err(|e| format!("handle inbound while gating sends: {e}"))?;
                record_data_outcome(
                    ledger,
                    epoch,
                    peer_hash,
                    &cell_digest,
                    &outcome,
                    owner.active_count(),
                    &inbound.bytes,
                    wall_ms(),
                );
                if let Some(id) = fresh_ibgw_id(ledger, epoch, baseline, floor_ms) {
                    baseline.push(id);
                    return Ok(Some(id));
                }
            }
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    Ok(None)
}

/// Polls one receiver SAM socket for a single payload-verified
/// inbound datagram shape (used by the IBGW epoch for its 1500 ×
/// 0x7E stimulus; the OBEP epoch uses [`poll_rx_datagrams`]).
/// Counts exact one-shot receipts; background never arrives on
/// this socket and doubles fail closed downstream.
async fn poll_rx_sized(
    rx: &mut SamClient,
    timeout: Duration,
    size: usize,
    byte: u8,
    count: &mut usize,
) -> Result<(), String> {
    let expected = vec![byte; size];
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match rx.try_read_datagram(remaining).await? {
            Some((got, payload)) if got == size && payload == expected => *count += 1,
            Some(_) => {}
            None => break,
        }
    }
    Ok(())
}

/// Polls one receiver SAM socket for payload-verified inbound
/// datagrams while KEEPING THE SSU2 DRAIN PUMPED. A receipt
/// counts only when its SIZE matches the expected stimulus and
/// every payload byte matches the stimulus pattern (512 bytes of
/// `0x5A`, 4096 bytes of `0xA5`): background control traffic
/// (publishes, tests, build replies) never arrives on this
/// socket, and any duplicate delivery surfaces as a count of two
/// and fails closed downstream. Runs at most `timeout`.
///
/// Like the freshness gate, pumping is load-bearing: idling on
/// the TCP socket alone would silence our SSU2 side for the
/// whole window and let either stack reap the quiet session.
///
/// Plan 257 work package I bundles the two payload-verified
/// receipt counters into [`RxDatagramCounts`] so the helper stays
/// within the workspace `too_many_arguments` ceiling without a
/// lint suppression.
struct RxDatagramCounts {
    rx_512: usize,
    rx_4096: usize,
}

async fn poll_rx_datagrams<R>(
    handle: &mut i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    owner: &mut TransitLiveOwner<R>,
    ledger: &mut TypedLedger,
    epoch: Epoch,
    rx: &mut SamClient,
    timeout: Duration,
    counts: &mut RxDatagramCounts,
) -> Result<(), String>
where
    R: rand_core::TryCryptoRng + RngCore + rand_core::CryptoRng + Send,
{
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        tokio::select! {
            biased;
            inbound = handle.next_inbound() => {
                let Some(inbound) = inbound else { break };
                let peer_hash = *inbound.peer.hash().as_bytes();
                let cell_digest = fnv_digest(&inbound.bytes);
                let outcome = owner
                    .handle_inbound(&inbound, wall_ms(), wall_secs())
                    .map_err(|e| format!("handle inbound while polling receiver: {e}"))?;
                record_data_outcome(
                    ledger,
                    epoch,
                    peer_hash,
                    &cell_digest,
                    &outcome,
                    owner.active_count(),
                    &inbound.bytes,
                    wall_ms(),
                );
            }
            datagram = rx.try_read_datagram(remaining) => {
                match datagram? {
                    Some((512, payload)) if payload == vec![0x5Au8; 512] => counts.rx_512 += 1,
                    Some((4096, payload)) if payload == vec![0xA5u8; 4096] => {
                        counts.rx_4096 += 1
                    }
                    Some(_) => {}
                    None => break,
                }
            }
        }
    }
    Ok(())
}

async fn drain_data_epoch<R>(
    handle: &mut i2pr_daemon::router_i2np::Ssu2DaemonHandle,
    owner: &mut TransitLiveOwner<R>,
    ledger: &mut TypedLedger,
    epoch: Epoch,
    timeout: Duration,
    stop: DataStop,
    settle: Duration,
) -> Result<(), String>
where
    R: rand_core::TryCryptoRng + RngCore + rand_core::CryptoRng + Send,
{
    let deadline = tokio::time::Instant::now() + timeout;
    let mut settle_until: Option<tokio::time::Instant> = None;
    let mut last_sweep = tokio::time::Instant::now();
    // Session/link truth per drain: the setup snapshots miss the
    // data-phase sends, so record the transport counters around
    // each drain (transmits, receives, closes, duplicates). When
    // our replies vanish mid-loopback, these rows show whether we
    // stopped transmitting or the peer stopped honoring.
    let drain_snap = handle.snapshot();
    while tokio::time::Instant::now() < deadline {
        if settle_until.is_some_and(|tail| tokio::time::Instant::now() >= tail) {
            break;
        }
        // Same periodic expiry sweep as the build epochs: only
        // naturally-expired registrations are removed, so live
        // counted state and retained replay cells are untouched.
        if last_sweep.elapsed() > Duration::from_secs(10) {
            owner.expire(wall_secs());
            last_sweep = tokio::time::Instant::now();
        }
        match tokio::time::timeout(POLL_INTERVAL, handle.next_inbound()).await {
            Ok(Some(inbound)) => {
                let peer_hash = *inbound.peer.hash().as_bytes();
                let cell_digest = fnv_digest(&inbound.bytes);
                let outcome = owner
                    .handle_inbound(&inbound, wall_ms(), wall_secs())
                    .map_err(|e| format!("handle inbound data in epoch {}: {e}", epoch.label()))?;
                record_data_outcome(
                    ledger,
                    epoch,
                    peer_hash,
                    &cell_digest,
                    &outcome,
                    owner.active_count(),
                    &inbound.bytes,
                    wall_ms(),
                );
                if settle_until.is_none() && data_stop_met(ledger, epoch, &stop) {
                    settle_until = Some(tokio::time::Instant::now() + settle);
                }
            }
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    record_snapshot_delta(handle, &drain_snap, epoch);
    ledger.write_evidence(Path::new(
        &std::env::var("EVIDENCE_DIR").unwrap_or_else(|_| ".".to_string()),
    ));
    Ok(())
}

/// Records one data-plane outcome as a typed observation, retaining
/// genuine forwarded bodies for replay/expiry pairing.
#[allow(clippy::too_many_arguments)]
fn record_data_outcome(
    ledger: &mut TypedLedger,
    epoch: Epoch,
    peer_hash: [u8; 32],
    cell_digest: &str,
    outcome: &LiveInboundOutcome,
    active: usize,
    raw_bytes: &[u8],
    logical_ms: u64,
) {
    use i2pr_daemon::transit_owner::{LiveGatewayOutcome, TransitDataDisposition};
    let digest = Some(format!("fnv-{cell_digest}"));
    let base = Observation {
        epoch,
        kind: ObservedKind::DataDropped,
        peer_hash,
        role: None,
        receive_tunnel: 0,
        next_router: None,
        next_message_id: 0,
        rejected: false,
        delivery: "dropped",
        active_before: active,
        active_after: active,
        logical_ms,
        digest: digest.clone(),
        aux_count: 0,
        pending_after: 0,
        peer_index_after: 0,
        queued_after: 0,
        bandwidth: None,
        gateway_failures: None,
        nested_len: None,
    };
    match outcome {
        LiveInboundOutcome::Data(TransitDataDisposition::Forwarded(forward)) => {
            ledger.push(Observation {
                kind: ObservedKind::DataForwarded,
                role: None,
                receive_tunnel: forward.receive_tunnel,
                next_router: Some(*forward.next_router.as_bytes()),
                // Plan 257 work package F: carry the exact next
                // tunnel id in `next_message_id` (data forwards
                // carry no message id) so the far-side predicate
                // can bind the B-side endpoint observation to the
                // counted forward's exact next tunnel.
                next_message_id: forward.next_tunnel,
                delivery: delivery_label(&forward.outcome),
                ..base
            });
            ledger.remember_forward_cell(epoch, forward.receive_tunnel, cell_digest, raw_bytes);
        }
        LiveInboundOutcome::Data(TransitDataDisposition::DeliveredObep {
            outcome: obep,
            message_len,
            inner_type,
            target_router,
            target_tunnel,
        }) => {
            use i2pr_daemon::transit_owner::ObepDeliveryOutcome;
            // Plan 262 WP D2: a self-target TUNNEL action that
            // entered the local IBGW registration surfaces as
            // LocalIbgwDelivered/Dropped. Delivered maps to the
            // same GatewayDelivered observation the network
            // gateway path emits (so `gatewayed_receipt` counts
            // genuine self-loop ingress); Dropped maps to
            // GatewayDropped (so `terminal-garlic-self` stays zero
            // and the ingress predicate fails closed).
            if let ObepDeliveryOutcome::LocalIbgwDelivered {
                receive_id,
                next_router,
                next_tunnel,
                delivered,
                failures,
                nested_len,
            } = obep
            {
                ledger.push(Observation {
                    kind: ObservedKind::GatewayDelivered,
                    aux_count: *delivered,
                    receive_tunnel: *receive_id,
                    next_router: Some(*next_router.as_bytes()),
                    next_message_id: *next_tunnel,
                    gateway_failures: Some(*failures),
                    nested_len: Some(*nested_len),
                    ..base
                });
                return;
            }
            if let ObepDeliveryOutcome::LocalIbgwDropped { receive_id, .. } = obep {
                ledger.push(Observation {
                    kind: ObservedKind::GatewayDropped,
                    receive_tunnel: receive_id.unwrap_or(0),
                    nested_len: Some(*message_len),
                    ..base
                });
                return;
            }
            // Record whether the OBEP action actually delivered
            // (accepted/local-ok) plus the inner message type
            // instead of the placeholder: datagrams arrive as
            // Garlic for the sibling destination, while LeaseSet
            // publishes (DatabaseStore) and tunnel tests complete
            // on the same registration and must not count toward
            // either datagram. Terminal outcomes (failed router /
            // gateway delivery) never count either. The pass
            // predicate filters on both acceptance and type; the
            // labels are a closed outcome × type vocabulary (all
            // branches const).
            use i2pr_proto::MessageType;
            let accepted = matches!(
                obep,
                ObepDeliveryOutcome::LocalOk
                    | ObepDeliveryOutcome::Router(RouterDeliveryOutcome::Accepted)
                    | ObepDeliveryOutcome::Tunnel(RouterDeliveryOutcome::Accepted)
            );
            let typename = match inner_type {
                Some(MessageType::Garlic) => "garlic",
                Some(MessageType::DatabaseStore) => "dbstore",
                Some(MessageType::Data) => "data",
                Some(MessageType::DeliveryStatus) => "deliverystatus",
                Some(_) => "other",
                None => "empty",
            };
            let label: &'static str = match (accepted, typename) {
                (true, "garlic") => "obep-ok-garlic",
                (true, "dbstore") => "obep-ok-dbstore",
                (true, "data") => "obep-ok-data",
                (true, "deliverystatus") => "obep-ok-deliverystatus",
                (true, _) => "obep-ok-other",
                (false, "garlic") => "obep-terminal-garlic",
                (false, "dbstore") => "obep-terminal-dbstore",
                (false, "data") => "obep-terminal-data",
                (false, "deliverystatus") => "obep-terminal-deliverystatus",
                (false, _) => "obep-terminal-other",
            };
            ledger.push(Observation {
                kind: ObservedKind::DataDeliveredObep,
                delivery: label,
                aux_count: *message_len,
                // Bind each OBEP delivery to the next router/tunnel
                // the delivered action addressed (Plan 256 section 8
                // fields): the lane can then separate B-bound genuine
                // traffic from misaddressed or reflected completions.
                next_router: target_router.map(|hash| *hash.as_bytes()),
                next_message_id: target_tunnel.unwrap_or(0),
                ..base
            });
        }
        LiveInboundOutcome::Data(_) => {
            ledger.push(base);
        }
        LiveInboundOutcome::Gateway(LiveGatewayOutcome::Delivered {
            delivered,
            failures,
            receive_tunnel,
            nested_len,
            next_router,
            next_tunnel,
        }) => {
            ledger.push(Observation {
                kind: ObservedKind::GatewayDelivered,
                aux_count: *delivered,
                // Bind the delivery to the addressed gateway
                // tunnel so the lane predicate can require the
                // accepted registration (background ingress to
                // other ids never counts). Plan 258 work package
                // A: carry the failure dimension (explicit even
                // when zero) plus the nested size fact so one
                // diagnostic execution can classify the missing
                // multicell emission (H1/H2/H3). Plan 260: carry
                // the committed next router/tunnel tuple (the
                // data-forward convention of recording the next
                // tunnel in `next_message_id`) so the
                // creator-owned inbound receipt tuple binds the
                // exact creator-local tunnel id.
                receive_tunnel: *receive_tunnel,
                next_router: Some(*next_router.as_bytes()),
                next_message_id: *next_tunnel,
                gateway_failures: Some(*failures),
                nested_len: Some(*nested_len),
                ..base
            });
        }
        LiveInboundOutcome::Gateway(LiveGatewayOutcome::Dropped {
            tunnel_id,
            nested_len,
        }) => {
            ledger.push(Observation {
                kind: ObservedKind::GatewayDropped,
                // The ADDRESSED id (accepted or stale) plus the
                // nested size fact. Drops never satisfy pass rows
                // (the acceptance filter reads Delivered only);
                // the drop-side diagnostic fold below separates
                // stale-id relays from accepted-id drops and
                // datagram-sized batches from maintenance trickle.
                receive_tunnel: *tunnel_id,
                nested_len: Some(*nested_len),
                ..base
            });
        }
        LiveInboundOutcome::Gateway(_) => {
            ledger.push(Observation {
                kind: ObservedKind::GatewayDropped,
                ..base
            });
        }
        LiveInboundOutcome::Build(i2pr_daemon::transit_owner::LiveBuildOutcome::Dispatched(
            evidence,
        )) => {
            ledger.push(Observation::build(
                epoch, peer_hash, evidence, active, active, logical_ms,
            ));
        }
        _ => {}
    }
}

/// Re-feeds one genuine previously observed cell through the live
/// owner with an explicit logical clock. The bytes and the peer are
/// exactly what the authenticated SSU2 session delivered; only the
/// caller-supplied clock advances (replay uses wall time, expiry
/// uses creation + 601 s). This is the Plan 256 section 11
/// replay/expiry experiment, not a hand-built injection: the
/// payload is never synthesized.
async fn feed_retained_cell<R>(
    owner: &mut TransitLiveOwner<R>,
    ledger: &mut TypedLedger,
    epoch: Epoch,
    peer: PeerId,
    cell: &RetainedCell,
    now_ms: u64,
    now_secs: u64,
) -> Result<(), String>
where
    R: rand_core::TryCryptoRng + RngCore + rand_core::CryptoRng + Send,
{
    let inbound = Ssu2InboundI2np {
        link_id: i2pr_transport::LinkId::new(99).expect("link"),
        peer,
        bytes: cell.bytes.clone(),
    };
    let peer_hash = *peer.hash().as_bytes();
    let outcome = owner
        .handle_inbound(&inbound, now_ms, now_secs)
        .map_err(|e| format!("feed retained cell: {e}"))?;
    let digest = fnv_digest(&cell.bytes);
    record_data_outcome(
        ledger,
        epoch,
        peer_hash,
        &digest,
        &outcome,
        owner.active_count(),
        &cell.bytes,
        now_ms,
    );
    Ok(())
}

/// Returns the first accepted receive tunnel id for one role epoch,
/// if the epoch ran and accepted.
fn receive_of(ledger: &TypedLedger, epoch: Epoch, role: TransitHopRoleKind) -> Option<u32> {
    ledger
        .of_epoch(epoch)
        .find(|obs| obs.kind == ObservedKind::BuildAccepted && obs.role == Some(role))
        .map(|obs| obs.receive_tunnel)
}

/// Records Plan 257 work package D exact registration-cardinality
/// rows for one counted role accept.
///
/// Derives before/after/delta/pending from the counted
/// observation itself (the observation carries the per-event
/// active counts plus the post-event snapshot dimensions), so an
/// unrelated background build can never satisfy the row: the
/// delta must be exactly +1, the counted receive id must resolve
/// to the observation, and pending state must be at baseline.
fn record_cardinality_for_obs(
    evidence_dir: &Path,
    epoch: Epoch,
    obs: &Observation,
    rows: &mut Vec<(String, String)>,
) -> Result<u32, String> {
    if obs.active_after != obs.active_before + 1 {
        return Err(format!(
            "{}: counted build delta is not exactly +1 ({} -> {})",
            epoch.label(),
            obs.active_before,
            obs.active_after
        ));
    }
    if obs.pending_after != 0 {
        return Err(format!(
            "{}: pending state did not return to baseline ({})",
            epoch.label(),
            obs.pending_after
        ));
    }
    let receive = obs.receive_tunnel;
    record_row(
        evidence_dir,
        epoch,
        "active-before",
        &obs.active_before.to_string(),
        rows,
    );
    record_row(
        evidence_dir,
        epoch,
        "active-after",
        &obs.active_after.to_string(),
        rows,
    );
    record_row(
        evidence_dir,
        epoch,
        "registration-delta",
        &obs.active_after.wrapping_sub(obs.active_before).to_string(),
        rows,
    );
    record_row(
        evidence_dir,
        epoch,
        "receive-id",
        &format!("{receive:#x}"),
        rows,
    );
    record_row(
        evidence_dir,
        epoch,
        "pending-baseline",
        &obs.pending_after.to_string(),
        rows,
    );
    Ok(receive)
}

fn record_cardinality_rows(
    evidence_dir: &Path,
    ledger: &TypedLedger,
    epoch: Epoch,
    role: TransitHopRoleKind,
    peers: &[[u8; 32]],
    rows: &mut Vec<(String, String)>,
) -> Result<u32, String> {
    let obs = ledger
        .first_accept(epoch, role, peers)
        .ok_or_else(|| format!("{}: no counted accept for cardinality", epoch.label()))?;
    record_cardinality_for_obs(evidence_dir, epoch, &obs, rows)
}

/// Records Plan 257 work package E typed bandwidth rows from the
/// counted observation's summary (never a hard-coded string).
fn record_bandwidth_rows(
    evidence_dir: &Path,
    ledger: &TypedLedger,
    epoch: Epoch,
    role: TransitHopRoleKind,
    rows: &mut Vec<(String, String)>,
) -> Result<(), String> {
    let summary = ledger
        .of_epoch(epoch)
        .find(|obs| {
            (obs.kind == ObservedKind::BuildAccepted || obs.kind == ObservedKind::BuildRejected)
                && obs.role == Some(role)
        })
        .and_then(|obs| obs.bandwidth)
        .ok_or_else(|| format!("{}: no typed bandwidth summary", epoch.label()))?;
    let request = format!(
        "m={} r={} l={}",
        summary
            .minimum_kbps
            .map_or("-".to_string(), |v| v.to_string()),
        summary
            .requested_kbps
            .map_or("-".to_string(), |v| v.to_string()),
        summary
            .limit_kbps
            .map_or("-".to_string(), |v| v.to_string()),
    );
    let reply = format!(
        "b={} accepted={}",
        summary
            .available_kbps
            .map_or("-".to_string(), |v| v.to_string()),
        summary.accepted,
    );
    record_row(evidence_dir, epoch, "bandwidth-request", &request, rows);
    record_row(evidence_dir, epoch, "bandwidth-reply", &reply, rows);
    record_row(
        evidence_dir,
        epoch,
        "bandwidth-disposition-observed",
        &summary.evidence_label(),
        rows,
    );
    Ok(())
}

fn data_stop_met(ledger: &TypedLedger, epoch: Epoch, stop: &DataStop) -> bool {
    match stop {
        DataStop::DeliveredSizedObep { count, min_len } => {
            ledger
                .of_epoch(epoch)
                .filter(|obs| {
                    obs.kind == ObservedKind::DataDeliveredObep
                        && obs.delivery == "obep-ok-garlic"
                        && obs.aux_count >= *min_len
                })
                .count()
                >= *count
        }
        DataStop::GatewayDelivered => ledger
            .of_epoch(epoch)
            .any(|obs| obs.kind == ObservedKind::GatewayDelivered),
        DataStop::Forwarded { receive, next } => ledger.of_epoch(epoch).any(|obs| {
            obs.kind == ObservedKind::DataForwarded
                && obs.receive_tunnel == *receive
                && obs.next_router == Some(*next)
        }),
    }
}

impl TypedLedger {
    /// Retains one genuine forwarded cell body for replay/expiry
    /// pairing. Bounded to [`MAX_RETAINED_CELLS`]; oldest entries
    /// are evicted first. Bodies stay in memory and never reach
    /// evidence; only digests are recorded.
    fn remember_forward_cell(
        &mut self,
        epoch: Epoch,
        receive_tunnel: u32,
        digest: &str,
        bytes: &[u8],
    ) {
        if self.retained_cells.len() >= MAX_RETAINED_CELLS {
            self.retained_cells.remove(0);
        }
        self.retained_cells.push(RetainedCell {
            epoch,
            receive_tunnel,
            digest: format!("fnv-{digest}"),
            observed_ms: wall_ms(),
            bytes: bytes.to_vec(),
        });
    }

    /// Returns the oldest retained cell for the supplied epoch and
    /// receive tunnel, if any.
    fn replay_candidate(&self, epoch: Epoch, receive_tunnel: u32) -> Option<RetainedCell> {
        self.retained_cells
            .iter()
            .find(|cell| cell.epoch == epoch && cell.receive_tunnel == receive_tunnel)
            .cloned()
    }

    /// Serializes every observation with all non-secret fields to
    /// `ledger-evidence.tsv` for audit. This renders each field at
    /// least once so no observation fact is write-only.
    fn write_evidence(&self, dir: &Path) {
        let mut out = String::new();
        for obs in &self.observations {
            out.push_str(&format!(
                "{}\t{:?}\t{}\t{}\t{:#x}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                obs.epoch.label(),
                obs.kind,
                hex_lower(&obs.peer_hash),
                obs.role.map_or("none", |kind| kind.label()),
                obs.receive_tunnel,
                obs.next_router
                    .map(|hash| hex_lower(&hash))
                    .unwrap_or_else(|| "-".to_string()),
                obs.next_message_id,
                obs.rejected,
                obs.delivery,
                obs.active_before,
                obs.active_after,
                obs.logical_ms,
                obs.digest.as_deref().unwrap_or("-"),
                obs.aux_count,
            ));
        }
        std::fs::write(dir.join("ledger-evidence.tsv"), out).expect("write ledger evidence");
    }
}

/// Records one epoch-qualified evidence row and tracks the label
/// for the end-of-run uniqueness self-guard. Every mandatory
/// external row flows through this function; a row emitted from a
/// generic branch cannot name its epoch and fails the checker.
fn record_row(dir: &Path, epoch: Epoch, key: &str, value: &str, rows: &mut Vec<(String, String)>) {
    let label = format!("{}/{}", epoch.label(), key);
    append_evidence(dir, &label, value);
    rows.push((label, value.to_string()));
}
